use core::fmt;

use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

pub mod consts;
mod win_media;

/// An object containing info about whatever is currently playing. This info is set by
/// external programs and thus may be formatted differently from each other and some info may be absent.
#[derive(Debug, Clone)]
pub struct MediaInfo {
    /// Name of the app or executable that started playing this media
    pub player_name: String,
    pub artist_name: String,
    pub song_name: String,
    pub album_name: String,
    pub status: MediaStatus,
    pub media_type: MediaType,
    /// Length of media in microseconds
    // This would be in milliseconds but for some reason Windows has it in
    // a length of time that doesn't have an actual name (it's like 10x smaller than a microsecond),
    // but the last few digits aren't significant anyway
    pub end_time: i64,
    /// Amount of time having watched / listened to media in microseconds
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaStatus {
    Closed,
    Opened,
    Changing,
    Stopped,
    Playing,
    Paused,
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
    Windows(windows::core::Error),
}

impl MediaError {
    /// When getting info from Windows about what is currently playing, the windows-rs API returns
    /// and error if nothing is playing. This is not an error. So this function makes sure its a real error.
    pub fn is_false_error(&self) -> bool {
        // this should eventually be refutable when other variants are added
        #[allow(irrefutable_let_patterns)]
        if let MediaError::Windows(win_err) = self {
            // NOTE: rust-analyzer thinks this is an error for some reason?
            win_err.code() == windows_result::HRESULT(0)
        } else {
            false
        }
    }
}

impl From<windows::core::Error> for MediaError {
    fn from(value: windows::core::Error) -> Self {
        MediaError::Windows(value)
    }
}

impl fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaError::Windows(error) => write!(f, "An error occurred while trying to get currently playing media: {error}"),
        }
    }
}

/// An object capable of getting information about the currently playing media (Music, Video, etc.).
pub enum MediaListener {
    Windows {
        session_manager: GlobalSystemMediaTransportControlsSessionManager,
    },
}

impl MediaListener {
    /// Get the currently playing song's info including what app started playing it.
    /// Blocks execution if waiting on async or syscalls.
    pub fn get_current_playing_info(&self) -> Result<Option<MediaInfo>, MediaError> {
        match self {
            MediaListener::Windows { session_manager } => {
                let session = win_media::get_current_session(session_manager)?;
                win_media::get_current_session_info(&session).map_err(|err| err.into())
            }
        }
    }
}

/// Creates a MediaListener for the given OS
pub fn get_listener() -> Result<MediaListener, MediaError> {
    if cfg!(windows) {
        let session_manager = win_media::get_session_manager()?;
        Ok(MediaListener::Windows { session_manager })
    } else {
        // BETTER IDEA: https://crates.io/crates/mpris
        todo!()
    }
}
