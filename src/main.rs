fn main() -> eframe::Result<()> {
    if let Some(code) = cbr_egui::mac_bundle::try_relaunch_via_bundle() {
        std::process::exit(code);
    }

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "cbr-egui",
        options,
        Box::new(|cc| {
            cbr_egui::mac_activation::activate();
            cbr_egui::app::ui::install_icon_fonts(&cc.egui_ctx);
            let app = cbr_egui::app::ui::EguiComicReaderApp::new();
            app.apply_config_to_context(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}
