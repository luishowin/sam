//! Timers and alarms. They are user-requested state, not personal memory:
//! a small JSON list and one transient systemd user timer per entry.

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::ExecError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub due: u64,
}

fn state_path() -> PathBuf {
    glib::user_state_dir().join("sam").join("timers.json")
}

fn load() -> Vec<Entry> {
    std::fs::read_to_string(state_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save(entries: &[Entry]) -> Result<(), ExecError> {
    let path = state_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| ExecError(format!("{}: {e}", dir.display())))?;
    }
    let text = serde_json::to_string_pretty(entries)
        .map_err(|e| ExecError(format!("timers: {e}")))?;
    std::fs::write(&path, text).map_err(|e| ExecError(format!("{}: {e}", path.display())))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn human(seconds: u64) -> String {
    let d = seconds / 86_400;
    let h = (seconds % 86_400) / 3_600;
    let m = (seconds % 3_600) / 60;
    let s = seconds % 60;
    let mut parts = Vec::new();
    if d > 0 {
        parts.push(format!("{d}d"));
    }
    if h > 0 {
        parts.push(format!("{h}h"));
    }
    if m > 0 {
        parts.push(format!("{m}m"));
    }
    if s > 0 && d == 0 && h == 0 {
        parts.push(format!("{s}s"));
    }
    parts.join(" ")
}

/// `10m`, `1h30m`, `90s`, `2h`.
pub fn parse_duration(text: &str) -> Result<u64, ExecError> {
    let text = text.trim().to_lowercase();
    if text.is_empty() {
        return Err(ExecError("a duration is needed, such as 10m".into()));
    }
    let mut total = 0u64;
    let mut number = String::new();
    let mut any = false;
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            number.push(ch);
            continue;
        }
        let value: u64 = number
            .parse()
            .map_err(|_| ExecError(format!("{text:?} is not a duration")))?;
        number.clear();
        let unit = match ch {
            's' => 1,
            'm' => 60,
            'h' => 3_600,
            'd' => 86_400,
            _ => return Err(ExecError(format!("{ch:?} is not a duration unit"))),
        };
        total += value * unit;
        any = true;
    }
    if !number.is_empty() {
        // Bare seconds: "90".
        total += number
            .parse::<u64>()
            .map_err(|_| ExecError(format!("{text:?} is not a duration")))?;
        any = true;
    }
    if !any || total == 0 {
        return Err(ExecError("a duration must be at least one second".into()));
    }
    Ok(total.min(7 * 86_400))
}

fn next_alarm(text: &str) -> Result<u64, ExecError> {
    let cleaned = text.trim().to_lowercase().replace(' ', "");
    let (clock, pm) = if let Some(v) = cleaned.strip_suffix("pm") {
        (v.to_owned(), Some(true))
    } else if let Some(v) = cleaned.strip_suffix("am") {
        (v.to_owned(), Some(false))
    } else {
        (cleaned, None)
    };
    let mut parts = clock.split(':');
    let mut hour: u32 = parts
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ExecError("an alarm needs a time such as 07:30".into()))?;
    let minute: u32 = parts
        .next()
        .unwrap_or("0")
        .parse()
        .map_err(|_| ExecError("the minutes in that alarm are not a number".into()))?;
    if minute > 59 || hour > 23 {
        return Err(ExecError("that is not a clock time".into()));
    }
    if let Some(pm) = pm {
        if hour == 12 {
            hour = if pm { 12 } else { 0 };
        } else if pm {
            hour += 12;
        }
    }
    let now = glib::DateTime::now_local().map_err(|e| ExecError(e.to_string()))?;
    let target = glib::DateTime::new(
        &now.timezone(),
        now.year(),
        now.month(),
        now.day_of_month(),
        hour as i32,
        minute as i32,
        0.0,
    )
    .map_err(|e| ExecError(e.to_string()))?;
    let mut due = target.to_unix() as u64;
    let now = now.to_unix() as u64;
    if due <= now + 5 {
        due += 86_400;
    }
    Ok(due)
}

fn id_for(due: u64) -> String {
    format!("{due}-{}", std::process::id())
}

fn schedule(entry: &Entry) -> Result<(), ExecError> {
    let wait = entry.due.saturating_sub(now()).max(1);
    let exe = std::env::current_exe().map_err(|e| ExecError(format!("sam: {e}")))?;
    let unit = format!("sam-timer-{}.timer", entry.id);
    let out = Command::new("systemd-run")
        .args([
            "--user",
            "--collect",
            &format!("--unit=sam-timer-{}", entry.id),
            &format!("--on-active={wait}s"),
            "--timer-property=AccuracySec=1s",
            exe.to_string_lossy().as_ref(),
            "notify",
            &entry.id,
        ])
        .output()
        .map_err(|e| ExecError(format!("systemd-run: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(ExecError(format!(
            "could not schedule {unit}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

pub fn set(duration: &str, label: &str) -> Result<String, ExecError> {
    let seconds = parse_duration(duration)?;
    let due = now() + seconds;
    let entry = Entry {
        id: id_for(due),
        kind: "timer".into(),
        label: if label.trim().is_empty() {
            format!("{} timer", human(seconds))
        } else {
            label.trim().to_owned()
        },
        due,
    };
    schedule(&entry)?;
    let mut entries = load();
    entries.push(entry.clone());
    save(&entries)?;
    Ok(format!("Timer set for {} · {}", human(seconds), entry.label))
}

pub fn alarm(time: &str, label: &str) -> Result<String, ExecError> {
    let due = next_alarm(time)?;
    let entry = Entry {
        id: id_for(due),
        kind: "alarm".into(),
        label: if label.trim().is_empty() {
            format!("Alarm at {time}")
        } else {
            label.trim().to_owned()
        },
        due,
    };
    schedule(&entry)?;
    let mut entries = load();
    entries.push(entry.clone());
    save(&entries)?;
    let when = glib::DateTime::from_unix_local(due as i64)
        .ok()
        .and_then(|d| d.format("%H:%M").ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| time.to_owned());
    Ok(format!("Alarm set for {when} · {}", entry.label))
}

pub fn list() -> Result<String, ExecError> {
    let entries = load();
    if entries.is_empty() {
        return Ok("No timers or alarms.".into());
    }
    let mut lines = Vec::new();
    for entry in entries {
        let when = glib::DateTime::from_unix_local(entry.due as i64)
            .ok()
            .and_then(|d| d.format("%H:%M").ok())
            .map(|s| s.to_string())
            .unwrap_or_default();
        let left = entry.due.saturating_sub(now());
        lines.push(format!(
            "{} · {} · {when} ({} left)",
            entry.kind,
            entry.label,
            human(left)
        ));
    }
    Ok(lines.join("\n"))
}

pub fn cancel(which: &str) -> Result<String, ExecError> {
    let mut entries = load();
    if entries.is_empty() {
        return Err(ExecError("there are no timers or alarms".into()));
    }
    let which = which.trim().to_lowercase();
    let index = if which.is_empty() {
        0
    } else if let Ok(n) = which.parse::<usize>() {
        n.saturating_sub(1)
    } else {
        entries
            .iter()
            .position(|e| e.label.to_lowercase().contains(&which))
            .ok_or_else(|| ExecError(format!("no timer matches {which:?}")))?
    };
    if index >= entries.len() {
        return Err(ExecError("there is no timer with that number".into()));
    }
    let entry = entries.remove(index);
    let _ = Command::new("systemctl")
        .args([
            "--user",
            "stop",
            &format!("sam-timer-{}.timer", entry.id),
        ])
        .output();
    save(&entries)?;
    Ok(format!("Cancelled {} · {}", entry.kind, entry.label))
}

/// Called by the transient systemd unit when an entry is due.
pub fn notify_due(id: &str) -> Result<String, ExecError> {
    let mut entries = load();
    let Some(index) = entries.iter().position(|e| e.id == id) else {
        return Ok(String::new());
    };
    let entry = entries.remove(index);
    save(&entries)?;
    let summary = if entry.kind == "alarm" {
        "Sam alarm"
    } else {
        "Sam timer"
    };
    let _ = Command::new("notify-send")
        .args(["-a", "Sam", summary, &entry.label])
        .output();
    Ok(entry.label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(parse_duration("10m").unwrap(), 600);
        assert_eq!(parse_duration("1h30m").unwrap(), 5_400);
        assert_eq!(parse_duration("90s").unwrap(), 90);
        assert!(parse_duration("nope").is_err());
        assert!(parse_duration("0s").is_err());
    }

    #[test]
    fn clock_times() {
        assert!(next_alarm("07:30").is_ok());
        assert!(next_alarm("7:30pm").is_ok());
        assert!(next_alarm("25:00").is_err());
        assert!(next_alarm("nope").is_err());
    }
}
