use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::viewer::ReadingDirection;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    pub zoom_sensitivity: f32,
    pub dark_mode: bool,
    pub reading_direction: ReadingDirection,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            zoom_sensitivity: 0.0015,
            dark_mode: true,
            reading_direction: ReadingDirection::LeftToRight,
        }
    }
}

impl AppConfig {
    pub fn load(path: impl AsRef<Path>) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(path, content)
    }
}
