//! The message protocol between the panel and the speech worker.
//!
//! Commands travel from the UI thread to the worker; events travel back. Both
//! are plain values, so the panel can consume them from a GTK main-loop timeout
//! without touching the audio or inference threads.

/// Messages from the panel to the speech worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Open the default microphone and begin listening.
    Start,
    /// Stop listening, flush the model, and emit [`Event::Final`].
    Stop,
    /// Stop listening and throw the audio away. No final event is emitted.
    Cancel,
    /// Drop the model and exit the worker thread.
    Shutdown,
}

/// Messages from the speech worker to the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The worker thread started and is loading the model.
    ModelsLoading,
    /// The model is loaded and warm; listening may start.
    ModelsReady,
    /// The microphone is open and audio is being captured.
    Listening,
    /// Live text for the utterance so far. Replaces the previous partial.
    Partial(String),
    /// The final text for the utterance. Empty when nothing was recognised.
    Final(String),
    /// A recoverable failure. The panel should return to idle and keep typing.
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn protocol_values_are_thread_safe() {
        assert_send_sync::<Command>();
        assert_send_sync::<Event>();
    }

    #[test]
    fn events_compare_by_value() {
        assert_eq!(Event::Partial("hello".into()), Event::Partial("hello".into()));
        assert_ne!(Event::Partial("hello".into()), Event::Final("hello".into()));
        assert_ne!(Event::ModelsReady, Event::Listening);
    }
}
