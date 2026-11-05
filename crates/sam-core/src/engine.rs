//! Text in, ranked candidates out (§5). The engine runs the core's own
//! providers (calculator, units, grammar, places) and whatever providers the
//! host adds (apps, projects, paths, files), then weighs and orders them.

use crate::action::{PLACES, Registry};
use crate::calc;
use crate::files::{self, Clock, FileKind, FileQuery};
use crate::grammar::{self, Command, Grammar, Hit};
use crate::intent::{Intent, Source};
use crate::rank::{Candidate, Kind, Scorer};
use crate::sites::{self, Site};
use crate::text;
use crate::units;

/// What the host knows that the core may ask (read-only, §4).
pub trait Host {
    /// Where a place (`downloads`, `trash`, …) is here, if it exists.
    fn place(&self, id: &str) -> Option<PlaceInfo>;
    fn clock(&self) -> Clock;
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaceInfo {
    /// "Downloads"
    pub title: String,
    /// "~/Downloads"
    pub subtitle: String,
    /// "file:///home/luis/Downloads" — also the row's dedupe key.
    pub uri: String,
    pub icon: String,
}

/// A source of candidates the host adds (§5).
pub trait Provider {
    /// Adds candidates for `query` with raw scores, 0–1.
    fn provide(&self, query: &Query, scorer: &Scorer, out: &mut Vec<Candidate>);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Plain text: everything competes.
    Normal,
    /// "open …"
    Open,
    /// "find …"
    Find,
    /// "/help"
    Help,
    /// "~/…" or "/…"
    Path,
}

#[derive(Debug, Clone)]
pub struct Query {
    pub raw: String,
    /// Normalised words (§6).
    pub words: Vec<String>,
    pub mode: Mode,
    /// What name-matching providers should match: the words after the verb
    /// in open and find modes, the typed text otherwise.
    pub subject: String,
    /// What to ask the file index, if anything.
    pub files: Option<FileQuery>,
    /// The input was arithmetic or a conversion; nothing else matters much.
    pub computed: bool,
    /// The whole input was a grammar command.
    pub command: bool,
}

/// Candidates for one query, before ranking. More can be added (the file
/// search arrives later), then [`Engine::rank`] orders them.
pub struct Results {
    pub query: Query,
    raw: Vec<Candidate>,
}

impl Results {
    pub fn extend(&mut self, more: impl IntoIterator<Item = Candidate>) {
        self.raw.extend(more);
    }

    pub fn raw(&self) -> &[Candidate] {
        &self.raw
    }
}

/// Shown when nothing better matched.
const FALLBACK_SCORE: f32 = 0.05;
/// "Ask Sam" for text shaped like a question: above anything but a whole
/// command or a pinned row (§15).
const QUESTION_SCORE: f32 = 0.96;
/// "Ask Sam" for several words that nothing else matched well: first,
/// unless a file's name matches.
const ASK_SCORE: f32 = 0.49;
/// "Ask Sam" under good matches: last but for the file-search fallback.
const ASK_LAST_SCORE: f32 = 0.06;
/// The Ask row's icon, installed with SAM.
pub const SPARKLE_ICON: &str = "io.github.luishowin.Sam-sparkle-symbolic";
/// Calculations, conversions and typed paths go first.
const PINNED: f32 = 1.5;
/// Below this, the interpreter (if any) is asked. Model-suggested rows
/// also score under this ceiling, so deterministic wins always rank first.
pub const WEAK: f32 = 0.5;

pub struct Engine {
    registry: Registry,
    grammar: Grammar,
    scorer: Scorer,
    /// Commands that can run as they are, for matching by title.
    ready: Vec<usize>,
    /// Web shortcuts (§17).
    sites: Vec<Site>,
}

impl Default for Engine {
    fn default() -> Self {
        Engine::new()
    }
}

impl Engine {
    pub fn new() -> Engine {
        Engine::with_grammar(Grammar::core())
    }

    pub fn with_grammar(grammar: Grammar) -> Engine {
        let registry = Registry::core();
        let ready = grammar
            .commands()
            .iter()
            .enumerate()
            .filter(|(_, c)| registry.validate(c.intent(Default::default())).is_ok())
            .map(|(i, _)| i)
            .collect();
        Engine {
            registry,
            grammar,
            scorer: Scorer::new(),
            ready,
            sites: sites::core(),
        }
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn scorer(&self) -> &Scorer {
        &self.scorer
    }

    pub fn parse(&self, input: &str, host: &dyn Host) -> Query {
        let raw = input.trim().to_owned();
        let words = text::normalize(&raw);
        let computed = calc::evaluate(&raw).is_some() || units::convert(&raw).is_some();
        let mut query = Query {
            raw: raw.clone(),
            words,
            mode: Mode::Normal,
            subject: raw.clone(),
            files: None,
            computed,
            command: false,
        };
        if raw == crate::help::QUESTION {
            query.mode = Mode::Help;
            return query;
        }
        if raw.starts_with('~') || raw.starts_with('/') {
            query.mode = Mode::Path;
            return query;
        }
        if computed {
            return query;
        }
        match self.grammar.hit(&query.words) {
            Some(Hit::Mode {
                mode: grammar::Mode::Find,
                rest,
            }) => {
                let files = files::parse(&rest, host.clock());
                query.mode = Mode::Find;
                query.subject = files.terms.join(" ");
                query.files = (!files.is_empty()).then_some(files);
            }
            Some(Hit::Mode {
                mode: grammar::Mode::Open,
                rest,
            }) => {
                query.mode = Mode::Open;
                query.subject = rest.join(" ");
            }
            // A whole command is not also a file search.
            Some(Hit::Command { .. }) => query.command = true,
            None => {
                // Files are searched for plain text too, below everything else.
                if raw.chars().count() >= 3 {
                    let files = files::parse(&query.words, host.clock());
                    query.files = (!files.is_empty()).then_some(files);
                }
            }
        }
        query
    }

    /// Everything the core and `providers` find for `input`, unranked.
    pub fn search(&self, input: &str, host: &dyn Host, providers: &[&dyn Provider]) -> Results {
        let query = self.parse(input, host);
        let mut raw = Vec::new();
        match query.mode {
            Mode::Help => {
                self.help(&mut raw);
                return Results { query, raw };
            }
            Mode::Path => {}
            Mode::Normal | Mode::Open | Mode::Find => {
                self.computed(&query, &mut raw);
                if !query.computed {
                    self.commands(&query, host, &mut raw);
                    self.places(&query, host, &mut raw);
                    self.sites(&query, &mut raw);
                    self.fallback(&query, &mut raw);
                }
            }
        }
        for provider in providers {
            provider.provide(&query, &self.scorer, &mut raw);
        }
        self.web_handoff(&query, &mut raw);
        let results = Results { query, raw };
        results
    }

    /// Weighs, de-duplicates and orders; at most `limit` rows.
    pub fn rank(&self, results: &Results, limit: usize) -> Vec<Candidate> {
        let query = &results.query;
        let mut rows: Vec<Candidate> = results
            .raw
            .iter()
            .filter_map(|c| {
                let w = weight(c.kind, query);
                (w > 0.0).then(|| Candidate {
                    score: c.score * w,
                    ..c.clone()
                })
            })
            .collect();
        rows.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then(a.kind.cmp(&b.kind))
                .then(a.title.cmp(&b.title))
        });
        let mut seen = std::collections::HashSet::new();
        rows.retain(|c| seen.insert(c.key.clone()));
        rows.truncate(limit);
        rows
    }

    /// `/help`: the one row, answered locally by the panel.
    fn help(&self, out: &mut Vec<Candidate>) {
        out.push(Candidate {
            title: "Help".into(),
            subtitle: "Everything Sam can do, and how".into(),
            kind: Kind::Help,
            icon: "help-about-symbolic".into(),
            score: 1.0,
            key: "help".into(),
            intent: Intent::new("help.show", Source::Grammar),
            alt: None,
        });
    }

    fn computed(&self, query: &Query, out: &mut Vec<Candidate>) {
        if let Some(c) = calc::evaluate(&query.raw) {
            out.push(Candidate {
                title: c.display(),
                subtitle: format!("{} · Enter copies the result", c.expression),
                kind: Kind::Calculation,
                icon: "accessories-calculator-symbolic".into(),
                score: 1.0,
                key: "calc".into(),
                intent: Intent::new("clipboard.copy", Source::Provider).arg("text", c.plain()),
                alt: None,
            });
        }
        if let Some(c) = units::convert(&query.raw) {
            out.push(Candidate {
                title: c.display(),
                subtitle: format!(
                    "{} · {} · Enter copies the result",
                    c.expression(),
                    c.from.dimension.name()
                ),
                kind: Kind::Conversion,
                icon: "accessories-calculator-symbolic".into(),
                score: 1.0,
                key: "convert".into(),
                intent: Intent::new("clipboard.copy", Source::Provider).arg("text", c.plain()),
                alt: None,
            });
        }
    }

    fn command_row(
        &self,
        command: &Command,
        intent: Intent,
        score: f32,
        host: &dyn Host,
    ) -> Option<Candidate> {
        // A place command shows the place itself.
        if intent.action == "place.open" {
            let info = host.place(intent.text("place")?)?;
            return Some(Candidate {
                title: info.title,
                subtitle: info.subtitle,
                kind: Kind::Place,
                icon: info.icon,
                score,
                key: format!("uri:{}", info.uri),
                intent,
                alt: None,
            });
        }
        Some(Candidate {
            title: command.title.clone(),
            subtitle: command.subtitle.clone().unwrap_or_else(|| "Command".into()),
            kind: Kind::Command,
            icon: command.icon.clone(),
            score,
            key: format!("cmd:{intent}"),
            intent,
            alt: None,
        })
    }

    fn commands(&self, query: &Query, host: &dyn Host, out: &mut Vec<Candidate>) {
        if let Some(Hit::Command { command, intent }) = self.grammar.hit(&query.words)
            && self.registry.validate(intent.clone()).is_ok()
            && let Some(row) = self.command_row(command, intent, 0.97, host)
        {
            out.push(row);
        }
        if query.mode != Mode::Normal || query.subject.chars().count() < 2 {
            return;
        }
        for &i in &self.ready {
            let command = &self.grammar.commands()[i];
            let names = std::iter::once((command.title.as_str(), 1.0))
                .chain(command.keywords.iter().map(|k| (k.as_str(), 0.9)));
            if let Some(score) = self.scorer.best(&query.subject, names)
                && let Some(row) = self.command_row(
                    command,
                    command.intent(Default::default()),
                    score * 0.9,
                    host,
                )
            {
                out.push(row);
            }
        }
    }

    fn places(&self, query: &Query, host: &dyn Host, out: &mut Vec<Candidate>) {
        if query.subject.chars().count() < 2 {
            return;
        }
        for id in PLACES {
            let Some(info) = host.place(id) else { continue };
            if let Some(score) = self.scorer.name(&query.subject, &info.title) {
                out.push(Candidate {
                    key: format!("uri:{}", info.uri),
                    title: info.title,
                    subtitle: info.subtitle,
                    kind: Kind::Place,
                    icon: info.icon,
                    score: score * 0.9,
                    intent: Intent::new("place.open", Source::Provider).arg("place", *id),
                    alt: None,
                });
            }
        }
    }

    /// Web shortcuts (§17). Matched by name and keywords, like a command's
    /// title, and opened by the browser.
    fn sites(&self, query: &Query, out: &mut Vec<Candidate>) {
        if query.mode == Mode::Find || query.subject.chars().count() < 2 {
            return;
        }
        for site in &self.sites {
            let names = std::iter::once((site.name.as_str(), 1.0))
                .chain(site.keywords.iter().map(|k| (k.as_str(), 0.9)));
            let Some(score) = self.scorer.best(&query.subject, names) else {
                continue;
            };
            out.push(Candidate {
                key: format!("uri:{}", site.url),
                title: format!("Open {}", site.name),
                subtitle: site.url.clone(),
                kind: Kind::Site,
                icon: "web-browser-symbolic".into(),
                score: score * 0.85,
                intent: Intent::new("url.open", Source::Provider).arg("url", site.url.as_str()),
                alt: None,
            });
        }
    }

    /// Out-of-domain text gets one explicit handoff. Sam itself never
    /// fetches anything; the browser is opened by the caller.
    fn web_handoff(&self, query: &Query, out: &mut Vec<Candidate>) {
        if query.mode != Mode::Normal
            || query.command
            || query.computed
            || query.subject.trim().chars().count() < 4
        {
            return;
        }
        if best_score(out, query) >= 0.8 {
            return;
        }
        let text = query.subject.trim();
        if text.split_whitespace().count() < 2 {
            return;
        }
        let url = format!("https://duckduckgo.com/?q={}", percent_encode(text));
        out.push(Candidate {
            title: format!("Search the web for “{text}”"),
            subtitle: "Opens your browser · Sam does not browse".into(),
            kind: Kind::Site,
            icon: "web-browser-symbolic".into(),
            score: 0.34,
            key: "web-handoff".into(),
            intent: Intent::new("url.open", Source::Provider).arg("url", url),
            alt: None,
        });
    }

    fn fallback(&self, query: &Query, out: &mut Vec<Candidate>) {
        if query.mode != Mode::Normal || query.command || query.subject.chars().count() < 2 {
            return;
        }
        out.push(Candidate {
            title: format!("Search files for “{}”", query.subject),
            subtitle: "Names and contents, through LocalSearch".into(),
            kind: Kind::Search,
            icon: "system-search-symbolic".into(),
            score: FALLBACK_SCORE,
            key: "search".into(),
            intent: Intent::new("search.files", Source::Provider)
                .arg("query", query.subject.as_str()),
            alt: None,
        });
    }
}

fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The best weighted score among rows that are not fallbacks.
fn best_score(rows: &[Candidate], query: &Query) -> f32 {
    rows.iter()
        .filter(|c| !matches!(c.kind, Kind::Search | Kind::Answer))
        .map(|c| c.score * weight(c.kind, query))
        .fold(0.0, f32::max)
}

/// `nairobi` → `Nairobi`, `new york` → `New York`.
fn title_case(text: &str) -> String {
    text.split(' ')
        .map(|w| {
            let mut chars = w.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// How much a kind counts in a mode (§5).
fn weight(kind: Kind, query: &Query) -> f32 {
    let typed_kind = query
        .files
        .as_ref()
        .is_some_and(|f| !f.kinds.is_empty() && !f.kinds.contains(&FileKind::Folder));
    match (kind, query.mode) {
        (Kind::Calculation | Kind::Conversion | Kind::Path, _) => PINNED,
        (Kind::Help, _) => 1.0,
        (Kind::Info | Kind::Doc, Mode::Normal) => 1.0,
        (Kind::Info | Kind::Doc, _) => 0.0,
        (Kind::Search | Kind::Answer, Mode::Normal) => 1.0,
        (Kind::Search | Kind::Answer, _) => 0.0,
        (Kind::App, Mode::Find) => 0.7,
        // A website is not a file, and "find" is about this computer.
        (Kind::Site, Mode::Find) => 0.3,
        (Kind::Setting, Mode::Find) => 1.2,
        (Kind::Command, Mode::Find) => 0.6,
        (Kind::Place, Mode::Find) => 0.9,
        // "find photodesk screenshots" wants screenshots, not the repo.
        (Kind::Project, Mode::Find) if typed_kind => 0.6,
        (Kind::Project, Mode::Find) => 1.05,
        (Kind::Folder | Kind::File, Mode::Find) => 1.2,
        (Kind::Setting, _) => 0.85,
        (Kind::Project, Mode::Open) => 0.9,
        (Kind::Project, _) => 0.8,
        (Kind::Folder, Mode::Open) => 0.8,
        (Kind::File, Mode::Open) => 0.75,
        (Kind::Folder | Kind::File, _) => 0.7,
        (Kind::App | Kind::Command | Kind::Place | Kind::Site, _) => 1.0,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub struct FakeHost;

    impl Host for FakeHost {
        fn place(&self, id: &str) -> Option<PlaceInfo> {
            let title = match id {
                "downloads" => "Downloads",
                "documents" => "Documents",
                "pictures" => "Pictures",
                "home" => "Home",
                "trash" => "Trash",
                _ => return None,
            };
            Some(PlaceInfo {
                title: title.into(),
                subtitle: format!("~/{title}"),
                uri: format!("file:///home/luis/{title}"),
                icon: "folder-symbolic".into(),
            })
        }

        fn clock(&self) -> Clock {
            Clock {
                now: 1_789_581_600,
                utc_offset: 3 * 3600,
            }
        }
    }

    /// Apps, projects and files as this machine has them (DECISIONS 2026-09-16).
    pub struct FakeWorld;

    fn app(id: &str, name: &str, scorer: &Scorer, q: &Query, extra: &[&str]) -> Option<Candidate> {
        let tail = id
            .trim_end_matches(".desktop")
            .rsplit('.')
            .next()
            .unwrap_or(id);
        let names = std::iter::once((name, 1.0))
            .chain(std::iter::once((tail, 0.9)))
            .chain(extra.iter().map(|e| (*e, 0.9)));
        let score = scorer.best(&q.subject, names)?;
        Some(Candidate {
            title: name.into(),
            subtitle: String::new(),
            kind: Kind::App,
            icon: String::new(),
            score,
            key: format!("app:{id}"),
            intent: Intent::new("app.launch", Source::Provider).arg("app_id", id),
            alt: None,
        })
    }

    fn file(uri: &str, name: &str, kind: Kind, score: f32) -> Candidate {
        Candidate {
            title: name.into(),
            subtitle: String::new(),
            kind,
            icon: String::new(),
            score,
            key: format!("uri:{uri}"),
            intent: Intent::new("file.open", Source::Provider).arg("uri", uri),
            alt: None,
        }
    }

    impl Provider for FakeWorld {
        fn provide(&self, q: &Query, scorer: &Scorer, out: &mut Vec<Candidate>) {
            if matches!(q.mode, Mode::Normal | Mode::Open | Mode::Find) {
                let apps: &[(&str, &str, &[&str])] = &[
                    (
                        "com.google.Chrome.desktop",
                        "Google Chrome",
                        &["browser", "web"],
                    ),
                    ("PhotoDesk.desktop", "PhotoDesk", &["photo", "editor"]),
                    (
                        "com.visualstudio.code.desktop",
                        "Visual Studio Code",
                        &["editor", "code"],
                    ),
                    ("org.gnome.Calculator.desktop", "Calculator", &["math"]),
                    ("org.gnome.Calendar.desktop", "Calendar", &[]),
                    (
                        "org.gnome.Nautilus.desktop",
                        "Files",
                        &["folder", "explorer"],
                    ),
                    (
                        "org.gnome.Ptyxis.desktop",
                        "Terminal",
                        &["shell", "console"],
                    ),
                    ("org.gnome.Settings.desktop", "Settings", &["preferences"]),
                ];
                for (id, name, extra) in apps {
                    out.extend(app(id, name, scorer, q, extra));
                }
                for (name, path) in [
                    ("photodesk", "/home/luis/Documents/GitHub/photodesk"),
                    ("atlas-map", "/home/luis/Desktop/atlas-map"),
                ] {
                    if let Some(score) = scorer.name(&q.subject, name) {
                        let mut c = file(&format!("file://{path}"), name, Kind::Project, score);
                        c.score = score;
                        out.push(c);
                    }
                }
            }
            // What LocalSearch returns for these terms (SPIKE-B).
            if let Some(f) = &q.files {
                let terms = f.terms.join(" ");
                if terms.contains("invoice") && (f.kinds.is_empty() || f.kinds == [FileKind::Pdf]) {
                    out.push(file(
                        "file:///home/luis/Documents/nanyukicountryhome/d4.pdf",
                        "d4.pdf",
                        Kind::File,
                        0.6,
                    ));
                }
                if terms == "photodesk" && f.kinds.is_empty() {
                    out.push(file(
                        "file:///home/luis/Pictures/a.jpg.photodesk.json",
                        "a.jpg.photodesk.json",
                        Kind::File,
                        0.75,
                    ));
                }
                if terms == "chrome" {
                    out.push(file(
                        "file:///home/luis/Downloads/chrome.txt",
                        "chrome.txt",
                        Kind::File,
                        0.75,
                    ));
                }
            }
        }
    }

    #[test]
    fn web_shortcuts_open_in_the_browser() {
        // The phrase that listed nothing until 0.0.4 (§17).
        let (kind, title, intent) = first("open youtube");
        assert_eq!(kind, Kind::Site);
        assert_eq!(title, "Open YouTube");
        assert_eq!(intent, r#"url.open {url: "https://www.youtube.com/"}"#);
        // A keyword finds it too, below its real name.
        assert_eq!(first("yt").0, Kind::Site);
        // "find" is about this computer, so a site sinks there.
        assert!(
            !top("find youtube").iter().any(|(k, _, _)| *k == Kind::Site),
            "a website ranked in find mode"
        );
        // And a one-letter subject asks nothing of the table.
        assert!(!top("y").iter().any(|(k, _, _)| *k == Kind::Site));
    }

    pub fn top(input: &str) -> Vec<(Kind, String, String)> {
        let engine = Engine::new();
        let results = engine.search(input, &FakeHost, &[&FakeWorld]);
        engine
            .rank(&results, 8)
            .into_iter()
            .map(|c| (c.kind, c.title, c.intent.to_string()))
            .collect()
    }

    fn first(input: &str) -> (Kind, String, String) {
        top(input)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("{input:?} found nothing"))
    }

    #[test]
    fn the_plan_table() {
        for (input, kind, intent) in [
            (
                "chrome",
                Kind::App,
                r#"app.launch {app_id: "com.google.Chrome.desktop"}"#,
            ),
            (
                "photodesk",
                Kind::App,
                r#"app.launch {app_id: "PhotoDesk.desktop"}"#,
            ),
            (
                "vsc",
                Kind::App,
                r#"app.launch {app_id: "com.visualstudio.code.desktop"}"#,
            ),
            (
                "open calculator",
                Kind::App,
                r#"app.launch {app_id: "org.gnome.Calculator.desktop"}"#,
            ),
            (
                "launch chrome",
                Kind::App,
                r#"app.launch {app_id: "com.google.Chrome.desktop"}"#,
            ),
            (
                "17 * 42",
                Kind::Calculation,
                r#"clipboard.copy {text: "714"}"#,
            ),
            (
                "sqrt(2)^2",
                Kind::Calculation,
                r#"clipboard.copy {text: "2"}"#,
            ),
            (
                "2.5 GB to MB",
                Kind::Conversion,
                r#"clipboard.copy {text: "2500"}"#,
            ),
            (
                "convert 34°C to fahrenheit",
                Kind::Conversion,
                r#"clipboard.copy {text: "93.2"}"#,
            ),
            (
                "open my downloads",
                Kind::Place,
                r#"place.open {place: "downloads"}"#,
            ),
            (
                "downloads",
                Kind::Place,
                r#"place.open {place: "downloads"}"#,
            ),
            ("downl", Kind::Place, r#"place.open {place: "downloads"}"#),
            (
                "find PhotoDesk",
                Kind::Project,
                r#"file.open {uri: "file:///home/luis/Documents/GitHub/photodesk"}"#,
            ),
            (
                "find the PDF about invoices",
                Kind::File,
                r#"file.open {uri: "file:///home/luis/Documents/nanyukicountryhome/d4.pdf"}"#,
            ),
            ("lock my computer", Kind::Command, "system.lock"),
            (
                "turn off bluetooth",
                Kind::Command,
                r#"bluetooth.set {state: "off"}"#,
            ),
            ("turn off wifi", Kind::Command, r#"wifi.set {state: "off"}"#),
            (
                "volume up",
                Kind::Command,
                r#"audio.volume {direction: "up"}"#,
            ),
            (
                "pause music",
                Kind::Command,
                r#"media.control {command: "pause"}"#,
            ),
            (
                "dark mode on",
                Kind::Command,
                r#"appearance.dark {state: "on"}"#,
            ),
            ("show me today's calendar", Kind::Command, "calendar.open"),
            ("log out", Kind::Command, "session.logout"),
            ("restart", Kind::Command, "session.restart"),
            ("shut down", Kind::Command, "session.power_off"),
            ("suspend", Kind::Command, "system.suspend"),
        ] {
            let (k, _, i) = first(input);
            assert_eq!(
                (k, i.as_str()),
                (kind, intent),
                "{input}: {:#?}",
                top(input)
            );
        }
    }

    #[test]
    fn slash_help_shows_only_help() {
        // `/help` is answered locally: the one row, nothing else.
        let rows = top("/help");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, Kind::Help);
        assert_eq!(rows[0].1, "Help");
        assert_eq!(rows[0].2, "help.show");
        // Anything longer is not help.
        assert!(top("/helper").iter().all(|r| r.0 != Kind::Help));
    }

    #[test]
    fn things_that_must_not_happen() {
        // An app name is never a file search first.
        assert_eq!(first("chrome").0, Kind::App);
        // "find photodesk" still offers the app, below the repo.
        let rows = top("find photodesk");
        assert!(rows.iter().any(|r| r.1 == "PhotoDesk" && r.0 == Kind::App));
        // Plain "photodesk" offers the repo, below the app.
        assert_eq!(top("photodesk")[1].0, Kind::Project);
        // A calculation is not also a file search.
        let engine = Engine::new();
        assert!(engine.parse("17 * 42", &FakeHost).files.is_none());
        // A lone letter matches no command by title.
        assert!(top("a").iter().all(|r| r.0 != Kind::Command));
        // Nothing typed, nothing shown.
        assert!(top("").is_empty());
        // A whole command needs no "search files for …" under it.
        assert!(top("lock my computer").iter().all(|r| r.0 != Kind::Search));
        // Three letters do not drag in commands by scattered matches.
        assert!(
            top("vsc").iter().all(|r| r.0 != Kind::Command),
            "{:#?}",
            top("vsc")
        );
    }

    #[test]
    fn typing_a_command_shows_it() {
        let rows = top("blueto");
        assert!(
            rows.iter().any(|r| r.1 == "Turn Bluetooth Off"),
            "{rows:#?}"
        );
        let rows = top("lock");
        assert_eq!(rows[0].2, "system.lock");
        let rows = top("dark");
        assert!(
            rows.iter()
                .take(3)
                .any(|r| r.1.starts_with("Dark Style") || r.1 == "Toggle Dark Style"),
            "{rows:#?}"
        );
    }

    #[test]
    fn plain_text_searches_files_last() {
        let rows = top("invoice");
        assert_eq!(rows[0].0, Kind::File);
        assert_eq!(
            rows.last().unwrap().0,
            Kind::Search,
            "the fallback comes last"
        );
        let engine = Engine::new();
        let q = engine.parse("invoice", &FakeHost);
        assert_eq!(q.files.unwrap().terms, ["invoice"]);
        assert!(
            engine.parse("ab", &FakeHost).files.is_none(),
            "not under three characters"
        );
    }

    #[test]
    fn find_with_a_kind_prefers_files() {
        let engine = Engine::new();
        let q = engine.parse("find the photodesk screenshots", &FakeHost);
        assert_eq!(q.mode, Mode::Find);
        assert_eq!(q.subject, "photodesk");
        let f = q.files.unwrap();
        assert_eq!(f.kinds, [FileKind::Screenshot]);
        assert!(
            weight(
                Kind::Project,
                &engine.parse("find photodesk screenshots", &FakeHost)
            ) < 1.0
        );
    }

}
