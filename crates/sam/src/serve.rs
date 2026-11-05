//! A small local JSON API over a Unix socket. This is the typed local
//! interface other Essential OS components can use until the full service
//! contract is designed. It never opens a network socket.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::ExitCode;
use std::time::Instant;

use serde_json::{json, Value};

use sam_core::intent::Value as ArgValue;
use sam_core::rank::Candidate;

use crate::world::World;

fn default_socket() -> std::path::PathBuf {
    glib::user_runtime_dir().join("sam.sock")
}

fn row_value(c: &Candidate) -> Value {
    let args: serde_json::Map<String, Value> = c
        .intent
        .args
        .iter()
        .map(|(k, v)| {
            let value = match v {
                ArgValue::Text(t) => json!(t),
                ArgValue::Number(n) if n.is_finite() => json!(n),
                ArgValue::Number(_) => Value::Null,
            };
            (k.clone(), value)
        })
        .collect();
    json!({
        "title": c.title,
        "subtitle": c.subtitle,
        "kind": c.kind.label(),
        "score": c.score,
        "intent": { "action": c.intent.action, "args": args },
    })
}

fn answer(world: &World, query: &str) -> Value {
    let started = Instant::now();
    let mut results = world.search(query);
    if let Some(file_query) = results.query.files.clone()
        && let Ok(rows) = world.files.search(&file_query)
    {
        results.extend(rows);
    }
    let rows = world.panel_rows(&results, 10);
    let status = if rows.is_empty() {
        "no_confident_match"
    } else {
        "ok"
    };
    json!({
        "status": status,
        "query": query,
        "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
        "results": rows.iter().map(row_value).collect::<Vec<_>>(),
    })
}

fn handle(stream: UnixStream, world: &World) {
    let Ok(read) = stream.try_clone() else {
        return;
    };
    let mut writer = stream;
    let reader = BufReader::new(read);
    for line in reader.lines() {
        let response = match line {
            Ok(line) if line.trim().is_empty() => continue,
            Ok(line) => match serde_json::from_str::<Value>(&line) {
                Ok(value) => match value.get("query").and_then(Value::as_str) {
                    Some(query) if !query.trim().is_empty() => answer(world, query),
                    _ => json!({
                        "status": "error",
                        "message": "send {\"query\": \"...\"}",
                    }),
                },
                Err(e) => json!({"status": "error", "message": format!("invalid JSON: {e}")}),
            },
            Err(e) => {
                let _ = writeln!(
                    writer,
                    "{}",
                    json!({"status": "error", "message": format!("read: {e}")})
                );
                return;
            }
        };
        if writeln!(writer, "{response}").is_err() {
            return;
        }
        let _ = writer.flush();
    }
}

pub fn serve(args: &[String]) -> ExitCode {
    let socket = args
        .iter()
        .position(|a| a == "--socket")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(default_socket);
    let _ = std::fs::remove_file(&socket);
    let listener = match UnixListener::bind(&socket) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("sam serve: {}: {e}", socket.display());
            return ExitCode::FAILURE;
        }
    };
    let world = World::new();
    world.projects.refresh_now();
    println!("sam listening on {}", socket.display());
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle(stream, &world),
            Err(e) => eprintln!("sam serve: {e}"),
        }
    }
    ExitCode::SUCCESS
}
