#![cfg(target_os = "android")]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::Duration;

use jni::JNIEnv;
use jni::objects::{JClass, JObject, JString, JValue, JValueOwned};
use jni::objects::{JFloatArray, JIntArray, JObjectArray};
use jni::sys::{jboolean, jfloatArray, jint, jintArray};
use pdf_agent::transport::{HttpAnswer, HttpCall, TransportError};
use pdf_window::android::{AndroidApp, Host};

static APP: OnceLock<AndroidApp> = OnceLock::new();

static ACTIVITY_CLASS: OnceLock<jni::objects::GlobalRef> = OnceLock::new();

#[unsafe(no_mangle)]
#[expect(
    clippy::no_mangle_with_rust_abi,
    reason = "the activity glue looks this function up by name and calls it with the Rust ABI"
)]
fn android_main(app: AndroidApp) {
    let _ = APP.set(app.clone());
    let host = Host {
        pick_pdf,
        export,
        set_clipboard,
        show_menu,
        hide_menu,
        leave,
        open_url,
        scan,
        keyboard,
        share,
        pick_files,
    };
    if let Err(why) = pdf_window::android::start(app, host) {
        eprintln!("PanPDF could not open its window: {why}");
    }
}

fn call(
    method: &str,
    signature: &str,
    make: impl for<'a> FnOnce(&mut JNIEnv<'a>) -> jni::errors::Result<Vec<JValueOwned<'a>>>,
) -> Result<(), String> {
    let app = APP.get().ok_or("no activity")?;
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }
        .map_err(|error| error.to_string())?;
    let mut env = vm
        .attach_current_thread_permanently()
        .map_err(|error| error.to_string())?;
    env.with_local_frame(8, |env| -> jni::errors::Result<()> {
        let arguments = make(env)?;
        let borrowed: Vec<JValue<'_, '_>> = arguments.iter().map(JValueOwned::borrow).collect();
        let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
        env.call_method(&activity, method, signature, &borrowed)?;
        Ok(())
    })
    .map_err(|error| error.to_string())
}

fn rgba_of(argb: &[i32]) -> Vec<u8> {
    argb.iter()
        .flat_map(|&pixel| {
            let [_, red, green, blue] = pixel.to_be_bytes();
            [red, green, blue, 255]
        })
        .collect()
}

fn string<'local>(
    env: &mut JNIEnv<'local>,
    text: &str,
) -> jni::errors::Result<JValueOwned<'local>> {
    Ok(JValueOwned::Object(JObject::from(env.new_string(text)?)))
}

fn pick_pdf() {
    if let Err(why) = call("pickPdf", "()V", |_| Ok(Vec::new())) {
        pdf_window::android::said(format!("not opened: {why}"));
    }
}

fn export(path: &Path, name: &str) {
    let path = path.to_string_lossy();
    let result = call(
        "exportFile",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        |env| Ok(vec![string(env, &path)?, string(env, name)?]),
    );
    if let Err(why) = result {
        pdf_window::android::said(format!("not saved: {why}"));
    }
}

fn set_clipboard(text: &str) {
    if let Err(why) = call("setClipboard", "(Ljava/lang/String;)V", |env| {
        Ok(vec![string(env, text)?])
    }) {
        eprintln!("PanPDF could not copy: {why}");
    }
}

fn show_menu(rect: [i32; 4], editing: bool) {
    let [left, top, right, bottom] = rect;
    let _ = call("showMenu", "(IIIIZ)V", |_| {
        Ok(vec![
            JValueOwned::Int(left),
            JValueOwned::Int(top),
            JValueOwned::Int(right),
            JValueOwned::Int(bottom),
            JValueOwned::Bool(u8::from(editing)),
        ])
    });
}

fn hide_menu() {
    let _ = call("hideMenu", "()V", |_| Ok(Vec::new()));
}

fn open_url(uri: &str) -> Result<(), String> {
    call("openUrl", "(Ljava/lang/String;)V", |env| {
        Ok(vec![string(env, uri)?])
    })
}

fn scan(into: bool) {
    if let Err(why) = call("startScan", "(Z)V", |_| {
        Ok(vec![JValueOwned::Bool(u8::from(into))])
    }) {
        pdf_window::android::said(format!("not opened: {why}"));
    }
}

fn keyboard(up: bool) {
    let _ = call("showKeyboard", "(Z)V", |_| {
        Ok(vec![JValueOwned::Bool(u8::from(up))])
    });
}

fn share(path: &Path) {
    let path = path.to_string_lossy();
    if let Err(why) = call("shareFile", "(Ljava/lang/String;)V", |env| {
        Ok(vec![string(env, &path)?])
    }) {
        pdf_window::android::said(format!("not shared: {why}"));
    }
}

fn pick_files(accepts: &[&str], several: bool, tool: &str) {
    let joined = accepts.join(",");
    let result = call(
        "pickFiles",
        "(Ljava/lang/String;ZLjava/lang/String;)V",
        |env| {
            Ok(vec![
                string(env, &joined)?,
                JValueOwned::Bool(u8::from(several)),
                string(env, tool)?,
            ])
        },
    );
    if let Err(why) = result {
        pdf_window::android::said(format!("not opened: {why}"));
    }
}

fn leave() {
    let _ = call("leave", "()V", |_| Ok(Vec::new()));
}

#[unsafe(no_mangle)]
extern "system" fn Java_org_panpdf_app_PanActivity_nativeInsets(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    top: jint,
    bottom: jint,
    left: jint,
    right: jint,
) {
    pdf_window::android::set_insets(top, bottom, left, right);
}

#[unsafe(no_mangle)]
extern "system" fn Java_org_panpdf_app_PanActivity_nativeOpened(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    path: JString<'_>,
) {
    if let Ok(path) = env.get_string(&path) {
        pdf_window::android::opened(PathBuf::from(String::from(path)));
    }
}

#[unsafe(no_mangle)]
extern "system" fn Java_org_panpdf_app_PanActivity_nativeSaid(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    what: JString<'_>,
) {
    if let Ok(what) = env.get_string(&what) {
        pdf_window::android::said(String::from(what));
    }
}

#[unsafe(no_mangle)]
extern "system" fn Java_org_panpdf_app_PanActivity_nativeCacheDir(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    path: JString<'_>,
) {
    if let Ok(path) = env.get_string(&path) {
        pdf_window::android::set_cache_dir(PathBuf::from(String::from(path)));
    }
}

#[unsafe(no_mangle)]
extern "system" fn Java_org_panpdf_app_PanActivity_nativeCommand(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    what: JString<'_>,
) {
    if let Ok(what) = env.get_string(&what) {
        pdf_window::android::command(&String::from(what));
    }
}

#[unsafe(no_mangle)]
extern "system" fn Java_org_panpdf_app_PanActivity_nativePaste(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    text: JString<'_>,
) {
    if let Ok(text) = env.get_string(&text) {
        pdf_window::android::paste(String::from(text));
    }
}

#[unsafe(no_mangle)]
extern "system" fn Java_org_panpdf_app_PanActivity_nativeScanned(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    paths: JObjectArray<'_>,
    into: jboolean,
) {
    let count = env.get_array_length(&paths).unwrap_or(0);
    let mut found = Vec::new();
    for index in 0..count {
        let Ok(item) = env.get_object_array_element(&paths, index) else {
            continue;
        };
        let item = JString::from(item);
        if let Ok(path) = env.get_string(&item) {
            found.push(PathBuf::from(String::from(path)));
        }
    }
    pdf_window::android::scanned(found, into != 0);
}

#[unsafe(no_mangle)]
extern "system" fn Java_org_panpdf_app_ScanActivity_flattenSheet<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    pixels: JIntArray<'local>,
    width: jint,
    height: jint,
    corners: JFloatArray<'local>,
    clean: jboolean,
) -> jintArray {
    let (Ok(w), Ok(h)) = (usize::try_from(width), usize::try_from(height)) else {
        return std::ptr::null_mut();
    };
    let mut argb = vec![0_i32; w * h];
    let mut points = [0_f32; 8];
    if env.get_int_array_region(&pixels, 0, &mut argb).is_err()
        || env
            .get_float_array_region(&corners, 0, &mut points)
            .is_err()
    {
        return std::ptr::null_mut();
    }
    let picture = pdf_scan::Rgba {
        width: w,
        height: h,
        pixels: rgba_of(&argb),
    };
    let sheet: pdf_scan::Corners = [
        (points[0], points[1]),
        (points[2], points[3]),
        (points[4], points[5]),
        (points[6], points[7]),
    ];
    let size = pdf_scan::true_size(&sheet, (w, h));
    let flat = pdf_scan::flatten(&picture, &sheet, size);
    let page = if clean != 0 {
        pdf_scan::clean(&flat)
    } else {
        pdf_scan::enhance(&flat)
    };
    let (Ok(pw), Ok(ph)) = (i32::try_from(page.width), i32::try_from(page.height)) else {
        return std::ptr::null_mut();
    };
    let mut out: Vec<i32> = Vec::with_capacity(page.width * page.height + 2);
    out.push(pw);
    out.push(ph);
    out.extend(
        page.pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| i32::from_be_bytes([0xFF, p[0], p[1], p[2]])),
    );
    let Ok(length) = i32::try_from(out.len()) else {
        return std::ptr::null_mut();
    };
    let Ok(array) = env.new_int_array(length) else {
        return std::ptr::null_mut();
    };
    if env.set_int_array_region(&array, 0, &out).is_err() {
        return std::ptr::null_mut();
    }
    array.into_raw()
}

#[unsafe(no_mangle)]
extern "system" fn Java_org_panpdf_app_PanActivity_nativePaths(
    mut env: JNIEnv<'_>,
    class: JClass<'_>,
    libraries: JString<'_>,
    files: JString<'_>,
) {
    if let (Ok(libraries), Ok(files)) = (env.get_string(&libraries), env.get_string(&files)) {
        let libraries = PathBuf::from(String::from(libraries));
        let files = PathBuf::from(String::from(files));
        pdf_ocr::tesseract::use_program(libraries.join("libtesseract.so"));
        pdf_ocr::store::keep_data_in(files.join("panpdf"));
        pdf_window::android::set_paths(libraries.clone(), files.clone());
        pdf_ocr::store::download_with(download);
        pdf_agent::transport::send_with(send);
        pdf_cli::use_package_root(files.join("fonts"));
    }
    if let Ok(global) = env.new_global_ref(&class) {
        let _ = ACTIVITY_CLASS.set(global);
    }
}

fn in_the_activity_class<R>(
    make: impl for<'a> FnOnce(&mut JNIEnv<'a>, &JClass<'a>) -> jni::errors::Result<R>,
) -> Result<R, String> {
    let app = APP.get().ok_or("no activity")?;
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }
        .map_err(|error| error.to_string())?;
    let mut env = vm
        .attach_current_thread_permanently()
        .map_err(|error| error.to_string())?;
    env.with_local_frame(64, |env| -> jni::errors::Result<R> {
        let class = ACTIVITY_CLASS
            .get()
            .map(|global| JClass::from(env.new_local_ref(global.as_obj()).unwrap_or_default()))
            .ok_or(jni::errors::Error::NullPtr("the activity's class"))?;
        make(env, &class).inspect_err(|_| {
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_clear();
            }
        })
    })
    .map_err(|error| error.to_string())
}

fn download(address: &str, path: &Path) -> Result<(), String> {
    let path = path.to_string_lossy();
    in_the_activity_class(|env, class| {
        let address = env.new_string(address)?;
        let path = env.new_string(&*path)?;
        let answer = env
            .call_static_method(
                class,
                "download",
                "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&address), JValue::Object(&path)],
            )?
            .l()?;
        if answer.is_null() {
            return Ok(None);
        }
        let answer = JString::from(answer);
        Ok(Some(env.get_string(&answer)?.into()))
    })?
    .map_or(Ok(()), Err)
}

static NEXT_CALL: AtomicI32 = AtomicI32::new(1);

const STOP_WATCH: Duration = Duration::from_millis(50);

fn seconds(wait: Duration) -> i32 {
    i32::try_from(wait.as_secs().max(1)).unwrap_or(i32::MAX)
}

fn send(call: &HttpCall<'_>) -> Result<HttpAnswer, TransportError> {
    let id = NEXT_CALL.fetch_add(1, Ordering::Relaxed);
    let over = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !over.load(Ordering::Acquire) {
                if call.cancel.load(Ordering::Relaxed) {
                    let _ = in_the_activity_class(|env, class| {
                        env.call_static_method(class, "abort", "(I)V", &[JValue::Int(id)])
                            .map(|_| ())
                    });
                    return;
                }
                std::thread::sleep(STOP_WATCH);
            }
        });
        let answer = request(id, call);
        over.store(true, Ordering::Release);
        answer
    })
}

fn request(id: i32, call: &HttpCall<'_>) -> Result<HttpAnswer, TransportError> {
    let heard = in_the_activity_class(|env, class| {
        let method = env.new_string(call.method)?;
        let address = env.new_string(call.url)?;
        let strings = env.find_class("java/lang/String")?;
        let nothing = JObject::null();
        let headers = env.new_object_array(
            i32::try_from(call.headers.len()).unwrap_or(i32::MAX),
            &strings,
            &nothing,
        )?;
        for (at, header) in (0..).zip(call.headers) {
            let header = env.new_string(header)?;
            env.set_object_array_element(&headers, at, &header)?;
        }
        let body: JObject<'_> = match call.body {
            Some(bytes) => JObject::from(env.byte_array_from_slice(bytes)?),
            None => JObject::null(),
        };
        let answer = env
            .call_static_method(
                class,
                "http",
                "(ILjava/lang/String;Ljava/lang/String;[Ljava/lang/String;[BII)[Ljava/lang/String;",
                &[
                    JValue::Int(id),
                    JValue::Object(&method),
                    JValue::Object(&address),
                    JValue::Object(&headers),
                    JValue::Object(&body),
                    JValue::Int(seconds(call.idle)),
                    JValue::Int(seconds(call.cap)),
                ],
            )?
            .l()?;
        let answer = JObjectArray::from(answer);
        let mut parts = Vec::new();
        for at in 0..3 {
            let part = JString::from(env.get_object_array_element(&answer, at)?);
            parts.push(String::from(env.get_string(&part)?));
        }
        Ok(parts)
    })
    .map_err(TransportError::Unreachable)?;
    let [first, second, third] = <[String; 3]>::try_from(heard)
        .map_err(|_| TransportError::Unreachable("the phone gave no answer".to_owned()))?;
    match first.as_str() {
        "cancelled" => Err(TransportError::Cancelled),
        "timeout" => Err(TransportError::TimedOut),
        "error" => Err(TransportError::Unreachable(second)),
        status => status
            .parse::<u16>()
            .map(|status| HttpAnswer {
                status,
                headers: second,
                body: third.into_bytes(),
            })
            .map_err(|_| TransportError::Unreachable("the phone gave no status".to_owned())),
    }
}

#[unsafe(no_mangle)]
extern "system" fn Java_org_panpdf_app_TypingView_nativeTyped(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    text: JString<'_>,
) {
    if let Ok(text) = env.get_string(&text) {
        pdf_window::android::typed(String::from(text));
    }
}

#[unsafe(no_mangle)]
extern "system" fn Java_org_panpdf_app_TypingView_nativeKey(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    code: jint,
) {
    pdf_window::android::key(code);
}

#[unsafe(no_mangle)]
extern "system" fn Java_org_panpdf_app_ScanActivity_lookForSheet<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    pixels: JIntArray<'local>,
    width: jint,
    height: jint,
    turns: jint,
) -> jfloatArray {
    let (Ok(w), Ok(h), Ok(turns)) = (
        usize::try_from(width),
        usize::try_from(height),
        u8::try_from(turns & 3),
    ) else {
        return std::ptr::null_mut();
    };
    let mut argb = vec![0_i32; w * h];
    if env.get_int_array_region(&pixels, 0, &mut argb).is_err() {
        return std::ptr::null_mut();
    }
    let picture = pdf_scan::Rgba {
        width: w,
        height: h,
        pixels: rgba_of(&argb),
    };
    let Some(found) = pdf_scan::quad::carried().and_then(|net| net.look(&picture, turns)) else {
        return std::ptr::null_mut();
    };
    let mut flat: Vec<f32> = found.corners.iter().flat_map(|(x, y)| [*x, *y]).collect();
    flat.push(found.sureness);
    let Ok(out) = env.new_float_array(9) else {
        return std::ptr::null_mut();
    };
    if env.set_float_array_region(&out, 0, &flat).is_err() {
        return std::ptr::null_mut();
    }
    out.into_raw()
}

#[unsafe(no_mangle)]
extern "system" fn Java_org_panpdf_app_ScanActivity_cornersInPhoto<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    pixels: JIntArray<'local>,
    width: jint,
    height: jint,
) -> jfloatArray {
    let (Ok(w), Ok(h)) = (usize::try_from(width), usize::try_from(height)) else {
        return std::ptr::null_mut();
    };
    let mut argb = vec![0_i32; w * h];
    if env.get_int_array_region(&pixels, 0, &mut argb).is_err() {
        return std::ptr::null_mut();
    }
    let picture = pdf_scan::Rgba {
        width: w,
        height: h,
        pixels: rgba_of(&argb),
    };
    let Some(corners) = pdf_scan::corners_in_photo(&picture) else {
        return std::ptr::null_mut();
    };
    let flat: Vec<f32> = corners.iter().flat_map(|(x, y)| [*x, *y]).collect();
    let Ok(out) = env.new_float_array(8) else {
        return std::ptr::null_mut();
    };
    if env.set_float_array_region(&out, 0, &flat).is_err() {
        return std::ptr::null_mut();
    }
    out.into_raw()
}

#[unsafe(no_mangle)]
extern "system" fn Java_org_panpdf_app_PanActivity_nativeToolFiles(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    tool: JString<'_>,
    paths: JObjectArray<'_>,
) {
    let Ok(tool) = env.get_string(&tool).map(String::from) else {
        return;
    };
    let count = env.get_array_length(&paths).unwrap_or(0);
    let mut found = Vec::new();
    for index in 0..count {
        let Ok(item) = env.get_object_array_element(&paths, index) else {
            continue;
        };
        let item = JString::from(item);
        if let Ok(path) = env.get_string(&item) {
            found.push(PathBuf::from(String::from(path)));
        }
    }
    pdf_window::android_tools::chosen_for_tool(tool, found);
}
