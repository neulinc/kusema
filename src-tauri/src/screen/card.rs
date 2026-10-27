//! The small card shown beside the captured window: "send this screenshot?"
//! with a thumbnail, the question and where it would go, or a short notice
//! when something went wrong.
//!
//! Each card is a fresh webview window (labels `screen_card_<n>`). The page
//! fetches its content with `get_screen_card` and calls `screen_card_ready`,
//! which shows it and, for a confirmation, gives it keyboard focus so Enter
//! sends and Escape cancels. Focus goes back to the captured window after.

use super::{restore_focus, CapturedWindow, ScreenRect, ScreenState};
use crate::actions::complete_unless_cancelled;
use crate::llm_client::ChatImage;
use log::{debug, warn};
use serde::Serialize;
use specta::Type;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow};
use tokio::sync::oneshot;

const LABEL_PREFIX: &str = "screen_card_";
/// A confirmation left unanswered this long is cancelled.
pub const CONFIRM_TIMEOUT: Duration = Duration::from_secs(30);
/// A notice closes itself after this long.
const NOTICE_DURATION: Duration = Duration::from_secs(8);
/// Card sizes in logical pixels.
const CONFIRM_SIZE: (f64, f64) = (380.0, 460.0);
const NOTICE_SIZE: (f64, f64) = (380.0, 150.0);
/// Space between the card and the window or screen edge, logical pixels.
const GAP: f64 = 12.0;

/// What the card shows.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ScreenCard {
    #[serde(rename_all = "camelCase")]
    Confirm {
        /// The exact image that would be sent, as a data URL.
        image: String,
        question: String,
        provider: String,
        model: String,
        timeout_secs: u32,
    },
    Notice {
        message: String,
    },
}

/// The user's answer to a confirmation card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    Send,
    Cancel,
    TimedOut,
    /// Cancelled from elsewhere (the Cancel hotkey).
    Interrupted,
}

/// Physical position and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Placement {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Default)]
struct Inner {
    card: Option<ScreenCard>,
    reply: Option<oneshot::Sender<bool>>,
    captured: Option<CapturedWindow>,
    label: Option<String>,
    focus: bool,
    small: Option<Placement>,
    large: Option<Placement>,
    counter: u64,
}

#[derive(Default)]
pub struct CardSlot {
    inner: Mutex<Inner>,
}

impl CardSlot {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Where a `width`×`height` card goes: beside the window if its monitor has
/// room, else above or below it, else inside its bottom-right corner, so it
/// never covers the middle of what the user asked about.
pub(crate) fn place_card(
    window: ScreenRect,
    work: ScreenRect,
    width: i32,
    height: i32,
    gap: i32,
) -> (i32, i32) {
    let clamp_x = |x: i32| x.clamp(work.left, (work.right - width).max(work.left));
    let clamp_y = |y: i32| y.clamp(work.top, (work.bottom - height).max(work.top));
    let beside_y = clamp_y(window.bottom - height);
    let beside_x = clamp_x(window.right - width);

    let right = window.right + gap;
    if right + width <= work.right {
        return (right, beside_y);
    }
    let left = window.left - gap - width;
    if left >= work.left {
        return (left, beside_y);
    }
    let below = window.bottom + gap;
    if below + height <= work.bottom {
        return (beside_x, below);
    }
    let above = window.top - gap - height;
    if above >= work.top {
        return (beside_x, above);
    }
    (
        clamp_x(window.right.min(work.right) - gap - width),
        clamp_y(window.bottom.min(work.bottom) - gap - height),
    )
}

/// The work area and scale to place a card on: the captured window's
/// monitor, or the primary monitor.
fn work_area(app: &AppHandle, near: Option<ScreenRect>) -> Option<(ScreenRect, f64)> {
    if let Some(found) = near.and_then(super::capture::monitor_work_area) {
        return Some(found);
    }
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
}

fn placements(
    app: &AppHandle,
    near: Option<ScreenRect>,
    size: (f64, f64),
) -> Option<(Placement, Placement)> {
    let (work, scale) = work_area(app, near)?;
    let px = |v: f64| (v * scale).round() as i32;
    let (width, height) = (px(size.0), px(size.1));
    let gap = px(GAP);
    let (x, y) = match near {
        Some(window) => place_card(window, work, width, height, gap),
        None => (work.right - gap - width, work.bottom - gap - height),
    };
    let small = Placement {
        x,
        y,
        width: width as u32,
        height: height as u32,
    };
    // Enlarged: most of the monitor, centred.
    let large = Placement {
        x: work.left + work.width() / 14,
        y: work.top + work.height() / 14,
        width: (work.width() * 12 / 14) as u32,
        height: (work.height() * 12 / 14) as u32,
    };
    Some((small, large))
}

fn apply(window: &WebviewWindow, placement: Placement) {
    let _ = window.set_size(PhysicalSize::new(placement.width, placement.height));
    let _ = window.set_position(PhysicalPosition::new(placement.x, placement.y));
}

fn close_windows(app: &AppHandle) {
    for (label, window) in app.webview_windows() {
        if label.starts_with(LABEL_PREFIX) {
            let _ = window.destroy();
        }
    }
}

/// Replace any card with a new one.
fn open(
    app: &AppHandle,
    card: ScreenCard,
    captured: Option<CapturedWindow>,
    reply: Option<oneshot::Sender<bool>>,
) -> Option<u64> {
    let state = app.try_state::<ScreenState>()?;
    let focus = reply.is_some();
    let size = if focus { CONFIRM_SIZE } else { NOTICE_SIZE };
    let near = captured.map(|w| w.rect);
    let (small, large) = placements(app, near, size)?;

    let (label, counter) = {
        let mut inner = state.card.lock();
        inner.counter += 1;
        let label = format!("{LABEL_PREFIX}{}", inner.counter);
        // Answering the replaced card's question as "cancel".
        if let Some(previous) = inner.reply.take() {
            let _ = previous.send(false);
        }
        *inner = Inner {
            card: Some(card),
            reply,
            captured,
            label: Some(label.clone()),
            focus,
            small: Some(small),
            large: Some(large),
            counter: inner.counter,
        };
        (label, inner.counter)
    };
    close_windows(app);

    let mut builder = tauri::WebviewWindowBuilder::new(
        app,
        &label,
        tauri::WebviewUrl::App("src/screen-card/index.html".into()),
    )
    .title("Kusema")
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(true)
    .accept_first_mouse(true)
    .focusable(focus)
    .focused(false)
    .visible(false);
    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }
    match builder.build() {
        Ok(window) => apply(&window, small),
        Err(e) => {
            warn!("Couldn't open the screen card: {e}");
            return None;
        }
    }
    Some(counter)
}

/// Close the card. With `refocus`, focus goes back to the captured window if
/// the card still has it (the user hasn't clicked elsewhere meanwhile).
fn close(app: &AppHandle, refocus: bool) {
    let Some(state) = app.try_state::<ScreenState>() else {
        return;
    };
    let (label, captured) = {
        let mut inner = state.card.lock();
        inner.card = None;
        inner.reply = None;
        (inner.label.take(), inner.captured.take())
    };
    if refocus {
        let card_focused = label
            .and_then(|l| app.get_webview_window(&l))
            .is_some_and(|w| w.is_focused().unwrap_or(false));
        if let (true, Some(window)) = (card_focused, captured) {
            restore_focus(&window);
        }
    }
    close_windows(app);
}

/// Show the screenshot about to be sent and wait for Send, Cancel, the
/// timeout or the Cancel hotkey.
pub async fn confirm<C: Fn() -> bool>(
    app: &AppHandle,
    captured: CapturedWindow,
    image: &ChatImage,
    question: &str,
    provider: &str,
    model: &str,
    is_cancelled: C,
) -> Confirmation {
    let (tx, rx) = oneshot::channel();
    let card = ScreenCard::Confirm {
        image: image.data_url(),
        question: question.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        timeout_secs: CONFIRM_TIMEOUT.as_secs() as u32,
    };
    if open(app, card, Some(captured), Some(tx)).is_none() {
        // Without a card there's no way to agree; nothing is sent.
        return Confirmation::Cancel;
    }
    crate::tts::say(app, &format!("Send this screenshot to {provider}?"));

    let answer =
        complete_unless_cancelled(tokio::time::timeout(CONFIRM_TIMEOUT, rx), is_cancelled).await;
    crate::tts::stop(app);
    let confirmation = match answer {
        None => Confirmation::Interrupted,
        Some(Err(_)) => Confirmation::TimedOut,
        Some(Ok(Ok(true))) => Confirmation::Send,
        Some(Ok(_)) => Confirmation::Cancel,
    };
    if matches!(
        confirmation,
        Confirmation::Interrupted | Confirmation::TimedOut
    ) {
        close(app, true);
    }
    debug!("Screenshot confirmation: {confirmation:?}");
    confirmation
}

/// Show `message` on a card (beside `near`, else at the corner of the
/// screen) and say it, if replies are spoken. It closes itself.
pub fn notice(app: &AppHandle, message: String, near: Option<CapturedWindow>) {
    crate::tts::say(app, &message);
    let Some(counter) = open(app, ScreenCard::Notice { message }, near, None) else {
        return;
    };
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(NOTICE_DURATION);
        let current = app
            .try_state::<ScreenState>()
            .is_some_and(|s| s.card.lock().counter == counter);
        if current {
            close(&app, false);
        }
    });
}

pub fn show_notice(app: &AppHandle, message: String) {
    notice(app, message, None);
}

/// The card's content, for its page.
#[tauri::command]
#[specta::specta]
pub fn get_screen_card(state: State<'_, ScreenState>) -> Option<ScreenCard> {
    state.card.lock().card.clone()
}

/// The card's page has rendered: show it, focused if it's asking something.
#[tauri::command]
#[specta::specta]
pub fn screen_card_ready(window: WebviewWindow, state: State<'_, ScreenState>) {
    let focus = {
        let inner = state.card.lock();
        if inner.label.as_deref() != Some(window.label()) {
            return; // replaced meanwhile
        }
        inner.focus
    };
    let _ = window.show();
    if focus {
        let _ = window.set_focus();
    }
}

/// Send (`true`) or cancel (`false`) from the confirmation card.
#[tauri::command]
#[specta::specta]
pub fn answer_screen_card(app: AppHandle, state: State<'_, ScreenState>, send: bool) {
    let (reply, captured) = {
        let mut inner = state.card.lock();
        (inner.reply.take(), inner.captured)
    };
    // While the card is still in front, so Windows lets us hand focus back.
    if let Some(window) = captured {
        restore_focus(&window);
    }
    close(&app, false);
    if let Some(reply) = reply {
        let _ = reply.send(send);
    }
}

/// Close a notice card.
#[tauri::command]
#[specta::specta]
pub fn dismiss_screen_card(app: AppHandle) {
    close(&app, false);
}

/// Show the screenshot large (or back to the small card).
#[tauri::command]
#[specta::specta]
pub fn set_screen_card_enlarged(
    window: WebviewWindow,
    state: State<'_, ScreenState>,
    enlarged: bool,
) {
    let placement = {
        let inner = state.card.lock();
        if inner.label.as_deref() != Some(window.label()) {
            return;
        }
        if enlarged {
            inner.large
        } else {
            inner.small
        }
    };
    if let Some(placement) = placement {
        apply(&window, placement);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: ScreenRect = ScreenRect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> ScreenRect {
        ScreenRect {
            left,
            top,
            right,
            bottom,
        }
    }

    fn covers_center(window: ScreenRect, (x, y): (i32, i32), w: i32, h: i32) -> bool {
        let (cx, cy) = (
            (window.left + window.right) / 2,
            (window.top + window.bottom) / 2,
        );
        (x..x + w).contains(&cx) && (y..y + h).contains(&cy)
    }

    #[test]
    fn goes_beside_the_window_when_there_is_room() {
        let window = rect(100, 100, 1000, 800);
        assert_eq!(place_card(window, WORK, 380, 460, 12), (1012, 340));
        let window = rect(1000, 100, 1900, 800);
        assert_eq!(place_card(window, WORK, 380, 460, 12), (608, 340));
    }

    #[test]
    fn goes_below_a_wide_window() {
        let window = rect(0, 0, 1920, 500);
        assert_eq!(place_card(window, WORK, 380, 460, 12), (1540, 512));
    }

    #[test]
    fn sits_in_the_corner_of_a_maximized_window() {
        let window = rect(0, 0, 1920, 1040);
        let position = place_card(window, WORK, 380, 460, 12);
        assert_eq!(position, (1528, 568));
        assert!(!covers_center(window, position, 380, 460));
    }

    #[test]
    fn stays_on_the_window_monitor() {
        // A window on a second monitor to the left (negative coordinates).
        let work = rect(-1920, 0, 0, 1040);
        let window = rect(-1900, 10, -10, 1030);
        let (x, y) = place_card(window, work, 380, 460, 12);
        assert!(x >= -1920 && x + 380 <= 0);
        assert!(y >= 0 && y + 460 <= 1040);
        assert!(!covers_center(window, (x, y), 380, 460));
    }
}
