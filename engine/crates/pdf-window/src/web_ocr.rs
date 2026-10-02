use std::collections::BTreeMap;

use eframe::egui;
use pdf_app::wording::{Command, Message};

use crate::dialog;
use crate::web_files::OcrAnswer;
use crate::window_state::{PageRead, Window};

#[derive(Default)]
pub(crate) struct WebOcr {
    ticked: Vec<String>,
    search: String,
    this_page: bool,
    run: Option<Run>,
    trouble: Option<String>,
}

struct Run {
    pages: Vec<usize>,
    next: usize,
    sets: Vec<Vec<String>>,
    waiting: Option<Waiting>,
    read: BTreeMap<usize, PageRead>,
    fetching: Option<String>,
}

struct Waiting {
    id: u32,
    page: usize,
    set: usize,
    pgm: Vec<u8>,
    scale: f64,
    shown_height: f64,
}

fn sets_for(ticked: &[String]) -> Vec<Vec<String>> {
    let has = |code: &str| ticked.iter().any(|t| t == code);
    if has("lao") && has("tha") {
        let without = |code: &str| ticked.iter().filter(|t| *t != code).cloned().collect();
        vec![without("tha"), without("lao")]
    } else {
        vec![ticked.to_vec()]
    }
}

fn joined(sets: &[Vec<String>]) -> String {
    sets.iter()
        .map(|set| set.join("+"))
        .collect::<Vec<_>>()
        .join(",")
}

static NEXT_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

const WIDTH: f32 = 320.0;

impl Window {
    pub(crate) fn open_the_web_ocr_panel(&mut self) {
        if self.web_ocr.is_none() {
            self.web_ocr = Some(WebOcr {
                ticked: vec!["eng".to_owned()],
                ..WebOcr::default()
            });
        }
    }

    pub(crate) fn web_ocr_panel(&mut self, ctx: &egui::Context) {
        if self.web_ocr.is_none() || !self.has_document() {
            return;
        }
        self.keep_reading_in_the_page();
        let languages = crate::web_files::ocr_language_list();
        let lang = self.lang;
        let Some(ocr) = self.web_ocr.as_mut() else {
            return;
        };
        let (mut read, mut stop, mut close) = (false, false, false);
        let canvas = self.canvas;
        let title = Message::Command(Command::RecognizeText).say(lang);
        let shut = Message::Close.say(lang);
        let spec = crate::dialog::Spec {
            id: "web-ocr-panel",
            width: WIDTH,
        };
        crate::dialog::panel(ctx, canvas, &spec, |ui, room| {
            close = dialog::tool_header(
                ui,
                title.trim_end_matches('\u{2026}'),
                Some(
                    "Reads scanned pages on this device. It can misread, so check the text. \
                     The languages you tick are downloaded once, from panpdf.org.",
                ),
                Some(&shut),
                lang,
            );
            let list = (room * 0.45).clamp(90.0, 190.0);
            dialog::body(ui, |ui| {
                dialog::scrolling(ui, "web-ocr-body", room, |ui| {
                    dialog::caption(ui, "Languages of the text");
                    if !ocr.ticked.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            let mut drop = None;
                            for code in &ocr.ticked {
                                let name = languages
                                    .iter()
                                    .find(|(c, _, _)| c == code)
                                    .map_or(code.as_str(), |(_, n, _)| n.as_str());
                                let chip = egui::Button::new(
                                    egui::RichText::new(format!("{name}  \u{00d7}")).size(12.0),
                                )
                                .fill(ui.visuals().selection.bg_fill)
                                .corner_radius(10);
                                if ui.add(chip).on_hover_text("Take it off").clicked() {
                                    drop = Some(code.clone());
                                }
                            }
                            if let Some(code) = drop {
                                ocr.ticked.retain(|c| *c != code);
                            }
                        });
                        ui.add_space(6.0);
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut ocr.search)
                            .hint_text("Search languages")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(4.0);
                    let words: Vec<String> = ocr
                        .search
                        .to_lowercase()
                        .split_whitespace()
                        .map(str::to_owned)
                        .collect();
                    egui::ScrollArea::vertical()
                        .id_salt("web-ocr-languages")
                        .max_height(list)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if languages.is_empty() {
                                dialog::small(ui, "The list of languages is loading\u{2026}");
                            }
                            let mut shown = 0;
                            for (code, name, mb) in &languages {
                                let haystack = format!("{} {}", name.to_lowercase(), code);
                                if !words.iter().all(|w| haystack.contains(w.as_str())) {
                                    continue;
                                }
                                shown += 1;
                                let mut on = ocr.ticked.contains(code);
                                ui.horizontal(|ui| {
                                    if ui.checkbox(&mut on, name.as_str()).changed() {
                                        ocr.ticked.retain(|c| c != code);
                                        if on {
                                            ocr.ticked.push(code.clone());
                                        }
                                    }
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                egui::RichText::new(format!("{mb} MB"))
                                                    .size(11.0)
                                                    .color(dialog::weak(ui)),
                                            );
                                        },
                                    );
                                });
                            }
                            if shown == 0 && !languages.is_empty() {
                                dialog::small(ui, "No language matches that.");
                            }
                        });
                    let mb: f64 = languages
                        .iter()
                        .filter(|(code, _, _)| ocr.ticked.contains(code))
                        .filter_map(|(_, _, mb)| mb.parse::<f64>().ok())
                        .sum();
                    ui.add_space(4.0);
                    dialog::small(
                        ui,
                        &format!(
                            "{} ticked \u{00b7} {mb:.1} MB. Tick only the languages the document is in: \
                         another one makes reading slower and can make it worse.",
                            ocr.ticked.len()
                        ),
                    );
                    dialog::divide(ui);
                    dialog::caption(ui, "Pages");
                    let _ = dialog::segments(
                        ui,
                        "web-ocr-pages",
                        &mut ocr.this_page,
                        &[
                            (true, "This page".to_owned()),
                            (false, "Every page".to_owned()),
                        ],
                    );
                    if let Some(why) = &ocr.trouble {
                        ui.add_space(8.0);
                        dialog::note(ui, dialog::Tone::Trouble, why);
                    }
                });
            });
            if let Some(run) = &ocr.run {
                let said = match (&run.fetching, &run.waiting) {
                    (Some(what), _) if what == "reader" => "Getting the reader\u{2026}".to_owned(),
                    (Some(code), _) => format!("Getting the {code} language file\u{2026}"),
                    (None, Some(waiting)) => format!(
                        "Reading page {} \u{00b7} {} of {}",
                        waiting.page + 1,
                        run.next.min(run.pages.len()),
                        run.pages.len()
                    ),
                    (None, None) => "Looking at the pages\u{2026}".to_owned(),
                };
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    dialog::spinner(ui);
                    ui.label(egui::RichText::new(said).size(12.0));
                });
            }
            dialog::body(ui, |ui| {
                dialog::footer(ui, |ui| {
                    if ocr.run.is_some() {
                        stop = dialog::primary(ui, "Stop", true).clicked();
                    } else {
                        read =
                            dialog::primary(ui, "Read the text", !ocr.ticked.is_empty()).clicked();
                    }
                    close |= dialog::secondary(ui, &shut).clicked();
                });
            });
        });
        if read {
            self.start_reading_in_the_page();
        }
        if stop || close {
            if let Some(ocr) = self.web_ocr.as_mut() {
                ocr.run = None;
            }
        }
        if close {
            self.web_ocr = None;
        }
    }

    fn start_reading_in_the_page(&mut self) {
        let count = self.editor.page_count();
        let focus = self.focus;
        let Some(ocr) = self.web_ocr.as_mut() else {
            return;
        };
        let pages = if ocr.this_page {
            vec![focus.min(count.saturating_sub(1))]
        } else {
            (0..count).collect()
        };
        ocr.trouble = None;
        ocr.run = Some(Run {
            pages,
            next: 0,
            sets: sets_for(&ocr.ticked),
            waiting: None,
            read: BTreeMap::new(),
            fetching: None,
        });
    }

    fn keep_reading_in_the_page(&mut self) {
        while let Some(answer) = crate::web_files::take_ocr_answer() {
            let Some(run) = self.web_ocr.as_mut().and_then(|ocr| ocr.run.as_mut()) else {
                continue;
            };
            match answer {
                OcrAnswer::Step(what) => run.fetching = Some(what),
                OcrAnswer::Failed { id, error } => {
                    if run.waiting.as_ref().is_some_and(|w| w.id == id) {
                        if let Some(ocr) = self.web_ocr.as_mut() {
                            ocr.run = None;
                            ocr.trouble = Some(format!("Nothing was read: {error}"));
                        }
                    }
                }
                OcrAnswer::Read { id, tsv, text } => {
                    let Some(waiting) = run.waiting.take_if(|w| w.id == id) else {
                        continue;
                    };
                    run.fetching = None;
                    let outcome = match pdf_ocr::tsv::read(&tsv, &text) {
                        Err(error) => PageRead::Failed(error.to_string()),
                        Ok(lines) => {
                            if run.sets.len() == 2
                                && waiting.set == 0
                                && pdf_ocr::is_not_lao(&lines)
                            {
                                let again = Waiting {
                                    id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                                    set: 1,
                                    ..waiting
                                };
                                if crate::web_files::ask_ocr(
                                    again.id,
                                    &again.pgm,
                                    &run.sets[1].join("+"),
                                    &joined(&run.sets),
                                ) {
                                    run.waiting = Some(again);
                                    continue;
                                }
                                PageRead::Failed("the page's reader is not there".to_owned())
                            } else {
                                PageRead::Read(pdf_ocr::reading(
                                    &lines,
                                    waiting.scale,
                                    waiting.shown_height,
                                ))
                            }
                        }
                    };
                    run.read.insert(waiting.page, outcome);
                }
            }
        }
        let (source, credential, fonts) = (
            self.editor.source().cloned(),
            self.editor.credential().to_vec(),
            self.editor.fonts(),
        );
        let Some(run) = self.web_ocr.as_mut().and_then(|ocr| ocr.run.as_mut()) else {
            return;
        };
        let Some(source) = source else {
            return;
        };
        while run.waiting.is_none() && run.next < run.pages.len() {
            let page = run.pages[run.next];
            run.next += 1;
            let view = match pdf_session::interpret_page_for_display(
                &source,
                page,
                &credential,
                None,
                fonts.clone(),
            ) {
                Ok(view) => view,
                Err(error) => {
                    run.read.insert(page, PageRead::Failed(error.to_string()));
                    continue;
                }
            };
            if !crate::ocr_tool::is_a_scan(&view) {
                run.read.insert(page, PageRead::HadText);
                continue;
            }
            let image = match pdf_ocr::page_image(&view.layers(), &view.program.geometry) {
                Ok(image) => image,
                Err(error) => {
                    run.read.insert(page, PageRead::Failed(error.to_string()));
                    continue;
                }
            };
            let waiting = Waiting {
                id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                page,
                set: 0,
                pgm: image.grey.pgm(),
                scale: image.scale,
                shown_height: image.shown_height,
            };
            if !crate::web_files::ask_ocr(
                waiting.id,
                &waiting.pgm,
                &run.sets[0].join("+"),
                &joined(&run.sets),
            ) {
                run.read.insert(
                    page,
                    PageRead::Failed("the page's reader is not there".to_owned()),
                );
                continue;
            }
            run.waiting = Some(waiting);
        }
        if run.waiting.is_none() && run.next >= run.pages.len() {
            let read = std::mem::take(&mut run.read);
            if let Some(ocr) = self.web_ocr.as_mut() {
                ocr.run = None;
            }
            self.write_what_was_read(read);
        }
    }
}
