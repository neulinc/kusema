//! Kusema live preview in continuous dictation: a stream on the loaded model
//! whose text only goes to the pill. Each segment is still transcribed in one
//! run when it ends, so the stream is never the typed text.

use std::sync::{Arc, Mutex};

use log::{debug, error};
use transcribe_cpp::StreamOptions;

use super::{PreviewScript, TranscriptionManager};
use crate::engine_supervisor::{StreamHandle, StreamProgress};
use crate::preview::throttle::Throttle;
use crate::preview::Progress;
use crate::settings::get_settings;

impl TranscriptionManager {
    /// Begins a preview stream, or `None` when the loaded model has none (or
    /// isn't loaded). Waits only for the engine's queue.
    pub fn start_preview_stream(&self, throttle: Arc<Mutex<Throttle>>) -> Option<StreamHandle> {
        let info = self.engine.loaded()?;
        if !info.capabilities.supports_streaming {
            debug!("Live preview: the loaded model has no stream");
            return None;
        }
        let settings = get_settings(&self.app_handle);
        let model_id = self.get_current_model().unwrap_or_default();
        let plan = self.language_plan(&settings, &model_id, &info);
        let setup = self.setup_for(&info, &settings, &plan, &[]);
        let output_language = setup.output_language(&settings, None);
        let languages = setup.languages.clone();
        let mut script = PreviewScript::new(settings.chinese_script, &output_language);
        let mut progress = Progress::new(&self.app_handle, throttle);
        let on_progress = move |p: StreamProgress| {
            let text = p
                .text
                .map(|text| script.convert(&text.committed, &text.tentative, &languages));
            progress.answer(
                p.update.input_received_ms,
                text.as_ref().map(|(c, t)| (c.as_str(), t.as_str())),
            );
        };
        let progress = Arc::new(Mutex::new(on_progress));
        let begin = |options: StreamOptions| {
            let progress = Arc::clone(&progress);
            self.engine
                .start_stream(setup.run_options.clone(), options, move |p| {
                    (crate::preview::lock(&progress))(p)
                })
        };
        // The quicker setting, or the model's own if it refuses it.
        let stream = begin(crate::preview::stream_options(&model_id))
            .or_else(|_| begin(StreamOptions::default()));
        match stream {
            Ok(stream) => Some(stream),
            Err(e) => {
                error!("Live preview: couldn't begin a stream: {e}");
                None
            }
        }
    }
}
