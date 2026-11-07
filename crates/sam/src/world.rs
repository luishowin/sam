//! The engine and every provider, built once and shared by the panel and the
//! CLI.

use std::sync::Arc;

use sam_core::action::ActionSpec;
use sam_core::engine::{Engine, Provider, Results};
use sam_core::intent::{Intent, Source, Value};
use sam_core::rank::{Candidate, Kind};
use sam_linux::apps::AppIndex;
use sam_linux::config::Config;
use sam_linux::files::FileIndex;
use sam_linux::host::LinuxHost;
use sam_linux::info::{Settings, SystemInfo};
use sam_linux::paths::PathProvider;
use sam_linux::projects::ProjectIndex;

#[derive(Debug)]
pub enum NeedleReply {
    Candidate(Candidate),
    Clarify(String),
    No,
}

pub struct World {
    pub engine: Engine,
    pub apps: AppIndex,
    pub projects: ProjectIndex,
    /// Shared with the threads that run file searches.
    pub files: Arc<FileIndex>,
    pub paths: PathProvider,
    pub info: SystemInfo,
    pub settings: Settings,
    needle: crate::needle::Needle,
}

impl World {
    pub fn new() -> World {
        let config = Config::load();
        let projects = ProjectIndex::new(config.project_roots(), config.projects.depth);
        let world = World {
            engine: Engine::new(),
            apps: AppIndex::new(),
            files: Arc::new(FileIndex::new(projects.clone())),
            projects,
            paths: PathProvider,
            info: SystemInfo,
            settings: Settings,
            needle: crate::needle::Needle::new(),
        };
        world.projects.refresh_if_stale();
        world
    }

    /// The synchronous half of a query: everything but files.
    pub fn search(&self, text: &str) -> Results {
        let providers: [&dyn Provider; 5] =
            [&self.apps, &self.projects, &self.paths, &self.info, &self.settings];
        self.engine.search(text, &LinuxHost, &providers)
    }

    /// What the panel lists.
    pub fn panel_rows(&self, results: &Results, limit: usize) -> Vec<Candidate> {
        self.engine.rank(results, limit)
    }

    /// Ask the local Needle sidecar for one proposal. Called on submission,
    /// not on every keystroke: a model miss must not make typing wait.
    pub fn needle_reply(&self, text: &str) -> NeedleReply {
        let needle = &self.needle;
        let text = stt_cleanup(text);
        let specs = self.needle_tools(&text);
        if specs.len() < 3 {
            return NeedleReply::No;
        }
        let tools = sam_core::schema::tools_json_for(&specs, true);
        let Some(call) = needle.propose(
            &text,
            &tools,
            &self.engine.registry().schema_fingerprint(),
        ) else {
            return NeedleReply::No;
        };
        let reply = self.adapt_call(&call);
        reply
    }

    fn needle_tools(&self, text: &str) -> Vec<&'static ActionSpec> {
        const FALLBACK: &[&str] = &[
            "app.launch",
            "file.open",
            "place.open",
            "search.files",
            "wifi.set",
            "bluetooth.set",
            "audio.volume_set",
            "display.brightness",
            "timer.set",
            "window.minimize",
            "probe.uptime",
        ];
        let registry = self.engine.registry();
        let mut seen: std::collections::HashSet<&'static str> = Default::default();
        let mut specs: Vec<&'static ActionSpec> = Vec::new();
        // Voice spellings and domain words the lexical ranker cannot see.
        for id in needle_hints(text) {
            if specs.len() >= 7 {
                break;
            }
            if let Some(spec) = registry.get(id)
                && seen.insert(spec.id)
            {
                specs.push(spec);
            }
        }
        for spec in registry.candidates_for(text, 7) {
            if specs.len() >= 7 {
                break;
            }
            if seen.insert(spec.id) {
                specs.push(spec);
            }
        }
        for id in FALLBACK {
            if specs.len() >= 7 {
                break;
            }
            if let Some(spec) = registry.get(id)
                && seen.insert(spec.id)
            {
                specs.push(spec);
            }
        }
        specs.truncate(7);
        specs
    }

    fn adapt_call(&self, call: &crate::needle::Call) -> NeedleReply {
        let registry = self.engine.registry();
        let intent = match call.name.as_str() {
            // The pseudo-tool: a question, not an action. The caller shows
            // it; a model must not guess and fall through to a file row.
            "clarify" => {
                const SLOTS: &[&str] = &[
                    "app", "path", "query", "duration", "time", "target", "position",
                    "level", "kelvin", "state", "place", "url", "name", "slot",
                ];
                let slot = text_arg(call, "slot").unwrap_or_else(|| "argument".into());
                let slot = if SLOTS.contains(&slot.as_str()) {
                    slot
                } else {
                    "argument".into()
                };
                return NeedleReply::Clarify(slot);
            }
            "app.launch" => {
                let Some(name) = text_arg(call, "app").or_else(|| text_arg(call, "app_id"))
                else {
                    return NeedleReply::No;
                };
                let Some(id) = self.resolve_app(&name) else {
                    return NeedleReply::No;
                };
                Intent::new("app.launch", Source::Model).arg("app_id", id)
            }
            "file.open" => {
                let Some(path) = text_arg(call, "path").or_else(|| text_arg(call, "uri"))
                else {
                    return NeedleReply::No;
                };
                let Some(uri) = self.file_uri(&path) else {
                    return NeedleReply::No;
                };
                Intent::new("file.open", Source::Model).arg("uri", uri)
            }
            "display.night_light_temperature" => {
                let Some(kelvin) = number_arg(call, "kelvin")
                    .or_else(|| number_arg(call, "temperature"))
                else {
                    return NeedleReply::No;
                };
                Intent::new("display.night_light_temperature", Source::Model)
                    .arg("temperature", kelvin)
            }
            // The model catalogue has content search; the runtime currently
            // has one file search tool. Keep the query, drop the roots.
            "search.content" => {
                let Some(query) = text_arg(call, "query") else {
                    return NeedleReply::No;
                };
                Intent::new("search.files", Source::Model).arg("query", query)
            }
            name => {
                if registry.get(name).is_none() {
                    return NeedleReply::No;
                }
                let mut intent = Intent::new(name, Source::Model);
                for (key, value) in &call.arguments {
                    let value = match value {
                        serde_json::Value::String(text) => Value::Text(text.clone()),
                        serde_json::Value::Number(number) => {
                            let Some(number) = number.as_f64() else {
                                return NeedleReply::No;
                            };
                            Value::Number(number)
                        }
                        _ => return NeedleReply::No,
                    };
                    intent = intent.arg(key, value);
                }
                intent
            }
        };
        let Ok(()) = registry.validate(intent.clone()).map(|_| ()) else {
            return NeedleReply::No;
        };
        let Some(spec) = registry.get(&intent.action) else {
            return NeedleReply::No;
        };
        NeedleReply::Candidate(Candidate {
            title: spec.title.into(),
            subtitle: "Needle".into(),
            kind: Kind::Command,
            icon: "system-search-symbolic".into(),
            score: 0.45,
            key: format!("model:{intent}"),
            intent,
            alt: None,
        })
    }

    fn resolve_app(&self, name: &str) -> Option<String> {
        let wanted = name.trim().to_lowercase();
        if wanted.is_empty() {
            return None;
        }
        let entries = self.apps.entries();
        let exact: Vec<_> = entries
            .iter()
            .filter(|app| {
                app.id.eq_ignore_ascii_case(&wanted)
                    || app.name.to_lowercase() == wanted
                    || app.tail.to_lowercase() == wanted
                    || app.exec.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(&wanted))
            })
            .collect();
        if exact.len() == 1 {
            return Some(exact[0].id.clone());
        }
        if exact.len() > 1 {
            return None;
        }
        let near: Vec<_> = entries
            .iter()
            .filter(|app| {
                app.name.to_lowercase().contains(&wanted)
                    || app.tail.to_lowercase().contains(&wanted)
                    || app
                        .exec
                        .as_deref()
                        .is_some_and(|e| e.to_lowercase().contains(&wanted))
            })
            .collect();
        (near.len() == 1).then(|| near[0].id.clone())
    }

    fn file_uri(&self, path: &str) -> Option<String> {
        if path.starts_with("file://") || path.starts_with("trash://") || path.starts_with("recent://") {
            return Some(path.to_owned());
        }
        let path = sam_linux::config::expand_home(path);
        let path = if path.is_absolute() {
            path
        } else {
            glib::home_dir().join(path)
        };
        if !path.exists() {
            return None;
        }
        glib::filename_to_uri(&path, None)
            .ok()
            .map(|uri| uri.to_string())
    }
}

/// Common STT spellings the model was not trained on. This is a proposal
/// cleanup, not a claim that Sam understood the transcript.
fn stt_cleanup(text: &str) -> String {
    let mut text = text.to_lowercase();
    for (from, to) in [
        ("why fi", "wifi"),
        ("wi fi", "wifi"),
        ("blu tooth", "bluetooth"),
        ("nite lite", "night light"),
        ("air plane", "airplane"),
        ("vol um", "volume"),
        ("brite ness", "brightness"),
        ("down loads", "downloads"),
        ("code edd itor", "code editor"),
        ("battree", "battery"),
    ] {
        text = text.replace(from, to);
    }
    text
}

/// Domain words and STT spellings that a lexical ranker misses. The model
/// still chooses; this only makes sure the right tool is on the menu.
fn needle_hints(text: &str) -> Vec<&'static str> {
    let q = text.to_lowercase();
    let mut out = Vec::new();
    let has = |needle: &str| q.contains(needle);
    if has("wifi") || has("why fi") || has("wi fi") || has("wi-fi") || has("network") {
        out.extend(["wifi.set", "probe.network"]);
    }
    if has("bluetooth") || has("blu tooth") {
        out.push("bluetooth.set");
    }
    if has("volume") || has("sound") || has("louder") || has("quieter") || has("mute") {
        out.extend(["audio.volume", "audio.volume_set", "audio.mute"]);
    }
    if has("bright") || has("screen") {
        out.extend(["display.brightness", "display.brightness_set"]);
    }
    if has("night") || has("warmth") || has("kelvin") || has("warm") {
        out.extend([
            "display.night_light",
            "display.night_light_temperature",
        ]);
    }
    if has("timer") {
        out.extend(["timer.set", "timer.cancel", "timer.list"]);
    }
    if has("alarm") {
        out.extend(["alarm.set", "alarm.cancel", "alarm.list"]);
    }
    if has("open") || has("launch") || has("start") {
        out.extend(["app.launch", "file.open", "place.open", "url.open"]);
    }
    if has("find") || has("search") {
        out.push("search.files");
    }
    if has("minimi") || has("maximi") || has("close") || has("window") || has("centre")
        || has("center") || has("move")
    {
        out.extend([
            "window.minimize",
            "window.maximize",
            "window.close",
            "window.center",
            "window.move",
        ]);
    }
    if has("battery") || has("charge") {
        out.push("probe.battery");
    }
    if has("uptime") || has("boot") || has("running") {
        out.extend(["probe.uptime", "probe.boot_history"]);
    }
    if has("disk") || has("storage") || has("space") {
        out.push("probe.disk");
    }
    if has("failed") || has("service") {
        out.push("probe.failed_services");
    }
    if has("temperature") || has("hot") {
        out.push("probe.temperature");
    }
    if has("note") {
        out.push("notes.open");
    }
    if has("calendar") || has("agenda") || has("meeting") {
        out.push("calendar.open");
    }
    if has("clock") || has("time now") {
        out.push("clock.open");
    }
    if has("terminal") || has("console") {
        out.push("terminal.open");
    }
    if has("lock") {
        out.push("system.lock");
    }
    out
}

fn text_arg(call: &crate::needle::Call, name: &str) -> Option<String> {
    call.arguments
        .get(name)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn number_arg(call: &crate::needle::Call, name: &str) -> Option<f64> {
    call.arguments.get(name).and_then(serde_json::Value::as_f64)
}
