//! Super+Space: taking it from GNOME, and giving it back (§8, P9).
//!
//! `install` removes `<Super>space` from the GNOME keys that ship with it,
//! remembering what each held, adds a custom shortcut at a path of SAM's own,
//! and enables the user service. `undo` reverses all of it. Both switch off
//! the glass extension that 0.0.2 installed, if SAM switched it on (F24).

use std::path::PathBuf;
use std::process::Command;

use gio::prelude::*;
use serde::{Deserialize, Serialize};

use sam_core::APP_ID;

const MEDIA_KEYS: &str = "org.gnome.settings-daemon.plugins.media-keys";
const CUSTOM_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
const CUSTOM_LIST_KEY: &str = "custom-keybindings";
/// A named path rather than `customN`, so GNOME Settings never picks it for a
/// shortcut of its own.
pub const CUSTOM_PATH: &str =
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/sam/";
pub const BINDING: &str = "<Super>space";
const MUTTER: &str = "org.gnome.mutter";
const CENTER_KEY: &str = "center-new-windows";
const SHELL: &str = "org.gnome.shell";
const ENABLED_KEY: &str = "enabled-extensions";
/// The GNOME Shell extension 0.0.2 installed for the blur, retired (F24).
const OLD_EXTENSION: &str = "sam@luishowin.github.io";

/// Keys GNOME ships bound to Super+Space on this machine or on a fresh install.
const CONFLICTS: &[(&str, &str)] = &[
    (MEDIA_KEYS, "search"),
    ("org.gnome.desktop.wm.keybindings", "switch-input-source"),
];

fn command() -> String {
    format!("gapplication action {APP_ID} toggle")
}

fn unit() -> String {
    format!("{APP_ID}.service")
}

#[derive(Debug)]
pub struct SetupError(pub String);

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SetupError {}

/// What a run did, one line per change, for the CLI to print.
pub type Report = Vec<String>;

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
struct State {
    #[serde(default)]
    taken: Vec<Taken>,
    /// Whether an older `install` switched the glass extension on, so that
    /// only what SAM switched on is switched off (F24).
    #[serde(default)]
    enabled_extension: bool,
}

/// A GNOME key SAM took `<Super>space` from.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Taken {
    schema: String,
    key: String,
    /// Its value before SAM touched it.
    previous: Vec<String>,
}

fn state_path() -> PathBuf {
    glib::user_config_dir().join("sam").join("setup-state.toml")
}

fn read_state() -> State {
    std::fs::read_to_string(state_path())
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_state(state: &State) -> Result<(), SetupError> {
    let path = state_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| SetupError(format!("{}: {e}", dir.display())))?;
    }
    let text = toml::to_string(state).map_err(|e| SetupError(e.to_string()))?;
    std::fs::write(&path, text).map_err(|e| SetupError(format!("{}: {e}", path.display())))
}

/// GNOME writes the same accelerator several ways (`<Super>space`,
/// `<Super>Space`, `<Mod4>space`); compare them as one.
pub fn same_accelerator(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> String {
        s.to_ascii_lowercase().replace("<mod4>", "<super>")
    }
    norm(a) == norm(b)
}

/// `values` without `binding`.
pub fn without_binding(values: &[String], binding: &str) -> Vec<String> {
    values
        .iter()
        .filter(|v| !same_accelerator(v, binding))
        .cloned()
        .collect()
}

/// `list` with `item` appended, unless it is already there.
pub fn with_item(list: &[String], item: &str) -> Vec<String> {
    let mut out = list.to_vec();
    if !out.iter().any(|p| p == item) {
        out.push(item.to_owned());
    }
    out
}

/// `list` without `item`.
pub fn without_item(list: &[String], item: &str) -> Vec<String> {
    list.iter().filter(|p| *p != item).cloned().collect()
}

/// `Settings` for `schema`, or `None` when this desktop does not have it —
/// `g_settings_new` aborts the process on a missing schema, so ask first.
fn settings(schema: &str, key: &str) -> Option<gio::Settings> {
    let found = gio::SettingsSchemaSource::default()?.lookup(schema, true)?;
    found.has_key(key).then(|| gio::Settings::new(schema))
}

fn strings(settings: &gio::Settings, key: &str) -> Vec<String> {
    settings.strv(key).iter().map(|s| s.to_string()).collect()
}

fn set_strings(settings: &gio::Settings, key: &str, values: &[String]) -> Result<(), SetupError> {
    let refs: Vec<&str> = values.iter().map(String::as_str).collect();
    settings
        .set_strv(key, refs.as_slice())
        .map_err(|e| SetupError(format!("{key}: {e}")))
}

fn systemctl(args: &[&str]) -> Result<(), String> {
    let out = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .map_err(|e| format!("systemctl: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// Switches the old glass extension off if SAM switched it on, and
/// forgets that it did (F24). GNOME Shell stops it at once.
fn retire_extension(state: &mut State, report: &mut Report) -> Result<(), SetupError> {
    if !state.enabled_extension {
        return Ok(());
    }
    if let Some(shell) = settings(SHELL, ENABLED_KEY) {
        let enabled = strings(&shell, ENABLED_KEY);
        let removed = without_item(&enabled, OLD_EXTENSION);
        if removed.len() != enabled.len() {
            set_strings(&shell, ENABLED_KEY, &removed)?;
            report.push(format!(
                "switched off SAM's old glass extension, {OLD_EXTENSION}"
            ));
        }
    }
    state.enabled_extension = false;
    Ok(())
}

/// Take Super+Space for SAM and enable its service.
pub fn install() -> Result<Report, SetupError> {
    let mut report = Report::new();
    let mut state = read_state();

    for (schema, key) in CONFLICTS {
        let Some(s) = settings(schema, key) else {
            continue;
        };
        let current = strings(&s, key);
        let remaining = without_binding(&current, BINDING);
        if remaining.len() == current.len() {
            continue;
        }
        // A second install must not overwrite the first one's memory of the
        // original value.
        if !state
            .taken
            .iter()
            .any(|t| t.schema == *schema && t.key == *key)
        {
            state.taken.push(Taken {
                schema: (*schema).to_owned(),
                key: (*key).to_owned(),
                previous: current.clone(),
            });
        }
        set_strings(&s, key, &remaining)?;
        report.push(format!("{schema} {key}: {current:?} → {remaining:?}"));
    }
    // Saved before anything else changes, so an interrupted install can
    // still be undone.
    write_state(&state)?;

    let media = settings(MEDIA_KEYS, CUSTOM_LIST_KEY)
        .ok_or_else(|| SetupError(format!("{MEDIA_KEYS} is not installed; is this GNOME?")))?;
    let list = strings(&media, CUSTOM_LIST_KEY);
    let added = with_item(&list, CUSTOM_PATH);
    if added.len() != list.len() {
        set_strings(&media, CUSTOM_LIST_KEY, &added)?;
        report.push(format!("added {CUSTOM_PATH} to {CUSTOM_LIST_KEY}"));
    }

    let custom = gio::Settings::with_path(CUSTOM_SCHEMA, CUSTOM_PATH);
    let err = |e: glib::BoolError| SetupError(e.to_string());
    custom.set_string("name", "Sam").map_err(err)?;
    custom.set_string("command", &command()).map_err(err)?;
    custom.set_string("binding", BINDING).map_err(err)?;
    report.push(format!("{BINDING} → {}", command()));
    gio::Settings::sync();

    retire_extension(&mut state, &mut report)?;
    write_state(&state)?;

    // Mutter places the panel; without this it lands wherever new windows
    // cascade (SPIKE-A). It is everyone's setting, so say so, don't change it.
    if let Some(mutter) = settings(MUTTER, CENTER_KEY)
        && !mutter.boolean(CENTER_KEY)
    {
        report.push(format!(
            "warning: {MUTTER} {CENTER_KEY} is false, so GNOME will not centre the panel; \
             to centre it: gsettings set {MUTTER} {CENTER_KEY} true"
        ));
    }

    match systemctl(&["daemon-reload"]).and_then(|()| systemctl(&["enable", "--now", &unit()])) {
        Ok(()) => report.push(format!("enabled and started {}", unit())),
        Err(e) => report.push(format!("warning: could not enable {}: {e}", unit())),
    }
    Ok(report)
}

/// Give Super+Space back and disable the service.
pub fn undo() -> Result<Report, SetupError> {
    let mut report = Report::new();
    let mut state = read_state();

    for taken in &state.taken {
        let Some(s) = settings(&taken.schema, &taken.key) else {
            continue;
        };
        let current = strings(&s, &taken.key);
        // Restore only what SAM left behind; if the key has changed since,
        // the change is someone's and it stays.
        if current == without_binding(&taken.previous, BINDING) {
            set_strings(&s, &taken.key, &taken.previous)?;
            report.push(format!(
                "{} {}: restored {:?}",
                taken.schema, taken.key, taken.previous
            ));
        } else {
            report.push(format!(
                "{} {}: changed since setup ({current:?}), left alone",
                taken.schema, taken.key
            ));
        }
    }

    if let Some(media) = settings(MEDIA_KEYS, CUSTOM_LIST_KEY) {
        let list = strings(&media, CUSTOM_LIST_KEY);
        let removed = without_item(&list, CUSTOM_PATH);
        if removed.len() != list.len() {
            set_strings(&media, CUSTOM_LIST_KEY, &removed)?;
            report.push(format!("removed {CUSTOM_PATH} from {CUSTOM_LIST_KEY}"));
        }
        let custom = gio::Settings::with_path(CUSTOM_SCHEMA, CUSTOM_PATH);
        for key in ["name", "command", "binding"] {
            custom.reset(key);
        }
    }
    retire_extension(&mut state, &mut report)?;
    gio::Settings::sync();

    match systemctl(&["disable", "--now", &unit()]) {
        Ok(()) => report.push(format!("disabled and stopped {}", unit())),
        Err(e) => report.push(format!("note: {}: {e}", unit())),
    }

    let path = state_path();
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| SetupError(format!("{}: {e}", path.display())))?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn accelerators_compare_across_spellings() {
        assert!(same_accelerator("<Super>space", "<Super>Space"));
        assert!(same_accelerator("<Mod4>space", "<Super>space"));
        assert!(!same_accelerator("<Shift><Super>space", "<Super>space"));
    }

    #[test]
    fn binding_removal_keeps_the_rest() {
        assert_eq!(
            without_binding(&v(&["<Super>space", "XF86Search"]), BINDING),
            v(&["XF86Search"])
        );
        assert_eq!(
            without_binding(&v(&["<Super>s"]), BINDING),
            v(&["<Super>s"])
        );
        assert!(without_binding(&v(&["<Super>Space"]), BINDING).is_empty());
    }

    #[test]
    fn existing_custom_shortcuts_survive() {
        let list = v(&[
            "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/custom0/",
            "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/custom1/",
        ]);
        let added = with_item(&list, CUSTOM_PATH);
        assert_eq!(added.len(), 3);
        assert_eq!(&added[..2], &list[..]);
        assert_eq!(
            with_item(&added, CUSTOM_PATH),
            added,
            "a second install adds nothing"
        );
        assert_eq!(without_item(&added, CUSTOM_PATH), list);
    }

    #[test]
    fn retiring_the_extension_leaves_the_others() {
        let enabled = v(&["zen@example.org", OLD_EXTENSION, "blur-my-shell@aunetx"]);
        assert_eq!(
            without_item(&enabled, OLD_EXTENSION),
            v(&["zen@example.org", "blur-my-shell@aunetx"])
        );
        let mut state = State::default();
        let mut report = Report::new();
        // Not switched on by SAM: nothing is touched, not even the settings.
        retire_extension(&mut state, &mut report).unwrap();
        assert!(report.is_empty());
    }

    #[test]
    fn state_round_trips() {
        let state = State {
            taken: vec![Taken {
                schema: MEDIA_KEYS.into(),
                key: "search".into(),
                previous: v(&["<Super>space"]),
            }],
            enabled_extension: true,
        };
        let text = toml::to_string(&state).unwrap();
        assert_eq!(toml::from_str::<State>(&text).unwrap(), state);
        assert_eq!(toml::from_str::<State>("").unwrap(), State::default());
    }
}
