//! Local, offline English speech-to-text for Sam.
//!
//! The crate has three layers:
//!
//! * [`VoiceEngine`] — a push-to-talk worker thread with a channel of
//!   [`Event`]s. This is what the panel should use.
//! * [`read_wav_16k_mono`] — a small WAV spike used by the `sam-voice`
//!   binary and tests.
//! * [`model`] and [`setup`] — model discovery, verification and the one-time
//!   download.
//!
//! Only Moonshine Small Streaming English is used in this build. The model is
//! loaded once, on a worker thread, and kept warm for the life of the process.
//! Runtime code never touches the network.
//!
//! # Panel integration
//!
//! ```text
//! let engine = Rc::new(VoiceEngine::spawn_default()?);
//!
//! // Called from a glib::timeout_add_local every ~50 ms:
//! for event in engine.events() {
//!     match event {
//!         Event::ModelsLoading => show_loading(),
//!         Event::ModelsReady   => enable_mic(),
//!         Event::Listening     => show_listening(),
//!         Event::Partial(text) => set_entry_text(&text),
//!         Event::Final(text)   => { set_entry_text(&text); focus_entry(); }
//!         Event::Failed(text)  => { show_error(&text); go_idle(); }
//!     }
//! }
//!
//! // Push-to-talk press / release:
//! engine.start()?;
//! engine.stop()?;
//! // Escape:
//! engine.cancel()?;
//! ```
//!
//! `VoiceEngine` is `Send` (create it before the UI loop) but not `Sync`
//! (it owns the receiver), so keep it in an `Rc` on the GTK main thread.
//! `start`, `stop` and `cancel` only enqueue a command and never block.

mod audio;
mod engine;
mod error;
pub mod model;
pub mod protocol;
pub mod setup;
#[cfg(test)]
mod testutil;
pub mod wav;

pub use audio::{resample_to_16k, SAMPLE_RATE};
pub use engine::{VoiceConfig, VoiceEngine};
pub use error::VoiceError;
pub use model::{
    canonical_model_dir, default_models_root, discover_model_dir, layout_description, model_status,
    model_status_for_root, resolve_model_dir, ModelStatus,
};
pub use protocol::{Command, Event};
pub use setup::{download_and_install, install_archive, SetupOutcome};
pub use wav::{read_wav_16k_mono, WavAudio};
