//! A recording's live decode: a stream the model decodes as audio arrives,
//! or long-form chunks decoded as the recording goes.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use log::{debug, error, info};
use transcribe_cpp::StreamOptions;

use super::chunked::{ChunkSetup, ChunkedDecode, PendingSetup};
use super::policy::LiveDecode;
use super::{
    join_chunks, lock_perf, PreviewScript, RunOutcome, RunSetup, StreamFinalizeReply, StreamPerf,
    TranscriptionManager,
};
use crate::engine_supervisor::{EngineError, LoadedInfo, StreamHandle, StreamProgress};
use crate::preview::throttle::Throttle;
use crate::preview::{Feeder, Progress};
use crate::settings::{get_settings, AppSettings};

/// One recording's live decode.
pub(super) struct LiveSession<'a> {
    decoder: LiveDecoder<'a>,
}

enum LiveDecoder<'a> {
    /// With the run setup its result finishes with.
    Stream(StreamSession<'a>, Box<RunSetup>),
    /// Its setup is resolved by its first chunk.
    Chunked(Box<ChunkedDecode<'a>>),
}

impl<'a> LiveSession<'a> {
    /// Begins the live decode `mode` picks on the loaded model. `None` when
    /// there is none or it couldn't begin: the recording is then transcribed
    /// in batch when it stops.
    pub(super) fn begin(
        tm: &'a TranscriptionManager,
        mode: LiveDecode,
        info: &LoadedInfo,
        model_id: &str,
    ) -> Option<Self> {
        if mode == LiveDecode::None {
            return None;
        }
        // The same run setup as the batch path: task + language gated
        // against what the model actually advertises.
        let settings = get_settings(&tm.app_handle);
        let plan = tm.language_plan(&settings, model_id, info);
        let decoder = match mode {
            LiveDecode::None => unreachable!("returned above"),
            LiveDecode::Stream => {
                // A stream's language is fixed before any audio, so it can't
                // be detected; a detect plan falls back.
                let setup = tm.setup_for(info, &settings, &plan, &[]);
                let stream = StreamSession::begin(tm, info, &settings, &setup, model_id)?;
                LiveDecoder::Stream(stream, Box::new(setup))
            }
            LiveDecode::Chunked => {
                info!("Live long-form chunking started (model '{}')", model_id);
                let backend = settings.vad_backend;
                LiveDecoder::Chunked(Box::new(ChunkedDecode::new(
                    tm,
                    tm.engine.cancel_mark(),
                    ChunkSetup::Pending(Box::new(PendingSetup {
                        plan,
                        info: info.clone(),
                        settings,
                    })),
                    model_id,
                    backend,
                )))
            }
        };
        Some(Self { decoder })
    }

    pub(super) fn feed(&mut self, pcm: Vec<f32>) {
        match &mut self.decoder {
            LiveDecoder::Stream(stream, _) => stream.feed(pcm),
            LiveDecoder::Chunked(chunks) => chunks.push(pcm),
        }
    }

    /// The live result; `None` to transcribe the recording in batch instead.
    /// `recorded_samples` is the whole recording's length.
    pub(super) fn finish(self, recorded_samples: usize) -> StreamFinalizeReply {
        match self.decoder {
            // Kusema: the stream is the live preview, never the typed text:
            // the whole recording is transcribed in one run instead, for the
            // best accuracy. Dropping it resets the stream.
            LiveDecoder::Stream(stream, _setup) => {
                stream.drop_for_batch();
                Ok(None)
            }
            LiveDecoder::Chunked(chunks) => finish_chunks(*chunks, recorded_samples),
        }
    }
}

fn finish_chunks(
    chunks: ChunkedDecode<'_>,
    recorded_samples: usize,
) -> Result<Option<RunOutcome>, EngineError> {
    // A failure stops the decode short of the recording; report the failure.
    if chunks.samples() != recorded_samples && !chunks.failed() {
        debug!(
            "Live chunks saw {} of the recording's {} samples; transcribing it in batch",
            chunks.samples(),
            recorded_samples
        );
        return Ok(None);
    }
    let while_recording = chunks.decoded();
    match chunks.finish() {
        Ok((transcripts, setup)) => {
            if while_recording > 0 {
                info!(
                    "{} of {} chunks were decoded while recording",
                    while_recording,
                    transcripts.len()
                );
            }
            let (text, model_detected_language) = join_chunks(transcripts);
            Ok(Some(RunOutcome {
                setup,
                text,
                model_detected_language,
            }))
        }
        Err(EngineError::Cancelled) => Err(EngineError::Cancelled),
        Err(e) => {
            info!("Live chunks failed ({}); transcribing in batch", e);
            Ok(None)
        }
    }
}

/// A stream the model decodes as audio arrives, with preview text emitted
/// to the overlay.
struct StreamSession<'a> {
    tm: &'a TranscriptionManager,
    stream: StreamHandle,
    perf: Arc<Mutex<StreamPerf>>,
    /// Kusema: the preview's pace, and the audio gathered for its next feed.
    throttle: Arc<Mutex<Throttle>>,
    feeder: Feeder,
}

impl<'a> StreamSession<'a> {
    fn begin(
        tm: &'a TranscriptionManager,
        info: &LoadedInfo,
        settings: &AppSettings,
        setup: &RunSetup,
        model_id: &str,
    ) -> Option<Self> {
        let output_language = setup.output_language(settings, None);

        // Feed results arrive on the engine's thread; this callback only
        // converts and emits preview text.
        let perf = Arc::new(Mutex::new(StreamPerf::new()));
        let throttle = Arc::new(Mutex::new(Throttle::new()));
        let on_progress = {
            let perf = Arc::clone(&perf);
            let languages = setup.languages.clone();
            let mut preview_script = PreviewScript::new(settings.chinese_script, &output_language);
            // Kusema: watches the pace, and shows the text as it will be typed.
            let mut preview = Progress::new(&tm.app_handle, Arc::clone(&throttle));
            move |progress: StreamProgress| {
                let mut perf = lock_perf(&perf);
                perf.record_compute(progress.elapsed);
                perf.record_update(&progress.update);
                let text = progress.text.map(|text| {
                    perf.record_emit();
                    preview_script.convert(&text.committed, &text.tentative, &languages)
                });
                perf.maybe_log();
                drop(perf);
                preview.answer(
                    progress.update.input_received_ms,
                    text.as_ref().map(|(c, t)| (c.as_str(), t.as_str())),
                );
            }
        };

        // Run the stream in the engine's worker process. Feeds are queued
        // without waiting; a crashed or hung worker makes finalize report no
        // result, and the caller falls back to batch transcription.
        // StreamOptions::default() uses CommitPolicy::Auto and lets the
        // family pick its own streaming strategy (no family-specific ext).
        // Kusema: the quicker setting for the preview, or the model's own if
        // it refuses it.
        let on_progress = Arc::new(Mutex::new(on_progress));
        let begin = |options: StreamOptions| {
            let on_progress = Arc::clone(&on_progress);
            tm.engine
                .start_stream(setup.run_options.clone(), options, move |p| {
                    (crate::preview::lock(&on_progress))(p)
                })
        };
        let started = begin(crate::preview::stream_options(model_id))
            .or_else(|_| begin(StreamOptions::default()));
        let stream = match started {
            Ok(stream) => stream,
            Err(e) => {
                error!("Failed to begin stream: {}", e);
                tm.forget_model_if_engine_dropped(model_id, &e.to_string());
                return None;
            }
        };

        tm.stream_active.store(true, Ordering::Release);
        info!(
            "Live streaming transcription started (model '{}', backend '{}')",
            model_id, info.backend
        );
        Some(Self {
            tm,
            stream,
            perf,
            throttle,
            feeder: Feeder::default(),
        })
    }

    fn feed(&mut self, pcm: Vec<f32>) {
        // Kusema: in the throttle's chunks, and not at all once it's off.
        let Some(chunk) = self.feeder.push(&self.throttle, &pcm) else {
            return;
        };
        lock_perf(&self.perf).record_feed(chunk.len());
        self.stream.feed(chunk);
    }

    /// Kusema: the recording is over and goes to a full run instead.
    fn drop_for_batch(self) {
        // What the preview cost, against the audio it covered.
        lock_perf(&self.perf).log_finalized(0);
        debug!("Live preview over; transcribing the whole recording");
        self.tm.stream_active.store(false, Ordering::Release);
    }
}
