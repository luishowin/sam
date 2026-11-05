//! What can go wrong in a way the user should hear about, and what Sam says
//! about it (§14, F21). The panel shows only these lines. The exact cause
//! goes to the journal, through [`redact`] first, so that no target or
//! argument outlives the process (F6).

use crate::intent::Intent;

/// A service on the other side of the network door (F5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    /// The answers provider (F29).
    Fireworks,
    /// Weather and place names (F28).
    OpenMeteo,
    /// The local action model over loopback (F37). No key, no account:
    /// failures here are the daemon or the model, never credentials.
    Ollama,
    /// The speech model download (`sam voice setup`, F34): huggingface.co
    /// and the CDN host it redirects to, and nothing else there.
    HuggingFace,
    /// The local transcription server over loopback (Spike F): the helper
    /// POSTs captured audio to it instead of running whisper itself.
    Phonon,
}

impl Service {
    pub fn name(self) -> &'static str {
        match self {
            Service::Fireworks => "Fireworks",
            Service::OpenMeteo => "Open-Meteo",
            Service::Ollama => "Ollama",
            Service::HuggingFace => "HuggingFace",
            Service::Phonon => "Phonon",
        }
    }
}

/// Something that went wrong.
#[derive(Debug, Clone, PartialEq)]
pub enum Trouble {
    /// The executor could not carry an action out. `what` says what, as a
    /// verb phrase from [`what`].
    ActionFailed { what: String },
    /// The registry refused an intent.
    Rejected,
    /// The policy does not allow it at this level.
    Denied,
    /// LocalSearch did not answer, so there are no file rows.
    FilesUnavailable,
    /// A bug: something SAM should have handled did not arrive where it
    /// should have.
    Internal,
    /// AI mode is off, so nothing may leave the machine (F5, F26).
    AnswersOff,
    /// Nothing answered, or not in time.
    Unreachable { service: Service },
    /// Something answered, but not with anything SAM could use.
    ServiceFailed { service: Service },
    /// The provider turned the key down.
    KeyRefused,
    /// The model declined to answer (a `refusal` stop, or a content filter).
    Declined,
    /// No place is configured for the weather (F28).
    NoPlace,
    /// The place named for the weather was not found.
    PlaceNotFound,
    /// There is no microphone to listen with (§16).
    NoMicrophone,
    /// There is a microphone, but it is muted. Nothing distinguishes that
    /// from a silent room by listening, so it is asked about (SPIKE-E).
    MicrophoneMuted,
    /// No speech model is installed (F34).
    NoSpeechModel,
    /// Listening worked and there were no words in it.
    NotHeard,
}

impl Trouble {
    /// What Sam says, in the first person.
    pub fn line(&self) -> String {
        match self {
            Trouble::ActionFailed { what } => {
                format!("I couldn't {what} — the details are in my log.")
            }
            Trouble::Rejected => {
                "That isn't something I know how to do — the details are in my log.".into()
            }
            Trouble::Denied => "I'm not allowed to do that from here.".into(),
            Trouble::FilesUnavailable => "I can't search your files right now.".into(),
            Trouble::Internal => {
                "Something went wrong on my side — the details are in my log.".into()
            }
            Trouble::AnswersOff => {
                "I can't answer while AI mode is off — Ctrl+Space turns it on.".into()
            }
            Trouble::Unreachable { service } => format!(
                "I couldn't reach {} — the details are in my log.",
                service.name()
            ),
            Trouble::ServiceFailed { service } => format!(
                "I didn't get an answer from {} — the details are in my log.",
                service.name()
            ),
            Trouble::KeyRefused => {
                "Fireworks turned my key down — `sam answers setup` sets a new one.".into()
            }
            Trouble::Declined => "I can't help with that one.".into(),
            Trouble::NoPlace => {
                "I don't know where you are — `sam weather setup PLACE` tells me.".into()
            }
            Trouble::PlaceNotFound => "I couldn't find that place.".into(),
            Trouble::NoMicrophone => "I can't find a microphone to listen with.".into(),
            Trouble::MicrophoneMuted => {
                "I can't hear anything — your microphone is muted.".into()
            }
            Trouble::NoSpeechModel => {
                "I haven't got a speech model yet — `sam voice setup` gets me one.".into()
            }
            Trouble::NotHeard => "I didn't catch that.".into(),
        }
    }

    /// A short, stable name for the journal's `IDA_TROUBLE` field.
    pub fn code(&self) -> &'static str {
        match self {
            Trouble::ActionFailed { .. } => "action-failed",
            Trouble::Rejected => "rejected",
            Trouble::Denied => "denied",
            Trouble::FilesUnavailable => "files-unavailable",
            Trouble::Internal => "internal",
            Trouble::AnswersOff => "answers-off",
            Trouble::Unreachable { .. } => "unreachable",
            Trouble::ServiceFailed { .. } => "service-failed",
            Trouble::KeyRefused => "key-refused",
            Trouble::Declined => "declined",
            Trouble::NoPlace => "no-place",
            Trouble::PlaceNotFound => "place-not-found",
            Trouble::NoMicrophone => "no-microphone",
            Trouble::MicrophoneMuted => "microphone-muted",
            Trouble::NoSpeechModel => "no-speech-model",
            Trouble::NotHeard => "not-heard",
        }
    }
}

/// What an action does, as the end of "I couldn't …". Only the words the
/// registry allows (on, off, up, …) are read from the arguments; names,
/// paths and typed text never are.
pub fn what(intent: &Intent) -> String {
    let arg = |name: &str| intent.text(name).unwrap_or_default();
    let switch = |thing: &str| match arg("state") {
        "on" => format!("turn {thing} on"),
        "off" => format!("turn {thing} off"),
        _ => format!("toggle {thing}"),
    };
    let phrase = match intent.action.as_str() {
        "app.launch" => "open that app",
        "file.open" => "open that file",
        "file.reveal" => "show that in Files",
        "place.open" => "open that folder",
        "url.open" => "open that link",
        "doc.open" => "open that documentation",
        "calendar.open" => "open Calendar",
        "system.lock" => "lock the screen",
        "system.suspend" => "suspend the computer",
        "session.logout" => "log you out",
        "session.restart" => "restart the computer",
        "session.power_off" => "power the computer off",
        "bluetooth.set" => return switch("Bluetooth"),
        "wifi.set" => return switch("Wi-Fi"),
        "appearance.dark" => return switch("dark style"),
        "notifications.dnd" => return switch("Do Not Disturb"),
        "display.night_light" => return switch("night light"),
        "display.night_light_temperature" => "set the night light warmth",
        "system.airplane" => return switch("airplane mode"),
        "power.profile" => "change the power profile",
        "media.seek" => "seek the current track",
        "bluetooth.connect" => match arg("action") {
            "disconnect" => "disconnect that Bluetooth device",
            _ => "connect that Bluetooth device",
        },
        "terminal.open" => "open a terminal",
        "timer.set" => "set a timer",
        "timer.cancel" => "cancel a timer",
        "timer.list" => "list the timers",
        "alarm.set" => "set an alarm",
        "alarm.cancel" => "cancel an alarm",
        "alarm.list" => "list the alarms",
        "info.copy" => "copy that answer",
        "audio.volume" if arg("direction") == "up" => "turn the volume up",
        "audio.volume" => "turn the volume down",
        "audio.volume_set" => "set the volume",
        "audio.mute" => match arg("state") {
            "on" => "mute the sound",
            "off" => "unmute the sound",
            _ => "toggle mute",
        },
        "media.control" => match arg("command") {
            "play" => "play the music",
            "pause" => "pause the music",
            "next" => "skip to the next track",
            "previous" => "go back a track",
            "stop" => "stop the music",
            _ => "play or pause the music",
        },
        "shell.run" => "run that command",
        "clipboard.copy" => "copy that",
        "search.files" => "search your files",
        "help.show" => "show the help",
        "ask.question" => "ask that",
        "weather.show" => "get the weather",
        "conversation.resume" => "open that in OpenCode",
        "voice.listen" => "listen",
        _ => "do that",
    };
    phrase.to_owned()
}

/// Where a path or a link in a message starts.
const TARGETS: &[&str] = &[
    "/home/",
    "/run/media/",
    "/media/",
    "/mnt/",
    "/tmp/",
    "/var/home/",
    "~/",
    "file://",
    "trash://",
    "recent://",
    "http://",
    "https://",
    "sftp://",
    "smb://",
    "ftp://",
];

/// `detail` with anything that could name what the user picked replaced by
/// `…`. That covers quoted strings (GLib quotes names “like this”, and Rust's
/// `{:?}` "like this"), paths under home or removable media, and links. The
/// cause itself stays readable.
pub fn redact(detail: &str) -> String {
    let mut out = String::with_capacity(detail.len());
    let mut closing: Option<char> = None;
    for c in detail.chars() {
        if let Some(close) = closing {
            if c == close {
                out.push('…');
                out.push(c);
                closing = None;
            }
            continue;
        }
        let close = match c {
            '“' => Some('”'),
            '‘' => Some('’'),
            '"' => Some('"'),
            _ => None,
        };
        out.push(c);
        closing = close;
    }
    if closing.is_some() {
        // An unclosed quote: whatever followed it is hidden too.
        out.push('…');
    }
    out.split(' ')
        .map(|word| {
            let bare = word.trim_start_matches(['(', '[', '<', '\'']);
            if TARGETS.iter().any(|t| bare.starts_with(t)) {
                let lead = &word[..word.len() - bare.len()];
                let tail: String = bare
                    .chars()
                    .rev()
                    .take_while(|c| matches!(c, ')' | ']' | '>' | ',' | ':' | ';' | '.' | '\''))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                format!("{lead}…{tail}")
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Registry;
    use crate::intent::Source;

    #[test]
    fn every_trouble_speaks_as_ida() {
        let all = [
            Trouble::ActionFailed {
                what: "turn Wi-Fi off".into(),
            },
            Trouble::Rejected,
            Trouble::Denied,
            Trouble::FilesUnavailable,
            Trouble::Internal,
            Trouble::AnswersOff,
            Trouble::Unreachable {
                service: Service::Fireworks,
            },
            Trouble::ServiceFailed {
                service: Service::OpenMeteo,
            },
            Trouble::KeyRefused,
            Trouble::Declined,
            Trouble::NoPlace,
            Trouble::PlaceNotFound,
            Trouble::NoMicrophone,
            Trouble::MicrophoneMuted,
            Trouble::NoSpeechModel,
            Trouble::NotHeard,
        ];
        let mut codes: Vec<_> = all.iter().map(Trouble::code).collect();
        for trouble in &all {
            let line = trouble.line();
            let first_person = line
                .split(|c: char| !c.is_alphanumeric() && c != '\'')
                .any(|w| matches!(w, "I" | "I'm" | "I've" | "me" | "my" | "we" | "our" | "us"));
            assert!(first_person, "{line:?} is not in Sam's first person");
            assert!(line.ends_with('.'), "{line:?}");
        }
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), all.len(), "codes are unique");
        assert_eq!(
            all[0].line(),
            "I couldn't turn Wi-Fi off — the details are in my log."
        );
    }

    #[test]
    fn every_action_has_words_and_none_leak_arguments() {
        for spec in Registry::core().specs() {
            let intent = Intent::new(spec.id, Source::Grammar)
                .arg("app_id", "secret-app.desktop")
                .arg("uri", "file:///home/luis/secret.pdf")
                .arg("command", "rm secret")
                .arg("text", "secret")
                .arg("question", "secret question")
                .arg("place", "secret town")
                .arg("prompt", "secret prompt");
            let words = what(&intent);
            assert_ne!(words, "do that", "{} has no words", spec.id);
            assert!(!words.contains("secret"), "{}: {words}", spec.id);
        }
        let wifi = Intent::new("wifi.set", Source::Grammar).arg("state", "off");
        assert_eq!(what(&wifi), "turn Wi-Fi off");
        let skip = Intent::new("media.control", Source::Grammar).arg("command", "next");
        assert_eq!(what(&skip), "skip to the next track");
        let ask = Intent::new("ask.question", Source::Grammar).arg("question", "x");
        assert_eq!(
            Trouble::ActionFailed { what: what(&ask) }.line(),
            "I couldn't ask that — the details are in my log."
        );
    }

    #[test]
    fn network_troubles_name_the_service_and_nothing_else() {
        assert_eq!(
            Trouble::Unreachable {
                service: Service::OpenMeteo
            }
            .line(),
            "I couldn't reach Open-Meteo — the details are in my log."
        );
        assert_eq!(
            Trouble::ServiceFailed {
                service: Service::Fireworks
            }
            .code(),
            "service-failed"
        );
    }

    #[test]
    fn redaction_keeps_the_cause_and_hides_the_target() {
        for (detail, expected) in [
            (
                "Failed to execute child process “evince” (No such file or directory)",
                "Failed to execute child process “…” (No such file or directory)",
            ),
            (
                "Error opening file /home/luis/Documents/invoice.pdf: Permission denied",
                "Error opening file …: Permission denied",
            ),
            ("unknown state \"maybe\"", "unknown state \"…\""),
            (
                "could not open (file:///home/luis/a%20b.txt)",
                "could not open (…)",
            ),
            (
                "GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown: The name org.freedesktop.FileManager1 was not provided by any .service files",
                "GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown: The name org.freedesktop.FileManager1 was not provided by any .service files",
            ),
            ("unterminated “secret", "unterminated “…"),
            (
                "Could not open file: permission denied",
                "Could not open file: permission denied",
            ),
            ("", ""),
        ] {
            assert_eq!(redact(detail), expected, "{detail}");
        }
    }
}
