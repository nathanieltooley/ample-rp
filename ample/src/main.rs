#![cfg_attr(feature = "headless", windows_subsystem = "windows")]
mod config;
mod http;
mod lastfm;
mod logging;
mod musicbrainz;
mod secrets;
mod tray;
mod uri;

use std::{
    error::Error,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crossbeam::{
    channel::{Receiver, RecvError, Sender},
    select,
};
use discord_rich_presence::{
    activity::{Assets, Timestamps},
    *,
};
use log::*;
use sys_media::{MediaInfo, MediaStatus};
use ureq::{Agent, config::Config};

use crate::{
    lastfm::{LastFm, LastFmCreds},
    musicbrainz::Musicbrainz,
    tray::{AmpleTray, TrayDiscordStatus, TraySongStatus},
};

const AMPLE_DPRC_ID: u64 = 1399214780564246670;
const TICK_TIME: Duration = Duration::from_secs(5);
const APP_NAME: &str = "ample";

fn main() {
    if let Err(err) = dotenvy::dotenv() {
        // none of this information actually gets logged because the logging init has to happen after this.
        // tbh not sure what to do about that
        if err.not_found() {
            println!("No .env file found. Skipping...")
        } else {
            println!("{err}");
            return;
        }
    }

    let debug = std::env::var("AMPLE_DEBUG").is_ok_and(|debug_var| debug_var == "true");

    let log_level = if debug { LevelFilter::Debug } else { LevelFilter::Info };

    logging::init_log(log_level).unwrap();

    std::panic::set_hook(Box::new(|panic_info| {
        // SAFETY: Payloads should only be strs
        let panic_msg = unsafe { panic_info.payload_as_str().unwrap_unchecked() };
        error!("panic: {panic_msg}");
    }));

    let http_agent = Agent::new_with_config(
        Config::builder()
            .http_status_as_error(false)
            .user_agent(format!("ample/{} {{nathanieltooley24@gmail.com}}", env!("CARGO_PKG_VERSION")))
            .build(),
    );

    debug!("basic init done");

    let (exit_tx, exit_rx) = crossbeam::channel::bounded::<bool>(1);

    let mut tray = AmpleTray::new(exit_tx);

    debug!("loading config file");
    let config = config::load_config();

    info!("Loading Discord IPC client");
    // TODO: it looks like connecting RP to Discord is heavily rate limited (can maybe only connect once every like 30 seconds or so).
    // maybe look into this
    let mut discord_client = AmpleDiscordClient::init();

    // main loop state
    let mut previously_played: Option<MediaInfo> = None;
    let mut previously_played_started: Option<SystemTime> = None;
    let mut current_has_been_scrobbled = false;
    let mut previously_paused = false;

    debug!("getting media listener");
    let media_listener = sys_media::get_listener().unwrap();

    let mut current_song_img = String::new();
    let (blocking_msg_tx, blocking_msg_rx) = crossbeam::channel::bounded::<BlockingThreadMessage>(1);
    let (song_img_tx, song_img_rx) = crossbeam::channel::bounded::<String>(1);

    let pool = Arc::new(threadpool::ThreadPool::new(4));
    info!("Attempting to load LastFM credentials");
    let last_fm = get_lastfm_creds(&http_agent);

    // ---- second thread setup ----
    let blocking_handler = match last_fm {
        Some(ref last_fm) => NetworkThreadHandler::new(WebApi::LastFM { last_fm: last_fm.clone() }, song_img_tx),
        None => NetworkThreadHandler::new(
            WebApi::Musicbrainz {
                mb: Musicbrainz::new(http_agent.clone()),
            },
            song_img_tx,
        ),
    };

    let blocking_handler = Arc::new(blocking_handler);

    let rc_pool = Arc::clone(&pool);
    debug!("Started LastFM loop");
    pool.execute(move || {
        loop {
            let result = blocking_msg_rx.recv();
            let blocking_handler_clone = Arc::clone(&blocking_handler);

            debug!("blocking thread received message");

            match result {
                // and this too?
                Ok(msg) => {
                    rc_pool.execute(move || blocking_handler_clone.handle_blocking_thread_msg(msg));
                }
                Err(err) => {
                    error!("Error trying to read from channel: {err}");
                    return;
                }
            }
        }
    });
    debug!("Started blocking thread");
    // ------------------------------

    // Main thread loop
    info!("Main loop started. Listening for changes");
    loop {
        // check to see if we need to retry our connection to discord
        if discord_client.should_retry() {
            if config.wait_for_discord {
                info!("Attempting to connect to Discord. Will block execution until connection is reestablished.");
                // either block indefinitely until we connect to discord
                if let Err(err) = tray.update_discord_status(TrayDiscordStatus::Disconnected) {
                    error!("Failed to update system tray: {err}");
                }

                if let Err(err) = tray.update_song(TraySongStatus::WaitingForDiscord) {
                    error!("Failed to update system tray: {err}")
                }

                match discord_client.retry_blocking(exit_rx.clone()) {
                    Ok(completed) => {
                        if !completed {
                            debug!("Exiting!");
                            break;
                        }
                    }
                    Err(recv_error) => {
                        error!("Failed to read from exit channel while blocking Discord reconnect: {recv_error}");
                    }
                }
            } else {
                // or just try again next time
                if let Err(err) = tray.update_discord_status(TrayDiscordStatus::Disconnected) {
                    error!("Failed to update system tray: {err}");
                }

                discord_client.retry();
            }
        } else if let Err(err) = tray.update_discord_status(TrayDiscordStatus::Connected) {
            error!("Failed to update system tray: {err}")
        }

        select! {
            recv(exit_rx) -> msg => {
                if msg.unwrap() {
                    info!("Manually stopping program...");
                    break;
                }
            }
            // Instantly update status cover img when we get it from LastFM
            recv(song_img_rx) -> msg => {
                match msg {
                    Ok(cover_url) => {
                        match discord_client.update_status(
                            previously_played.as_ref().expect("Cover update should only happen after a song has started to play"), &cover_url)
                        {
                            Ok(()) => info!("Status img updated to: {cover_url}"),
                            Err(err) => error!("Error trying to update status: {err}")
                        }
                        current_song_img = cover_url.clone();
                    },
                    Err(err) => {
                        error!("Error trying to receive from LastFM thread: {err}");
                        return;
                    }
                }
            },
            // Otherwise continue checking currently playing song
            default(TICK_TIME) => {
                let currently_playing = media_listener.get_current_playing_info();
                // let currently_playing: Result<Option<MediaInfo>, MediaError> = Ok(Some(MediaInfo{
                //     album_name: "Test".to_owned(),
                //     player_name: APPLE_MUSIC_ID.to_owned(),
                //     artist_name: "Test".to_owned(),
                //     current_position: 0,
                //     end_time: 1000000,
                //     song_name: "Test Song".to_owned(),
                //     status: MediaStatus::Playing,
                //     media_type: sys_media::MediaType::Music

                // }));

                debug!("{currently_playing:#?}");

                match currently_playing {
                    // nothing playing or a failure to get info
                    Err(media_error) => {
                        if media_error.is_false_error() {
                            debug!("No media is paused or playing!");
                            if let Err(err) = discord_client.clear_status() {
                                error!("Failed to clear discord status: {err}");
                                info!("Will reset Discord connection next tick");

                                discord_client.mark_for_retry();
                            }

                            if let Err(err) = tray.update_song(TraySongStatus::NotPlaying) {
                                error!("Failed to clear tray status: {err}")
                            }

                        } else {
                            error!("{media_error}")
                        }
                    }
                    // something playing
                    Ok(Some(media_info)) => {
                        if media_info.status == MediaStatus::Playing && config.is_valid_media_source(&media_info.player_name)
                        {
                            previously_paused = false;
                            // New song
                            if previously_played.as_ref() != Some(&media_info) {
                                info!("App currently playing media: {}", media_info.player_name);
                                info!(
                                    "Currently Playing: {} by {} on {}",
                                    media_info.song_name, media_info.artist_name, media_info.album_name
                                );

                                current_has_been_scrobbled = false;
                                previously_played_started = Some(SystemTime::now());
                                previously_played = None;

                                // tell lastfm we are listening to the current song
                                if last_fm.is_some() {
                                    let send_err = blocking_msg_tx.send(BlockingThreadMessage::NowPlaying(media_info.clone()));
                                    if let Err(err) = send_err {
                                        error!("Cannot send to Blocking thread: {err}");
                                    }
                                }

                                // get album cover
                                current_song_img = String::new();
                                let send_err = blocking_msg_tx.send(BlockingThreadMessage::AlbumImg(media_info.clone()));
                                if let Err(err) = send_err {
                                    error!("Cannot send to Blocking thread: {err}");
                                }
                            } else if last_fm.is_some() {
                                // Try to scrobble current song if we have the creds
                                let song_len = Duration::from_micros(media_info.end_time as u64);
                                let duration = Duration::from_micros(media_info.current_position as u64);

                                let song_len_secs = song_len.as_secs();

                                // Per LastFM, scrobbles should only happen for songs longer than 30 secs and
                                // when the user has listened to atleast half of the song
                                if song_len_secs > 30 && duration.as_secs() > song_len_secs / 2 && !current_has_been_scrobbled {
                                    let timestamp = previously_played_started.unwrap_or_else(SystemTime::now);
                                    match blocking_msg_tx.send(BlockingThreadMessage::Scrobble(media_info.clone(), timestamp)) {
                                        Ok(()) => current_has_been_scrobbled = true,
                                        Err(err) => error!("Cannot send to LastFM thread: {err}"),
                                    }
                                }
                            }

                            if let Err(err) = discord_client.update_status(&media_info, &current_song_img) {
                                error!("Error while setting activity: {err}");
                                info!("Will reset Discord connection next tick");

                                discord_client.mark_for_retry();
                            } else if previously_played.is_none() {
                                // this log is guarded to make sure it only logs the first time the loop
                                // updates the discord activity
                                info!("Activity set to listening to {} - {}", media_info.song_name, media_info.artist_name);
                            }

                            if let Err(err) = tray.update_song(TraySongStatus::Playing(&media_info)) {
                                error!("failed to update tray status: {err}");
                            }

                            previously_played = Some(media_info);
                        } else if !previously_paused {
                            // clear everything while paused
                            debug!("Media is paused. Clearing activity");
                            if let Err(err) = discord_client.clear_status() {
                                error!("Error while clearing activity: {err}");
                                info!("Will reset Discord connection next tick");
                            }

                            if let Err(err) = tray.update_song(TraySongStatus::NotPlaying) {
                                error!("failed to update tray status: {err}")
                            }

                            previously_paused = true;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

enum BlockingThreadMessage {
    Scrobble(MediaInfo, SystemTime),
    NowPlaying(MediaInfo),
    AlbumImg(MediaInfo),
}

enum WebApi {
    LastFM { last_fm: LastFm },
    Musicbrainz { mb: Musicbrainz },
}
/// Wrapper over network api calls so that either LastFM or musicbrainz can be used as needed
struct NetworkThreadHandler {
    web_api: WebApi,
    song_img_tx: Sender<String>,
}

impl NetworkThreadHandler {
    fn new(web_api: WebApi, song_img_tx: Sender<String>) -> Self {
        NetworkThreadHandler { web_api, song_img_tx }
    }

    fn handle_blocking_thread_msg(&self, msg: BlockingThreadMessage) {
        match self.web_api {
            WebApi::LastFM { ref last_fm } => match msg {
                BlockingThreadMessage::NowPlaying(info) => {
                    match last_fm
                        .clone()
                        .now_playing(info.artist_name.as_str(), info.song_name.as_str(), Some(&info.album_name))
                    {
                        Err(err) => error!("{err}"),
                        Ok(_) => info!("LastFM Now Playing: {} - {}", info.song_name, info.artist_name),
                    }
                }
                BlockingThreadMessage::AlbumImg(info) => {
                    let lf_track_info = last_fm.get_track_info(&info.artist_name, &info.song_name);
                    match lf_track_info {
                        Ok(track) => {
                            debug!("Got track info from LastFM: {track:?}");
                            if let Some(album) = track.album {
                                let song_img = album
                                    .images
                                    .iter()
                                    .find(|info| info.size == "large")
                                    .map(|info| info.url.clone())
                                    .unwrap_or_default();

                                if !song_img.is_empty()
                                    && let Err(r_err) = self.song_img_tx.send(song_img)
                                {
                                    error!("{r_err}");
                                }
                            }
                        }
                        Err(err) => {
                            error!("{err}")
                        }
                    }
                }
                BlockingThreadMessage::Scrobble(info, timestamp) => {
                    match last_fm.scrobble(&info.artist_name, &info.song_name, timestamp, Some(&info.album_name)) {
                        Ok(()) => {
                            info!("Song, {} by {} has been scrobbled!", info.song_name, info.artist_name);
                        }
                        Err(err) => error!("Failed to scrobble current track: {err}"),
                    }
                }
            },
            WebApi::Musicbrainz { ref mb } => {
                if let BlockingThreadMessage::AlbumImg(info) = msg {
                    match mb.get_release_mbid(&info) {
                        Ok(mbid) => match mb.get_cover_url(&mbid) {
                            Ok(Some(url)) => {
                                if let Err(err) = self.song_img_tx.send(url) {
                                    error!("failed to send mb album cover url: {err}")
                                }
                            }
                            Ok(None) => {
                                error!("no cover art exists for album: {} by {}", info.album_name, info.artist_name)
                            }
                            Err(err) => {
                                error!("could not get cover art: {err}")
                            }
                        },
                        Err(err) => {
                            error!("Could not get musicbrainz mbid: {err}")
                        }
                    }
                }
            }
        }
    }
}

struct AmpleDiscordClient {
    inner: Option<DiscordIpcClient>,
    /// True if a previous retry attempt had failed previously.
    /// Used to make sure the logs aren't filled with errors about failing to connect to Discord.
    previously_retried: bool,
}

impl AmpleDiscordClient {
    fn init() -> Self {
        let client_result = Self::get_client();
        let mut retried = false;
        if let Err(err) = client_result {
            error!("Failed to connect client to Discord: {err}");
            retried = true;
        }
        // Result<()> -> Result<client>
        AmpleDiscordClient {
            inner: Self::get_client().ok(),
            previously_retried: retried,
        }
    }

    fn get_client() -> Result<DiscordIpcClient, Box<dyn Error + 'static>> {
        let mut client = DiscordIpcClient::new(&format!("{AMPLE_DPRC_ID}")).unwrap();
        let connect_result = client.connect();

        // Result<()> -> Result<client>
        connect_result.map(|_| client)
    }

    // Will attempt to get the client once. Errors are logged but success is not guaranteed.
    fn retry(&mut self) {
        let inner = Self::get_client();
        match inner {
            Ok(client) => {
                self.inner = Some(client);
                self.previously_retried = false;
            }
            Err(err) => {
                if !self.previously_retried {
                    error!("Failed to connect client to Discord: {err}");
                    self.previously_retried = true;
                }

                self.inner = None;
            }
        }
    }

    /// Will attempt to indefinitely retry its connection to Discord. Returns whether it completed the connection.
    fn retry_blocking(&mut self, exit_rx: Receiver<bool>) -> Result<bool, RecvError> {
        let mut client = DiscordIpcClient::new(&format!("{AMPLE_DPRC_ID}")).unwrap();
        let mut error_logged = false;
        loop {
            select! {
                // if the read fails we want to stop too
                recv(exit_rx) -> exit => if exit? {
                    return Ok(false);
                },
                default(Duration::from_secs(10)) => {
                    match client.connect() {
                        Ok(()) => break,
                        Err(err) => {
                            if !error_logged {
                                error_logged = true;
                                error!("Failed to connect to Discord: {err}. Make sure it is running!");
                            }
                        }
                    }
                }
            }
        }

        self.inner = Some(client);
        Ok(true)
    }

    fn should_retry(&self) -> bool {
        self.inner.is_none()
    }

    /// Used to mark this client as needing to retry its connection to Discord.
    /// Usually occurs when some error occurs during status setting.
    fn mark_for_retry(&mut self) {
        self.inner = None;
        self.previously_retried = false;
    }

    fn clear_status(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(client) = self.inner.as_mut() {
            return client.clear_activity();
        }

        Ok(())
    }

    fn update_status(&mut self, media_info: &MediaInfo, cover_url: &str) -> Result<(), Box<dyn Error>> {
        if let Some(client) = self.inner.as_mut() {
            let now = SystemTime::now();
            let dur = now.duration_since(UNIX_EPOCH).expect("epoch should hopefully always be in the past");

            let start_dur = dur.saturating_sub(Duration::from_micros(media_info.current_position as u64));
            let remaining_time = media_info.end_time - media_info.current_position;
            let end_dur = dur.saturating_add(Duration::from_micros(remaining_time as u64));

            let state_name = format!("{} - {}", media_info.artist_name, media_info.album_name);

            let mut activity = activity::Activity::new()
                // TODO: This function fails silently to set the activity when the song title, and thus details, is one of two things:
                // - Too short
                // - Starts with a number
                // I tried to get this to work with the song 7 by the Catfish and the Bottlemen. Thus I don't
                // know if it fails because of the 7 or because its only 1 character. Need to test this out.
                .details(&media_info.song_name)
                .state(&state_name)
                .activity_type(activity::ActivityType::Listening)
                .timestamps(Timestamps::new().start(start_dur.as_secs() as i64).end(end_dur.as_secs() as i64));

            if !cover_url.is_empty() {
                activity = activity.assets(Assets::new().large_image(cover_url))
            }

            debug!("setting status");

            return client.set_activity(activity);
        }

        Ok(())
    }
}

fn get_lastfm_creds(client: &Agent) -> Option<LastFm> {
    let cred_result = LastFmCreds::get_creds(client.clone());

    match cred_result {
        Ok(creds) => {
            info!("Got LastFM credentials");
            Some(lastfm::LastFm::new(client.clone(), creds))
        }
        Err(err) => {
            error!("LastFM support not enabled: {err}");
            None
        }
    }
}
