//! Kusema type-to-ask: a hotkey opens a small "Ask Kusema" box near the
//! window in front. Type a question, press Enter, and the answer streams into
//! the box; Escape closes it and gives focus back. It's the same Assistant as
//! spoken questions: one conversation, the same provider, Privileged mode and
//! ledger, and "…and type it" types the answer into that window.
//!
//! The box is one webview window (`ask_box`), created at the first press and
//! hidden, not destroyed, when it closes.

use log::{debug, info, warn};
use serde::Serialize;
use specta::Type;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow};

use crate::actions::ShortcutAction;
use crate::assistant::insert::DocTarget;
use crate::assistant::{AskOptions, Source};
use crate::screen::{CapturedWindow, ScreenRect};

pub const BINDING: &str = "ask_box";
const LABEL: &str = "ask_box";
/// Logical size: the width, and the height with no answer yet.
const WIDTH: f64 = 520.0;
const MIN_HEIGHT: f64 = 132.0;
const MAX_HEIGHT: f64 = 460.0;
/// Space from the work area's edge, logical pixels.
const GAP: f64 = 12.0;
/// Time a window needs to come to the front and redraw before it's captured.
const SETTLE_BEFORE_CAPTURE: Duration = Duration::from_millis(250);

/// What a press of the hotkey does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    Open,
    /// Open and in front: the hotkey closes it again.
    Close,
    /// Open behind another window: bring it back.
    Focus,
}

pub fn on_hotkey(open: bool, focused: bool) -> HotkeyAction {
    match (open, focused) {
        (false, _) => HotkeyAction::Open,
        (true, true) => HotkeyAction::Close,
        (true, false) => HotkeyAction::Focus,
    }
}

/// Focus goes back to the window the box was opened over only while the box
/// still has it: after a click elsewhere, the user has chosen where to be.
pub fn refocus_on_close(box_focused: bool, has_window: bool) -> bool {
    box_focused && has_window
}

/// Clicking away closes a box with nothing in it; a question or an answer
/// stays up, and so does the box while a question is out (the screenshot card
/// takes the focus for a moment).
pub fn close_on_blur(empty: bool, has_answer: bool, busy: bool) -> bool {
    empty && !has_answer && !busy
}

/// Where the box goes: over the upper part of the window it was opened from,
/// centred on it, kept inside the work area; with no window, the upper third
/// of the screen.
pub fn place(
    near: Option<ScreenRect>,
    work: ScreenRect,
    width: i32,
    height: i32,
    gap: i32,
) -> (i32, i32) {
    let (center_x, top) = match near {
        Some(window) => (
            (window.left + window.right) / 2,
            window.top + (window.height() / 6).clamp(gap, 160 + gap),
        ),
        None => ((work.left + work.right) / 2, work.top + work.height() / 4),
    };
    let x = (center_x - width / 2).clamp(
        work.left + gap,
        (work.right - gap - width).max(work.left + gap),
    );
    let y = top.clamp(
        work.top + gap,
        (work.bottom - gap - height).max(work.top + gap),
    );
    (x, y)
}

/// Sent to the page each time the box opens.
#[derive(Debug, Clone, Serialize, Type)]
pub struct AskBoxInfo {
    /// The program answers would be typed into, if any.
    pub app: Option<String>,
    /// Its style, when "type into document" is on.
    pub style: Option<crate::app_styles::Active>,
    /// "Ask about my screen" has a window to look at.
    pub can_see_screen: bool,
}

#[derive(Default)]
struct Inner {
    /// The window and control in front when the box opened.
    target: Option<DocTarget>,
    /// That window, even with no caret in it (for the screenshot).
    near: Option<CapturedWindow>,
    open: bool,
    /// Questions out, by id, with a flag that cancels each.
    cancels: HashMap<u64, Arc<AtomicBool>>,
    /// The question this box is waiting on.
    busy: Option<u64>,
    info: Option<AskBoxInfo>,
    scale: f64,
}

#[derive(Default)]
pub struct AskBoxState {
    inner: Mutex<Inner>,
}

impl AskBoxState {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Cancels one typed question.
#[derive(Clone)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// A flag the question `id` checks, set by `cancel`.
pub fn cancel_flag(app: &AppHandle, id: u64) -> CancelFlag {
    let flag = Arc::new(AtomicBool::new(false));
    if let Some(state) = app.try_state::<AskBoxState>() {
        state.lock().cancels.insert(id, Arc::clone(&flag));
    }
    CancelFlag(flag)
}

pub fn cancel(app: &AppHandle, id: u64) {
    if let Some(state) = app.try_state::<AskBoxState>() {
        if let Some(flag) = state.lock().cancels.remove(&id) {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

fn forget(app: &AppHandle, id: u64) {
    if let Some(state) = app.try_state::<AskBoxState>() {
        let mut inner = state.lock();
        inner.cancels.remove(&id);
        if inner.busy == Some(id) {
            inner.busy = None;
        }
    }
}

/// A question ended with no reply or error to show (cancelled, or the
/// screenshot wasn't sent): the box stops waiting for it.
pub fn question_ended(app: &AppHandle, id: u64) {
    forget(app, id);
    let _ = app.emit("assistant-ended", id);
}

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

fn is_focused(app: &AppHandle) -> bool {
    window(app).is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
}

fn build(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(existing) = window(app) {
        return Some(existing);
    }
    let mut builder = tauri::WebviewWindowBuilder::new(
        app,
        LABEL,
        tauri::WebviewUrl::App("src/ask-box/index.html".into()),
    )
    .title("Ask Kusema")
    .inner_size(WIDTH, MIN_HEIGHT)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(true)
    .accept_first_mouse(true)
    .focused(false)
    .visible(false);
    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }
    match builder.build() {
        Ok(window) => Some(window),
        Err(e) => {
            warn!("Couldn't open the Ask box: {e}");
            None
        }
    }
}

/// Size and place the box for `near`, `height` logical pixels tall.
fn position(app: &AppHandle, window: &WebviewWindow, near: Option<ScreenRect>, height: f64) -> f64 {
    let found = near.and_then(crate::screen::work_area_near).or_else(|| {
        let monitor = app.primary_monitor().ok().flatten()?;
        let area = monitor.work_area();
        Some((
            ScreenRect {
                left: area.position.x,
                top: area.position.y,
                right: area.position.x + area.size.width as i32,
                bottom: area.position.y + area.size.height as i32,
            },
            monitor.scale_factor(),
        ))
    });
    let Some((work, scale)) = found else {
        return 1.0;
    };
    let px = |v: f64| (v * scale).round() as i32;
    let (width, height) = (px(WIDTH), px(height));
    let (x, y) = place(near, work, width, height, px(GAP));
    let _ = window.set_size(PhysicalSize::new(width as u32, height as u32));
    let _ = window.set_position(PhysicalPosition::new(x, y));
    scale
}

/// Remember the window in front, and what the box can do with it.
fn note_front(app: &AppHandle, inner: &mut Inner) {
    let near = crate::screen::foreground_target().ok();
    inner.near = near;
    inner.target = crate::assistant::insert::at_press();
    let settings = crate::settings::get_settings(app);
    let process = inner
        .target
        .map(|t| crate::app_styles::process_of(t.window.handle()))
        .filter(|p| !p.is_empty());
    let types = settings.assistant_answer_to != crate::assistant::AnswerTo::Speak;
    inner.info = Some(AskBoxInfo {
        style: process
            .as_deref()
            .filter(|_| types)
            .and_then(|p| crate::app_styles::active(&settings, p)),
        app: process,
        can_see_screen: near.is_some(),
    });
}

fn open(app: &AppHandle) {
    let Some(state) = app.try_state::<AskBoxState>() else {
        return;
    };
    let near = {
        let mut inner = state.lock();
        note_front(app, &mut inner);
        inner.open = true;
        inner.near.map(|w| w.rect)
    };
    let Some(window) = build(app) else {
        return;
    };
    let scale = position(app, &window, near, MIN_HEIGHT);
    let info = {
        let mut inner = state.lock();
        inner.scale = scale;
        inner.info.clone()
    };
    debug!(
        "Ask box opened (types into: {:?})",
        info.as_ref().and_then(|i| i.app.clone())
    );
    let _ = window.emit("ask-box-opened", info);
    // A new window shows itself once its page is ready (`ask_box_ready`).
    if window.is_visible().unwrap_or(false) || page_ready(&window) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn page_ready(window: &WebviewWindow) -> bool {
    window
        .app_handle()
        .try_state::<AskBoxReady>()
        .is_some_and(|r| r.0.load(Ordering::SeqCst))
}

/// Set once the box's page has rendered for the first time.
#[derive(Default)]
pub struct AskBoxReady(AtomicBool);

/// Close the box, cancelling a question it's waiting on. Focus goes back to
/// the window it was opened over if the box still had it; `true` if it did.
fn close(app: &AppHandle, cancel_question: bool) -> bool {
    let Some(state) = app.try_state::<AskBoxState>() else {
        return false;
    };
    let focused = is_focused(app);
    let (near, busy) = {
        let mut inner = state.lock();
        inner.open = false;
        (inner.near, inner.busy.take())
    };
    if cancel_question {
        if let Some(id) = busy {
            cancel(app, id);
            crate::tts::stop(app);
        }
    }
    let refocus = refocus_on_close(focused, near.is_some());
    if let Some(window) = window(app) {
        // While it's still in front, so Windows lets focus go back.
        if let (true, Some(near)) = (refocus, near) {
            crate::screen::activate(&near);
        }
        let _ = window.hide();
    }
    refocus
}

/// The answer is about to be typed: close the box, leaving the question to
/// finish. Focus goes back to the document only if the box still had it; if
/// the user has gone elsewhere, the answer waits for them to come back.
pub fn close_for_typing(app: &AppHandle) {
    let app_for_close = app.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = app.run_on_main_thread(move || {
        let _ = tx.send(close(&app_for_close, false));
    });
    let _ = rx.recv_timeout(Duration::from_secs(2));
}

/// The Cancel hotkey (Escape) with the box in front closes it.
pub fn cancel_open(app: &AppHandle) -> bool {
    if !is_focused(app) {
        return false;
    }
    close(app, true);
    true
}

/// The type-to-ask hotkey.
pub struct AskBoxAction;

impl ShortcutAction for AskBoxAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        let open = app
            .try_state::<AskBoxState>()
            .is_some_and(|s| s.lock().open)
            && window(app).is_some_and(|w| w.is_visible().unwrap_or(false));
        let action = on_hotkey(open, is_focused(app));
        info!("Ask box hotkey: {action:?}");
        let app = app.clone();
        // Window calls belong on the main thread.
        let _ = app.clone().run_on_main_thread(move || match action {
            HotkeyAction::Open | HotkeyAction::Focus => open_or_focus(&app, action),
            HotkeyAction::Close => {
                close(&app, true);
            }
        });
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

fn open_or_focus(app: &AppHandle, action: HotkeyAction) {
    if action == HotkeyAction::Focus {
        // Behind another app now: if that's a window to answer into, the box
        // moves to it; over Kusema's own windows it keeps the old one.
        let moved = crate::screen::foreground_target().is_ok();
        if !moved {
            if let Some(window) = window(app) {
                let _ = window.show();
                let _ = window.set_focus();
            }
            return;
        }
    }
    open(app);
}

/// The tray's "Type to Kusema…": the box, over whatever was in front.
pub fn open_from_tray(app: &AppHandle) {
    let app_for_open = app.clone();
    let _ = app.run_on_main_thread(move || open(&app_for_open));
}

/// The page rendered: show the box and give it the keyboard.
#[tauri::command]
#[specta::specta]
pub fn ask_box_ready(window: WebviewWindow, state: State<'_, AskBoxState>) -> Option<AskBoxInfo> {
    if let Some(ready) = window.app_handle().try_state::<AskBoxReady>() {
        ready.0.store(true, Ordering::SeqCst);
    }
    let (open, info) = {
        let inner = state.lock();
        (inner.open, inner.info.clone())
    };
    if open {
        let _ = window.show();
        let _ = window.set_focus();
    }
    info
}

/// Escape, or the close button.
#[tauri::command]
#[specta::specta]
pub fn ask_box_close(app: AppHandle) {
    close(&app, true);
}

/// The box lost the keyboard.
#[tauri::command]
#[specta::specta]
pub fn ask_box_blurred(app: AppHandle, empty: bool, has_answer: bool) {
    let busy = app
        .try_state::<AskBoxState>()
        .is_some_and(|s| s.lock().busy.is_some());
    if close_on_blur(empty, has_answer, busy) {
        debug!("Ask box closed: clicked away from an empty box");
        close(&app, false);
    }
}

/// The page's content is `height` logical pixels tall.
#[tauri::command]
#[specta::specta]
pub fn set_ask_box_height(app: AppHandle, state: State<'_, AskBoxState>, height: f64) {
    let Some(window) = window(&app) else {
        return;
    };
    let scale = state.lock().scale.max(1.0);
    let height = height.clamp(MIN_HEIGHT, MAX_HEIGHT);
    let width = (WIDTH * scale).round() as u32;
    let _ = window.set_size(PhysicalSize::new(width, (height * scale).round() as u32));
}

/// Ask what was typed, about the window in front too with `screen`. Returns
/// the question's id; the answer comes as `assistant-stream`, then
/// `assistant-reply` or `assistant-error`.
#[tauri::command]
#[specta::specta]
pub fn ask_box_submit(app: AppHandle, question: String, screen: bool) -> Result<u64, String> {
    let question = question.trim().to_string();
    if question.is_empty() {
        return Err("empty".to_string());
    }
    let state = app.try_state::<AskBoxState>().ok_or("no state")?;
    let id = crate::assistant::next_id(&app);
    let (target, near) = {
        let mut inner = state.lock();
        inner.busy = Some(id);
        (inner.target, inner.near)
    };
    if screen && near.is_none() {
        forget(&app, id);
        return Err("noWindow".to_string());
    }
    let options = AskOptions {
        source: Source::AskBox,
        target,
        id,
        in_dictation: false,
    };
    let cancelled = cancel_flag(&app, id);
    let app_for_task = app.clone();
    tauri::async_runtime::spawn(async move {
        let app = app_for_task;
        if let (true, Some(near)) = (screen, near) {
            let capture = step_aside_and_capture(&app, near).await;
            match capture {
                Ok(capture) => {
                    crate::assistant::ask_about_screen(
                        &app,
                        question,
                        Some(capture),
                        options,
                        || cancelled.is_set(),
                    )
                    .await
                }
                Err(e) => {
                    info!("Ask box: couldn't capture the window ({e:?})");
                    let _ = app.emit(
                        "assistant-error",
                        crate::assistant::AssistantError {
                            question,
                            message: e.message(),
                            id,
                            key: "askBox.noWindow".to_string(),
                            name: None,
                            setup: None,
                        },
                    );
                }
            }
        } else {
            crate::assistant::ask(&app, question, options, || cancelled.is_set()).await;
        }
        forget(&app, id);
    });
    Ok(id)
}

/// Hide the box, bring the window back to the front and capture it, then show
/// the box again (the confirmation card comes up beside the window).
async fn step_aside_and_capture(
    app: &AppHandle,
    near: CapturedWindow,
) -> Result<crate::screen::Capture, crate::screen::CaptureError> {
    if let Some(window) = window(app) {
        let _ = window.hide();
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::screen::activate(&near);
        std::thread::sleep(SETTLE_BEFORE_CAPTURE);
        crate::screen::capture_window(near)
    })
    .await
    .unwrap_or_else(|e| Err(crate::screen::CaptureError::Failed(e.to_string())));
    let still_open = app
        .try_state::<AskBoxState>()
        .is_some_and(|s| s.lock().open);
    if let (true, Some(window)) = (still_open, window(app)) {
        let _ = window.show();
    }
    result
}

/// Stop the question the box is waiting on.
#[tauri::command]
#[specta::specta]
pub fn ask_box_stop(app: AppHandle, id: u64) {
    cancel(&app, id);
    crate::tts::stop(&app);
    forget(&app, id);
}

/// "Insert into document": close the box, then type the answer where the
/// question was asked.
#[tauri::command]
#[specta::specta]
pub async fn ask_box_insert(app: AppHandle, answer: String) -> Result<(), String> {
    let app_for_close = app.clone();
    let _ = app.run_on_main_thread(move || {
        close(&app_for_close, false);
    });
    crate::assistant::insert_answer(&app, answer).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hotkey_opens_closes_or_brings_back() {
        assert_eq!(on_hotkey(false, false), HotkeyAction::Open);
        assert_eq!(on_hotkey(true, true), HotkeyAction::Close);
        assert_eq!(on_hotkey(true, false), HotkeyAction::Focus);
    }

    #[test]
    fn focus_goes_back_only_if_the_box_still_had_it() {
        assert!(refocus_on_close(true, true));
        // The user clicked into another app: leave them there.
        assert!(!refocus_on_close(false, true));
        // Opened over Kusema itself or the desktop: nothing to go back to.
        assert!(!refocus_on_close(true, false));
    }

    #[test]
    fn clicking_away_closes_only_an_empty_idle_box() {
        assert!(close_on_blur(true, false, false));
        assert!(
            !close_on_blur(false, false, false),
            "a half-typed question stays"
        );
        assert!(
            !close_on_blur(true, true, false),
            "an answer stays to be read"
        );
        assert!(
            !close_on_blur(true, false, true),
            "the screenshot card took focus"
        );
    }

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> ScreenRect {
        ScreenRect {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn the_box_sits_over_the_window_inside_the_screen() {
        let work = rect(0, 0, 1920, 1040);
        let (x, y) = place(Some(rect(400, 200, 1400, 900)), work, 520, 132, 12);
        assert_eq!(x, 900 - 260, "centred on the window");
        assert!(y > 200 && y < 400, "in its upper part");
        // A window off the right edge: the box stays on screen.
        let (x, _) = place(Some(rect(1700, 100, 2600, 800)), work, 520, 132, 12);
        assert_eq!(x, 1920 - 12 - 520);
        // No window: the upper third of the screen.
        let (x, y) = place(None, work, 520, 132, 12);
        assert_eq!((x, y), (960 - 260, 260));
        // A window at the bottom: the box is pulled up into the work area.
        let (_, y) = place(Some(rect(0, 1000, 800, 1040)), work, 520, 132, 12);
        assert_eq!(y, 1040 - 12 - 132);
    }
}
