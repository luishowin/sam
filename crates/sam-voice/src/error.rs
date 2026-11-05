//! Error type for `sam-voice`.
//!
//! Every failure is a value. The library never panics on missing models,
//! missing microphones, malformed audio, or inference errors.

use std::error::Error;
use std::fmt;
use std::path::PathBuf;

/// Something the speech pipeline could not do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceError {
    /// The ONNX model files are not installed.
    ModelsMissing {
        /// The model directory that was checked.
        dir: PathBuf,
        /// The required files that are absent or empty.
        missing: Vec<String>,
    },
    /// The model files exist but ONNX Runtime could not load them.
    ModelLoad(String),
    /// Inference failed after the model loaded.
    Inference(String),
    /// The microphone could not be opened or reported a stream error.
    Microphone(String),
    /// A resampling operation failed.
    Resample(String),
    /// A WAV file could not be read or converted.
    Wav(String),
    /// `sam-voice setup` failed.
    Setup(String),
    /// The worker thread has already exited.
    WorkerStopped,
}

impl VoiceError {
    /// The actionable setup hint shown when models are missing.
    pub fn setup_hint() -> &'static str {
        "install the model with `sam-voice setup` (network is only used during setup)"
    }
}

impl fmt::Display for VoiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VoiceError::ModelsMissing { dir, missing } => {
                write!(
                    f,
                    "Moonshine Small Streaming model files are missing in {}: {}; {}",
                    dir.display(),
                    missing.join(", "),
                    VoiceError::setup_hint()
                )
            }
            VoiceError::ModelLoad(message) => write!(f, "could not load the speech model: {message}"),
            VoiceError::Inference(message) => write!(f, "speech inference failed: {message}"),
            VoiceError::Microphone(message) => write!(f, "microphone unavailable: {message}"),
            VoiceError::Resample(message) => write!(f, "audio resampling failed: {message}"),
            VoiceError::Wav(message) => write!(f, "could not read WAV audio: {message}"),
            VoiceError::Setup(message) => write!(f, "model setup failed: {message}"),
            VoiceError::WorkerStopped => write!(f, "the speech worker is no longer running"),
        }
    }
}

impl Error for VoiceError {}
