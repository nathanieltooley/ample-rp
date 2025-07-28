use std::{thread::{self, yield_now}, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};

use log::*;
use simplelog::*;
use discord_rich_presence::{activity::Timestamps, *};
use sys_media::{MediaInfo, MediaStatus};

const AMPLE_DPRC_ID: u64 = 1399214780564246670;
const TICK_TIME: Duration = Duration::from_secs(5);

fn main() {
    SimpleLogger::init(LevelFilter::Debug, Config::default()).unwrap();

    let only_am = true;

    let mut client = DiscordIpcClient::new(&format!("{AMPLE_DPRC_ID}")).unwrap();
    let mut previously_played: Option<MediaInfo> = None;

    client.connect().unwrap();

    loop {
        let currently_playing = sys_media::get_current_playing_info();

        match currently_playing {
            Err(error) => {
                error!("Error while trying to get currently playing song: {error}")
            }
            Ok(Some(media_info)) => {
                let valid_player = !only_am || media_info.player_name == sys_media::consts::APPLE_MUSIC_ID;
                if let MediaStatus::Playing = media_info.status && valid_player {
                    if previously_played.as_ref() != Some(&media_info) {
                        info!("App currently playing media: {}", media_info.player_name);
                        info!("Currently Playing: {} by {} on {}", media_info.song_name, media_info.artist_name, media_info.album_name);
                    }

                    let now = SystemTime::now();
                    let dur = now.duration_since(UNIX_EPOCH).expect("epoch should hopefully always be in the future");

                    let start_dur = dur.saturating_sub(Duration::from_micros(media_info.current_position as u64));
                    let remaining_time = media_info.end_time - media_info.current_position;
                    let end_dur = dur.saturating_add(Duration::from_micros(remaining_time as u64));

                    let state_name =format!("{} - {}", media_info.artist_name, media_info.album_name); 

                    let activity = 
                        activity::Activity::new()
                            .details(&media_info.song_name)
                            .state(&state_name)
                            .activity_type(activity::ActivityType::Listening)
                            .timestamps(Timestamps::new().start(start_dur.as_secs() as i64).end(end_dur.as_secs() as i64));
                    
                    if let Err(error) = client.set_activity(activity) {
                        error!("Error while setting activity: {error}");
                    }

                    previously_played = Some(media_info);
                } else {
                    debug!("Media is paused. Clearing activity");
                    if let Err(error) = client.clear_activity() {
                        error!("Error while clearing activity: {error}");
                    }
                }

            }
            _ => {}
        }



        thread::sleep(TICK_TIME);
    }
}
