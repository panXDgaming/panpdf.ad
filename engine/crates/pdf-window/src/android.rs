use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Mutex, OnceLock};

pub use android_activity::AndroidApp;
use eframe::egui;

use crate::window_state::Window;

#[derive(Clone, Copy, Debug)]
pub struct Host {
    pub pick_pdf: fn(),
    pub export: fn(&Path, &str),
    pub set_clipboard: fn(&str),
    pub show_menu: fn([i32; 4], bool),
    pub hide_menu: fn(),
    pub leave: fn(),
    pub open_url: fn(&str) -> Result<(), String>,
    pub scan: fn(bool),
    pub keyboard: fn(bool),
    pub share: fn(&Path),
    pub pick_files: fn(&[&str], bool, &str),
}

static ACTIVITY: OnceLock<AndroidApp> = OnceLock::new();
static HOST: OnceLock<Host> = OnceLock::new();
static CONTEXT: OnceLock<egui::Context> = OnceLock::new();
static OPENED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static SAID: Mutex<Vec<String>> = Mutex::new(Vec::new());
static CACHE: OnceLock<PathBuf> = OnceLock::new();
static LIBRARIES: OnceLock<PathBuf> = OnceLock::new();
static FILES: OnceLock<PathBuf> = OnceLock::new();
static SCANNED: Mutex<Vec<(Vec<PathBuf>, bool)>> = Mutex::new(Vec::new());
static EVENTS: Mutex<Vec<egui::Event>> = Mutex::new(Vec::new());
static BACK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static MENU: Mutex<Option<([i32; 4], bool)>> = Mutex::new(None);
static KEYBOARD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

static MENU_GONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static INSETS: [AtomicI32; 4] = [
    AtomicI32::new(0),
    AtomicI32::new(0),
    AtomicI32::new(0),
    AtomicI32::new(0),
];

pub(crate) fn activity() -> Option<AndroidApp> {
    ACTIVITY.get().cloned()
}

pub fn start(app: AndroidApp, host: Host) -> Result<(), String> {
    let _ = ACTIVITY.set(app);
    let _ = HOST.set(host);
    let editor = pdf_app::Editor::stand_in()?;
    crate::app::run(editor, PathBuf::new(), Vec::new(), None)
}

pub fn set_insets(top: i32, bottom: i32, left: i32, right: i32) {
    for (held, value) in INSETS.iter().zip([top, bottom, left, right]) {
        held.store(value.max(0), Ordering::Relaxed);
    }
    wake();
}

pub fn scanned(paths: Vec<PathBuf>, into: bool) {
    if let Ok(mut scanned) = SCANNED.lock() {
        scanned.push((paths, into));
    }
    wake();
}

pub(crate) fn share(name: &str, bytes: &[u8]) -> Result<(), String> {
    let folder = cache_dir().ok_or("no cache folder")?.join("shared");
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    if let Ok(old) = std::fs::read_dir(&folder) {
        for entry in old.flatten() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let safe: String = name
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let safe = safe.trim_start_matches('.');
    let file = folder.join(if safe.is_empty() {
        "document.pdf"
    } else {
        safe
    });
    std::fs::write(&file, bytes).map_err(|error| error.to_string())?;
    let host = HOST.get().ok_or("no activity")?;
    (host.share)(&file);
    Ok(())
}

pub(crate) fn pick_files(accepts: &[&str], several: bool, tool: &str) {
    if let Some(host) = HOST.get() {
        (host.pick_files)(accepts, several, tool);
    }
}

pub(crate) fn tools_program() -> Option<PathBuf> {
    LIBRARIES
        .get()
        .map(|folder| folder.join("libpanpdftools.so"))
        .filter(|path| path.is_file())
}

pub(crate) fn fonts_dir() -> Option<PathBuf> {
    FILES.get().map(|folder| folder.join("fonts"))
}

pub fn set_paths(libraries: PathBuf, files: PathBuf) {
    let _ = LIBRARIES.set(libraries);
    let _ = FILES.set(files);
}

pub(crate) fn scan(into: bool) {
    if let Some(host) = HOST.get() {
        (host.scan)(into);
    }
}

pub fn set_cache_dir(folder: PathBuf) {
    let _ = CACHE.set(folder);
}

pub(crate) fn cache_dir() -> Option<PathBuf> {
    CACHE.get().cloned()
}

pub fn opened(path: PathBuf) {
    if let Ok(mut opened) = OPENED.lock() {
        opened.push(path);
    }
    wake();
}

pub fn said(what: String) {
    let exported = what == "saved" || what.starts_with("not saved");
    if let Ok(mut said) = SAID.lock() {
        said.push(what);
    }
    if exported {
        export_ended();
    }
    wake();
}

pub fn command(what: &str) {
    let event = match what {
        "copy" => Some(egui::Event::Copy),
        "cut" => Some(egui::Event::Cut),
        "selectAll" => Some(egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }),
        "back" => {
            BACK.store(true, Ordering::Relaxed);
            None
        }
        "menuGone" => {
            if let Ok(mut shown) = MENU.lock() {
                *shown = None;
            }
            MENU_GONE.store(true, Ordering::Relaxed);
            None
        }
        _ => None,
    };
    if let Some(event) = event
        && let Ok(mut events) = EVENTS.lock()
    {
        events.push(event);
    }
    wake();
}

pub fn typed(text: String) {
    if let Ok(mut events) = EVENTS.lock() {
        events.push(egui::Event::Text(text));
    }
    wake();
}

pub fn key(code: i32) {
    let key = match code {
        67 => egui::Key::Backspace,
        112 => egui::Key::Delete,
        66 => egui::Key::Enter,
        61 => egui::Key::Tab,
        21 => egui::Key::ArrowLeft,
        22 => egui::Key::ArrowRight,
        19 => egui::Key::ArrowUp,
        20 => egui::Key::ArrowDown,
        _ => return,
    };
    if let Ok(mut events) = EVENTS.lock() {
        for pressed in [true, false] {
            events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
    wake();
}

pub fn paste(text: String) {
    if let Ok(mut events) = EVENTS.lock() {
        events.push(egui::Event::Paste(text));
    }
    wake();
}

pub(crate) fn feed(raw: &mut egui::RawInput) {
    if let Ok(mut events) = EVENTS.lock() {
        raw.events.append(&mut events);
    }
}

fn wake() {
    if let Some(context) = CONTEXT.get() {
        context.request_repaint();
    }
}

pub(crate) fn pick_pdf() {
    if let Some(host) = HOST.get() {
        (host.pick_pdf)();
    }
}

pub(crate) fn open_url(uri: &str) -> Result<(), String> {
    (HOST.get().ok_or("no activity")?.open_url)(uri)
}

pub(crate) fn export(path: &Path) {
    if let Ok(mut waiting) = EXPORTS.lock() {
        waiting.push_back(path.to_path_buf());
    }
    export_the_next();
}

static EXPORTS: Mutex<std::collections::VecDeque<PathBuf>> =
    Mutex::new(std::collections::VecDeque::new());
static EXPORTING: Mutex<Option<std::time::Instant>> = Mutex::new(None);

const EXPORT_PATIENCE: std::time::Duration = std::time::Duration::from_mins(3);

fn export_the_next() {
    let Ok(mut exporting) = EXPORTING.lock() else {
        return;
    };
    if exporting.is_some_and(|since| since.elapsed() < EXPORT_PATIENCE) {
        return;
    }
    let Some(path) = EXPORTS
        .lock()
        .ok()
        .and_then(|mut waiting| waiting.pop_front())
    else {
        *exporting = None;
        return;
    };
    let name = path.file_name().map_or_else(
        || "document.pdf".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    if let Some(host) = HOST.get() {
        *exporting = Some(std::time::Instant::now());
        (host.export)(&path, &name);
    }
}

fn export_ended() {
    if let Ok(mut exporting) = EXPORTING.lock() {
        *exporting = None;
    }
    export_the_next();
}

impl Window {
    pub(crate) fn take_from_android(&mut self, ctx: &egui::Context) {
        let _ = CONTEXT.set(ctx.clone());
        let back_key =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::BrowserBack));
        if BACK.swap(false, Ordering::Relaxed) || back_key {
            self.back();
        }
        self.tool_work(ctx);
        let said: Vec<String> = SAID
            .lock()
            .map(|mut said| said.drain(..).collect())
            .unwrap_or_default();
        for what in said {
            if what.starts_with("not ") {
                self.editor
                    .say(pdf_app::wording::Message::Refused(what.into()));
            }
        }
        if self.editor.is_busy() || self.loading.is_some() || self.leaving.is_some() {
            return;
        }
        let next = OPENED
            .lock()
            .ok()
            .and_then(|mut opened| (!opened.is_empty()).then(|| opened.remove(0)));
        if let Some(path) = next {
            self.open(&path);
            return;
        }
        let scan = SCANNED
            .lock()
            .ok()
            .and_then(|mut scanned| (!scanned.is_empty()).then(|| scanned.remove(0)));
        if let Some((paths, into)) = scan {
            self.take_the_scan(&paths, into);
        }
    }

    pub(crate) fn share(&mut self) {
        if self.input.pending() || self.input.draft().is_some() {
            self.editor
                .say(pdf_app::wording::Message::ResolveDraftBeforeSaving);
            return;
        }
        let name = self.opened.file_name().map_or_else(
            || "document.pdf".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        let shared = self
            .editor
            .export()
            .and_then(|export| share(&name, &export.bytes));
        if let Err(why) = shared {
            self.editor.say(pdf_app::wording::Message::Refused(
                format!("not shared: {why}").into(),
            ));
        }
    }

    fn take_the_scan(&mut self, paths: &[PathBuf], into: bool) {
        if paths.is_empty() {
            return;
        }
        if into && self.has_document() {
            self.pictures_chosen(paths, Some(false));
            return;
        }
        let pictures: Vec<std::sync::Arc<[u8]>> = paths
            .iter()
            .filter_map(|path| std::fs::read(path).ok().map(std::sync::Arc::from))
            .collect();
        let Some(folder) = cache_dir() else { return };
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        let day = pdf_app::dates::moment(stamp);
        let base = format!("Scan {}-{:02}-{:02}", day.year, day.month, day.day);
        let path = (1..)
            .map(|n| {
                folder.join(if n == 1 {
                    format!("{base}.pdf")
                } else {
                    format!("{base} ({n}).pdf")
                })
            })
            .find(|path| !path.exists())
            .unwrap_or_else(|| folder.join(format!("{base}.pdf")));
        self.make_pages(pictures, crate::pictures::AfterPictures::Scan(path));
    }

    fn back(&mut self) {
        let chosen = self.reading.is_some()
            || self.picture_menu.is_some()
            || !matches!(self.pointing, crate::window_state::Pointing::Nothing);
        if chosen {
            self.reading = None;
            self.picture_menu = None;
            if let Ok(mut events) = EVENTS.lock() {
                events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            wake();
        } else if !self.home {
            self.go_home();
        } else if let Some(host) = HOST.get() {
            (host.leave)();
        }
    }

    pub(crate) fn after_the_frame_on_android(&self, ctx: &egui::Context) {
        let Some(host) = HOST.get() else {
            return;
        };
        let copied: Vec<String> = ctx.output(|output| {
            output
                .commands
                .iter()
                .filter_map(|command| match command {
                    egui::OutputCommand::CopyText(text) => Some(text.clone()),
                    _ => None,
                })
                .collect()
        });
        for text in copied {
            (host.set_clipboard)(&text);
        }
        let links: Vec<String> = ctx.output(|output| {
            output
                .commands
                .iter()
                .filter_map(|command| match command {
                    egui::OutputCommand::OpenUrl(open) => Some(open.url.clone()),
                    _ => None,
                })
                .collect()
        });
        for link in links {
            let _ = open_url(&link);
        }
        let typing = self.touched
            && !self.viewing
            && (matches!(self.pointing, crate::window_state::Pointing::Text { .. })
                || self.text_draft.is_some());
        if KEYBOARD.swap(typing, Ordering::Relaxed) != typing {
            (host.keyboard)(typing);
        }
        let mut wanted = self.text_menu_wanted(ctx);
        if wanted.is_none() {
            MENU_GONE.store(false, Ordering::Relaxed);
        } else if MENU_GONE.load(Ordering::Relaxed) {
            wanted = None;
        }
        if let Ok(mut shown) = MENU.lock()
            && *shown != wanted
        {
            match wanted {
                Some((rect, editing)) => (host.show_menu)(rect, editing),
                None => (host.hide_menu)(),
            }
            *shown = wanted;
        }
    }

    fn text_menu_wanted(&self, ctx: &egui::Context) -> Option<([i32; 4], bool)> {
        if ctx.input(|input| input.pointer.any_down()) || self.carrying_an_end {
            return None;
        }
        let ppp = ctx.pixels_per_point();
        #[allow(clippy::cast_possible_truncation)]
        let pixels = |rect: egui::Rect| {
            [
                (rect.min.x * ppp).round() as i32,
                (rect.min.y * ppp).round() as i32,
                (rect.max.x * ppp).round() as i32,
                (rect.max.y * ppp).round() as i32,
            ]
        };
        if self.viewing {
            let (_, ends) = self.reading_ends()?;
            return Some((pixels(ends[0].band().union(ends[1].band())), false));
        }
        let crate::window_state::Pointing::Text { caret, .. } = self.pointing else {
            return None;
        };
        if self.live.as_ref().is_some_and(|live| !live.written) {
            return None;
        }
        if caret.anchor == caret.at {
            return None;
        }
        Some((pixels(self.caret_shown.get()?), true))
    }

    pub(crate) fn clear_the_system_bars(ui: &mut egui::Ui) {
        let ppp = ui.ctx().pixels_per_point().max(0.1);
        #[allow(clippy::cast_precision_loss)]
        let points = |index: usize| INSETS[index].load(Ordering::Relaxed) as f32 / ppp;
        let fill = egui::Frame::NONE.fill(ui.visuals().panel_fill);
        let (top, bottom) = (points(0), points(1));
        if top > 0.0 {
            egui::Panel::top("android-status-bar")
                .exact_size(top)
                .frame(fill)
                .show(ui, |_| {});
        }
        if bottom > 0.0 {
            egui::Panel::bottom("android-navigation-bar")
                .exact_size(bottom)
                .frame(fill)
                .show(ui, |_| {});
        }
    }
}
