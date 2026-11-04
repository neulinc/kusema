//! The live preview in continuous dictation. The segment being spoken is
//! streamed to the pill; when a pause ends it, its preview stays until its
//! text is typed. There is one engine, and a segment's full run would end any
//! stream, so the preview pauses while the dictation transcribes a segment,
//! and picks up afterwards with what was said meanwhile.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use log::{debug, info};
use tauri::{AppHandle, Manager};

use super::throttle::Throttle;
use super::{clear, lock, Feeder};
use crate::engine_supervisor::StreamHandle;
use crate::managers::transcription::TranscriptionManager;

enum Command {
    Speech(Vec<f32>),
    /// The segment ended at a pause (or the length limit).
    SegmentEnd,
    /// The dictation is about to transcribe a segment; answer once the
    /// stream is out of the way.
    Pause(Sender<()>),
    Resume,
    /// A segment's text was typed (or turned out empty).
    Typed,
    Stop,
}

/// The preview of one continuous dictation session. Cheap to clone; every
/// call returns at once except `pause`, which waits for the preview thread.
#[derive(Clone)]
pub struct SegmentPreview {
    tx: Option<Sender<Command>>,
}

impl SegmentPreview {
    /// Starts the preview thread, or a preview that does nothing when the
    /// setting is off.
    pub fn start(app: &AppHandle) -> Self {
        if !super::enabled(&crate::settings::get_settings(app)) {
            return Self { tx: None };
        }
        let (tx, rx) = mpsc::channel();
        let app = app.clone();
        thread::spawn(move || run(app, rx));
        Self { tx: Some(tx) }
    }

    fn send(&self, command: Command) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(command);
        }
    }

    pub fn speech(&self, frame: &[f32]) {
        self.send(Command::Speech(frame.to_vec()));
    }

    pub fn segment_end(&self) {
        self.send(Command::SegmentEnd);
    }

    pub fn pause(&self) {
        let Some(tx) = &self.tx else { return };
        let (done, wait) = mpsc::channel();
        if tx.send(Command::Pause(done)).is_ok() {
            let _ = wait.recv();
        }
    }

    pub fn resume(&self) {
        self.send(Command::Resume);
    }

    pub fn typed(&self) {
        self.send(Command::Typed);
    }

    pub fn stop(&self) {
        self.send(Command::Stop);
    }
}

/// The preview thread's state.
struct Driver {
    app: AppHandle,
    tm: Arc<TranscriptionManager>,
    throttle: Arc<Mutex<Throttle>>,
    feeder: Feeder,
    /// The segment being spoken, so a stream begun late (after a pause) can
    /// catch up on it.
    segment: Vec<f32>,
    stream: Option<StreamHandle>,
    paused: bool,
    /// The loaded model has no stream: nothing to show this session.
    unavailable: bool,
}

impl Driver {
    fn off(&self) -> bool {
        self.unavailable || lock(&self.throttle).is_off()
    }

    /// Begins a stream for the segment so far, feeding it all at once.
    fn begin(&mut self) {
        if self.stream.is_some() || self.paused || self.off() || self.segment.is_empty() {
            return;
        }
        lock(&self.throttle).begin(self.segment.len());
        match self.tm.start_preview_stream(Arc::clone(&self.throttle)) {
            Some(stream) => {
                lock(&self.throttle).fed(self.segment.len());
                stream.feed(self.segment.clone());
                self.stream = Some(stream);
            }
            None => {
                info!("Live preview isn't available with this model");
                self.unavailable = true;
            }
        }
    }

    fn speech(&mut self, frame: Vec<f32>) {
        self.segment.extend_from_slice(&frame);
        if self.stream.is_none() {
            self.begin();
            return;
        }
        if let Some(chunk) = self.feeder.push(&self.throttle, &frame) {
            if let Some(stream) = &self.stream {
                stream.feed(chunk);
            }
        }
    }

    /// Dropping the handle resets the stream in the engine.
    fn end_stream(&mut self) {
        self.stream = None;
        self.feeder.clear();
    }
}

fn run(app: AppHandle, rx: Receiver<Command>) {
    let tm = Arc::clone(&*app.state::<Arc<TranscriptionManager>>());
    let mut driver = Driver {
        app,
        tm,
        throttle: Arc::new(Mutex::new(Throttle::new())),
        feeder: Feeder::default(),
        segment: Vec::new(),
        stream: None,
        paused: false,
        unavailable: false,
    };
    while let Ok(command) = rx.recv() {
        match command {
            Command::Speech(frame) => driver.speech(frame),
            Command::SegmentEnd => {
                driver.end_stream();
                driver.segment.clear();
            }
            Command::Pause(done) => {
                driver.end_stream();
                driver.paused = true;
                let _ = done.send(());
            }
            Command::Resume => {
                driver.paused = false;
                driver.begin();
            }
            Command::Typed => {
                // Its text is in the document now; clear unless the next
                // segment's preview has begun.
                if driver.segment.is_empty() {
                    clear(&driver.app);
                }
            }
            Command::Stop => break,
        }
    }
    debug!("Live preview for continuous dictation ended");
    driver.end_stream();
    clear(&driver.app);
}
