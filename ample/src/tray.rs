use crossbeam::channel::Sender;
use log::error;
use sys_media::MediaInfo;
use tray_item::{TIError, TrayItem};

const SONG_WAITING_MSG: &str = "Waiting for Discord connection. Make sure Discord is running!";
const SONG_INACTIVE_MSG: &str = "Currently Playing: Nothing";
const DISCORD_CONNECTION_MSG: &str = "Discord Status: Connected";
const DISCORD_DISCONN_MSG: &str = "Discord Status: Disconnected";

pub struct AmpleTray {
    tray_item: Option<TrayItem>,
    song_status_id: u32,
    discord_status_id: u32,
}

pub enum TrayDiscordStatus {
    Connected,
    Disconnected,
}

pub enum TraySongStatus<'a> {
    Playing(&'a MediaInfo),
    WaitingForDiscord,
    NotPlaying,
}

fn create_tray() -> Result<TrayItem, TIError> {
    let mut tray = TrayItem::new("Ample", tray_item::IconSource::Resource("ample_icon"))?;

    tray.inner_mut().set_tooltip("Ample")?;

    Ok(tray)
}

fn add_labels(tray: &mut TrayItem, exit_channel: Sender<bool>) -> Result<(u32, u32), TIError> {
    let song_status_id = tray.inner_mut().add_label_with_id(SONG_INACTIVE_MSG)?;
    let discord_status_id = tray.inner_mut().add_label_with_id(DISCORD_DISCONN_MSG)?;
    tray.add_menu_item("Stop", move || {
        if let Err(err) = exit_channel.send(true) {
            error!("Failed to close program: {err}")
        }
    })?;

    Ok((song_status_id, discord_status_id))
}

fn convert_to_tray_string(original: &str) -> String {
    original.replace("&", "and")
}

// Class designed to abstract some Tray manipulation stuff and also optional values.
// I don't really know if it is advisable to hide optional values like this but it makes
// the main loop code easier to read. Also it's not the main loops responsibility to care about
// a faulty or missing tray item
//
// TODO: Currently, no errors are shown in the clear, update, and set_not_running functions
// if we failed to create the TrayItem in the first place, it only shows up new. It might be nice
// to log to the user that we're trying to update the TrayItem but it doesn't exist.
impl AmpleTray {
    pub fn new(exit_channel: Sender<bool>) -> AmpleTray {
        let mut tray = create_tray();
        let mut ids: (u32, u32) = (0, 0);
        match tray.as_mut() {
            Err(err) => {
                error!("Error while trying to create tray item: {err}");
            }
            Ok(tray) => ids = add_labels(tray, exit_channel).unwrap_or((0, 0)),
        }

        AmpleTray {
            tray_item: tray.ok(),
            song_status_id: ids.0,
            discord_status_id: ids.1,
        }
    }

    pub fn update_song(&mut self, status: TraySongStatus) -> Result<(), TIError> {
        if let Some(tray) = self.tray_item.as_mut() {
            match status {
                TraySongStatus::Playing(media_info) => tray.inner_mut().set_label(
                    &format!(
                        "Currently listening to {} by {}",
                        convert_to_tray_string(&media_info.song_name),
                        convert_to_tray_string(&media_info.artist_name)
                    ),
                    self.song_status_id,
                ),
                TraySongStatus::WaitingForDiscord => tray.inner_mut().set_label(SONG_WAITING_MSG, self.song_status_id),
                TraySongStatus::NotPlaying => tray.inner_mut().set_label(SONG_INACTIVE_MSG, self.song_status_id),
            }
        } else {
            Ok(())
        }
    }

    pub fn update_discord_status(&mut self, status: TrayDiscordStatus) -> Result<(), TIError> {
        if let Some(tray) = self.tray_item.as_mut() {
            match status {
                TrayDiscordStatus::Connected => tray.inner_mut().set_label(DISCORD_CONNECTION_MSG, self.discord_status_id),
                TrayDiscordStatus::Disconnected => tray.inner_mut().set_label(DISCORD_DISCONN_MSG, self.discord_status_id),
            }
        } else {
            Ok(())
        }
    }
}
