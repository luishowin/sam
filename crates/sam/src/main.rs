//! `sam` — the panel, and a headless CLI around the same engine.

mod app;
mod cli;
mod panel;
mod row;
mod serve;
mod surface;
mod trace;
mod world;

use std::process::ExitCode;

const HELP: &str = "\
sam — search execution engine for this computer

USAGE
  sam                         show the panel (starting Sam if needed)
  sam show [TEXT]             show the panel with TEXT typed
  sam query TEXT [--json] [--limit N]
                              what the panel would list for TEXT
  sam run TEXT [--dry-run] [--pick N] [--yes]
                              do what Enter would do on the first (or Nth) row;
                              --dry-run only prints the intent and the decision,
                              --yes answers an action that asks first
  sam bench [--runs N]        time the engine and file search
  sam voice [status|setup]    the local speech model and microphone
  sam setup [--undo]          take Super+Space for Sam and enable its service
  sam serve [--socket PATH]   local JSON API over a Unix socket
  sam config                  terminal configuration menu
  sam status                  status, version, service and memory
  sam doctor                  versions, sources and action count
  sam log [journalctl args]   what went wrong today, from the journal
  sam --gapplication-service  run hidden, as the systemd unit does
  sam --gapplication-replace  replace a running instance (development)

  SAM_TRACE=1 logs summon, focus and query timings to stderr.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("setup") => setup(&args[2..]),
        Some("query") => cli::query(&args[2..]),
        Some("run") => cli::run(&args[2..]),
        Some("bench") => cli::bench(&args[2..]),
        Some("voice") => cli::voice(&args[2..]),
        Some("doctor") => cli::doctor(),
        Some("serve") => serve::serve(&args[2..]),
        Some("config") => cli::config(&args[2..]),
        Some("status") => cli::status(),
        Some("log") => cli::log(&args[2..]),
        Some("notify") => cli::notify(&args[2..]),
        Some("show") => app::show_remote(&args[2..].join(" ")),
        Some("help" | "--help" | "-h") => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Some("--version" | "-V") => {
            println!("sam {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => app::run(),
    }
}

fn setup(args: &[String]) -> ExitCode {
    let result = match args.first().map(String::as_str) {
        None => sam_linux::setup::install(),
        Some("--undo") => sam_linux::setup::undo(),
        Some(other) => {
            eprintln!("sam setup: unknown argument {other:?}\n\n{HELP}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(report) => {
            for line in report {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("sam setup: {e}");
            ExitCode::FAILURE
        }
    }
}
