//! Live answers about this computer, read from procfs, sysfs and fixed
//! system tools. Nothing here takes text from the user; every handler is a
//! closed function over a named source.

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::process::Command;

use sam_core::engine::{Mode, Provider, Query};
use sam_core::intent::{Intent, Source};
use sam_core::rank::{Candidate, Kind, Scorer};
use sam_core::text;

struct Topic {
    id: &'static str,
    title: &'static str,
    icon: &'static str,
    /// Natural phrasings in normalised form: lowercase, punctuation trimmed
    /// and filler words removed, exactly as [`text::normalize`] produces
    /// them. A test keeps this true.
    aliases: &'static [&'static str],
}

const TOPICS: &[Topic] = &[
    Topic {
        id: "brightness",
        title: "Brightness",
        icon: "display-brightness-symbolic",
        aliases: &[
            "brightness",
            "screen brightness",
            "display brightness",
            "backlight",
            "how bright",
            "how bright is screen",
            "how bright is display",
            "what is brightness",
            "what is screen brightness",
            "is screen dim",
            "is screen bright",
        ],
    },
    Topic {
        id: "volume",
        title: "Volume",
        icon: "audio-volume-high-symbolic",
        aliases: &[
            "volume",
            "volume level",
            "sound level",
            "audio level",
            "how loud",
            "how loud is sound",
            "is sound muted",
            "is audio muted",
            "muted",
            "mute status",
            "speaker volume",
            "what is volume",
        ],
    },
    Topic {
        id: "battery",
        title: "Battery",
        icon: "battery-good-symbolic",
        aliases: &[
            "battery",
            "battery level",
            "battery status",
            "battery health",
            "battery percentage",
            "charge",
            "charging",
            "am charging",
            "is charging",
            "plugged in",
            "on battery",
            "ac power",
            "power",
            "power status",
            "power level",
            "how much battery",
            "what is battery at",
        ],
    },
    Topic {
        id: "power_profile",
        title: "Power Profile",
        icon: "battery-good-symbolic",
        aliases: &[
            "power profile",
            "power mode",
            "power saving",
            "power saver",
            "performance mode",
            "balanced mode",
            "battery saver",
            "which power profile",
            "current power profile",
        ],
    },
    Topic {
        id: "network",
        title: "Network",
        icon: "network-wireless-symbolic",
        aliases: &[
            "network",
            "networking",
            "wifi",
            "wi-fi",
            "wireless",
            "internet",
            "online",
            "am online",
            "connected to",
            "which network",
            "network status",
            "ip",
            "ip address",
            "ipv4",
            "ipv6",
            "what is ip",
            "what is ip address",
            "gateway",
            "dns",
            "ssid",
            "wifi name",
            "is wifi on",
            "wifi status",
            "ethernet",
            "connection",
            "which wifi",
        ],
    },
    Topic {
        id: "bluetooth",
        title: "Bluetooth",
        icon: "bluetooth-active-symbolic",
        aliases: &[
            "bluetooth",
            "bluetooth status",
            "is bluetooth on",
            "bluetooth on",
            "bluetooth devices",
            "what bluetooth devices are connected",
            "paired devices",
            "connected devices",
            "bluetooth connection",
        ],
    },
    Topic {
        id: "time",
        title: "Time & Date",
        icon: "preferences-system-time-symbolic",
        aliases: &[
            "time",
            "what time is it",
            "current time",
            "date",
            "what date is it",
            "today date",
            "todays date",
            "what day is it",
            "day",
            "timezone",
            "time zone",
            "clock",
            "what is date",
        ],
    },
    Topic {
        id: "cpu",
        title: "CPU",
        icon: "utilities-system-monitor-symbolic",
        aliases: &[
            "cpu",
            "processor",
            "cpu model",
            "what cpu",
            "what cpu do have",
            "which cpu",
            "what processor",
            "cpu info",
            "cpu information",
            "how many cores",
            "cores",
            "threads",
            "cpu speed",
            "cpu frequency",
        ],
    },
    Topic {
        id: "cpu_load",
        title: "CPU Load",
        icon: "utilities-system-monitor-symbolic",
        aliases: &[
            "cpu load",
            "load average",
            "system load",
            "cpu usage",
            "how busy is cpu",
            "is cpu busy",
            "cpu busy",
            "load",
        ],
    },
    Topic {
        id: "processes",
        title: "Processes",
        icon: "utilities-system-monitor-symbolic",
        aliases: &[
            "processes",
            "process",
            "running processes",
            "top processes",
            "what is using cpu",
            "what is eating cpu",
            "cpu consumers",
            "busy processes",
            "what is running",
        ],
    },
    Topic {
        id: "memory",
        title: "Memory",
        icon: "utilities-system-monitor-symbolic",
        aliases: &[
            "memory",
            "ram",
            "memory info",
            "ram info",
            "how much ram",
            "how much memory",
            "memory usage",
            "ram usage",
            "free memory",
            "available memory",
            "swap",
            "how much swap",
            "what is using memory",
            "what is eating memory",
            "memory consumers",
            "top memory",
            "biggest memory consumers",
            "what is using ram",
            "why is memory usage high",
            "memory usage high",
            "which apps are using memory",
        ],
    },
    Topic {
        id: "disk",
        title: "Disk",
        icon: "drive-harddisk-symbolic",
        aliases: &[
            "disk",
            "disk space",
            "storage",
            "storage space",
            "free space",
            "how much space",
            "how much disk",
            "how much storage",
            "disk usage",
            "how full is disk",
            "filesystem",
            "filesystems",
            "partitions",
            "drives",
        ],
    },
    Topic {
        id: "display",
        title: "Display",
        icon: "video-display-symbolic",
        aliases: &[
            "display",
            "displays",
            "monitor",
            "monitors",
            "screen",
            "screen resolution",
            "resolution",
            "refresh rate",
            "how many displays",
            "connected displays",
            "external monitor",
            "what is resolution",
        ],
    },
    Topic {
        id: "audio",
        title: "Audio",
        icon: "audio-speakers-symbolic",
        aliases: &[
            "audio",
            "audio output",
            "sound output",
            "speaker",
            "speakers",
            "sound device",
            "audio device",
            "microphone",
            "mic",
            "audio input",
            "what speakers",
            "what audio output",
            "sound card",
        ],
    },
    Topic {
        id: "temperature",
        title: "Temperature",
        icon: "weather-clear-symbolic",
        aliases: &[
            "temperature",
            "how hot",
            "how hot is cpu",
            "thermal",
            "cpu temp",
            "cpu temperature",
            "gpu temp",
            "overheating",
            "heat",
            "fan",
            "fans",
        ],
    },
    Topic {
        id: "gpu",
        title: "GPU",
        icon: "video-display-symbolic",
        aliases: &[
            "gpu",
            "graphics",
            "graphics card",
            "video card",
            "graphics adapter",
            "graphics processor",
            "what gpu",
            "which gpu",
            "what graphics",
        ],
    },
    Topic {
        id: "uptime",
        title: "Uptime",
        icon: "preferences-system-time-symbolic",
        aliases: &[
            "uptime",
            "how long has this computer been on",
            "how long has this machine been on",
            "how long has it been on",
            "when did boot",
            "last boot",
            "boot time",
        ],
    },
    Topic {
        id: "system",
        title: "System",
        icon: "computer-symbolic",
        aliases: &[
            "system",
            "operating system",
            "os",
            "os version",
            "kernel",
            "hostname",
            "fedora version",
            "what os",
            "which os",
            "desktop",
            "desktop environment",
            "session",
            "session type",
            "wayland",
            "x11",
            "architecture",
            "what system",
            "computer info",
            "about this computer",
            "what desktop am running",
        ],
    },
];

struct Answer {
    title: String,
    subtitle: String,
    copy: String,
}

/// How well `words` names one of `aliases`. The aliases are already
/// normalised, so a whole question can match exactly.
fn phrase_score(words: &[String], aliases: &[&str]) -> f32 {
    let mut best = 0.0f32;
    for alias in aliases {
        let parts: Vec<&str> = alias.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        let score = if words.len() == parts.len()
            && words.iter().zip(parts.iter()).all(|(w, p)| w.as_str() == *p)
        {
            1.0
        } else if parts.len() > 1
            && words
                .windows(parts.len())
                .any(|window| window.iter().zip(parts.iter()).all(|(w, p)| w.as_str() == *p))
        {
            0.97
        } else if parts
            .iter()
            .all(|p| words.iter().any(|w| w.as_str() == *p || w.starts_with(*p)))
        {
            if parts.len() == 1 { 0.82 } else { 0.91 }
        } else {
            0.0
        };
        best = best.max(score);
    }
    best
}

fn score(query: &Query, topic: &Topic) -> f32 {
    phrase_score(&query.words, topic.aliases)
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

fn meminfo() -> Option<(u64, u64, u64, u64)> {
    let text = read("/proc/meminfo")?;
    let mut total = 0;
    let mut available = 0;
    let mut swap_total = 0;
    let mut swap_free = 0;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("MemTotal:") => total = parts.next()?.parse().ok()?,
            Some("MemAvailable:") => available = parts.next()?.parse().ok()?,
            Some("SwapTotal:") => swap_total = parts.next()?.parse().ok()?,
            Some("SwapFree:") => swap_free = parts.next()?.parse().ok()?,
            _ => {}
        }
    }
    (total > 0).then_some((total, available, swap_total, swap_free))
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

fn battery_dir() -> Option<PathBuf> {
    std::fs::read_dir("/sys/class/power_supply")
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("BAT"))
        })
}

fn ac_online() -> Option<bool> {
    for entry in std::fs::read_dir("/sys/class/power_supply").ok()?.flatten() {
        let path = entry.path();
        let Some(kind) = read(path.join("type").to_str()?) else {
            continue;
        };
        if kind == "Mains" {
            return read(path.join("online").to_str()?).map(|v| v.trim() == "1");
        }
    }
    None
}

fn usage(path: &str) -> Option<(u64, u64)> {
    let path = CString::new(path).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(path.as_ptr(), &mut stat) };
    if rc != 0 {
        return None;
    }
    let total = stat.f_blocks as u64 * stat.f_frsize as u64;
    let free = stat.f_bavail as u64 * stat.f_frsize as u64;
    Some((total, free))
}

fn audio_node(target: &str) -> Option<String> {
    let text = command(&["wpctl", "inspect", target])?;
    text.lines()
        .find(|line| line.trim_start().starts_with("node.description"))
        .and_then(|line| line.split_once('='))
        .map(|(_, value)| value.trim().trim_matches('"').to_owned())
        .filter(|value| !value.is_empty())
}

fn brightness_answer() -> Option<Answer> {
    let (device, current, max) = crate::system::brightness().ok()?;
    let percent = f64::from(current) / f64::from(max) * 100.0;
    Some(Answer {
        title: format!("Screen brightness: {percent:.0}%"),
        subtitle: format!("{device} · try `brightness up` or `brightness down`"),
        copy: format!(
            "Screen brightness: {percent:.0}% ({device}, {current}/{max})"
        ),
    })
}

fn volume_answer() -> Option<Answer> {
    let (level, muted) = crate::system::volume().ok()?;
    let sink = audio_node("@DEFAULT_AUDIO_SINK@");
    let state = if muted { "muted" } else { "not muted" };
    let title = format!("Volume: {level:.0}% · {state}");
    let subtitle = sink
        .as_ref()
        .map(|name| format!("Output: {name}"))
        .unwrap_or_else(|| "From PipeWire".into());
    Some(Answer {
        copy: format!(
            "Volume: {level:.0}% ({state}){}",
            sink.as_ref()
                .map(|name| format!("\nOutput: {name}"))
                .unwrap_or_default()
        ),
        title,
        subtitle,
    })
}

fn battery_answer() -> Option<Answer> {
    let ac = ac_online();
    let profile = crate::system::power_profile_current().ok();
    let Some(dir) = battery_dir() else {
        if ac.is_none() && profile.is_none() {
            return None;
        }
        let title = match ac {
            Some(true) => "Power: AC connected".to_owned(),
            Some(false) => "Power: no battery · no AC".to_owned(),
            None => "Power: no battery".to_owned(),
        };
        let subtitle = profile
            .as_ref()
            .map(|profile| format!("Power profile {profile}"))
            .unwrap_or_else(|| "From sysfs".into());
        return Some(Answer {
            copy: format!("{title}\n{subtitle}"),
            title,
            subtitle,
        });
    };
    let capacity = read(dir.join("capacity").to_str()?)?.parse::<u64>().ok()?;
    let status = read(dir.join("status").to_str()?).unwrap_or_default();
    let energy_now = read(dir.join("energy_now").to_str()?).and_then(|v| v.parse::<u64>().ok());
    let power_now = read(dir.join("power_now").to_str()?).and_then(|v| v.parse::<u64>().ok());
    let energy_full = read(dir.join("energy_full").to_str()?).and_then(|v| v.parse::<u64>().ok());
    let energy_design = read(dir.join("energy_full_design").to_str()?)
        .and_then(|v| v.parse::<u64>().ok());
    let remaining = match (energy_now, power_now) {
        (Some(energy), Some(power)) if power > 0 => {
            let minutes = energy * 60 / power;
            format!(" · about {}h {:02}m", minutes / 60, minutes % 60)
        }
        _ => String::new(),
    };
    let health = match (energy_full, energy_design) {
        (Some(full), Some(design)) if design > 0 => {
            format!("Battery health {:.0}%", full as f64 / design as f64 * 100.0)
        }
        _ => String::new(),
    };
    let ac = ac.map(|on| if on { "AC connected" } else { "On battery" });
    let mut bits: Vec<String> = Vec::new();
    if status != "Unknown" && !status.is_empty() {
        bits.push(status.clone());
    }
    if let Some(ac) = ac {
        bits.push(ac.to_owned());
    }
    if let Some(profile) = &profile {
        bits.push(format!("power profile {profile}"));
    }
    if !health.is_empty() {
        bits.push(health.clone());
    }
    Some(Answer {
        title: format!("Battery: {capacity}% · {status}{remaining}"),
        subtitle: if bits.is_empty() {
            "From the battery's own sysfs interface".into()
        } else {
            bits.join(" · ")
        },
        copy: format!(
            "Battery: {capacity}% ({status}){remaining}\n{}",
            bits.join(" · ")
        ),
    })
}

fn power_profile_answer() -> Option<Answer> {
    let profile = crate::system::power_profile_current().ok()?;
    Some(Answer {
        title: format!("Power profile: {profile}"),
        subtitle: "From power-profiles-daemon".into(),
        copy: format!("Power profile: {profile}"),
    })
}

fn network_answer() -> Option<Answer> {
    let wifi = crate::system::wifi_on().ok();
    let devices = command(&[
        "nmcli",
        "-t",
        "-f",
        "DEVICE,TYPE,STATE,CONNECTION",
        "device",
    ]);
    let active = devices.as_deref().and_then(|text| {
        text.lines().find_map(|line| {
            let mut parts = line.split(':');
            let device = parts.next()?;
            let kind = parts.next()?;
            let state = parts.next()?;
            let connection = parts.next().unwrap_or("");
            (state == "connected" && device != "lo").then(|| {
                (
                    device.to_owned(),
                    kind.to_owned(),
                    connection.to_owned(),
                )
            })
        })
    });
    let wifi_state = match wifi {
        Some(true) => "Wi-Fi on",
        Some(false) => "Wi-Fi off",
        None => "Wi-Fi unknown",
    };
    let Some((device, kind, connection)) = active else {
        return Some(Answer {
            title: format!("Network: {wifi_state} · no active connection"),
            subtitle: "From NetworkManager".into(),
            copy: format!("Network: {wifi_state}, no active connection"),
        });
    };
    let details = command(&[
        "nmcli",
        "-t",
        "-f",
        "IP4.ADDRESS,IP4.GATEWAY,IP4.DNS",
        "device",
        "show",
        &device,
    ]);
    let mut ip = None;
    let mut gateway = None;
    let mut dns = None;
    for line in details.as_deref().unwrap_or("").lines() {
        if let Some(value) = line.strip_prefix("IP4.ADDRESS[1]:") {
            ip = Some(value.split('/').next().unwrap_or(value).to_owned());
        } else if let Some(value) = line.strip_prefix("IP4.GATEWAY:") {
            gateway = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("IP4.DNS[1]:") {
            dns = Some(value.to_owned());
        }
    }
    let name = if connection.is_empty() {
        format!("{kind} ({device})")
    } else {
        format!("{connection} · {kind} ({device})")
    };
    let title = match &ip {
        Some(ip) => format!("Network: {name} · {ip}"),
        None => format!("Network: {name}"),
    };
    let mut bits = vec![wifi_state.to_owned()];
    if let Some(gateway) = &gateway {
        bits.push(format!("gateway {gateway}"));
    }
    if let Some(dns) = &dns {
        bits.push(format!("DNS {dns}"));
    }
    Some(Answer {
        subtitle: bits.join(" · "),
        copy: format!(
            "{title}\nIP: {}\nGateway: {}\nDNS: {}",
            ip.as_deref().unwrap_or("none"),
            gateway.as_deref().unwrap_or("none"),
            dns.as_deref().unwrap_or("none")
        ),
        title,
    })
}

fn bluetooth_answer() -> Option<Answer> {
    let text = command(&["bluetoothctl", "show"]);
    let on = text
        .as_deref()
        .and_then(|text| text.lines().find(|line| line.starts_with("Powered:")))
        .is_some_and(|line| line.contains("yes"));
    let controller = text
        .as_deref()
        .and_then(|text| text.lines().find(|line| line.starts_with("Controller")))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap_or("")
        .to_owned();
    if !on {
        return Some(Answer {
            title: "Bluetooth: off".into(),
            subtitle: "From BlueZ".into(),
            copy: "Bluetooth: off".into(),
        });
    }
    let devices = command(&["bluetoothctl", "devices", "Connected"])
        .or_else(|| command(&["bluetoothctl", "devices"]));
    let names: Vec<String> = devices
        .as_deref()
        .unwrap_or("")
        .lines()
        .filter_map(|line| line.strip_prefix("Device "))
        .filter_map(|rest| rest.split_once(' ').map(|(_, name)| name.to_owned()))
        .collect();
    let title = if names.is_empty() {
        "Bluetooth: on · no connected devices".to_owned()
    } else {
        format!("Bluetooth: on · {} connected", names.len())
    };
    let subtitle = if names.is_empty() {
        if controller.is_empty() {
            "From BlueZ".into()
        } else {
            format!("Controller {controller}")
        }
    } else {
        names.join(" · ")
    };
    Some(Answer {
        copy: format!("{title}\n{}", names.join("\n")),
        title,
        subtitle,
    })
}

fn time_answer() -> Option<Answer> {
    let now = glib::DateTime::now_local().ok()?;
    let time = now.format("%H:%M").ok()?.to_string();
    let date = now.format("%A, %d %B %Y").ok()?.to_string();
    let zone = now.format("%Z").ok()?.to_string();
    Some(Answer {
        title: format!("Time: {time} · {date}"),
        subtitle: format!("Time zone {zone}"),
        copy: format!("{time} · {date} · {zone}"),
    })
}

fn cpu_answer() -> Option<Answer> {
    let info = read("/proc/cpuinfo")?;
    let model = info
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim() == "model name").then(|| value.trim().to_owned())
        })
        .unwrap_or_else(|| "unknown processor".into());
    let threads = std::thread::available_parallelism().map_or(0, |n| n.get());
    let max_khz = read("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq")
        .or_else(|| read("/sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq"))
        .and_then(|value| value.parse::<u64>().ok());
    let speed = max_khz.map(|khz| format!(" · up to {:.1} GHz", khz as f64 / 1_000_000.0));
    Some(Answer {
        title: format!("CPU: {model}"),
        subtitle: match &speed {
            Some(speed) => format!("{threads} threads{speed}"),
            None => format!("{threads} threads"),
        },
        copy: format!(
            "CPU: {model}\n{threads} threads{}",
            speed.as_deref().unwrap_or("")
        ),
    })
}

fn cpu_load_answer() -> Option<Answer> {
    let load = read("/proc/loadavg")?;
    let mut fields = load.split_whitespace();
    let (a, b, c) = (fields.next()?, fields.next()?, fields.next()?);
    let threads = std::thread::available_parallelism().map_or(0, |n| n.get());
    Some(Answer {
        title: format!("CPU load: {a}, {b}, {c}"),
        subtitle: format!("{threads} threads · 1, 5 and 15 minute averages"),
        copy: format!("CPU load: {a} (1m), {b} (5m), {c} (15m) across {threads} threads"),
    })
}

fn processes_answer() -> Option<Answer> {
    let cpu = top_by(false, 5)
        .into_iter()
        .map(|(name, value)| format!("{name} {value}"))
        .collect::<Vec<_>>()
        .join(" · ");
    let mem = top_by(true, 5)
        .into_iter()
        .map(|(name, value)| format!("{name} {value}"))
        .collect::<Vec<_>>()
        .join(" · ");
    Some(Answer {
        title: "Busiest processes".into(),
        subtitle: format!("CPU: {cpu}"),
        copy: format!("Top CPU: {cpu}\nTop memory: {mem}"),
    })
}

fn memory_answer() -> Option<Answer> {
    let (total, available, swap_total, swap_free) = meminfo()?;
    let used = total.saturating_sub(available);
    let percent = used as f64 / total as f64 * 100.0;
    let tops = top_by(true, 4)
        .into_iter()
        .map(|(name, value)| format!("{name} {value}"))
        .collect::<Vec<_>>()
        .join(" · ");
    let swap = if swap_total > 0 {
        let swap_used = swap_total.saturating_sub(swap_free);
        format!(
            "Swap: {} used of {}",
            human_kb(swap_used),
            human_kb(swap_total)
        )
    } else {
        "No swap".into()
    };
    Some(Answer {
        title: format!(
            "RAM: {} used of {} ({percent:.0}%)",
            human_kb(used),
            human_kb(total)
        ),
        subtitle: if tops.is_empty() {
            format!("{} available · {swap}", human_kb(available))
        } else {
            format!("Top: {tops} · {swap}")
        },
        copy: format!(
            "RAM: {} used of {} ({} available, {percent:.0}% used)\nTop: {tops}\n{swap}",
            human_kb(used),
            human_kb(total),
            human_kb(available)
        ),
    })
}

fn disk_answer() -> Option<Answer> {
    let home = glib::home_dir().to_string_lossy().into_owned();
    let (total, free) = usage(&home)?;
    let used = total.saturating_sub(free);
    let percent = used as f64 / total as f64 * 100.0;
    let mut others = Vec::new();
    if let Some(mounts) = read("/proc/mounts") {
        for line in mounts.lines() {
            let mut parts = line.split_whitespace();
            let Some(device) = parts.next() else { continue };
            let Some(mount) = parts.next() else { continue };
            if !device.starts_with("/dev/") || mount == "/" || mount == "/home" {
                continue;
            }
            let mount = mount.replace("\\040", " ");
            if let Some((total, free)) = usage(&mount)
                && total > 0
            {
                others.push(format!(
                    "{mount}: {:.0}% free",
                    free as f64 / total as f64 * 100.0
                ));
            }
            if others.len() >= 3 {
                break;
            }
        }
    }
    Some(Answer {
        title: format!(
            "Disk: {} free of {} ({percent:.0}% used)",
            human_bytes(free),
            human_bytes(total)
        ),
        subtitle: if others.is_empty() {
            format!("{} used on the home filesystem", human_bytes(used))
        } else {
            others.join(" · ")
        },
        copy: format!(
            "Disk: {} free of {} ({percent:.0}% used), {} used\n{}",
            human_bytes(free),
            human_bytes(total),
            human_bytes(used),
            others.join("\n")
        ),
    })
}

fn display_answer() -> Option<Answer> {
    let mut connected = Vec::new();
    for entry in std::fs::read_dir("/sys/class/drm").ok()?.flatten() {
        let path = entry.path();
        let Some(status) = read(path.join("status").to_str()?) else {
            continue;
        };
        if status != "connected" {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let mode = read(path.join("modes").to_str()?)
            .and_then(|modes| modes.lines().next().map(str::to_owned));
        connected.push(match mode {
            Some(mode) => format!("{name} · {mode}"),
            None => name,
        });
    }
    if connected.is_empty() {
        return Some(Answer {
            title: "Display: no connected output found".into(),
            subtitle: "From the DRM connector".into(),
            copy: "Display: no connected output found".into(),
        });
    }
    let title = if connected.len() == 1 {
        format!("Display: {}", connected[0])
    } else {
        format!("Displays: {} connected", connected.len())
    };
    Some(Answer {
        subtitle: if connected.len() == 1 {
            "From the DRM connector".into()
        } else {
            connected.join(" · ")
        },
        copy: if connected.len() == 1 {
            title.clone()
        } else {
            format!("{title}\n{}", connected.join("\n"))
        },
        title,
    })
}

fn audio_answer() -> Option<Answer> {
    let sink = audio_node("@DEFAULT_AUDIO_SINK@").unwrap_or_else(|| "default output".into());
    let source = audio_node("@DEFAULT_AUDIO_SOURCE@");
    let volume = crate::system::volume().ok();
    let mut bits = Vec::new();
    if let Some((level, muted)) = volume {
        bits.push(format!(
            "Volume {level:.0}%{}",
            if muted { " (muted)" } else { "" }
        ));
    }
    if let Some(source) = &source {
        bits.push(format!("Input: {source}"));
    }
    Some(Answer {
        title: format!("Audio output: {sink}"),
        subtitle: if bits.is_empty() {
            "From PipeWire".into()
        } else {
            bits.join(" · ")
        },
        copy: format!(
            "Audio output: {sink}\nInput: {}",
            source.as_deref().unwrap_or("none")
        ),
    })
}

fn temperature_answer() -> Option<Answer> {
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

fn gpu_answer() -> Option<Answer> {
    let mut found = None;
    for entry in std::fs::read_dir("/sys/class/drm").ok()?.flatten() {
        let path = entry.path().join("device/uevent");
        let Some(text) = read(path.to_str()?) else {
            continue;
        };
        let mut driver = String::new();
        let mut pci = String::new();
        for line in text.lines() {
            if let Some(value) = line.strip_prefix("DRIVER=") {
                driver = value.to_owned();
            }
            if let Some(value) = line.strip_prefix("PCI_ID=") {
                pci = value.to_owned();
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

fn uptime_answer() -> Option<Answer> {
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

fn system_answer() -> Option<Answer> {
    let os = read("/etc/os-release")
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("PRETTY_NAME="))
                .map(|line| {
                    line.split('=')
                        .nth(1)
                        .unwrap_or(line)
                        .trim_matches('"')
                        .to_owned()
                })
        })
        .unwrap_or_else(|| "Linux".into());
    let kernel = command(&["uname", "-sr"]).unwrap_or_default();
    let arch = command(&["uname", "-m"]).unwrap_or_default();
    let host = read("/proc/sys/kernel/hostname").unwrap_or_default();
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| std::env::var("DESKTOP_SESSION"))
        .unwrap_or_default();
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let user = std::env::var("USER").unwrap_or_default();
    let mut bits = vec![host.clone()];
    if !desktop.is_empty() {
        bits.push(desktop.clone());
    }
    if !session.is_empty() {
        bits.push(session.clone());
    }
    if !arch.is_empty() {
        bits.push(arch.clone());
    }
    if !user.is_empty() {
        bits.push(user.clone());
    }
    Some(Answer {
        title: format!("{os} · {kernel}"),
        subtitle: bits.join(" · "),
        copy: format!(
            "{os}\nKernel {kernel}\nHostname {host}\nDesktop {desktop} ({session})\nArchitecture {arch}\nUser {user}"
        ),
    })
}

fn answer(id: &str) -> Option<Answer> {
    match id {
        "brightness" => brightness_answer(),
        "volume" => volume_answer(),
        "battery" => battery_answer(),
        "power_profile" => power_profile_answer(),
        "network" => network_answer(),
        "bluetooth" => bluetooth_answer(),
        "time" => time_answer(),
        "cpu" => cpu_answer(),
        "cpu_load" => cpu_load_answer(),
        "processes" => processes_answer(),
        "memory" => memory_answer(),
        "disk" => disk_answer(),
        "display" => display_answer(),
        "audio" => audio_answer(),
        "temperature" => temperature_answer(),
        "gpu" => gpu_answer(),
        "uptime" => uptime_answer(),
        "system" => system_answer(),
        _ => None,
    }
}

/// The live-system provider. It reads only when a topic matches.
pub struct SystemInfo;

impl Provider for SystemInfo {
    fn provide(&self, query: &Query, _: &Scorer, out: &mut Vec<Candidate>) {
        if !matches!(query.mode, Mode::Normal | Mode::Open) || query.subject.trim().is_empty() {
            return;
        }
        let mut scored: Vec<(f32, &Topic)> = TOPICS
            .iter()
            .map(|topic| (score(query, topic), topic))
            .filter(|(score, _)| *score >= 0.8)
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (score, topic) in scored {
            let Some(a) = answer(topic.id) else {
                continue;
            };
            out.push(Candidate {
                title: a.title,
                subtitle: a.subtitle,
                kind: Kind::Info,
                icon: topic.icon.into(),
                score,
                key: format!("info:{}", topic.id),
                intent: Intent::new("info.copy", Source::Provider)
                    .arg("title", topic.title)
                    .arg("text", a.copy),
                alt: None,
            });
            break;
        }
    }
}

struct Panel {
    title: &'static str,
    app_id: &'static str,
    /// Normalised everyday names for this panel. A test keeps them
    /// normalised, and the settings provider matches whole phrases first.
    aliases: &'static [&'static str],
}

const PANELS: &[Panel] = &[
    Panel {
        title: "Power",
        app_id: "gnome-power-panel.desktop",
        aliases: &[
            "power",
            "battery",
            "brightness",
            "display",
            "screen",
            "dim",
            "blank",
            "suspend",
            "sleep",
            "energy",
            "power saving",
            "power saver",
        ],
    },
    Panel {
        title: "Displays",
        app_id: "gnome-display-panel.desktop",
        aliases: &[
            "display",
            "displays",
            "screen",
            "monitor",
            "monitors",
            "resolution",
            "refresh rate",
            "scale",
            "rotation",
            "brightness",
            "night light",
            "night mode",
            "blue light",
        ],
    },
    Panel {
        title: "Sound",
        app_id: "gnome-sound-panel.desktop",
        aliases: &[
            "sound",
            "audio",
            "volume",
            "speaker",
            "speakers",
            "microphone",
            "mic",
            "output",
            "input",
            "headset",
        ],
    },
    Panel {
        title: "Wi-Fi",
        app_id: "gnome-wifi-panel.desktop",
        aliases: &["wifi", "wi-fi", "wireless", "hotspot", "hotspots"],
    },
    Panel {
        title: "Network",
        app_id: "gnome-network-panel.desktop",
        aliases: &[
            "network",
            "networking",
            "wifi",
            "wi-fi",
            "wireless",
            "ethernet",
            "proxy",
            "vpn",
            "connection",
            "internet",
        ],
    },
    Panel {
        title: "Bluetooth",
        app_id: "gnome-bluetooth-panel.desktop",
        aliases: &["bluetooth", "bluetooth device", "bluetooth devices"],
    },
    Panel {
        title: "Keyboard",
        app_id: "gnome-keyboard-panel.desktop",
        aliases: &[
            "keyboard",
            "shortcut",
            "shortcuts",
            "key bindings",
            "repeat",
            "layout",
            "input source",
            "input sources",
        ],
    },
    Panel {
        title: "Mouse & Touchpad",
        app_id: "gnome-mouse-panel.desktop",
        aliases: &["mouse", "touchpad", "pointer", "scroll", "tap", "click"],
    },
    Panel {
        title: "Privacy",
        app_id: "gnome-privacy-panel.desktop",
        aliases: &[
            "privacy",
            "security",
            "screen lock",
            "lock screen",
            "camera",
            "microphone access",
            "location",
            "file history",
            "trash",
            "diagnostics",
        ],
    },
    Panel {
        title: "Accessibility",
        app_id: "gnome-universal-access-panel.desktop",
        aliases: &[
            "accessibility",
            "zoom",
            "contrast",
            "screen reader",
            "visual",
            "hearing",
            "typing",
        ],
    },
    Panel {
        title: "Date & Time",
        app_id: "gnome-datetime-panel.desktop",
        aliases: &[
            "date",
            "time",
            "timezone",
            "time zone",
            "clock",
            "24 hour",
            "date and time",
        ],
    },
    Panel {
        title: "Region & Language",
        app_id: "gnome-region-panel.desktop",
        aliases: &["region", "language", "languages", "formats", "locale"],
    },
    Panel {
        title: "Users",
        app_id: "gnome-users-panel.desktop",
        aliases: &[
            "user",
            "users",
            "account",
            "accounts",
            "password",
            "login",
        ],
    },
    Panel {
        title: "About",
        app_id: "gnome-about-panel.desktop",
        aliases: &[
            "about",
            "system info",
            "system information",
            "hardware",
            "specs",
            "version",
        ],
    },
    Panel {
        title: "Multitasking",
        app_id: "gnome-multitasking-panel.desktop",
        aliases: &["multitasking", "workspaces", "workspace", "hot corner", "overview"],
    },
    Panel {
        title: "Wellbeing",
        app_id: "gnome-wellbeing-panel.desktop",
        aliases: &["wellbeing", "screen time", "downtime", "break reminder"],
    },
    Panel {
        title: "Search",
        app_id: "gnome-search-panel.desktop",
        aliases: &["search", "file search", "app search", "search settings"],
    },
    Panel {
        title: "Printers",
        app_id: "gnome-printers-panel.desktop",
        aliases: &["printer", "printers", "printing"],
    },
    Panel {
        title: "Notifications",
        app_id: "gnome-notifications-panel.desktop",
        aliases: &[
            "notification",
            "notifications",
            "do not disturb",
            "dnd",
            "banners",
        ],
    },
    Panel {
        title: "Applications",
        app_id: "gnome-applications-panel.desktop",
        aliases: &[
            "applications",
            "apps",
            "default apps",
            "default applications",
            "file associations",
        ],
    },
    Panel {
        title: "Sharing",
        app_id: "gnome-sharing-panel.desktop",
        aliases: &[
            "sharing",
            "share",
            "remote desktop",
            "media sharing",
            "file sharing",
        ],
    },
    Panel {
        title: "Background",
        app_id: "gnome-background-panel.desktop",
        aliases: &[
            "background",
            "wallpaper",
            "desktop background",
            "lock screen background",
        ],
    },
    Panel {
        title: "Color Management",
        app_id: "gnome-color-panel.desktop",
        aliases: &[
            "color",
            "colour",
            "color management",
            "display",
            "icc",
            "profile",
            "calibrate",
        ],
    },
    Panel {
        title: "Wacom Tablet",
        app_id: "gnome-wacom-panel.desktop",
        aliases: &["wacom", "tablet", "stylus", "pen"],
    },
    Panel {
        title: "Online Accounts",
        app_id: "gnome-online-accounts-panel.desktop",
        aliases: &[
            "online account",
            "online accounts",
            "google account",
            "nextcloud",
            "microsoft account",
        ],
    },
    Panel {
        title: "System",
        app_id: "gnome-system-panel.desktop",
        aliases: &["system", "firmware", "updates", "secure boot", "device security"],
    },
    Panel {
        title: "Mobile Broadband",
        app_id: "gnome-wwan-panel.desktop",
        aliases: &["mobile broadband", "cellular", "wwan", "sim"],
    },
];

/// A small curated map from everyday names to the GNOME Settings panels.
pub struct Settings;

fn panel_rows(words: &[String], generic: bool) -> Vec<(&'static Panel, f32)> {
    if generic {
        return PANELS
            .iter()
            .take(8)
            .enumerate()
            .map(|(index, panel)| (panel, 0.72 - index as f32 * 0.001))
            .collect();
    }
    let mut rows: Vec<(&'static Panel, f32)> = PANELS
        .iter()
        .filter_map(|panel| {
            let score = phrase_score(words, panel.aliases);
            if score < 0.8 {
                return None;
            }
            // A panel whose own title matches ("Displays" for "display",
            // "Wi-Fi" for "wifi") should come before one that only matched a
            // keyword.
            let title = text::normalize(panel.title);
            let compact: String = panel
                .title
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase();
            let title_match = words.iter().any(|word| {
                let compact_word: String =
                    word.chars().filter(|c| c.is_alphanumeric()).collect();
                title
                    .iter()
                    .any(|part| part.starts_with(word.as_str()))
                    || compact.starts_with(&compact_word)
            });
            Some((panel, if title_match { score + 0.01 } else { score }))
        })
        .collect();
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    rows.truncate(6);
    rows
}

impl Provider for Settings {
    fn provide(&self, query: &Query, _: &Scorer, out: &mut Vec<Candidate>) {
        if !matches!(query.mode, Mode::Normal | Mode::Open | Mode::Find) {
            return;
        }
        let normalised = text::normalize(&query.subject);
        let generic = normalised
            .iter()
            .any(|word| matches!(word.as_str(), "setting" | "settings" | "preference" | "preferences"));
        let words: Vec<String> = normalised
            .into_iter()
            .filter(|word| {
                !matches!(
                    word.as_str(),
                    "setting" | "settings" | "preference" | "preferences" | "panel" | "panels"
                )
            })
            .collect();
        let mut rows = panel_rows(&words, false);
        if generic && rows.is_empty() {
            rows = panel_rows(&[], true);
        }
        for (panel, score) in rows {
            out.push(Candidate {
                title: format!("{} Settings", panel.title),
                subtitle: "Open the GNOME Settings panel".into(),
                kind: Kind::Setting,
                icon: "preferences-system-symbolic".into(),
                score,
                key: format!("app:{}", panel.app_id),
                intent: Intent::new("app.launch", Source::Provider).arg("app_id", panel.app_id),
                alt: None,
            });
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

    fn provide(query: &str) -> Vec<Candidate> {
        let engine = Engine::new();
        let q = engine.parse(query, &crate::host::LinuxHost);
        let mut out = Vec::new();
        SystemInfo.provide(&q, &Scorer::new(), &mut out);
        out
    }

    fn settings(query: &str) -> Vec<Candidate> {
        let engine = Engine::new();
        let q = engine.parse(query, &crate::host::LinuxHost);
        let mut out = Vec::new();
        Settings.provide(&q, &Scorer::new(), &mut out);
        out
    }

    fn best_topic(query: &str) -> &'static str {
        let engine = Engine::new();
        let q = engine.parse(query, &crate::host::LinuxHost);
        TOPICS
            .iter()
            .map(|topic| (score(&q, topic), topic.id))
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id)
            .unwrap_or("")
    }

    #[test]
    fn aliases_are_normalised() {
        for topic in TOPICS {
            for alias in topic.aliases {
                assert_eq!(
                    text::normalize(alias).join(" "),
                    *alias,
                    "{} alias is not normalised: {alias:?}",
                    topic.id
                );
            }
        }
        for panel in PANELS {
            for alias in panel.aliases {
                assert_eq!(
                    text::normalize(alias).join(" "),
                    *alias,
                    "{} alias is not normalised: {alias:?}",
                    panel.app_id
                );
            }
        }
    }

    #[test]
    fn natural_questions_pick_the_best_topic() {
        for (query, expected) in [
            ("how bright is my screen", "brightness"),
            ("what is my brightness", "brightness"),
            ("what is my ip address", "network"),
            ("what is my ip", "network"),
            ("am i charging", "battery"),
            ("what is my battery at", "battery"),
            ("what cpu do i have", "cpu"),
            ("what is using my cpu", "processes"),
            ("how hot is the cpu", "temperature"),
            ("what time is it", "time"),
            ("what date is it", "time"),
            ("what bluetooth devices are connected", "bluetooth"),
            ("what desktop am i running", "system"),
            ("how much ram do i have", "memory"),
            ("how full is my disk", "disk"),
            ("what is my screen resolution", "display"),
            ("what speakers do i have", "audio"),
            ("what gpu do i have", "gpu"),
            ("how long has this computer been on", "uptime"),
        ] {
            assert_eq!(best_topic(query), expected, "{query}");
        }
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
    fn settings_are_searchable_without_the_word_settings() {
        let rows = settings("brightness");
        assert!(
            rows.iter()
                .any(|row| row.intent.text("app_id") == Some("gnome-power-panel.desktop")),
            "{rows:#?}"
        );
        let rows = settings("open wifi settings");
        assert!(
            rows.iter().any(|row| row.intent.text("app_id") == Some("gnome-wifi-panel.desktop")),
            "{rows:#?}"
        );
    }

    #[test]
    fn settings_return_more_than_one_match() {
        let rows = settings("display settings");
        assert!(rows.len() >= 2, "{rows:#?}");
        assert!(
            rows.iter()
                .any(|row| row.intent.text("app_id") == Some("gnome-display-panel.desktop")),
            "{rows:#?}"
        );
    }

    #[test]
    fn a_generic_settings_query_lists_panels() {
        let rows = settings("settings");
        assert!(rows.len() >= 8, "{rows:#?}");
        assert_eq!(rows[0].intent.text("app_id"), Some("gnome-power-panel.desktop"));
    }
}
