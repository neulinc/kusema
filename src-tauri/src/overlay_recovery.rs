//! Rebuilds the recording overlay after its WebView2 loses a process.
//!
//! The overlay is a transparent webview. When WebView2's GPU process fails,
//! for example because Windows ran out of memory, it restarts the process, and
//! after three failures it falls back to drawing without the GPU. The overlay
//! window can then come back as a solid black rectangle around the pill until
//! the app restarts. Recreating the window gives it a fresh surface, so the
//! pill is see-through again.
//!
//! The rebuild waits until the overlay is hidden, so a recording in progress
//! keeps its pill, and runs at most once every few seconds, so a process that
//! keeps failing can't keep the window rebuilding.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
};
use webview2_com::ProcessFailedEventHandler;

const LABEL: &str = "recording_overlay";
const MIN_INTERVAL: Duration = Duration::from_secs(5);

/// A failure has happened and the overlay hasn't been rebuilt since.
static PENDING: AtomicBool = AtomicBool::new(false);
static LAST_REBUILD: Mutex<Option<Instant>> = Mutex::new(None);

/// Whether a failure of this kind can leave the overlay drawn wrongly. A
/// failed GPU process is the case seen in practice; a lost page or browser
/// process leaves the window empty, which a rebuild also fixes.
fn needs_rebuild(kind: COREWEBVIEW2_PROCESS_FAILED_KIND) -> bool {
    kind == COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED
        || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
        || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE
        || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED
}

/// Whether enough time has passed since the last rebuild to do another.
fn may_rebuild(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|last| now.duration_since(last) >= MIN_INTERVAL)
}

/// Starts watching the overlay's webview. Call after the overlay is created.
pub fn watch(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let app = app.clone();
    let result = window.with_webview(move |webview| unsafe {
        let app = app.clone();
        let handler = ProcessFailedEventHandler::create(Box::new(move |_, args| {
            let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
            if let Some(args) = args {
                args.ProcessFailedKind(&mut kind)?;
            }
            log::warn!(
                "The recording overlay's WebView2 lost a process (kind {})",
                kind.0
            );
            if needs_rebuild(kind) {
                PENDING.store(true, Ordering::SeqCst);
                let hidden = app
                    .get_webview_window(LABEL)
                    .and_then(|w| w.is_visible().ok())
                    == Some(false);
                if hidden {
                    let app = app.clone();
                    std::thread::spawn(move || rebuild_if_needed(&app));
                }
            }
            Ok(())
        }));
        let mut token = 0i64;
        if let Err(e) = webview
            .controller()
            .CoreWebView2()
            .and_then(|core| core.add_ProcessFailed(&handler, &mut token))
        {
            log::warn!("Couldn't watch the recording overlay's WebView2: {e}");
        }
    });
    if let Err(e) = result {
        log::warn!("Couldn't reach the recording overlay's WebView2: {e}");
    }
}

/// Rebuilds the overlay if a process failed since the last rebuild. Called
/// when the overlay has just been hidden, and right after a failure while it's
/// hidden. Not on the UI thread: it waits for the old window to go.
pub fn rebuild_if_needed(app: &AppHandle) {
    if !PENDING.load(Ordering::SeqCst) {
        return;
    }
    {
        let mut last = LAST_REBUILD.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if !may_rebuild(*last, now) {
            return;
        }
        *last = Some(now);
    }
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        // Shown again since: the next hide does it.
        *LAST_REBUILD.lock().unwrap_or_else(|e| e.into_inner()) = None;
        return;
    }
    PENDING.store(false, Ordering::SeqCst);
    log::info!("Rebuilding the recording overlay after a WebView2 process failure");
    if let Err(e) = window.destroy() {
        log::warn!("Couldn't close the recording overlay to rebuild it: {e}");
        return;
    }
    for _ in 0..60 {
        if app.get_webview_window(LABEL).is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    crate::overlay::create_recording_overlay(app);
    watch(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_UTILITY_PROCESS_EXITED,
    };

    #[test]
    fn a_failed_gpu_or_page_process_rebuilds_the_overlay() {
        assert!(needs_rebuild(
            COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED
        ));
        assert!(needs_rebuild(
            COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
        ));
        assert!(needs_rebuild(
            COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE
        ));
        assert!(needs_rebuild(
            COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED
        ));
        // These don't draw the overlay.
        assert!(!needs_rebuild(
            COREWEBVIEW2_PROCESS_FAILED_KIND_UTILITY_PROCESS_EXITED
        ));
        assert!(!needs_rebuild(
            COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED
        ));
    }

    #[test]
    fn rebuilds_are_spaced_out() {
        let now = Instant::now();
        assert!(may_rebuild(None, now));
        assert!(!may_rebuild(Some(now), now + Duration::from_secs(1)));
        assert!(may_rebuild(Some(now), now + MIN_INTERVAL));
    }
}
