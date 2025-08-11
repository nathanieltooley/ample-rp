use log::debug;
use serde::Deserialize;
use thiserror::Error;
use keyring::Entry;
use ureq::Agent;

use std::env;

const API_ROOT: &str = "https://ws.audioscrobbler.com/2.0";

pub struct LastFm {
    client: ureq::Agent,
    creds: LastFmCreds,
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
    Http(#[from] ureq::Error)
}

impl LastFm {
    pub fn new(creds: LastFmCreds) -> LastFm {
        LastFm {
            client: ureq::agent(),
            creds
        }
    }
}
pub struct LastFmCreds {
    pub api_key: String,
    pub username: String,
    pub password: String,
    pub api_secret: String,
    pub session_token: String,
}

impl LastFmCreds {
    pub fn get_creds(client: Agent) -> Result<LastFmCreds, CredsError> {
        let api_key = env::var("AMPLE_API_KEY").map_err(|var_error| CredsError::Env("AMPLE_API_KEY", var_error))?;
        let username = env::var("AMPLE_USERNAME").map_err(|var_error| CredsError::Env("AMPLE_USERNAME", var_error))?;

        let user = "test";

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
                    let raw_params = &mut [
                        ("method", "auth.getMobileSession"),
                        ("api_key", &api_key),
                        ("password", &password),
                        ("username", &username),
                    ];
                    let mut params = Params(raw_params);
                    let sig = create_api_sig(&mut params, &secret);
                    debug!("sig: {sig}");
                    let uri = create_param_uri(params, Some(sig));
                    debug!("uri: {uri}");

                    let mut rep = client
                        .post(uri)
                        .send("")?;

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


        Ok(LastFmCreds {api_key, username, password, api_secret: secret, session_token})
    }
}

struct Params<'a>(&'a mut [(&'a str, &'a str)]);

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

/// Creates an MD5 hash needed to sign API requests.
/// This function will mutate params by sorting them first,
/// which is required by last.fm.
fn create_api_sig(params: &mut Params, secret: &str) -> String {
    let mut unhashed_api_string = String::new();
    params.0.sort_by(|a, b| a.0.cmp(b.0));

    for (name, value) in &mut *params.0 {
        unhashed_api_string.push_str(name);
        unhashed_api_string.push_str(value);
    }

    unhashed_api_string.push_str(secret);

    debug!("Unhashed API sig: {unhashed_api_string}");

    let dig = md5::compute(unhashed_api_string);

    format!("{dig:x}")
}

fn create_param_uri(params: Params, sig: Option<String>) -> String {
    let mut uri = format!("{API_ROOT}/?");
    for i in 0..params.0.len() {
        let (name, value) = params.0[i];
        if i != 0 {
            uri.push('&');
        }

        uri.push_str(&format!("{name}={value}"));
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
        let test_params = &mut [
            ("method", "juice"),
            ("api_key", "apple"),
            ("fortnite", "battlePass"),
        ];
        let params = Params(test_params);

        let uri = create_param_uri(params, None);
        assert_eq!(
            uri,
            "http://ws.audioscrobbler.com/2.0/?method=juice&api_key=apple&fortnite=battlePass&format=json"
        )
    }
}
