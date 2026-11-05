//! Installed applications and GNOME Settings panels, from the desktop
//! entries GLib already indexes (§5). Refreshed when GLib says they changed
//! (F12).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gio::prelude::*;
use sam_core::APP_ID;
use sam_core::engine::{Mode, Provider, Query};
use sam_core::intent::{Intent, Source};
use sam_core::rank::{Candidate, Kind, Scorer};

#[derive(Debug, Clone, PartialEq)]
pub struct AppEntry {
    /// The desktop-entry ID, `org.gnome.Calculator.desktop`.
    pub id: String,
    pub name: String,
    pub generic: Option<String>,
    pub keywords: Vec<String>,
    /// The last part of the ID: `Calculator`, `google-chrome`.
    pub tail: String,
    /// The executable's file name: `google-chrome-stable`.
    pub exec: Option<String>,
    /// A GIcon string.
    pub icon: String,
    pub setting: bool,
    pub subtitle: String,
}

/// `org.gnome.Calculator.desktop` → `Calculator`. Web-app IDs such as
/// `chrome-bnigahogajmnmklmhmkpnkphddldljno-Default` name nothing a person
/// types, so long tails are not matched (empty).
pub fn id_tail(id: &str) -> &str {
    let stem = id.strip_suffix(".desktop").unwrap_or(id);
    let tail = stem.rsplit('.').next().unwrap_or(stem);
    if tail.len() > 32 { "" } else { tail }
}

/// Same name and executable: one row, the reverse-DNS ID preferred. Same
/// name, different programs: both rows, told apart by their IDs.
pub fn dedupe(mut entries: Vec<AppEntry>) -> Vec<AppEntry> {
    entries.sort_by_key(|e| (e.id.matches('.').count() < 2, e.id.clone()));
    let mut seen = std::collections::HashSet::new();
    entries.retain(|e| seen.insert((e.name.to_lowercase(), e.exec.clone())));
    let mut names: HashMap<String, usize> = HashMap::new();
    for e in &entries {
        *names.entry(e.name.to_lowercase()).or_default() += 1;
    }
    for e in &mut entries {
        if names[&e.name.to_lowercase()] > 1 {
            e.subtitle = format!("{} · {}", e.subtitle, e.id.trim_end_matches(".desktop"));
        }
    }
    entries.sort_by_key(|e| e.name.to_lowercase());
    entries
}

fn load() -> Vec<AppEntry> {
    let mut entries = Vec::new();
    for info in gio::AppInfo::all() {
        let Ok(desktop) = info.clone().downcast::<gio_unix::DesktopAppInfo>() else {
            continue;
        };
        let Some(id) = info.id().map(|s| s.to_string()) else {
            continue;
        };
        if id == format!("{APP_ID}.desktop") || desktop.is_hidden() {
            continue;
        }
        let categories = desktop
            .categories()
            .map(|c| c.to_string())
            .unwrap_or_default();
        let setting = categories.split(';').any(|c| c == "X-GNOME-Settings-Panel");
        // Settings panels are NoDisplay by design; show them if this desktop does.
        let visible = if setting {
            desktop.shows_in(None)
        } else {
            info.should_show()
        };
        if !visible {
            continue;
        }
        let name = info.display_name().to_string();
        let generic = desktop.generic_name().map(|g| g.to_string());
        let subtitle = if setting {
            "Settings".to_owned()
        } else {
            generic
                .clone()
                .or_else(|| info.description().map(|d| d.to_string()))
                .unwrap_or_else(|| "Application".to_owned())
        };
        let exec = info.executable();
        let exec = exec
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .filter(|e| !e.is_empty());
        entries.push(AppEntry {
            tail: id_tail(&id).to_owned(),
            icon: info
                .icon()
                .and_then(|i| IconExt::to_string(&i))
                .map(|s| s.to_string())
                .unwrap_or_else(|| "application-x-executable-symbolic".into()),
            keywords: desktop.keywords().iter().map(|k| k.to_string()).collect(),
            id,
            name,
            generic,
            exec,
            setting,
            subtitle,
        });
    }
    dedupe(entries)
}

/// The app index. Lives on the main thread.
pub struct AppIndex {
    entries: RefCell<Vec<AppEntry>>,
    stale: Rc<Cell<bool>>,
    monitor: RefCell<Option<(gio::AppInfoMonitor, glib::SignalHandlerId)>>,
}

impl Default for AppIndex {
    fn default() -> Self {
        AppIndex::new()
    }
}

impl AppIndex {
    pub fn new() -> AppIndex {
        AppIndex {
            entries: RefCell::new(load()),
            stale: Rc::default(),
            monitor: RefCell::default(),
        }
    }

    /// Reload after GLib reports a change. Needs the main loop that will run
    /// the panel; the CLI does without.
    pub fn watch(&self) {
        let monitor = gio::AppInfoMonitor::get();
        let stale = self.stale.clone();
        let id = monitor.connect_changed(move |_| stale.set(true));
        *self.monitor.borrow_mut() = Some((monitor, id));
    }

    pub fn entries(&self) -> std::cell::Ref<'_, Vec<AppEntry>> {
        if self.stale.replace(false) {
            *self.entries.borrow_mut() = load();
        }
        self.entries.borrow()
    }
}

impl Provider for AppIndex {
    fn provide(&self, query: &Query, scorer: &Scorer, out: &mut Vec<Candidate>) {
        if !matches!(query.mode, Mode::Normal | Mode::Open | Mode::Find) || query.subject.is_empty()
        {
            return;
        }
        for e in self.entries().iter() {
            let names = [
                (Some(e.name.as_str()), 1.0),
                (Some(e.tail.as_str()), 0.9),
                (e.generic.as_deref(), 0.85),
                (e.exec.as_deref(), 0.85),
            ]
            .into_iter()
            .filter_map(|(n, w)| n.map(|n| (n, w)))
            .chain(e.keywords.iter().map(|k| (k.as_str(), 0.8)));
            let Some(score) = scorer.best(&query.subject, names) else {
                continue;
            };
            out.push(Candidate {
                title: e.name.clone(),
                subtitle: e.subtitle.clone(),
                kind: if e.setting { Kind::Setting } else { Kind::App },
                icon: e.icon.clone(),
                score,
                key: format!("app:{}", e.id),
                intent: Intent::new("app.launch", Source::Provider).arg("app_id", e.id.as_str()),
                alt: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, name: &str, exec: &str) -> AppEntry {
        AppEntry {
            id: id.into(),
            name: name.into(),
            generic: None,
            keywords: vec![],
            tail: id_tail(id).into(),
            exec: Some(exec.into()),
            icon: String::new(),
            setting: false,
            subtitle: "Terminal Emulator".into(),
        }
    }

    #[test]
    fn tails() {
        assert_eq!(id_tail("org.gnome.Calculator.desktop"), "Calculator");
        assert_eq!(id_tail("google-chrome.desktop"), "google-chrome");
        assert_eq!(id_tail("PhotoDesk.desktop"), "PhotoDesk");
        assert_eq!(
            id_tail("chrome-bnigahogajmnmklmhmkpnkphddldljno-Default.desktop"),
            ""
        );
    }

    #[test]
    fn duplicates_merge_and_namesakes_are_told_apart() {
        let out = dedupe(vec![
            entry(
                "google-chrome.desktop",
                "Google Chrome",
                "google-chrome-stable",
            ),
            entry(
                "com.google.Chrome.desktop",
                "Google Chrome",
                "google-chrome-stable",
            ),
            entry("org.gnome.Terminal.desktop", "Terminal", "gnome-terminal"),
            entry("org.gnome.Ptyxis.desktop", "Terminal", "ptyxis"),
        ]);
        let ids: Vec<_> = out.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids.iter().filter(|i| i.contains("hrome")).count(), 1);
        assert!(
            ids.contains(&"com.google.Chrome.desktop"),
            "the reverse-DNS ID is kept: {ids:?}"
        );
        let terminals: Vec<_> = out.iter().filter(|e| e.name == "Terminal").collect();
        assert_eq!(terminals.len(), 2);
        assert!(
            terminals.iter().all(|t| t.subtitle.contains("org.gnome.")),
            "{terminals:?}"
        );
    }

    #[test]
    fn this_machine_has_its_apps() {
        let index = AppIndex::new();
        let entries = index.entries();
        let find = |id: &str| entries.iter().find(|e| e.id == id);
        assert!(find("org.gnome.Calculator.desktop").is_some());
        // GNOME Settings panels carry OnlyShowIn=GNOME, so on KDE they are
        // correctly hidden (shows_in), while on GNOME they surface as settings.
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        let gnome = desktop.split(':').any(|d| d.eq_ignore_ascii_case("gnome"));
        let panel = find("gnome-bluetooth-panel.desktop");
        if gnome {
            assert!(panel.is_some_and(|e| e.setting));
        } else {
            assert!(panel.is_none(), "a GNOME-only panel must hide off GNOME");
        }
        assert!(
            entries
                .iter()
                .all(|e| e.id != "io.github.luishowin.Sam.desktop"),
            "SAM does not list itself"
        );
    }
}
