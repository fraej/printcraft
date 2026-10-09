//! The Android side: start-up, the document PrintCraft was opened with, All files access and
//! leaving the app. Java is reached only through `jni`'s checked calls on public framework APIs;
//! every failure is reported and the app carries on without that feature.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use android_activity::AndroidApp;
use jni::objects::{JObject, JString};
use jni::{Env, JValue, JavaVM, jni_sig, jni_str};
use printcraft_ui_egui::PrintCraftApp;
use printcraft_ui_egui::browse::Platform;

use crate::storage::{document_path, places};

/// The largest document read from another app (bytes).
const MAX_DOCUMENT: usize = 1 << 30;

/// Android's entry point (NativeActivity ▸ android-activity ▸ here).
#[allow(unsafe_code)] // Exporting the symbol Android looks for; the function is safe Rust.
#[unsafe(no_mangle)]
pub fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("PrintCraft"));
    std::panic::set_hook(Box::new(|info| log::error!("internal error: {info}")));
    log::info!("PrintCraft {} starting", env!("CARGO_PKG_VERSION"));
    let internal = app.internal_data_path();
    let external = app.external_data_path();
    let activity = app.clone();
    let options = eframe::NativeOptions {
        android_app: Some(app),
        renderer: eframe::Renderer::Glow,
        persistence_path: internal.as_ref().map(|d| d.join("app.ron")),
        ..Default::default()
    };
    let started = eframe::run_native(
        "PrintCraft",
        options,
        Box::new(move |cc| {
            let mut app = PrintCraftApp::new();
            if let Some(json) = cc.storage.and_then(|s| s.get_string("printcraft")) {
                app.restore(&json);
            }
            let platform = Arc::new(Android { app: activity.clone(), access: Mutex::new(None) });
            app.places = places(external.as_deref(), platform.has_file_access());
            app.platform = Some(platform);
            app.frame_log = true;
            // Autosave unsaved changes; offer to recover what a killed session left behind.
            if let Some(dir) = &internal {
                app.enable_recovery(printcraft_ui_egui::RecoveryStore::new(dir.join("recovery")));
            }
            match launch_document() {
                Ok(Some(Launched { name, path: Some(path), .. })) => {
                    log::info!("opening {name}");
                    app.open_path(&path.to_string_lossy());
                }
                Ok(Some(Launched { name, path: None, bytes })) => {
                    if let Err(e) = app.open_bytes(&name, None, bytes) {
                        app.notify(format!("Couldn't open {name}: {e}"));
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    log::warn!("the document PrintCraft was opened with: {e}");
                    app.notify("Couldn't open the file you shared with PrintCraft");
                }
            }
            Ok(Box::new(app))
        }),
    );
    if let Err(e) = started {
        log::error!("PrintCraft stopped: {e}");
    }
    // The window closed (Back on Home, after any "save changes?"): eframe has saved the
    // settings. Close the task so that "Open with" starts afresh, and the process with it
    // (winit can't start a second event loop in the same process).
    if let Err(e) = with_env(|env| {
        let task = app_task(env)?;
        if !task.is_null() {
            env.call_method(&task, jni_str!("finishAndRemoveTask"), jni_sig!("()V"), &[])?.v()?;
        }
        Ok(())
    }) {
        log::warn!("closing the task: {e}");
    }
    std::process::exit(0);
}

/// Android services for the shell.
struct Android {
    app: AndroidApp,
    /// The last answer to "may PrintCraft see all files?" (asked at most once a second).
    access: Mutex<Option<(bool, Instant)>>,
}

impl Platform for Android {
    fn has_file_access(&self) -> bool {
        let mut cached = self.access.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((ok, at)) = *cached
            && at.elapsed() < Duration::from_secs(1)
        {
            return ok;
        }
        let ok = with_env(|env| {
            env.call_static_method(jni_str!("android/os/Environment"), jni_str!("isExternalStorageManager"), jni_sig!("()Z"), &[])?.z()
        })
        .unwrap_or_else(|e| {
            log::warn!("isExternalStorageManager: {e}");
            false
        });
        *cached = Some((ok, Instant::now()));
        ok
    }

    fn insets(&self) -> [f32; 4] {
        // NativeActivity draws on the whole window; the content rect is the part the system
        // bars leave free.
        let Some(w) = self.app.native_window() else { return [0.0; 4] };
        let (width, height) = (w.width(), w.height());
        let r = self.app.content_rect();
        if r.right <= r.left || r.bottom <= r.top {
            return [0.0; 4];
        }
        [r.left, r.top, width.saturating_sub(r.right), height.saturating_sub(r.bottom)].map(|v| v.max(0) as f32)
    }

    fn request_file_access(&self) {
        if let Err(e) = with_env(|env| {
            let ctx = context(env)?;
            let pkg = env.call_method(&ctx, jni_str!("getPackageName"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
            let pkg = env.cast_local::<JString>(pkg)?.try_to_string(env)?;
            let uri_text = env.new_string(format!("package:{pkg}"))?;
            let uri = env
                .call_static_method(
                    jni_str!("android/net/Uri"),
                    jni_str!("parse"),
                    jni_sig!("(Ljava/lang/String;)Landroid/net/Uri;"),
                    &[JValue::Object(&uri_text)],
                )?
                .l()?;
            let action = env.new_string("android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION")?;
            let intent = env.new_object(
                jni_str!("android/content/Intent"),
                jni_sig!("(Ljava/lang/String;Landroid/net/Uri;)V"),
                &[JValue::Object(&action), JValue::Object(&uri)],
            )?;
            // FLAG_ACTIVITY_NEW_TASK: started from the application, not an activity.
            env.call_method(&intent, jni_str!("addFlags"), jni_sig!("(I)Landroid/content/Intent;"), &[JValue::Int(0x1000_0000)])?;
            env.call_method(&ctx, jni_str!("startActivity"), jni_sig!("(Landroid/content/Intent;)V"), &[JValue::Object(&intent)])?.v()
        }) {
            log::warn!("asking for All files access: {e}");
        }
        *self.access.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }
}

/// Run `f` with this thread attached to the Java VM android-activity started.
fn with_env<T>(f: impl FnOnce(&mut Env) -> jni::errors::Result<T>) -> Result<T, String> {
    let vm = JavaVM::singleton().map_err(|e| e.to_string())?;
    vm.attach_current_thread(|env| f(env)).map_err(|e: jni::errors::Error| e.to_string())
}

/// The application context.
fn context<'l>(env: &mut Env<'l>) -> jni::errors::Result<JObject<'l>> {
    env.call_static_method(jni_str!("android/app/ActivityThread"), jni_str!("currentApplication"), jni_sig!("()Landroid/app/Application;"), &[])?.l()
}

/// This app's task (`ActivityManager.AppTask`), or null.
fn app_task<'l>(env: &mut Env<'l>) -> jni::errors::Result<JObject<'l>> {
    let ctx = context(env)?;
    let name = env.new_string("activity")?;
    let am =
        env.call_method(&ctx, jni_str!("getSystemService"), jni_sig!("(Ljava/lang/String;)Ljava/lang/Object;"), &[JValue::Object(&name)])?.l()?;
    if am.is_null() {
        return Ok(JObject::null());
    }
    let tasks = env.call_method(&am, jni_str!("getAppTasks"), jni_sig!("()Ljava/util/List;"), &[])?.l()?;
    if tasks.is_null() || env.call_method(&tasks, jni_str!("size"), jni_sig!("()I"), &[])?.i()? < 1 {
        return Ok(JObject::null());
    }
    env.call_method(&tasks, jni_str!("get"), jni_sig!("(I)Ljava/lang/Object;"), &[JValue::Int(0)])?.l()
}

/// The document PrintCraft was started with.
struct Launched {
    name: String,
    /// The file itself, when PrintCraft may write it (saves go back there).
    path: Option<PathBuf>,
    /// Otherwise its bytes.
    bytes: Vec<u8>,
}

/// The document in the intent that started PrintCraft's task (Open with / Share), if any.
fn launch_document() -> Result<Option<Launched>, String> {
    with_env(|env| {
        let task = app_task(env)?;
        if task.is_null() {
            return Ok(None);
        }
        let info = env.call_method(&task, jni_str!("getTaskInfo"), jni_sig!("()Landroid/app/ActivityManager$RecentTaskInfo;"), &[])?.l()?;
        if info.is_null() {
            return Ok(None);
        }
        let intent = env.get_field(&info, jni_str!("baseIntent"), jni_sig!("Landroid/content/Intent;"))?.l()?;
        if intent.is_null() {
            return Ok(None);
        }
        let action = env.call_method(&intent, jni_str!("getAction"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
        let action = string(env, action)?;
        let uri = match action.as_str() {
            "android.intent.action.VIEW" => env.call_method(&intent, jni_str!("getData"), jni_sig!("()Landroid/net/Uri;"), &[])?.l()?,
            "android.intent.action.SEND" => {
                let key = env.new_string("android.intent.extra.STREAM")?;
                env.call_method(
                    &intent,
                    jni_str!("getParcelableExtra"),
                    jni_sig!("(Ljava/lang/String;)Landroid/os/Parcelable;"),
                    &[JValue::Object(&key)],
                )?
                .l()?
            }
            _ => return Ok(None),
        };
        if uri.is_null() {
            return Ok(None);
        }
        read_uri(env, &uri).map(Some)
    })
}

/// A Java string as Rust (empty for null).
fn string(env: &mut Env, obj: JObject) -> jni::errors::Result<String> {
    if obj.is_null() {
        return Ok(String::new());
    }
    env.cast_local::<JString>(obj)?.try_to_string(env)
}

/// Read a `content:` or `file:` URI. A file in shared storage that PrintCraft may write is
/// returned as its path, so that Save writes it in place.
fn read_uri(env: &mut Env, uri: &JObject) -> jni::errors::Result<Launched> {
    let scheme = env.call_method(uri, jni_str!("getScheme"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
    let scheme = string(env, scheme)?;
    let last = env.call_method(uri, jni_str!("getLastPathSegment"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
    let last = string(env, last)?;
    if scheme == "file" {
        let path = env.call_method(uri, jni_str!("getPath"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
        let path = PathBuf::from(string(env, path)?);
        let name = path.file_name().map_or(last, |n| n.to_string_lossy().into_owned());
        return Ok(Launched { name, path: Some(path), bytes: Vec::new() });
    }
    let ctx = context(env)?;
    if let Some(path) = shared_path(env, &ctx, uri)?
        && std::fs::metadata(&path).is_ok_and(|m| m.is_file())
        && std::fs::OpenOptions::new().append(true).open(&path).is_ok()
    {
        let name = path.file_name().map_or_else(|| last.clone(), |n| n.to_string_lossy().into_owned());
        return Ok(Launched { name, path: Some(path), bytes: Vec::new() });
    }
    let resolver = env.call_method(&ctx, jni_str!("getContentResolver"), jni_sig!("()Landroid/content/ContentResolver;"), &[])?.l()?;
    let name = display_name(env, &resolver, uri).ok().filter(|n| !n.trim().is_empty()).unwrap_or(last);
    let name = if name.trim().is_empty() { "document.pdf".to_string() } else { name };
    let input =
        env.call_method(&resolver, jni_str!("openInputStream"), jni_sig!("(Landroid/net/Uri;)Ljava/io/InputStream;"), &[JValue::Object(uri)])?.l()?;
    if input.is_null() {
        return Err(jni::errors::Error::NullPtr("openInputStream"));
    }
    let buf = env.new_byte_array(64 * 1024)?;
    let mut bytes = Vec::new();
    let read = loop {
        let n = match env.call_method(&input, jni_str!("read"), jni_sig!("([B)I"), &[JValue::Object(&buf)]).and_then(|v| v.i()) {
            Ok(n) => n,
            Err(e) => break Err(e),
        };
        let Ok(n) = usize::try_from(n) else { break Ok(()) }; // -1: the end
        let chunk = env.convert_byte_array(&buf)?;
        bytes.extend_from_slice(chunk.get(..n.min(chunk.len())).unwrap_or_default());
        if bytes.len() > MAX_DOCUMENT {
            break Err(jni::errors::Error::JniCall(jni::errors::JniError::NoMemory));
        }
    };
    let _ = env.call_method(&input, jni_str!("close"), jni_sig!("()V"), &[]);
    read?;
    Ok(Launched { name, path: None, bytes })
}

/// The path of a document in shared storage (Files and Downloads providers), when PrintCraft
/// may read it directly.
fn shared_path(env: &mut Env, ctx: &JObject, uri: &JObject) -> jni::errors::Result<Option<PathBuf>> {
    let is_document = env
        .call_static_method(
            jni_str!("android/provider/DocumentsContract"),
            jni_str!("isDocumentUri"),
            jni_sig!("(Landroid/content/Context;Landroid/net/Uri;)Z"),
            &[JValue::Object(ctx), JValue::Object(uri)],
        )?
        .z()?;
    if !is_document {
        return Ok(None);
    }
    let authority = env.call_method(uri, jni_str!("getAuthority"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
    let authority = string(env, authority)?;
    let id = env
        .call_static_method(
            jni_str!("android/provider/DocumentsContract"),
            jni_str!("getDocumentId"),
            jni_sig!("(Landroid/net/Uri;)Ljava/lang/String;"),
            &[JValue::Object(uri)],
        )?
        .l()?;
    let id = string(env, id)?;
    Ok(document_path(&authority, &id))
}

/// `OpenableColumns.DISPLAY_NAME` for `uri`.
fn display_name(env: &mut Env, resolver: &JObject, uri: &JObject) -> jni::errors::Result<String> {
    let column = env.new_string("_display_name")?;
    let columns = env.new_object_array(1, jni_str!("java/lang/String"), &column)?;
    let null = JObject::null();
    let cursor = env
        .call_method(
            resolver,
            jni_str!("query"),
            jni_sig!("(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;"),
            &[JValue::Object(uri), JValue::Object(&columns), JValue::Object(&null), JValue::Object(&null), JValue::Object(&null)],
        )?
        .l()?;
    if cursor.is_null() {
        return Ok(String::new());
    }
    let name = if env.call_method(&cursor, jni_str!("moveToFirst"), jni_sig!("()Z"), &[])?.z()? {
        let s = env.call_method(&cursor, jni_str!("getString"), jni_sig!("(I)Ljava/lang/String;"), &[JValue::Int(0)])?.l()?;
        string(env, s)?
    } else {
        String::new()
    };
    let _ = env.call_method(&cursor, jni_str!("close"), jni_sig!("()V"), &[]);
    Ok(name)
}
