use std::collections::BTreeMap;

use eframe::egui;
use pdf_app::wording::{Command, Message};

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
        let mut open = true;
        crate::side_panel::side_panel(ctx, Message::Command(Command::Ocr).say(lang), "web-ocr-panel", WIDTH)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(crate::side_panel::box_width(ui.ctx(), WIDTH));
                let weak = ui.visuals().weak_text_color();
                ui.label(
                    egui::RichText::new(
                        "Reads scanned pages on this device with Tesseract, a small open-source model: \
                         it can misread, so check the text. The reader and the languages you tick are \
                         downloaded from panpdf.org and are gone when you leave the page.",
                    )
                    .size(11.5)
                    .color(weak),
                );
                ui.separator();
                ui.label("Languages of the text");
                ui.horizontal_wrapped(|ui| {
                    let mut drop = None;
                    for code in &ocr.ticked {
                        let name = languages
                            .iter()
                            .find(|(c, _, _)| c == code)
                            .map_or(code.as_str(), |(_, n, _)| n.as_str());
                        if ui.small_button(format!("{name}  \u{00d7}")).clicked() {
                            drop = Some(code.clone());
                        }
                    }
                    if let Some(code) = drop {
                        ocr.ticked.retain(|c| *c != code);
                    }
                });
                ui.add(
                    egui::TextEdit::singleline(&mut ocr.search)
                        .hint_text("Search languages")
                        .desired_width(f32::INFINITY),
                );
                let words: Vec<String> = ocr
                    .search
                    .to_lowercase()
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect();
                egui::ScrollArea::vertical().max_height(190.0).show(ui, |ui| {
                    if languages.is_empty() {
                        ui.label(egui::RichText::new("The list of languages is loading\u{2026}").color(weak));
                    }
                    for (code, name, mb) in &languages {
                        let haystack = format!("{} {}", name.to_lowercase(), code);
                        if !words.iter().all(|w| haystack.contains(w.as_str())) {
                            continue;
                        }
                        let mut on = ocr.ticked.contains(code);
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut on, name.as_str()).changed() {
                                ocr.ticked.retain(|c| c != code);
                                if on {
                                    ocr.ticked.push(code.clone());
                                }
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(format!("{mb} MB")).size(11.0).color(weak));
                            });
                        });
                    }
                });
                let mb: f64 = languages
                    .iter()
                    .filter(|(code, _, _)| ocr.ticked.contains(code))
                    .filter_map(|(_, _, mb)| mb.parse::<f64>().ok())
                    .sum();
                ui.label(
                    egui::RichText::new(format!(
                        "{} ticked \u{00b7} {mb:.1} MB to download. Tick only the languages in the document: \
                         one it does not have makes reading slower and can make it worse.",
                        ocr.ticked.len()
                    ))
                    .size(11.0)
                    .color(weak),
                );
                ui.separator();
                ui.horizontal(|ui| {
                    ui.radio_value(&mut ocr.this_page, true, "This page");
                    ui.radio_value(&mut ocr.this_page, false, "Every page");
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
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(said);
                    });
                }
                if let Some(why) = &ocr.trouble {
                    ui.colored_label(ui.visuals().error_fg_color, why);
                }
                ui.horizontal(|ui| {
                    if ocr.run.is_some() {
                        stop = ui.button("Stop").clicked();
                    } else {
                        read = ui
                            .add_enabled(!ocr.ticked.is_empty(), egui::Button::new("Read the text"))
                            .clicked();
                    }
                    close = ui.button("Close").clicked();
                });
            });
        if read {
            self.start_reading_in_the_page();
        }
        if stop || close || !open {
            if let Some(ocr) = self.web_ocr.as_mut() {
                ocr.run = None;
            }
        }
        if close || !open {
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
