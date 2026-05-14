fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "cbr-egui",
        options,
        Box::new(|_cc| Ok(Box::new(cbr_egui::app::ui::EguiComicReaderApp::new()))),
    )
}
