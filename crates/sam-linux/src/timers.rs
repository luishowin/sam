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

/// `10m`, `10 minutes`, `five minutes`, `1h30m`, `90s`, `2h`,
/// `half an hour`, `one hour thirty minutes`.
pub fn parse_duration(text: &str) -> Result<u64, ExecError> {
    let mut text = text.trim().to_lowercase();
    if text.is_empty() {
        return Err(ExecError("a duration is needed, such as 10m".into()));
    }
    // Spoken forms that are easier to handle before tokenising.
    for (phrase, value) in [
        ("half an hour", "0.5 hour"),
        ("half a hour", "0.5 hour"),
        ("half hour", "0.5 hour"),
        ("quarter of an hour", "0.25 hour"),
        ("quarter hour", "0.25 hour"),
        ("an hour", "1 hour"),
        ("a hour", "1 hour"),
        ("an minute", "1 minute"),
        ("a minute", "1 minute"),
    ] {
        text = text.replace(phrase, value);
    }
    // Split compact forms: 1h30m, 5min, 90s, 1.5h.
    let mut spaced = String::with_capacity(text.len() + 8);
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        spaced.push(*c);
        if let Some(next) = chars.get(i + 1) {
            let boundary = (c.is_ascii_digit() && next.is_ascii_alphabetic())
                || (c.is_ascii_alphabetic() && next.is_ascii_digit());
            if boundary {
                spaced.push(' ');
            }
        }
    }

    let words: Vec<&str> = spaced
        .split(|c: char| c.is_whitespace() || c == ',' || c == '-')
        .filter(|w| !w.is_empty() && !matches!(*w, "and" | "for" | "about" | "around"))
        .collect();
    if words.is_empty() {
        return Err(ExecError("a duration is needed, such as 10m".into()));
    }

    let mut total = 0.0f64;
    let mut pending: Option<f64> = None;
    let mut any_unit = false;
    for word in words {
        if let Some(n) = number_word(word).or_else(|| word.parse::<f64>().ok()) {
            pending = Some(pending.unwrap_or(0.0) + n);
            continue;
        }
        let unit = match word {
            "s" | "sec" | "secs" | "second" | "seconds" => 1.0,
            "m" | "min" | "mins" | "minute" | "minutes" => 60.0,
            "h" | "hr" | "hrs" | "hour" | "hours" => 3_600.0,
            "d" | "day" | "days" => 86_400.0,
            _ => {
                return Err(ExecError(format!("{word:?} is not a duration unit")));
            }
        };
        let value = pending.take().unwrap_or(1.0);
        total += value * unit;
        any_unit = true;
    }
    if let Some(value) = pending {
        // A bare number means seconds, as before.
        total += value;
        any_unit = true;
    }
    if !any_unit || !total.is_finite() || total < 1.0 {
        return Err(ExecError("a duration must be at least one second".into()));
    }
    let seconds = total.round() as u64;
    Ok(seconds.min(7 * 86_400))
}

fn number_word(word: &str) -> Option<f64> {
    Some(match word {
        "zero" => 0.0,
        "one" => 1.0,
        "two" => 2.0,
        "three" => 3.0,
        "four" => 4.0,
        "five" => 5.0,
        "six" => 6.0,
        "seven" => 7.0,
        "eight" => 8.0,
        "nine" => 9.0,
        "ten" => 10.0,
        "eleven" => 11.0,
        "twelve" => 12.0,
        "thirteen" => 13.0,
        "fourteen" => 14.0,
        "fifteen" => 15.0,
        "sixteen" => 16.0,
        "seventeen" => 17.0,
        "eighteen" => 18.0,
        "nineteen" => 19.0,
        "twenty" => 20.0,
        "thirty" => 30.0,
        "forty" => 40.0,
        "fifty" => 50.0,
        "sixty" => 60.0,
        "seventy" => 70.0,
        "eighty" => 80.0,
        "ninety" => 90.0,
        "hundred" => 100.0,
        _ => return None,
    })
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

/// A started timer, with the entry the panel needs for its countdown.
pub struct Started {
    pub message: String,
    pub entry: Entry,
}

pub fn set_entry(duration: &str, label: &str) -> Result<Started, ExecError> {
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
    Ok(Started {
        message: format!("Timer set for {} · {}", human(seconds), entry.label),
        entry,
    })
}

pub fn set(duration: &str, label: &str) -> Result<String, ExecError> {
    set_entry(duration, label).map(|started| started.message)
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
            .position(|e| e.id == which || e.label.to_lowercase().contains(&which))
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
        assert_eq!(parse_duration("5 minutes").unwrap(), 300);
        assert_eq!(parse_duration("five minutes").unwrap(), 300);
        assert_eq!(parse_duration("1 hour 30 minutes").unwrap(), 5_400);
        assert_eq!(parse_duration("half an hour").unwrap(), 1_800);
        assert_eq!(parse_duration("1.5 hours").unwrap(), 5_400);
        assert_eq!(parse_duration("twenty five minutes").unwrap(), 1_500);
        assert_eq!(parse_duration("2 hrs").unwrap(), 7_200);
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
