use cbr_egui::app::ui::EguiComicReaderApp;
use cbr_egui::config::AppConfig;
use cbr_egui::viewer::ReadingDirection;
use std::collections::HashMap;

#[derive(Default)]
struct MemoryStorage {
    values: HashMap<String, String>,
}

impl eframe::Storage for MemoryStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }

    fn set_string(&mut self, key: &str, value: String) {
        self.values.insert(key.to_owned(), value);
    }

    fn flush(&mut self) {}
}

#[test]
fn config_round_trips_to_json_file() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("config.json");
    let config = AppConfig {
        zoom_sensitivity: 0.002,
        dark_mode: false,
        reading_direction: ReadingDirection::RightToLeft,
        resume_last_session: true,
        last_import_dir: None,
    };

    config.save(&path).expect("save");
    let loaded = AppConfig::load(&path);

    assert_eq!(loaded, config);
}

#[test]
fn missing_config_uses_defaults() {
    let loaded = AppConfig::load("/missing/config.json");

    assert_eq!(loaded, AppConfig::default());
}

#[test]
fn invalid_zoom_sensitivity_is_normalized() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("config.json");
    std::fs::write(
        &path,
        r#"{"zoom_sensitivity":-1.0,"dark_mode":true,"reading_direction":"LeftToRight"}"#,
    )
    .expect("write");

    let loaded = AppConfig::load(&path);

    assert_eq!(loaded.zoom_sensitivity, AppConfig::DEFAULT_ZOOM_SENSITIVITY);
}

#[test]
fn missing_resume_flag_defaults_to_disabled() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("config.json");
    std::fs::write(
        &path,
        r#"{"zoom_sensitivity":0.002,"dark_mode":true,"reading_direction":"LeftToRight"}"#,
    )
    .expect("write");

    let loaded = AppConfig::load(&path);

    assert!(!loaded.resume_last_session);
}

#[test]
fn app_lifecycle_save_writes_config_file() {
    let dir = tempfile::tempdir().expect("dir");
    let config_path = dir.path().join("config.json");
    let mut app = EguiComicReaderApp::test_instance();
    app.config_path = config_path.clone();
    app.config = AppConfig {
        zoom_sensitivity: 0.002,
        dark_mode: false,
        reading_direction: ReadingDirection::RightToLeft,
        resume_last_session: true,
        last_import_dir: None,
    };
    let mut storage = MemoryStorage::default();

    eframe::App::save(&mut app, &mut storage);

    assert_eq!(AppConfig::load(config_path), app.config);
}
