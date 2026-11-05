//! SAM's core. Nothing here touches the operating system: it turns text into
//! [`Intent`](intent::Intent)s and decides which of them may run. See
//! `docs/ARCHITECTURE.md` §3.

pub mod action;
pub mod calc;
pub mod engine;
pub mod entity;
pub mod files;
pub mod format;
pub mod grammar;
pub mod help;
pub mod intent;
pub mod policy;
pub mod rank;
pub mod result;
pub mod sites;
pub mod text;
pub mod trouble;
pub mod units;

/// The application ID (§0 F4): the D-Bus name, the desktop-file name and the
/// systemd unit name.
pub const APP_ID: &str = "io.github.luishowin.Sam";
