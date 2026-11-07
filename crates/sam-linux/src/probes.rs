//! Fixed read-only probes.
//!
//! A probe is a name plus a fixed argv. The model never supplies a command,
//! a flag or a path. Each run has a hard timeout and an output cap, and a
//! missing tool is a normal "source unavailable", not a crash.

use std::fs;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::ExecError;

const TIMEOUT: Duration = Duration::from_secs(2);
const MAX_OUTPUT: usize = 64 * 1024;

pub fn run(id: &str) -> Result<String, ExecError> {
    match id {
        "probe.uptime" => uptime(),
        "probe.boot_history" => boot_history(),
        "probe.battery" => battery(),
        "probe.failed_services" => failed_services(),
        "probe.disk" => disk(),
        "probe.network" => network(),
        "probe.temperature" => temperature(),
        other => Err(ExecError(format!("unknown probe {other}"))),
    }
}

fn read_capped<R: Read>(reader: R) -> String {
    let mut text = String::new();
    let _ = reader.take(MAX_OUTPUT as u64).read_to_string(&mut text);
    text
}

fn run_command(program: &str, args: &[&str]) -> Result<String, ExecError> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ExecError(format!("{program}: {e}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ExecError(format!("{program}: no stdout")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ExecError(format!("{program}: no stderr")))?;
    let out_thread = thread::spawn(move || read_capped(stdout));
    let err_thread = thread::spawn(move || read_capped(stderr));

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = out_thread.join();
                let _ = err_thread.join();
                return Err(ExecError(format!("{program}: {e}")));
            }
        }
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = out_thread.join();
            let _ = err_thread.join();
            return Err(ExecError(format!("{program} timed out")));
        }
        thread::sleep(Duration::from_millis(20));
    };

    let out = out_thread.join().unwrap_or_default();
    let err = err_thread.join().unwrap_or_default();
    if status.success() {
        Ok(out.trim().to_owned())
    } else if !err.trim().is_empty() {
        Err(ExecError(err.trim().to_owned()))
    } else {
        Err(ExecError(format!("{program} exited with {status}")))
    }
}

fn human_duration(seconds: u64) -> String {
    let d = seconds / 86_400;
    let h = (seconds % 86_400) / 3_600;
    let m = (seconds % 3_600) / 60;
    let mut parts = Vec::new();
    if d > 0 {
        parts.push(format!("{d}d"));
    }
    if h > 0 {
        parts.push(format!("{h}h"));
    }
    if m > 0 && d == 0 {
        parts.push(format!("{m}m"));
    }
    if parts.is_empty() {
        parts.push(format!("{seconds}s"));
    }
    parts.join(" ")
}

fn summarize(text: &str, lines: usize) -> String {
    let mut tail: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .rev()
        .take(lines)
        .collect();
    tail.reverse();
    tail.join(" · ")
}

fn uptime() -> Result<String, ExecError> {
    let text = fs::read_to_string("/proc/uptime")
        .map_err(|e| ExecError(format!("/proc/uptime: {e}")))?;
    let seconds = text
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<f64>().ok())
        .ok_or_else(|| ExecError("/proc/uptime is not a number".into()))?;
    let seconds = seconds.max(0.0) as u64;
    let boot = glib::DateTime::now_local()
        .and_then(|now| now.add_seconds(-(seconds as f64)))
        .ok()
        .and_then(|dt| dt.format("%H:%M").ok())
        .map(|s| s.to_string());
    Ok(match boot {
        Some(boot) => format!("Up {}; booted at {boot}", human_duration(seconds)),
        None => format!("Up {}", human_duration(seconds)),
    })
}

fn boot_history() -> Result<String, ExecError> {
    let text = run_command("journalctl", &["--list-boots", "--no-pager"])
        .or_else(|_| run_command("last", &["-x", "reboot", "-n", "5"]))?;
    if text.trim().is_empty() {
        return Ok("No boot history available".into());
    }
    Ok(format!("Recent boots: {}", summarize(&text, 4)))
}

fn battery() -> Result<String, ExecError> {
    if let Ok(devices) = run_command("upower", &["-e"]) {
        if let Some(device) = devices
            .lines()
            .map(str::trim)
            .find(|line| line.to_ascii_lowercase().contains("battery"))
        {
            if let Ok(info) = run_command("upower", &["-i", device]) {
                let mut percent = None;
                let mut state = None;
                let mut remaining = None;
                for line in info.lines().map(str::trim) {
                    if let Some(value) = line.strip_prefix("percentage:") {
                        percent = Some(value.trim().to_owned());
                    } else if let Some(value) = line.strip_prefix("state:") {
                        state = Some(value.trim().to_owned());
                    } else if let Some(value) = line
                        .strip_prefix("time to empty:")
                        .or_else(|| line.strip_prefix("time to full:"))
                    {
                        remaining = Some(value.trim().to_owned());
                    }
                }
                if let Some(percent) = percent {
                    let mut parts = vec![format!("Battery {percent}")];
                    if let Some(state) = state {
                        parts.push(state);
                    }
                    if let Some(remaining) = remaining {
                        parts.push(remaining);
                    }
                    return Ok(parts.join(" · "));
                }
            }
        }
    }
    // Fallback: the kernel's own battery nodes.
    let Ok(entries) = fs::read_dir("/sys/class/power_supply") else {
        return Ok("No battery source available".into());
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("BAT") {
            continue;
        }
        let capacity = fs::read_to_string(entry.path().join("capacity")).ok();
        let status = fs::read_to_string(entry.path().join("status")).ok();
        if let Some(capacity) = capacity {
            let capacity = capacity.trim();
            let status = status.as_deref().unwrap_or("").trim().to_ascii_lowercase();
            return Ok(match status.is_empty() {
                true => format!("Battery {capacity}%"),
                false => format!("Battery {capacity}% · {status}"),
            });
        }
    }
    Ok("No battery found".into())
}

fn failed_services() -> Result<String, ExecError> {
    let text = run_command("systemctl", &["--user", "--failed", "--no-pager"])?;
    let units: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| line.ends_with(".service") || line.contains(".service "))
        .collect();
    if units.is_empty() {
        return Ok("No failed user services".into());
    }
    let shown = units.iter().take(3).copied().collect::<Vec<_>>().join(", ");
    Ok(format!("{} failed: {shown}", units.len()))
}

fn disk() -> Result<String, ExecError> {
    let text = run_command("df", &["-h", "--output=size,avail,pcent,target", "/home"])?;
    let Some(row) = text.lines().nth(1) else {
        return Ok("No disk space source available".into());
    };
    let fields: Vec<&str> = row.split_whitespace().collect();
    if fields.len() < 4 {
        return Ok(text);
    }
    Ok(format!(
        "Home: {} free of {} ({} used)",
        fields[1], fields[0], fields[2]
    ))
}

fn network() -> Result<String, ExecError> {
    let text = run_command(
        "nmcli",
        &["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "device", "status"],
    )
    .map_err(|_| ExecError("network source unavailable".into()))?;
    for line in text.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() >= 4 && fields[2] == "connected" {
            let kind = match fields[1] {
                "wifi" => "Wi-Fi",
                "ethernet" => "Ethernet",
                other => other,
            };
            let name = if fields[3].is_empty() {
                fields[0]
            } else {
                fields[3]
            };
            return Ok(format!("{kind} connected to {name}"));
        }
    }
    Ok("No active network connection".into())
}

fn temperature() -> Result<String, ExecError> {
    let Ok(entries) = fs::read_dir("/sys/class/thermal") else {
        return Ok("No temperature source available".into());
    };
    let mut hottest: Option<(i64, String)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("thermal_zone"))
        {
            continue;
        }
        let Ok(temp) = fs::read_to_string(path.join("temp")) else {
            continue;
        };
        let Ok(millidegrees) = temp.trim().parse::<i64>() else {
            continue;
        };
        let label = fs::read_to_string(path.join("type"))
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "thermal".into());
        if hottest.as_ref().is_none_or(|(best, _)| millidegrees > *best) {
            hottest = Some((millidegrees, label));
        }
    }
    match hottest {
        Some((millidegrees, label)) => Ok(format!(
            "{} {} °C",
            title_case(&label),
            millidegrees / 1000
        )),
        None => Ok("No temperature sensor available".into()),
    }
}

fn title_case(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => "Thermal".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_are_human() {
        assert_eq!(human_duration(45), "45s");
        assert_eq!(human_duration(90), "1m");
        assert_eq!(human_duration(3_600 + 120), "1h 2m");
        assert_eq!(human_duration(86_400 + 3_600), "1d 1h");
    }

    #[test]
    fn summaries_keep_the_newest_lines() {
        assert_eq!(summarize("a\nb\nc\nd\ne\n", 3), "c · d · e");
    }

    #[test]
    fn unknown_probes_fail_cleanly() {
        assert!(run("probe.nope").is_err());
    }

}
