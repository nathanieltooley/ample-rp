use windows::Media::Control::*;

use crate::{consts::APPLE_MUSIC_ID, MediaInfo, MediaStatus, MediaType};

// wrapper around i32 that verifies we got this number from windows and not just any i32
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

pub fn get_current_session() -> Result<GlobalSystemMediaTransportControlsSession, windows::core::Error> {
    let media_controller = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?;
    let media_controller = media_controller.get()?;

    media_controller.GetCurrentSession()
}

pub fn get_current_session_info(session: &GlobalSystemMediaTransportControlsSession) -> Result<Option<MediaInfo>, windows::core::Error> {
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

fn get_raw_status_code(session: &GlobalSystemMediaTransportControlsSession) -> Result<RawStatusNumber, windows::core::Error> {
    Ok(RawStatusNumber(session.GetPlaybackInfo()?.PlaybackStatus()?.0))
}

fn get_raw_media_type(session: &GlobalSystemMediaTransportControlsSession) -> Result<RawMediaTypeNumber, windows::core::Error> {
    Ok(RawMediaTypeNumber(session.GetPlaybackInfo()?.PlaybackType()?.Value()?.0))
}
