use std::{thread, time::Duration};

use log::*;
use simplelog::*;
use sys_media::*;
use discord_rich_presence::{activity::Timestamps, *};

const AMPLE_DPRC_ID: u64 = 1399214780564246670;
const TICK_TIME: Duration = Duration::from_secs(5);

fn main() {
    SimpleLogger::init(LevelFilter::Debug, Config::default()).unwrap();

    let mut client = DiscordIpcClient::new(&format!("{AMPLE_DPRC_ID}")).unwrap();

    client.connect().unwrap();

    loop {
        let currently_playing = sys_media::get_current_playing_info();

        match currently_playing {
            Err(error) => {
                error!("Error while trying to get currently playing song: {error}")
            }
            Ok(Some(media_info)) => {
                client.set_activity(
                    activity::Activity::new()
                        .details(&media_info.song_name)
                        .state(&format!("{} - {}", media_info.artist_name, media_info.album_name))
                        .activity_type(activity::ActivityType::Listening)
                )
                .unwrap();
            }
            _ => {}
        }



        thread::sleep(TICK_TIME);
    }
}
