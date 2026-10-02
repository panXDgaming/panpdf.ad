use eframe::egui;

const START: egui::Vec2 = egui::vec2(-12.0, 56.0);

#[cfg(target_arch = "wasm32")]
pub(crate) fn side_panel(
    ctx: &egui::Context,
    title: impl Into<egui::WidgetText>,
    id: &str,
    width: f32,
) -> egui::Window<'static> {
    let room = room_on(ctx);
    let id = egui::Id::new(id);
    fit_to_screen(
        ctx,
        id,
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .default_width(width.min(room.x))
            .pivot(egui::Align2::RIGHT_TOP)
            .default_pos(ctx.content_rect().right_top() + START),
    )
}

const EDGE: f32 = 8.0;

fn room_on(ctx: &egui::Context) -> egui::Vec2 {
    let screen = ctx.content_rect();
    egui::vec2(
        (screen.width() - 2.0 * EDGE).max(160.0),
        (screen.height() - START.y - BARS_BELOW).max(160.0),
    )
}

const BARS_BELOW: f32 = 110.0;

pub(crate) fn fit_to_screen<'open>(
    ctx: &egui::Context,
    id: egui::Id,
    window: egui::Window<'open>,
) -> egui::Window<'open> {
    let room = room_on(ctx);
    let too_tall = ctx
        .memory(|memory| memory.area_rect(id))
        .is_some_and(|rect| rect.height() >= room.y);
    let window = window.id(id).max_width(room.x).constrain(true);
    if too_tall {
        window
            .default_height(room.y)
            .max_height(room.y)
            .vscroll(true)
    } else {
        window
    }
}

pub(crate) fn box_width(ctx: &egui::Context, wanted: f32) -> f32 {
    let spacing = &ctx.global_style().spacing;
    let frame = spacing
        .window_margin
        .sum()
        .x
        .max(spacing.menu_margin.sum().x)
        + 2.0;
    wanted.min((room_on(ctx).x - frame).max(120.0))
}
