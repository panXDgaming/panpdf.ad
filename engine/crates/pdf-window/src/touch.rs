use eframe::egui;
use pdf_app::view::Quad;

use crate::window_state::{Carrying, Drag, Pointing, Window};

pub(crate) const LONG_PRESS: f64 = 0.5;

const FLING_START: f32 = 50.0;

const EDGE_FRAMES: u8 = 3;

const FLING_FASTEST: f32 = 8000.0;

const PHYSICAL: f64 = 9.806_65 * 39.37 * 160.0 * 0.84;
const SCROLL_FRICTION: f64 = 0.015;
const INFLEXION: f64 = 0.35;

fn deceleration_rate() -> f64 {
    0.78_f64.ln() / 0.9_f64.ln()
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Fling {
    from: egui::Vec2,
    began: f64,
    way: egui::Vec2,
    distance: f32,
    seconds: f64,
    held: u8,
}

impl Fling {
    pub(crate) fn of(speed: egui::Vec2, from: egui::Vec2, now: f64) -> Option<Self> {
        let length = speed.length().min(FLING_FASTEST);
        if length < FLING_START {
            return None;
        }
        let base = SCROLL_FRICTION * PHYSICAL;
        let rate = deceleration_rate();
        let l = (INFLEXION * f64::from(length) / base).ln();
        #[allow(clippy::cast_possible_truncation)]
        let distance = (base * (rate / (rate - 1.0) * l).exp()) as f32;
        Some(Self {
            from,
            began: now,
            way: speed.normalized(),
            distance,
            seconds: (l / (rate - 1.0)).exp(),
            held: 0,
        })
    }

    fn at(&self, now: f64) -> (egui::Vec2, bool) {
        let t = ((now - self.began) / self.seconds).clamp(0.0, 1.0);
        #[allow(clippy::cast_possible_truncation)]
        let gone = self.distance * spline_position(t as f32);
        (self.from - self.way * gone, t < 1.0)
    }
}

fn spline_position(t: f32) -> f32 {
    const P1: f32 = 0.175;
    const P2: f32 = 0.35;
    const START_TENSION: f32 = 0.5;
    if t >= 1.0 {
        return 1.0;
    }
    let (mut low, mut high) = (0.0_f32, 1.0_f32);
    let mut x = 0.5;
    for _ in 0..32 {
        x = f32::midpoint(low, high);
        let coef = 3.0 * x * (1.0 - x);
        let tx = coef * ((1.0 - x) * P1 + x * P2) + x * x * x;
        if (tx - t).abs() < 1e-5 {
            break;
        }
        if tx > t {
            high = x;
        } else {
            low = x;
        }
    }
    let coef = 3.0 * x * (1.0 - x);
    coef * ((1.0 - x) * START_TENSION + x) + x * x * x
}

#[cfg(target_os = "android")]
const THUMB_WIDE: f32 = 6.0;
#[cfg(target_os = "android")]
const THUMB_LENGTH: f32 = 48.0;
#[cfg(target_os = "android")]
const THUMB_INSET: f32 = 3.0;
#[cfg(target_os = "android")]
const THUMB_REACH: f32 = 36.0;

const END_GRIP: f32 = 11.0;
const END_REACH: f32 = 28.0;
const END_BELOW: f32 = 0.2;

#[derive(Clone, Copy, Debug)]
pub(crate) struct EndHandle {
    stop: usize,
    grip: egui::Pos2,
    em: f32,
}

const CARET_GRIP: f32 = 10.0;
const CARET_REACH: f32 = 30.0;

pub(crate) fn a_swipe(ctx: &egui::Context) -> bool {
    ctx.input(|input| {
        input.any_touches()
            && input
                .pointer
                .press_start_time()
                .is_some_and(|began| input.time - began < LONG_PRESS)
    })
}

impl Window {
    pub(crate) fn pan_from(&mut self, at: egui::Pos2) {
        self.drag = Some(Drag {
            what: Carrying::Panning { was: self.offset },
            page: self.focus,
            from: at,
            to: at,
            started_at: Quad::of([0.0; 4]),
            straight: false,
        });
    }

    pub(crate) fn on_the_choice(&self, page: usize, point: (f64, f64)) -> bool {
        let Some(overlay) = self.overlay(page) else {
            return false;
        };
        let quad = match self.pointing {
            Pointing::Block { page: on, block } if on == page => {
                overlay.blocks.get(block).map(|block| block.quad)
            }
            Pointing::Object { page: on, object } if on == page => {
                overlay.objects.get(object).map(|object| object.quad)
            }
            _ => None,
        };
        quad.is_some_and(|quad| Quad::from_pixels(quad).contains(point))
    }

    pub(crate) fn two_fingers(&mut self, ctx: &egui::Context) -> bool {
        let Some(touch) = ctx.input(egui::InputState::multi_touch) else {
            return false;
        };
        if !matches!(
            self.drag,
            Some(Drag {
                what: Carrying::NewLine,
                ..
            })
        ) {
            self.drag = None;
        }
        self.reading = None;
        self.pinching = true;
        let between = touch.center_pos - self.view_corner;
        if (touch.zoom_delta - 1.0).abs() > 1e-4 {
            self.zoom_to(self.zoom * f64::from(touch.zoom_delta), Some(between));
        }
        if touch.translation_delta != egui::Vec2::ZERO {
            let from = self.wanted_offset.unwrap_or(self.offset);
            self.wanted_offset = Some(from - touch.translation_delta);
        }
        ctx.request_repaint();
        true
    }

    pub(crate) fn still_pinching(&mut self, ctx: &egui::Context) -> bool {
        if self.pinching && !ctx.input(egui::InputState::any_touches) {
            self.pinching = false;
        }
        self.pinching
    }

    pub(crate) fn follow_the_finger(&mut self, ctx: &egui::Context) {
        let Some(Drag {
            what: Carrying::Panning { was },
            from,
            ..
        }) = self.drag
        else {
            return;
        };
        let at = ctx.input(|input| {
            input
                .pointer
                .primary_down()
                .then(|| input.pointer.interact_pos())
                .flatten()
        });
        let Some(at) = at else {
            return;
        };
        let wanted = was - (at - from);
        let kept = self.within_the_pages(wanted);
        if kept != wanted
            && let Some(Drag {
                what: Carrying::Panning { was },
                ..
            }) = self.drag.as_mut()
        {
            *was += kept - wanted;
        }
        self.wanted_offset = Some(kept);
    }

    pub(crate) fn start_fling(&mut self, ctx: &egui::Context) {
        if !self.touched || self.pinching {
            return;
        }
        let (speed, now) = ctx.input(|input| (input.pointer.velocity(), input.time));
        let from = self.wanted_offset.unwrap_or(self.offset);
        self.fling = Fling::of(speed, from, now);
    }

    pub(crate) fn keep_flinging(&mut self, ctx: &egui::Context) {
        let Some(fling) = self.fling else {
            return;
        };
        let (down, now) = ctx.input(|input| (input.pointer.any_down(), input.time));
        if down {
            self.fling = None;
            return;
        }
        let (at, going) = fling.at(now);
        let kept = self.within_the_pages(at);
        self.wanted_offset = Some(kept);
        let blocked =
            |axis: usize| (kept[axis] - at[axis]).abs() > 0.5 || fling.way[axis].abs() < 0.2;
        let held = if blocked(0) && blocked(1) {
            fling.held + 1
        } else {
            0
        };
        if going && held < EDGE_FRAMES {
            self.fling = Some(Fling { held, ..fling });
            ctx.request_repaint();
        } else {
            self.fling = None;
        }
    }

    #[cfg(target_os = "android")]
    pub(crate) fn scroll_thumb(&mut self, ui: &egui::Ui, view: egui::Rect, content: egui::Vec2) {
        let reach = content.y - view.height();
        if reach <= 1.0 || view.height() < 4.0 * THUMB_LENGTH {
            self.dragging_thumb = false;
            return;
        }
        let track = view.shrink2(egui::vec2(0.0, THUMB_INSET));
        let length = (track.height() * view.height() / content.y).max(THUMB_LENGTH);
        let travel = (track.height() - length).max(1.0);
        let along = (self.offset.y / reach).clamp(0.0, 1.0) * travel;
        let thumb = egui::Rect::from_min_size(
            egui::pos2(
                track.right() - THUMB_WIDE - THUMB_INSET,
                track.top() + along,
            ),
            egui::vec2(THUMB_WIDE, length),
        );
        let grip = egui::Rect::from_min_max(
            egui::pos2(track.right() - THUMB_REACH, thumb.top()),
            egui::pos2(track.right(), thumb.bottom()),
        );
        let response = ui.interact(grip, ui.id().with("scroll-thumb"), egui::Sense::drag());
        if response.drag_started() {
            self.fling = None;
        }
        self.dragging_thumb = response.dragged();
        if self.dragging_thumb {
            let moved = response.drag_delta().y * reach / travel;
            let from = self.wanted_offset.unwrap_or(self.offset);
            self.wanted_offset = Some(egui::vec2(from.x, (from.y + moved).clamp(0.0, reach)));
            ui.ctx().request_repaint();
        }
        let painter = ui.painter_at(view);
        let (colour, shape) = if self.dragging_thumb {
            (
                egui::Color32::from_rgb(0, 90, 200),
                thumb.expand2(egui::vec2(1.5, 0.0)),
            )
        } else {
            (egui::Color32::from_black_alpha(110), thumb)
        };
        painter.rect_filled(shape, THUMB_WIDE, colour);
        if self.dragging_thumb {
            let pages = self.editor.strip().pages();
            let text = format!("{} / {pages}", self.focus + 1);
            let font = egui::FontId::proportional(15.0);
            let galley = painter.layout_no_wrap(text, font, egui::Color32::WHITE);
            let bubble = egui::Rect::from_center_size(
                egui::pos2(
                    thumb.left() - 16.0 - galley.size().x / 2.0 - 12.0,
                    thumb.center().y,
                ),
                galley.size() + egui::vec2(24.0, 14.0),
            );
            painter.rect_filled(
                bubble,
                bubble.height() / 2.0,
                egui::Color32::from_rgb(48, 49, 54),
            );
            painter.galley(
                bubble.center() - galley.size() / 2.0,
                galley,
                egui::Color32::WHITE,
            );
        }
    }

    pub(crate) fn draw_caret_handle(
        &self,
        painter: &egui::Painter,
        foot: egui::Pos2,
        head: egui::Pos2,
    ) {
        if !self.touched {
            return;
        }
        let colour = egui::Color32::from_rgb(0, 90, 200);
        let below = foot + (foot - head) * 0.2;
        let centre = below + egui::vec2(0.0, CARET_GRIP * 1.35);
        let side = CARET_GRIP * 0.72;
        painter.add(egui::Shape::convex_polygon(
            vec![
                below,
                centre + egui::vec2(side, -side),
                centre + egui::vec2(-side, -side),
            ],
            colour,
            egui::Stroke::NONE,
        ));
        painter.circle_filled(centre, CARET_GRIP, colour);
        self.caret_handle.set(Some(centre));
    }

    pub(crate) fn draw_selection_handles(
        &self,
        painter: &egui::Painter,
        ends: [(usize, egui::Pos2, f32); 2],
    ) {
        if !self.touched {
            return;
        }
        let colour = egui::Color32::from_rgb(0, 90, 200);
        let mut handles = [EndHandle {
            stop: 0,
            grip: egui::Pos2::ZERO,
            em: 1.0,
        }; 2];
        for (slot, (side, (stop, foot, em))) in [-1.0_f32, 1.0].into_iter().zip(ends).enumerate() {
            let em = em.max(1.0);
            let tip = foot + egui::vec2(0.0, END_BELOW * em);
            let grip = tip + egui::vec2(side * END_GRIP, END_GRIP);
            painter.circle_filled(grip, END_GRIP, colour);
            painter.rect_filled(egui::Rect::from_two_pos(tip, grip), 0.0, colour);
            handles[slot] = EndHandle { stop, grip, em };
        }
        self.selection_handles.set(Some(handles));
    }

    pub(crate) fn carry_the_caret(&mut self, response: &egui::Response) -> bool {
        if response.drag_started()
            && let (Some(grip), Some(at)) =
                (self.caret_handle.get(), response.interact_pointer_pos())
            && grip.distance(at) <= CARET_REACH
        {
            self.carrying_caret = true;
            self.carrying_end = None;
        }
        if response.drag_started() && !self.carrying_caret {
            self.take_an_end(response);
        }
        if !self.carrying_caret {
            return false;
        }
        if !response.dragged() {
            self.carrying_caret = false;
            self.carrying_end = None;
            return true;
        }
        let Pointing::Text { page, block, .. } = self.pointing else {
            self.carrying_caret = false;
            return true;
        };
        let lift = self
            .caret_shown
            .get()
            .map_or(CARET_GRIP, |caret| caret.height() * 0.7);
        let Some(at) = response.interact_pointer_pos() else {
            return true;
        };
        let aim = match (self.carrying_end, self.selection_handles.get()) {
            (Some(last), Some(handles)) => {
                let side = if last { 1.0 } else { -1.0 };
                let em = handles[usize::from(last)].em;
                at - egui::vec2(side * END_GRIP, END_GRIP + (END_BELOW + 0.5) * em)
            }
            _ => at - egui::vec2(0.0, CARET_GRIP * 2.4 + lift),
        };
        let stop = self
            .laid
            .iter()
            .find(|laid| laid.page == page)
            .and_then(|laid| {
                let point = laid.placed.point_in_page((aim.x, aim.y))?;
                let overlay = self.overlay(page)?;
                let owner = overlay.blocks.get(block)?;
                pdf_app::view::caret_at_in(&overlay.carets, &owner.lines, point)
            });
        if let Some(stop) = stop {
            let anchor = match (self.carrying_end, self.pointing) {
                (Some(_), Pointing::Text { caret, .. }) => caret.anchor,
                _ => stop,
            };
            self.pointing = Pointing::Text {
                page,
                block,
                caret: crate::window_state::Caret { at: stop, anchor },
            };
        }
        true
    }

    fn take_an_end(&mut self, response: &egui::Response) {
        let (Some(handles), Some(origin)) = (
            self.selection_handles.get(),
            response
                .ctx
                .input(|input| input.pointer.press_origin())
                .or_else(|| response.interact_pointer_pos()),
        ) else {
            return;
        };
        let Pointing::Text { page, block, .. } = self.pointing else {
            return;
        };
        let near = |handle: &EndHandle| handle.grip.distance(origin);
        let last = near(&handles[1]) < near(&handles[0]);
        let (held, other) = (handles[usize::from(last)], handles[usize::from(!last)]);
        if near(&held) > END_REACH {
            return;
        }
        self.pointing = Pointing::Text {
            page,
            block,
            caret: crate::window_state::Caret {
                at: held.stop,
                anchor: other.stop,
            },
        };
        self.carrying_caret = true;
        self.carrying_end = Some(last);
    }

    pub(crate) fn keep_panning(&mut self, response: &egui::Response) -> bool {
        let Some(Drag {
            what: Carrying::Panning { was },
            from,
            ..
        }) = self.drag
        else {
            return false;
        };
        if let Some(at) = response.interact_pointer_pos() {
            self.wanted_offset = Some(was - (at - from));
        }
        if response.drag_stopped() || !response.dragged() {
            self.drag = None;
        }
        true
    }

    pub(crate) fn keep_the_caret_in_sight(&mut self, ctx: &egui::Context) {
        const AIR: f32 = 24.0;
        let Some(caret) = self.caret_shown.get() else {
            self.caret_sighted = None;
            return;
        };
        if !self.touched || self.viewing || self.view.y <= 0.0 {
            return;
        }
        let in_document = caret.translate(self.offset);
        let now = (in_document, self.view.y);
        let moved = self.caret_sighted.is_none_or(|(was, height)| {
            (was.min - in_document.min).length() > 0.5 || (height - self.view.y).abs() > 0.5
        });
        self.caret_sighted = Some(now);
        if !moved || self.wanted_offset.is_some() {
            return;
        }
        let top = self.view_corner.y + AIR;
        let bottom = self.view_corner.y + self.view.y
            - crate::text::docked_bar_height(self.view.x, self.touched)
            - crate::text::CARET_HANDLE_ROOM
            - AIR;
        let by = if caret.max.y > bottom {
            caret.max.y - bottom
        } else if caret.min.y < top {
            caret.min.y - top
        } else {
            return;
        };
        let mut offset = self.offset;
        offset.y = (offset.y + by).max(0.0);
        self.wanted_offset = Some(offset);
        ctx.request_repaint();
    }

    pub(crate) fn ask_for_the_keyboard(&self, ctx: &egui::Context) {
        if cfg!(target_os = "android") {
            return;
        }
        if !self.touched
            || self.viewing
            || !(matches!(self.pointing, Pointing::Text { .. }) || self.text_draft.is_some())
        {
            return;
        }
        let at = self
            .caret_shown
            .get()
            .unwrap_or_else(|| egui::Rect::from_min_size(self.view_corner, egui::vec2(1.0, 1.0)));
        ctx.output_mut(|output| {
            output.ime = Some(egui::output::IMEOutput {
                rect: at,
                cursor_rect: at,
                should_interrupt_composition: false,
            });
        });
    }

    pub(crate) fn fit_a_narrow_view(&mut self) {
        if self.view.x <= 0.0 || self.pages_flowing {
            return;
        }
        let turned = self.fitted.is_some_and(|(width, zoom)| {
            (zoom - self.zoom).abs() < 1e-9 && (width - self.view.x).abs() > 1.0
        });
        if !self.fit_on_open && !turned {
            return;
        }
        self.fit_on_open = false;
        let (width, _) = self.editor.strip().size();
        if width <= 0.0 {
            return;
        }
        let fit = fit_zoom(self.view.x, width);
        let wanted = if self.touched {
            fit
        } else {
            fit.min(self.zoom)
        };
        if (wanted - self.zoom).abs() > 1e-6 {
            self.zoom_to(wanted, Some(egui::Vec2::ZERO));
            if !turned {
                self.wanted_offset = Some(egui::Vec2::ZERO);
            }
        }
        self.fitted = self.touched.then_some((self.view.x, self.zoom));
    }
}

fn fit_zoom(view: f32, width: f64) -> f64 {
    let room = f64::from(view - 2.0 * crate::canvas::DESK_MARGIN).max(1.0);
    room / width
}

#[cfg(test)]
mod tests {
    use super::{Fling, fit_zoom, spline_position};
    use eframe::egui;

    #[test]
    fn a_fling_goes_as_far_and_as_long_as_androids() {
        let fling = Fling::of(egui::vec2(0.0, -2000.0), egui::Vec2::ZERO, 0.0).unwrap();
        assert!((fling.distance - 648.0).abs() < 5.0, "{}", fling.distance);
        assert!((fling.seconds - 0.925).abs() < 0.01, "{}", fling.seconds);
        let fast = Fling::of(egui::vec2(5000.0, 0.0), egui::Vec2::ZERO, 0.0).unwrap();
        assert!((fast.distance - 3170.0).abs() < 20.0, "{}", fast.distance);
        let (end, going) = fling.at(10.0);
        assert!(!going);
        assert!((end.y - fling.distance).abs() < 1e-3, "{end:?}");
        assert!(Fling::of(egui::vec2(30.0, 0.0), egui::Vec2::ZERO, 0.0).is_none());
    }

    #[test]
    fn the_spline_runs_from_rest_to_rest_without_going_back() {
        assert!(spline_position(0.0).abs() < 1e-3);
        assert!((spline_position(1.0) - 1.0).abs() < 1e-6);
        let mut was = 0.0;
        for step in 1..=100 {
            #[allow(clippy::cast_precision_loss)]
            let now = spline_position(step as f32 / 100.0);
            assert!(now >= was - 1e-4, "{step}: {now} < {was}");
            was = now;
        }
        assert!(spline_position(0.25) > 0.5);
    }

    #[test]
    fn a_phone_fits_a_page_width_with_the_margins_left_over() {
        let zoom = fit_zoom(412.0, 595.0);
        assert!((595.0 * zoom - (412.0 - 32.0)).abs() < 1e-6, "{zoom}");
        assert!(zoom < 1.0);
    }

    #[test]
    fn a_wide_view_would_zoom_closer_and_is_left_alone_by_the_caller() {
        assert!(fit_zoom(1920.0, 595.0) > 1.0);
    }
}
