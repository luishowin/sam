//! The journal side of Sam's voice (§14, F21): what went wrong, exactly,
//! as structured fields. Under systemd, GLib writes them to journald with
//! their priority (`journalctl --user IDA_TROUBLE=…`); in a terminal they go
//! to stderr. Every detail passes through `trouble::redact` first, so no
//! target or argument outlives the process (F6).

use sam_core::trouble::redact;

/// Records a failure. `code` is a short, stable name: a `Trouble`'s code,
/// or `scope-failed`, `config-ignored` and the like. `action` is the ID of
/// the action involved, if any. `detail` is the exact cause.
pub fn warn(code: &str, action: Option<&str>, detail: impl std::fmt::Display) {
    let detail = redact(&detail.to_string());
    let action = action.unwrap_or("-");
    glib::log_structured!(
        "sam",
        glib::LogLevel::Warning,
        {
            "MESSAGE" => "{} ({}): {}", code, action, detail;
            "IDA_TROUBLE" => code;
            "IDA_ACTION" => action;
            "IDA_DETAIL" => detail.as_str();
        }
    );
}

/// Records a network failure (§14): as [`warn`], plus the host (never the
/// path or the query) and the HTTP status, if there was one.
pub fn warn_net(
    code: &str,
    action: Option<&str>,
    host: Option<&str>,
    status: Option<u32>,
    detail: impl std::fmt::Display,
) {
    let detail = redact(&detail.to_string());
    let action = action.unwrap_or("-");
    let host = host.unwrap_or("-");
    let status = status.map_or_else(|| "-".to_owned(), |s| s.to_string());
    glib::log_structured!(
        "sam",
        glib::LogLevel::Warning,
        {
            "MESSAGE" => "{} ({}, {} {}): {}", code, action, host, status, detail;
            "IDA_TROUBLE" => code;
            "IDA_ACTION" => action;
            "IDA_HOST" => host;
            "IDA_STATUS" => status.as_str();
            "IDA_DETAIL" => detail.as_str();
        }
    );
}

/// Records a speech failure (§14): the trouble and the phase it happened in,
/// and nothing else.
///
/// There is no field for what was said, and there will not be one (F6, F30).
/// The phase is one of `mic`, `model` or `engine`, so the journal says where
/// listening broke without saying anything about the person listening.
pub fn warn_voice(code: &str, phase: &str) {
    glib::log_structured!(
        "sam",
        glib::LogLevel::Warning,
        {
            "MESSAGE" => "{} (voice.listen, {})", code, phase;
            "IDA_TROUBLE" => code;
            "IDA_ACTION" => "voice.listen";
            "IDA_VOICE" => phase;
        }
    );
}
