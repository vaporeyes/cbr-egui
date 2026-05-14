use cbr_egui::config::AppConfig;
use cbr_egui::viewer::ReadingDirection;

#[test]
fn config_round_trips_to_json_file() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("config.json");
    let config = AppConfig {
        zoom_sensitivity: 0.002,
        dark_mode: false,
        reading_direction: ReadingDirection::RightToLeft,
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
