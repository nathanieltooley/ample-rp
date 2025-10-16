use ::windows::Media::Control::{GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager};

use crate::{consts::APPLE_MUSIC_ID, MediaInfo, MediaStatus, MediaType};

/// Gets a "SessionManager" from the Windows API.
///
/// This function leaks memory due to a bug in the Windows API itself.
/// See here for more info: https://github.com/microsoft/windows-rs/issues/2061
///
/// Limit the amount of times this function is called; preferably only once.
/// This function blocks until the manager is received.
pub fn get_session_manager() -> windows_result::Result<GlobalSystemMediaTransportControlsSessionManager> {
    let media_controller = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?;
    media_controller.get()
}

/// Gets the current "Session" from the SessionManager.
/// This will usually get the session that is currently active (i.e. playing music) at the point of the function call.
pub fn get_current_session(
    session_manager: &GlobalSystemMediaTransportControlsSessionManager,
) -> windows_result::Result<GlobalSystemMediaTransportControlsSession> {
    session_manager.GetCurrentSession()
}

/// Gets the relevant info about the currently active media from a session.
pub fn get_current_session_info(session: &GlobalSystemMediaTransportControlsSession) -> windows_result::Result<Option<MediaInfo>> {
    let player = session.SourceAppUserModelId()?;
    let media_props = session.TryGetMediaPropertiesAsync()?.get()?;

    let status: MediaStatus = get_raw_status_code(session)?.into();
    let m_type: MediaType = get_raw_media_type(session)?.into();

    let mut artist_name = media_props.Artist()?.to_string_lossy();
    let mut album_name = media_props.AlbumTitle()?.to_string_lossy();

    // Apple Music combines the Artist and Album names together with a dash,
    // however this dash is not a normal '-', its actually '—', which I didn't know was a different character.
    // Neat.
    if player.to_string_lossy() == APPLE_MUSIC_ID {
        let apple_artist_album_string = media_props.Artist()?.to_string_lossy();
        let mut splits = apple_artist_album_string.split('—');

        artist_name = splits
            .next()
            .expect("apple music has changed how they display artist and album names")
            .trim()
            .to_owned();
        album_name = splits
            .next()
            .expect("apple music has changed how they display artist and album names")
            .trim()
            .to_owned();
    }

    let timeline_info = session.GetTimelineProperties()?;
    let end_time = timeline_info.EndTime()?.Duration / 10; // For some reason, these values are 10x smaller than a microsecond?
    let position = timeline_info.Position()?.Duration / 10;

    Ok(Some(MediaInfo {
        player_name: player.to_string_lossy(),
        artist_name,
        song_name: media_props.Title()?.to_string_lossy(),
        album_name,
        status,
        media_type: m_type,
        end_time,
        current_position: position,
    }))
}

// wrapper around i32 that verifies we got this number from windows and not just any i32.
// probably unneeded but its still nice to have.
struct RawStatusNumber(i32);
struct RawMediaTypeNumber(i32);

impl From<RawStatusNumber> for MediaStatus {
    fn from(value: RawStatusNumber) -> Self {
        match value.0 {
            0 => Self::Closed,
            1 => Self::Opened,
            2 => Self::Changing,
            3 => Self::Stopped,
            4 => Self::Playing,
            5 => Self::Paused,
            // SAFETY: Using RawStatusNumber we make sure that the only values we could get here are from the windows API itself
            _ => unreachable!(),
        }
    }
}

impl From<RawMediaTypeNumber> for MediaType {
    fn from(value: RawMediaTypeNumber) -> Self {
        match value.0 {
            0 => Self::Unknown,
            1 => Self::Music,
            2 => Self::Video,
            3 => Self::Image,
            // SAFETY: Using RawMediaTypeNumber we make sure that the only values we could get here are from the windows API itself
            _ => unreachable!(),
        }
    }
}

fn get_raw_status_code(session: &GlobalSystemMediaTransportControlsSession) -> windows_result::Result<RawStatusNumber> {
    Ok(RawStatusNumber(session.GetPlaybackInfo()?.PlaybackStatus()?.0))
}

fn get_raw_media_type(session: &GlobalSystemMediaTransportControlsSession) -> windows_result::Result<RawMediaTypeNumber> {
    Ok(RawMediaTypeNumber(session.GetPlaybackInfo()?.PlaybackType()?.Value()?.0))
}
