#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast as _;

    let Some(window) = web_sys::window() else {
        return;
    };
    let canvas = window
        .document()
        .and_then(|document| document.get_element_by_id("panpdf"))
        .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok());
    let Some(canvas) = canvas else {
        web_sys::console::error_1(&"no <canvas id=\"panpdf\"> on this page".into());
        return;
    };
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(reason) = pdf_window::app::start(canvas).await {
            web_sys::console::error_1(&format!("PanPDF could not start: {reason}").into());
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("panpdf-web is built for wasm32-unknown-unknown: tools/build-web.sh");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn open_pdf(name: String, bytes: Vec<u8>) {
    pdf_window::web_files::open_from_page(name, bytes);
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn open_draft(name: String, bytes: Vec<u8>) {
    pdf_window::web_files::open_draft(name, bytes);
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
#[must_use]
pub fn draft_now(write_typing: bool) -> wasm_bindgen::JsValue {
    pdf_window::web_files::draft_now(write_typing)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
#[must_use]
pub fn draft_base(document: f64, opened: f64) -> wasm_bindgen::JsValue {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pdf_window::web_files::draft_base(document as u64, opened as usize)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn add_font(name: String, bytes: Vec<u8>) {
    pdf_window::web_files::add_font(name, bytes);
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn ocr_languages(list: String) {
    pdf_window::web_files::ocr_languages(&list);
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn ocr_step(what: String) {
    pdf_window::web_files::ocr_step(what);
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn ocr_read(id: u32, tsv: String, text: String) {
    pdf_window::web_files::ocr_read(id, tsv, text);
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn ocr_failed(id: u32, error: String) {
    pdf_window::web_files::ocr_failed(id, error);
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn fonts_ready(manifest: String) {
    pdf_window::web_files::fonts_ready(&manifest);
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn add_worker(worker: web_sys::Worker) {
    pdf_window::web_tiles::add_worker(worker);
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn worker_document(epoch: f64, bytes: Vec<u8>, credential: Vec<u8>) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pdf_window::web_tiles::worker_document(epoch as u64, bytes, credential);
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn worker_tail(
    epoch: f64,
    opened: f64,
    tail: &[u8],
    credential: Vec<u8>,
) -> Result<(), String> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pdf_window::web_tiles::worker_tail(epoch as u64, opened as usize, tail, credential)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn worker_tile(epoch: f64, page: u32, scale: f64, window: Vec<u32>) -> Result<Vec<u8>, String> {
    let window: [u32; 4] = window
        .try_into()
        .map_err(|_| "a square is named by four numbers".to_owned())?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let pixels = pdf_window::web_tiles::worker_tile(epoch as u64, page as usize, scale, window)?;
    let mut bytes = Vec::with_capacity(8 + pixels.rgba.len());
    bytes.extend_from_slice(&pixels.width.to_le_bytes());
    bytes.extend_from_slice(&pixels.height.to_le_bytes());
    bytes.extend_from_slice(&pixels.rgba);
    Ok(bytes)
}
