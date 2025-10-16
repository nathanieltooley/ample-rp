use core::fmt;

use windows::Media::Control::GlobalSystemMediaTransportControlsSession;

pub mod consts;
mod win_media;

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

#[derive(Debug, Clone)]
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
    Windows { session: GlobalSystemMediaTransportControlsSession },
}

impl MediaListener {
    /// Get the currently playing song's info including what app started playing it.
    /// Blocks execution if waiting on async or syscalls.
    pub fn get_current_playing_info(&self) -> Result<Option<MediaInfo>, MediaError> {
        match self {
            MediaListener::Windows { session } => win_media::get_current_session_info(session).map_err(|err| err.into()),
        }
    }
}

/// Creates a MediaListener for the given OS
pub fn get_listener() -> Result<MediaListener, MediaError> {
    if cfg!(windows) {
        let session = win_media::get_current_session()?;
        Ok(MediaListener::Windows { session })
    } else {
        // Possible ways I've found to get info on linux:
        // - playerctl
        // This could be done the "dirty" way by using processes and piping that info inside the library.
        //
        // The other option is using the playerctl "library" but this seems more complicated than just a libplayerctl sort of thing.
        // It also looks like to use the playerctl "library," we'd have run Glib's EventLoop and listen for events? Which would require
        // a more complicated API or possibly an explicit separation of functions. Basically, windows would have a function and linux would need an
        // init or start function and then a normal function? Maybe have the function agnostic but init on windows is a no-op?
        // https://github.com/altdesktop/playerctl/tree/master
        // For library route: https://gtk-rs.org/
        //
        // In either case this does introduce a dependency on playerctl which is outside of Rust. I'm not exactly sure how to depend explicitly
        // on a system binary.
        //
        // - from scratch?
        // If there is a nice way to "ask" the OS about info from the current media player, we might be able to sidestep any gtk / GLib stuff.
        // However, I fear this is actually not simple to do.
        todo!()
    }
}
