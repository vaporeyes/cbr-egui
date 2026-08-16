fn main() -> eframe::Result<()> {
    if let Some(code) = cbr_egui::mac_bundle::try_relaunch_via_bundle() {
        std::process::exit(code);
    }

    cbr_egui::mac_open::queue_paths_from_args();
    cbr_egui::mac_open::install_launch_hook();

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "cbr-egui",
        options,
        Box::new(|cc| {
            cbr_egui::mac_activation::activate();
            cbr_egui::mac_open::install_open_handler();
            cbr_egui::mac_open::set_repaint_context(cc.egui_ctx.clone());
            cbr_egui::app::ui::install_icon_fonts(&cc.egui_ctx);
            let app = cbr_egui::app::ui::EguiComicReaderApp::new();
            app.apply_config_to_context(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}
