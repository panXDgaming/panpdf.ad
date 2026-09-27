use std::cell::RefCell;
use std::collections::VecDeque;

use eframe::egui;
use pdf_app::painter::{Painter, TileId, TilePixels};
use pdf_bytes::{ByteStore, SourceId};
use wasm_bindgen::JsCast as _;
use wasm_bindgen::prelude::Closure;

const IN_HAND: usize = 2;

struct Hand {
    worker: web_sys::Worker,
    busy: usize,
    holds: Option<(SourceId, u64)>,
    opened: Option<(SourceId, usize)>,
    _listener: Closure<dyn FnMut(web_sys::MessageEvent)>,
}

struct Answer {
    hand: usize,
    id: TileId,
    epoch: u64,
    pixels: Result<TilePixels, String>,
}

thread_local! {
    static HANDS: RefCell<Vec<Hand>> = const { RefCell::new(Vec::new()) };
    static ANSWERS: RefCell<VecDeque<Answer>> = const { RefCell::new(VecDeque::new()) };
    static CONTEXT: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
}

pub fn add_worker(worker: web_sys::Worker) {
    let hand = HANDS.with(|hands| hands.borrow().len());
    let listener =
        Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
            if let Some(answer) = read_answer(hand, &event.data()) {
                ANSWERS.with(|answers| answers.borrow_mut().push_back(answer));
                CONTEXT.with(|context| {
                    if let Some(context) = context.borrow().as_ref() {
                        context.request_repaint();
                    }
                });
            }
        });
    worker.set_onmessage(Some(listener.as_ref().unchecked_ref()));
    HANDS.with(|hands| {
        hands.borrow_mut().push(Hand {
            worker,
            busy: 0,
            holds: None,
            opened: None,
            _listener: listener,
        });
    });
}

pub(crate) fn any() -> bool {
    HANDS.with(|hands| !hands.borrow().is_empty())
}

pub(crate) fn turn(
    context: &egui::Context,
    painter: &mut Painter,
    source: Option<&ByteStore>,
    opened: Option<(SourceId, usize)>,
    credential: &[u8],
    epoch: u64,
) {
    CONTEXT.with(|kept| {
        if kept.borrow().is_none() {
            *kept.borrow_mut() = Some(context.clone());
        }
    });
    let answers = ANSWERS.with(|answers| std::mem::take(&mut *answers.borrow_mut()));
    HANDS.with(|hands| {
        let mut hands = hands.borrow_mut();
        for answer in answers {
            if let Some(hand) = hands.get_mut(answer.hand) {
                hand.busy = hand.busy.saturating_sub(1);
            }
            painter.answer_far(answer.id, answer.epoch, answer.pixels);
        }
        let Some(source) = source else {
            return;
        };
        let revision = (source.id(), epoch);
        let opened = opened.filter(|(_, length)| *length <= source.len());
        for hand in hands.iter_mut() {
            if hand.holds == Some(revision) {
                continue;
            }
            match opened {
                Some((id, length)) if hand.opened == Some((id, length)) => {
                    send_tail(&hand.worker, source, length, credential, epoch);
                }
                _ => {
                    send_document(&hand.worker, source, credential, epoch);
                    hand.opened = opened;
                }
            }
            hand.holds = Some(revision);
        }
        loop {
            let Some(hand) = hands
                .iter_mut()
                .filter(|hand| hand.busy < IN_HAND)
                .min_by_key(|hand| hand.busy)
            else {
                return;
            };
            let Some(far) = painter.take_far() else {
                return;
            };
            send_tile(&hand.worker, &far);
            hand.busy += 1;
        }
    });
}

fn send_document(worker: &web_sys::Worker, source: &ByteStore, credential: &[u8], epoch: u64) {
    let message = js_sys::Object::new();
    set(&message, "kind", &"document".into());
    set(&message, "epoch", &number(epoch));
    let bytes = uint8_array_of(source, 0, source.len());
    set(&message, "bytes", &bytes);
    set(
        &message,
        "credential",
        &js_sys::Uint8Array::from(credential),
    );
    let _ = worker.post_message_with_transfer(&message, &js_sys::Array::of1(&bytes.buffer()));
}

fn send_tail(
    worker: &web_sys::Worker,
    source: &ByteStore,
    opened: usize,
    credential: &[u8],
    epoch: u64,
) {
    let message = js_sys::Object::new();
    set(&message, "kind", &"tail".into());
    set(&message, "epoch", &number(epoch));
    set(&message, "opened", &number(opened as u64));
    let tail = uint8_array_of(source, opened, source.len());
    set(&message, "tail", &tail);
    set(
        &message,
        "credential",
        &js_sys::Uint8Array::from(credential),
    );
    let _ = worker.post_message_with_transfer(&message, &js_sys::Array::of1(&tail.buffer()));
}

pub(crate) fn uint8_array_of(source: &ByteStore, from: usize, to: usize) -> js_sys::Uint8Array {
    let to = to.min(source.len());
    let len = to.saturating_sub(from);
    let array = js_sys::Uint8Array::new_with_length(u32::try_from(len).unwrap_or(u32::MAX));
    let mut at = from;
    while at < to {
        let (start, run) = source.run_at(at);
        let run = &run[at - start..(to - start).min(run.len())];
        let offset = u32::try_from(at - from).unwrap_or(u32::MAX);
        let end = u32::try_from(at - from + run.len()).unwrap_or(u32::MAX);
        array.subarray(offset, end).copy_from(run);
        at += run.len();
    }
    array
}

fn send_tile(worker: &web_sys::Worker, far: &pdf_app::painter::FarTile) {
    let message = js_sys::Object::new();
    set(&message, "kind", &"tile".into());
    set(&message, "epoch", &number(far.epoch));
    set(&message, "page", &number(far.id.page as u64));
    set(&message, "zoom", &number(far.id.zoom as u64));
    set(&message, "col", &far.id.col.into());
    set(&message, "row", &far.id.row.into());
    set(&message, "scale", &far.scale.into());
    let window = js_sys::Array::new();
    for side in far.window {
        window.push(&side.into());
    }
    set(&message, "window", &window);
    let _ = worker.post_message(&message);
}

fn read_answer(hand: usize, data: &wasm_bindgen::JsValue) -> Option<Answer> {
    let whole = |key: &str| -> Option<u64> {
        let value = js_sys::Reflect::get(data, &key.into()).ok()?.as_f64()?;
        (value.is_finite() && value >= 0.0 && value.fract() == 0.0).then(|| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let whole = value as u64;
            whole
        })
    };
    let id = TileId {
        page: usize::try_from(whole("page")?).ok()?,
        zoom: usize::try_from(whole("zoom")?).ok()?,
        col: u32::try_from(whole("col")?).ok()?,
        row: u32::try_from(whole("row")?).ok()?,
    };
    let epoch = whole("epoch")?;
    let error = js_sys::Reflect::get(data, &"error".into())
        .ok()
        .and_then(|value| value.as_string());
    let pixels = if let Some(error) = error {
        Err(error)
    } else {
        let width = u32::try_from(whole("width")?).ok()?;
        let height = u32::try_from(whole("height")?).ok()?;
        let rgba = js_sys::Reflect::get(data, &"rgba".into())
            .ok()?
            .dyn_into::<js_sys::Uint8Array>()
            .ok()?
            .to_vec();
        if u64::from(width) * u64::from(height) * 4 != rgba.len() as u64 {
            return None;
        }
        Ok(TilePixels {
            width,
            height,
            rgba,
        })
    };
    Some(Answer {
        hand,
        id,
        epoch,
        pixels,
    })
}

fn set(object: &js_sys::Object, key: &str, value: &wasm_bindgen::JsValue) {
    let _ = js_sys::Reflect::set(object, &key.into(), value);
}

#[allow(clippy::cast_precision_loss)]
fn number(value: u64) -> wasm_bindgen::JsValue {
    (value as f64).into()
}

thread_local! {
    static HELD: RefCell<Option<Held>> = const { RefCell::new(None) };
}

struct Held {
    epoch: u64,
    source: ByteStore,
    credential: Vec<u8>,
    pages: Vec<(usize, std::sync::Arc<pdf_session::PageView>)>,
}

const PAGES_KEPT: usize = 6;

pub fn worker_document(epoch: u64, bytes: Vec<u8>, credential: Vec<u8>) {
    HELD.with(|held| {
        *held.borrow_mut() = Some(Held {
            epoch,
            source: ByteStore::owning(SourceId::next_document(), bytes),
            credential,
            pages: Vec::new(),
        });
    });
}

pub fn worker_tail(
    epoch: u64,
    opened: usize,
    tail: &[u8],
    credential: Vec<u8>,
) -> Result<(), String> {
    HELD.with(|held| {
        let mut held = held.borrow_mut();
        let source = held
            .as_ref()
            .and_then(|held| held.source.prefix(SourceId::next_document(), opened))
            .ok_or_else(|| "this worker holds no document to continue".to_owned())?
            .followed_by(SourceId::next_document(), tail.to_vec());
        *held = Some(Held {
            epoch,
            source,
            credential,
            pages: Vec::new(),
        });
        Ok(())
    })
}

pub fn worker_tile(
    epoch: u64,
    page: usize,
    scale: f64,
    window: [u32; 4],
) -> Result<TilePixels, String> {
    HELD.with(|held| {
        let mut held = held.borrow_mut();
        let held = held
            .as_mut()
            .ok_or_else(|| "this worker holds no document".to_owned())?;
        if held.epoch != epoch {
            return Err("this worker holds another revision".to_owned());
        }
        let view = if let Some(at) = held.pages.iter().position(|(kept, _)| *kept == page) {
            let kept = held.pages.remove(at);
            let view = std::sync::Arc::clone(&kept.1);
            held.pages.push(kept);
            view
        } else {
            let view = pdf_session::interpret_page_for_display(
                &held.source,
                page,
                &held.credential,
                None,
                pdf_cli::font_provider(),
            )
            .map(std::sync::Arc::new)
            .map_err(|error| error.to_string())?;
            if held.pages.len() >= PAGES_KEPT {
                held.pages.remove(0);
            }
            held.pages.push((page, std::sync::Arc::clone(&view)));
            view
        };
        let (canvas, _) = pdf_app::painter::draw_region(&view, scale, window)
            .map_err(|error| error.to_string())?;
        Ok(TilePixels {
            width: canvas.width,
            height: canvas.height,
            rgba: canvas.to_rgba8(),
        })
    })
}
