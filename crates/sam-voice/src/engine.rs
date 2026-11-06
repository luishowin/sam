//! Push-to-talk worker.
//!
//! [`VoiceEngine::spawn`] starts one thread that owns the model and the
//! microphone. The panel talks to it with [`VoiceEngine::start`],
//! [`VoiceEngine::stop`] and [`VoiceEngine::cancel`], and reads events from
//! [`VoiceEngine::try_recv`] (or [`VoiceEngine::events`]) in a main-loop
//! timeout. No audio or inference work happens on the caller's thread.
//!
//! transcribe-rs 0.3.12 exposes a whole-clip `transcribe_raw` for the streaming
//! Moonshine model, not an incremental decoder API. Partials are therefore
//! produced by re-running the model over the accumulated utterance at a
//! throttled interval. The final result is the same model over the complete
//! clip (or the last partial when no new audio arrived).

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use transcribe_rs::onnx::moonshine::StreamingModel;
use transcribe_rs::onnx::Quantization;
use transcribe_rs::{SpeechModel, TranscribeOptions};

use crate::audio::{self, CaptureMessage, InputCapture, SAMPLE_RATE};
use crate::error::VoiceError;
use crate::model::{
    discover_model_dir, layout_description, missing_model_error, model_status, resolve_model_dir,
    ModelStatus,
};
use crate::protocol::{Command, Event};

/// Tuning knobs for the speech worker.
#[derive(Debug, Clone)]
pub struct VoiceConfig {
    /// Directory containing `moonshine-streaming/moonshine-small-streaming-en/`.
    pub models_root: PathBuf,
    /// The `voice.model` selection from the config, if one was given.
    pub model: Option<PathBuf>,
    /// ONNX Runtime intra-op threads. The guide recommends 2–4 for Moonshine.
    pub threads: usize,
    /// Minimum spacing between partial inference passes.
    pub partial_interval: Duration,
    /// Do not run a partial until this much audio has accumulated.
    pub min_partial_audio: Duration,
    /// Automatically finish an utterance at this length.
    pub max_utterance: Duration,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            models_root: crate::model::default_models_root(),
            model: None,
            threads: 4,
            partial_interval: Duration::from_millis(800),
            min_partial_audio: Duration::from_millis(400),
            max_utterance: Duration::from_secs(30),
        }
    }
}

/// A push-to-talk speech worker.
///
/// The value may be moved to the thread that owns it (it is `Send`), but it is
/// not `Sync` because it owns the event receiver. On GTK, keep it in an
/// `Rc<VoiceEngine>` and drain events from `glib::timeout_add_local`.
pub struct VoiceEngine {
    command_tx: Sender<Command>,
    event_rx: Receiver<Event>,
    worker: Option<JoinHandle<()>>,
    models_root: PathBuf,
    model: Option<PathBuf>,
}

impl VoiceEngine {
    /// Start the worker. This returns immediately; model loading happens on the
    /// worker and is reported as [`Event::ModelsLoading`] / [`Event::ModelsReady`]
    /// or [`Event::Failed`].
    pub fn spawn(config: VoiceConfig) -> io::Result<Self> {
        let models_root = config.models_root.clone();
        let model = config.model.clone();
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();

        let worker = thread::Builder::new()
            .name("sam-voice-worker".into())
            .spawn(move || worker_main(config, command_rx, event_tx))?;

        Ok(Self {
            command_tx,
            event_rx,
            worker: Some(worker),
            models_root,
            model,
        })
    }

    /// Start with [`VoiceConfig::default`] (`~/.local/share/sam/models`).
    pub fn spawn_default() -> io::Result<Self> {
        Self::spawn(VoiceConfig::default())
    }

    /// The models root this worker uses.
    pub fn models_root(&self) -> &Path {
        &self.models_root
    }

    /// Check the installed model files without blocking on model loading.
    pub fn model_status(&self) -> ModelStatus {
        let dir = match self.model.as_deref() {
            Some(selection) => resolve_model_dir(selection),
            None => discover_model_dir(&self.models_root),
        };
        model_status(&dir)
    }

    /// Ask the worker to open the microphone. Never blocks.
    pub fn start(&self) -> Result<(), VoiceError> {
        self.send(Command::Start)
    }

    /// Ask the worker to finish the utterance and emit [`Event::Final`].
    /// Never blocks.
    pub fn stop(&self) -> Result<(), VoiceError> {
        self.send(Command::Stop)
    }

    /// Ask the worker to discard the utterance. No final event is emitted.
    /// Never blocks.
    pub fn cancel(&self) -> Result<(), VoiceError> {
        self.send(Command::Cancel)
    }

    /// Take the next event if one is ready. This is the GTK-friendly poll call.
    pub fn try_recv(&self) -> Option<Event> {
        match self.event_rx.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
        }
    }

    /// Drain all currently available events, in order.
    pub fn drain_events(&self) -> Vec<Event> {
        self.events().collect()
    }

    /// Iterate over events that are ready right now.
    pub fn events(&self) -> impl Iterator<Item = Event> + '_ {
        std::iter::from_fn(|| self.try_recv())
    }

    /// Block for at most `timeout`. Intended for CLI/tests, not the GTK thread.
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Event, RecvTimeoutError> {
        self.event_rx.recv_timeout(timeout)
    }

    /// Send `Shutdown` and wait for the worker to exit.
    ///
    /// This can block for the duration of an in-flight inference pass. Prefer
    /// dropping the engine at application shutdown; dropping only requests
    /// shutdown and does not join.
    pub fn shutdown(&mut self) {
        let _ = self.command_tx.send(Command::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    fn send(&self, command: Command) -> Result<(), VoiceError> {
        self.command_tx
            .send(command)
            .map_err(|_| VoiceError::WorkerStopped)
    }
}

impl Drop for VoiceEngine {
    fn drop(&mut self) {
        let _ = self.command_tx.send(Command::Shutdown);
    }
}

struct ListeningState {
    capture: InputCapture,
    raw: Vec<f32>,
    sample_rate: u32,
    last_partial_samples: usize,
    last_partial_text: String,
    last_partial_ran: bool,
    last_partial_at: Instant,
    started_at: Instant,
}

fn worker_main(config: VoiceConfig, command_rx: Receiver<Command>, events: Sender<Event>) {
    if events.send(Event::ModelsLoading).is_err() {
        return;
    }

    let model_dir = match config.model.as_deref() {
        Some(selection) => resolve_model_dir(selection),
        None => discover_model_dir(&config.models_root),
    };
    let layout_root = config.model.as_deref().unwrap_or(&config.models_root);
    let status = model_status(&model_dir);
    if !status.is_present() {
        let error = missing_model_error(&status);
        log::error!("{error}");
        let _ = events.send(Event::Failed(format!(
            "{error}\n\n{}",
            layout_description(layout_root)
        )));
        return;
    }

    let load_start = Instant::now();
    let mut model = match StreamingModel::load(
        &model_dir,
        config.threads.max(1),
        &Quantization::default(),
    ) {
        Ok(model) => model,
        Err(error) => {
            let message = format!(
                "could not load Moonshine from {}: {error}",
                model_dir.display()
            );
            log::error!("{message}");
            let _ = events.send(Event::Failed(message));
            return;
        }
    };
    log::info!(
        "loaded Moonshine Small Streaming from {} in {:?}",
        model_dir.display(),
        load_start.elapsed()
    );

    // Warm the sessions so the first real utterance does not pay one-time
    // setup costs. A failed warm-up is not fatal.
    let warmup = vec![0.0f32; SAMPLE_RATE as usize / 2];
    let warm_start = Instant::now();
    match model.transcribe_raw(&warmup, &TranscribeOptions::default()) {
        Ok(_) => log::debug!("model warm-up took {:?}", warm_start.elapsed()),
        Err(error) => log::warn!("model warm-up failed (continuing): {error}"),
    }

    if events.send(Event::ModelsReady).is_err() {
        return;
    }

    run_loop(&mut model, &config, &command_rx, &events);
}

fn run_loop(
    model: &mut StreamingModel,
    config: &VoiceConfig,
    command_rx: &Receiver<Command>,
    events: &Sender<Event>,
) {
    let mut listening: Option<ListeningState> = None;

    loop {
        let first = command_rx.recv_timeout(Duration::from_millis(20));
        let mut commands = Vec::new();
        match first {
            Ok(command) => commands.push(command),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        while let Ok(command) = command_rx.try_recv() {
            commands.push(command);
        }

        let mut shutdown = false;
        for command in commands {
            match command {
                Command::Start => {
                    if listening.is_none() {
                        listening = start_listening(events);
                    }
                }
                Command::Stop => {
                    if let Some(state) = listening.take() {
                        finalize(state, model, config, events);
                    }
                }
                Command::Cancel => {
                    if let Some(state) = listening.take() {
                        log::debug!(
                            "cancelled after {:.2}s of audio",
                            state.raw.len() as f64 / state.sample_rate.max(1) as f64
                        );
                    }
                }
                Command::Shutdown => {
                    shutdown = true;
                    break;
                }
            }
        }
        if shutdown {
            drop(listening);
            break;
        }

        let auto_stop = match listening.as_mut() {
            Some(state) => match drain_audio(state, config.max_utterance) {
                Ok(auto_stop) => auto_stop,
                Err(error) => {
                    let message = error.to_string();
                    log::error!("{message}");
                    let _ = events.send(Event::Failed(message));
                    listening = None;
                    continue;
                }
            },
            None => false,
        };

        if auto_stop {
            if let Some(state) = listening.take() {
                log::info!(
                    "maximum utterance length reached ({:.1}s); finishing",
                    config.max_utterance.as_secs_f64()
                );
                finalize(state, model, config, events);
            }
            continue;
        }

        if let Some(state) = listening.as_mut() {
            if let Err(error) = maybe_partial(state, model, config, events) {
                let message = error.to_string();
                log::error!("{message}");
                let _ = events.send(Event::Failed(message));
                listening = None;
            }
        }
    }
}

fn start_listening(events: &Sender<Event>) -> Option<ListeningState> {
    match audio::start_default_input() {
        Ok(capture) => {
            log::info!(
                "capture started on {} ({} Hz, {} ch)",
                capture.device,
                capture.sample_rate,
                capture.channels
            );
            let sample_rate = capture.sample_rate;
            let now = Instant::now();
            let _ = events.send(Event::Listening);
            Some(ListeningState {
                capture,
                raw: Vec::new(),
                sample_rate,
                last_partial_samples: 0,
                last_partial_text: String::new(),
                last_partial_ran: false,
                last_partial_at: now,
                started_at: now,
            })
        }
        Err(error) => {
            log::error!("{error}");
            let _ = events.send(Event::Failed(error.to_string()));
            None
        }
    }
}

/// Returns `Ok(true)` when the utterance hit the configured maximum.
fn drain_audio(state: &mut ListeningState, max_utterance: Duration) -> Result<bool, VoiceError> {
    loop {
        match state.capture.rx.try_recv() {
            Ok(CaptureMessage::Samples(chunk)) => {
                if state.raw.is_empty() {
                    log::debug!(
                        "first audio sample arrived {:?} after Listening",
                        state.started_at.elapsed()
                    );
                }
                if chunk.sample_rate != state.sample_rate {
                    state.sample_rate = chunk.sample_rate;
                }
                state.raw.extend_from_slice(&chunk.samples);
                let max_samples =
                    (max_utterance.as_secs_f64() * state.sample_rate as f64) as usize;
                if state.raw.len() >= max_samples {
                    return Ok(true);
                }
            }
            Ok(CaptureMessage::Error(message)) => {
                return Err(VoiceError::Microphone(message));
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => return Ok(false),
        }
    }
}

fn maybe_partial(
    state: &mut ListeningState,
    model: &mut StreamingModel,
    config: &VoiceConfig,
    events: &Sender<Event>,
) -> Result<(), VoiceError> {
    let rate = state.sample_rate as f64;
    let min_samples = (config.min_partial_audio.as_secs_f64() * rate) as usize;
    let interval_samples = (config.partial_interval.as_secs_f64() * rate) as usize;

    if state.raw.len() < min_samples {
        return Ok(());
    }
    if state.raw.len().saturating_sub(state.last_partial_samples) < interval_samples {
        return Ok(());
    }
    if state.last_partial_at.elapsed() < config.partial_interval {
        return Ok(());
    }

    let resample_start = Instant::now();
    let samples = audio::resample_to_16k(&state.raw, state.sample_rate)?;
    let resample_time = resample_start.elapsed();
    let audio_seconds = samples.len() as f64 / SAMPLE_RATE as f64;

    let inference_start = Instant::now();
    let text = transcribe(model, &samples)?;
    let inference_time = inference_start.elapsed();

    state.last_partial_samples = state.raw.len();
    state.last_partial_text = text.clone();
    state.last_partial_ran = true;
    state.last_partial_at = Instant::now();

    log::debug!(
        "partial at {:?}: {audio_seconds:.2}s audio, resample {resample_time:?}, inference {inference_time:?}, text={text:?}",
        state.started_at.elapsed()
    );

    if !text.is_empty() {
        let _ = events.send(Event::Partial(text));
    }
    Ok(())
}

fn finalize(
    mut state: ListeningState,
    model: &mut StreamingModel,
    config: &VoiceConfig,
    events: &Sender<Event>,
) {
    state.capture.stop();

    // Drain callbacks that had already queued before the device was closed.
    loop {
        match state.capture.rx.try_recv() {
            Ok(CaptureMessage::Samples(chunk)) => {
                if state.raw.is_empty() {
                    log::debug!(
                        "first audio sample arrived {:?} after Listening",
                        state.started_at.elapsed()
                    );
                }
                if chunk.sample_rate != state.sample_rate {
                    state.sample_rate = chunk.sample_rate;
                }
                state.raw.extend_from_slice(&chunk.samples);
            }
            Ok(CaptureMessage::Error(message)) => {
                log::warn!("microphone error while stopping: {message}");
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
        }
    }

    let max_samples = (config.max_utterance.as_secs_f64() * state.sample_rate as f64) as usize;
    if state.raw.len() > max_samples {
        state.raw.truncate(max_samples);
    }

    let clip_seconds = state.raw.len() as f64 / state.sample_rate.max(1) as f64;
    log::debug!(
        "finishing {clip_seconds:.2}s utterance after {:?}",
        state.started_at.elapsed()
    );

    // A handful of samples is not a word. Avoid handing ONNX Runtime a
    // degenerate clip when the user taps the mic and releases immediately.
    let min_final_samples = (state.sample_rate as f64 * 0.05) as usize;
    if state.raw.len() < min_final_samples {
        let _ = events.send(Event::Final(String::new()));
        return;
    }

    // If a partial already covered exactly this audio, reuse its text. This is
    // the common short-query case and saves a full inference pass.
    if state.last_partial_ran && state.last_partial_samples == state.raw.len() {
        log::debug!("final: reusing last partial (no new audio)");
        let _ = events.send(Event::Final(state.last_partial_text));
        return;
    }

    let resample_start = Instant::now();
    let samples = match audio::resample_to_16k(&state.raw, state.sample_rate) {
        Ok(samples) => samples,
        Err(error) => {
            let _ = events.send(Event::Failed(error.to_string()));
            return;
        }
    };
    let resample_time = resample_start.elapsed();
    let audio_seconds = samples.len() as f64 / SAMPLE_RATE as f64;

    let inference_start = Instant::now();
    let text = match transcribe(model, &samples) {
        Ok(text) => text,
        Err(error) => {
            let _ = events.send(Event::Failed(error.to_string()));
            return;
        }
    };
    log::debug!(
        "final: {audio_seconds:.2}s audio, resample {resample_time:?}, inference {:?}, text={text:?}",
        inference_start.elapsed()
    );

    let _ = events.send(Event::Final(text));
}

fn transcribe(model: &mut StreamingModel, samples: &[f32]) -> Result<String, VoiceError> {
    model
        .transcribe_raw(samples, &TranscribeOptions::default())
        .map(|result| result.text.trim().to_string())
        .map_err(|error| VoiceError::Inference(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn assert_send<T: Send>() {}

    #[test]
    fn engine_is_send_so_it_can_be_created_before_the_ui_loop() {
        assert_send::<VoiceEngine>();
    }

    #[test]
    fn missing_model_reports_loading_then_failed() {
        let temp = TempDir::new("engine-missing");
        let engine = VoiceEngine::spawn(VoiceConfig {
            models_root: temp.path().to_path_buf(),
            ..VoiceConfig::default()
        })
        .unwrap();

        assert_eq!(
            engine.recv_timeout(Duration::from_secs(5)).unwrap(),
            Event::ModelsLoading
        );
        match engine.recv_timeout(Duration::from_secs(5)).unwrap() {
            Event::Failed(message) => {
                assert!(message.contains("missing"), "{message}");
                assert!(message.contains("sam-voice setup"), "{message}");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        assert!(engine.model_status().missing().len() > 0);
    }

    #[test]
    fn commands_after_worker_exit_return_an_error_instead_of_panicking() {
        let temp = TempDir::new("engine-stopped");
        let engine = VoiceEngine::spawn(VoiceConfig {
            models_root: temp.path().to_path_buf(),
            ..VoiceConfig::default()
        })
        .unwrap();

        // Drain the model failure, then wait for the worker to drop the command
        // receiver. This is the state the panel is in after a Failed event from
        // a worker that cannot start at all.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            while engine.try_recv().is_some() {}
            match engine.start() {
                Err(VoiceError::WorkerStopped) => break,
                Err(other) => panic!("unexpected error: {other}"),
                Ok(()) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Ok(()) => panic!("worker did not stop within 5s"),
            }
        }
    }
}
