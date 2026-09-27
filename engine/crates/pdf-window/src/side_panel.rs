use eframe::egui;

const START: egui::Vec2 = egui::vec2(-12.0, 56.0);

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

#[cfg(test)]
mod tests {
    use eframe::egui;

    use super::side_panel;

    fn frame(ctx: &egui::Context, events: Vec<egui::Event>) -> egui::Rect {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            events,
            ..Default::default()
        };
        let mut placed = egui::Rect::NOTHING;
        let _ = ctx.run_ui(input, |ui| {
            if let Some(shown) =
                side_panel(ui.ctx(), "Watermark", "test-panel", 300.0).show(ui.ctx(), |ui| {
                    ui.label("text");
                })
            {
                placed = shown.response.rect;
            }
        });
        placed
    }

    fn press(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn a_tool_window_opens_at_the_top_right_and_can_be_dragged() {
        let ctx = egui::Context::default();
        frame(&ctx, Vec::new());
        let at = frame(&ctx, Vec::new());
        assert!((at.right() - 1188.0).abs() < 1.0, "{at:?}");
        assert!((at.top() - 56.0).abs() < 1.0, "{at:?}");

        let grab = egui::pos2(at.center().x, at.top() + 8.0);
        let to = grab + egui::vec2(-400.0, 200.0);
        frame(&ctx, vec![egui::Event::PointerMoved(grab)]);
        frame(&ctx, vec![press(grab, true)]);
        for step in 1..=10_u8 {
            let t = f32::from(step) / 10.0;
            frame(
                &ctx,
                vec![egui::Event::PointerMoved(grab + (to - grab) * t)],
            );
        }
        frame(&ctx, vec![press(to, false)]);
        let moved = frame(&ctx, Vec::new());
        assert!(
            (moved.left() - (at.left() - 400.0)).abs() < 1.0,
            "{at:?} -> {moved:?}"
        );
        assert!(
            (moved.top() - (at.top() + 200.0)).abs() < 1.0,
            "{at:?} -> {moved:?}"
        );
    }
}
