fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "cbr-egui",
        options,
        Box::new(|cc| {
            let app = cbr_egui::app::ui::EguiComicReaderApp::new();
            app.apply_config_to_context(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}
