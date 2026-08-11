//! Soft keyboard + desktop clipboard helpers.
//!
//! Vidya's `keyboard-api` branch used to export these; current vidya `main`
//! does not. Browse keeps a small local copy so path-dep builds against sibling
//! vidya work. Android paste still goes through [`crate::android_clipboard`]
//! (EditText focus for API 29+).

use egui::Context;

#[cfg(target_os = "android")]
use egui::Id;

#[cfg(target_os = "android")]
use std::sync::OnceLock;

#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
static ANDROID_APP: OnceLock<AndroidApp> = OnceLock::new();

#[cfg(target_os = "android")]
const SOFT_KB_SHOWN_ID: &str = "browse.soft_keyboard_shown";

/// Install the [`AndroidApp`] handle used by [`sync_soft_keyboard`].
///
/// Call once from `android_main` **before** `eframe::run_native` (clone the
/// app — eframe takes ownership of the other copy).
#[cfg(target_os = "android")]
pub fn install_android_app(app: AndroidApp) {
    let _ = ANDROID_APP.set(app);
}

/// Show or hide the platform soft keyboard to match egui focus.
///
/// Call **once per frame** after building UI so [`Context::wants_keyboard_input`]
/// reflects this frame’s focus. On desktop this is a no-op.
pub fn sync_soft_keyboard(ctx: &Context) {
    let want = ctx.wants_keyboard_input();

    #[cfg(target_os = "android")]
    {
        let id = Id::new(SOFT_KB_SHOWN_ID);
        let was = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        if want == was {
            return;
        }
        ctx.data_mut(|d| d.insert_temp(id, want));
        let Some(app) = ANDROID_APP.get() else {
            return;
        };
        if want {
            // `show_implicit: false` → forced show (user tapped a field).
            app.show_soft_input(false);
        } else {
            // `hide_implicit_only: false` → always hide.
            app.hide_soft_input(false);
        }
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = (ctx, want);
    }
}

/// Read plain text from the **system** clipboard (desktop via `arboard`).
///
/// On Android prefer [`crate::android_clipboard::request`] /
/// [`crate::android_clipboard::poll`] — this blocking path is for desktop.
pub fn clipboard_text() -> Option<String> {
    #[cfg(target_os = "android")]
    {
        None
    }
    #[cfg(not(target_os = "android"))]
    {
        let mut cb = arboard::Clipboard::new().ok()?;
        cb.get_text().ok().filter(|s| !s.is_empty())
    }
}

/// Write plain text to the **system** clipboard (desktop via `arboard`).
///
/// On Android also call [`crate::android_clipboard::set_text`].
pub fn set_clipboard_text(text: &str) {
    #[cfg(target_os = "android")]
    {
        let _ = text;
    }
    #[cfg(not(target_os = "android"))]
    {
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.set_text(text.to_owned());
        }
    }
}

/// Trim BOM / surrounding whitespace from pasted clipboard text.
pub fn normalize_paste(raw: &str) -> String {
    raw.trim_start_matches('\u{feff}').trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::normalize_paste;

    #[test]
    fn normalize_strips_bom_and_whitespace() {
        assert_eq!(normalize_paste("\u{feff}  rad:zabc\n"), "rad:zabc");
        assert_eq!(normalize_paste("rad:zabc"), "rad:zabc");
    }
}
