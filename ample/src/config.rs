pub struct AmpleConfig {
    /// not used yet
    pub valid_media_sources: Vec<String>,
    pub wait_for_discord: bool,
}

impl AmpleConfig {
    pub fn is_valid_media_source(&self, source: &str) -> bool {
        self.valid_media_sources.contains(&source.to_owned())
    }
    pub fn load_config() -> AmpleConfig {
        AmpleConfig {
            valid_media_sources: vec![sys_media::consts::APPLE_MUSIC_ID.to_owned()],
            wait_for_discord: false,
        }
    }
}
