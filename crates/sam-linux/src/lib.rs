//! SAM's Fedora/GNOME adapter. The only public door to a side effect is the
//! [`Executor`](executor::Executor), and it takes nothing but an approved
//! intent (`docs/ARCHITECTURE.md` §3). Everything else public here only
//! reads.

pub mod apps;
pub mod config;
pub mod executor;
pub mod files;
pub mod host;
pub mod info;
mod launch;
pub mod log;
pub mod paths;
pub mod projects;
pub mod setup;
pub mod system;
pub mod timers;
pub mod tinysparql;

pub use launch::{scope_name, systemd_escape};

/// Why an action could not be carried out, in words for the user.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecError(pub String);

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ExecError {}
