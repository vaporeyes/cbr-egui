// ABOUTME: Editor-inspired colour palette, frames, and icon button helpers.
// ABOUTME: Owns the egui Style and Visuals the whole app is rendered with.
use eframe::egui;

pub(crate) const EDITOR_BACKGROUND: egui::Color32 = egui::Color32::from_rgb(44, 58, 60);
pub(crate) const EDITOR_PANEL: egui::Color32 = egui::Color32::from_rgb(50, 68, 70);
pub(crate) const EDITOR_PANEL_DARK: egui::Color32 = egui::Color32::from_rgb(39, 53, 55);
pub(crate) const EDITOR_PANEL_ACTIVE: egui::Color32 = egui::Color32::from_rgb(63, 98, 104);
pub(crate) const EDITOR_WIDGET: egui::Color32 = egui::Color32::from_rgb(57, 78, 80);
pub(crate) const EDITOR_WIDGET_HOVER: egui::Color32 = egui::Color32::from_rgb(70, 102, 106);
pub(crate) const EDITOR_TEXT: egui::Color32 = egui::Color32::from_rgb(221, 226, 220);
pub(crate) const EDITOR_TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(154, 164, 160);
pub(crate) const EDITOR_GREEN: egui::Color32 = egui::Color32::from_rgb(82, 190, 145);
pub(crate) const EDITOR_CYAN: egui::Color32 = egui::Color32::from_rgb(117, 219, 210);
pub(crate) const EDITOR_ORANGE: egui::Color32 = egui::Color32::from_rgb(222, 129, 70);
pub(crate) const EDITOR_PURPLE: egui::Color32 = egui::Color32::from_rgb(176, 127, 218);

pub(crate) fn editor_panel_frame() -> egui::Frame {
    egui::Frame::new()
        .inner_margin(egui::Margin::same(14))
        .fill(EDITOR_BACKGROUND)
}

pub(crate) fn editor_toolbar_frame() -> egui::Frame {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(12, 8))
        .fill(EDITOR_PANEL_DARK)
}

pub(crate) fn editor_card_stroke() -> egui::Stroke {
    egui::Stroke::new(1.0, EDITOR_GREEN)
}

const TOOLBAR_ICON_SIZE: f32 = 17.0;

/// Merges the Phosphor icon font into the egui context. Phosphor attaches to the
/// Proportional family, so icon glyphs must be rendered with a proportional FontId
/// (our default text style is monospace). Call once at startup.
pub fn install_icon_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
}

pub(crate) fn icon_text(glyph: &str) -> egui::RichText {
    egui::RichText::new(glyph).font(egui::FontId::proportional(TOOLBAR_ICON_SIZE))
}

/// A compact icon-only toolbar button with a hover tooltip.
pub(crate) fn icon_button(ui: &mut egui::Ui, glyph: &str, tooltip: &str) -> egui::Response {
    ui.add(egui::Button::new(icon_text(glyph)))
        .on_hover_text(tooltip)
}

/// An icon button that is disabled when `enabled` is false.
pub(crate) fn icon_button_enabled(
    ui: &mut egui::Ui,
    enabled: bool,
    glyph: &str,
    tooltip: &str,
) -> egui::Response {
    ui.add_enabled(enabled, egui::Button::new(icon_text(glyph)))
        .on_hover_text(tooltip)
}

/// An icon button that renders in a pressed/active state when `active` is true.
pub(crate) fn icon_toggle(
    ui: &mut egui::Ui,
    active: bool,
    glyph: &str,
    tooltip: &str,
) -> egui::Response {
    ui.add(egui::Button::new(icon_text(glyph)).selected(active))
        .on_hover_text(tooltip)
}

pub(crate) fn apply_editor_text_styles(style: &mut egui::Style) {
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::monospace(20.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::monospace(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::monospace(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::monospace(11.0));
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.interact_size.y = 26.0;
}

pub(crate) fn editor_dark_visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(EDITOR_TEXT);
    visuals.panel_fill = EDITOR_BACKGROUND;
    visuals.window_fill = EDITOR_PANEL;
    visuals.window_stroke = editor_card_stroke();
    visuals.window_corner_radius = 4.0.into();
    visuals.menu_corner_radius = 4.0.into();
    visuals.faint_bg_color = EDITOR_PANEL_DARK;
    visuals.extreme_bg_color = EDITOR_PANEL_DARK;
    visuals.code_bg_color = EDITOR_PANEL_DARK;
    visuals.hyperlink_color = EDITOR_CYAN;
    visuals.warn_fg_color = EDITOR_ORANGE;
    visuals.error_fg_color = egui::Color32::from_rgb(233, 104, 104);
    visuals.selection.bg_fill = EDITOR_PANEL_ACTIVE;
    visuals.selection.stroke = egui::Stroke::new(1.0, EDITOR_CYAN);

    visuals.widgets.noninteractive.bg_fill = EDITOR_PANEL;
    visuals.widgets.noninteractive.weak_bg_fill = EDITOR_PANEL_DARK;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, EDITOR_WIDGET_HOVER);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, EDITOR_TEXT);
    visuals.widgets.noninteractive.corner_radius = 4.0.into();

    visuals.widgets.inactive.bg_fill = EDITOR_WIDGET;
    visuals.widgets.inactive.weak_bg_fill = EDITOR_WIDGET;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, EDITOR_WIDGET_HOVER);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, EDITOR_TEXT);
    visuals.widgets.inactive.corner_radius = 4.0.into();

    visuals.widgets.hovered.bg_fill = EDITOR_WIDGET_HOVER;
    visuals.widgets.hovered.weak_bg_fill = EDITOR_PANEL_ACTIVE;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, EDITOR_CYAN);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, EDITOR_TEXT);
    visuals.widgets.hovered.corner_radius = 4.0.into();

    visuals.widgets.active.bg_fill = EDITOR_PANEL_ACTIVE;
    visuals.widgets.active.weak_bg_fill = EDITOR_PANEL_ACTIVE;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, EDITOR_GREEN);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, EDITOR_CYAN);
    visuals.widgets.active.corner_radius = 4.0.into();

    visuals.widgets.open = visuals.widgets.hovered;
    visuals.button_frame = true;
    visuals.collapsing_header_frame = true;
    visuals.striped = false;
    visuals
}
