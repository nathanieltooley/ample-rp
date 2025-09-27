#![cfg_attr(feature = "headless", windows_subsystem = "windows")]
mod lastfm;
mod uri;

use std::{
    env::{self, VarError},
    error::Error,
    fs::{self, File},
    io::{self, Write},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crossbeam::select;
use discord_rich_presence::{
    activity::{Assets, Timestamps},
    *,
};
use log::*;
use simplelog::*;
use sys_media::{MediaInfo, MediaStatus};
use tray_item::{TIError, TrayItem};
use ureq::{Agent, config::Config};

use crate::lastfm::{CredsError, LastFm, LastFmCreds};

const AMPLE_DPRC_ID: u64 = 1399214780564246670;
const TICK_TIME: Duration = Duration::from_secs(5);
const APP_NAME: &str = "ample";
const SECRET_ENTRY_NAME: &str = "ampleSecret";
const PASSWORD_ENTRY_NAME: &str = "amplePassword";
const SESSION_ENTRY_NAME: &str = "ampleSession";

// #[cfg(feature = "win_service")]
// fn main() {
//     if let Err(err) = service::run() {
//         panic!("Error trying to start service: {err}");
//     }
// }

fn main() {
    if let Err(err) = dotenvy::dotenv() {
        if err.not_found() {
            info!("No .env file found. Skipping...")
        } else {
            error!("{err}");
            return;
        }
    }

    let debug = match std::env::var("AMPLE_DEBUG") {
        Ok(debug_var) => debug_var == "true",
        Err(err) => match err {
            VarError::NotPresent => false,
            _ => panic!("{err}"),
        },
    };

    let log_level = if debug { LevelFilter::Debug } else { LevelFilter::Info };
    let log_file = open_log_file().unwrap();

    init_log(log_level, log_file);

    debug!("inited");

    let args = env::args();
    let mut password_flag = false;
    let mut secret_flag = false;

    // simple arg parsing
    for arg in args.into_iter().skip(1) {
        if arg == "--password" || arg == "-p" {
            password_flag = true;
        } else if arg == "--secret" || arg == "-s" {
            secret_flag = true;
        } else {
            warn!("Unknown argument: {arg}")
        }
    }

    if password_flag {
        let input = prompted_input("Password: ");
        let password_entry = match keyring::Entry::new_with_target(PASSWORD_ENTRY_NAME, APP_NAME, APP_NAME) {
            Err(err) => {
                error!("{err}");
                return;
            }
            Ok(entry) => entry,
        };
        match password_entry.set_password(&input) {
            Err(err) => {
                error!("Could not set password!: {err}");
                return;
            }
            Ok(()) => info!("Password has been set!"),
        }
    }

    if secret_flag {
        let input = prompted_input("API Secret: ");
        let secret_entry = match keyring::Entry::new_with_target(SECRET_ENTRY_NAME, APP_NAME, APP_NAME) {
            Err(err) => {
                error!("{err}");
                return;
            }
            Ok(entry) => entry,
        };
        match secret_entry.set_password(&input) {
            Err(err) => {
                error!("Failed to set secret!: {err}");
                return;
            }
            Ok(()) => info!("Secret has been set!"),
        }
    }

    if password_flag || secret_flag {
        return;
    }

    let only_am = true;
    let mut client = get_client();
    let mut previously_played: Option<MediaInfo> = None;
    let mut previously_played_started: Option<SystemTime> = None;
    let mut current_has_been_scrobbled = false;

    let tray_result = create_tray_icon();
    if let Err(ref err) = tray_result {
        error!("Error while trying to create tray icon: {err}");
    }

    let mut tray = tray_result.ok();

    let mut current_song_img = String::new();
    let (last_fm_tx, last_fm_rx) = crossbeam::channel::unbounded::<LastFmThreadMessage>();
    let (song_img_tx, song_img_rx) = crossbeam::channel::unbounded::<String>();

    let last_fm = get_lastfm_creds();
    if let Some(ref l) = last_fm {
        let inner_last_fm = l.clone();
        thread::spawn(move || {
            loop {
                let result = last_fm_rx.recv();
                match result {
                    Ok(msg) => match msg {
                        LastFmThreadMessage::NowPlaying(info) => {
                            match inner_last_fm.now_playing(&info.artist_name, &info.song_name, Some(&info.album_name)) {
                                Err(err) => error!("{err}"),
                                Ok(_) => info!("LastFM Now Playing: {} - {}", info.song_name, info.artist_name),
                            }
                        }
                        LastFmThreadMessage::AlbumImg(info) => {
                            let lf_track_info = inner_last_fm.get_track_info(&info.artist_name, &info.song_name);
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

                                        if !song_img.is_empty() {
                                            if let Err(r_err) = song_img_tx.send(song_img) {
                                                error!("{r_err}");
                                                return;
                                            }
                                        }
                                    }
                                }
                                Err(err) => {
                                    error!("{err}")
                                }
                            }
                        }
                        LastFmThreadMessage::Scrobble(info, timestamp) => {
                            match inner_last_fm.scrobble(&info.artist_name, &info.song_name, timestamp, Some(&info.album_name)) {
                                Ok(()) => {
                                    info!("Song, {} by {} has been scrobbled!", info.song_name, info.artist_name);
                                }
                                Err(err) => error!("Failed to scrobble current track: {err}"),
                            }
                        }
                    },
                    Err(err) => {
                        error!("Error trying to read from channel: {err}");
                        return;
                    }
                }
            }
        });
    }

    loop {
        select! {
            // Instantly update status cover img when we get it from LastFM
            recv(song_img_rx) -> msg => {
                match msg {
                    Ok(cover_url) => {
                        match update_status(&mut client, previously_played.as_ref().expect("Cover update should only happen after a song has started to play"), &cover_url) {
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
                let currently_playing = sys_media::get_current_playing_info();

                match currently_playing {
                    Err(error) => {
                        if error.is_false_error() {
                            info!("No media is paused or playing!");
                        } else {
                            error!("{error}")
                        }
                    }
                    Ok(Some(media_info)) => {
                        let valid_player = !only_am || media_info.player_name == sys_media::consts::APPLE_MUSIC_ID;
                        if let MediaStatus::Playing = media_info.status
                            && valid_player
                        {
                            // New song
                            if previously_played.as_ref() != Some(&media_info) {
                                info!("App currently playing media: {}", media_info.player_name);
                                info!(
                                    "Currently Playing: {} by {} on {}",
                                    media_info.song_name, media_info.artist_name, media_info.album_name
                                );

                                current_has_been_scrobbled = false;
                                previously_played_started = Some(SystemTime::now());

                                // try to get info from LastFM if we have the creds
                                if last_fm.is_some() {
                                    let send_err = last_fm_tx.send(LastFmThreadMessage::NowPlaying(media_info.clone()));
                                    if let Err(err) = send_err {
                                        error!("Cannot send to LastFM thread: {err}");
                                    }

                                    let send_err = last_fm_tx.send(LastFmThreadMessage::AlbumImg(media_info.clone()));
                                    if let Err(err) = send_err {
                                        error!("Cannot send to LastFM thread: {err}");
                                    }
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
                                    match last_fm_tx.send(LastFmThreadMessage::Scrobble(media_info.clone(), timestamp)) {
                                        Ok(()) => current_has_been_scrobbled = true,
                                        Err(err) => error!("Cannot send to LastFM thread: {err}"),
                                    }
                                }
                            }

                            if let Err(error) = update_status(&mut client, &media_info, &current_song_img) {
                                error!("Error while setting activity: {error}");
                            } else if previously_played.is_none() {
                                info!("Activity set to listening to {} - {}", media_info.song_name, media_info.artist_name);
                                if let Some(ref mut tray) = tray {
                                    if let Err(err) = tray.0.inner_mut().set_label(&format!("Currently listening to {} by {}", media_info.song_name, media_info.artist_name), tray.1) {
                                        error!("Failed to set tray label: {err}")
                                    }
                                }
                            }

                            previously_played = Some(media_info);
                        } else {
                            debug!("Media is paused. Clearing activity");
                            clear_status(&mut client);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

enum LastFmThreadMessage {
    Scrobble(MediaInfo, SystemTime),
    NowPlaying(MediaInfo),
    AlbumImg(MediaInfo),
}

fn update_status(client: &mut DiscordIpcClient, media_info: &MediaInfo, cover_url: &str) -> Result<(), Box<dyn Error>> {
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

    client.set_activity(activity)
}

fn clear_status(client: &mut DiscordIpcClient) {
    if let Err(err) = client.clear_activity() {
        error!("Error while clearing activity: {err}");
    }
}

fn prompted_input(prompt: &str) -> String {
    io::stdout().write_all(prompt.as_bytes()).expect("Could not write to stdout");
    io::stdout().flush().expect("can't flush :(");

    let mut input = String::new();
    io::stdin().read_line(&mut input).expect("Failed to read stdin!");
    input.trim().to_owned()
}

fn retry_creds(client: Agent, attempts: usize) -> Result<LastFmCreds, CredsError> {
    let mut creds = None;
    for _ in 0..attempts {
        match lastfm::LastFmCreds::get_creds(client.clone()) {
            Ok(ok_creds) => {
                creds = Some(ok_creds);
                break;
            }
            Err(err) => {
                debug!("{err:?}");
                if let CredsError::RetryableError(_, _) = err {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                } else {
                    return Err(err);
                }
            }
        }
    }

    creds.ok_or(CredsError::RetryableError(
        -1,
        format!("Failed to connect to LastFM after {attempts} attempts"),
    ))
}

fn get_lastfm_creds() -> Option<LastFm> {
    let client = Agent::new_with_config(Config::builder().http_status_as_error(false).build());
    let retry_attempts = 10;
    let cred_attempt = retry_creds(client.clone(), retry_attempts);

    match cred_attempt {
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

fn open_log_file() -> io::Result<File> {
    // Should create something like "/AppData/ample/config/logs" on windows
    // and "~/.config/ample/logs" on linux
    let log_dir = directories::ProjectDirs::from("", "", APP_NAME)
        .expect("valid project dir")
        .config_dir()
        .join("logs");

    fs::create_dir_all(&log_dir)?;

    // TODO: Append to end of file, not truncate file
    File::create(log_dir.join("ample.log"))
}

fn init_log(log_level: LevelFilter, log_file: File) {
    // only possible error is initting twice
    let _ = CombinedLogger::init(vec![
        TermLogger::new(
            log_level,
            ConfigBuilder::new()
                .set_location_level(LevelFilter::Debug)
                .set_level_color(Level::Error, Some(Color::Red))
                .build(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(log_level, ConfigBuilder::new().set_location_level(LevelFilter::Debug).build(), log_file),
    ]);
}

fn get_client() -> DiscordIpcClient {
    let mut client = DiscordIpcClient::new(&format!("{AMPLE_DPRC_ID}")).unwrap();
    // NOTE: Panics because really this entire app can't function without it.
    // In the future, I'll probably make the error output a bit nicer but still
    client.connect().unwrap();

    client
}

fn create_tray_icon() -> Result<(TrayItem, u32), TIError> {
    let mut tray = TrayItem::new("Ample", tray_item::IconSource::Resource("ample_icon"))?;
    tray.inner_mut().set_tooltip("Ample");
    let id = tray.inner_mut().add_label_with_id("Currently Listening to: Nothing :(")?;
    Ok((tray, id))
}

// debugging and logging with services is basically impossible
// so im not bothering with this anymore
// #[cfg(feature = "win_service")]
// pub mod service {
//     use crate::*;

//     use std::ffi::OsString;
//     use std::sync::mpsc;
//     use std::sync::mpsc::RecvTimeoutError;

//     use windows_service::service::{ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType};
//     use windows_service::{define_windows_service, service_control_handler, service_dispatcher};

//     define_windows_service!(ffi_service_main, service_main);

//     enum ThreadMessage {
//         Pause,
//         Continue,
//         Stop,
//     }

//     pub fn run() -> windows_service::Result<()> {
//         eventlog::register("ample").unwrap();
//         eventlog::init("ample", Level::Info).unwrap();

//         service_dispatcher::start(APP_NAME, ffi_service_main)
//     }

//     fn service_loop(rx: mpsc::Receiver<ThreadMessage>) {
//         let mut listener = MediaListener::new(true, get_lastfm_creds());
//         let mut paused = false;

//         loop {
//             if paused {
//                 listener.clear_status();

//                 // block until we receive since we need to wait for a Continue message anyway
//                 match rx.recv() {
//                     Err(_) => {
//                         error!("Event handler channel disconnected!");
//                         return;
//                     }
//                     Ok(msg) => match msg {
//                         ThreadMessage::Continue => paused = false,
//                         ThreadMessage::Stop => return,
//                         ThreadMessage::Pause => unreachable!(),
//                     },
//                 }
//             } else {
//                 match rx.recv_timeout(TICK_TIME) {
//                     Ok(msg) => match msg {
//                         ThreadMessage::Continue => unreachable!(),
//                         ThreadMessage::Stop => return,
//                         ThreadMessage::Pause => {
//                             paused = true;
//                             continue;
//                         }
//                     },
//                     Err(err) => {
//                         match err {
//                             // Timeout here is the happy path
//                             RecvTimeoutError::Timeout => {}
//                             RecvTimeoutError::Disconnected => {
//                                 error!("Event handler channel disconnected!");
//                                 return;
//                             }
//                         }
//                     }
//                 }
//             }

//             let currently_playing = sys_media::get_current_playing_info();

//             match currently_playing {
//                 Err(error) => {
//                     error!("Error while trying to get currently playing song: {error}");
//                 }
//                 Ok(Some(media_info)) => {
//                     listener.update_status(media_info);
//                 }
//                 _ => {}
//             }
//         }
//     }

//     fn service_main(_args: Vec<OsString>) {
//         use windows_service::{service::ServiceControl, service_control_handler::ServiceControlHandlerResult};

//         info!("starting ample service");

//         let (ev_tx, ev_rx) = mpsc::channel();

//         let event_handler = move |ctl_event: ServiceControl| -> ServiceControlHandlerResult {
//             match ctl_event {
//                 ServiceControl::Stop => {
//                     ev_tx.send(ThreadMessage::Stop).unwrap();
//                     ServiceControlHandlerResult::NoError
//                 }
//                 ServiceControl::Continue => {
//                     ev_tx.send(ThreadMessage::Continue).unwrap();
//                     ServiceControlHandlerResult::NoError
//                 }
//                 ServiceControl::Pause => {
//                     ev_tx.send(ThreadMessage::Pause).unwrap();
//                     ServiceControlHandlerResult::NoError
//                 }
//                 ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
//                 _ => ServiceControlHandlerResult::NotImplemented,
//             }
//         };

//         let service_handle = match service_control_handler::register(APP_NAME, event_handler) {
//             Ok(handle) => handle,
//             Err(err) => {
//                 error!("Error trying to setup up service control handler: {err}");
//                 return;
//             }
//         };

//         if let Err(err) = service_handle.set_service_status(ServiceStatus {
//             service_type: ServiceType::OWN_PROCESS,
//             current_state: ServiceState::Running,
//             controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::PAUSE_CONTINUE,
//             exit_code: ServiceExitCode::Win32(0),
//             wait_hint: Duration::default(),
//             process_id: None,
//             checkpoint: 0,
//         }) {
//             error!("Error trying to set service status: {err}");
//             return;
//         }

//         service_loop(ev_rx);

//         if let Err(err) = service_handle.set_service_status(ServiceStatus {
//             service_type: ServiceType::OWN_PROCESS,
//             current_state: ServiceState::Stopped,
//             controls_accepted: ServiceControlAccept::empty(),
//             exit_code: ServiceExitCode::Win32(0),
//             checkpoint: 0,
//             wait_hint: Duration::default(),
//             process_id: None,
//         }) {
//             error!("Error trying to stop service: {err}")
//         }
//     }
// }
