//! The headless side of `sam`: the same engine and adapters as the panel,
//! with text in and text out.

use std::io::Write;
use std::process::ExitCode;
use std::time::Instant;

use sam_core::action::{Handler, Registry};
use sam_core::intent::{Intent, Value};
use sam_core::policy::{self, CORE_LEVEL, Decision, Origin};
use sam_core::rank::Candidate;

use sam_linux::config::{tilde, Config};
use sam_linux::executor::Executor;

use crate::world::World;

const USAGE: u8 = 2;
const NOTHING: u8 = 4;
const NEEDS_YES: u8 = 3;

struct Timed {
    results: sam_core::engine::Results,
    sync_ms: f64,
    files_ms: Option<f64>,
    files_error: Option<String>,
}

fn search_all(world: &World, text: &str) -> Timed {
    let started = Instant::now();
    let mut results = world.search(text);
    let sync_ms = started.elapsed().as_secs_f64() * 1000.0;
    let (mut files_ms, mut files_error) = (None, None);
    if let Some(query) = results.query.files.clone() {
        let started = Instant::now();
        match world.files.search(&query) {
            Ok(rows) => results.extend(rows),
            Err(e) => files_error = Some(e),
        }
        files_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
    }
    Timed {
        results,
        sync_ms,
        files_ms,
        files_error,
    }
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn option<T: std::str::FromStr>(args: &[String], name: &str) -> Result<Option<T>, String> {
    match args.iter().position(|a| a == name) {
        None => Ok(None),
        Some(i) => args
            .get(i + 1)
            .and_then(|v| v.parse().ok())
            .map(Some)
            .ok_or_else(|| format!("{name} needs a number")),
    }
}

fn text_of(args: &[String], valued: &[&str]) -> String {
    let mut words = Vec::new();
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
        } else if valued.contains(&a.as_str()) {
            skip = true;
        } else if !a.starts_with("--") {
            words.push(a.as_str());
        }
    }
    words.join(" ")
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("sam: {msg}\n\n{}", crate::HELP);
    ExitCode::from(USAGE)
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_intent(intent: &Intent) -> String {
    let args: Vec<String> = intent
        .args
        .iter()
        .map(|(k, v)| {
            let value = match v {
                Value::Text(t) => json_string(t),
                Value::Number(n) if n.is_finite() => n.to_string(),
                Value::Number(_) => "null".into(),
            };
            format!("{}:{value}", json_string(k))
        })
        .collect();
    format!(
        "{{\"action\":{},\"args\":{{{}}}}}",
        json_string(&intent.action),
        args.join(",")
    )
}

fn json_row(c: &Candidate) -> String {
    format!(
        "{{\"title\":{},\"subtitle\":{},\"kind\":{},\"score\":{:.4},\"intent\":{},\"alt\":{}}}",
        json_string(&c.title),
        json_string(&c.subtitle),
        json_string(c.kind.label()),
        c.score,
        json_intent(&c.intent),
        c.alt.as_ref().map_or("null".into(), json_intent),
    )
}

fn print_rows(rows: &[Candidate]) {
    for (i, c) in rows.iter().enumerate() {
        println!(
            "{:>2}. {:>5.3}  {:<11}  {}",
            i + 1,
            c.score,
            c.kind.label(),
            c.title
        );
        if !c.subtitle.is_empty() {
            println!("                         {}", c.subtitle);
        }
        println!("                         ↵ {}", c.intent);
        if let Some(alt) = &c.alt {
            println!("                         ctrl+↵ {alt}");
        }
    }
}

pub fn query(args: &[String]) -> ExitCode {
    let limit = match option::<usize>(args, "--limit") {
        Ok(l) => l.unwrap_or(8),
        Err(e) => return usage(&e),
    };
    let text = text_of(args, &["--limit"]);
    if text.is_empty() {
        return usage("query needs some text");
    }
    let world = World::new();
    world.projects.refresh_now();
    let timed = search_all(&world, &text);
    let rows = world.panel_rows(&timed.results, limit);
    if flag(args, "--json") {
        let items: Vec<String> = rows.iter().map(json_row).collect();
        println!("[{}]", items.join(","));
    } else {
        let q = &timed.results.query;
        println!("{:?} · mode {:?} · subject {:?}", q.raw, q.mode, q.subject);
        print_rows(&rows);
        let files_ms = timed
            .files_ms
            .map_or(String::new(), |ms| format!(" · files {ms:.1} ms"));
        println!("timing: engine {:.1} ms{files_ms}", timed.sync_ms);
    }
    if let Some(e) = timed.files_error {
        eprintln!("sam: file search failed: {e}");
    }
    if rows.is_empty() {
        ExitCode::from(NOTHING)
    } else {
        ExitCode::SUCCESS
    }
}

pub fn run(args: &[String]) -> ExitCode {
    let pick = match option::<usize>(args, "--pick") {
        Ok(p) => p.unwrap_or(1),
        Err(e) => return usage(&e),
    };
    let text = text_of(args, &["--pick"]);
    if text.is_empty() {
        return usage("run needs some text");
    }
    let dry_run = flag(args, "--dry-run");
    let yes = flag(args, "--yes");
    let world = World::new();
    world.projects.refresh_now();
    let timed = search_all(&world, &text);
    let rows = world.panel_rows(&timed.results, pick.max(1) + 8);
    let Some(row) = rows.get(pick.saturating_sub(1)) else {
        eprintln!("sam: nothing matched {text:?}");
        return ExitCode::from(NOTHING);
    };
    println!("row:      {} ({})", row.title, row.kind.label());
    println!("intent:   {}", row.intent);
    let validated = match Registry::core().validate(row.intent.clone()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("sam: {e}");
            return ExitCode::FAILURE;
        }
    };
    let decision = policy::decide(CORE_LEVEL, validated, Origin::UserSelected);
    match decision {
        Decision::Approved(approved) => {
            println!(
                "decision: Approved ({:?}/{:?})",
                approved.spec().permission,
                approved.spec().confirmation
            );
            if dry_run {
                return ExitCode::SUCCESS;
            }
            execute(approved)
        }
        Decision::NeedsConfirmation(pending) => {
            println!(
                "decision: NeedsConfirmation ({:?})",
                pending.spec().permission
            );
            if dry_run {
                return ExitCode::from(NEEDS_YES);
            }
            if !yes {
                eprintln!("sam: this action asks first; pass --yes to confirm");
                return ExitCode::from(NEEDS_YES);
            }
            println!("confirmed: --yes");
            execute(pending.confirm())
        }
        Decision::Denied(denial) => {
            println!("decision: Denied — {denial}");
            ExitCode::FAILURE
        }
    }
}

fn execute(approved: policy::Approved) -> ExitCode {
    if approved.spec().handler == Handler::Panel {
        return match approved.action() {
            "info.copy" => {
                println!("{}", approved.text("text").unwrap_or_default());
                ExitCode::SUCCESS
            }
            "timer.list" | "alarm.list" => match sam_linux::timers::list() {
                Ok(list) => {
                    println!("{list}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("sam: {e}");
                    ExitCode::FAILURE
                }
            },
            "search.files" => {
                println!("find {}", approved.text("query").unwrap_or_default());
                ExitCode::SUCCESS
            }
            other => {
                eprintln!("sam: {other} is carried out by the panel");
                ExitCode::FAILURE
            }
        };
    }
    let ctx = gio::AppLaunchContext::new();
    match Executor.execute(approved, &ctx) {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("sam: {e}");
            ExitCode::FAILURE
        }
    }
}

pub fn bench(args: &[String]) -> ExitCode {
    let runs = match option::<usize>(args, "--runs") {
        Ok(n) => n.unwrap_or(5).max(1),
        Err(e) => return usage(&e),
    };
    let phrases = [
        "chrome",
        "find the PDF about invoices",
        "what is using my RAM",
        "how much disk space is free",
        "turn off bluetooth",
        "lock my computer",
        "what GPU do I have",
        "which network am I connected to",
    ];
    let world = World::new();
    world.projects.refresh_now();
    println!("{:<38} {:>10} {:>10}", "query", "engine ms", "files ms");
    for phrase in phrases {
        let mut engine = Vec::new();
        let mut files = Vec::new();
        for _ in 0..runs {
            let started = Instant::now();
            let results = world.search(phrase);
            engine.push(started.elapsed().as_secs_f64() * 1000.0);
            if let Some(query) = results.query.files {
                let started = Instant::now();
                let _ = world.files.search(&query);
                files.push(started.elapsed().as_secs_f64() * 1000.0);
            }
        }
        engine.sort_by(f64::total_cmp);
        files.sort_by(f64::total_cmp);
        let p50 = engine[engine.len() / 2];
        let f = files
            .get(files.len() / 2)
            .map_or("-".to_owned(), |v| format!("{v:.1}"));
        println!("{:<38} {:>10.2} {:>10}", phrase, p50, f);
    }
    ExitCode::SUCCESS
}

pub fn log(args: &[String]) -> ExitCode {
    let mut command = std::process::Command::new("journalctl");
    command.args(["--user", "-u", "io.github.luishowin.Sam.service", "-q"]);
    if args.is_empty() {
        command.args(["--since", "today"]);
    } else {
        command.args(args);
    }
    match command.status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("sam: journalctl: {e}");
            ExitCode::FAILURE
        }
    }
}

pub fn voice(args: &[String]) -> ExitCode {
    let root = sam_voice::default_models_root();
    match args.first().map(String::as_str) {
        None | Some("status") => {
            let status = sam_voice::model_status_for_root(&root);
            println!("engine:    Moonshine Small Streaming (local, offline)");
            println!("models:    {}", root.display());
            println!("model:     {}", status.dir().display());
            if status.is_present() {
                println!("state:     ready");
                ExitCode::SUCCESS
            } else {
                println!("state:     missing — run `sam voice setup`");
                if !status.missing().is_empty() {
                    println!("missing:   {}", status.missing().join(", "));
                }
                ExitCode::FAILURE
            }
        }
        Some("setup") => {
            let archive = args
                .iter()
                .position(|a| a == "--archive")
                .and_then(|i| args.get(i + 1))
                .map(std::path::PathBuf::from);
            let result = match archive {
                Some(path) => sam_voice::install_archive(&path, &root),
                None => sam_voice::download_and_install(&root),
            };
            match result {
                Ok(outcome) => {
                    println!("speech model ready at {}", outcome.dir().display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("sam voice setup: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some(other) => usage(&format!("voice: unknown command {other:?}")),
    }
}

pub fn notify(args: &[String]) -> ExitCode {
    let Some(id) = args.first() else {
        return usage("notify needs a timer id");
    };
    match sam_linux::timers::notify_due(id) {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sam: {e}");
            ExitCode::FAILURE
        }
    }
}

pub fn doctor() -> ExitCode {
    let world = World::new();
    println!("Sam {}", env!("CARGO_PKG_VERSION"));
    println!("app id:    {}", sam_core::APP_ID);
    println!("config:    {}", Config::path().display());
    println!("projects:  {} roots", world.projects.projects().len());
    println!("apps:      {} entries", world.apps.entries().len());
    println!("files:     LocalSearch adapter");
    println!("actions:   {}", Registry::core().specs().len());
    println!("network:   none (Sam has no egress)");
    let mut out = std::io::stdout();
    let _ = writeln!(out);
    ExitCode::SUCCESS
}
