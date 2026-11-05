//! Live answers about this computer, read from procfs, sysfs and fixed
//! system tools. Nothing here takes text from the user; every handler is a
//! closed function over a named source.

use std::path::Path;
use std::process::Command;

use sam_core::engine::{Mode, Provider, Query};
use sam_core::intent::{Intent, Source};
use sam_core::rank::{Candidate, Kind, Scorer};

struct Topic {
    id: &'static str,
    title: &'static str,
    icon: &'static str,
    keywords: &'static [&'static str],
}

const TOPICS: &[Topic] = &[
    Topic {
        id: "memory",
        title: "Memory",
        icon: "utilities-system-monitor-symbolic",
        keywords: &[
            "ram",
            "memory",
            "memory usage",
            "what is using my ram",
            "what is eating my memory",
            "memory consumers",
            "why is memory usage high",
        ],
    },
    Topic {
        id: "cpu",
        title: "CPU",
        icon: "utilities-system-monitor-symbolic",
        keywords: &["cpu", "processor", "load", "system load", "load average"],
    },
    Topic {
        id: "processes",
        title: "Processes",
        icon: "utilities-system-monitor-symbolic",
        keywords: &[
            "top processes",
            "processes",
            "running processes",
            "what is using cpu",
            "cpu consumers",
            "busy processes",
        ],
    },
    Topic {
        id: "disk",
        title: "Disk",
        icon: "drive-harddisk-symbolic",
        keywords: &[
            "disk",
            "disk space",
            "storage",
            "free space",
            "how much space",
            "how much disk",
        ],
    },
    Topic {
        id: "uptime",
        title: "Uptime",
        icon: "preferences-system-time-symbolic",
        keywords: &[
            "uptime",
            "how long has this computer been on",
            "how long has this machine been on",
        ],
    },
    Topic {
        id: "battery",
        title: "Battery",
        icon: "battery-good-symbolic",
        keywords: &["battery", "charge", "power level", "battery level"],
    },
    Topic {
        id: "network",
        title: "Network",
        icon: "network-wireless-symbolic",
        keywords: &[
            "network",
            "wifi",
            "wi-fi",
            "internet",
            "ip address",
            "connected to",
            "which network",
        ],
    },
    Topic {
        id: "bluetooth",
        title: "Bluetooth",
        icon: "bluetooth-active-symbolic",
        keywords: &["bluetooth", "bluetooth on", "bluetooth status"],
    },
    Topic {
        id: "gpu",
        title: "GPU",
        icon: "video-display-symbolic",
        keywords: &["gpu", "graphics", "graphics card", "video card", "graphics adapter"],
    },
    Topic {
        id: "display",
        title: "Display",
        icon: "video-display-symbolic",
        keywords: &["display", "monitor", "screen", "resolution", "screen resolution"],
    },
    Topic {
        id: "audio",
        title: "Audio",
        icon: "audio-speakers-symbolic",
        keywords: &["audio", "audio output", "sound output", "speaker", "speakers", "sound device"],
    },
    Topic {
        id: "temperature",
        title: "Temperature",
        icon: "weather-clear-symbolic",
        keywords: &["temperature", "how hot", "thermal", "cpu temp", "overheating", "heat"],
    },
    Topic {
        id: "system",
        title: "System",
        icon: "computer-symbolic",
        keywords: &[
            "operating system",
            "os version",
            "kernel",
            "hostname",
            "fedora version",
            "what os",
            "which os",
        ],
    },
];

struct Answer {
    title: String,
    subtitle: String,
    copy: String,
}

fn score(query: &Query, topic: &Topic) -> f32 {
    let subject = query.subject.to_lowercase();
    let words = &query.words;
    let mut best = 0.0f32;
    for phrase in topic.keywords {
        if subject == *phrase {
            best = best.max(1.0);
        } else if subject.contains(phrase) {
            best = best.max(0.96);
        } else {
            let parts: Vec<&str> = phrase.split_whitespace().collect();
            if parts.len() > 1
                && parts
                    .iter()
                    .all(|p| words.iter().any(|w| w == p || w.starts_with(p)))
            {
                best = best.max(0.92);
            } else if parts.len() == 1
                && words
                    .iter()
                    .any(|w| w == parts[0] || w.starts_with(parts[0]))
            {
                best = best.max(0.82);
            }
        }
    }
    best
}

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_owned())
}

fn command(args: &[&str]) -> Option<String> {
    let out = Command::new(args[0]).args(&args[1..]).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn human_kb(kb: u64) -> String {
    let bytes = kb as f64 * 1024.0;
    if bytes >= 1_099_511_627_776.0 {
        format!("{:.1} TB", bytes / 1_099_511_627_776.0)
    } else if bytes >= 1_073_741_824.0 {
        format!("{:.1} GB", bytes / 1_073_741_824.0)
    } else if bytes >= 1_048_576.0 {
        format!("{:.0} MB", bytes / 1_048_576.0)
    } else {
        format!("{:.0} KB", bytes / 1024.0)
    }
}

fn meminfo() -> Option<(u64, u64)> {
    let text = read("/proc/meminfo")?;
    let mut total = 0;
    let mut available = 0;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("MemTotal:") => total = parts.next()?.parse().ok()?,
            Some("MemAvailable:") => available = parts.next()?.parse().ok()?,
            _ => {}
        }
    }
    (total > 0).then_some((total, available))
}

fn top_by(by_mem: bool, count: usize) -> Vec<(String, String)> {
    let sort = if by_mem { "-rss" } else { "-%cpu" };
    let Some(text) = command(&["ps", "-eo", "comm=,rss=,%cpu=", &format!("--sort={sort}")])
    else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next()?.to_owned();
            let rss: u64 = parts.next()?.parse().ok()?;
            let cpu: f32 = parts.next()?.parse().ok()?;
            let value = if by_mem {
                human_kb(rss)
            } else {
                format!("{cpu:.1}%")
            };
            Some((name, value))
        })
        .filter(|(name, _)| name != "ps" && name != "comm")
        .take(count)
        .collect()
}

fn answer(id: &str) -> Option<Answer> {
    match id {
        "memory" => {
            let (total, available) = meminfo()?;
            let used = total.saturating_sub(available);
            let percent = used as f64 / total as f64 * 100.0;
            let tops = top_by(true, 4)
                .into_iter()
                .map(|(n, v)| format!("{n} {v}"))
                .collect::<Vec<_>>()
                .join(" · ");
            Some(Answer {
                title: format!(
                    "RAM: {} used of {} ({percent:.0}%)",
                    human_kb(used),
                    human_kb(total)
                ),
                subtitle: if tops.is_empty() {
                    format!("{} available", human_kb(available))
                } else {
                    format!("Top: {tops}")
                },
                copy: format!(
                    "RAM: {} used of {} ({} available, {percent:.0}% used)\nTop: {tops}",
                    human_kb(used),
                    human_kb(total),
                    human_kb(available)
                ),
            })
        }
        "cpu" => {
            let load = read("/proc/loadavg")?;
            let mut f = load.split_whitespace();
            let (a, b, c) = (f.next()?, f.next()?, f.next()?);
            let threads = std::thread::available_parallelism().map_or(0, |n| n.get());
            Some(Answer {
                title: format!("CPU load: {a}, {b}, {c}"),
                subtitle: format!("{threads} threads · 1, 5 and 15 minute averages"),
                copy: format!("CPU load: {a} (1m), {b} (5m), {c} (15m) across {threads} threads"),
            })
        }
        "processes" => {
            let cpu = top_by(false, 5)
                .into_iter()
                .map(|(n, v)| format!("{n} {v}"))
                .collect::<Vec<_>>()
                .join(" · ");
            let mem = top_by(true, 5)
                .into_iter()
                .map(|(n, v)| format!("{n} {v}"))
                .collect::<Vec<_>>()
                .join(" · ");
            Some(Answer {
                title: "Busiest processes".into(),
                subtitle: format!("CPU: {cpu}"),
                copy: format!("Top CPU: {cpu}\nTop memory: {mem}"),
            })
        }
        "disk" => {
            let home = std::ffi::CString::new(glib::home_dir().to_string_lossy().as_bytes()).ok()?;
            let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
            let rc = unsafe { libc::statvfs(home.as_ptr(), &mut stat) };
            if rc != 0 {
                return None;
            }
            let total = stat.f_blocks as u64 * stat.f_frsize as u64;
            let free = stat.f_bavail as u64 * stat.f_frsize as u64;
            let used = total.saturating_sub(free);
            let percent = used as f64 / total as f64 * 100.0;
            Some(Answer {
                title: format!(
                    "Disk: {} free of {} ({percent:.0}% used)",
                    human_bytes(free),
                    human_bytes(total)
                ),
                subtitle: format!("{} used on the home filesystem", human_bytes(used)),
                copy: format!(
                    "Disk: {} free of {} ({percent:.0}% used), {} used",
                    human_bytes(free),
                    human_bytes(total),
                    human_bytes(used)
                ),
            })
        }
        "uptime" => {
            let text = read("/proc/uptime")?;
            let seconds: f64 = text.split_whitespace().next()?.parse().ok()?;
            let days = (seconds / 86_400.0).floor() as u64;
            let hours = ((seconds % 86_400.0) / 3_600.0).floor() as u64;
            let minutes = ((seconds % 3_600.0) / 60.0).floor() as u64;
            let line = if days > 0 {
                format!("{days} days, {hours} hours, {minutes} minutes")
            } else {
                format!("{hours} hours, {minutes} minutes")
            };
            Some(Answer {
                title: format!("Uptime: {line}"),
                subtitle: "Since the last boot".into(),
                copy: format!("Uptime: {line}"),
            })
        }
        "battery" => {
            let dir = std::fs::read_dir("/sys/class/power_supply")
                .ok()?
                .flatten()
                .map(|e| e.path())
                .find(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("BAT"))
                })?;
            let capacity = read(dir.join("capacity").to_str()?)?.parse::<u64>().ok()?;
            let status = read(dir.join("status").to_str()?).unwrap_or_default();
            let energy_now = read(dir.join("energy_now").to_str()?).and_then(|v| v.parse::<u64>().ok());
            let power_now = read(dir.join("power_now").to_str()?).and_then(|v| v.parse::<u64>().ok());
            let remaining = match (energy_now, power_now) {
                (Some(e), Some(p)) if p > 0 => {
                    let minutes = e * 60 / p;
                    format!(" · about {}h {:02}m left", minutes / 60, minutes % 60)
                }
                _ => String::new(),
            };
            Some(Answer {
                title: format!("Battery: {capacity}% ({status}){remaining}"),
                subtitle: "From the battery's own sysfs interface".into(),
                copy: format!("Battery: {capacity}% ({status}){remaining}"),
            })
        }
        "network" => {
            let text = command(&[
                "nmcli",
                "-t",
                "-f",
                "DEVICE,TYPE,STATE,CONNECTION",
                "device",
            ]);
            let active = text.as_deref().and_then(|text| {
                text.lines().find_map(|line| {
                    let mut parts = line.split(':');
                    let device = parts.next()?;
                    let kind = parts.next()?;
                    let state = parts.next()?;
                    let connection = parts.next().unwrap_or("");
                    (state == "connected" && device != "lo").then(|| {
                        if connection.is_empty() {
                            format!("{kind} ({device})")
                        } else {
                            format!("{connection} · {kind} ({device})")
                        }
                    })
                })
            });
            let name = active.unwrap_or_else(|| {
                std::fs::read_dir("/sys/class/net")
                    .ok()
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter_map(|e| e.file_name().into_string().ok())
                    .find(|n| n != "lo")
                    .unwrap_or_else(|| "no active connection".into())
            });
            Some(Answer {
                title: format!("Network: {name}"),
                subtitle: "From NetworkManager".into(),
                copy: format!("Network: {name}"),
            })
        }
        "bluetooth" => {
            let text = command(&["bluetoothctl", "show"]);
            let on = text
                .as_deref()
                .and_then(|t| t.lines().find(|l| l.starts_with("Powered:")))
                .map_or(false, |l| l.contains("yes"));
            let name = text
                .as_deref()
                .and_then(|t| t.lines().find(|l| l.starts_with("Controller")))
                .and_then(|l| l.split_whitespace().nth(2))
                .unwrap_or("");
            let line = if on {
                format!("Bluetooth: on{}", if name.is_empty() { String::new() } else { format!(" · {name}") })
            } else {
                "Bluetooth: off".into()
            };
            Some(Answer {
                title: line.clone(),
                subtitle: "From BlueZ".into(),
                copy: line,
            })
        }
        "gpu" => {
            let mut found = None;
            for entry in std::fs::read_dir("/sys/class/drm").ok()?.flatten() {
                let path = entry.path().join("device/uevent");
                let Some(text) = read(path.to_str()?) else {
                    continue;
                };
                let mut driver = String::new();
                let mut pci = String::new();
                for line in text.lines() {
                    if let Some(v) = line.strip_prefix("DRIVER=") {
                        driver = v.to_owned();
                    }
                    if let Some(v) = line.strip_prefix("PCI_ID=") {
                        pci = v.to_owned();
                    }
                }
                if !pci.is_empty() {
                    let vendor = match pci.get(..4).unwrap_or("") {
                        "1002" => "AMD",
                        "8086" => "Intel",
                        "10de" => "NVIDIA",
                        _ => "GPU",
                    };
                    found = Some(format!("{vendor} GPU · {pci} · driver {driver}"));
                    break;
                }
            }
            let line = found.unwrap_or_else(|| "No discrete GPU found in sysfs".into());
            Some(Answer {
                title: line.clone(),
                subtitle: "From the DRM device".into(),
                copy: line,
            })
        }
        "display" => {
            let mut found = None;
            for entry in std::fs::read_dir("/sys/class/drm").ok()?.flatten() {
                let path = entry.path();
                if read(path.join("status").to_str()?).as_deref() != Some("connected") {
                    continue;
                }
                let mode = read(path.join("modes").to_str()?)
                    .and_then(|m| m.lines().next().map(str::to_owned));
                if let Some(mode) = mode {
                    found = Some(format!("{} · {mode}", entry.file_name().to_string_lossy()));
                    break;
                }
            }
            let line = found.unwrap_or_else(|| "No connected display found in sysfs".into());
            Some(Answer {
                title: format!("Display: {line}"),
                subtitle: "From the DRM connector".into(),
                copy: format!("Display: {line}"),
            })
        }
        "audio" => {
            let text = command(&["wpctl", "inspect", "@DEFAULT_AUDIO_SINK@"]);
            let line = text
                .as_deref()
                .and_then(|t| {
                    t.lines()
                        .find(|l| l.contains("node.description"))
                        .map(|l| l.split('=').nth(1).unwrap_or(l).trim().trim_matches('"').to_owned())
                })
                .unwrap_or_else(|| "No default audio output".into());
            Some(Answer {
                title: format!("Audio output: {line}"),
                subtitle: "From PipeWire".into(),
                copy: format!("Audio output: {line}"),
            })
        }
        "temperature" => {
            let mut hottest: Option<(f64, String)> = None;
            for entry in std::fs::read_dir("/sys/class/thermal").ok()?.flatten() {
                let path = entry.path();
                let Some(zone) = read(path.join("type").to_str()?) else {
                    continue;
                };
                let Some(raw) = read(path.join("temp").to_str()?) else {
                    continue;
                };
                let Ok(milli) = raw.parse::<f64>() else {
                    continue;
                };
                let temp = milli / 1000.0;
                if hottest.as_ref().is_none_or(|(t, _)| temp > *t) {
                    hottest = Some((temp, zone));
                }
            }
            let (temp, zone) = hottest?;
            Some(Answer {
                title: format!("Temperature: {temp:.0}°C · {zone}"),
                subtitle: "Hottest thermal zone".into(),
                copy: format!("Temperature: {temp:.0}°C ({zone})"),
            })
        }
        "system" => {
            let os = read("/etc/os-release")
                .and_then(|t| {
                    t.lines()
                        .find(|l| l.starts_with("PRETTY_NAME="))
                        .map(|l| l.split('=').nth(1).unwrap_or(l).trim_matches('"').to_owned())
                })
                .unwrap_or_else(|| "Linux".into());
            let kernel = command(&["uname", "-sr"]).unwrap_or_default();
            let host = read("/proc/sys/kernel/hostname").unwrap_or_default();
            Some(Answer {
                title: format!("{os} · {kernel}"),
                subtitle: format!("Hostname {host}"),
                copy: format!("{os}\nKernel {kernel}\nHostname {host}"),
            })
        }
        _ => None,
    }
}

fn human_bytes(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1_099_511_627_776.0 {
        format!("{:.1} TB", b / 1_099_511_627_776.0)
    } else if b >= 1_073_741_824.0 {
        format!("{:.1} GB", b / 1_073_741_824.0)
    } else if b >= 1_048_576.0 {
        format!("{:.0} MB", b / 1_048_576.0)
    } else {
        format!("{:.0} KB", b / 1024.0)
    }
}

/// The live-system provider. It reads only when a topic matches.
pub struct SystemInfo;

impl Provider for SystemInfo {
    fn provide(&self, query: &Query, _: &Scorer, out: &mut Vec<Candidate>) {
        if !matches!(query.mode, Mode::Normal | Mode::Open) || query.subject.trim().is_empty() {
            return;
        }
        for topic in TOPICS {
            let score = score(query, topic);
            if score < 0.8 {
                continue;
            }
            let Some(a) = answer(topic.id) else {
                continue;
            };
            let key = format!("info:{}", topic.id);
            out.push(Candidate {
                title: a.title,
                subtitle: a.subtitle,
                kind: Kind::Info,
                icon: topic.icon.into(),
                score,
                key,
                intent: Intent::new("info.copy", Source::Provider)
                    .arg("title", topic.title)
                    .arg("text", a.copy),
                alt: None,
            });
            break;
        }
    }
}

/// Installed manuals and documentation, indexed once by name.
fn docs_index() -> &'static [(String, String)] {
    static INDEX: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
    INDEX.get_or_init(|| {
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir("/usr/share/man") {
            for section in entries.flatten().filter(|e| e.path().is_dir()) {
                let Ok(pages) = std::fs::read_dir(section.path()) else {
                    continue;
                };
                let section_name = section.file_name().to_string_lossy().to_string();
                for page in pages.flatten() {
                    let name = page.file_name().to_string_lossy().to_string();
                    let stem = name
                        .trim_end_matches(".gz")
                        .trim_end_matches(".xz")
                        .trim_end_matches(".bz2");
                    let page = stem
                        .rsplit_once('.')
                        .filter(|(_, n)| n.chars().all(|c| c.is_ascii_digit()))
                        .map_or(stem, |(page, _)| page);
                    out.push((page.to_owned(), format!("Manual · section {section_name}")));
                }
            }
        }
        if let Ok(entries) = std::fs::read_dir("/usr/share/doc") {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    out.push((
                        entry.file_name().to_string_lossy().to_string(),
                        "Installed documentation".into(),
                    ));
                }
            }
        }
        out
    })
}

/// Local-docs provider: names only, no full-text index in this slice.
pub struct Docs;

impl Provider for Docs {
    fn provide(&self, query: &Query, _: &Scorer, out: &mut Vec<Candidate>) {
        if query.mode != Mode::Normal || query.subject.trim().chars().count() < 3 {
            return;
        }
        const STOP: &[&str] = &[
            "man", "manual", "page", "pages", "doc", "docs", "documentation", "help", "for",
            "about", "of", "local",
        ];
        let words: Vec<&str> = query
            .words
            .iter()
            .map(String::as_str)
            .filter(|w| !STOP.contains(w))
            .collect();
        if words.is_empty() {
            return;
        }
        let needle = words.join(" ");
        let mut found = 0;
        for (name, subtitle) in docs_index() {
            let lower = name.to_lowercase();
            if !lower.contains(&needle) {
                continue;
            }
            let score = if lower == needle {
                0.80
            } else if lower.starts_with(&needle) {
                0.75
            } else {
                0.70
            };
            out.push(Candidate {
                title: name.clone(),
                subtitle: subtitle.clone(),
                kind: Kind::Doc,
                icon: "help-contents-symbolic".into(),
                score: score - found as f32 * 0.005,
                key: format!("doc:{name}"),
                intent: Intent::new("doc.open", Source::Provider).arg("page", name.as_str()),
                alt: None,
            });
            found += 1;
            if found >= 5 {
                break;
            }
        }
    }
}

/// A small curated map from everyday names to GNOME Settings panels.
pub struct Settings;

const PANELS: &[(&[&str], &str, &str)] = &[
    (&["night light", "night mode", "color", "colour", "warmth"], "Color", "gnome-color-panel.desktop"),
    (&["keyboard", "shortcut", "shortcuts"], "Keyboard", "gnome-keyboard-panel.desktop"),
    (&["mouse", "touchpad", "pointer"], "Mouse & Touchpad", "gnome-mouse-panel.desktop"),
    (&["display", "monitor", "resolution", "scale"], "Displays", "gnome-display-panel.desktop"),
    (&["sound", "audio", "speaker", "microphone"], "Sound", "gnome-sound-panel.desktop"),
    (&["power", "battery", "suspend", "sleep"], "Power", "gnome-power-panel.desktop"),
    (&["network", "wifi", "wi-fi", "proxy", "vpn"], "Network", "gnome-network-panel.desktop"),
    (&["bluetooth"], "Bluetooth", "gnome-bluetooth-panel.desktop"),
    (&["privacy", "security", "screen lock", "lock screen"], "Privacy", "gnome-privacy-panel.desktop"),
    (&["accessibility", "zoom", "contrast"], "Accessibility", "gnome-universal-access-panel.desktop"),
    (&["region", "language", "formats"], "Region & Language", "gnome-region-panel.desktop"),
    (&["printer", "printers"], "Printers", "gnome-printers-panel.desktop"),
    (&["user", "users", "account", "accounts"], "Users", "gnome-users-panel.desktop"),
    (&["about", "system info", "system information"], "About", "gnome-about-panel.desktop"),
    (&["multitasking", "workspaces"], "Multitasking", "gnome-multitasking-panel.desktop"),
    (&["wellbeing", "screen time"], "Wellbeing", "gnome-wellbeing-panel.desktop"),
    (&["search"], "Search", "gnome-search-panel.desktop"),
];

impl Provider for Settings {
    fn provide(&self, query: &Query, _: &Scorer, out: &mut Vec<Candidate>) {
        if !matches!(query.mode, Mode::Normal | Mode::Find) {
            return;
        }
        let subject = query.subject.to_lowercase();
        if !subject.contains("setting") && !subject.contains("preference") {
            return;
        }
        for (keywords, title, app_id) in PANELS {
            if !keywords.iter().any(|k| subject.contains(k)) {
                continue;
            }
            out.push(Candidate {
                title: format!("{title} settings"),
                subtitle: "Open the GNOME Settings panel".into(),
                kind: Kind::Setting,
                icon: "preferences-system-symbolic".into(),
                score: 0.9,
                key: format!("setting:{app_id}"),
                intent: Intent::new("app.launch", Source::Provider).arg("app_id", *app_id),
                alt: None,
            });
            break;
        }
    }
}

/// Whether a path exists, for the setup diagnostics.
pub fn exists(path: &str) -> bool {
    Path::new(path).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sam_core::engine::Engine;
    use sam_core::rank::Scorer;

    fn provide(query: &str) -> Vec<Candidate> {
        let engine = Engine::new();
        let q = engine.parse(query, &crate::host::LinuxHost);
        let mut out = Vec::new();
        SystemInfo.provide(&q, &Scorer::new(), &mut out);
        out
    }

    #[test]
    fn memory_and_disk_are_answered() {
        let memory = provide("what is using my RAM");
        assert_eq!(memory.len(), 1, "{memory:#?}");
        assert!(memory[0].title.starts_with("RAM:"), "{}", memory[0].title);
        assert_eq!(memory[0].intent.action, "info.copy");

        let disk = provide("how much disk space is free");
        assert_eq!(disk.len(), 1, "{disk:#?}");
        assert!(disk[0].title.starts_with("Disk:"), "{}", disk[0].title);
    }

    #[test]
    fn the_settings_map_finds_a_panel() {
        let engine = Engine::new();
        let q = engine.parse("where is night light settings", &crate::host::LinuxHost);
        let mut out = Vec::new();
        Settings.provide(&q, &Scorer::new(), &mut out);
        assert_eq!(out.len(), 1, "{out:#?}");
        assert_eq!(out[0].intent.action, "app.launch");
        assert_eq!(
            out[0].intent.text("app_id"),
            Some("gnome-color-panel.desktop")
        );
    }
}
