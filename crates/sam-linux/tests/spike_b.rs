//! Spike B (§0 P4): LocalSearch through the hand-written FFI. Needs a live
//! session with LocalSearch running:
//!
//!     cargo test -p sam-linux --test spike_b -- --ignored --nocapture

use std::collections::BTreeSet;
use std::process::Command;
use std::time::Instant;

use sam_linux::tinysparql::Connection;

const GATE_TERMS: &[&str] = &["invoice", "invoices", "photodesk", "raycast", "screenshot"];

/// The shape `localsearch search` uses (all graphs, file name or content).
const PARITY: &str = r#"
SELECT DISTINCT ?url WHERE {
  { GRAPH tracker:FileSystem { ?f fts:match ~match . } }
  UNION { GRAPH tracker:FileSystem { ?e fts:match ~match ; nie:isStoredAs ?f . } }
  UNION { GRAPH tracker:Audio { ?e fts:match ~match ; nie:isStoredAs ?f . } }
  UNION { GRAPH tracker:Documents { ?e fts:match ~match ; nie:isStoredAs ?f . } }
  UNION { GRAPH tracker:Video { ?e fts:match ~match ; nie:isStoredAs ?f . } }
  UNION { GRAPH tracker:Software { ?e fts:match ~match ; nie:isStoredAs ?f . } }
  UNION { GRAPH tracker:Pictures { ?e fts:match ~match ; nie:isStoredAs ?f . } }
  GRAPH tracker:FileSystem { ?f nie:url ?url . }
}
"#;

/// What SAM will ask: the same match, with the fields a row needs, ranked
/// file-name hits first, then newest, and optionally limited to a time range.
const RICH: &str = r#"
SELECT ?url ?name ?mime ?modified (MAX(?hit) AS ?rank) WHERE {
  { GRAPH tracker:FileSystem { ?f fts:match ~match . BIND (2 AS ?hit) } }
  UNION { GRAPH tracker:FileSystem { ?e fts:match ~match ; nie:isStoredAs ?f . BIND (1 AS ?hit) } }
  UNION { GRAPH tracker:Audio { ?e fts:match ~match ; nie:isStoredAs ?f . BIND (1 AS ?hit) } }
  UNION { GRAPH tracker:Documents { ?e fts:match ~match ; nie:isStoredAs ?f . BIND (1 AS ?hit) } }
  UNION { GRAPH tracker:Video { ?e fts:match ~match ; nie:isStoredAs ?f . BIND (1 AS ?hit) } }
  UNION { GRAPH tracker:Software { ?e fts:match ~match ; nie:isStoredAs ?f . BIND (1 AS ?hit) } }
  UNION { GRAPH tracker:Pictures { ?e fts:match ~match ; nie:isStoredAs ?f . BIND (1 AS ?hit) } }
  GRAPH tracker:FileSystem { ?f nie:url ?url ; nfo:fileName ?name ; nfo:fileLastModified ?modified . }
  OPTIONAL { ?any nie:isStoredAs ?f ; nie:mimeType ?mime . }
  FILTER (?modified >= ~from && ?modified < ~to)
}
GROUP BY ?url ?name ?mime ?modified
ORDER BY DESC(?rank) DESC(?modified)
LIMIT 20
"#;

fn cli(term: &str) -> BTreeSet<String> {
    let out = Command::new("localsearch")
        .args(["search", "--limit", "1000", term])
        .output()
        .expect("localsearch runs");
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .map(|w| {
            // Strip the CLI's colour escapes.
            let mut clean = String::new();
            let mut chars = w.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '\u{1b}' {
                    for d in chars.by_ref() {
                        if d == 'm' {
                            break;
                        }
                    }
                } else {
                    clean.push(c);
                }
            }
            clean
        })
        .filter(|w| w.starts_with("file://"))
        .collect()
}

fn p(values: &mut [f64], q: f64) -> f64 {
    values.sort_by(|a, b| a.total_cmp(b));
    values[((values.len() - 1) as f64 * q).round() as usize]
}

#[test]
#[ignore = "needs a live session with LocalSearch"]
fn gate() {
    let started = Instant::now();
    let conn = Connection::localsearch().expect("connects to LocalSearch");
    println!(
        "connect: {:.2} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );

    let started = Instant::now();
    let mut parity = conn.statement(PARITY).expect("parity query parses");
    let mut rich = conn.statement(RICH).expect("rich query parses");
    println!(
        "prepare both: {:.2} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );

    for term in GATE_TERMS {
        parity.bind_str("match", term);
        let mut cursor = parity.execute().expect("parity query runs");
        let mut ours = BTreeSet::new();
        while cursor.next_row().expect("row") {
            ours.insert(cursor.string(0).expect("url bound"));
        }
        let theirs = cli(term);
        println!("{term:>10}: ffi {} · cli {}", ours.len(), theirs.len());
        assert_eq!(ours, theirs, "URIs for {term:?}");
    }

    let mut times = Vec::new();
    for _ in 0..40 {
        for term in GATE_TERMS {
            let t = Instant::now();
            rich.bind_str("match", term);
            rich.bind_unix_time("from", 0).unwrap();
            rich.bind_unix_time("to", i64::from(i32::MAX)).unwrap();
            let mut cursor = rich.execute().expect("rich query runs");
            let mut rows = 0;
            while cursor.next_row().expect("row") {
                let _ = (
                    cursor.string(0),
                    cursor.string(1),
                    cursor.string(2),
                    cursor.string(3),
                );
                rows += 1;
            }
            assert!(rows > 0, "{term} returns rows");
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
    }
    let (p50, p95, max) = (p(&mut times, 0.5), p(&mut times, 0.95), p(&mut times, 1.0));
    println!(
        "rich query, {} runs: p50 {p50:.2} ms · p95 {p95:.2} ms · max {max:.2} ms",
        times.len()
    );
    assert!(p95 < 30.0, "p95 {p95:.2} ms is over the 30 ms gate");
}

#[test]
#[ignore = "needs a live session with LocalSearch"]
fn time_range_and_injection() {
    let conn = Connection::localsearch().unwrap();
    let mut rich = conn.statement(RICH).unwrap();

    // Nothing is modified before 1971.
    rich.bind_str("match", "screenshot");
    rich.bind_unix_time("from", 0).unwrap();
    rich.bind_unix_time("to", 365 * 86_400).unwrap();
    let mut cursor = rich.execute().unwrap();
    assert!(
        !cursor.next_row().unwrap(),
        "an empty range returns nothing"
    );

    // Text that would end the query if it were spliced in is only a string.
    rich.clear();
    rich.bind_str("match", "x\" } } DELETE { ?s ?p ?o } WHERE { ?s ?p ?o");
    rich.bind_unix_time("from", 0).unwrap();
    rich.bind_unix_time("to", i64::from(i32::MAX)).unwrap();
    // Either no rows or an FTS syntax error; never a changed store.
    match rich.execute() {
        Ok(mut c) => while c.next_row().unwrap_or(false) {},
        Err(e) => println!("rejected as FTS syntax: {e}"),
    }
}
