use std::{
    fs,
    io::{ErrorKind, Write},
};

use log::{error, info};
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Debug)]
pub struct AmpleConfig {
    /// not used yet
    pub valid_media_sources: Vec<String>,
    pub wait_for_discord: bool,
}

impl AmpleConfig {
    pub fn is_valid_media_source(&self, source: &str) -> bool {
        self.valid_media_sources.contains(&source.to_owned())
    }
    pub fn default_config() -> AmpleConfig {
        AmpleConfig {
            // this will have to be updated when / if linux support is added
            // the program name will definitely be different
            valid_media_sources: vec![sys_media::consts::WIN_APPLE_MUSIC_ID.to_owned()],
            wait_for_discord: false,
        }
    }
}

pub fn load_config() -> AmpleConfig {
    // Should create something like "/AppData/ample/config/" on windows
    // and "~/.config/ample/" on linux
    let config_dir = directories::ProjectDirs::from("", "", crate::APP_NAME)
        .expect("valid project dir")
        .config_dir()
        .to_path_buf();

    let config_file_path = config_dir.join("ample_config.json");

    match fs::File::open(&config_file_path) {
        // try to read config file
        Ok(config_file) => match serde_json::from_reader(config_file) {
            Ok(parsed_config) => return parsed_config,
            Err(parse_error) => {
                error!("Invalid config: {parse_error}");
            }
        },
        Err(open_error) => {
            // create if not found
            if open_error.kind() == ErrorKind::NotFound {
                info!("Creating new config file");
                // write default config to file
                match fs::File::create_new(&config_file_path) {
                    Ok(mut new_config) => {
                        let default = AmpleConfig::default_config();
                        if let Err(write_error) =
                            new_config.write_all(serde_json::to_vec(&default).expect("default config should be serializable").as_slice())
                        {
                            error!("Failed to write default config: {write_error}");
                        }
                    }
                    Err(create_error) => {
                        error!("Failed to create new config file: {create_error}");
                    }
                }
            } else {
                error!("Error trying to open config file: {open_error}")
            }
        }
    }

    AmpleConfig::default_config()
}
