use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::viewer::ReadingDirection;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    pub zoom_sensitivity: f32,
    pub dark_mode: bool,
    pub reading_direction: ReadingDirection,
    pub resume_last_session: bool,
    pub last_import_dir: Option<PathBuf>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            zoom_sensitivity: 0.0015,
            dark_mode: true,
            reading_direction: ReadingDirection::LeftToRight,
            resume_last_session: false,
            last_import_dir: None,
        }
    }
}

impl AppConfig {
    pub const DEFAULT_ZOOM_SENSITIVITY: f32 = 0.0015;
    pub const MIN_ZOOM_SENSITIVITY: f32 = 0.0002;
    pub const MAX_ZOOM_SENSITIVITY: f32 = 0.01;

    /// Loads the config, falling back to defaults. A file that cannot be parsed
    /// is moved aside rather than left in place: the next save would otherwise
    /// overwrite it, silently destroying settings that a small edit could have
    /// recovered.
    pub fn load(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        // No config yet is the normal first-run case, not a problem.
        let Ok(content) = fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str::<Self>(&content) {
            Ok(config) => config.normalized(),
            Err(_) => {
                let _ = fs::rename(path, path.with_extension("json.bak"));
                Self::default()
            }
        }
    }

    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&self.clone().normalized())
            .map_err(std::io::Error::other)?;
        fs::write(path, content)
    }

    pub fn normalized(mut self) -> Self {
        self.zoom_sensitivity = normalize_zoom_sensitivity(self.zoom_sensitivity);
        self
    }
}

pub fn normalize_zoom_sensitivity(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value.clamp(
            AppConfig::MIN_ZOOM_SENSITIVITY,
            AppConfig::MAX_ZOOM_SENSITIVITY,
        )
    } else {
        AppConfig::DEFAULT_ZOOM_SENSITIVITY
    }
}

pub fn default_config_path() -> PathBuf {
    config_root().join("config.json")
}

pub fn default_library_db_path() -> PathBuf {
    data_root().join("library.sqlite")
}

pub fn default_library_store_root() -> PathBuf {
    data_root().join("comics")
}

fn config_root() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(std::env::temp_dir)
        .join("cbr-egui")
}

fn data_root() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(std::env::temp_dir)
        .join("cbr-egui")
}
