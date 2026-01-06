pub struct AmpleConfig {
    /// not used yet
    pub valid_media_sources: Vec<String>,
    pub wait_for_discord: bool,
}

pub fn load_config() -> AmpleConfig {
    AmpleConfig {
        valid_media_sources: Vec::new(),
        wait_for_discord: false,
    }
}
