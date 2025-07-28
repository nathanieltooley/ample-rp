use core::fmt;


mod win_media;
pub mod consts;


#[derive(Debug, Clone)]
pub struct MediaInfo {
    pub player_name: String,
    pub artist_name: String,
    pub song_name: String,
    pub album_name: String,
    pub status: MediaStatus,
    pub media_type: MediaType,
    pub end_time: i64,
    pub current_position: i64,
}

impl PartialEq for MediaInfo {
   fn eq(&self, other: &Self) -> bool {
       self.album_name == other.album_name 
       && self.artist_name == other.artist_name 
       && self.song_name == other.song_name
       && self.player_name == other.player_name
   } 
}

#[derive(Debug, Clone)]
pub enum MediaStatus {
    Closed,
    Opened,
    Changing,
    Stopped,
    Playing,
    Paused
}

#[derive(Debug, Clone)]
pub enum MediaType {
    Unknown,
    Music,
    Video,
    Image,
}

#[derive(Debug)]
pub enum MediaError {
    Windows(windows::core::Error)
}

impl From<windows::core::Error> for MediaError {
    fn from(value: windows::core::Error) -> Self {
        MediaError::Windows(value)
    }
}

impl fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaError::Windows(error) => write!(f, "An error occuring while trying to get currently playing media: {error}") 
        }
    }
}

/// Get the currently playing song's info including what app started playing it.
/// Blocks on windows async calls.
pub fn get_current_playing_info() -> Result<Option<MediaInfo>, MediaError> {
    if cfg!(windows) {
        let session = win_media::get_current_session()?;
        win_media::get_current_session_info(session).map_err(|err| err.into())
    } else {
        todo!()
    }
}

// pub fn get_all_sessions_info() -> Result<Vec<MediaInfo>, MediaError> {
//     if cfg!(windows) {

//     }
//     let media_controller = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?;
//     let media_controller = media_controller.get()?;

//     let sessions = media_controller.GetSessions()?;

//     let session_infos = sessions.into_iter().map(|session| {
//         let player = session.SourceAppUserModelId()?;
//         let media_props = session.TryGetMediaPropertiesAsync()?.get()?;

//         MediaInfo {
//             player_name: player.to_string_lossy(),
//             artist_name: media_props.Artist()?.to_string_lossy(),
//             song_name: media_props.Title()?.to_string_lossy(),
//             album_name: media_props.AlbumTitle()?.to_string_lossy()
//         }
//     }).collect();

//     Ok(session_infos)
// }

pub fn add(left: usize, right: usize) -> usize {
    left + right
}
