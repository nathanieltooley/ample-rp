use std::{
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use discord_rich_presence::{activity::Timestamps, *};
use log::*;
use simplelog::*;
use sys_media::{MediaInfo, MediaStatus};

const AMPLE_DPRC_ID: u64 = 1399214780564246670;
const TICK_TIME: Duration = Duration::from_secs(5);

fn main() {
    SimpleLogger::init(LevelFilter::Debug, Config::default()).unwrap();

    #[cfg(feature = "win_service")]
    {
        if let Err(err) = service::run() {
            error!("Error trying to start service: {err}");
        }
        return;
    }

    #[allow(unreachable_code)]
    #[allow(unused_variables)]
    {
        let mut listener = MediaListener::new(true);

        loop {
            let currently_playing = sys_media::get_current_playing_info();

            match currently_playing {
                Err(error) => {
                    error!("Error while trying to get currently playing song: {error}")
                }
                Ok(Some(media_info)) => {
                    listener.update_status(media_info);
                }
                _ => {}
            }

            thread::sleep(TICK_TIME);
        }
    }
}

struct MediaListener {
    only_am: bool,
    client: DiscordIpcClient,
    previously_played: Option<MediaInfo>,
}

impl MediaListener {
    pub fn new(only_am: bool) -> MediaListener {
        MediaListener {
            only_am,
            client: get_client(),
            previously_played: None,
        }
    }

    pub fn update_status(&mut self, media_info: MediaInfo) {
        let valid_player =
            !self.only_am || media_info.player_name == sys_media::consts::APPLE_MUSIC_ID;
        if let MediaStatus::Playing = media_info.status
            && valid_player
        {
            if self.previously_played.as_ref() != Some(&media_info) {
                info!("App currently playing media: {}", media_info.player_name);
                info!(
                    "Currently Playing: {} by {} on {}",
                    media_info.song_name, media_info.artist_name, media_info.album_name
                );
            }

            let now = SystemTime::now();
            let dur = now
                .duration_since(UNIX_EPOCH)
                .expect("epoch should hopefully always be in the future");

            let start_dur =
                dur.saturating_sub(Duration::from_micros(media_info.current_position as u64));
            let remaining_time = media_info.end_time - media_info.current_position;
            let end_dur = dur.saturating_add(Duration::from_micros(remaining_time as u64));

            let state_name = format!("{} - {}", media_info.artist_name, media_info.album_name);

            let activity = activity::Activity::new()
                .details(&media_info.song_name)
                .state(&state_name)
                .activity_type(activity::ActivityType::Listening)
                .timestamps(
                    Timestamps::new()
                        .start(start_dur.as_secs() as i64)
                        .end(end_dur.as_secs() as i64),
                );

            if let Err(error) = self.client.set_activity(activity) {
                error!("Error while setting activity: {error}");
            }

            self.previously_played = Some(media_info);
        } else {
            debug!("Media is paused. Clearing activity");
            if let Err(error) = self.client.clear_activity() {
                error!("Error while clearing activity: {error}");
            }
        }
    }
}

fn get_client() -> DiscordIpcClient {
    let mut client = DiscordIpcClient::new(&format!("{AMPLE_DPRC_ID}")).unwrap();
    // NOTE: Panics because really this entire app can't function without it.
    // In the future, I'll probably make the error output a bit nicer but still
    client.connect().unwrap();

    client
}

#[cfg(feature = "win_service")]
pub mod service {
    use crate::*;

    use std::ffi::OsString;
    use std::sync::mpsc::RecvTimeoutError;
    use std::sync::mpsc;

    use windows_service::service::{
        ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
    };
    use windows_service::{define_windows_service, service_control_handler, service_dispatcher};

    define_windows_service!(ffi_service_main, service_main);

    enum ThreadMessage {
        Pause,
        Continue,
        Stop,
    }

    pub fn run() -> windows_service::Result<()> {
        service_dispatcher::start("ample", ffi_service_main)
    }

    fn service_loop(rx: mpsc::Receiver<ThreadMessage>) {
        let mut listener = MediaListener::new(true);
        let mut paused = false;

        loop {
            if paused {
                // block until we receive since we need to wait for a Continue message anyway
                match rx.recv() {
                    Err(_) => {
                        error!("Event handler channel disconnected!");
                        return;
                    }
                    Ok(msg) => match msg {
                        ThreadMessage::Continue => paused = false,
                        ThreadMessage::Stop => return,
                        ThreadMessage::Pause => unreachable!(),
                    },
                }
            } else {
                match rx.recv_timeout(TICK_TIME) {
                    Ok(msg) => match msg {
                        ThreadMessage::Continue => unreachable!(),
                        ThreadMessage::Stop => return,
                        ThreadMessage::Pause => {
                            paused = true;
                            continue;
                        }
                    },
                    Err(err) => {
                        match err {
                            // Timeout here is the happy path
                            RecvTimeoutError::Timeout => {}
                            RecvTimeoutError::Disconnected => {
                                error!("Event handler channel disconnected!");
                                return;
                            }
                        }
                    }
                }
            }

            let currently_playing = sys_media::get_current_playing_info();

            match currently_playing {
                Err(error) => {
                    error!("Error while trying to get currently playing song: {error}")
                }
                Ok(Some(media_info)) => {
                    listener.update_status(media_info);
                }
                _ => {}
            }
        }
    }

    fn service_main(_args: Vec<OsString>) {
        use windows_service::{
            service::ServiceControl, service_control_handler::ServiceControlHandlerResult,
        };

        let (ev_tx, ev_rx) = mpsc::channel();

        let event_handler = move |ctl_event: ServiceControl| -> ServiceControlHandlerResult {
            match ctl_event {
                ServiceControl::Stop => {
                    ev_tx.send(ThreadMessage::Stop).unwrap();
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Continue => {
                    ev_tx.send(ThreadMessage::Continue).unwrap();
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Pause => {
                    ev_tx.send(ThreadMessage::Pause).unwrap();
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };

        let service_handle = match service_control_handler::register("ample", event_handler) {
            Ok(handle) => handle,
            Err(err) => {
                error!("Error trying to setup up service control handler: {err}");
                return;
            }
        };

        if let Err(err) = service_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::PAUSE_CONTINUE,
            exit_code: ServiceExitCode::Win32(0),
            wait_hint: Duration::default(),
            process_id: None,
            checkpoint: 0,
        }) {
            error!("Error trying to set service status: {err}");
            return;
        }

        service_loop(ev_rx);

        if let Err(err) = service_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        }) {
            error!("Error trying to stop service: {err}")
        }
    }
}
