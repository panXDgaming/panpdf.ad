use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};

use eframe::egui;

use pdf_app::ocr_choice::Choice;
use pdf_app::wording::{Command, Done, Message};
use pdf_edit::stamp::Only;
use pdf_ocr::Quality;
use pdf_paint::PaintAtomKind;

use crate::window_state::{OcrDraft, OcrFetch, OcrReading, OcrWhich, PageRead, Window};

const PANEL_WIDTH: f32 = 340.0;

const MOST_WORKERS: usize = 4;

const KNOWN: [&str; 3] = ["lao", "tha", "eng"];

const SIMPLE: bool = cfg!(target_os = "android");

fn choice_file() -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("state"))
        })?;
    Some(state.join("panpdf").join("ocr"))
}

fn remembered() -> Choice {
    choice_file()
        .and_then(|file| std::fs::read_to_string(file).ok())
        .and_then(|text| pdf_app::ocr_choice::read(&text))
        .unwrap_or_else(Choice::fresh)
}

fn remember(choice: Choice) {
    let Some(file) = choice_file() else {
        return;
    };
    if let Some(folder) = file.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let temporary = file.with_extension("new");
    if std::fs::write(&temporary, pdf_app::ocr_choice::write(&choice)).is_ok() {
        let _ = std::fs::rename(&temporary, &file);
    }
}

fn draft() -> OcrDraft {
    let choice = remembered();
    let mut engine = pdf_ocr::Tesseract::locate().ok();
    if let Some(engine) = engine.as_mut() {
        engine.own = pdf_ocr::store::models_dir(choice.quality);
    }
    let mut draft = OcrDraft {
        engine,
        languages: KNOWN
            .iter()
            .map(|code| ((*code).to_owned(), true))
            .collect(),
        here: Vec::new(),
        choice,
        which: OcrWhich::default(),
        range: String::new(),
        reading: None,
        fetching: None,
        trouble: None,
        read_when_fetched: false,
        search: String::new(),
        adding: false,
    };
    if SIMPLE {
        let mut every: Vec<&str> = pdf_ocr::models::every_language()
            .into_iter()
            .filter(|code| *code != "osd")
            .collect();
        every.sort_by_key(|code| {
            (
                *code != "eng",
                pdf_ocr::names::name_of(code).unwrap_or(*code),
            )
        });
        draft.languages = every
            .into_iter()
            .map(|code| (code.to_owned(), code == "eng"))
            .collect();
        draft.choice.quality = Quality::Fast;
        draft.choice.skip_text = true;
    }
    draft.take_stock();
    draft
}

impl OcrDraft {
    fn take_stock(&mut self) {
        if let Some(engine) = self.engine.as_mut() {
            engine.own = pdf_ocr::store::models_dir(self.choice.quality);
        }
        let system = self
            .engine
            .as_ref()
            .and_then(|engine| engine.installed_languages().ok())
            .unwrap_or_default();
        let own = pdf_ocr::store::models_dir(self.choice.quality);
        self.here = self
            .languages
            .iter()
            .map(|(code, _)| code)
            .filter(|code| {
                system.iter().any(|have| have == *code)
                    || own.as_deref().is_some_and(|dir| {
                        pdf_ocr::models::model(code, self.choice.quality)
                            .is_some_and(|model| pdf_ocr::store::have(dir, model))
                    })
            })
            .cloned()
            .collect();
        if SIMPLE {
            return;
        }
        for (code, chosen) in &mut self.languages {
            if !self.here.contains(code) {
                *chosen = false;
            }
        }
        if self.here.iter().all(|code| {
            !self
                .languages
                .iter()
                .any(|(name, chosen)| name == code && *chosen)
        }) {
            for (code, chosen) in &mut self.languages {
                if self.here.contains(code) {
                    *chosen = true;
                }
            }
        }
    }

    fn chosen(&self) -> Vec<String> {
        self.languages
            .iter()
            .filter(|(_, chosen)| *chosen)
            .map(|(code, _)| code.clone())
            .collect()
    }
}

fn has_text(view: &pdf_session::PageView) -> bool {
    view.graph
        .atoms
        .iter()
        .any(|atom| matches!(atom.kind, PaintAtomKind::Text(_)))
}

pub(crate) fn is_a_scan(view: &pdf_session::PageView) -> bool {
    !has_text(view)
        && view
            .graph
            .atoms
            .iter()
            .any(|atom| matches!(atom.kind, PaintAtomKind::Image(_)))
}

fn read_one(
    source: &pdf_bytes::ByteStore,
    page: usize,
    (credential, fonts): (&[u8], Option<Arc<dyn pdf_content::FontProvider>>),
    (engine, languages, skip_text): (&pdf_ocr::Tesseract, &[String], bool),
    cancel: &AtomicBool,
) -> PageRead {
    let view = match pdf_session::interpret_page_for_display(source, page, credential, None, fonts)
    {
        Ok(view) => view,
        Err(error) => return PageRead::Failed(error.to_string()),
    };
    if skip_text && has_text(&view) {
        return PageRead::HadText;
    }
    match pdf_ocr::read_page(
        &view.layers(),
        &view.program.geometry,
        engine,
        languages,
        cancel,
    ) {
        Ok(reading) => PageRead::Read(reading),
        Err(error) => PageRead::Failed(error.to_string()),
    }
}

impl Window {
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn scan_notice(&mut self, _ctx: &egui::Context, _area: egui::Rect) {}

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn scan_notice(&mut self, ctx: &egui::Context, area: egui::Rect) {
        if self.ocr_draft.is_some()
            || !matches!(self.pointing, crate::window_state::Pointing::Nothing)
            || self.editor.is_busy()
            || self.scan_notice_shut.as_ref() == Some(&self.opened)
            || !self
                .editor
                .leaf(self.focus)
                .is_some_and(|leaf| is_a_scan(&leaf.view))
        {
            return;
        }
        let lang = self.lang;
        let (mut read, mut shut) = (false, false);
        egui::Area::new(egui::Id::new("scan-notice"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::CENTER_TOP)
            .fixed_pos(area.center_top() + egui::vec2(0.0, 10.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width((area.width() - 32.0).max(160.0));
                    ui.horizontal_wrapped(|ui| {
                        if SIMPLE {
                            ui.label("This page is a scan");
                        } else {
                            ui.label(Message::ThisPageIsAScan.say(lang));
                        }
                        let label = if SIMPLE {
                            "Read the text".to_owned()
                        } else {
                            Message::Command(Command::RecognizeText).say(lang)
                        };
                        let button = egui::Button::new(
                            egui::RichText::new(label.trim_end_matches('\u{2026}'))
                                .color(ui.visuals().selection.stroke.color),
                        )
                        .fill(ui.visuals().selection.bg_fill.gamma_multiply(0.4));
                        read = ui.add(button).clicked();
                        shut = ui.small_button("\u{d7}").clicked();
                    });
                });
            });
        if read {
            self.open_the_ocr_panel();
        }
        if shut {
            self.scan_notice_shut = Some(self.opened.clone());
        }
    }

    pub(crate) fn open_the_ocr_panel(&mut self) {
        if self
            .ocr_draft
            .as_ref()
            .is_some_and(|draft| draft.reading.is_some())
        {
            return;
        }
        self.ocr_draft = Some(draft());
    }

    fn ocr_pages(&self, draft: &OcrDraft) -> Result<Vec<usize>, Message> {
        let count = self.editor.page_count();
        match draft.which {
            OcrWhich::ThisPage => {
                pdf_edit::stamp::pages_of(&(self.focus + 1).to_string(), count, Only::Every)
            }
            OcrWhich::All => pdf_edit::stamp::pages_of("", count, Only::Every),
            OcrWhich::Some => pdf_edit::stamp::pages_of(&draft.range, count, Only::Every),
        }
        .map_err(|error| Message::Refused(error.to_string().into()))
    }

    pub(crate) fn ocr_panel(&mut self, ctx: &egui::Context) {
        if self.ocr_draft.is_none() || !self.has_document() {
            return;
        }
        let now = ctx.input(|input| input.time);
        self.keep_reading(now);
        self.keep_fetching(ctx);
        if self
            .ocr_draft
            .as_ref()
            .is_some_and(|draft| draft.reading.is_some() || draft.fetching.is_some())
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        let lang = self.lang;
        let count = self.editor.page_count();
        let pages = self.ocr_draft.as_ref().map(|draft| self.ocr_pages(draft));
        let Some(draft) = self.ocr_draft.as_mut() else {
            return;
        };
        let mut asked = Asked::default();
        let title = if SIMPLE {
            Message::Command(Command::Ocr).say(lang)
        } else {
            Message::Command(Command::RecognizeText).say(lang)
        };
        crate::side_panel::side_panel(
            ctx,
            title.trim_end_matches('\u{2026}'),
            "ocr-panel",
            PANEL_WIDTH,
        )
        .show(ctx, |ui| {
            ui.set_width(crate::side_panel::box_width(ui.ctx(), PANEL_WIDTH));
            if SIMPLE {
                simple_panel(ui, draft, (lang, pages.as_ref(), now), &mut asked);
                return;
            }
            let why = if SIMPLE {
                "Reads scanned pages on this phone with Tesseract, a small open-source model: it \
                 can misread, so check the text. The languages you tick are downloaded once and \
                 kept on the phone; nothing else is sent anywhere."
                    .to_owned()
            } else {
                Message::OcrWhy.say(lang)
            };
            ui.label(
                egui::RichText::new(why)
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.separator();
            the_choices(ui, draft, (lang, count), &mut asked);
            what_stands_in_the_way(ui, draft, lang, pages.as_ref());
            ui.separator();
            how_far(ui, draft, lang, now);
            the_buttons(ui, draft, (lang, pages.as_ref()), &mut asked);
        });
        if asked.pressed == Pressed::Stop {
            if let Some(reading) = draft.reading.as_ref() {
                reading.cancel.store(true, Ordering::Relaxed);
            }
            if let Some(fetch) = draft.fetching.as_ref() {
                fetch.cancel.store(true, Ordering::Relaxed);
            }
        }
        if asked.quality != draft.choice.quality {
            draft.choice.quality = asked.quality;
            draft.take_stock();
            asked.remember = true;
        }
        if asked.remember {
            remember(draft.choice);
        }
        match asked.pressed {
            Pressed::Close => self.ocr_draft = None,
            Pressed::GetModel(code) => self.fetch_model(ctx, &code),
            Pressed::GetEngine => self.fetch_engine(ctx),
            Pressed::Read if SIMPLE => self.read_or_fetch(ctx),
            Pressed::Read => self.start_reading(ctx),
            Pressed::Nothing | Pressed::Stop => {}
        }
    }

    fn fetch_model(&mut self, ctx: &egui::Context, code: &str) {
        let Some(draft) = self.ocr_draft.as_mut() else {
            return;
        };
        let (Some(model), Some(dir)) = (
            pdf_ocr::models::model(code, draft.choice.quality),
            pdf_ocr::store::models_dir(draft.choice.quality),
        ) else {
            draft.trouble = Some(Message::OcrModelFailed(
                pdf_ocr::FetchError::NoHome.to_string(),
            ));
            return;
        };
        draft.trouble = None;
        let (cancel, seen) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicU64::new(0)),
        );
        let (send, answer) = mpsc::channel();
        let (worker_cancel, worker_seen, ctx) =
            (Arc::clone(&cancel), Arc::clone(&seen), ctx.clone());
        let worker = std::thread::spawn(move || {
            let landed = pdf_ocr::store::fetch(
                model,
                &dir,
                &|bytes| {
                    worker_seen.store(bytes, Ordering::Relaxed);
                    ctx.request_repaint();
                },
                &worker_cancel,
            );
            let _ = send.send(landed.map(|_| ()).map_err(|why| why.to_string()));
            ctx.request_repaint();
        });
        draft.fetching = Some(OcrFetch {
            code: Some(code.to_owned()),
            seen,
            total: model.bytes,
            cancel,
            answer,
            worker: Some(worker),
        });
    }

    fn fetch_engine(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.ocr_draft.as_mut() else {
            return;
        };
        let Some(into) = pdf_ocr::store::own_engine() else {
            draft.trouble = Some(Message::OcrEngineFailed(
                pdf_ocr::FetchError::NoHome.to_string(),
            ));
            return;
        };
        draft.trouble = None;
        let (cancel, seen) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicU64::new(0)),
        );
        let (send, answer) = mpsc::channel();
        let (worker_cancel, ctx) = (Arc::clone(&cancel), ctx.clone());
        let worker = std::thread::spawn(move || {
            let done = pdf_ocr::setup::install(&into, &|_| {}, &worker_cancel);
            let _ = send.send(done.map(|_| ()).map_err(|why| why.to_string()));
            ctx.request_repaint();
        });
        draft.fetching = Some(OcrFetch {
            code: None,
            seen,
            total: 0,
            cancel,
            answer,
            worker: Some(worker),
        });
    }

    fn keep_fetching(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.ocr_draft.as_mut() else {
            return;
        };
        let Some(fetch) = draft.fetching.as_mut() else {
            return;
        };
        let Ok(answer) = fetch.answer.try_recv() else {
            return;
        };
        let was_a_model = fetch.code.is_some();
        if let Some(worker) = fetch.worker.take() {
            let _ = worker.join();
        }
        draft.fetching = None;
        draft.trouble = match answer {
            Ok(()) => None,
            Err(why) if why == pdf_ocr::FetchError::Cancelled.to_string() => None,
            Err(why) if was_a_model => Some(Message::OcrModelFailed(why)),
            Err(why) => Some(Message::OcrEngineFailed(why)),
        };
        if !was_a_model {
            let mut engine = pdf_ocr::Tesseract::locate().ok();
            if let Some(engine) = engine.as_mut() {
                engine.own = pdf_ocr::store::models_dir(draft.choice.quality);
            }
            draft.engine = engine;
        }
        draft.take_stock();
        ctx.request_repaint();
        if draft.read_when_fetched {
            draft.read_when_fetched = false;
            if draft.trouble.is_none() {
                self.read_or_fetch(ctx);
            }
        }
    }

    fn read_or_fetch(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.ocr_draft.as_mut() else {
            return;
        };
        let missing = draft
            .chosen()
            .into_iter()
            .find(|code| !draft.here.contains(code));
        match missing {
            Some(code) => {
                draft.read_when_fetched = true;
                self.fetch_model(ctx, &code);
            }
            None => self.start_reading(ctx),
        }
    }

    fn start_reading(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.ocr_draft.as_ref() else {
            return;
        };
        let (Some(engine), Some(source)) = (draft.engine.clone(), self.editor.source().cloned())
        else {
            return;
        };
        let pages = match self.ocr_pages(draft) {
            Ok(pages) => pages,
            Err(why) => {
                self.editor.say(why);
                return;
            }
        };
        let languages = draft.chosen();
        let skip_text = draft.choice.skip_text;
        let now = ctx.input(|input| input.time);
        let cancel = Arc::new(AtomicBool::new(false));
        let next = Arc::new(AtomicUsize::new(0));
        let shared_pages = Arc::new(pages.clone());
        let (send, answers) = mpsc::channel();
        let credential = self.editor.credential().to_vec();
        let fonts = self.editor.fonts();
        let workers = std::thread::available_parallelism()
            .map_or(1, |cores| cores.get().saturating_sub(1))
            .clamp(1, MOST_WORKERS)
            .min(pages.len());
        let handles = (0..workers)
            .map(|_| {
                let (cancel, next, pages, send) = (
                    Arc::clone(&cancel),
                    Arc::clone(&next),
                    Arc::clone(&shared_pages),
                    send.clone(),
                );
                let (source, credential, fonts, engine, languages, ctx) = (
                    source.clone(),
                    credential.clone(),
                    fonts.clone(),
                    engine.clone(),
                    languages.clone(),
                    ctx.clone(),
                );
                std::thread::spawn(move || {
                    loop {
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        let Some(&page) = pages.get(next.fetch_add(1, Ordering::Relaxed)) else {
                            break;
                        };
                        let read = read_one(
                            &source,
                            page,
                            (&credential, fonts.clone()),
                            (&engine, &languages, skip_text),
                            &cancel,
                        );
                        if send.send((page, read)).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                    }
                })
            })
            .collect();
        if let Some(draft) = self.ocr_draft.as_mut() {
            draft.reading = Some(OcrReading {
                cancel,
                answers,
                workers: handles,
                pages,
                read: BTreeMap::new(),
                epoch: self.editor.epoch(),
                began: now,
                last_came: now,
            });
        }
    }

    fn keep_reading(&mut self, now: f64) {
        let Some(reading) = self
            .ocr_draft
            .as_mut()
            .and_then(|draft| draft.reading.as_mut())
        else {
            return;
        };
        while let Ok((page, read)) = reading.answers.try_recv() {
            reading.read.insert(page, read);
            reading.last_came = now;
        }
        let stopped = reading.cancel.load(Ordering::Relaxed);
        let finished = reading
            .workers
            .iter()
            .all(std::thread::JoinHandle::is_finished);
        if !(finished && (stopped || reading.read.len() == reading.pages.len())) {
            return;
        }
        if self.editor.is_busy() {
            return;
        }
        let Some(reading) = self
            .ocr_draft
            .as_mut()
            .and_then(|draft| draft.reading.take())
        else {
            return;
        };
        for worker in reading.workers {
            let _ = worker.join();
        }
        if stopped {
            self.editor.say(Done::RecognitionStopped.into());
            return;
        }
        if reading.epoch != self.editor.epoch() {
            self.editor.say(Done::RecognitionOutdated.into());
            return;
        }
        self.write_what_was_read(reading.read);
    }

    pub(crate) fn write_what_was_read(&mut self, read: BTreeMap<usize, PageRead>) {
        let mut layers = Vec::new();
        let (mut had_text, mut unread, mut failed) = (0, 0, None);
        let (mut weight, mut sum) = (0.0_f64, 0.0_f64);
        for (page, outcome) in read {
            match outcome {
                PageRead::Read(reading) => {
                    if reading.layer.words.is_empty() {
                        continue;
                    }
                    #[allow(clippy::cast_precision_loss)]
                    let words = reading.layer.words.len() as f64;
                    weight += words;
                    sum += words * f64::from(reading.confidence.unwrap_or(0.0));
                    layers.push((page, reading.layer));
                }
                PageRead::HadText => had_text += 1,
                PageRead::Failed(why) => {
                    unread += 1;
                    failed.get_or_insert(why);
                }
            }
        }
        if layers.is_empty() {
            let said = match failed {
                Some(why) => Message::Refused(why.into()),
                None if had_text > 0 => Done::NothingToRecognize.into(),
                None => Done::NothingChanged.into(),
            };
            self.editor.say(said);
            return;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let confidence = (sum / weight.max(1.0)).round().clamp(0.0, 100.0) as u8;
        let job = self
            .editor
            .begin_text_layers(layers, (confidence, had_text, unread));
        if job.is_none() {
            self.editor.say(Message::AnotherEditIsRunning);
            return;
        }
        self.send(job);
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
enum Pressed {
    #[default]
    Nothing,
    Read,
    Stop,
    Close,
    GetModel(String),
    GetEngine,
}

#[derive(Clone, Debug)]
struct Asked {
    pressed: Pressed,
    quality: Quality,
    remember: bool,
}

impl Default for Asked {
    fn default() -> Self {
        Self {
            pressed: Pressed::Nothing,
            quality: Quality::Accurate,
            remember: false,
        }
    }
}

fn which_languages(
    ui: &mut egui::Ui,
    draft: &mut OcrDraft,
    lang: pdf_app::wording::Lang,
    asked: &mut Asked,
) {
    ui.horizontal_wrapped(|ui| {
        for (code, chosen) in &mut draft.languages {
            let name = Message::OcrLanguage(code.clone()).say(lang);
            if draft.here.contains(code) {
                ui.checkbox(chosen, name);
            } else if let Some(model) = pdf_ocr::models::model(code, draft.choice.quality) {
                let label = Message::OcrGetModel {
                    code: code.clone(),
                    bytes: model.bytes,
                }
                .say(lang);
                if ui
                    .small_button(format!("\u{2b07} {label}"))
                    .on_hover_text(model.url())
                    .clicked()
                {
                    asked.pressed = Pressed::GetModel(code.clone());
                }
            }
        }
    });
}

fn which_models(
    ui: &mut egui::Ui,
    draft: &mut OcrDraft,
    lang: pdf_app::wording::Lang,
    asked: &mut Asked,
) {
    asked.quality = draft.choice.quality;
    ui.horizontal(|ui| {
        ui.label(Message::OcrModel.say(lang));
        for quality in Quality::ALL {
            ui.selectable_value(
                &mut asked.quality,
                quality,
                Message::OcrQuality(quality).say(lang),
            );
        }
    });
    let chosen = draft.chosen();
    let about: Vec<String> = KNOWN
        .iter()
        .filter(|code| chosen.is_empty() || chosen.iter().any(|ticked| ticked == *code))
        .filter_map(|code| pdf_ocr::models::model(code, asked.quality))
        .filter_map(|model| {
            model.cer.map(|cer| {
                Message::OcrErrorRate {
                    code: model.code.to_owned(),
                    cer,
                }
                .say(lang)
            })
        })
        .collect();
    if !about.is_empty() {
        ui.label(
            egui::RichText::new(about.join(" \u{b7} "))
                .size(11.0)
                .color(ui.visuals().weak_text_color()),
        );
    }
}

fn which_pages(
    ui: &mut egui::Ui,
    draft: &mut OcrDraft,
    (lang, count): (pdf_app::wording::Lang, usize),
) {
    ui.horizontal_wrapped(|ui| {
        ui.radio_value(
            &mut draft.which,
            OcrWhich::ThisPage,
            Message::OcrThisPage.say(lang),
        );
        ui.radio_value(&mut draft.which, OcrWhich::All, Message::AllPages.say(lang));
        ui.radio_value(
            &mut draft.which,
            OcrWhich::Some,
            Message::SomePages.say(lang),
        );
        let range = ui.add(
            egui::TextEdit::singleline(&mut draft.range)
                .hint_text(format!("1-{count}"))
                .desired_width(90.0),
        );
        if range.changed() {
            draft.which = OcrWhich::Some;
        }
    });
}

fn the_recogniser_itself(
    ui: &mut egui::Ui,
    draft: &OcrDraft,
    lang: pdf_app::wording::Lang,
    asked: &mut Asked,
) {
    if !pdf_ocr::setup::possible() {
        ui.colored_label(
            ui.visuals().error_fg_color,
            Message::OcrEngineElsewhere.say(lang),
        );
        return;
    }
    ui.colored_label(
        ui.visuals().error_fg_color,
        Message::OcrNotInstalled.say(lang),
    );
    ui.add_enabled_ui(draft.fetching.is_none(), |ui| {
        if ui
            .button(format!("\u{2b07} {}", Message::OcrGetEngine.say(lang)))
            .clicked()
        {
            asked.pressed = Pressed::GetEngine;
        }
    });
}

fn how_far(ui: &mut egui::Ui, draft: &OcrDraft, lang: pdf_app::wording::Lang, now: f64) {
    if let Some(reading) = &draft.reading {
        let (done, total) = (reading.read.len(), reading.pages.len());
        ui.add(
            egui::ProgressBar::new(creep(reading, now))
                .text(Message::OcrProgress { done, total }.say(lang)),
        );
    }
    let Some(fetch) = &draft.fetching else {
        return;
    };
    let said = match &fetch.code {
        Some(code) => Message::OcrGettingModel(code.clone()).say(lang),
        None => Message::OcrGettingEngine.say(lang),
    };
    let seen = fetch.seen.load(Ordering::Relaxed);
    let bar = if fetch.total > 0 {
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let fraction = (seen as f64 / fetch.total as f64) as f32;
        egui::ProgressBar::new(fraction.clamp(0.0, 1.0))
    } else {
        egui::ProgressBar::new(0.0).animate(true)
    };
    ui.add(bar.text(said));
}

fn creep(reading: &OcrReading, now: f64) -> f32 {
    let (done, total) = (reading.read.len(), reading.pages.len().max(1));
    if done >= total {
        return 1.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let per_page = if done > 0 {
        ((reading.last_came - reading.began) / done as f64).max(1.0)
    } else {
        20.0
    };
    let into_page = (now - reading.last_came).max(0.0);
    let part = (1.0 - (-into_page / per_page).exp()).min(0.95);
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let fraction = ((done as f64 + part) / total as f64) as f32;
    fraction
}

fn simple_panel(
    ui: &mut egui::Ui,
    draft: &mut OcrDraft,
    (lang, pages, now): (
        pdf_app::wording::Lang,
        Option<&Result<Vec<usize>, Message>>,
        f64,
    ),
    asked: &mut Asked,
) {
    asked.quality = draft.choice.quality;
    ui.spacing_mut().item_spacing.y = 10.0;
    let weak = ui.visuals().weak_text_color();
    let wide = ui.available_width();
    if simple_running(ui, draft, now, asked) {
        return;
    }
    ui.label(
        egui::RichText::new(
            "Turns a scanned page into text you can search and copy. It runs on this phone \
             and can misread, so check what it finds.",
        )
        .size(14.0)
        .color(weak),
    );
    if draft.engine.is_none() {
        the_recogniser_itself(ui, draft, lang, asked);
    }
    simple_choices(ui, draft);
    what_stands_in_the_way(ui, draft, lang, pages);
    let count = pages
        .and_then(|pages| pages.as_ref().ok())
        .map_or(0, Vec::len);
    let ready = count > 0 && !draft.chosen().is_empty();
    let to_fetch: u64 = draft
        .languages
        .iter()
        .filter(|(code, chosen)| *chosen && !draft.here.contains(code))
        .filter_map(|(code, _)| pdf_ocr::models::model(code, draft.choice.quality))
        .map(|model| model.bytes)
        .sum();
    ui.add_space(4.0);
    let read = egui::Button::new(
        egui::RichText::new("Read the text")
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
            egui::RichText::new(format!(
                "Downloads {} once, then works offline",
                megabytes(to_fetch)
            ))
            .size(13.0)
            .color(weak),
        );
    }
    if ui
        .add_sized(
            [wide, 40.0],
            egui::Button::new(egui::RichText::new(Message::Close.say(lang)).size(15.0))
                .frame(false),
        )
        .clicked()
    {
        asked.pressed = Pressed::Close;
    }
}

fn simple_running(ui: &mut egui::Ui, draft: &OcrDraft, now: f64, asked: &mut Asked) -> bool {
    let weak = ui.visuals().weak_text_color();
    let wide = ui.available_width();
    if let Some(reading) = &draft.reading {
        let (done, total) = (reading.read.len(), reading.pages.len());
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(22.0));
            ui.label(
                egui::RichText::new(format!("Reading page {} of {total}", (done + 1).min(total)))
                    .size(16.0),
            );
        });
        ui.add(
            egui::ProgressBar::new(creep(reading, now))
                .desired_height(10.0)
                .corner_radius(5.0),
        );
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let seconds = (now - reading.began).max(0.0) as u64;
        ui.label(
            egui::RichText::new(format!("{seconds} s \u{00b7} a page takes up to a minute"))
                .size(13.0)
                .color(weak),
        );
        if ui
            .add_sized(
                [wide, 44.0],
                egui::Button::new(egui::RichText::new("Stop").size(16.0)),
            )
            .clicked()
        {
            asked.pressed = Pressed::Stop;
        }
        return true;
    }
    if let Some(fetch) = &draft.fetching {
        let name = fetch.code.as_deref().map_or("the reader", |code| {
            pdf_ocr::names::name_of(code).unwrap_or(code)
        });
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(22.0));
            ui.label(egui::RichText::new(format!("Downloading {name}")).size(16.0));
        });
        let seen = fetch.seen.load(Ordering::Relaxed);
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let bar = if fetch.total > 0 {
            egui::ProgressBar::new(((seen as f64 / fetch.total as f64) as f32).clamp(0.0, 1.0))
        } else {
            egui::ProgressBar::new(0.0).animate(true)
        };
        ui.add(bar.desired_height(10.0).corner_radius(5.0));
        if fetch.total > 0 {
            ui.label(
                egui::RichText::new(format!("{} of {}", megabytes(seen), megabytes(fetch.total)))
                    .size(13.0)
                    .color(weak),
            );
        }
        if ui
            .add_sized(
                [wide, 44.0],
                egui::Button::new(egui::RichText::new("Stop").size(16.0)),
            )
            .clicked()
        {
            asked.pressed = Pressed::Stop;
        }
        return true;
    }
    false
}

fn the_choices(
    ui: &mut egui::Ui,
    draft: &mut OcrDraft,
    (lang, count): (pdf_app::wording::Lang, usize),
    asked: &mut Asked,
) {
    if draft.engine.is_none() {
        the_recogniser_itself(ui, draft, lang, asked);
        ui.separator();
    }
    let idle = draft.reading.is_none() && draft.fetching.is_none();
    ui.add_enabled_ui(idle, |ui| {
        which_languages(ui, draft, lang, asked);
        which_models(ui, draft, lang, asked);
        ui.separator();
        which_pages(ui, draft, (lang, count));
        if ui
            .checkbox(&mut draft.choice.skip_text, Message::OcrSkipText.say(lang))
            .changed()
        {
            asked.remember = true;
        }
    });
}

fn simple_choices(ui: &mut egui::Ui, draft: &mut OcrDraft) {
    let weak = ui.visuals().weak_text_color();
    let name_of = |code: &str| pdf_ocr::names::name_of(code).unwrap_or(code).to_owned();
    ui.label(egui::RichText::new("Language").size(15.0).strong());
    ui.horizontal_wrapped(|ui| {
        for (code, chosen) in draft.languages.iter_mut().filter(|(_, chosen)| *chosen) {
            let chip = egui::Button::new(
                egui::RichText::new(format!("{}   \u{00d7}", name_of(code))).size(15.0),
            )
            .corner_radius(18.0)
            .min_size(egui::vec2(0.0, 36.0));
            if ui.add(chip).clicked() {
                *chosen = false;
            }
        }
        let add = egui::Button::new(
            egui::RichText::new(if draft.adding {
                "Done"
            } else {
                "+  Add a language"
            })
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
        ui.add(
            egui::TextEdit::singleline(&mut draft.search)
                .hint_text("Search languages")
                .font(egui::FontId::proportional(15.0))
                .desired_width(f32::INFINITY)
                .margin(egui::vec2(8.0, 8.0)),
        );
        let words: Vec<String> = draft
            .search
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        egui::ScrollArea::vertical()
            .id_salt("ocr-languages")
            .max_height(220.0)
            .show(ui, |ui| {
                for (code, chosen) in &mut draft.languages {
                    let name = name_of(code);
                    let haystack = format!("{} {code}", name.to_lowercase());
                    if !words.iter().all(|word| haystack.contains(word.as_str())) {
                        continue;
                    }
                    let size = if draft.here.contains(code) {
                        "on this phone".to_owned()
                    } else {
                        pdf_ocr::models::model(code, draft.choice.quality)
                            .map(|model| megabytes(model.bytes))
                            .unwrap_or_default()
                    };
                    ui.horizontal(|ui| {
                        ui.set_min_height(36.0);
                        ui.checkbox(chosen, egui::RichText::new(name).size(15.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(size).size(13.0).color(weak));
                        });
                    });
                }
            });
        ui.label(
            egui::RichText::new(
                "Tick only the languages in the document: one it does not have makes reading \
                 slower and can make it worse.",
            )
            .size(13.0)
            .color(weak),
        );
    }
    ui.label(egui::RichText::new("Pages").size(15.0).strong());
    if draft.which == OcrWhich::Some {
        draft.which = OcrWhich::ThisPage;
    }
    ui.horizontal(|ui| {
        let half = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
        for (which, label) in [
            (OcrWhich::ThisPage, "This page"),
            (OcrWhich::All, "Every page"),
        ] {
            let on = draft.which == which;
            let button = egui::Button::new(egui::RichText::new(label).size(15.0))
                .selected(on)
                .corner_radius(10.0)
                .min_size(egui::vec2(half, 40.0));
            if ui.add(button).clicked() {
                draft.which = which;
            }
        }
    });
}

fn megabytes(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let megabytes = bytes as f64 / 1_000_000.0;
    format!("{megabytes:.1} MB")
}

fn what_stands_in_the_way(
    ui: &mut egui::Ui,
    draft: &OcrDraft,
    lang: pdf_app::wording::Lang,
    pages: Option<&Result<Vec<usize>, Message>>,
) {
    if let Some(Err(why)) = pages {
        ui.colored_label(ui.visuals().error_fg_color, why.say(lang));
    }
    if draft.engine.is_some() && draft.chosen().is_empty() {
        ui.colored_label(ui.visuals().warn_fg_color, Message::OcrNoLanguage.say(lang));
    }
    if let Some(trouble) = &draft.trouble {
        ui.colored_label(ui.visuals().error_fg_color, trouble.say(lang));
    }
}

fn the_buttons(
    ui: &mut egui::Ui,
    draft: &OcrDraft,
    (lang, pages): (pdf_app::wording::Lang, Option<&Result<Vec<usize>, Message>>),
    asked: &mut Asked,
) {
    ui.horizontal(|ui| {
        if draft.reading.is_some() || draft.fetching.is_some() {
            if ui.button(Message::OcrStop.say(lang)).clicked() {
                asked.pressed = Pressed::Stop;
            }
            return;
        }
        let count = pages
            .and_then(|pages| pages.as_ref().ok())
            .map_or(0, Vec::len);
        let ready = count > 0 && !draft.chosen().is_empty();
        if ui
            .add_enabled(
                ready,
                egui::Button::new(
                    egui::RichText::new(if SIMPLE {
                        "Read the text".to_owned()
                    } else {
                        Message::OcrStart(count).say(lang)
                    })
                    .strong(),
                )
                .min_size(egui::vec2(160.0, 30.0)),
            )
            .clicked()
        {
            asked.pressed = Pressed::Read;
        }
        if ui.button(Message::Close.say(lang)).clicked() {
            asked.pressed = Pressed::Close;
        }
    });
}
