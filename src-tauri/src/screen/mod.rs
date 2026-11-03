//! Kusema "Ask about my screen": captures the active window when the hotkey is
//! pressed, flashes a border around it, and (with the user's go-ahead on the
//! confirmation card) lets Assistant mode send it with the question.
//!
//! Screenshots live in memory only. They are never written to disk, logged or
//! stored in History, and are dropped when the conversation is cleared or the
//! app exits.

mod capture;
pub mod card;
mod flash;
pub mod image;

pub use card::Confirmation;

use ::image::RgbImage;
use log::{debug, info};
use std::sync::Mutex;
use std::time::Instant;
use tauri::{AppHandle, Manager};

/// A rectangle on the virtual screen, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl ScreenRect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    fn intersect(&self, other: &ScreenRect) -> ScreenRect {
        ScreenRect {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        }
    }
}

/// The window a screenshot was taken of.
#[derive(Debug, Clone, Copy)]
pub struct CapturedWindow {
    /// Native handle (HWND on Windows), to give focus back after the card.
    handle: isize,
    pub rect: ScreenRect,
}

impl CapturedWindow {
    pub fn handle(&self) -> isize {
        self.handle
    }
}

/// Kusema voice editing: the window in front, to put cards beside and give
/// focus back to, refusing the same windows a screenshot would.
pub fn foreground_target() -> Result<CapturedWindow, CaptureError> {
    capture::foreground_window()
}

/// Kusema vocabulary: brings the window with this handle to the front.
pub fn activate_handle(handle: isize) {
    capture::activate(handle);
}

/// Give focus back to `window`.
pub fn activate(window: &CapturedWindow) {
    restore_focus(window);
}

/// Kusema type-to-ask: the work area and scale of the monitor showing most of
/// `rect`.
pub fn work_area_near(rect: ScreenRect) -> Option<(ScreenRect, f64)> {
    capture::monitor_work_area(rect)
}

/// Kusema type-to-ask: capture `window`, which must be back in front (the Ask
/// box has stepped aside for it), and flash a border around it.
pub fn capture_window(window: CapturedWindow) -> Result<Capture, CaptureError> {
    let now = capture::foreground_window()?;
    if now.handle != window.handle {
        return Err(CaptureError::NoWindow);
    }
    let capture = capture::capture(now)?;
    flash::flash(capture.window.rect);
    Ok(capture)
}

/// A screenshot of one window, as captured.
pub struct Capture {
    pub image: RgbImage,
    pub window: CapturedWindow,
}

impl std::fmt::Debug for Capture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Capture")
            .field("width", &self.image.width())
            .field("height", &self.image.height())
            .field("window", &self.window)
            .finish()
    }
}

/// Why the active window wasn't captured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    NoWindow,
    OwnWindow,
    Desktop,
    Taskbar,
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    Unsupported,
    Failed(String),
}

impl CaptureError {
    pub fn message(&self) -> String {
        const CLICK: &str = "Click the window you want to ask about, then try again.";
        match self {
            CaptureError::NoWindow => format!("There's no window to look at. {CLICK}"),
            CaptureError::OwnWindow => format!("Kusema can't look at its own window. {CLICK}"),
            CaptureError::Desktop => format!("That's the desktop, not a window. {CLICK}"),
            CaptureError::Taskbar => {
                format!("That's the taskbar or Start menu, not a window. {CLICK}")
            }
            CaptureError::Unsupported => {
                "Ask about my screen only works on Windows for now.".to_string()
            }
            CaptureError::Failed(e) => format!("Couldn't capture the window ({e})."),
        }
    }
}

/// Tauri managed state.
#[derive(Default)]
pub struct ScreenState {
    /// Captured at the hotkey press, waiting for the question.
    pending: Mutex<Option<Capture>>,
    card: card::CardSlot,
}

impl ScreenState {
    fn pending(&self) -> std::sync::MutexGuard<'_, Option<Capture>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Capture the foreground window for the question about to be recorded and
/// flash a border around it. Returns `false`, after telling the user why,
/// when there's nothing that can be captured; the recording shouldn't start.
pub fn capture_at_press(app: &AppHandle) -> bool {
    let Some(state) = app.try_state::<ScreenState>() else {
        return false;
    };
    // An unused capture from an earlier press goes, whatever happens now.
    state.pending().take();

    let started = Instant::now();
    match capture::foreground_window().and_then(capture::capture) {
        Ok(capture) => {
            debug!(
                "Captured the active window ({}x{}) in {:?}",
                capture.image.width(),
                capture.image.height(),
                started.elapsed()
            );
            flash::flash(capture.window.rect);
            *state.pending() = Some(capture);
            true
        }
        Err(e) => {
            info!("Not capturing the active window: {e:?}");
            card::show_notice(app, e.message());
            false
        }
    }
}

/// The capture taken at the last press, if it hasn't been used yet.
pub fn take_capture(app: &AppHandle) -> Option<Capture> {
    app.try_state::<ScreenState>()?.pending().take()
}

/// Drop any screenshot not yet sent (the conversation was cleared).
pub fn discard(app: &AppHandle) {
    if let Some(state) = app.try_state::<ScreenState>() {
        state.pending().take();
    }
}

/// Give focus back to the captured window.
fn restore_focus(window: &CapturedWindow) {
    capture::activate(window.handle);
}
