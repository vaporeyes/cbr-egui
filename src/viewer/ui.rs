use eframe::egui;

use crate::viewer::layout::{
    Point2, Size2, ViewMode, page_display_size, spread_display_size, spread_page_sizes,
};
use crate::viewer::spread::{ReadingLayoutMode, SpreadDecision, ordered_spread_pages};
use crate::viewer::state::{
    ContinuousPageStatus, PageNavigationCommand, PageStatus, ViewCommand, ViewerState, ZoomAnchor,
    corrupted_page_color_image,
};

const KEYBOARD_ZOOM_STEP: f32 = 1.2;

pub fn render_viewer_panel(ctx: &egui::Context, state: &mut ViewerState<egui::TextureHandle>) {
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(ctx.style().visuals.panel_fill))
        .show(ctx, |ui| {
            render_viewer_contents(ui, state);
        });
}

pub fn render_viewer_contents(ui: &mut egui::Ui, state: &mut ViewerState<egui::TextureHandle>) {
    state.viewport_size = Size2::new(ui.available_width(), ui.available_height());
    handle_keybindings(ui, state);

    if state.layout_mode == ReadingLayoutMode::ContinuousVertical {
        render_continuous_page_flow(ui, state);
        return;
    }

    match state.page_status.clone() {
        PageStatus::Empty => render_center_message(ui, "No page selected"),
        PageStatus::Loading { .. } => render_center_message(ui, "Loading page"),
        PageStatus::Failed { message, .. } => render_corrupted_page(ui, &message),
        PageStatus::Ready {
            page_id: _,
            texture,
            pixel_size,
        } => {
            let next_page_id = next_page_id(&state.next_page_status);
            state.recompute_spread_decision(next_page_id);
            render_ready_page(ui, state, &texture, pixel_size);
        }
    }
}

fn render_ready_page(
    ui: &mut egui::Ui,
    state: &mut ViewerState<egui::TextureHandle>,
    texture: &egui::TextureHandle,
    pixel_size: Size2,
) {
    let viewport = state.viewport_size;
    let base_display_size = current_display_size(state, pixel_size);
    apply_pending_view_command(state, base_display_size, pixel_size);
    let display_size = base_display_size.scaled(state.zoom_pan.zoom);

    state.zoom_pan.clamp_pan(display_size, viewport);

    let scroll_delta = ui.input(|input| input.smooth_scroll_delta.y);
    let mut drag_delta = egui::Vec2::ZERO;
    let mut pending_zoom: Option<(f32, ZoomAnchor)> = None;
    let mut reset_zoom = false;

    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, _viewport| {
            let offset = state.zoom_pan.pan_offset;
            let image_size = egui::vec2(display_size.width, display_size.height);
            let canvas_size = egui::vec2(
                display_size.width.max(viewport.width),
                display_size.height.max(viewport.height),
            );

            let (rect, response) = ui.allocate_exact_size(canvas_size, egui::Sense::drag());
            paint_current_page_or_spread(ui, state, texture, pixel_size, rect, image_size, offset);

            if response.hovered() && scroll_delta != 0.0 {
                let factor = scroll_zoom_factor(scroll_delta, state.zoom_sensitivity);
                let anchor = ui
                    .input(|input| input.pointer.hover_pos())
                    .map(|pos| zoom_anchor_for_pointer(pos, rect))
                    .unwrap_or(ZoomAnchor::CENTER);
                pending_zoom = Some((factor, anchor));
            }
            if response.double_clicked() {
                reset_zoom = true;
            }
            if response.dragged() {
                drag_delta = response.drag_delta();
            }
        });

    if let Some((factor, anchor)) = pending_zoom {
        state.zoom_pan.apply_zoom_factor(factor, anchor);
    }
    if reset_zoom {
        state.zoom_pan.reset_zoom();
    }

    if drag_delta != egui::Vec2::ZERO {
        state
            .zoom_pan
            .apply_drag_pan([drag_delta.x, drag_delta.y], display_size, viewport);
    }

    render_status_chrome(ui, state);
}

fn apply_pending_view_command(
    state: &mut ViewerState<egui::TextureHandle>,
    base_display_size: Size2,
    pixel_size: Size2,
) {
    let Some(command) = state.pending_view_command.take() else {
        return;
    };

    match command {
        ViewCommand::Fit => {
            state.view_mode = ViewMode::Fit;
            state.zoom_pan.reset_zoom();
        }
        ViewCommand::Fill => {
            state.view_mode = ViewMode::Fill;
            state.zoom_pan.reset_zoom();
        }
        ViewCommand::FitWidth => {
            state.view_mode = ViewMode::FitWidth;
            state.zoom_pan.reset_zoom();
        }
        ViewCommand::FitHeight => {
            state.view_mode = ViewMode::FitHeight;
            state.zoom_pan.reset_zoom();
        }
        ViewCommand::OneToOne => {
            state.view_mode = ViewMode::Fit;
            let one_to_one_zoom = one_to_one_zoom(base_display_size, pixel_size);
            state.zoom_pan.zoom =
                one_to_one_zoom.clamp(state.zoom_pan.min_zoom, state.zoom_pan.max_zoom);
            state.zoom_pan.pan_offset = Point2::ZERO;
        }
        ViewCommand::ZoomIn => {
            state
                .zoom_pan
                .apply_zoom_factor(KEYBOARD_ZOOM_STEP, ZoomAnchor::CENTER);
        }
        ViewCommand::ZoomOut => {
            state
                .zoom_pan
                .apply_zoom_factor(1.0 / KEYBOARD_ZOOM_STEP, ZoomAnchor::CENTER);
        }
        ViewCommand::ToggleSpread => {}
        ViewCommand::ToggleContinuous => {}
        // Rotation is handled at the app layer (re-decode + cache invalidation);
        // the viewer never consumes these.
        ViewCommand::RotateLeft => {}
        ViewCommand::RotateRight => {}
        ViewCommand::ExtractPage => {}
    }
}

fn render_continuous_page_flow(ui: &mut egui::Ui, state: &mut ViewerState<egui::TextureHandle>) {
    let Some(canvas) = state.continuous_canvas.clone() else {
        render_center_message(ui, "Loading pages");
        return;
    };
    let canvas_width = canvas
        .viewport_width
        .max(state.viewport_size.width)
        .max(1.0);
    let canvas_size = egui::vec2(
        canvas_width,
        canvas.total_height.max(state.viewport_size.height),
    );
    let mut visible_window = None;

    egui::ScrollArea::vertical()
        .id_salt("continuous_reader_scroll")
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            let (canvas_rect, _response) =
                ui.allocate_exact_size(canvas_size, egui::Sense::hover());
            if let Some(scroll_top) = state.continuous_pending_scroll_top.take() {
                let restore_rect = egui::Rect::from_min_size(
                    egui::pos2(canvas_rect.left(), canvas_rect.top() + scroll_top.max(0.0)),
                    egui::vec2(1.0, 1.0),
                );
                ui.scroll_to_rect(restore_rect, Some(egui::Align::Min));
            }
            let viewport_top = viewport.top().max(0.0);
            let window =
                crate::viewer::visible_page_window(&canvas, viewport_top, viewport.height());
            let pages_to_paint = window.all_pages();

            for page in &state.continuous_pages {
                if !pages_to_paint.contains(&page.page_index) {
                    continue;
                }
                let rect = egui::Rect::from_min_size(
                    egui::pos2(canvas_rect.left(), canvas_rect.top() + page.rect.y),
                    egui::vec2(page.rect.size.width, page.rect.size.height),
                );
                match &page.status {
                    ContinuousPageStatus::Ready { texture, .. } => {
                        egui::Image::new((texture.id(), rect.size())).paint_at(ui, rect);
                    }
                    ContinuousPageStatus::Loading => {
                        paint_page_placeholder(ui, rect, "Loading page");
                    }
                    ContinuousPageStatus::Failed { message } => {
                        paint_page_placeholder(ui, rect, message);
                    }
                }
            }

            visible_window = Some(window);
        });

    state.continuous_visible_window = visible_window;
    render_status_chrome(ui, state);
}

fn paint_page_placeholder(ui: &mut egui::Ui, rect: egui::Rect, message: &str) {
    ui.painter()
        .rect_filled(rect.shrink(1.0), 2.0, ui.visuals().faint_bg_color);
    ui.painter().rect_stroke(
        rect.shrink(1.0),
        2.0,
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        message,
        egui::FontId::proportional(13.0),
        ui.visuals().weak_text_color(),
    );
}

fn one_to_one_zoom(base_display_size: Size2, pixel_size: Size2) -> f32 {
    if !base_display_size.is_valid() || !pixel_size.is_valid() {
        return 1.0;
    }
    (pixel_size.width / base_display_size.width).max(pixel_size.height / base_display_size.height)
}

fn scroll_zoom_factor(scroll_delta: f32, sensitivity: f32) -> f32 {
    let sensitivity = if sensitivity.is_finite() && sensitivity > 0.0 {
        sensitivity
    } else {
        crate::viewer::state::DEFAULT_SCROLL_ZOOM_SENSITIVITY
    };
    (scroll_delta * sensitivity).exp()
}

fn zoom_anchor_for_pointer(pointer: egui::Pos2, canvas: egui::Rect) -> ZoomAnchor {
    let delta = pointer - canvas.center();
    ZoomAnchor::from_viewport_delta(delta.x, delta.y)
}

fn current_display_size(state: &ViewerState<egui::TextureHandle>, pixel_size: Size2) -> Size2 {
    match (&state.spread_decision, &state.next_page_status) {
        (
            Some(SpreadDecision::Pair { .. }),
            PageStatus::Ready {
                pixel_size: right_size,
                ..
            },
        ) => spread_display_size(
            pixel_size,
            *right_size,
            state.viewport_size,
            state.view_mode,
        ),
        _ => page_display_size(pixel_size, state.viewport_size, state.view_mode),
    }
}

fn paint_current_page_or_spread(
    ui: &mut egui::Ui,
    state: &ViewerState<egui::TextureHandle>,
    texture: &egui::TextureHandle,
    left_size: Size2,
    canvas: egui::Rect,
    image_size: egui::Vec2,
    offset: Point2,
) {
    match (&state.spread_decision, &state.next_page_status) {
        (
            Some(SpreadDecision::Pair {
                left_page_id,
                right_page_id,
            }),
            PageStatus::Ready {
                texture: right_texture,
                pixel_size: right_size,
                ..
            },
        ) => {
            let (left_display, right_display) =
                spread_page_sizes(left_size, *right_size, state.viewport_size, state.view_mode);
            let ordered_pages =
                ordered_spread_pages(*left_page_id, *right_page_id, state.reading_direction);
            let left_display = left_display.scaled(state.zoom_pan.zoom);
            let right_display = right_display.scaled(state.zoom_pan.zoom);
            let total = egui::vec2(
                left_display.width + right_display.width,
                left_display.height.max(right_display.height),
            );
            let origin = canvas.center() - total / 2.0 - egui::vec2(offset.x, offset.y);
            let left_rect = egui::Rect::from_min_size(
                origin,
                egui::vec2(left_display.width, left_display.height),
            );
            let right_rect = egui::Rect::from_min_size(
                egui::pos2(origin.x + left_display.width, origin.y),
                egui::vec2(right_display.width, right_display.height),
            );
            if ordered_pages.0 == *left_page_id {
                egui::Image::new((texture.id(), left_rect.size())).paint_at(ui, left_rect);
                egui::Image::new((right_texture.id(), right_rect.size())).paint_at(ui, right_rect);
            } else {
                egui::Image::new((right_texture.id(), right_rect.size())).paint_at(ui, left_rect);
                egui::Image::new((texture.id(), left_rect.size())).paint_at(ui, right_rect);
            }
        }
        (Some(SpreadDecision::PairPending { .. }), _) => {
            let image_rect = image_rect(canvas, image_size, offset);
            egui::Image::new((texture.id(), image_size)).paint_at(ui, image_rect);
            paint_side_message(ui, image_rect.right_top(), "Loading next page");
        }
        (Some(SpreadDecision::PairFailed { message, .. }), _) => {
            let image_rect = image_rect(canvas, image_size, offset);
            egui::Image::new((texture.id(), image_size)).paint_at(ui, image_rect);
            paint_side_message(ui, image_rect.right_top(), message);
        }
        _ => {
            let image_rect = image_rect(canvas, image_size, offset);
            egui::Image::new((texture.id(), image_size)).paint_at(ui, image_rect);
        }
    }
}

fn handle_keybindings(ui: &mut egui::Ui, state: &mut ViewerState<egui::TextureHandle>) {
    ui.input(|input| {
        if input.key_pressed(egui::Key::ArrowRight) || input.key_pressed(egui::Key::PageDown) {
            state.pending_navigation = Some(PageNavigationCommand::NextPage);
        } else if input.key_pressed(egui::Key::ArrowLeft) || input.key_pressed(egui::Key::PageUp) {
            state.pending_navigation = Some(PageNavigationCommand::PreviousPage);
        } else if input.key_pressed(egui::Key::Space) || input.key_pressed(egui::Key::ArrowDown) {
            state.pending_navigation = Some(PageNavigationCommand::ScrollDown);
        } else if input.key_pressed(egui::Key::ArrowUp) {
            state.pending_navigation = Some(PageNavigationCommand::ScrollUp);
        } else if input.key_pressed(egui::Key::Home) {
            state.pending_navigation = Some(PageNavigationCommand::FirstPage);
        } else if input.key_pressed(egui::Key::End) {
            state.pending_navigation = Some(PageNavigationCommand::LastPage);
        }

        if input.key_pressed(egui::Key::F) {
            state.pending_view_command = Some(if input.modifiers.shift {
                ViewCommand::Fill
            } else {
                ViewCommand::Fit
            });
        } else if input.key_pressed(egui::Key::W) {
            state.pending_view_command = Some(ViewCommand::FitWidth);
        } else if input.key_pressed(egui::Key::H) {
            state.pending_view_command = Some(ViewCommand::FitHeight);
        } else if input.key_pressed(egui::Key::Num1) {
            state.pending_view_command = Some(ViewCommand::OneToOne);
        } else if input.key_pressed(egui::Key::Equals) || input.key_pressed(egui::Key::Plus) {
            state.pending_view_command = Some(ViewCommand::ZoomIn);
        } else if input.key_pressed(egui::Key::Minus) {
            state.pending_view_command = Some(ViewCommand::ZoomOut);
        } else if input.key_pressed(egui::Key::S) {
            state.pending_view_command = Some(ViewCommand::ToggleSpread);
        } else if input.key_pressed(egui::Key::V) {
            state.pending_view_command = Some(ViewCommand::ToggleContinuous);
        } else if input.key_pressed(egui::Key::R) {
            state.pending_view_command = Some(if input.modifiers.shift {
                ViewCommand::RotateLeft
            } else {
                ViewCommand::RotateRight
            });
        }

        if input.key_pressed(egui::Key::B) {
            state.pending_bookmark_toggle = true;
        }
    });
}

fn image_rect(canvas: egui::Rect, image_size: egui::Vec2, pan: Point2) -> egui::Rect {
    let center = canvas.center() - egui::vec2(pan.x, pan.y);
    egui::Rect::from_center_size(center, image_size)
}

fn render_center_message(ui: &mut egui::Ui, message: &str) {
    ui.centered_and_justified(|ui| {
        ui.label(egui::RichText::new(message).strong());
    });
}

fn render_corrupted_page(ui: &mut egui::Ui, message: &str) {
    let texture = ui.ctx().load_texture(
        "corrupted-page-fallback",
        corrupted_page_color_image(message),
        egui::TextureOptions::LINEAR,
    );
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.image((texture.id(), egui::vec2(180.0, 252.0)));
            ui.label(egui::RichText::new("Corrupted Page").strong());
            ui.label(egui::RichText::new(message).weak());
        });
    });
}

fn render_status_chrome(ui: &mut egui::Ui, state: &ViewerState<egui::TextureHandle>) {
    if !state.chrome.visible {
        return;
    }

    if let Some(status_text) = &state.chrome.status_text {
        let painter = ui.painter();
        let rect = ui.max_rect();
        let pos = egui::pos2(rect.left() + 14.0, rect.top() + 14.0);
        painter.text(
            pos,
            egui::Align2::LEFT_TOP,
            status_text,
            egui::FontId::proportional(13.0),
            ui.visuals().weak_text_color(),
        );
    }
}

fn paint_side_message(ui: &mut egui::Ui, pos: egui::Pos2, message: &str) {
    ui.painter().text(
        pos + egui::vec2(18.0, 18.0),
        egui::Align2::LEFT_TOP,
        message,
        egui::FontId::proportional(13.0),
        ui.visuals().weak_text_color(),
    );
}

fn next_page_id(status: &PageStatus<egui::TextureHandle>) -> Option<crate::viewer::PageId> {
    status.page_id()
}
