//! Continuous dictation: press once, keep talking. The microphone stays open;
//! each pause cuts a segment, which is transcribed locally and typed into the
//! window the session started in.
//!
//! One thread does the work in order for each segment (transcribe, save to
//! History, type), so text is safe before it's typed, and a monitor thread
//! watches the foreground window, silence and the microphone. Nothing here
//! calls AI cleanup or any cloud service: only the local model and the keyboard.

#[cfg(debug_assertions)]
mod debug_feed;
mod guard;
mod join;
mod platform;
mod segmenter;
mod voice_commands;

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter, Manager};
use tauri_specta::Event;

use crate::actions::ShortcutAction;
use crate::audio_feedback::{play_session_sound, play_session_sound_blocking, SoundType};
use crate::audio_toolkit::{
    audio::FrameTap, constants::WHISPER_SAMPLE_RATE, vad::VAD_CONTINUOUS_HANGOVER_MS, VadPolicy,
};
use crate::clipboard::{press_backspaces, type_segment, OnTarget, TypeOutcome};
use crate::managers::audio::{recording_error_type, AudioRecordingManager};
use crate::managers::history::HistoryManager;
use crate::managers::model::ModelManager;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{get_settings, write_settings, ClipboardHandling};
use crate::shortcut;
use crate::tray::{set_tray_state, TrayIconState};
use crate::utils;
use crate::TranscriptionCoordinator;

use guard::{check_scratch, Disposition, HistoryTail, LastTyped, ScratchRefusal, WindowGuard};
use join::JoinState;
use segmenter::SegmentCutter;
use voice_commands::VoiceCommand;

pub const BINDING: &str = "dictate_continuous";

/// History entries of a session have no audio; the file name says so to the UI.
pub const NO_AUDIO_FILE_PREFIX: &str = "continuous-";

pub const PAUSE_MS_RANGE: (u64, u64) = (300, 3000);
pub const AUTO_STOP_MINUTES_RANGE: (u32, u32) = (1, 30);

const MONITOR_INTERVAL: Duration = Duration::from_millis(100);
/// How long a passing message stays in the overlay.
const NOTICE_TIME: Duration = Duration::from_secs(4);
/// How long the reason a session ended stays up after it ends.
const END_NOTICE_TIME: Duration = Duration::from_secs(4);
/// Audio waiting for a model slower than speech. Past this the session stops
/// (what's queued still finishes) instead of growing without bound.
const MAX_BACKLOG_SAMPLES: usize = 5 * 60 * WHISPER_SAMPLE_RATE as usize;
/// Failures in a row before a session gives up: the model isn't coming back.
const MAX_FAILURES: u32 = 3;
const LINE_BREAK: &str = "\n";
const PARAGRAPH_BREAK: &str = "\n\n";

/// What the overlay shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DictationPhase {
    Dictating,
    /// The target window isn't in front.
    Paused,
    /// Started in Kusema: waiting for a window to type into.
    NoTarget,
    /// Stopped; the last segments are finishing.
    Finishing,
    /// Over; the overlay is only still up to say why.
    Ended,
}

/// A message under the phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DictationNotice {
    NothingToScratch,
    ScratchWindowChanged,
    ScratchOtherInput,
    /// Something said while paused was saved to History, not typed.
    SavedToHistory,
    StoppedSilence,
    MicrophoneLost,
    CantKeepUp,
    TranscriptionFailed,
    PasteFailed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct DictationStatusEvent {
    pub phase: DictationPhase,
    pub notice: Option<DictationNotice>,
    /// When the session started, in milliseconds since the epoch.
    pub started_at_ms: f64,
}

/// Why a session ended, when the overlay should say so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StopReason {
    User,
    Silence,
    MicrophoneLost,
    CantKeepUp,
    TranscriptionFailed,
}

impl StopReason {
    fn notice(self) -> Option<DictationNotice> {
        match self {
            StopReason::User => None,
            StopReason::Silence => Some(DictationNotice::StoppedSilence),
            StopReason::MicrophoneLost => Some(DictationNotice::MicrophoneLost),
            StopReason::CantKeepUp => Some(DictationNotice::CantKeepUp),
            StopReason::TranscriptionFailed => Some(DictationNotice::TranscriptionFailed),
        }
    }
}

enum Message {
    Segment {
        samples: Vec<f32>,
        started_at: Instant,
    },
    /// Held text can be typed now.
    TypeHeld,
    /// Every segment has been sent.
    Finish,
}

/// What's been typed and saved, and the window rules.
struct Core {
    guard: WindowGuard,
    typed_join: JoinState,
    history_join: JoinState,
    last_typed: Option<LastTyped>,
    /// Spoken before the window changed; typed on return.
    held: Vec<Piece>,
    history_id: Option<i64>,
    /// Text went to History without being typed since the session began.
    saved_untyped: bool,
    notice: Option<(DictationNotice, Instant)>,
    last_status: Option<DictationStatusEvent>,
}

/// Something to write: spoken words, or a line break a command asked for.
#[derive(Clone, Debug)]
enum Piece {
    Words(String),
    Break(&'static str),
}

impl Piece {
    fn join(&self, state: &JoinState) -> (String, JoinState) {
        match self {
            Piece::Words(words) => state.join(words),
            Piece::Break(line_break) => state.join_break(line_break),
        }
    }
}

struct Shared {
    app: AppHandle,
    core: Mutex<Core>,
    voice_commands: bool,
    started_at_ms: f64,
    clock: Instant,
    /// `clock` time of the last words. Not of the last sound the detector took
    /// for speech: room noise does that all day, and transcribes to nothing.
    last_words_ms: AtomicU64,
    backlog_samples: AtomicUsize,
    finishing: AtomicBool,
    type_held_queued: AtomicBool,
    stop_monitor: AtomicBool,
    stop_reason: Mutex<Option<StopReason>>,
}

struct Session {
    shared: Arc<Shared>,
    tx: Sender<Message>,
    worker: JoinHandle<()>,
    monitor: JoinHandle<()>,
    #[cfg(debug_assertions)]
    debug_feed: Option<debug_feed::DebugFeed>,
}

/// Tauri state: the running session, if any.
#[derive(Default)]
pub struct DictationState {
    session: Mutex<Option<Session>>,
}

/// Runs on the recorder's delivery thread: turns audio into queued segments.
pub(crate) struct SegmentTap {
    cutter: SegmentCutter,
    started_at: Instant,
    shared: Arc<Shared>,
    tx: Sender<Message>,
}

impl SegmentTap {
    fn new(pause_ms: u64, shared: Arc<Shared>, tx: Sender<Message>) -> Self {
        Self {
            cutter: SegmentCutter::new(pause_ms, VAD_CONTINUOUS_HANGOVER_MS),
            started_at: Instant::now(),
            shared,
            tx,
        }
    }

    fn send(&self, segment: Option<Vec<f32>>) {
        if let Some(samples) = segment {
            self.shared
                .backlog_samples
                .fetch_add(samples.len(), Ordering::Relaxed);
            let _ = self.tx.send(Message::Segment {
                samples,
                started_at: self.started_at,
            });
        }
    }
}

impl FrameTap for SegmentTap {
    fn speech(&mut self, frame: &[f32]) {
        if !self.cutter.is_collecting() {
            self.started_at = Instant::now();
        }
        let segment = self.cutter.push_speech(frame);
        self.send(segment);
    }

    fn silence(&mut self, samples: usize) {
        let segment = self.cutter.push_silence(samples);
        self.send(segment);
    }

    fn finish(&mut self) {
        let segment = self.cutter.finish();
        self.send(segment);
    }
}

impl Shared {
    fn now_ms(&self) -> u64 {
        self.clock.elapsed().as_millis() as u64
    }

    fn set_notice(&self, notice: DictationNotice) {
        self.core.lock().unwrap().notice = Some((notice, Instant::now()));
        self.emit_status();
    }

    /// Tells the overlay the current state, if it changed.
    fn emit_status(&self) {
        let mut core = self.core.lock().unwrap();
        let phase = if self.finishing.load(Ordering::Acquire) {
            DictationPhase::Finishing
        } else if core.guard.target().is_none() {
            DictationPhase::NoTarget
        } else if core.guard.is_paused() {
            DictationPhase::Paused
        } else {
            DictationPhase::Dictating
        };
        let status = DictationStatusEvent {
            phase,
            notice: core.notice.map(|(notice, _)| notice),
            started_at_ms: self.started_at_ms,
        };
        if core.last_status.as_ref() != Some(&status) {
            core.last_status = Some(status.clone());
            drop(core);
            crate::glow::set_paused(&self.app, status.phase == DictationPhase::Paused);
            if let Err(e) = status.emit(&self.app) {
                warn!("Failed to emit the dictation status: {e}");
            }
        }
    }
}

/// Asks for the session to end. Goes through the coordinator, which runs the
/// stop the same way a key press does, whoever asked.
fn request_stop(app: &AppHandle, shared: &Shared, reason: StopReason) {
    {
        let mut stop_reason = shared.stop_reason.lock().unwrap();
        if stop_reason.is_some() {
            return;
        }
        *stop_reason = Some(reason);
    }
    info!("Stopping continuous dictation: {reason:?}");
    if let Some(coordinator) = app.try_state::<TranscriptionCoordinator>() {
        coordinator.stop_recording(BINDING);
    }
}

/// Stops the session if one is running (Escape, the overlay's stop button, the
/// tray, `--cancel`), keeping everything said so far. False if there's none.
pub fn stop_if_active(app: &AppHandle) -> bool {
    let Some(state) = app.try_state::<DictationState>() else {
        return false;
    };
    let shared = match state.session.lock().unwrap().as_ref() {
        Some(session) => Arc::clone(&session.shared),
        None => return false,
    };
    request_stop(app, &shared, StopReason::User);
    true
}

/* ──────────────────────────── the working thread ──────────────────────── */

fn worker(shared: Arc<Shared>, rx: Receiver<Message>) {
    let tm = Arc::clone(&*shared.app.state::<Arc<TranscriptionManager>>());
    let mut failures = 0;
    while let Ok(message) = rx.recv() {
        match message {
            Message::Segment {
                samples,
                started_at,
            } => {
                shared
                    .backlog_samples
                    .fetch_sub(samples.len(), Ordering::Relaxed);
                // Waits out a load in progress, and recovers from an unload.
                tm.initiate_model_load();
                match tm.transcribe(samples) {
                    Ok(text) => {
                        failures = 0;
                        if !text.trim().is_empty() {
                            shared
                                .last_words_ms
                                .store(shared.now_ms(), Ordering::Relaxed);
                        }
                        handle_text(&shared, text.trim(), started_at);
                    }
                    Err(e) => {
                        failures += 1;
                        error!("Couldn't transcribe a dictation segment: {e}");
                        shared.set_notice(DictationNotice::TranscriptionFailed);
                        let _ = shared.app.emit("transcription-error", e.to_string());
                        if failures >= MAX_FAILURES {
                            request_stop(&shared.app, &shared, StopReason::TranscriptionFailed);
                        }
                    }
                }
            }
            Message::TypeHeld => {
                shared.type_held_queued.store(false, Ordering::Release);
                type_held(&shared);
            }
            Message::Finish => {
                type_held(&shared);
                break;
            }
        }
    }
}

fn handle_text(shared: &Shared, text: &str, started_at: Instant) {
    if text.is_empty() {
        return;
    }
    let command = shared
        .voice_commands
        .then(|| voice_commands::parse(text))
        .flatten();
    match command {
        Some(VoiceCommand::StopDictation) => {
            request_stop(&shared.app, shared, StopReason::User);
        }
        Some(VoiceCommand::ScratchThat) => scratch(shared),
        Some(VoiceCommand::NewLine) => write(shared, Piece::Break(LINE_BREAK), started_at),
        Some(VoiceCommand::NewParagraph) => {
            write(shared, Piece::Break(PARAGRAPH_BREAK), started_at);
            crate::setup::first_wins::record(
                &shared.app,
                crate::setup::first_wins::Win::NewParagraph,
            );
        }
        None => {
            // Local only. AI cleanup never runs per segment.
            let settings = get_settings(&shared.app);
            let cleaned = crate::cleanup::apply_to_segment(&settings, text, shared.voice_commands);
            let cleaned = cleaned.trim_matches(' ');
            if !cleaned.is_empty() {
                write(shared, Piece::Words(cleaned.to_string()), started_at);
                if cleaned.contains(PARAGRAPH_BREAK) {
                    crate::setup::first_wins::record(
                        &shared.app,
                        crate::setup::first_wins::Win::NewParagraph,
                    );
                }
                crate::setup::first_wins::record(
                    &shared.app,
                    crate::setup::first_wins::Win::Dictated,
                );
            }
        }
    }
}

/// Saves a piece to History first, so a crash can't lose it, then types it if
/// the window rules allow.
fn write(shared: &Shared, piece: Piece, started_at: Instant) {
    let (disposition, history) = {
        let mut core = shared.core.lock().unwrap();
        // The monitor only looks ten times a second.
        core.guard
            .pause_if_away(platform::foreground(), Instant::now());
        let history = append_to_history(shared, &mut core, &piece);
        (core.guard.disposition(started_at), history)
    };
    shared.emit_status();
    match disposition {
        Disposition::Type => {
            // Anything still held was spoken first.
            let in_order = shared.core.lock().unwrap().held.is_empty();
            if !in_order {
                type_held(shared);
            }
            type_piece(shared, piece, history);
        }
        Disposition::Hold => shared.core.lock().unwrap().held.push(piece),
        Disposition::HistoryOnly => {
            shared.core.lock().unwrap().saved_untyped = true;
            shared.set_notice(DictationNotice::SavedToHistory);
        }
    }
}

/// Returns what was saved, so it can be taken back if the piece is scratched.
fn append_to_history(shared: &Shared, core: &mut Core, piece: &Piece) -> Option<HistoryTail> {
    let join_before = core.history_join;
    let (text, history_join) = piece.join(&join_before);
    if text.is_empty() {
        return None;
    }
    let hm = shared.app.state::<Arc<HistoryManager>>();
    let appended = match core.history_id {
        Some(id) => match hm.append_text(id, &text) {
            Ok(found) => found,
            Err(e) => {
                error!("Couldn't add to the dictation's History entry: {e}");
                return None;
            }
        },
        None => false,
    };
    // No entry yet, or it's gone: the user deleted it, or retention did.
    if !appended {
        let file_name = format!(
            "{NO_AUDIO_FILE_PREFIX}{}.txt",
            chrono::Utc::now().timestamp()
        );
        match hm.save_entry(
            file_name,
            text.trim_start().to_string(),
            false,
            None,
            None,
            None,
        ) {
            Ok(entry) => core.history_id = Some(entry.id),
            Err(e) => {
                error!("Couldn't save the dictation to History: {e}");
                return None;
            }
        }
    }
    // Whatever was typed last is no longer History's tail.
    if let Some(last) = &mut core.last_typed {
        last.history = None;
    }
    core.history_join = history_join;
    Some(HistoryTail {
        chars: text.chars().count(),
        join_before,
    })
}

/// Whether keystrokes sent now would reach `target` as sent. A modifier the user
/// is holding counts against it: Alt (mid Alt+Tab) would turn Ctrl+V into a
/// different shortcut, and the text would silently not arrive.
fn on_window(target: isize) -> OnTarget {
    Arc::new(move || {
        platform::foreground().is_some_and(|f| f.window == target) && !platform::modifier_held()
    })
}

fn type_piece(shared: &Shared, piece: Piece, history: Option<HistoryTail>) {
    let (target, join_before) = {
        let core = shared.core.lock().unwrap();
        (core.guard.target(), core.typed_join)
    };
    let Some(target) = target else {
        return;
    };
    let (text, join_after) = piece.join(&join_before);
    if text.is_empty() {
        return;
    }
    let outcome = type_segment(&text, &shared.app, on_window(target));
    debug!("Typing: {outcome:?}");
    match outcome {
        Ok(TypeOutcome::Typed) => {
            let mut core = shared.core.lock().unwrap();
            core.typed_join = join_after;
            core.last_typed = platform::focus().map(|focus| LastTyped {
                chars: text.chars().count(),
                focus,
                input_tick: platform::last_input_tick(),
                join_before,
                history,
            });
        }
        Ok(TypeOutcome::NotTyping) => {
            // The "None" paste method: History is the only output.
            shared.core.lock().unwrap().typed_join = join_after;
        }
        Ok(TypeOutcome::WindowChanged) => {
            debug!("The window changed just before typing; holding the segment");
            shared.core.lock().unwrap().held.push(piece);
        }
        Err(e) => {
            error!("Couldn't type a dictation segment: {e}");
            let _ = shared.app.emit("paste-error", ());
            shared.set_notice(DictationNotice::PasteFailed);
        }
    }
}

fn type_held(shared: &Shared) {
    let held = std::mem::take(&mut shared.core.lock().unwrap().held);
    // Saved when spoken; other text may have followed them into History, so
    // scratching one of these leaves History as it is.
    for piece in held {
        type_piece(shared, piece, None);
    }
}

fn scratch(shared: &Shared) {
    let mut core = shared.core.lock().unwrap();
    let decision = check_scratch(
        core.last_typed.as_ref(),
        core.guard.is_paused(),
        platform::focus(),
        platform::last_input_tick(),
    );
    let chars = match decision {
        Ok(chars) => chars,
        Err(refusal) => {
            debug!("Not scratching: {refusal:?}");
            drop(core);
            shared.set_notice(match refusal {
                ScratchRefusal::NothingToScratch => DictationNotice::NothingToScratch,
                ScratchRefusal::WindowChanged => DictationNotice::ScratchWindowChanged,
                ScratchRefusal::OtherInput => DictationNotice::ScratchOtherInput,
            });
            return;
        }
    };
    let Some(last) = core.last_typed.take() else {
        return;
    };
    let target = last.focus.window;
    drop(core);

    if let Err(e) = press_backspaces(chars, &shared.app, on_window(target)) {
        error!("Couldn't scratch the last segment: {e}");
        shared.set_notice(DictationNotice::PasteFailed);
        return;
    }

    let mut core = shared.core.lock().unwrap();
    core.typed_join = last.join_before;
    if let (Some(history), Some(id)) = (last.history, core.history_id) {
        let hm = shared.app.state::<Arc<HistoryManager>>();
        match hm.trim_text_end(id, history.chars) {
            Ok(_) => core.history_join = history.join_before,
            Err(e) => error!("Couldn't remove scratched text from History: {e}"),
        }
    }
}

/* ──────────────────────────── the monitor thread ──────────────────────── */

fn monitor(shared: Arc<Shared>, tx: Sender<Message>, auto_stop: Duration) {
    let rm = Arc::clone(&*shared.app.state::<Arc<AudioRecordingManager>>());
    while !shared.stop_monitor.load(Ordering::Acquire) {
        thread::sleep(MONITOR_INTERVAL);
        let now = Instant::now();
        {
            let mut core = shared.core.lock().unwrap();
            core.guard.observe(platform::foreground(), now);
            if !core.guard.is_paused()
                && !core.held.is_empty()
                && !shared.type_held_queued.swap(true, Ordering::AcqRel)
            {
                let _ = tx.send(Message::TypeHeld);
            }
            if core
                .notice
                .is_some_and(|(_, since)| now.duration_since(since) >= NOTICE_TIME)
            {
                core.notice = None;
            }
        }
        shared.emit_status();

        if shared.finishing.load(Ordering::Acquire) {
            continue;
        }
        let now_ms = shared.now_ms();
        let silent_for = now_ms - shared.last_words_ms.load(Ordering::Relaxed).min(now_ms);
        let backlog = shared.backlog_samples.load(Ordering::Relaxed);
        // Segments still queued may hold words, however long they've waited.
        if Duration::from_millis(silent_for) >= auto_stop && backlog == 0 {
            request_stop(&shared.app, &shared, StopReason::Silence);
        } else if rm.microphone_failed() {
            request_stop(&shared.app, &shared, StopReason::MicrophoneLost);
        } else if backlog > MAX_BACKLOG_SAMPLES {
            request_stop(&shared.app, &shared, StopReason::CantKeepUp);
        }
    }
}

/* ──────────────────────────── start and stop ──────────────────────────── */

/// Whatever else happens, the coordinator hears the session is over.
struct NotifyFinished(AppHandle);

impl Drop for NotifyFinished {
    fn drop(&mut self) {
        if let Some(coordinator) = self.0.try_state::<TranscriptionCoordinator>() {
            coordinator.notify_processing_finished();
        }
        crate::memory::trim_freed_memory();
    }
}

fn new_join_state(settings: &crate::settings::AppSettings) -> JoinState {
    if settings.local_cleanup_enabled {
        JoinState::with_period_insertion()
    } else {
        JoinState::default()
    }
}

fn start(app: &AppHandle) {
    if !platform::SUPPORTED {
        warn!("Continuous dictation is Windows only for now");
        return;
    }
    crate::tts::stop(app);

    let tm = Arc::clone(&*app.state::<Arc<TranscriptionManager>>());
    let rm = Arc::clone(&*app.state::<Arc<AudioRecordingManager>>());
    tm.initiate_model_load();
    let settings = get_settings(app);
    if !tm.is_model_loaded() {
        if let Err(e) = app
            .state::<Arc<ModelManager>>()
            .get_model_path(&settings.selected_model)
        {
            warn!("Not starting dictation: no model can transcribe it ({e})");
            return;
        }
    }

    let started_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64);
    let shared = Arc::new(Shared {
        app: app.clone(),
        core: Mutex::new(Core {
            guard: WindowGuard::new(platform::foreground()),
            typed_join: new_join_state(&settings),
            history_join: new_join_state(&settings),
            last_typed: None,
            held: Vec::new(),
            history_id: None,
            saved_untyped: false,
            notice: None,
            last_status: None,
        }),
        voice_commands: settings.dictation_voice_commands,
        started_at_ms,
        clock: Instant::now(),
        last_words_ms: AtomicU64::new(0),
        backlog_samples: AtomicUsize::new(0),
        finishing: AtomicBool::new(false),
        type_held_queued: AtomicBool::new(false),
        stop_monitor: AtomicBool::new(false),
        stop_reason: Mutex::new(None),
    });
    let (tx, rx) = mpsc::channel();

    let pause_ms = settings.dictation_pause_ms;
    #[cfg(debug_assertions)]
    let debug_wav = debug_feed::configured_wav();
    let make_tap = {
        let (shared, tx) = (Arc::clone(&shared), tx.clone());
        #[cfg(debug_assertions)]
        let real_mic = debug_wav.is_none();
        #[cfg(not(debug_assertions))]
        let real_mic = true;
        move || -> Box<dyn FrameTap> {
            if real_mic {
                Box::new(SegmentTap::new(pause_ms, Arc::clone(&shared), tx.clone()))
            } else {
                Box::new(NoTap)
            }
        }
    };

    // The model stays loaded for the whole session, whatever the unload setting.
    tm.hold_model(true);
    set_tray_state(app, TrayIconState::Recording);
    utils::show_dictation_overlay(app);

    let readiness = match rm.try_start_segmented(BINDING, VadPolicy::Continuous, &make_tap) {
        Ok(readiness) => readiness,
        Err(e) => {
            error!("Couldn't start continuous dictation: {e:#}");
            tm.hold_model(false);
            utils::hide_recording_overlay(app);
            set_tray_state(app, TrayIconState::Idle);
            let _ = app.emit(
                "recording-error",
                serde_json::json!({
                    "error_type": recording_error_type(&e),
                    "detail": format!("{e:#}"),
                }),
            );
            return;
        }
    };

    let generation = readiness.generation();
    {
        let (app, rm) = (app.clone(), Arc::clone(&rm));
        thread::spawn(move || {
            if !readiness.wait() || !rm.is_recording_readiness_current(generation) {
                return;
            }
            utils::emit_recording_ready(&app);
            if rm.is_recording_readiness_current(generation) {
                play_session_sound_blocking(&app, SoundType::Start);
            }
            if rm.is_recording_readiness_current(generation) {
                rm.apply_mute();
            }
        });
    }

    shortcut::register_cancel_shortcut(app);

    let worker = {
        let shared = Arc::clone(&shared);
        thread::spawn(move || worker(shared, rx))
    };
    let monitor = {
        let (shared, tx) = (Arc::clone(&shared), tx.clone());
        let auto_stop = Duration::from_secs(u64::from(settings.dictation_auto_stop_minutes) * 60);
        thread::spawn(move || monitor(shared, tx, auto_stop))
    };
    #[cfg(debug_assertions)]
    let debug_feed = debug_wav.map(|wav| debug_feed::DebugFeed::start(wav, pause_ms, &shared, &tx));
    shared.emit_status();

    *app.state::<DictationState>().session.lock().unwrap() = Some(Session {
        shared,
        tx,
        worker,
        monitor,
        #[cfg(debug_assertions)]
        debug_feed,
    });
    info!("Continuous dictation started");
}

/// The real microphone's tap while a debug build is fed from a file.
struct NoTap;

impl FrameTap for NoTap {
    fn speech(&mut self, _frame: &[f32]) {}
    fn silence(&mut self, _samples: usize) {}
    fn finish(&mut self) {}
}

fn stop(app: &AppHandle) {
    let session = app.state::<DictationState>().session.lock().unwrap().take();
    let Some(session) = session else {
        // A start that failed left nothing to stop.
        let _finished = NotifyFinished(app.clone());
        return;
    };

    let rm = Arc::clone(&*app.state::<Arc<AudioRecordingManager>>());
    rm.invalidate_recording_readiness();
    // Escape goes back to the app in front now, however the session ended.
    shortcut::unregister_cancel_shortcut(app);
    session.shared.finishing.store(true, Ordering::Release);
    session.shared.emit_status();
    set_tray_state(app, TrayIconState::Transcribing);
    rm.remove_mute();
    play_session_sound(app, SoundType::Stop);

    let app = app.clone();
    // Off the coordinator's thread: the last segments take a moment.
    thread::spawn(move || {
        let _finished = NotifyFinished(app.clone());
        finish(&app, session, &rm);
    });
}

fn finish(app: &AppHandle, session: Session, rm: &AudioRecordingManager) {
    let Session {
        shared,
        tx,
        worker,
        monitor,
        #[cfg(debug_assertions)]
        debug_feed,
    } = session;

    #[cfg(debug_assertions)]
    if let Some(feed) = debug_feed {
        feed.finish();
    }
    // Hands the audio still held to the worker, as a last segment.
    let _ = rm.stop_recording(BINDING, rm.cancel_generation());
    let _ = tx.send(Message::Finish);
    if worker.join().is_err() {
        error!("The dictation worker panicked");
    }
    shared.stop_monitor.store(true, Ordering::Release);
    if monitor.join().is_err() {
        error!("The dictation monitor panicked");
    }

    let tm = app.state::<Arc<TranscriptionManager>>();
    tm.hold_model(false);
    tm.maybe_unload_immediately("continuous dictation");

    let (history_id, unsent, saved_untyped) = {
        let mut core = shared.core.lock().unwrap();
        (
            core.history_id,
            std::mem::take(&mut core.held),
            core.saved_untyped,
        )
    };
    copy_session_if_asked(app, history_id);
    set_tray_state(app, TrayIconState::Idle);

    let stop_reason = *shared.stop_reason.lock().unwrap();
    let end_notice = stop_reason
        .and_then(StopReason::notice)
        .or((saved_untyped || !unsent.is_empty()).then_some(DictationNotice::SavedToHistory));
    if let Some(notice) = end_notice {
        let status = DictationStatusEvent {
            phase: DictationPhase::Ended,
            notice: Some(notice),
            started_at_ms: shared.started_at_ms,
        };
        if let Err(e) = status.emit(app) {
            warn!("Failed to emit the dictation status: {e}");
        }
        thread::sleep(END_NOTICE_TIME);
    }
    utils::hide_recording_overlay(app);
    info!("Continuous dictation finished ({stop_reason:?})");
}

/// The "copy to clipboard" setting, applied to the whole session at its end
/// rather than to each segment.
fn copy_session_if_asked(app: &AppHandle, history_id: Option<i64>) {
    if get_settings(app).clipboard_handling != ClipboardHandling::CopyToClipboard {
        return;
    }
    let Some(id) = history_id else {
        return;
    };
    match app.state::<Arc<HistoryManager>>().entry_text(id) {
        Ok(Some(text)) => {
            if let Err(e) = platform::write_clipboard_private(&text) {
                warn!("Couldn't copy the dictation to the clipboard: {e}");
            }
        }
        Ok(None) => {}
        Err(e) => warn!("Couldn't read the dictation back: {e}"),
    }
}

pub use platform::write_clipboard_private;

#[cfg(test)]
pub fn command_phrases() -> impl Iterator<Item = &'static str> {
    voice_commands::phrases()
}

pub struct DictateContinuousAction;

impl ShortcutAction for DictateContinuousAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        start(app);
    }

    fn stop(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        stop(app);
    }
}

/* ──────────────────────────── settings ────────────────────────────────── */

#[tauri::command]
#[specta::specta]
pub fn change_dictation_pause_ms_setting(app: AppHandle, ms: u64) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.dictation_pause_ms = ms.clamp(PAUSE_MS_RANGE.0, PAUSE_MS_RANGE.1);
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_dictation_auto_stop_minutes_setting(
    app: AppHandle,
    minutes: u32,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.dictation_auto_stop_minutes =
        minutes.clamp(AUTO_STOP_MINUTES_RANGE.0, AUTO_STOP_MINUTES_RANGE.1);
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_dictation_voice_commands_setting(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.dictation_voice_commands = enabled;
    write_settings(&app, settings);
    Ok(())
}

#[cfg(test)]
mod line_command_tests {
    //! "new paragraph" and "new line" from what the model wrote to what's typed,
    //! in both kinds of dictation.
    use super::join::JoinState;
    use super::voice_commands::{self, VoiceCommand};
    use super::{LINE_BREAK, PARAGRAPH_BREAK};
    use crate::cleanup;
    use crate::settings::get_default_settings;

    fn normal(said: &str) -> String {
        cleanup::apply(&get_default_settings(), said, Some("en"))
    }

    /// What `handle_text` writes for these segments.
    fn continuous_with(segments: &[&str], voice_commands_on: bool) -> String {
        let settings = get_default_settings();
        let mut state = JoinState::with_period_insertion();
        let mut typed = String::new();
        for segment in segments {
            let command = voice_commands_on
                .then(|| voice_commands::parse(segment))
                .flatten();
            let (out, next) = match command {
                Some(VoiceCommand::NewParagraph) => state.join_break(PARAGRAPH_BREAK),
                Some(VoiceCommand::NewLine) => state.join_break(LINE_BREAK),
                _ => state.join(&cleanup::apply_to_segment(
                    &settings,
                    segment,
                    voice_commands_on,
                )),
            };
            typed.push_str(&out);
            state = next;
        }
        typed
    }

    fn continuous(segments: &[&str]) -> String {
        continuous_with(segments, true)
    }

    #[test]
    fn every_variant_said_alone_in_both_modes() {
        for said in [
            "new paragraph",
            "New paragraph",
            "New Paragraph",
            "NEW PARAGRAPH",
            "New paragraph.",
            "New paragraph,",
            "New paragraph\"",
            "New paragraph.\"",
            "new paragraphs",
            "New paragraphs.",
            "new para graph",
            "New para graph.",
            "new-paragraph",
            "newparagraph",
        ] {
            assert_eq!(normal(said), "\n\n", "normal: {said}");
            assert_eq!(
                continuous(&["We agree.", said, "the end"]),
                "We agree.\n\nThe end",
                "continuous: {said}"
            );
        }
        for said in [
            "new line",
            "New line",
            "NEW LINE",
            "New line.",
            "New line\"",
            "new lines",
            "New lines.",
            "new-line",
            "newline",
            "Newline.",
        ] {
            assert_eq!(normal(said), "\n", "normal: {said}");
            assert_eq!(
                continuous(&["Main Street", said, "apartment 4"]),
                "Main Street\napartment 4",
                "continuous: {said}"
            );
        }
    }

    #[test]
    fn said_mid_flow_in_both_modes() {
        for (said, written) in [
            (
                "Thank you new paragraph The court agreed",
                "Thank you\n\nThe court agreed",
            ),
            (
                "Thank you. New paragraph. the court agreed",
                "Thank you.\n\nThe court agreed",
            ),
            (
                "Thank you New paragraph\" The court agreed",
                "Thank you\n\nThe court agreed",
            ),
            (
                "Thank you new paragraphs The court agreed",
                "Thank you\n\nThe court agreed",
            ),
            (
                "Thank you new para graph. The court agreed",
                "Thank you\n\nThe court agreed",
            ),
            (
                "The motion is denied period new paragraph The parties agree",
                "The motion is denied.\n\nThe parties agree",
            ),
            (
                "Main Street new line Springfield",
                "Main Street\nSpringfield",
            ),
            (
                "Main Street, new line, apartment 4",
                "Main Street,\napartment 4",
            ),
            (
                "Main Street. New line. Springfield",
                "Main Street.\nSpringfield",
            ),
        ] {
            assert_eq!(normal(said), written, "normal: {said}");
            assert_eq!(continuous(&[said]), written, "continuous: {said}");
        }
    }

    #[test]
    fn at_the_end_of_a_segment_in_continuous_dictation() {
        assert_eq!(
            continuous(&["Thank you new paragraph", "the court agreed"]),
            "Thank you\n\nThe court agreed"
        );
        assert_eq!(
            continuous(&["Main Street new line", "apartment 4"]),
            "Main Street\napartment 4"
        );
    }

    #[test]
    fn at_the_start_of_a_segment_in_continuous_dictation() {
        assert_eq!(
            continuous(&["The court denied the motion", "New paragraph The parties"]),
            "The court denied the motion\n\nThe parties"
        );
    }

    #[test]
    fn normal_sentences_stay_words_in_both_modes() {
        for sentence in [
            "the new paragraph in the contract is fine",
            "The new paragraph in the contract is fine.",
            "We added a new paragraph.",
            "We added new paragraphs.",
            "Please add new paragraph.",
            "Insert new line.",
            "Start on a new line.",
            "New line of credit approved",
            "We signed a new line of credit.",
            "New Line Cinema released it",
            "the new paragraphs are fine",
            "two new paragraphs.",
            "the new para graph.",
        ] {
            assert_eq!(normal(sentence), sentence, "normal: {sentence}");
            assert_eq!(continuous(&[sentence]), sentence, "continuous: {sentence}");
        }
    }

    #[test]
    fn literal_types_the_words_in_both_modes() {
        for (said, written) in [
            ("literal new paragraph", "new paragraph"),
            ("Literal new paragraph.", "New paragraph."),
            ("literal new line", "new line"),
            ("literal new paragraphs", "new paragraphs"),
        ] {
            assert_eq!(normal(said), written, "normal: {said}");
            assert_eq!(continuous(&[said]), written, "continuous: {said}");
        }
    }

    #[test]
    fn other_spoken_marks_with_the_models_punctuation_in_both_modes() {
        for (said, written) in [
            ("Dear Sir Comma. thank you", "Dear Sir, thank you"),
            ("Dear Sir comma\" thank you", "Dear Sir, thank you"),
            ("Is it final Question mark.", "Is it final?"),
            ("We won Period.", "We won."),
        ] {
            assert_eq!(normal(said), written, "normal: {said}");
            assert_eq!(continuous(&[said]), written, "continuous: {said}");
        }
    }

    #[test]
    fn voice_commands_off_types_them_in_continuous_dictation() {
        assert_eq!(
            continuous_with(&["Thank you new paragraph The end"], false),
            "Thank you new paragraph The end"
        );
        assert_eq!(
            continuous_with(&["New paragraph."], false),
            "New paragraph."
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::shortcut::{handy_keys, tauri_impl};
    use crate::transcription_coordinator::is_transcribe_binding;
    use ::handy_keys as handy_keys_crate;

    #[test]
    fn the_default_key_is_valid_on_both_keyboard_backends() {
        let settings = crate::settings::get_default_settings();
        let binding = &settings.bindings[super::BINDING];
        assert_eq!(binding.default_binding, "ctrl+alt+shift+d");
        handy_keys::validate_shortcut(&binding.default_binding).unwrap();
        tauri_impl::validate_shortcut(&binding.default_binding).unwrap();
    }

    #[test]
    fn the_default_key_is_not_used_by_another_binding() {
        let settings = crate::settings::get_default_settings();
        let ours = &settings.bindings[super::BINDING].default_binding;
        for (id, binding) in &settings.bindings {
            assert!(
                id == super::BINDING || &binding.default_binding != ours,
                "{id}"
            );
        }
    }

    /// The strings the "Use Dragon's key" button sends (`DRAGON_KEY`).
    #[test]
    fn numpad_plus_is_valid_on_both_keyboard_backends() {
        let hotkey: handy_keys_crate::Hotkey = "keypadplus".parse().unwrap();
        assert_eq!(hotkey.key, Some(handy_keys_crate::Key::KeypadPlus));
        handy_keys::validate_shortcut("keypadplus").unwrap();
        tauri_impl::validate_shortcut("NumpadAdd").unwrap();
    }

    #[test]
    fn the_binding_goes_through_the_coordinator() {
        assert!(is_transcribe_binding(super::BINDING));
    }
}
