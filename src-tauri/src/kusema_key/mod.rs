//! The Kusema key: one key for everything. Hold it to dictate; double-tap
//! for continuous dictation. Start what you say with the trigger word
//! ("Kusema, …") to ask, edit or look at your screen instead (see `route`).
//!
//! The key is read by Kusema's own keyboard hook (`hook`), which swallows it
//! so the app in front never sees it, and turned into dictation by `gesture`
//! through the same coordinator as the hotkeys. The classic per-action
//! shortcuts are still there, off unless the user wants them.

pub mod gesture;
mod hook;
pub mod keys;
pub mod migrate;
pub mod route;
pub mod trigger;

use log::{debug, info, warn};
use serde::Serialize;
use specta::Type;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

use crate::assistant::AskMode;
use crate::screen::{Capture, CapturedWindow};
use crate::settings::{get_settings, write_settings, AppSettings, ShortcutActivation};
use crate::transcription_coordinator::TranscriptionCoordinator;
pub use keys::KusemaKey;
use route::Route;

/// The per-action shortcuts the Kusema key replaces. The Ask box, Cancel and
/// Privileged mode keep their own keys.
pub const CLASSIC: &[&str] = &[
    "transcribe",
    "transcribe_with_post_process",
    "assistant",
    "assistant_screen",
    crate::dictation::BINDING,
    crate::voice_edit::BINDING,
];

/// What the coordinator is told presses came from.
const SOURCE: &str = "kusema_key";

/// A classic shortcut that isn't registered, because they're off.
pub fn skip_binding(settings: &AppSettings, id: &str) -> bool {
    !settings.classic_shortcuts && CLASSIC.contains(&id)
}

/// The window in front when a dictation key went down, for "Kusema, look at
/// my screen".
static PRESS_WINDOW: Mutex<Option<CapturedWindow>> = Mutex::new(None);

/// At any dictation press: note where the user is, cheaply, in case what's
/// said starts with the trigger word.
pub fn note_press(app: &AppHandle) {
    *PRESS_WINDOW.lock().unwrap_or_else(|e| e.into_inner()) =
        crate::screen::foreground_target().ok();
    crate::voice_edit::note_press_quietly();
    crate::assistant::note_target_at_press(app);
}

/// How Parakeet also hears "Kusema" said at the start of a sentence (measured
/// with synthesized speech). Only for spotting the trigger word: in the
/// vocabulary they'd rewrite a real name, Kuzma, everywhere.
const KUSEMA_TRIGGER_FORMS: &[&str] = &["Kuzma", "Kyzma", "Kusma", "Kozma", "Kizma", "Cuzma"];

/// The trigger word and the ways it's heard.
pub fn trigger_forms(settings: &AppSettings) -> Vec<String> {
    let trigger = settings.voice_edit_trigger.trim();
    if trigger.is_empty() {
        return Vec::new();
    }
    let mut forms = crate::vocabulary::spoken_forms_of(settings, trigger);
    if trigger.eq_ignore_ascii_case(crate::voice_edit::commands::DEFAULT_TRIGGER) {
        forms.extend(KUSEMA_TRIGGER_FORMS.iter().map(|f| f.to_string()));
    }
    // Spellings the user added from "correct that".
    forms.extend(settings.trigger_variants.iter().cloned());
    forms
}

/// The first few words, for the debug log (never the whole utterance).
fn first_words_for_log(text: &str) -> String {
    text.split_whitespace()
        .take(3)
        .collect::<Vec<_>>()
        .join(" ")
}

/// What follows the trigger word at the start of `text`, if it's there
/// (strictly or as it sounds), logged at debug level.
pub fn trigger_rest(settings: &AppSettings, text: &str) -> Option<String> {
    let found = trigger::find(text, &trigger_forms(settings));
    debug!(
        "Trigger check: \"{}\" -> {}",
        first_words_for_log(text),
        describe(&found)
    );
    match found {
        trigger::TriggerMatch::Found { rest, .. } => Some(rest),
        _ => None,
    }
}

fn describe(found: &trigger::TriggerMatch) -> String {
    match found {
        trigger::TriggerMatch::Found { exact, heard, .. } => {
            format!(
                "trigger \"{heard}\" ({})",
                if *exact { "exact" } else { "sounds like" }
            )
        }
        trigger::TriggerMatch::Near { heard } => format!("near miss \"{heard}\", typed"),
        trigger::TriggerMatch::None => "no trigger, typed".to_string(),
    }
}

/// Space was tapped during the take being held: it's for the Assistant.
static ASSIST_HELD: AtomicBool = AtomicBool::new(false);
/// A finished take that's for the Assistant, until it's routed.
static ASSIST_PENDING: AtomicBool = AtomicBool::new(false);
/// The last take was typed but began close to the trigger word.
static NEAR_MISS: AtomicBool = AtomicBool::new(false);

/// Shows in the pill which way the speech went.
fn announce(app: &AppHandle, route: &Route) {
    info!("Trigger word: {}", route.kind());
    if let Some(window) = app.get_webview_window("recording_overlay") {
        let _ = window.emit("overlay-route", route.kind());
    }
}

/// What a held take becomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// For the Assistant: route this (what followed the trigger word, or all
    /// of it after a Space tap).
    Route(String),
    /// Dictation. `hint`: it began close to the trigger word, so the pill
    /// says how to reach Kusema for sure.
    Type { hint: bool },
}

pub fn decide(found: trigger::TriggerMatch, forced: bool, text: &str) -> Decision {
    let rest = match found {
        trigger::TriggerMatch::Found { rest, .. } => rest,
        // Switched with Space: no trigger word needed.
        _ if forced => text.trim().to_string(),
        trigger::TriggerMatch::Near { .. } => return Decision::Type { hint: true },
        trigger::TriggerMatch::None => return Decision::Type { hint: false },
    };
    if rest.chars().any(char::is_alphanumeric) {
        Decision::Route(rest)
    } else {
        Decision::Type { hint: false }
    }
}

/// Speech from normal dictation, after it started with the trigger word.
pub enum Routed {
    /// Handle it as this Assistant mode, with this text.
    As(AskMode, String, Option<Capture>),
    /// Already dealt with (and said why).
    Done,
}

/// Captures the window noted at the press, bringing it back first if needed.
fn capture_press_window() -> Result<Capture, crate::screen::CaptureError> {
    let window = PRESS_WINDOW
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .ok_or(crate::screen::CaptureError::NoWindow)?;
    let in_front = crate::screen::foreground_target().is_ok_and(|w| w.handle() == window.handle());
    if !in_front {
        crate::screen::activate(&window);
        std::thread::sleep(Duration::from_millis(250));
    }
    crate::screen::capture_window(window)
}

/// Normal dictation: where `text` goes if it starts with the trigger word.
pub fn route_dictation(app: &AppHandle, text: &str) -> Option<Routed> {
    let forced = ASSIST_PENDING.swap(false, Ordering::SeqCst);
    NEAR_MISS.store(false, Ordering::SeqCst);
    let settings = get_settings(app);
    let found = trigger::find(text, &trigger_forms(&settings));
    debug!(
        "Trigger check: \"{}\" -> {}{}",
        first_words_for_log(text),
        describe(&found),
        if forced {
            " (Space was tapped: for the Assistant)"
        } else {
            ""
        }
    );
    let rest = match decide(found, forced, text) {
        Decision::Route(rest) => rest,
        Decision::Type { hint } => {
            NEAR_MISS.store(hint, Ordering::SeqCst);
            return None;
        }
    };
    let (here, selection) = crate::voice_edit::text_here();
    let routed = route::route(&rest, here);
    debug!("Routed to {} ({here:?})", routed.kind());
    announce(app, &routed);
    Some(match routed {
        Route::Command(_) | Route::Edit(_) if !crate::voice_edit::has_pending() => {
            crate::voice_edit::tell_no_window(app);
            Routed::Done
        }
        Route::Command(_) => Routed::As(AskMode::Edit, rest, None),
        Route::Edit(instruction) => Routed::As(AskMode::Edit, instruction, None),
        Route::Screen(question) => match capture_press_window() {
            Ok(capture) => Routed::As(AskMode::Screen, question, Some(capture)),
            Err(e) => {
                info!("Not capturing for a screen question: {e:?}");
                crate::screen::card::show_notice(app, e.message());
                Routed::Done
            }
        },
        Route::Ask {
            question,
            about_selection,
        } => {
            let question = match (about_selection, selection) {
                (true, Some(selection)) => route::question_about(&question, &selection),
                _ => question,
            };
            Routed::As(AskMode::Plain, question, None)
        }
    })
}

/// Continuous dictation: `rest` followed the trigger word in a segment
/// typed into `window`. Blocks until it's done (the edit's preview closed, or
/// the answer has been spoken), so the session types nothing meanwhile.
pub fn run_in_dictation(app: &AppHandle, rest: &str, window: isize) {
    let (here, selection) = crate::voice_edit::text_here_in_window(window);
    let routed = route::route(rest, here);
    announce(app, &routed);
    let notice = |key: &str, english: &str| {
        let notice = serde_json::json!({
            "key": format!("overlay.route.{key}"),
            "name": null,
            "message": english,
            "in_dictation": true,
        });
        crate::overlay::show_edit_notice(app, &notice, true, 5);
    };
    let options = || crate::assistant::AskOptions {
        source: crate::assistant::Source::Spoken,
        target: crate::assistant::insert::at_press(),
        id: crate::assistant::next_id(app),
    };
    match routed {
        Route::Command(_) => crate::voice_edit::run_trigger_in_dictation(app, rest, window),
        Route::Edit(instruction) => {
            crate::voice_edit::run_trigger_in_dictation(app, &instruction, window)
        }
        Route::Screen(question) => {
            notice("ask_screen", "Looking at your screen…");
            let capture = crate::screen::foreground_target()
                .ok()
                .filter(|w| w.handle() == window)
                .ok_or(crate::screen::CaptureError::NoWindow)
                .and_then(crate::screen::capture_window);
            match capture {
                Ok(capture) => tauri::async_runtime::block_on(crate::assistant::ask_about_screen(
                    app,
                    question,
                    Some(capture),
                    options(),
                    || false,
                )),
                Err(e) => crate::screen::card::show_notice(app, e.message()),
            }
        }
        Route::Ask {
            question,
            about_selection,
        } => {
            notice("ask", "Asking Kusema…");
            let question = match (about_selection, selection) {
                (true, Some(selection)) => route::question_about(&question, &selection),
                _ => question,
            };
            tauri::async_runtime::block_on(crate::assistant::ask(app, question, options(), || {
                false
            }));
        }
    }
    // A spoken answer would be heard by the microphone: wait it out.
    let deadline = Instant::now() + Duration::from_secs(180);
    while crate::tts::is_speaking(app) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
}

/* ─────────────────────────── the key itself ─────────────────────────── */

/// Hands the hook its key from the settings.
pub fn apply_settings(settings: &AppSettings) {
    #[cfg(target_os = "windows")]
    hook::windows::with_state(|state| {
        state.key = settings
            .kusema_key_enabled
            .then(|| settings.kusema_key.clone());
        state.accept_injected = cfg!(debug_assertions)
            && std::env::var_os("KUSEMA_DEBUG_KEY_ACCEPT_INJECTED").is_some();
    });
    #[cfg(not(target_os = "windows"))]
    let _ = settings;
}

/// The binding a hold dictates with: AI cleanup when it's on.
fn hold_binding(settings: &AppSettings) -> &'static str {
    if settings.post_process_enabled {
        "transcribe_with_post_process"
    } else {
        "transcribe"
    }
}

/// Does what a gesture asks.
fn run(app: &AppHandle, effect: gesture::Effect, binding: &mut &'static str) {
    let Some(coordinator) = app.try_state::<TranscriptionCoordinator>() else {
        warn!("Kusema key: no coordinator");
        return;
    };
    debug!("Kusema key: {effect:?}");
    match effect {
        gesture::Effect::StartHold => {
            crate::tts::stop(app);
            *binding = hold_binding(&get_settings(app));
            coordinator.send_input(
                binding,
                SOURCE,
                true,
                ShortcutActivation::PushToTalk,
                Duration::ZERO,
            );
        }
        gesture::Effect::FinishHold { assistant } => {
            ASSIST_HELD.store(false, Ordering::SeqCst);
            ASSIST_PENDING.store(assistant, Ordering::SeqCst);
            coordinator.send_input(
                binding,
                SOURCE,
                false,
                ShortcutActivation::PushToTalk,
                Duration::ZERO,
            )
        }
        gesture::Effect::SwitchToAssistant => {
            info!("Kusema key: Space tapped, this take is for the Assistant");
            ASSIST_HELD.store(true, Ordering::SeqCst);
            // The pill and the glow take the Assistant's look.
            crate::overlay::set_mode(app, "ask");
            if let Some(window) = app.get_webview_window("recording_overlay") {
                let _ = window.emit("overlay-assistant-take", ());
            }
            crate::glow::recording_started(app, "assistant");
            crate::tts::prepare(app);
        }
        gesture::Effect::DiscardHold => {
            discard(app);
            // The start may still have been on its way: check once more.
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(250));
                let holding = GESTURE
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .is_holding();
                if !holding
                    && !crate::dictation::is_active(&app)
                    && app
                        .state::<std::sync::Arc<crate::managers::audio::AudioRecordingManager>>()
                        .is_recording()
                {
                    discard(&app);
                }
            });
        }
        gesture::Effect::ToggleContinuous => {
            crate::tts::stop(app);
            coordinator.send_input(
                crate::dictation::BINDING,
                SOURCE,
                true,
                ShortcutActivation::Toggle,
                Duration::ZERO,
            );
        }
    }
}

/// A tap's recording goes, unheard.
fn discard(app: &AppHandle) {
    if crate::dictation::is_active(app) {
        return;
    }
    crate::utils::cancel_current_operation(app);
}

static GESTURE: Mutex<gesture::Gesture> = Mutex::new(gesture::Gesture::new());

/// Starts the hook and the thread that turns its edges into dictation.
pub fn init(app: &AppHandle) {
    apply_settings(&get_settings(app));
    #[cfg(target_os = "windows")]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        hook::windows::start(tx);
        let app = app.clone();
        std::thread::Builder::new()
            .name("kusema-key-gesture".into())
            .spawn(move || {
                let mut binding: &'static str = "transcribe";
                loop {
                    let deadline = GESTURE.lock().unwrap_or_else(|e| e.into_inner()).deadline();
                    let event = match deadline {
                        Some(at) => {
                            match rx.recv_timeout(at.saturating_duration_since(Instant::now())) {
                                Ok(event) => Some(event),
                                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
                                Err(_) => break,
                            }
                        }
                        None => match rx.recv() {
                            Ok(event) => Some(event),
                            Err(_) => break,
                        },
                    };
                    let effect = match event {
                        None => {
                            GESTURE
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .tick(Instant::now());
                            None
                        }
                        Some(hook::windows::Event::Edge(edge, at)) => {
                            let continuous = crate::dictation::is_active(&app);
                            let mut gesture = GESTURE.lock().unwrap_or_else(|e| e.into_inner());
                            match edge {
                                hook::Edge::Down => gesture.down(at, continuous),
                                hook::Edge::Up => gesture.up(at),
                                hook::Edge::Space => gesture.space(),
                            }
                        }
                        Some(hook::windows::Event::Captured(captured)) => {
                            captured_key(&app, captured);
                            None
                        }
                    };
                    if let Some(effect) = effect {
                        run(&app, effect, &mut binding);
                    }
                }
            })
            .ok();
    }
}

/// The Kusema key is down right now.
pub fn is_held() -> bool {
    #[cfg(target_os = "windows")]
    return hook::windows::with_state(|s| s.key_held());
    #[cfg(not(target_os = "windows"))]
    false
}

/// A classic shortcut fired while the Kusema key is held (its hook was
/// ahead of Kusema's): the Kusema key's gesture wins. A Space in it is the
/// Space tap; anything else is ignored.
pub fn classic_while_held(binding_id: &str, hotkey: &str, pressed: bool) -> bool {
    if !CLASSIC.contains(&binding_id) || !is_held() {
        return false;
    }
    debug!("Kusema key held: classic '{binding_id}' ({hotkey}) gives way");
    #[cfg(target_os = "windows")]
    {
        if pressed && hotkey.to_lowercase().contains("space") {
            hook::windows::inject(hook::windows::Event::Edge(
                hook::Edge::Space,
                Instant::now(),
            ));
        }
        // Its hook was ahead of Kusema's: put Kusema's first again.
        hook::windows::reinstall_soon();
    }
    let _ = pressed;
    true
}

/// The English name of the key, for messages that are spoken and logged.
fn english_label(key: &KusemaKey) -> String {
    match key {
        KusemaKey::RightCtrl => "Right Ctrl".to_string(),
        KusemaKey::Copilot => "the Copilot key".to_string(),
        KusemaKey::NumpadPlus => "Numpad +".to_string(),
        KusemaKey::Custom { name, .. } => name.clone(),
    }
}

/// After a held take was typed: if it began close to the trigger word, say
/// how to reach Kusema for sure.
pub fn hint_after_typed(app: &AppHandle) {
    if !NEAR_MISS.swap(false, Ordering::SeqCst) {
        return;
    }
    let settings = get_settings(app);
    if !settings.kusema_key_enabled {
        return;
    }
    let name = english_label(&settings.kusema_key);
    let notice = serde_json::json!({
        "key": "overlay.triggerHint",
        "name": name,
        "message": format!("Typed as dictation. To ask Kusema, tap Space while holding {name}."),
        "in_dictation": false,
    });
    crate::overlay::show_edit_notice(app, &notice, false, 6);
}

/// "Correct that" changed a misheard first word to the trigger word: offer
/// to add that spelling, so speech starting with it goes to Kusema. Blocks
/// until the card is answered.
pub fn offer_variant(app: &AppHandle, heard: &str, near: CapturedWindow) {
    let settings = get_settings(app);
    let card = crate::screen::card::ScreenCard::Offer {
        word: heard.to_string(),
        trigger: route_trigger_word(&settings),
        timeout_secs: crate::screen::card::EDIT_TIMEOUT.as_secs() as u32,
    };
    let answer =
        tauri::async_runtime::block_on(crate::screen::card::confirm_edit(app, near, card, || {
            false
        }));
    if answer == crate::screen::Confirmation::Send {
        add_variant(app, heard);
    }
}

fn route_trigger_word(settings: &AppSettings) -> String {
    let trigger = settings.voice_edit_trigger.trim();
    if trigger.is_empty() {
        crate::voice_edit::commands::DEFAULT_TRIGGER.to_string()
    } else {
        trigger.to_string()
    }
}

fn add_variant(app: &AppHandle, heard: &str) {
    let heard = heard.trim().to_string();
    if heard.is_empty() {
        return;
    }
    let mut settings = get_settings(app);
    if !settings
        .trigger_variants
        .iter()
        .any(|v| v.eq_ignore_ascii_case(&heard))
    {
        info!("Trigger word: added a spelling");
        settings.trigger_variants.push(heard);
        changed(app, settings, "trigger_variants");
    }
}

/// Removes a spelling added from "correct that".
#[tauri::command]
#[specta::specta]
pub fn remove_trigger_variant(app: AppHandle, variant: String) {
    let mut settings = get_settings(&app);
    settings
        .trigger_variants
        .retain(|v| !v.eq_ignore_ascii_case(variant.trim()));
    changed(&app, settings, "trigger_variants");
}

/* ─────────────────────────── choosing a key ─────────────────────────── */

/// Sent as `kusema-key-captured` when a key was pressed to be the Kusema key.
#[derive(Debug, Clone, Serialize, Type)]
pub struct CapturedKey {
    /// The key to save, unless it was refused (or Escape, which cancels).
    pub key: Option<KusemaKey>,
    pub refusal: Option<keys::KeyRefusal>,
    pub cancelled: bool,
}

#[cfg(target_os = "windows")]
fn captured_key(app: &AppHandle, captured: hook::Captured) {
    let result = if captured.vk == keys::vk::ESCAPE {
        CapturedKey {
            key: None,
            refusal: None,
            cancelled: true,
        }
    } else if captured.copilot {
        CapturedKey {
            key: Some(KusemaKey::Copilot),
            refusal: None,
            cancelled: false,
        }
    } else {
        let key = match captured.vk {
            keys::vk::RCONTROL => KusemaKey::RightCtrl,
            keys::vk::ADD => KusemaKey::NumpadPlus,
            vk => KusemaKey::Custom {
                vk,
                name: hook::windows::key_name(vk),
            },
        };
        match keys::check_custom(captured.vk) {
            Ok(()) => CapturedKey {
                key: Some(key),
                refusal: None,
                cancelled: false,
            },
            Err(refusal) => CapturedKey {
                key: None,
                refusal: Some(refusal),
                cancelled: false,
            },
        }
    };
    info!("Kusema key capture: {result:?}");
    let _ = app.emit("kusema-key-captured", result);
}

/// What settings and setup show about the key.
#[derive(Debug, Clone, Serialize, Type)]
pub struct KusemaKeyInfo {
    pub key: KusemaKey,
    pub enabled: bool,
    pub classic_shortcuts: bool,
    /// This keyboard has sent a Copilot key press since Kusema started.
    pub seen_copilot: bool,
    /// ...and a Right Ctrl press.
    pub seen_right_ctrl: bool,
    /// A laptop, whose keyboard may have no Right Ctrl.
    pub is_laptop: bool,
    /// Suggest the Copilot key: it's been seen and Right Ctrl hasn't.
    pub suggest_copilot: bool,
}

pub fn suggest_copilot(seen_copilot: bool, seen_right_ctrl: bool, key: &KusemaKey) -> bool {
    seen_copilot && !seen_right_ctrl && *key == KusemaKey::RightCtrl
}

#[tauri::command]
#[specta::specta]
pub fn get_kusema_key_info(app: AppHandle) -> KusemaKeyInfo {
    let settings = get_settings(&app);
    #[cfg(target_os = "windows")]
    let (seen_copilot, seen_right_ctrl) =
        hook::windows::with_state(|s| (s.seen_copilot, s.seen_right_ctrl));
    #[cfg(not(target_os = "windows"))]
    let (seen_copilot, seen_right_ctrl) = (false, false);
    KusemaKeyInfo {
        suggest_copilot: suggest_copilot(seen_copilot, seen_right_ctrl, &settings.kusema_key),
        key: settings.kusema_key,
        enabled: settings.kusema_key_enabled,
        classic_shortcuts: settings.classic_shortcuts,
        seen_copilot,
        seen_right_ctrl,
        is_laptop: crate::helpers::clamshell::is_laptop().unwrap_or(false),
    }
}

fn changed(app: &AppHandle, settings: AppSettings, name: &str) {
    apply_settings(&settings);
    write_settings(app, settings);
    let _ = app.emit("settings-changed", serde_json::json!({ "setting": name }));
}

#[tauri::command]
#[specta::specta]
pub fn set_kusema_key(app: AppHandle, key: KusemaKey) -> Result<(), String> {
    if let KusemaKey::Custom { vk, .. } = &key {
        keys::check_custom(*vk).map_err(|r| format!("{r:?}"))?;
    }
    let mut settings = get_settings(&app);
    settings.kusema_key = key;
    changed(&app, settings, "kusema_key");
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_kusema_key_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.kusema_key_enabled = enabled;
    changed(&app, settings, "kusema_key_enabled");
    Ok(())
}

/// Turns the classic per-action shortcuts on or off, registering them or not.
#[tauri::command]
#[specta::specta]
pub fn change_classic_shortcuts_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.classic_shortcuts = enabled;
    let bindings = settings.bindings.clone();
    let post_process = settings.post_process_enabled;
    changed(&app, settings, "classic_shortcuts");
    for id in CLASSIC {
        let Some(binding) = bindings.get(*id) else {
            continue;
        };
        if *id == "transcribe_with_post_process" && !post_process {
            continue;
        }
        let result = if enabled {
            crate::shortcut::register_shortcut(&app, binding.clone())
        } else {
            crate::shortcut::unregister_shortcut(&app, binding.clone())
        };
        if let Err(e) = result {
            debug!("Classic shortcut '{id}': {e}");
        }
    }
    Ok(())
}

/// The next key pressed becomes the Kusema key (Escape cancels). The answer
/// comes as `kusema-key-captured`.
#[tauri::command]
#[specta::specta]
pub fn start_kusema_key_capture() {
    #[cfg(target_os = "windows")]
    hook::windows::with_state(|s| s.capturing = true);
}

#[tauri::command]
#[specta::specta]
pub fn cancel_kusema_key_capture() {
    #[cfg(target_os = "windows")]
    hook::windows::with_state(|s| s.capturing = false);
}

#[tauri::command]
#[specta::specta]
pub fn dismiss_simpler_keys_card(app: AppHandle) {
    let mut settings = get_settings(&app);
    settings.simpler_keys_card = false;
    changed(&app, settings, "simpler_keys_card");
}

/// The tray's "Kusema keys": the main window, with the card.
pub fn show_keys_card(app: &AppHandle) {
    crate::show_main_window(app);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("show-kusema-keys", ());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trigger_word_is_spotted_as_its_heard() {
        let settings = crate::settings::get_default_settings();
        let forms = trigger_forms(&settings);
        for said in [
            "Kusema, hi",
            "Kusama, hi",
            "Kyzma, hi",
            "Kuzma, hi",
            "Ku sema, hi",
        ] {
            assert!(
                matches!(
                    trigger::find(said, &forms),
                    trigger::TriggerMatch::Found { .. }
                ),
                "{said}"
            );
        }
        // Later in a sentence, Kuzma is just a name.
        assert!(!matches!(
            trigger::find("Thanks to Kuzma, the letter went out", &forms),
            trigger::TriggerMatch::Found { .. }
        ));
        // Another trigger word doesn't take Kusema's mishearings.
        let mut other = settings.clone();
        other.voice_edit_trigger = "Computer".into();
        let forms = trigger_forms(&other);
        assert!(matches!(
            trigger::find("Computer, hi", &forms),
            trigger::TriggerMatch::Found { .. }
        ));
        assert!(!matches!(
            trigger::find("Kyzma, hi", &forms),
            trigger::TriggerMatch::Found { .. }
        ));
        other.voice_edit_trigger = String::new();
        assert!(trigger_forms(&other).is_empty());
    }

    fn decided(text: &str, forced: bool) -> Decision {
        let settings = crate::settings::get_default_settings();
        decide(trigger::find(text, &trigger_forms(&settings)), forced, text)
    }

    #[test]
    fn a_space_tap_routes_without_the_trigger_word() {
        use route::{route, Route, TextHere};
        let rest = |d: Decision| match d {
            Decision::Route(rest) => rest,
            other => panic!("not routed: {other:?}"),
        };
        // A question.
        let q = rest(decided("What's the capital of Ghana?", true));
        assert!(matches!(route(&q, TextHere::Nothing), Route::Ask { .. }));
        // The screen, with its confirm card.
        let s = rest(decided("Look at my screen and tell me what's wrong", true));
        assert!(matches!(route(&s, TextHere::Nothing), Route::Screen(_)));
        // With text selected, an instruction is an edit.
        let e = rest(decided("Make this more formal", true));
        assert!(matches!(route(&e, TextHere::Selected), Route::Edit(_)));
        // Saying the trigger word anyway does no harm.
        assert_eq!(
            decided("Kusema, what time is it?", true),
            Decision::Route("what time is it?".into())
        );
        // Nothing said: nothing to ask.
        assert_eq!(decided("", true), Decision::Type { hint: false });
    }

    #[test]
    fn a_near_miss_is_typed_with_a_hint() {
        assert_eq!(
            decided("Cosmo, what time is it?", false),
            Decision::Type { hint: true }
        );
        assert_eq!(
            decided("Cosmetics are expensive.", false),
            Decision::Type { hint: false }
        );
        assert_eq!(
            decided("Kosima, what time is it?", false),
            Decision::Route("what time is it?".into())
        );
        // A Space tap needs no hint: it's routed.
        assert!(matches!(
            decided("Cosmo, what time is it?", true),
            Decision::Route(_)
        ));
    }

    #[test]
    fn variants_added_from_correct_that_are_heard() {
        let mut settings = crate::settings::get_default_settings();
        let said = "Koozeymah, what time is it?";
        assert!(!matches!(
            trigger::find(said, &trigger_forms(&settings)),
            trigger::TriggerMatch::Found { .. }
        ));
        settings.trigger_variants.push("Koozeymah".into());
        assert!(matches!(
            trigger::find(said, &trigger_forms(&settings)),
            trigger::TriggerMatch::Found { .. }
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn the_kusema_key_wins_over_a_classic_shortcut() {
        let press = |down| hook::RawKey {
            vk: keys::vk::RCONTROL,
            down,
            injected: false,
            extra_info: 0,
            win_shift: false,
        };
        hook::windows::with_state(|s| {
            s.key = Some(KusemaKey::RightCtrl);
            hook::classify(s, press(true));
        });
        // Right Ctrl + Space reads as Ctrl+Space to a classic shortcut: it
        // gives way (and its Space becomes the tap).
        assert!(classic_while_held("transcribe", "ctrl+space", true));
        assert!(classic_while_held("transcribe", "ctrl+space", false));
        assert!(classic_while_held("assistant", "ctrl+alt+space", true));
        // Not a classic one: Cancel and the Ask box work as usual.
        assert!(!classic_while_held("cancel", "escape", true));
        assert!(!classic_while_held(
            crate::ask_box::BINDING,
            "ctrl+alt+k",
            true
        ));
        hook::windows::with_state(|s| {
            hook::classify(s, press(false));
            s.key = None;
        });
        // Key let go: classic shortcuts are theirs again.
        assert!(!classic_while_held("transcribe", "ctrl+space", true));
    }

    #[test]
    fn the_copilot_key_is_suggested_only_without_a_right_ctrl() {
        let right = KusemaKey::RightCtrl;
        assert!(suggest_copilot(true, false, &right));
        assert!(!suggest_copilot(true, true, &right));
        assert!(!suggest_copilot(false, false, &right));
        assert!(!suggest_copilot(true, false, &KusemaKey::Copilot));
    }

    #[test]
    fn classic_shortcuts_are_skipped_only_when_off() {
        let mut settings = crate::settings::get_default_settings();
        assert!(skip_binding(&settings, "transcribe"));
        assert!(skip_binding(&settings, crate::dictation::BINDING));
        // These keep their own keys either way.
        for id in [
            crate::ask_box::BINDING,
            "cancel",
            crate::network::TOGGLE_BINDING,
        ] {
            assert!(!skip_binding(&settings, id), "{id}");
        }
        settings.classic_shortcuts = true;
        assert!(!skip_binding(&settings, "transcribe"));
    }

    #[test]
    fn the_default_ask_box_key_clashes_with_no_other_kusema_key() {
        let settings = crate::settings::get_default_settings();
        let ask = &settings.bindings[crate::ask_box::BINDING].default_binding;
        #[cfg(not(target_os = "macos"))]
        assert_eq!(ask, "ctrl+alt+k");
        for (id, binding) in &settings.bindings {
            assert!(
                id == crate::ask_box::BINDING || &binding.default_binding != ask,
                "{id}"
            );
        }
        crate::shortcut::handy_keys::validate_shortcut(ask).unwrap();
        crate::shortcut::tauri_impl::validate_shortcut(ask).unwrap();
    }
}
