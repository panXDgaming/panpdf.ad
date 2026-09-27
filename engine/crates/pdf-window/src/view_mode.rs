use eframe::egui;

use pdf_app::wording::{Command, Message};

use crate::window_state::{Tool, Window};

pub(crate) const PICTURE_DPI: f64 = 300.0;

const MOST_PICTURE_PIXELS: f64 = 6000.0;

impl ReadingEnd {
    #[cfg(target_os = "android")]
    pub(crate) fn band(&self) -> egui::Rect {
        egui::Rect::from_min_max(
            egui::pos2(self.tip.x, self.tip.y - self.em * (1.0 + TIP_BELOW)),
            self.tip,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReadingEnd {
    stop: usize,
    tip: egui::Pos2,
    grip: egui::Pos2,
    em: f32,
}

const STILL: f32 = 8.0;

const HANDLE_RADIUS: f32 = 11.0;

const TIP_BELOW: f32 = 0.2;

const HANDLE_REACH: f32 = 28.0;

const MENU_LIFT: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PictureMenu {
    pub(crate) at: egui::Pos2,
    pub(crate) page: usize,
    pub(crate) object: usize,
}

pub(crate) fn picture_png(
    view: &pdf_session::PageView,
    box_pixels: [f64; 4],
) -> Result<Vec<u8>, String> {
    let (width, height) = (box_pixels[2] - box_pixels[0], box_pixels[3] - box_pixels[1]);
    if width <= 0.0 || height <= 0.0 {
        return Err("the picture has no size on the page".to_owned());
    }
    let wanted = PICTURE_DPI / 72.0 / pdf_app::document::OVERLAY_SCALE;
    let scale = wanted.min(MOST_PICTURE_PIXELS / width.max(height));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let region = [
        (box_pixels[0] * scale).round().max(0.0) as u32,
        (box_pixels[1] * scale).round().max(0.0) as u32,
        (box_pixels[2] * scale).round().max(0.0) as u32,
        (box_pixels[3] * scale).round().max(0.0) as u32,
    ];
    let options = pdf_render::RenderOptions {
        scale: scale * pdf_app::document::OVERLAY_SCALE,
        ..pdf_render::RenderOptions::default()
    };
    let (canvas, _) =
        pdf_render::render_region_layers(&view.layers(), &view.program.geometry, options, region)
            .map_err(|error| error.to_string())?;
    let metre = pdf_edit::png::per_metre(scale * pdf_app::document::OVERLAY_SCALE * 72.0);
    pdf_edit::png::write(
        (canvas.width, canvas.height),
        &canvas.to_rgb8(),
        Some((metre, metre)),
    )
    .map_err(str::to_owned)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Reading {
    pub(crate) page: usize,
    pub(crate) anchor: usize,
    pub(crate) caret: usize,
}

impl Window {
    pub(crate) fn edit_when_asked(&mut self) {
        if self.viewing && (self.tool != Tool::Select || self.stamp_draft.is_some()) {
            self.viewing = false;
            self.reading = None;
        }
    }

    pub(crate) fn view_pointer(&mut self, ctx: &egui::Context, response: &egui::Response) {
        let was_panning = matches!(
            self.drag,
            Some(crate::window_state::Drag {
                what: crate::window_state::Carrying::Panning { .. },
                ..
            })
        );
        if self.keep_panning(response) {
            if was_panning && response.drag_stopped() {
                self.start_fling(ctx);
            }
            return;
        }
        if self.carry_an_end(ctx, response) {
            return;
        }
        self.view_cursor(ctx, response);
        if ctx.input(|input| input.pointer.any_pressed()) {
            self.pressed_word = false;
        }
        self.rest_on_a_word(ctx);
        if response.secondary_clicked()
            && let Some(at) = response.interact_pointer_pos()
            && let Some((page, point)) = self.page_point(at)
        {
            self.picture_menu =
                self.picture_at(page, point)
                    .map(|object| PictureMenu { at, page, object });
            return;
        }
        if response.clicked() {
            self.picture_menu = None;
        }
        if response.clicked()
            && !self.editor.is_busy()
            && let Some(at) = response.interact_pointer_pos()
            && let Some((page, point)) = self.page_point(at)
            && let Some(link) = self.editor.link_at(page, point)
        {
            self.follow(&link);
            return;
        }
        if self.clicked_a_field(ctx, response) {
            return;
        }
        if response.drag_started() {
            let from = ctx
                .input(|input| input.pointer.press_origin())
                .or_else(|| response.interact_pointer_pos());
            if crate::touch::a_swipe(ctx) {
                if let Some(at) = from {
                    self.pan_from(at);
                }
                return;
            }
            if !self.pressed_word {
                self.reading = from.and_then(|at| {
                    let (page, point) = self.page_point(at)?;
                    let stop = pdf_app::view::reading_caret_at(&self.overlay(page)?.carets, point)?;
                    Some(Reading {
                        page,
                        anchor: stop,
                        caret: stop,
                    })
                });
            }
        } else if response.dragged()
            && let Some(reading) = self.reading
            && let Some(at) = response.interact_pointer_pos()
            && let Some(laid) = self.laid.iter().find(|laid| laid.page == reading.page)
            && let Some(point) = laid.placed.point_in_page((at.x, at.y))
            && let Some(stop) = self
                .overlay(reading.page)
                .and_then(|overlay| pdf_app::view::reading_caret_at(&overlay.carets, point))
        {
            self.reading = Some(Reading {
                caret: stop,
                ..reading
            });
        } else if response.clicked() && !self.pressed_word {
            self.reading = None;
        }
        if let Some(at) = response.interact_pointer_pos()
            && let Some((page, point)) = self.page_point(at)
        {
            if response.triple_clicked() {
                self.read_around(page, point, true);
            } else if response.double_clicked() {
                self.read_around(page, point, false);
            }
        }
    }

    fn read_around(&mut self, page: usize, point: (f64, f64), whole_row: bool) {
        let Some(overlay) = self.overlay(page) else {
            return;
        };
        let Some(stop) = pdf_app::view::reading_caret_at(&overlay.carets, point)
            .and_then(|index| overlay.carets.get(index))
        else {
            return;
        };
        let line = stop.line;
        let span = if whole_row {
            pdf_app::view::row_at(&overlay.carets, line)
        } else {
            pdf_app::view::word_at(&overlay.clusters, line, stop.offset)
        };
        let Some((from, to)) = span else {
            return;
        };
        if let (Some(anchor), Some(caret)) = (
            pdf_app::view::stop_at(&overlay.carets, line, from),
            pdf_app::view::stop_at(&overlay.carets, line, to),
        ) {
            self.reading = Some(Reading {
                page,
                anchor,
                caret,
            });
        }
    }

    fn rest_on_a_word(&mut self, ctx: &egui::Context) {
        let (touch, began, origin, now_at, time) = ctx.input(|input| {
            (
                input.any_touches(),
                input.pointer.press_start_time(),
                input.pointer.press_origin(),
                input.pointer.interact_pos(),
                input.time,
            )
        });
        if !touch || self.pressed_word || self.drag.is_some() {
            return;
        }
        let (Some(began), Some(origin)) = (began, origin) else {
            return;
        };
        if now_at.is_some_and(|at| at.distance(origin) > STILL) {
            return;
        }
        let held = time - began;
        if held < crate::touch::LONG_PRESS {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                crate::touch::LONG_PRESS - held,
            ));
            return;
        }
        if let Some((page, point)) = self.page_point(origin) {
            self.read_around(page, point, false);
            self.pressed_word = true;
        }
    }

    pub(crate) fn reading_ends(&self) -> Option<(usize, [ReadingEnd; 2])> {
        let reading = self.reading?;
        let overlay = self.overlay(reading.page)?;
        let laid = self.laid.iter().find(|laid| laid.page == reading.page)?;
        let corner = egui::pos2(laid.placed.origin.0, laid.placed.origin.1);
        let end = |index: usize, side: f32| {
            let (top, foot) = laid.placed.caret_line(overlay.carets.get(index)?);
            let em = (foot[1] - top[1]).abs().max(1.0);
            let tip = corner + egui::vec2(foot[0], foot[1] + TIP_BELOW * em);
            Some(ReadingEnd {
                stop: index,
                tip,
                grip: tip + egui::vec2(side * HANDLE_RADIUS, HANDLE_RADIUS),
                em,
            })
        };
        let (first, last) = if reading.anchor <= reading.caret {
            (reading.anchor, reading.caret)
        } else {
            (reading.caret, reading.anchor)
        };
        Some((reading.page, [end(first, -1.0)?, end(last, 1.0)?]))
    }

    fn carry_an_end(&mut self, ctx: &egui::Context, response: &egui::Response) -> bool {
        if response.drag_started() && self.touched {
            let origin = ctx.input(|input| input.pointer.press_origin());
            if let (Some(origin), Some((page, ends))) = (origin, self.reading_ends()) {
                let [first, last] = ends;
                let near = |end: &ReadingEnd| end.grip.distance(origin);
                let (held, other) = if near(&first) <= near(&last) {
                    (first, last)
                } else {
                    (last, first)
                };
                if near(&held) <= HANDLE_REACH {
                    self.reading = Some(Reading {
                        page,
                        anchor: other.stop,
                        caret: held.stop,
                    });
                    self.carrying_an_end = true;
                    return true;
                }
            }
        }
        if !self.carrying_an_end {
            return false;
        }
        if response.dragged()
            && let Some(reading) = self.reading
            && let Some(at) = response.interact_pointer_pos()
            && let Some(laid) = self.laid.iter().find(|laid| laid.page == reading.page)
        {
            let em = self
                .reading_ends()
                .map_or(HANDLE_RADIUS, |(_, ends)| ends[1].em);
            let aim = at
                - egui::vec2(
                    Self::reading_side(reading) * HANDLE_RADIUS,
                    HANDLE_RADIUS + (TIP_BELOW + 0.5) * em,
                );
            if let Some(point) = laid.placed.point_in_page((aim.x, aim.y))
                && let Some(stop) = self
                    .overlay(reading.page)
                    .and_then(|overlay| pdf_app::view::reading_caret_at(&overlay.carets, point))
            {
                self.reading = Some(Reading {
                    caret: stop,
                    ..reading
                });
            }
        }
        if !response.dragged() {
            self.carrying_an_end = false;
        }
        true
    }

    const fn reading_side(reading: Reading) -> f32 {
        if reading.caret < reading.anchor {
            -1.0
        } else {
            1.0
        }
    }

    pub(crate) fn draw_reading_handles(&self, painter: &egui::Painter) {
        if !self.touched {
            return;
        }
        let Some((_, ends)) = self.reading_ends() else {
            return;
        };
        let colour = egui::Color32::from_rgb(0, 90, 200);
        for end in ends {
            painter.circle_filled(end.grip, HANDLE_RADIUS, colour);
            painter.rect_filled(egui::Rect::from_two_pos(end.tip, end.grip), 0.0, colour);
        }
    }

    pub(crate) fn reading_menu(&mut self, ctx: &egui::Context) {
        if cfg!(target_os = "android") || !self.touched || !self.viewing || self.carrying_an_end {
            return;
        }
        if ctx.input(|input| input.pointer.any_down()) {
            return;
        }
        let Some((_, [first, _])) = self.reading_ends() else {
            return;
        };
        let screen = ctx.content_rect();
        let at = egui::pos2(
            (first.tip.x - MENU_LIFT)
                .clamp(screen.min.x + 4.0, (screen.max.x - 200.0).max(screen.min.x)),
            (first.tip.y - first.em * (1.0 + TIP_BELOW) - MENU_LIFT - 40.0).max(screen.min.y + 4.0),
        );
        let lang = self.lang;
        let (mut copy, mut all) = (false, false);
        let word = |ui: &mut egui::Ui, command: Command| {
            ui.add(
                egui::Button::new(
                    egui::RichText::new(Message::Command(command).say(lang))
                        .size(15.0)
                        .color(egui::Color32::WHITE),
                )
                .frame(false)
                .min_size(egui::vec2(48.0, 32.0)),
            )
            .clicked()
        };
        egui::Area::new(egui::Id::new("reading-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(egui::Color32::from_rgb(48, 49, 54))
                    .corner_radius(20.0)
                    .inner_margin(egui::Margin::symmetric(12, 4))
                    .shadow(ui.style().visuals.popup_shadow)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 12.0;
                            copy = word(ui, Command::Copy);
                            all = word(ui, Command::SelectAll);
                        });
                    });
            });
        if copy {
            self.copy_reading(ctx);
            self.reading = None;
        } else if all {
            self.read_the_whole_page();
        }
    }

    fn view_cursor(&mut self, ctx: &egui::Context, response: &egui::Response) {
        let Some((page, point)) = response.hover_pos().and_then(|at| self.page_point(at)) else {
            return;
        };
        if self.editor.link_at(page, point).is_some() {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            return;
        }
        let over = |quad: &[f64; 4]| {
            point.0 >= quad[0] && point.0 <= quad[2] && point.1 >= quad[1] && point.1 <= quad[3]
        };
        if self
            .overlay(page)
            .is_some_and(|overlay| overlay.blocks.iter().any(|block| over(&block.box_pixels)))
        {
            ctx.set_cursor_icon(egui::CursorIcon::Text);
        }
    }

    pub(crate) fn reading_spans(&self, page: usize) -> Vec<(usize, usize, usize)> {
        let (Some(reading), Some(overlay)) = (self.reading, self.overlay(page)) else {
            return Vec::new();
        };
        if reading.page != page {
            return Vec::new();
        }
        let rows = pdf_app::view::page_rows(&overlay.carets);
        pdf_app::view::selection_rows(&overlay.carets, &rows, reading.anchor, reading.caret)
    }

    pub(crate) fn copy_reading(&mut self, ctx: &egui::Context) -> bool {
        let Some(reading) = self.reading else {
            return false;
        };
        let spans = self.reading_spans(reading.page);
        let Some(overlay) = self.overlay(reading.page) else {
            return false;
        };
        let text = pdf_app::view::text_of_spans(&overlay.clusters, &spans);
        if text.is_empty() {
            return false;
        }
        ctx.copy_text(text);
        true
    }

    pub(crate) fn read_the_whole_page(&mut self) {
        let page = self.focus;
        let Some(overlay) = self.overlay(page) else {
            return;
        };
        if overlay.carets.is_empty() {
            return;
        }
        self.reading = Some(Reading {
            page,
            anchor: 0,
            caret: overlay.carets.len() - 1,
        });
    }

    fn picture_at(&self, page: usize, point: (f64, f64)) -> Option<usize> {
        self.overlay(page)?.objects.iter().rposition(|object| {
            matches!(object.kind, pdf_semantics::ObjectKind::Image)
                && point.0 >= object.box_pixels[0]
                && point.0 <= object.box_pixels[2]
                && point.1 >= object.box_pixels[1]
                && point.1 <= object.box_pixels[3]
        })
    }

    pub(crate) fn picture_menu(&mut self, ctx: &egui::Context) {
        let Some(menu) = self.picture_menu else {
            return;
        };
        let label = Message::Command(Command::SavePicture).say(self.lang);
        let mut save = false;
        let area = egui::Area::new(egui::Id::new("picture-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(menu.at)
            .show(ctx, |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    save = ui.button(label).clicked();
                });
            });
        let pressed_elsewhere = ctx.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|at| !area.response.rect.contains(at))
        });
        if save {
            self.picture_menu = None;
            self.save_picture(menu.page, menu.object);
        } else if pressed_elsewhere || ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.picture_menu = None;
        }
    }

    fn save_picture(&mut self, page: usize, object: usize) {
        let Some(leaf) = self.editor.leaf(page).cloned() else {
            return;
        };
        let Some(found) = leaf.overlay.objects.get(object) else {
            return;
        };
        match picture_png(&leaf.view, found.box_pixels) {
            Ok(png) => {
                #[cfg(target_arch = "wasm32")]
                if let Err(why) = crate::web_files::download("picture.png", &png) {
                    self.editor.say(Message::Refused(why.into()));
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let folder = self.opened.parent().map(std::path::Path::to_path_buf);
                    self.chooser = Some(crate::chooser::Chooser::saving_as(
                        folder.as_deref(),
                        pdf_app::files::named_for_pages(&self.opened, &[page])
                            .trim_end_matches(".pdf"),
                        1,
                        "png",
                    ));
                    self.choosing_for = crate::page_actions::Choosing::PictureOut(png);
                }
            }
            Err(why) => self.editor.say(Message::Refused(why.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::picture_png;

    fn page_with_a_picture() -> pdf_bytes::ByteStore {
        let content = b"q 72 0 0 72 100 100 cm /Im1 Do Q";
        let samples = [255u8, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0];
        let mut pdf = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        let mut object = |pdf: &mut Vec<u8>, body: &[u8]| {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
            pdf.extend_from_slice(body);
            pdf.extend_from_slice(b"\nendobj\n");
        };
        object(&mut pdf, b"<< /Type /Catalog /Pages 2 0 R >>");
        object(&mut pdf, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
        object(&mut pdf, b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Contents 4 0 R /Resources << /XObject << /Im1 5 0 R >> >> >>");
        let mut body = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        body.extend_from_slice(content);
        body.extend_from_slice(b"\nendstream");
        object(&mut pdf, &body);
        let mut body = format!("<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length {} >>\nstream\n", samples.len()).into_bytes();
        body.extend_from_slice(&samples);
        body.extend_from_slice(b"\nendstream");
        object(&mut pdf, &body);
        let start = pdf.len();
        pdf.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes(),
        );
        for offset in &offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n",
                offsets.len() + 1
            )
            .as_bytes(),
        );
        pdf_bytes::ByteStore::new(pdf_bytes::SourceId::new(95), Arc::<[u8]>::from(pdf))
    }

    #[test]
    fn a_picture_is_saved_as_the_page_shows_it_at_300_dpi() {
        let source = page_with_a_picture();
        let view = pdf_session::interpret_page_for_display(&source, 0, b"", None, None).unwrap();
        let png = picture_png(&view, [100.0, 128.0, 172.0, 200.0]).unwrap();
        let read = pdf_edit::image_file::ImageFile::read(&png).unwrap();
        let (across, down) = read.dpi().unwrap();
        assert!((across - 300.0).abs() < 1.0 && (down - 300.0).abs() < 1.0);
        let picture = read.thumbnail(1000).unwrap();
        assert_eq!((picture.width, picture.height), (300, 300));
        let middle = ((150 * 300 + 150) * 4) as usize;
        assert_eq!(&picture.rgba[middle..middle + 3], &[255, 0, 0]);
        assert!(picture_png(&view, [10.0, 10.0, 10.0, 20.0]).is_err());
    }
}
