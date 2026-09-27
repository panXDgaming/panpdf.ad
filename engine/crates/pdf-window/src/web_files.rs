use std::cell::RefCell;
use std::collections::VecDeque;

use eframe::egui;
use pdf_bytes::SourceId;
use wasm_bindgen::JsCast as _;

use crate::window_state::Window;

thread_local! {
    static ARRIVED: RefCell<VecDeque<(String, Vec<u8>)>> = const { RefCell::new(VecDeque::new()) };
    static CONTEXT: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
    static FONTS: RefCell<Vec<(String, Vec<u8>)>> = const { RefCell::new(Vec::new()) };
    static HELD: RefCell<Vec<(String, Vec<u8>)>> = const { RefCell::new(Vec::new()) };
    static PICKED: RefCell<VecDeque<Picked>> = const { RefCell::new(VecDeque::new()) };
    static UNSAVED: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    static FONTS_GREW: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static RUNNER: RefCell<Option<eframe::WebRunner>> = const { RefCell::new(None) };
    static DOCUMENT: std::cell::Cell<(Option<SourceId>, u64)> = const { std::cell::Cell::new((None, 0)) };
    static TOLD: std::cell::Cell<Option<(u64, u64, bool, u64)>> = const { std::cell::Cell::new(None) };
    static RESTORING: RefCell<Option<String>> = const { RefCell::new(None) };
    static OCR_LANGUAGES: RefCell<Vec<(String, String, String)>> = const { RefCell::new(Vec::new()) };
    static OCR_ANSWERS: RefCell<VecDeque<OcrAnswer>> = const { RefCell::new(VecDeque::new()) };
}

#[derive(Clone, Debug)]
pub(crate) enum OcrAnswer {
    Step(String),
    Read { id: u32, tsv: String, text: String },
    Failed { id: u32, error: String },
}

pub fn ocr_languages(list: &str) {
    let parsed = list
        .lines()
        .filter_map(|line| {
            let mut cells = line.split('\t');
            Some((
                cells.next()?.to_owned(),
                cells.next()?.to_owned(),
                cells.next().unwrap_or("").to_owned(),
            ))
        })
        .collect();
    OCR_LANGUAGES.with(|languages| *languages.borrow_mut() = parsed);
    wake();
}

pub(crate) fn ocr_language_list() -> Vec<(String, String, String)> {
    OCR_LANGUAGES.with(|languages| languages.borrow().clone())
}

pub fn ocr_step(what: String) {
    OCR_ANSWERS.with(|answers| answers.borrow_mut().push_back(OcrAnswer::Step(what)));
    wake();
}

pub fn ocr_read(id: u32, tsv: String, text: String) {
    OCR_ANSWERS.with(|answers| {
        answers
            .borrow_mut()
            .push_back(OcrAnswer::Read { id, tsv, text })
    });
    wake();
}

pub fn ocr_failed(id: u32, error: String) {
    OCR_ANSWERS.with(|answers| {
        answers
            .borrow_mut()
            .push_back(OcrAnswer::Failed { id, error })
    });
    wake();
}

pub(crate) fn take_ocr_answer() -> Option<OcrAnswer> {
    OCR_ANSWERS.with(|answers| answers.borrow_mut().pop_front())
}

pub(crate) fn ask_ocr(id: u32, pgm: &[u8], set: &str, sets: &str) -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let Ok(function) = js_sys::Reflect::get(&window, &"panpdfOcrRead".into())
        .and_then(|function| function.dyn_into::<js_sys::Function>().map_err(Into::into))
    else {
        return false;
    };
    let arguments = js_sys::Array::of4(
        &f64::from(id).into(),
        &js_sys::Uint8Array::from(pgm).into(),
        &set.into(),
        &sets.into(),
    );
    function.apply(&window, &arguments).is_ok()
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Pick {
    Picture,
    Pages { before: bool },
    PicturesIn { before: Option<bool> },
}

pub(crate) type Picked = (Pick, Vec<(String, Vec<u8>)>);

pub fn add_font(name: String, bytes: Vec<u8>) {
    FONTS.with(|fonts| fonts.borrow_mut().push((name, bytes)));
}

pub fn fonts_ready(manifest: &str) {
    let again = pdf_cli::fonts_held();
    let files = FONTS.with(|fonts| std::mem::take(&mut *fonts.borrow_mut()));
    interface_faces(&files);
    let held = pdf_cli::hold_fonts(manifest, files);
    web_sys::console::log_1(&format!("PanPDF: {held} substitute faces ready").into());
    if again {
        FONTS_GREW.with(|grew| grew.set(true));
    }
    wake();
}

fn interface_faces(files: &[(String, Vec<u8>)]) {
    let changed = HELD.with(|held| {
        let mut held = held.borrow_mut();
        let before = crate::interface_fonts::held(&held).len();
        for (name, bytes) in files {
            if !held.iter().any(|(kept, _)| kept == name) {
                held.push((name.clone(), bytes.clone()));
            }
        }
        let chosen = crate::interface_fonts::held(&held);
        held.retain(|(name, _)| {
            chosen
                .iter()
                .any(|face| face.path.as_os_str() == name.as_str())
        });
        (chosen.len() != before).then_some(chosen)
    });
    let Some(chosen) = changed else {
        return;
    };
    CONTEXT.with(|context| {
        if let Some(ctx) = context.borrow().as_ref() {
            ctx.set_fonts(crate::interface_fonts::definitions(chosen));
            ctx.request_repaint();
        }
    });
}

pub(crate) fn fonts_grew() -> bool {
    FONTS_GREW.with(|grew| grew.replace(false))
}

pub(crate) fn want_text(text: &str) {
    if !text.is_ascii() {
        call_page("panpdfWantText", text);
    }
}

pub(crate) fn want_family(family: &str) {
    call_page("panpdfWantFamily", family);
}

fn call_page(function: &str, argument: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    if let Ok(function) = js_sys::Reflect::get(&window, &function.into())
        && let Ok(function) = function.dyn_into::<js_sys::Function>()
    {
        let _ = function.call1(&window, &argument.into());
    }
}

pub fn open_from_page(name: String, bytes: Vec<u8>) {
    RESTORING.with(|restoring| restoring.borrow_mut().take());
    ARRIVED.with(|arrived| arrived.borrow_mut().push_back((name, bytes)));
    wake();
}

pub fn open_draft(name: String, bytes: Vec<u8>) {
    RESTORING.with(|restoring| *restoring.borrow_mut() = Some(name.clone()));
    ARRIVED.with(|arrived| arrived.borrow_mut().push_back((name, bytes)));
    wake();
}

pub(crate) fn was_restored(path: &std::path::Path) -> bool {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    RESTORING
        .with(|restoring| restoring.borrow_mut().take())
        .is_some_and(|kept| Some(kept) == name)
}

pub(crate) fn keep_runner(runner: eframe::WebRunner) {
    RUNNER.with(|kept| *kept.borrow_mut() = Some(runner));
}

fn with_window<R>(act: impl FnOnce(&mut Window) -> R) -> Option<R> {
    let runner = RUNNER.with(|kept| kept.borrow().clone())?;
    let mut window = runner.app_mut::<Window>()?;
    Some(act(&mut window))
}

fn document_number(id: SourceId) -> u64 {
    DOCUMENT.with(|kept| {
        let (last, number) = kept.get();
        if last == Some(id) {
            number
        } else {
            kept.set((Some(id), number + 1));
            number + 1
        }
    })
}

pub(crate) fn tell_the_page(
    opened: Option<SourceId>,
    epoch: u64,
    unsaved: bool,
    typing: u64,
    path: &std::path::Path,
) {
    let document = opened.map_or_else(|| DOCUMENT.with(|kept| kept.get().1), document_number);
    let now = (document, epoch, unsaved, typing);
    if TOLD.with(|told| told.replace(Some(now))) == Some(now) {
        return;
    }
    let Some(window) = web_sys::window() else {
        return;
    };
    if let Ok(function) = js_sys::Reflect::get(&window, &"panpdfChanged".into())
        && let Ok(function) = function.dyn_into::<js_sys::Function>()
    {
        let name = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        #[expect(clippy::cast_precision_loss, reason = "counters far below 2^53")]
        let (document, epoch) = (document as f64, epoch as f64);
        let arguments = js_sys::Array::new();
        arguments.push(&document.into());
        arguments.push(&epoch.into());
        arguments.push(&unsaved.into());
        arguments.push(&name.into());
        let _ = function.apply(&window, &arguments);
    }
}

#[must_use]
pub fn draft_now(write_typing: bool) -> wasm_bindgen::JsValue {
    with_window(|window| window.draft(write_typing))
        .flatten()
        .unwrap_or(wasm_bindgen::JsValue::NULL)
}

#[must_use]
pub fn draft_base(document: u64, opened: usize) -> wasm_bindgen::JsValue {
    with_window(|window| {
        let (id, length) = window.editor.opened()?;
        let source = window.editor.source()?;
        let length = if length <= source.len() { length } else { 0 };
        (document_number(id) == document && length == opened)
            .then(|| crate::web_tiles::uint8_array_of(source, 0, opened).into())
    })
    .flatten()
    .unwrap_or(wasm_bindgen::JsValue::NULL)
}

impl Window {
    fn draft(&mut self, write_typing: bool) -> Option<wasm_bindgen::JsValue> {
        if !self.has_document() || self.editor.is_busy() {
            return None;
        }
        if write_typing
            && self
                .live
                .as_ref()
                .is_some_and(crate::live_typing::LiveTyping::holds_typing)
        {
            self.write_live(true);
            wake();
            if self.editor.is_busy() {
                return None;
            }
        }
        let (id, opened) = self.editor.opened()?;
        let source = self.editor.source()?;
        let opened = if opened <= source.len() { opened } else { 0 };
        let name = self.opened.file_name().map_or_else(
            || "document.pdf".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        let draft = js_sys::Object::new();
        #[expect(
            clippy::cast_precision_loss,
            reason = "counters and lengths far below 2^53"
        )]
        let numbers = [
            ("document", document_number(id) as f64),
            ("epoch", self.editor.epoch() as f64),
            ("opened", opened as f64),
            ("size", source.len() as f64),
        ];
        for (key, value) in numbers {
            let _ = js_sys::Reflect::set(&draft, &key.into(), &value.into());
        }
        let _ = js_sys::Reflect::set(&draft, &"unsaved".into(), &self.unsaved().into());
        let _ = js_sys::Reflect::set(&draft, &"name".into(), &name.into());
        let tail = crate::web_tiles::uint8_array_of(source, opened, source.len());
        let _ = js_sys::Reflect::set(&draft, &"tail".into(), &tail);
        Some(draft.into())
    }
}

pub(crate) fn saved(name: &str) {
    call_page("panpdfSaved", name);
}

fn wake() {
    CONTEXT.with(|context| {
        if let Some(context) = context.borrow().as_ref() {
            context.request_repaint();
        }
    });
}

pub(crate) fn remember(context: &egui::Context) {
    let first = CONTEXT.with(|kept| {
        let first = kept.borrow().is_none();
        if first {
            *kept.borrow_mut() = Some(context.clone());
        }
        first
    });
    if first {
        let chosen = HELD.with(|held| crate::interface_fonts::held(&held.borrow()));
        if !chosen.is_empty() {
            context.set_fonts(crate::interface_fonts::definitions(chosen));
        }
    }
}

pub(crate) fn take() -> Option<(String, Vec<u8>)> {
    ARRIVED.with(|arrived| arrived.borrow_mut().pop_front())
}

pub(crate) fn choose() {
    let input = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("panpdf-file"))
        .and_then(|element| element.dyn_into::<web_sys::HtmlInputElement>().ok());
    if let Some(input) = input {
        input.set_value("");
        input.click();
    }
}

pub(crate) fn mark_unsaved(unsaved: bool) {
    if UNSAVED.with(std::cell::Cell::get) == Some(unsaved) {
        return;
    }
    UNSAVED.with(|kept| kept.set(Some(unsaved)));
    if let Some(window) = web_sys::window() {
        let _ = js_sys::Reflect::set(&window, &"panpdfUnsaved".into(), &unsaved.into());
    }
}

pub(crate) fn drag_position(ctx: &egui::Context) -> Option<egui::Pos2> {
    let window = web_sys::window()?;
    let at = js_sys::Reflect::get(&window, &"panpdfDragAt".into()).ok()?;
    let at: js_sys::Array = at.dyn_into().ok()?;
    #[expect(clippy::cast_possible_truncation, reason = "a position on a screen")]
    let (x, y) = (at.get(0).as_f64()? as f32, at.get(1).as_f64()? as f32);
    let zoom = ctx.zoom_factor();
    Some(egui::pos2(x / zoom, y / zoom))
}

pub(crate) fn pick(pick: Pick) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    if let Some(old) = document.get_element_by_id("panpdf-pick") {
        old.remove();
    }
    let Some(input) = document
        .create_element("input")
        .ok()
        .and_then(|element| element.dyn_into::<web_sys::HtmlInputElement>().ok())
    else {
        return;
    };
    input.set_id("panpdf-pick");
    input.set_type("file");
    input.set_hidden(true);
    match pick {
        Pick::Pages { .. } => input.set_accept("application/pdf,.pdf"),
        Pick::Picture | Pick::PicturesIn { .. } => {
            input.set_accept("image/png,image/jpeg,.png,.jpg,.jpeg");
            input.set_multiple(true);
        }
    }
    let chosen = input.clone();
    let on_change = wasm_bindgen::closure::Closure::once(move || {
        chosen.remove();
        let Some(list) = chosen.files() else {
            return;
        };
        let files: Vec<web_sys::File> = (0..list.length()).filter_map(|at| list.get(at)).collect();
        wasm_bindgen_futures::spawn_local(async move {
            let mut read = Vec::with_capacity(files.len());
            for file in files {
                if let Ok(buffer) = wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await
                {
                    read.push((file.name(), js_sys::Uint8Array::new(&buffer).to_vec()));
                }
            }
            if !read.is_empty() {
                PICKED.with(|picked| picked.borrow_mut().push_back((pick, read)));
                wake();
            }
        });
    });
    input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
    on_change.forget();
    if let Some(body) = document.body() {
        let _ = body.append_child(&input);
    }
    input.click();
}

pub(crate) fn take_picked() -> Option<Picked> {
    PICKED.with(|picked| picked.borrow_mut().pop_front())
}

pub(crate) fn seed_random() -> bool {
    const SEED: u32 = 64;
    let global = js_sys::global();
    let crypto = js_sys::Reflect::get(&global, &"crypto".into()).ok();
    let draw = crypto.as_ref().and_then(|crypto| {
        js_sys::Reflect::get(crypto, &"getRandomValues".into())
            .ok()?
            .dyn_into::<js_sys::Function>()
            .ok()
    });
    let (Some(crypto), Some(draw)) = (crypto, draw) else {
        web_sys::console::error_1(
            &"no crypto.getRandomValues: protected files cannot be written".into(),
        );
        return false;
    };
    let array = js_sys::Uint8Array::new_with_length(SEED);
    let drawn = draw.call1(&crypto, &array).is_ok();
    let mut seed = [0_u8; SEED as usize];
    array.copy_to(&mut seed);
    array.fill(0, 0, SEED);
    let seeded = drawn && pdf_security::seed_random(&seed).is_ok();
    seed.fill(0);
    std::hint::black_box(&seed);
    if !seeded {
        web_sys::console::error_1(&"the browser's random bytes were refused as a seed".into());
    }
    seeded
}

pub(crate) fn touch_first() -> bool {
    web_sys::window()
        .and_then(|window| js_sys::Reflect::get(&window, &"panpdfTouch".into()).ok())
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

pub(crate) fn open_the_sample() {
    call_page("panpdfOpenSample", "");
}

pub(crate) fn download(name: &str, bytes: &[u8]) -> Result<(), String> {
    let failed = |what: &str| format!("the browser refused the download: {what}");
    let parts = js_sys::Array::new();
    parts.push(&js_sys::Uint8Array::from(bytes));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(if name.to_lowercase().ends_with(".png") {
        "image/png"
    } else {
        "application/pdf"
    });
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(|_| failed("blob"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|_| failed("url"))?;
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| failed("document"))?;
    let anchor = document
        .create_element("a")
        .ok()
        .and_then(|element| element.dyn_into::<web_sys::HtmlAnchorElement>().ok())
        .ok_or_else(|| failed("link"))?;
    anchor.set_href(&url);
    anchor.set_download(name);
    anchor.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}
