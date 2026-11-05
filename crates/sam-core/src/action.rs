//! The actions Core can take, what each accepts, and how risky it is (§7).
//! [`Registry::validate`] is the only way to turn an [`Intent`] into a
//! [`Validated`] one, which is what [`policy`](crate::policy) needs.

use std::fmt;

use crate::intent::{Intent, Value};

/// How much could go wrong (0.0.6 permissions; the model proposes, the
/// runtime decides — model output can only ever become an `Intent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Permission {
    /// Looks, never changes anything.
    Read,
    /// Changes something trivially undoable, or that asks for itself
    /// (a GNOME dialog, a new terminal window kept open).
    LowRisk,
    /// Runs something the runtime cannot take back: a shell command, a file
    /// operation. Ambiguous proposals confirm; an explicit typed gesture runs.
    Execute,
    /// Destroys user data. Always confirms below L4. No Core action holds
    /// this yet; it exists so the table cannot silently admit one.
    Destructive,
    /// Acts on the machine itself (suspend, reboot). Always confirms below L4.
    Privileged,
}

/// Whether the runtime asks before running, beyond what `Permission` says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    /// Never asks (permission still applies: Read always runs, the rest need
    /// a user pick at Core's level).
    Never,
    /// Asks when the proposal is ambiguous: model-sourced, low confidence,
    /// or destructive-looking arguments. A fully visible typed gesture runs.
    WhenAmbiguous,
    /// Always asks below L4, however the intent arose.
    Always,
}

/// Who carries an action out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handler {
    /// The OS adapter's executor.
    Os,
    /// The panel itself (clipboard, switching into find mode).
    Panel,
    /// The executor, through the network door (F5); what comes back is
    /// shown in the panel (§15).
    Net,
    /// The executor, through the microphone door (F30); the transcript
    /// comes back to the panel (§16).
    Mic,
}

/// What an argument must look like.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// Non-empty text, at most 4 KiB, no NUL.
    Text,
    /// Non-empty text, at most 64 KiB, no NUL: an answer, or a whole
    /// conversation.
    LongText,
    /// An absolute URI whose scheme is one of these.
    Uri(&'static [&'static str]),
    /// A desktop-entry ID, `something.desktop`.
    AppId,
    /// A finite number in `min..=max`.
    Number { min: f64, max: f64 },
    /// One of these words.
    OneOf(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Param {
    pub name: &'static str,
    pub kind: Kind,
    pub required: bool,
}

const fn required(name: &'static str, kind: Kind) -> Param {
    Param {
        name,
        kind,
        required: true,
    }
}

const fn optional(name: &'static str, kind: Kind) -> Param {
    Param {
        name,
        kind,
        required: false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActionSpec {
    pub id: &'static str,
    pub title: &'static str,
    /// One line for humans and for capability retrieval (§13): what this
    /// does, in plain words. Shown in `--explain`, fed to the action model
    /// only as part of a small pre-filtered candidate set, never the whole
    /// catalogue at once.
    pub description: &'static str,
    pub params: &'static [Param],
    pub permission: Permission,
    pub confirmation: Confirmation,
    pub handler: Handler,
}

const FILE_SCHEMES: &[&str] = &["file", "trash", "recent"];
const WEB_SCHEMES: &[&str] = &["http", "https"];
pub const STATES: &[&str] = &["on", "off", "toggle"];
pub const PLACES: &[&str] = &[
    "home",
    "desktop",
    "documents",
    "downloads",
    "music",
    "pictures",
    "videos",
    "templates",
    "public",
    "trash",
    "recent",
    "screenshots",
];
pub const MEDIA_COMMANDS: &[&str] = &["play_pause", "play", "pause", "next", "previous", "stop"];

const fn spec(
    id: &'static str,
    title: &'static str,
    description: &'static str,
    params: &'static [Param],
    permission: Permission,
    confirmation: Confirmation,
    handler: Handler,
) -> ActionSpec {
    ActionSpec {
        id,
        title,
        description,
        params,
        permission,
        confirmation,
        handler,
    }
}

use Confirmation::{Always, Never};
use Handler::{Os, Panel};
use Permission::{Destructive, LowRisk, Privileged, Read};

/// Every action Core knows (§7). Questions, weather and listening are
/// LowRisk, not Read: at L1 a Read action runs as the user types, and
/// neither may text leave the machine (§15) nor the microphone open (§16)
/// unless the user picked it.
pub const CORE_ACTIONS: &[ActionSpec] = &[
    spec(
        "app.launch",
        "Open application",
        "Open an installed application.",
        &[required("app_id", Kind::AppId)],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "file.open",
        "Open",
        "Open a file with its default application.",
        &[required("uri", Kind::Uri(FILE_SCHEMES))],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "file.reveal",
        "Show in Files",
        "Show a file in the Files browser.",
        &[required("uri", Kind::Uri(&["file"]))],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "place.open",
        "Open folder",
        "Open a standard folder such as Downloads or Documents.",
        &[required("place", Kind::OneOf(PLACES))],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "doc.open",
        "Open documentation",
        "Open an installed manual page or local document.",
        &[required("page", Kind::Text)],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "url.open",
        "Open link",
        "Open a web link in the default browser.",
        &[required("url", Kind::Uri(WEB_SCHEMES))],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "calendar.open",
        "Open Calendar",
        "Open the GNOME Calendar application.",
        &[],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "system.lock",
        "Lock screen",
        "Lock the screen immediately.",
        &[],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "system.suspend",
        "Suspend",
        "Suspend the machine to RAM.",
        &[],
        Privileged,
        Always,
        Os,
    ),
    // GNOME's end-session dialog asks for these itself.
    spec(
        "session.logout",
        "Log out…",
        "Log out; GNOME asks to confirm.",
        &[],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "session.restart",
        "Restart…",
        "Restart; GNOME asks to confirm.",
        &[],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "session.power_off",
        "Power off…",
        "Power off; GNOME asks to confirm.",
        &[],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "bluetooth.set",
        "Bluetooth",
        "Switch Bluetooth on, off or toggle it.",
        &[required("state", Kind::OneOf(STATES))],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "wifi.set",
        "Wi-Fi",
        "Switch Wi-Fi on, off or toggle it.",
        &[required("state", Kind::OneOf(STATES))],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "audio.volume",
        "Volume",
        "Turn the volume up or down a step.",
        &[required("direction", Kind::OneOf(&["up", "down"]))],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "audio.volume_set",
        "Set volume",
        "Set the volume to a level from 0 to 100.",
        &[required(
            "level",
            Kind::Number {
                min: 0.0,
                max: 100.0,
            },
        )],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "audio.mute",
        "Mute",
        "Mute or unmute the microphone and speaker.",
        &[required("state", Kind::OneOf(STATES))],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "media.control",
        "Media",
        "Control the currently playing media.",
        &[required("command", Kind::OneOf(MEDIA_COMMANDS))],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "appearance.dark",
        "Dark style",
        "Switch the dark style on, off or toggle it.",
        &[required("state", Kind::OneOf(STATES))],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "display.night_light",
        "Night light",
        "Switch the night light on, off or toggle it.",
        &[required("state", Kind::OneOf(STATES))],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "display.night_light_temperature",
        "Night light warmth",
        "Set the night light colour temperature in kelvin.",
        &[required(
            "temperature",
            Kind::Number {
                min: 1700.0,
                max: 6500.0,
            },
        )],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "system.airplane",
        "Airplane mode",
        "Switch airplane mode on, off or toggle it.",
        &[required("state", Kind::OneOf(STATES))],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "power.profile",
        "Power profile",
        "Choose the balanced, performance or power-saver profile.",
        &[required(
            "profile",
            Kind::OneOf(&["balanced", "performance", "power-saver"]),
        )],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "media.seek",
        "Seek",
        "Seek the current media player forward or back by seconds.",
        &[
            required("direction", Kind::OneOf(&["forward", "back"])),
            required(
                "seconds",
                Kind::Number {
                    min: 1.0,
                    max: 60.0,
                },
            ),
        ],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "bluetooth.connect",
        "Bluetooth device",
        "Connect or disconnect a paired Bluetooth device by name.",
        &[
            required("action", Kind::OneOf(&["connect", "disconnect"])),
            required("device", Kind::Text),
        ],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "terminal.open",
        "Terminal",
        "Open the configured terminal.",
        &[],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "timer.set",
        "Timer",
        "Set a timer for a duration such as 10m or 1h30m.",
        &[required("duration", Kind::Text), optional("label", Kind::Text)],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "timer.cancel",
        "Cancel timer",
        "Cancel a timer by its label or number.",
        &[optional("id", Kind::Text)],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "timer.list",
        "Timers",
        "List the running timers.",
        &[],
        Read,
        Never,
        Panel,
    ),
    spec(
        "alarm.set",
        "Alarm",
        "Set an alarm for a time such as 07:30.",
        &[required("time", Kind::Text), optional("label", Kind::Text)],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "alarm.cancel",
        "Cancel alarm",
        "Cancel an alarm by its label or number.",
        &[optional("id", Kind::Text)],
        LowRisk,
        Never,
        Os,
    ),
    spec(
        "alarm.list",
        "Alarms",
        "List the scheduled alarms.",
        &[],
        Read,
        Never,
        Panel,
    ),
    spec(
        "notifications.dnd",
        "Do Not Disturb",
        "Switch Do Not Disturb on, off or toggle it.",
        &[required("state", Kind::OneOf(STATES))],
        LowRisk,
        Never,
        Os,
    ),
    // 0.0.46: Execute/WhenAmbiguous, not Confirm. A leading `>` is a
    // run-dialog gesture: the command is fully visible in the entry, the
    // user typed the `>` themselves, and it runs in a new Ptyxis window
    // that stays open (Luis). A model-proposed command is ambiguous by
    // definition and confirms. Spoken commands still wait out the
    // countdown (F32).
    spec(
        "clipboard.copy",
        "Copy",
        "Copy text to the clipboard.",
        &[required("text", Kind::LongText)],
        LowRisk,
        Never,
        Panel,
    ),
    spec(
        "info.copy",
        "Copy answer",
        "Copy a live system answer to the clipboard.",
        &[required("title", Kind::Text), required("text", Kind::LongText)],
        Read,
        Never,
        Panel,
    ),
    spec(
        "search.files",
        "Search files",
        "Search indexed files by name and content.",
        &[required("query", Kind::Text)],
        Read,
        Never,
        Panel,
    ),
    // `/help` answers locally: static text, no provider, no network.
    spec(
        "help.show",
        "Help",
        "Show what SAM can do.",
        &[],
        Read,
        Never,
        Panel,
    ),
    // OpenCode only ever fills a prompt in, and the user sends it (F25), so
    // Resume runs at once.
    // Listening takes no arguments: what is heard comes back, and nothing
    // about what to listen for goes in (§16, F30).
];

/// The [`Destructive`] permission exists so the table cannot silently admit
/// a data-destroying action: nothing holds it yet, and this fails loudly if
/// that ever changes without a policy test to match.
#[allow(dead_code)]
fn destructive_is_unused(specs: &[ActionSpec]) -> bool {
    !specs.iter().any(|s| s.permission == Destructive)
}

/// An intent the registry has checked. Only [`Registry::validate`] makes one.
#[derive(Debug)]
pub struct Validated {
    intent: Intent,
    spec: ActionSpec,
}

impl Validated {
    pub fn intent(&self) -> &Intent {
        &self.intent
    }

    pub fn spec(&self) -> &ActionSpec {
        &self.spec
    }
}

/// Why an intent was refused.
#[derive(Debug, Clone, PartialEq)]
pub enum Rejection {
    UnknownAction(String),
    MissingArg {
        action: String,
        arg: &'static str,
    },
    UnexpectedArg {
        action: String,
        arg: String,
    },
    BadArg {
        action: String,
        arg: String,
        reason: String,
    },
}

impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rejection::UnknownAction(a) => write!(f, "no action called {a:?}"),
            Rejection::MissingArg { action, arg } => write!(f, "{action} needs {arg}"),
            Rejection::UnexpectedArg { action, arg } => write!(f, "{action} takes no {arg}"),
            Rejection::BadArg {
                action,
                arg,
                reason,
            } => write!(f, "{action}: {arg} {reason}"),
        }
    }
}

impl std::error::Error for Rejection {}

pub struct Registry {
    specs: &'static [ActionSpec],
}

impl Registry {
    pub fn core() -> Self {
        Registry {
            specs: CORE_ACTIONS,
        }
    }

    pub fn get(&self, id: &str) -> Option<&'static ActionSpec> {
        self.specs.iter().find(|s| s.id == id)
    }

    pub fn specs(&self) -> &'static [ActionSpec] {
        self.specs
    }

    /// Machine-readable catalogue for the action model (§13): one
    /// `id — description` line per capability. The caller sends only a
    /// pre-filtered subset (see [`candidates_for`](Registry::candidates_for)),
    /// never the whole catalogue with every request.
    pub fn describe(&self) -> Vec<String> {
        self.specs
            .iter()
            .map(|s| format!("{} — {}", s.id, s.description))
            .collect()
    }

    /// Deterministic capability retrieval (§13): keep the specs sharing a
    /// word with the request text (matched over id, title and description),
    /// most overlap first. A tiny keyword filter, deliberately — no
    /// embeddings, no model call. Returns at most `limit`, possibly empty.
    pub fn candidates_for(&self, text: &str, limit: usize) -> Vec<&'static ActionSpec> {
        let words: Vec<String> = text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 2)
            .map(|w| w.to_ascii_lowercase())
            .collect();
        if words.is_empty() || limit == 0 {
            return Vec::new();
        }
        let mut scored: Vec<(usize, &'static ActionSpec)> = self
            .specs
            .iter()
            .map(|s| {
                let haystack = format!("{} {} {}", s.id, s.title, s.description).to_ascii_lowercase();
                let overlap = words.iter().filter(|w| haystack.contains(w.as_str())).count();
                (overlap, s)
            })
            .filter(|(overlap, _)| *overlap > 0)
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.id.cmp(b.1.id)));
        scored.into_iter().take(limit).map(|(_, s)| s).collect()
    }

    pub fn validate(&self, intent: Intent) -> Result<Validated, Rejection> {
        let spec = *self
            .get(&intent.action)
            .ok_or_else(|| Rejection::UnknownAction(intent.action.clone()))?;
        for (name, value) in &intent.args {
            let Some(param) = spec.params.iter().find(|p| p.name == name) else {
                return Err(Rejection::UnexpectedArg {
                    action: spec.id.into(),
                    arg: name.clone(),
                });
            };
            check(param.kind, value).map_err(|reason| Rejection::BadArg {
                action: spec.id.into(),
                arg: name.clone(),
                reason,
            })?;
        }
        if let Some(missing) = spec
            .params
            .iter()
            .find(|p| p.required && !intent.args.contains_key(p.name))
        {
            return Err(Rejection::MissingArg {
                action: spec.id.into(),
                arg: missing.name,
            });
        }
        Ok(Validated { intent, spec })
    }
}

fn check_text(t: &str, max: usize, limit: &str) -> Result<(), String> {
    if t.trim().is_empty() {
        return Err("is empty".into());
    }
    if t.len() > max {
        return Err(format!("is longer than {limit}"));
    }
    if t.contains('\0') {
        return Err("contains NUL".into());
    }
    Ok(())
}

fn check(kind: Kind, value: &Value) -> Result<(), String> {
    let text = || value.as_text().ok_or_else(|| "must be text".to_owned());
    match kind {
        Kind::Text => check_text(text()?, 4096, "4 KiB"),
        Kind::LongText => check_text(text()?, 64 * 1024, "64 KiB"),
        Kind::Uri(schemes) => {
            let t = text()?;
            let scheme = uri_scheme(t).ok_or_else(|| format!("is not an absolute URI: {t:?}"))?;
            if !schemes.iter().any(|s| s.eq_ignore_ascii_case(scheme)) {
                return Err(format!("has scheme {scheme:?}, not one of {schemes:?}"));
            }
            if t.chars().any(|c| c.is_whitespace() || c.is_control()) {
                return Err("contains spaces or control characters (it must be escaped)".into());
            }
            Ok(())
        }
        Kind::AppId => {
            let t = text()?;
            let stem = t.strip_suffix(".desktop").ok_or("must end in .desktop")?;
            let ok = !stem.is_empty()
                && stem
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
            if ok {
                Ok(())
            } else {
                Err(format!("is not a desktop-entry ID: {t:?}"))
            }
        }
        Kind::Number { min, max } => {
            let n = value.as_number().ok_or("must be a number")?;
            if n.is_finite() && (min..=max).contains(&n) {
                Ok(())
            } else {
                Err(format!("must be between {min} and {max}"))
            }
        }
        Kind::OneOf(words) => {
            let t = text()?;
            if words.contains(&t) {
                Ok(())
            } else {
                Err(format!("must be one of {words:?}"))
            }
        }
    }
}

/// `file:///x` → `file`. RFC 3986: a letter, then letters, digits, `+-.`.
fn uri_scheme(s: &str) -> Option<&str> {
    let (scheme, rest) = s.split_once(':')?;
    let mut chars = scheme.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
    let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    (first_ok && rest_ok && !rest.is_empty()).then_some(scheme)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::Source;

    fn intent(action: &str) -> Intent {
        Intent::new(action, Source::Provider)
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = CORE_ACTIONS.iter().map(|s| s.id).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before);
    }

    #[test]
    fn accepts_well_formed_intents() {
        let r = Registry::core();
        for good in [
            intent("app.launch").arg("app_id", "org.gnome.Calculator.desktop"),
            intent("app.launch").arg("app_id", "google-chrome.desktop"),
            intent("file.open").arg("uri", "file:///home/luis/Downloads"),
            intent("file.open").arg("uri", "trash:///"),
            intent("url.open").arg("url", "https://example.org/a%20b"),
            intent("place.open").arg("place", "downloads"),
            intent("system.lock"),
            intent("bluetooth.set").arg("state", "off"),
            intent("audio.volume_set").arg("level", 35.0),
            intent("clipboard.copy").arg("text", "714"),
            intent("clipboard.copy").arg("text", "a".repeat(20_000)),
        ] {
            let shown = good.to_string();
            assert!(r.validate(good).is_ok(), "{shown}");
        }
    }

    #[test]
    fn refuses_malformed_intents() {
        let r = Registry::core();
        let cases = [
            (intent("rm.rf"), "no action"),
            (intent("app.launch"), "needs app_id"),
            (intent("app.launch").arg("app_id", "chrome"), "desktop"),
            (
                intent("app.launch").arg("app_id", "../evil.desktop"),
                "desktop-entry ID",
            ),
            (intent("system.lock").arg("now", "yes"), "takes no now"),
            (
                intent("file.open").arg("uri", "/etc/passwd"),
                "absolute URI",
            ),
            (
                intent("file.open").arg("uri", "https://example.org"),
                "scheme",
            ),
            (
                intent("url.open").arg("url", "file:///etc/passwd"),
                "scheme",
            ),
            (
                intent("url.open").arg("url", "javascript:alert(1)"),
                "scheme",
            ),
            (intent("file.open").arg("uri", "file:///a b"), "spaces"),
            (intent("place.open").arg("place", "/etc"), "one of"),
            (intent("bluetooth.set").arg("state", "maybe"), "one of"),
            (intent("audio.volume_set").arg("level", 150.0), "between"),
            (intent("audio.volume_set").arg("level", f64::NAN), "between"),
            (intent("audio.volume_set").arg("level", "loud"), "number"),
            (intent("clipboard.copy").arg("text", "   "), "empty"),
        ];
        for (bad, why) in cases {
            let shown = bad.to_string();
            let err = r.validate(bad).expect_err(&shown).to_string();
            assert!(err.contains(why), "{shown}: {err:?} should mention {why:?}");
        }
    }

    #[test]
    fn permissions_are_as_specified() {
        let r = Registry::core();
        let perm = |id| r.get(id).unwrap().permission;
        let conf = |id| r.get(id).unwrap().confirmation;
        assert_eq!(perm("system.suspend"), Permission::Privileged);
        assert_eq!(conf("system.suspend"), Confirmation::Always);
        assert_eq!(perm("help.show"), Permission::Read);
        assert_eq!(perm("search.files"), Permission::Read);
        assert_eq!(perm("app.launch"), Permission::LowRisk);
        assert_eq!(r.get("clipboard.copy").unwrap().handler, Handler::Panel);
        assert_eq!(r.get("help.show").unwrap().handler, Handler::Panel);
        // No capability destroys data without the table knowing it.
        assert!(destructive_is_unused(r.specs()));
        // Every capability describes itself for retrieval (§13).
        for s in r.specs() {
            assert!(!s.description.is_empty(), "{} has no description", s.id);
        }
    }

    #[test]
    fn retrieval_surfs_relevant_capabilities() {
        let r = Registry::core();
        let ids: Vec<_> = r
            .candidates_for("open that PDF file", 3)
            .iter()
            .map(|s| s.id)
            .collect();
        assert!(ids.contains(&"file.open"), "{ids:?}");
        assert!(r.candidates_for("!!", 3).is_empty());
        assert!(r.candidates_for("open file", 0).is_empty());
        // The catalogue is machine-readable.
        assert_eq!(r.describe().len(), r.specs().len());
    }
}
