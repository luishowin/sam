//! `~/.config/sam/config.toml` — optional; everything has a default.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub projects: Projects,
    pub voice: Voice,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Projects {
    pub roots: Vec<String>,
    pub depth: usize,
}

impl Default for Projects {
    fn default() -> Self {
        Projects {
            roots: [
                "~/Documents/GitHub",
                "~/Documents/github",
                "~/Projects",
                "~/Desktop",
                "~/src",
                "~/code",
            ]
            .map(String::from)
            .to_vec(),
            depth: 3,
        }
    }
}

/// Local speech input. English only, offline, no model in the search path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Voice {
    /// Whether the panel starts the speech worker at all.
    pub enabled: bool,
    /// "eager" loads at service start, "lazy" on first press, "off" never.
    pub mode: String,
    /// Run a confident voice result immediately; risky actions still ask.
    pub auto_run: bool,
    /// Directory under `~/.local/share/sam/models` holding Moonshine.
    pub model: String,
    /// 0 means every available core.
    pub threads: usize,
    /// The longest Sam will listen for, in seconds.
    pub max_seconds: u64,
    /// Silence after speech before the utterance ends, in milliseconds.
    pub silence_ms: u64,
}

impl Default for Voice {
    fn default() -> Self {
        Voice {
            enabled: true,
            mode: "eager".into(),
            auto_run: true,
            model: "moonshine-streaming".into(),
            threads: 0,
            max_seconds: 15,
            silence_ms: 900,
        }
    }
}

impl Voice {
    pub fn model_path(&self) -> PathBuf {
        let path = expand_home(&self.model);
        if path.is_absolute() {
            path
        } else {
            models_dir().join(path)
        }
    }

    pub fn threads(&self) -> usize {
        if self.threads > 0 {
            self.threads
        } else {
            std::thread::available_parallelism().map_or(4, Into::into)
        }
    }
}

/// Where speech models live. They are data, not settings.
pub fn models_dir() -> PathBuf {
    glib::user_data_dir().join("sam").join("models")
}

impl Config {
    pub fn path() -> PathBuf {
        glib::user_config_dir().join("sam").join("config.toml")
    }

    pub fn load() -> Config {
        let path = Config::path();
        match std::fs::read_to_string(&path) {
            Ok(text) => Config::parse(&text).unwrap_or_else(|e| {
                crate::log::warn(
                    "config-ignored",
                    None,
                    format_args!("the config file does not parse, so the defaults apply: {e}"),
                );
                Config::default()
            }),
            Err(_) => Config::default(),
        }
    }

    pub fn parse(text: &str) -> Result<Config, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Config::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The effective mode, treating `enabled = false` as off.
    pub fn voice_mode(&self) -> &str {
        if !self.voice.enabled {
            "off"
        } else {
            match self.voice.mode.as_str() {
                "eager" | "lazy" | "off" => self.voice.mode.as_str(),
                _ => "eager",
            }
        }
    }

    /// The project roots that exist, with `~` expanded.
    pub fn project_roots(&self) -> Vec<PathBuf> {
        self.projects
            .roots
            .iter()
            .map(|r| expand_home(r))
            .filter(|p| p.is_dir())
            .collect()
    }
}

pub fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return glib::home_dir();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return glib::home_dir().join(rest);
    }
    PathBuf::from(path)
}

/// `~/Documents/a` → `~/Documents/a`.
pub fn tilde(path: &Path) -> String {
    let home = glib::home_dir();
    match path.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}
