use std::env;

use keyring::Entry;
use log::info;

const SECRET_ENTRY_NAME: &str = "ampleSecret";
const PASSWORD_ENTRY_NAME: &str = "amplePassword";

/// Attempt to get password from OS password/credential manager. If that fails,
/// attempt to get environment variable.
pub fn get_lastfm_password() -> Option<String> {
    let password_entry = Entry::new_with_target(PASSWORD_ENTRY_NAME, crate::APP_NAME, crate::APP_NAME).and_then(|entry| entry.get_password());

    match password_entry {
        Ok(entry) => Some(entry),
        Err(err) => {
            info!("Failed to get LastFM password from creds manager: {err}");
            info!("Fall back to environment variable");
            env::var("AMPLE_FM_PASSWORD").ok()
        }
    }
}

/// Attempt to get API secret from OS password/credential manager. If that fails,
/// attempt to get environment variable.
pub fn get_lastfm_secret() -> Option<String> {
    let secret_entry = Entry::new_with_target(SECRET_ENTRY_NAME, crate::APP_NAME, crate::APP_NAME).and_then(|entry| entry.get_password());

    match secret_entry {
        Ok(entry) => Some(entry),
        Err(err) => {
            info!("Failed to get LastFM api secret from creds manager: {err}");
            info!("Fall back to environment variable");
            env::var("AMPLE_FM_SECRET").ok()
        }
    }
}
