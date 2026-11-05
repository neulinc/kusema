//! Kusema: the native title bar in the app's own colours, so it blends into
//! the window. Windows 11 lets an app set the caption, its text and the
//! window border (DWM attributes 34 to 36); Windows 10 ignores them and keeps
//! its own dark or light title bar, which `set_theme` already matches. A
//! custom-drawn title bar was weighed and not used: snap layouts, the
//! maximize button's flyout, dragging and keyboard access would all have to
//! be rebuilt, and none of that is reliable on both versions.

/// A `#rrggbb` colour as Windows' COLORREF (0x00BBGGRR).
pub fn colorref(rgb: u32) -> u32 {
    let (r, g, b) = ((rgb >> 16) & 0xff, (rgb >> 8) & 0xff, rgb & 0xff);
    (b << 16) | (g << 8) | r
}

/// The caption, its text and the border for a theme: the window background,
/// primary text and hairline line of src/styles/theme.css.
pub fn colors(dark: bool) -> (u32, u32, u32) {
    if dark {
        (colorref(0x18181b), colorref(0xececee), colorref(0x2e2e33))
    } else {
        (colorref(0xf6f6f7), colorref(0x18181b), colorref(0xe4e4e7))
    }
}

/// Colours the main window's title bar for the theme it shows now.
#[cfg(target_os = "windows")]
pub fn match_title_bar(window: &tauri::WebviewWindow) {
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};

    const BORDER: i32 = 34;
    const CAPTION: i32 = 35;
    const TEXT: i32 = 36;

    let dark = matches!(window.theme(), Ok(tauri::Theme::Dark));
    let (caption, text, border) = colors(dark);
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    for (attribute, value) in [(CAPTION, caption), (TEXT, text), (BORDER, border)] {
        // Windows 10 refuses these; its own themed title bar stays.
        let result = unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWINDOWATTRIBUTE(attribute),
                &value as *const u32 as *const std::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            )
        };
        if let Err(e) = result {
            log::debug!("Title bar colour {attribute} not applied (Windows 10?): {e}");
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn match_title_bar(_window: &tauri::WebviewWindow) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_given_to_windows_as_bgr() {
        assert_eq!(colorref(0x18181b), 0x001b1818);
        assert_eq!(colorref(0x112233), 0x00332211);
    }

    #[test]
    fn each_theme_has_a_matching_caption_and_readable_text() {
        let (caption, text, _) = colors(true);
        assert_eq!(caption, colorref(0x18181b));
        assert_ne!(caption, text);
        let (caption, text, _) = colors(false);
        assert_eq!(caption, colorref(0xf6f6f7));
        assert_eq!(text, colorref(0x18181b));
    }
}
