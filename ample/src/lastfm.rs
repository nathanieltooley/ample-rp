use log::debug;
use serde::Deserialize;
use thiserror::Error;
use keyring::Entry;
use ureq::Agent;

use std::{collections::HashMap, env, time::{SystemTime, UNIX_EPOCH}};

use crate::uri;

const API_ROOT: &str = "https://ws.audioscrobbler.com/2.0";

pub struct LastFm {
    client: ureq::Agent,
    creds: LastFmCreds,
}

pub struct LastFmCreds {
    pub api_key: String,
    pub api_secret: String,
    pub session_token: String,
}

#[derive(Deserialize)]
struct AuthMobileSessionResponse {
    pub session: AuthMobileSessionResponseInner
}

#[derive(Deserialize)]
struct AuthMobileSessionResponseInner {
    pub name: String,
    pub key: String,
    pub subscriber: i64,
}

#[derive(Deserialize, Debug)]
struct TrackInfoResponse {
    pub track: TrackInfo
}

#[derive(Deserialize, Debug)]
pub struct TrackInfo {
    pub name: String,
    pub artist: ArtistInfo,
    pub album: AlbumInfo,
}

#[derive(Deserialize, Debug)]
pub struct ArtistInfo {
    pub name: String
}

#[derive(Deserialize, Debug)]
pub struct AlbumInfo {
    pub artist: String,
    pub title: String,

    #[serde(rename = "image")]
    pub images: Vec<ImageInfo>
}

#[derive(Deserialize, Debug)]
pub struct ImageInfo {
    // thought about making this an enum but I'm only gonna use
    // small images
    pub size: String,

    #[serde(rename = "#text")]
    pub url: String
}

#[derive(Error, Debug)]
pub enum CredsError {
    // static lifetime of key since it should be a string literal
    #[error("Error obtaining environment variable, {0}, because {1}")]
    Env(&'static str, env::VarError),
    #[error("Error obtaining credentials from keyring: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("Password has not been set! Call with --password flag to set password!")]
    MissingPassword,
    #[error("LastFM secret has not been set! Call with --secret flag to set secret!")]
    MissingApiSecret,
    #[error("Http error: {0}")]
    Http(#[from] ureq::Error),
}

pub struct ScrobbleError;

impl LastFm {
    pub fn new(client: ureq::Agent, creds: LastFmCreds) -> LastFm {
        LastFm {
            client,
            creds
        }
    }

    pub fn scrobble(&self, artist: &str, track: &str, timestamp: SystemTime, album: Option<&str>) -> Result<(), ureq::Error> {
        let timestamp_str = format!("{}", timestamp.duration_since(UNIX_EPOCH).unwrap().as_secs());
        let mut params = HashMap::new();
        params.insert("method", "track.scrobble");
        params.insert("artist", artist);
        params.insert("track", track);
        params.insert("timestamp", &timestamp_str);
        params.insert("api_key", &self.creds.api_key);
        params.insert("sk", &self.creds.session_token);
        
        if let Some(album) = album {
            params.insert("album", album);
        }
        
        let sig = create_api_sig(&params, &self.creds.api_secret);
        params.insert("format", "json");
        params.insert("api_sig", &sig);

        let mut rep = self.client.post(API_ROOT).send_form(params)?;
        let body = rep.body_mut().read_to_string()?;

        // ureq::http_status_as_error is set to false so that this can happen
        // inbetween the error. There might be a better way of doing this but im not sure.
        debug!("{body}");

        if rep.status().is_client_error() || rep.status().is_server_error() {
            return Err(ureq::Error::StatusCode(rep.status().as_u16()));
        }

        Ok(())
    }

    pub fn now_playing(&self, artist: &str, track: &str, album: Option<&str>) -> Result<(), ureq::Error> {
        let mut params = HashMap::new();
        params.insert("method", "track.updateNowPlaying");
        params.insert("artist", artist);
        params.insert("track", track);
        params.insert("api_key", &self.creds.api_key);
        params.insert("sk", &self.creds.session_token);
        
        if let Some(album) = album {
            params.insert("album", album);
        }
        
        let sig = create_api_sig(&params, &self.creds.api_secret);
        params.insert("format", "json");
        params.insert("api_sig", &sig);

        let mut rep = self.client.post(API_ROOT).send_form(params)?;
        let body = rep.body_mut().read_to_string()?;

        debug!("{body}");

        if rep.status().is_client_error() || rep.status().is_server_error() {
            return Err(ureq::Error::StatusCode(rep.status().as_u16()));
        }

        Ok(())
    }

    pub fn get_track_info(&self, artist: &str, track: &str) -> Result<TrackInfo, ureq::Error> {
        let mut params = HashMap::new();
        params.insert("method", "track.getInfo");
        params.insert("artist", artist);
        params.insert("track", track);
        params.insert("api_key", &self.creds.api_key);
        params.insert("format", "json");

        let uri = create_param_uri(&params, None);
        debug!("{uri}");
        let mut rep = self.client.get(uri).call()?;
        let body = rep.body_mut().read_to_string()?;

        debug!("{body}");

        if rep.status().is_client_error() || rep.status().is_server_error() {
            return Err(ureq::Error::StatusCode(rep.status().as_u16()));
        }

        let track: TrackInfoResponse = serde_json::from_str(&body)?;

        Ok(track.track)
    }
}

/// Represents all required credentials for autheticated LastFM API requests.
/// This struct uses mobile authentication so that the application does not have to
/// open a web browser.
impl LastFmCreds {
    pub fn get_creds(client: Agent) -> Result<LastFmCreds, CredsError> {
        let api_key = env::var("AMPLE_API_KEY").map_err(|var_error| CredsError::Env("AMPLE_API_KEY", var_error))?;
        let username = env::var("AMPLE_USERNAME").map_err(|var_error| CredsError::Env("AMPLE_USERNAME", var_error))?;

        let password_entry = Entry::new_with_target(crate::PASSWORD_ENTRY_NAME, crate::APP_NAME, crate::APP_NAME)?;
        let secret_entry = Entry::new_with_target(crate::SECRET_ENTRY_NAME, crate::APP_NAME, crate::APP_NAME)?;
        
        let password = password_entry.get_password().map_err(|kr_err| {
            match kr_err {
                keyring::Error::NoEntry => CredsError::MissingPassword, 
                _ => CredsError::Keyring(kr_err)
            }
        })?;
        let secret = secret_entry.get_password().map_err(|kr_err| {
            match kr_err {
                keyring::Error::NoEntry => CredsError::MissingApiSecret,
                _ => CredsError::Keyring(kr_err)
            }
        })?;
        
        let session_entry = Entry::new_with_target(crate::SESSION_ENTRY_NAME, crate::APP_NAME, crate::APP_NAME)?;
        let session_token = match session_entry.get_password() {
            Err(err) => {
                // Ask LastFM for session token
                if let keyring::Error::NoEntry = err {
                    let mut map_params = HashMap::new();
                    map_params.insert("method", "auth.getMobileSession");
                    map_params.insert("api_key", &api_key);
                    map_params.insert("password", &password);
                    map_params.insert("username", &username);

                    let sig = create_api_sig(&map_params, &secret);
                    map_params.insert("api_sig", &sig);
                    map_params.insert("format", "json");

                    debug!("sig: {sig}");
                    debug!("uri: {API_ROOT}");

                    let mut rep = client
                        .post(API_ROOT)
                        .send_form(map_params)?;

                    let body = rep.body_mut().read_to_string()?;

                    debug!("{body}");
                    if rep.status().is_client_error() || rep.status().is_server_error() {
                        return Err(CredsError::Http(ureq::Error::StatusCode(rep.status().as_u16())));
                    }
                    
                    let json_response: AuthMobileSessionResponse = serde_json::from_str(&body).map_err(ureq::Error::Json)?;
                    let key = json_response.session.key;

                    session_entry.set_password(&key)?;

                    key
                } else {
                    return Err(CredsError::Keyring(err));
                }
            }
            Ok(sess) => sess
        };


        Ok(LastFmCreds {api_key, api_secret: secret, session_token})
    }
}

/// Creates an MD5 hash needed to sign API requests.
fn create_api_sig(params: &HashMap<&str, &str>, secret: &str) -> String {
    let mut unhashed_api_string = String::new();
    let mut sorted_params: Vec<(&&str, &&str)>  = params.iter().collect();
    sorted_params.sort_by(|a, b| {
        a.0.cmp(b.0)
    });

    for (name, value) in sorted_params {
        unhashed_api_string.push_str(name);
        unhashed_api_string.push_str(value);
    }

    unhashed_api_string.push_str(secret);

    debug!("Unhashed API sig: {unhashed_api_string}");

    let dig = md5::compute(unhashed_api_string);

    format!("{dig:x}")
}

/// Creates a uri from API_ROOT that contains the given params.
/// For a more consistent output (since iterating through a HashMap has a random order),
/// the parameters are sorted.
fn create_param_uri(params: &HashMap<&str, &str>, sig: Option<String>) -> String {
    let mut uri = format!("{API_ROOT}/?");
    let mut params: Vec<(&&str, &&str)> = params.iter().collect();
    params.sort_by(|a, b| {
        a.0.cmp(b.0)
    });
    for (i, (name, value)) in params.into_iter().enumerate() {
        if i != 0 {
            uri.push('&');
        }

        let encoded_name = uri::percent_encode(name);
        let encoded_value = uri::percent_encode(value);

        uri.push_str(&format!("{encoded_name}={encoded_value}"));
    }

    if let Some(sig) = sig {
        uri.push_str(&format!("&api_sig={sig}"));
    }

    // force json response
    uri.push_str("&format=json");

    uri
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn param_uri() {
        let mut params = HashMap::new();
        params.insert("method", "juice");
        params.insert("api_key", "apple");
        params.insert("fortnite", "battlePass");

        let uri = create_param_uri(&params, None);
        assert_eq!(
            uri,
            "https://ws.audioscrobbler.com/2.0/?api_key=apple&fortnite=battlePass&method=juice&format=json"
        )
    }
}
