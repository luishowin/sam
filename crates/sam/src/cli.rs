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

pub fn status() -> ExitCode {
    config_status();
    ExitCode::SUCCESS
}

pub fn doctor() -> ExitCode {
    let world = World::new();
    world.projects.refresh_now();
    println!("Sam {}", env!("CARGO_PKG_VERSION"));
    println!("app id:    {}", sam_core::APP_ID);
    println!("config:    {}", Config::path().display());
    println!("projects:  {}", world.projects.projects().len());
    println!("apps:      {} entries", world.apps.entries().len());
    println!("files:     LocalSearch adapter");
    println!("actions:   {}", Registry::core().specs().len());
    println!("network:   none (Sam has no egress)");
    let speech = sam_voice::model_status_for_root(&sam_voice::default_models_root());
    println!(
        "speech:    {}",
        if speech.is_present() { "ready" } else { "missing" }
    );
    if let Ok(status) = std::fs::read_to_string("/proc/self/status")
        && let Some(line) = status.lines().find(|l| l.starts_with("VmRSS:"))
    {
        println!("rss:       {}", line.trim_start_matches("VmRSS:").trim());
    }
    let mut out = std::io::stdout();
    let _ = writeln!(out);
    ExitCode::SUCCESS
}

// ---- terminal configuration ------------------------------------------------

const SERVICE: &str = "io.github.luishowin.Sam.service";

fn systemctl(args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .map_err(|e| format!("systemctl: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if out.status.success() {
        Ok(text)
    } else {
        Err(if text.is_empty() {
            String::from_utf8_lossy(&out.stderr).trim().to_owned()
        } else {
            text
        })
    }
}

fn restart_service() -> String {
    match systemctl(&["restart", SERVICE]) {
        Ok(_) => "service restarted".into(),
        Err(e) => format!("service restart failed: {e}"),
    }
}

fn service_pid() -> Option<u32> {
    systemctl(&["show", "-p", "MainPID", "--value", SERVICE])
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|p| *p > 0)
}

fn service_rss(pid: u32) -> Option<(u64, u64, u64)> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let value = |name: &str| {
        status
            .lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    };
    Some((value("VmRSS:"), value("VmHWM:"), value("Threads:")))
}

fn config_status() {
    let config = Config::load();
    println!("Sam {}", env!("CARGO_PKG_VERSION"));
    println!("app id:    {}", sam_core::APP_ID);
    println!("config:    {}", Config::path().display());
    println!("service:   {}", systemctl(&["is-active", SERVICE]).unwrap_or_else(|e| e));
    println!(
        "voice:     {} ({})",
        if config.voice.enabled { "enabled" } else { "disabled" },
        config.voice_mode()
    );
    println!(
        "auto-run:  {}",
        if config.voice.auto_run { "on" } else { "off" }
    );
    let model = sam_voice::model_status_for_root(&sam_voice::default_models_root());
    println!(
        "model:     {}",
        if model.is_present() { "ready" } else { "missing" }
    );
    if let Some(pid) = service_pid()
        && let Some((rss, peak, threads)) = service_rss(pid)
    {
        println!(
            "memory:    {} MB RSS · {} MB peak · {} threads",
            rss / 1024,
            peak / 1024,
            threads
        );
    }
}

fn service_menu() {
    loop {
        println!("\nService");
        println!("  1. Start");
        println!("  2. Stop");
        println!("  3. Restart");
        println!("  0. Back");
        match prompt("Choose: ").as_str() {
            "1" => println!("{}", systemctl(&["start", SERVICE]).unwrap_or_else(|e| e)),
            "2" => println!("{}", systemctl(&["stop", SERVICE]).unwrap_or_else(|e| e)),
            "3" => println!("{}", restart_service()),
            "0" => return,
            _ => println!("Unknown choice."),
        }
    }
}

fn voice_menu() {
    loop {
        println!("\nVoice");
        println!("  1. Enable (eager, instant)");
        println!("  2. Enable (lazy, low idle memory)");
        println!("  3. Disable");
        println!("  4. Download / verify model");
        println!("  5. Status");
        println!("  0. Back");
        let choice = prompt("Choose: ");
        match choice.as_str() {
            "1" | "2" | "3" => {
                let mut config = Config::load();
                match choice.as_str() {
                    "1" => {
                        config.voice.enabled = true;
                        config.voice.mode = "eager".into();
                    }
                    "2" => {
                        config.voice.enabled = true;
                        config.voice.mode = "lazy".into();
                    }
                    _ => {
                        config.voice.enabled = false;
                        config.voice.mode = "off".into();
                    }
                }
                match config.save() {
                    Ok(()) => println!("Saved. {}", restart_service()),
                    Err(e) => println!("Could not save: {e}"),
                }
            }
            "4" => match sam_voice::download_and_install(&sam_voice::default_models_root()) {
                Ok(outcome) => println!("Model ready at {}", outcome.dir().display()),
                Err(e) => println!("Voice setup failed: {e}"),
            },
            "5" => {
                let status = sam_voice::model_status_for_root(&sam_voice::default_models_root());
                println!("Model: {}", status.dir().display());
                println!("State: {}", if status.is_present() { "ready" } else { "missing" });
            }
            "0" => return,
            _ => println!("Unknown choice."),
        }
    }
}

fn prompt(label: &str) -> String {
    print!("{label}");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    line.trim().to_owned()
}

pub fn config(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        None => config_menu(),
        Some("path") => {
            println!("{}", Config::path().display());
            ExitCode::SUCCESS
        }
        Some("get") => {
            let Some(key) = args.get(1) else {
                return usage("config get needs a key");
            };
            let config = Config::load();
            let value = match key.as_str() {
                "voice.enabled" => config.voice.enabled.to_string(),
                "voice.mode" => config.voice_mode().to_owned(),
                "voice.auto_run" => config.voice.auto_run.to_string(),
                "voice.threads" => config.voice.threads.to_string(),
                "voice.model" => config.voice.model.clone(),
                other => return usage(&format!("unknown key {other:?}")),
            };
            println!("{value}");
            ExitCode::SUCCESS
        }
        Some("set") => {
            let (Some(key), Some(value)) = (args.get(1), args.get(2)) else {
                return usage("config set needs a key and a value");
            };
            let mut config = Config::load();
            match key.as_str() {
                "voice.enabled" => {
                    config.voice.enabled = matches!(value.as_str(), "1" | "true" | "yes" | "on");
                    if !config.voice.enabled {
                        config.voice.mode = "off".into();
                    }
                }
                "voice.mode" => match value.as_str() {
                    "eager" | "lazy" => {
                        config.voice.enabled = true;
                        config.voice.mode = value.clone();
                    }
                    "off" => {
                        config.voice.enabled = false;
                        config.voice.mode = "off".into();
                    }
                    other => return usage(&format!("unknown voice mode {other:?}")),
                },
                "voice.auto_run" => {
                    config.voice.auto_run = matches!(value.as_str(), "1" | "true" | "yes" | "on")
                }
                "voice.threads" => match value.parse::<usize>() {
                    Ok(n) if (1..=16).contains(&n) => config.voice.threads = n,
                    _ => return usage("voice.threads must be 1–16"),
                },
                other => return usage(&format!("unknown key {other:?}")),
            }
            match config.save() {
                Ok(()) => {
                    println!("saved. {}", restart_service());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("sam: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some(other) => usage(&format!("config: unknown command {other:?}")),
    }
}

fn config_menu() -> ExitCode {
    loop {
        let config = Config::load();
        println!("\nSam configuration");
        println!(
            "  voice: {} ({}) · auto-run {}",
            if config.voice.enabled { "on" } else { "off" },
            config.voice_mode(),
            if config.voice.auto_run { "on" } else { "off" }
        );
        println!("  1. Status");
        println!("  2. Version");
        println!("  3. Service start / stop / restart");
        println!("  4. Voice on / off / mode");
        println!("  5. Voice model setup / status");
        println!("  6. Memory and CPU");
        println!("  7. Config file path");
        println!("  8. Re-run sam setup (Super+Space)");
        println!("  9. Toggle voice auto-run");
        println!("  0. Quit");
        match prompt("Choose: ").as_str() {
            "1" => config_status(),
            "2" => println!("sam {}", env!("CARGO_PKG_VERSION")),
            "3" => service_menu(),
            "4" => voice_menu(),
            "5" => voice_menu(),
            "6" => {
                if let Some(pid) = service_pid()
                    && let Some((rss, peak, threads)) = service_rss(pid)
                {
                    println!("pid:      {pid}");
                    println!("rss:      {} MB", rss / 1024);
                    println!("peak:     {} MB", peak / 1024);
                    println!("threads:  {threads}");
                } else {
                    println!("The service is not running.");
                }
            }
            "7" => println!("{}", Config::path().display()),
            "8" => match sam_linux::setup::install() {
                Ok(report) => {
                    for line in report {
                        println!("{line}");
                    }
                }
                Err(e) => println!("setup failed: {e}"),
            },
            "9" => {
                let mut config = Config::load();
                config.voice.auto_run = !config.voice.auto_run;
                match config.save() {
                    Ok(()) => println!(
                        "auto-run {}. {}",
                        if config.voice.auto_run { "on" } else { "off" },
                        restart_service()
                    ),
                    Err(e) => println!("Could not save: {e}"),
                }
            }
            "0" => return ExitCode::SUCCESS,
            _ => println!("Unknown choice."),
        }
    }
}
