//! System clipboard for Android NativeActivity.
//!
//! egui-winit has no OS clipboard on Android. Vidya's helpers post to the Java
//! UI thread, but API 29+ often still returns null unless a real text `View`
//! holds input focus. We create a 1×1 `EditText`, focus it, then read
//! `ClipboardManager`.

use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use jni::objects::{JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};
use winit::platform::android::activity::AndroidApp;

static ANDROID_APP: OnceLock<AndroidApp> = OnceLock::new();
static PENDING_GET: OnceLock<Mutex<Option<Option<String>>>> = OnceLock::new();

fn pending_get() -> &'static Mutex<Option<Option<String>>> {
    PENDING_GET.get_or_init(|| Mutex::new(None))
}

/// Store the activity handle (clone before eframe takes ownership).
pub fn install(app: AndroidApp) {
    let _ = ANDROID_APP.set(app);
}

/// Kick off a UI-thread clipboard read. Pair with [`poll`].
pub fn request() {
    let Some(app) = ANDROID_APP.get().cloned() else {
        log::warn!("browse clipboard: AndroidApp not installed");
        *pending_get().lock().unwrap_or_else(|e| e.into_inner()) = Some(None);
        return;
    };
    *pending_get().lock().unwrap_or_else(|e| e.into_inner()) = None;

    let app_for_jni = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        let text = with_env(&app_for_jni, read_clipboard);
        if text.is_none() {
            log::warn!("browse clipboard: UI-thread get returned None");
        } else {
            log::info!("browse clipboard: got {} chars", text.as_ref().map(|s| s.len()).unwrap_or(0));
        }
        *pending_get().lock().unwrap_or_else(|e| e.into_inner()) = Some(text);
    }));
}

/// `None` = still pending; `Some(None)` = failed/empty; `Some(Some(text))` = ok.
pub fn poll() -> Option<Option<String>> {
    pending_get()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
}

/// Write plain text to the system clipboard (fire-and-forget on UI thread).
pub fn set_text(text: &str) {
    let text = text.to_owned();
    let Some(app) = ANDROID_APP.get().cloned() else {
        log::warn!("browse clipboard: AndroidApp not installed (set)");
        return;
    };
    let app_for_jni = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        let ok = with_env(&app_for_jni, |env, activity| write_clipboard(env, activity, &text));
        if ok.is_none() {
            log::warn!("browse clipboard: UI-thread set failed");
        }
    }));
}

fn with_env<T>(
    app: &AndroidApp,
    f: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>) -> Option<T>,
) -> Option<T> {
    let vm_ptr = app.vm_as_ptr();
    let activity_ptr = app.activity_as_ptr();
    if vm_ptr.is_null() || activity_ptr.is_null() {
        log::warn!("browse clipboard: null JavaVM or Activity");
        return None;
    }
    let vm = unsafe { JavaVM::from_raw(vm_ptr.cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    // SAFETY: activity jobject owned by AndroidApp for the process lifetime.
    let activity = unsafe { JObject::from_raw(activity_ptr as jni::sys::jobject) };
    f(&mut *env, &activity)
}

fn read_clipboard(env: &mut JNIEnv<'_>, activity: &JObject<'_>) -> Option<String> {
    // NativeActivity's SurfaceView is not an editor; API 29+ needs a focused
    // text view before getPrimaryClip returns data for this uid.
    let _edit = ensure_focus_edit_text(env, activity);

    let cm = clipboard_manager(env, activity)?;
    if let Some(s) = primary_clip_text(env, activity, &cm) {
        return Some(s);
    }

    // Deprecated getText() — still works on some OEM builds.
    let r = env.call_method(&cm, "getText", "()Ljava/lang/CharSequence;", &[]);
    let cs = clear_exc(env, r).ok()?.l().ok()?;
    char_seq_to_string(env, &cs)
}

fn write_clipboard(env: &mut JNIEnv<'_>, activity: &JObject<'_>, text: &str) -> Option<()> {
    let _edit = ensure_focus_edit_text(env, activity);
    let cm = clipboard_manager(env, activity)?;
    let label = env.new_string("browse").ok()?;
    let value = env.new_string(text).ok()?;
    let label_obj = JObject::from(label);
    let value_obj = JObject::from(value);
    let r = env.call_static_method(
        "android/content/ClipData",
        "newPlainText",
        "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Landroid/content/ClipData;",
        &[JValue::Object(&label_obj), JValue::Object(&value_obj)],
    );
    let clip = clear_exc(env, r).ok()?.l().ok()?;
    let r = env.call_method(
        &cm,
        "setPrimaryClip",
        "(Landroid/content/ClipData;)V",
        &[JValue::Object(&clip)],
    );
    let _ = clear_exc(env, r);
    Some(())
}

/// Attach a 1×1 focusable EditText under the content root and request focus.
fn ensure_focus_edit_text<'a>(
    env: &mut JNIEnv<'a>,
    activity: &JObject<'_>,
) -> Option<JObject<'a>> {
    let content_id = clear_exc(env, env.get_static_field("android/R$id", "content", "I"))
        .ok()?
        .i()
        .ok()?;
    let r = env.call_method(
        activity,
        "findViewById",
        "(I)Landroid/view/View;",
        &[JValue::Int(content_id)],
    );
    let content = clear_exc(env, r).ok()?.l().ok()?;
    if content.is_null() {
        return None;
    }

    // Reuse a previously attached edit if present (tagged via setTag).
    let tag_key = env.new_string("browse.clipboard.focus").ok()?;
    let tag_key_obj = JObject::from(tag_key);
    let r = env.call_method(
        &content,
        "findViewWithTag",
        "(Ljava/lang/Object;)Landroid/view/View;",
        &[JValue::Object(&tag_key_obj)],
    );
    if let Ok(existing) = clear_exc(env, r).and_then(|v| v.l()) {
        if !existing.is_null() {
            focus_view(env, &existing);
            return Some(existing);
        }
    }

    let edit = clear_exc(
        env,
        env.new_object(
            "android/widget/EditText",
            "(Landroid/content/Context;)V",
            &[JValue::Object(activity)],
        ),
    )
    .ok()?;
    if edit.is_null() {
        return None;
    }

    let _ = clear_exc(
        env,
        env.call_method(&edit, "setTag", "(Ljava/lang/Object;)V", &[JValue::Object(&tag_key_obj)]),
    );
    let _ = clear_exc(
        env,
        env.call_method(&edit, "setFocusable", "(Z)V", &[JValue::Bool(true.into())]),
    );
    let _ = clear_exc(
        env,
        env.call_method(
            &edit,
            "setFocusableInTouchMode",
            "(Z)V",
            &[JValue::Bool(true.into())],
        ),
    );
    // Keep it on-screen but tiny so it can take IME/clipboard focus.
    let lp = clear_exc(
        env,
        env.new_object(
            "android/widget/FrameLayout$LayoutParams",
            "(II)V",
            &[JValue::Int(1), JValue::Int(1)],
        ),
    )
    .ok()?;
    let r = env.call_method(
        &content,
        "addView",
        "(Landroid/view/View;Landroid/view/ViewGroup$LayoutParams;)V",
        &[JValue::Object(&edit), JValue::Object(&lp)],
    );
    let _ = clear_exc(env, r);
    focus_view(env, &edit);
    Some(edit)
}

fn focus_view(env: &mut JNIEnv<'_>, view: &JObject<'_>) {
    let _ = clear_exc(
        env,
        env.call_method(view, "setFocusable", "(Z)V", &[JValue::Bool(true.into())]),
    );
    let _ = clear_exc(
        env,
        env.call_method(
            view,
            "setFocusableInTouchMode",
            "(Z)V",
            &[JValue::Bool(true.into())],
        ),
    );
    let _ = clear_exc(env, env.call_method(view, "requestFocus", "()Z", &[]));
    let _ = clear_exc(env, env.call_method(view, "requestFocusFromTouch", "()Z", &[]));

    // Also ask InputMethodManager to notice the view (helps some OEMs).
    if let Ok(imm_name) = env.new_string("input_method") {
        let imm_name_obj = JObject::from(imm_name);
        // Walk up to activity context: view.getContext()
        if let Ok(ctx) = clear_exc(
            env,
            env.call_method(view, "getContext", "()Landroid/content/Context;", &[]),
        )
        .and_then(|v| v.l())
        {
            if let Ok(imm) = clear_exc(
                env,
                env.call_method(
                    &ctx,
                    "getSystemService",
                    "(Ljava/lang/String;)Ljava/lang/Object;",
                    &[JValue::Object(&imm_name_obj)],
                ),
            )
            .and_then(|v| v.l())
            {
                if !imm.is_null() {
                    let _ = clear_exc(
                        env,
                        env.call_method(
                            &imm,
                            "restartInput",
                            "(Landroid/view/View;)V",
                            &[JValue::Object(view)],
                        ),
                    );
                }
            }
        }
    }
}

fn clipboard_manager<'a>(
    env: &mut JNIEnv<'a>,
    activity: &JObject<'_>,
) -> Option<JObject<'a>> {
    let service = clear_exc(
        env,
        env.get_static_field(
            "android/content/Context",
            "CLIPBOARD_SERVICE",
            "Ljava/lang/String;",
        ),
    )
    .ok()
    .and_then(|v| v.l().ok())
    .or_else(|| env.new_string("clipboard").ok().map(JObject::from))?;

    let r = env.call_method(
        activity,
        "getSystemService",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        &[JValue::Object(&service)],
    );
    let cm = clear_exc(env, r).ok()?.l().ok()?;
    if cm.is_null() {
        None
    } else {
        Some(cm)
    }
}

fn primary_clip_text(
    env: &mut JNIEnv<'_>,
    activity: &JObject<'_>,
    cm: &JObject<'_>,
) -> Option<String> {
    let r = env.call_method(cm, "getPrimaryClip", "()Landroid/content/ClipData;", &[]);
    let clip = clear_exc(env, r).ok()?.l().ok()?;
    if clip.is_null() {
        return None;
    }
    let r = env.call_method(&clip, "getItemCount", "()I", &[]);
    let count = clear_exc(env, r).ok()?.i().ok()?;
    if count <= 0 {
        return None;
    }
    let r = env.call_method(
        &clip,
        "getItemAt",
        "(I)Landroid/content/ClipData$Item;",
        &[JValue::Int(0)],
    );
    let item = clear_exc(env, r).ok()?.l().ok()?;
    if item.is_null() {
        return None;
    }

    let r = env.call_method(&item, "getText", "()Ljava/lang/CharSequence;", &[]);
    if let Some(s) = clear_exc(env, r)
        .ok()
        .and_then(|v| v.l().ok())
        .and_then(|cs| char_seq_to_string(env, &cs))
    {
        return Some(s);
    }

    let r = env.call_method(
        &item,
        "coerceToText",
        "(Landroid/content/Context;)Ljava/lang/CharSequence;",
        &[JValue::Object(activity)],
    );
    let cs = clear_exc(env, r).ok()?.l().ok()?;
    char_seq_to_string(env, &cs)
}

fn char_seq_to_string(env: &mut JNIEnv<'_>, cs: &JObject<'_>) -> Option<String> {
    if cs.is_null() {
        return None;
    }
    let r = env.call_method(cs, "toString", "()Ljava/lang/String;", &[]);
    let jstr_obj = clear_exc(env, r).ok()?.l().ok()?;
    if jstr_obj.is_null() {
        return None;
    }
    let jstr = JString::from(jstr_obj);
    let s: String = env.get_string(&jstr).ok()?.into();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn clear_exc<T>(
    env: &mut JNIEnv<'_>,
    result: jni::errors::Result<T>,
) -> jni::errors::Result<T> {
    if result.is_err() || env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    result
}

/// Blocking read — only for tests / tooling; prefer [`request`] / [`poll`].
#[allow(dead_code)]
pub fn get_text_blocking() -> Option<String> {
    let Some(app) = ANDROID_APP.get().cloned() else {
        return None;
    };
    let (tx, rx) = mpsc::channel();
    let app_for_jni = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        let _ = tx.send(with_env(&app_for_jni, read_clipboard));
    }));
    match rx.recv_timeout(Duration::from_millis(1500)) {
        Ok(v) => v,
        Err(_) => {
            log::warn!("browse clipboard: timed out waiting for UI thread");
            None
        }
    }
}
