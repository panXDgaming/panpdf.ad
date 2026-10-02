use std::sync::atomic::Ordering;

use eframe::egui;

use pdf_app::wording::{Lang, Message, Phone};

use super::{Asked, Pressed, creep, fetch_share, the_recogniser_itself, what_stands_in_the_way};
use crate::window_state::{OcrDraft, OcrWhich};

const LIST_HEIGHT: f32 = 220.0;

pub(super) fn panel(
    ui: &mut egui::Ui,
    draft: &mut OcrDraft,
    (lang, pages, now): (Lang, Option<&Result<Vec<usize>, Message>>, f64),
    asked: &mut Asked,
) {
    ui.spacing_mut().item_spacing.y = 10.0;
    let weak = ui.visuals().weak_text_color();
    let wide = ui.available_width();
    if running(ui, draft, (lang, now), asked) {
        return;
    }
    ui.label(
        egui::RichText::new(Phone::OcrAbout.say(lang))
            .size(14.0)
            .color(weak),
    );
    if draft.engine.is_none() {
        the_recogniser_itself(ui, draft, lang, asked);
    }
    choices(ui, draft, lang, asked);
    what_stands_in_the_way(ui, draft, lang, pages);
    let count = pages
        .and_then(|pages| pages.as_ref().ok())
        .map_or(0, Vec::len);
    let ready = draft.engine.is_some() && count > 0 && !draft.chosen().is_empty();
    let to_fetch: u64 = draft
        .chosen()
        .iter()
        .filter(|code| !draft.here.contains(code))
        .filter_map(|code| pdf_ocr::models::model(code, draft.choice.quality))
        .map(|model| model.bytes)
        .sum();
    ui.add_space(4.0);
    let read = egui::Button::new(
        egui::RichText::new(Phone::ReadTheText.say(lang))
            .size(16.0)
            .strong()
            .color(ui.visuals().selection.stroke.color),
    )
    .fill(ui.visuals().selection.bg_fill)
    .corner_radius(10.0);
    let pressed = ui
        .add_enabled_ui(ready, |ui| ui.add_sized([wide, 46.0], read).clicked())
        .inner;
    if pressed {
        asked.pressed = Pressed::Read;
    }
    if to_fetch > 0 {
        ui.label(
            egui::RichText::new(Phone::DownloadsOnce(to_fetch).say(lang))
                .size(13.0)
                .color(weak),
        );
    }
    let close = egui::Button::new(egui::RichText::new(Message::Close.say(lang)).size(15.0));
    if ui.add_sized([wide, 40.0], close.frame(false)).clicked() {
        asked.pressed = Pressed::Close;
    }
}

fn stop_button(ui: &mut egui::Ui, lang: Lang, asked: &mut Asked) {
    let stop = egui::Button::new(egui::RichText::new(Message::OcrStop.say(lang)).size(16.0));
    if ui.add_sized([ui.available_width(), 44.0], stop).clicked() {
        asked.pressed = Pressed::Stop;
    }
}

fn running(
    ui: &mut egui::Ui,
    draft: &OcrDraft,
    (lang, now): (Lang, f64),
    asked: &mut Asked,
) -> bool {
    let weak = ui.visuals().weak_text_color();
    if let Some(reading) = &draft.reading {
        let (done, total) = (reading.read.len(), reading.pages.len());
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(22.0));
            let said = Phone::ReadingPage {
                page: (done + 1).min(total),
                of: total,
            };
            ui.label(egui::RichText::new(said.say(lang)).size(16.0));
        });
        ui.add(
            egui::ProgressBar::new(creep(reading, now))
                .desired_height(10.0)
                .corner_radius(5.0),
        );
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "whole seconds since the reading began"
        )]
        let seconds = (now - reading.began).max(0.0) as u64;
        ui.label(
            egui::RichText::new(Phone::SecondsIn { seconds }.say(lang))
                .size(13.0)
                .color(weak),
        );
        stop_button(ui, lang, asked);
        return true;
    }
    if let Some(fetch) = &draft.fetching {
        let said = match &fetch.code {
            Some(code) => Message::OcrGettingModel(code.clone()),
            None => Message::OcrGettingEngine,
        };
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(22.0));
            ui.label(egui::RichText::new(said.say(lang)).size(16.0));
        });
        let bar = match fetch_share(fetch) {
            Some(share) => egui::ProgressBar::new(share),
            None => egui::ProgressBar::new(0.0).animate(true),
        };
        ui.add(bar.desired_height(10.0).corner_radius(5.0));
        if fetch.total > 0 {
            let seen = fetch.seen.load(Ordering::Relaxed);
            let sizes = Phone::BytesOf {
                done: seen,
                of: fetch.total,
            };
            ui.label(egui::RichText::new(sizes.say(lang)).size(13.0).color(weak));
        }
        stop_button(ui, lang, asked);
        return true;
    }
    false
}

fn choices(ui: &mut egui::Ui, draft: &mut OcrDraft, lang: Lang, asked: &mut Asked) {
    let weak = ui.visuals().weak_text_color();
    let name_of = |code: &str| Message::OcrLanguage(code.to_owned()).say(lang);
    ui.label(
        egui::RichText::new(Phone::Language.say(lang))
            .size(15.0)
            .strong(),
    );
    ui.horizontal_wrapped(|ui| {
        let mut taken_off = None;
        for code in &draft.wanted {
            let chip = egui::Button::new(
                egui::RichText::new(format!("{}   \u{00d7}", name_of(code))).size(15.0),
            )
            .corner_radius(18.0)
            .min_size(egui::vec2(0.0, 36.0));
            if ui.add(chip).clicked() {
                taken_off = Some(code.clone());
            }
        }
        if let Some(code) = taken_off {
            draft.wanted.retain(|wanted| *wanted != code);
            asked.remember = true;
        }
        let add = if draft.adding {
            Phone::DoneAdding
        } else {
            Phone::AddALanguage
        };
        let add = egui::Button::new(
            egui::RichText::new(add.say(lang))
                .size(15.0)
                .color(ui.visuals().selection.stroke.color),
        )
        .frame(false)
        .min_size(egui::vec2(0.0, 36.0));
        if ui.add(add).clicked() {
            draft.adding = !draft.adding;
            draft.search.clear();
        }
    });
    if draft.adding {
        adding(ui, draft, lang, asked);
    }
    ui.label(
        egui::RichText::new(Phone::Pages.say(lang))
            .size(15.0)
            .strong(),
    );
    if draft.which == OcrWhich::Some {
        draft.which = OcrWhich::ThisPage;
    }
    ui.horizontal(|ui| {
        let half = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
        for (which, label) in [
            (OcrWhich::ThisPage, Message::OcrThisPage.say(lang)),
            (OcrWhich::All, Phone::EveryPage.say(lang)),
        ] {
            let button = egui::Button::new(egui::RichText::new(label).size(15.0))
                .selected(draft.which == which)
                .corner_radius(10.0)
                .min_size(egui::vec2(half, 40.0));
            if ui.add(button).clicked() {
                draft.which = which;
            }
        }
    });
    let _ = weak;
}

fn adding(ui: &mut egui::Ui, draft: &mut OcrDraft, lang: Lang, asked: &mut Asked) {
    let weak = ui.visuals().weak_text_color();
    ui.add(
        egui::TextEdit::singleline(&mut draft.search)
            .hint_text(Phone::SearchLanguages.say(lang))
            .font(egui::FontId::proportional(15.0))
            .desired_width(f32::INFINITY)
            .margin(egui::vec2(8.0, 8.0)),
    );
    let found: Vec<pdf_app::ocr_languages::Language> =
        pdf_app::ocr_languages::search(&draft.list, &draft.search)
            .into_iter()
            .filter(|language| language.reads)
            .copied()
            .collect();
    egui::ScrollArea::vertical()
        .id_salt("ocr-languages")
        .max_height(LIST_HEIGHT)
        .show(ui, |ui| {
            for language in &found {
                let code = language.code;
                let size = if draft.here.iter().any(|here| here == code) {
                    Phone::OnThisPhone.say(lang)
                } else {
                    pdf_ocr::models::model(code, draft.choice.quality)
                        .map(|model| Message::OcrSize(model.bytes).say(lang))
                        .unwrap_or_default()
                };
                ui.horizontal(|ui| {
                    ui.set_min_height(36.0);
                    let mut on = draft.wanted.iter().any(|wanted| wanted == code);
                    let name = Message::OcrLanguage(code.to_owned()).say(lang);
                    if ui
                        .checkbox(&mut on, egui::RichText::new(name).size(15.0))
                        .changed()
                    {
                        draft.wanted.retain(|wanted| wanted != code);
                        if on {
                            draft.wanted.push(code.to_owned());
                        }
                        asked.remember = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(size).size(13.0).color(weak));
                    });
                });
            }
        });
    ui.label(
        egui::RichText::new(Phone::TickOnlyWhatIsThere.say(lang))
            .size(13.0)
            .color(weak),
    );
}
