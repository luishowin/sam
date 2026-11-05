//! Phrasings → intents (§6). The rules live in `data/intents.toml`, compiled
//! into the binary; this module compiles their patterns and matches them
//! against normalised words.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::action::{PLACES, Registry};
use crate::intent::{Intent, Source, Value};
use crate::text::FILLERS;

const CORE_GRAMMAR: &str = include_str!("../../../data/intents.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    /// One or more words, to the end of the input.
    Rest,
    /// A place word; captures the place ID.
    Place,
    /// 0–100, optionally followed by `%`.
    Level,
    /// A colour temperature in kelvin, 1700–6500.
    Kelvin,
    /// on / off / toggle and their synonyms.
    OnOff,
}

#[derive(Debug, Clone, PartialEq)]
enum Element {
    Word(String),
    Choice {
        options: Vec<Vec<String>>,
        optional: bool,
    },
    Slot {
        name: String,
        kind: SlotKind,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    source: String,
    elements: Vec<Element>,
}

fn place_id(word: &str) -> Option<&'static str> {
    let id = match word {
        "home" | "~" => "home",
        "desktop" => "desktop",
        "documents" | "docs" => "documents",
        "downloads" | "download" => "downloads",
        "music" => "music",
        "pictures" | "pics" | "photos" => "pictures",
        "videos" | "movies" => "videos",
        "templates" => "templates",
        "public" => "public",
        "trash" | "bin" | "wastebasket" => "trash",
        "recent" | "recents" => "recent",
        "screenshots" => "screenshots",
        _ => return None,
    };
    debug_assert!(PLACES.contains(&id));
    Some(id)
}

fn on_off(word: &str) -> Option<&'static str> {
    Some(match word {
        "on" | "enable" | "enabled" | "activate" | "start" => "on",
        "off" | "disable" | "disabled" | "deactivate" | "stop" => "off",
        "toggle" | "switch" => "toggle",
        _ => return None,
    })
}

fn level(word: &str) -> Option<f64> {
    let n: f64 = word.strip_suffix('%').unwrap_or(word).parse().ok()?;
    (n.is_finite() && (0.0..=100.0).contains(&n)).then_some(n)
}

fn kelvin(word: &str) -> Option<f64> {
    let n: f64 = word.strip_suffix('k').unwrap_or(word).parse().ok()?;
    (n.is_finite() && (1700.0..=6500.0).contains(&n)).then_some(n)
}

impl Pattern {
    pub fn compile(source: &str) -> Result<Pattern, String> {
        let err = |msg: &str| format!("pattern {source:?}: {msg}");
        let chars: Vec<char> = source.chars().collect();
        let mut elements = Vec::new();
        let mut i = 0;
        let check_word = |w: &str| -> Result<String, String> {
            if w != w.to_lowercase() {
                return Err(err(&format!("{w:?} must be lowercase")));
            }
            if FILLERS.contains(&w) {
                return Err(err(&format!(
                    "{w:?} is a filler word; the input never contains it"
                )));
            }
            Ok(w.to_owned())
        };
        while i < chars.len() {
            let c = chars[i];
            if c.is_whitespace() {
                i += 1;
            } else if c == '(' || c == '[' {
                let close = if c == '(' { ')' } else { ']' };
                let end = chars[i..]
                    .iter()
                    .position(|&d| d == close)
                    .ok_or_else(|| err("unclosed group"))?
                    + i;
                let inner: String = chars[i + 1..end].iter().collect();
                if inner.contains(['(', '[', '{']) {
                    return Err(err("groups do not nest"));
                }
                let mut options = Vec::new();
                for alt in inner.split('|') {
                    let words = alt
                        .split_whitespace()
                        .map(check_word)
                        .collect::<Result<Vec<_>, _>>()?;
                    if words.is_empty() {
                        return Err(err("empty alternative"));
                    }
                    options.push(words);
                }
                elements.push(Element::Choice {
                    options,
                    optional: c == '[',
                });
                i = end + 1;
            } else if c == '{' {
                let end = chars[i..]
                    .iter()
                    .position(|&d| d == '}')
                    .ok_or_else(|| err("unclosed slot"))?
                    + i;
                let inner: String = chars[i + 1..end].iter().collect();
                let (name, kind) = inner.split_once(':').unwrap_or((&inner, "rest"));
                let kind = match kind {
                    "rest" => SlotKind::Rest,
                    "place" => SlotKind::Place,
                    "level" => SlotKind::Level,
                    "kelvin" => SlotKind::Kelvin,
                    "on_off" => SlotKind::OnOff,
                    other => return Err(err(&format!("unknown slot type {other:?}"))),
                };
                elements.push(Element::Slot {
                    name: name.trim().to_owned(),
                    kind,
                });
                i = end + 1;
            } else if matches!(c, ')' | ']' | '}' | '|') {
                return Err(err(&format!("unexpected {c:?}")));
            } else {
                let end = chars[i..]
                    .iter()
                    .position(|d| {
                        d.is_whitespace() || matches!(d, '(' | ')' | '[' | ']' | '{' | '}' | '|')
                    })
                    .map_or(chars.len(), |p| p + i);
                let word: String = chars[i..end].iter().collect();
                elements.push(Element::Word(check_word(&word)?));
                i = end;
            }
        }
        if elements.is_empty() {
            return Err(err("is empty"));
        }
        let rest_positions: Vec<usize> = elements
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                matches!(
                    e,
                    Element::Slot {
                        kind: SlotKind::Rest,
                        ..
                    }
                )
            })
            .map(|(i, _)| i)
            .collect();
        if rest_positions.iter().any(|&p| p + 1 != elements.len()) {
            return Err(err("a rest slot must come last"));
        }
        Ok(Pattern {
            source: source.to_owned(),
            elements,
        })
    }

    /// Captured slots if the pattern matches all of `words`.
    pub fn matches(&self, words: &[String]) -> Option<BTreeMap<String, Value>> {
        let mut captures = BTreeMap::new();
        self.match_from(0, words, &mut captures).then_some(captures)
    }

    fn match_from(
        &self,
        at: usize,
        words: &[String],
        captures: &mut BTreeMap<String, Value>,
    ) -> bool {
        let Some(element) = self.elements.get(at) else {
            return words.is_empty();
        };
        let starts_with = |seq: &[String]| words.len() >= seq.len() && words[..seq.len()] == *seq;
        match element {
            Element::Word(w) => {
                words.first() == Some(w) && self.match_from(at + 1, &words[1..], captures)
            }
            Element::Choice { options, optional } => {
                for option in options {
                    if starts_with(option)
                        && self.match_from(at + 1, &words[option.len()..], captures)
                    {
                        return true;
                    }
                }
                *optional && self.match_from(at + 1, words, captures)
            }
            Element::Slot { name, kind } => {
                let value = match kind {
                    SlotKind::Rest => {
                        if words.is_empty() {
                            return false;
                        }
                        captures.insert(name.clone(), Value::Text(words.join(" ")));
                        return true;
                    }
                    SlotKind::Place => words.first().and_then(|w| place_id(w)).map(Value::from),
                    SlotKind::Level => words.first().and_then(|w| level(w)).map(Value::Number),
                    SlotKind::Kelvin => words.first().and_then(|w| kelvin(w)).map(Value::Number),
                    SlotKind::OnOff => words.first().and_then(|w| on_off(w)).map(Value::from),
                };
                let Some(value) = value else { return false };
                captures.insert(name.clone(), value);
                if self.match_from(at + 1, &words[1..], captures) {
                    return true;
                }
                captures.remove(name);
                false
            }
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// "open …": things that open are preferred.
    Open,
    /// "find …": files and projects are searched and preferred.
    Find,
}

/// A command the grammar knows, as the panel will show it.
#[derive(Debug, Clone)]
pub struct Command {
    pub action: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: String,
    pub keywords: Vec<String>,
    pub args: BTreeMap<String, Value>,
    pub patterns: Vec<Pattern>,
}

impl Command {
    pub fn intent(&self, captures: BTreeMap<String, Value>) -> Intent {
        let mut intent = Intent::new(self.action.clone(), Source::Grammar);
        intent.args = self.args.clone();
        intent.args.extend(captures);
        intent
    }
}

#[derive(Debug, Clone)]
struct ModeRule {
    mode: Mode,
    patterns: Vec<Pattern>,
}

#[derive(Debug)]
pub struct Grammar {
    commands: Vec<Command>,
    modes: Vec<ModeRule>,
}

/// What the grammar made of a whole input.
#[derive(Debug, Clone)]
pub enum Hit<'g> {
    Command {
        command: &'g Command,
        intent: Intent,
    },
    Mode {
        mode: Mode,
        rest: Vec<String>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    intent: Vec<IntentEntry>,
    #[serde(default)]
    mode: Vec<ModeEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntentEntry {
    action: String,
    title: String,
    subtitle: Option<String>,
    icon: String,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    args: BTreeMap<String, toml::Value>,
    patterns: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModeEntry {
    mode: String,
    patterns: Vec<String>,
}

impl Grammar {
    /// The rules shipped in `data/intents.toml`.
    pub fn core() -> Grammar {
        Grammar::parse(CORE_GRAMMAR).expect("data/intents.toml is valid (tested)")
    }

    pub fn parse(source: &str) -> Result<Grammar, String> {
        let file: File = toml::from_str(source).map_err(|e| e.to_string())?;
        let mut commands = Vec::new();
        for entry in file.intent {
            let mut args = BTreeMap::new();
            for (k, v) in entry.args {
                let value = match v {
                    toml::Value::String(s) => Value::Text(s),
                    toml::Value::Integer(i) => Value::Number(i as f64),
                    toml::Value::Float(f) => Value::Number(f),
                    other => {
                        return Err(format!(
                            "{}: arg {k} has unsupported type {}",
                            entry.action,
                            other.type_str()
                        ));
                    }
                };
                args.insert(k, value);
            }
            let patterns = entry
                .patterns
                .iter()
                .map(|p| Pattern::compile(p))
                .collect::<Result<_, _>>()?;
            commands.push(Command {
                action: entry.action,
                title: entry.title,
                subtitle: entry.subtitle,
                icon: entry.icon,
                keywords: entry.keywords,
                args,
                patterns,
            });
        }
        let mut modes = Vec::new();
        for entry in file.mode {
            let mode = match entry.mode.as_str() {
                "open" => Mode::Open,
                "find" => Mode::Find,
                other => return Err(format!("unknown mode {other:?}")),
            };
            let patterns = entry
                .patterns
                .iter()
                .map(|p| Pattern::compile(p))
                .collect::<Result<Vec<Pattern>, _>>()?;
            for p in &patterns {
                let ends_in_rest = matches!(
                    p.elements.last(),
                    Some(Element::Slot {
                        kind: SlotKind::Rest,
                        ..
                    })
                );
                if !ends_in_rest {
                    return Err(format!(
                        "mode pattern {:?} must end in a rest slot",
                        p.source
                    ));
                }
            }
            modes.push(ModeRule { mode, patterns });
        }
        Ok(Grammar { commands, modes })
    }

    /// Every rule's intent must be one the registry accepts. Checked by
    /// tests for the shipped file, and by anyone loading another.
    pub fn check(&self, registry: &Registry) -> Result<(), String> {
        for c in &self.commands {
            let spec = registry
                .get(&c.action)
                .ok_or_else(|| format!("{:?}: no such action", c.title))?;
            for p in &c.patterns {
                let mut probe = c.args.clone();
                for e in &p.elements {
                    if let Element::Slot { name, kind } = e {
                        let sample = match kind {
                            SlotKind::Rest => Value::from("sample"),
                            SlotKind::Place => Value::from("downloads"),
                            SlotKind::Level => Value::Number(50.0),
                            SlotKind::Kelvin => Value::Number(3000.0),
                            SlotKind::OnOff => Value::from("on"),
                        };
                        probe.insert(name.clone(), sample);
                    }
                }
                let mut intent = Intent::new(spec.id, Source::Grammar);
                intent.args = probe;
                registry
                    .validate(intent)
                    .map_err(|e| format!("{:?} / {:?}: {e}", c.title, p.source))?;
            }
        }
        Ok(())
    }

    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    /// The first rule matching all of `words`: commands before modes, in file
    /// order.
    pub fn hit(&self, words: &[String]) -> Option<Hit<'_>> {
        if words.is_empty() {
            return None;
        }
        for command in &self.commands {
            for pattern in &command.patterns {
                if let Some(captures) = pattern.matches(words) {
                    return Some(Hit::Command {
                        command,
                        intent: command.intent(captures),
                    });
                }
            }
        }
        for rule in &self.modes {
            for pattern in &rule.patterns {
                if let Some(mut captures) = pattern.matches(words) {
                    let rest = match captures.remove("rest") {
                        Some(Value::Text(t)) => t.split(' ').map(str::to_owned).collect::<Vec<_>>(),
                        _ => continue,
                    };
                    return Some(Hit::Mode {
                        mode: rule.mode,
                        rest,
                    });
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::normalize;

    fn hit_of(g: &Grammar, input: &str) -> String {
        match g.hit(&normalize(input)) {
            Some(Hit::Command { intent, .. }) => intent.to_string(),
            Some(Hit::Mode { mode, rest }) => format!("{mode:?}: {}", rest.join(" ")),
            None => "-".into(),
        }
    }

    #[test]
    fn shipped_grammar_is_valid() {
        let g = Grammar::core();
        g.check(&Registry::core()).unwrap();
        assert!(g.commands().len() > 20);
    }

    #[test]
    fn the_plan_phrases() {
        let g = Grammar::core();
        for (input, want) in [
            ("lock my computer", "system.lock"),
            ("Lock", "system.lock"),
            ("lock the screen", "system.lock"),
            ("turn off bluetooth", r#"bluetooth.set {state: "off"}"#),
            ("Bluetooth on", r#"bluetooth.set {state: "on"}"#),
            ("disable wifi", r#"wifi.set {state: "off"}"#),
            ("turn wi-fi on", r#"wifi.set {state: "on"}"#),
            ("show me today's calendar", "calendar.open"),
            ("what's on today", "calendar.open"),
            ("open my downloads", r#"place.open {place: "downloads"}"#),
            ("downloads", r#"place.open {place: "downloads"}"#),
            ("go to the trash", r#"place.open {place: "trash"}"#),
            ("volume up", r#"audio.volume {direction: "up"}"#),
            (
                "turn the volume down",
                r#"audio.volume {direction: "down"}"#,
            ),
            ("set volume to 30%", "audio.volume_set {level: 30}"),
            ("mute", r#"audio.mute {state: "on"}"#),
            ("pause music", r#"media.control {command: "pause"}"#),
            ("next song", r#"media.control {command: "next"}"#),
            ("dark mode on", r#"appearance.dark {state: "on"}"#),
            ("turn on dark mode", r#"appearance.dark {state: "on"}"#),
            ("do not disturb", r#"notifications.dnd {state: "toggle"}"#),
            ("log out", "session.logout"),
            ("restart", "session.restart"),
            ("shut down", "session.power_off"),
            ("turn off the computer", "session.power_off"),
            ("suspend", "system.suspend"),
            ("go to sleep", "system.suspend"),
            ("find the PDF about invoices", "Find: pdf about invoices"),
            ("find PhotoDesk", "Find: photodesk"),
            (
                "search for photodesk screenshots",
                "Find: photodesk screenshots",
            ),
            (
                "where is the photodesk repository",
                "Find: photodesk repository",
            ),
            ("open calculator", "Open: calculator"),
            ("launch chrome", "Open: chrome"),
        ] {
            assert_eq!(hit_of(&g, input), want, "{input}");
        }
    }

    #[test]
    fn leaves_other_text_alone() {
        let g = Grammar::core();
        for input in [
            "chrome",
            "photodesk",
            "17 * 42",
            "2.5 gb to mb",
            "vsc",
            "bluetooth",
            "lock screen please now x",
        ] {
            let got = hit_of(&g, input);
            assert!(got == "-" || input == "bluetooth", "{input} → {got}");
        }
    }

    #[test]
    fn pattern_syntax() {
        let p = Pattern::compile("lock [screen|this computer] (now|at once)").unwrap();
        let w = |s: &str| s.split(' ').map(str::to_owned).collect::<Vec<_>>();
        assert!(p.matches(&w("lock now")).is_some());
        assert!(p.matches(&w("lock screen at once")).is_some());
        assert!(p.matches(&w("lock this computer now")).is_some());
        assert!(p.matches(&w("lock")).is_none());
        assert!(p.matches(&w("lock now please")).is_none());

        let v = Pattern::compile("set volume [to] {level:level}").unwrap();
        assert_eq!(v.matches(&w("set volume 101%")), None);
        assert_eq!(
            v.matches(&w("set volume to 5%")).unwrap()["level"],
            Value::Number(5.0)
        );

        for bad in [
            "",
            "(a",
            "a)",
            "[a|]",
            "{x:colour}",
            "{rest} after",
            "(a (b))",
            "Lock",
            "open the door",
        ] {
            assert!(Pattern::compile(bad).is_err(), "{bad:?} should not compile");
        }
    }
}
