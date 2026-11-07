//! The local Needle client.
//!
//! The sidecar is optional. If its socket is missing or slow, Sam simply
//! keeps the deterministic result. A model call is never executed here: it
//! is parsed, resolved and returned as an inert [`Intent`](sam_core::intent::Intent).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Call {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct Reply {
    #[serde(default)]
    call: Option<Call>,
    #[serde(default)]
    error: Option<String>,
}

pub struct Needle {
    socket: PathBuf,
}

impl Needle {
    /// The socket may not exist yet: the sidecar and Sam both start with the
    /// graphical session, in either order. Check lazily on every proposal.
    pub fn new() -> Needle {
        let socket = std::env::var_os("SAM_NEEDLE_SOCKET")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("XDG_RUNTIME_DIR")
                    .map(PathBuf::from)
                    .map(|dir| dir.join("sam").join("needle.sock"))
            })
            .unwrap_or_else(|| PathBuf::from("/tmp/sam/needle.sock"));
        Needle { socket }
    }

    pub fn socket(&self) -> &std::path::Path {
        &self.socket
    }

    /// Ask for one call. `None` means no usable proposal: missing sidecar,
    /// timeout, malformed reply, or `[]`.
    pub fn propose(&self, query: &str, tools_json: &str, fingerprint: &str) -> Option<Call> {
        if !self.socket.exists() {
            return None;
        }
        let tools: serde_json::Value = serde_json::from_str(tools_json).ok()?;
        let request = serde_json::json!({
            "query": query,
            "tools": tools,
            "fingerprint": fingerprint,
            "system": system_fact(),
        });
        let mut stream = UnixStream::connect(&self.socket).ok()?;
        stream
            .set_read_timeout(Some(Duration::from_millis(4_000)))
            .ok()?;
        stream
            .set_write_timeout(Some(Duration::from_millis(200)))
            .ok()?;
        stream
            .write_all(format!("{request}\n").as_bytes())
            .ok()?;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let reply: Reply = serde_json::from_str(line.trim()).ok()?;
        if let Some(error) = reply.error {
            eprintln!("sam: needle: {error}");
            return None;
        }
        reply.call
    }
}

fn system_fact() -> String {
    let now = glib::DateTime::now_local()
        .ok()
        .and_then(|dt| dt.format("%Y-%m-%d %a %H:%M").ok())
        .map(|s| s.to_string())
        .unwrap_or_default();
    format!("date: {now}; locale: en-GB")
}
