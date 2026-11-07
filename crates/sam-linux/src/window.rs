//! Window control over the narrow GNOME Shell extension.
//!
//! Wayland does not let one client move another's surfaces. The extension is
//! the only code that touches `Meta.Window`; this module is a typed client
//! and a resolver. Every method fails closed when the extension is missing or
//! incompatible.

use gio::prelude::*;
use serde::Deserialize;

use crate::ExecError;

pub const EXTENSION_UUID: &str = "sam-window@luishowin.github.io";
const DEST: &str = "org.gnome.Shell";
const PATH: &str = "/org/gnome/Shell/Extensions/SamWindow";
const IFACE: &str = "org.gnome.Shell.Extensions.SamWindow";

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    #[serde(default)]
    pub wm_class: String,
    #[serde(default)]
    pub wm_class_instance: String,
    #[serde(default)]
    pub workspace: i32,
    #[serde(default)]
    pub monitor: i32,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default)]
    pub focused: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Resolve {
    One(WindowInfo),
    Many(Vec<WindowInfo>),
    None,
}

fn err(e: impl std::fmt::Display) -> ExecError {
    ExecError(e.to_string())
}

fn call(method: &str, args: Option<glib::Variant>) -> Result<glib::Variant, ExecError> {
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).map_err(err)?;
    bus.call_sync(
        Some(DEST),
        PATH,
        IFACE,
        method,
        args.as_ref(),
        None,
        gio::DBusCallFlags::NONE,
        2_000,
        gio::Cancellable::NONE,
    )
    .map_err(|e| {
        ExecError(format!(
            "window control is unavailable ({e}); run `sam setup` and check `sam doctor`"
        ))
    })
}

/// Whether the extension is loaded and answering on the session bus.
pub fn available() -> bool {
    list().is_ok()
}

fn list() -> Result<Vec<WindowInfo>, ExecError> {
    let reply = call("List", None)?;
    let json = reply
        .child_value(0)
        .get::<String>()
        .ok_or_else(|| err("window list: unexpected reply"))?;
    serde_json::from_str(&json).map_err(|e| err(format!("window list: {e}")))
}

fn work_area() -> Result<WorkArea, ExecError> {
    let reply = call("GetWorkArea", None)?;
    let json = reply
        .child_value(0)
        .get::<String>()
        .ok_or_else(|| err("work area: unexpected reply"))?;
    serde_json::from_str(&json).map_err(|e| err(format!("work area: {e}")))
}

fn method_bool(method: &str, args: Option<glib::Variant>) -> Result<(), ExecError> {
    let reply = call(method, args)?;
    match reply.child_value(0).get::<bool>() {
        Some(true) => Ok(()),
        _ => Err(err(format!("{method} was refused"))),
    }
}

/// Resolve a spoken target against the live windows.
///
/// Order: exact app class/id, exact title, then case-insensitive substring.
/// More than one match is ambiguity, never a guess.
pub fn resolve(windows: &[WindowInfo], target: &str) -> Resolve {
    let wanted = target.trim().to_lowercase();
    if wanted.is_empty() {
        return Resolve::None;
    }
    let exact: Vec<WindowInfo> = windows
        .iter()
        .filter(|w| {
            w.wm_class.to_lowercase() == wanted
                || w.wm_class_instance.to_lowercase() == wanted
                || w.title.to_lowercase() == wanted
        })
        .cloned()
        .collect();
    match exact.len() {
        1 => return Resolve::One(exact[0].clone()),
        n if n > 1 => return Resolve::Many(exact),
        _ => {}
    }
    let near: Vec<WindowInfo> = windows
        .iter()
        .filter(|w| {
            w.wm_class.to_lowercase().contains(&wanted)
                || w.wm_class_instance.to_lowercase().contains(&wanted)
                || w.title.to_lowercase().contains(&wanted)
        })
        .cloned()
        .collect();
    match near.len() {
        1 => Resolve::One(near[0].clone()),
        0 => Resolve::None,
        _ => Resolve::Many(near),
    }
}

fn one(windows: &[WindowInfo], target: &str) -> Result<WindowInfo, ExecError> {
    match resolve(windows, target) {
        Resolve::One(window) => Ok(window),
        Resolve::Many(matches) => {
            let names: Vec<String> = matches
                .iter()
                .take(5)
                .map(|w| format!("{} — {}", w.wm_class, w.title))
                .collect();
            Err(ExecError(format!(
                "more than one window matches {target:?}: {}",
                names.join("; ")
            )))
        }
        Resolve::None => Err(ExecError(format!("no window matches {target:?}"))),
    }
}

fn position_for(window: &WindowInfo, area: WorkArea, position: &str) -> Result<glib::Variant, ExecError> {
    let (x, y) = match position {
        "left" => (area.x, window.y),
        "right" => (area.x + area.width - window.width, window.y),
        "top" => (window.x, area.y),
        "bottom" => (window.x, area.y + area.height - window.height),
        other => return Err(ExecError(format!("unknown position {other:?}"))),
    };
    Ok((window.id, x, y, window.width, window.height).to_variant())
}

fn center_for(window: &WindowInfo, area: WorkArea) -> glib::Variant {
    let x = area.x + (area.width - window.width) / 2;
    let y = area.y + (area.height - window.height) / 2;
    (window.id, x, y, window.width, window.height).to_variant()
}

/// Run one `window.*` action.
pub fn run(action: &str, target: Option<&str>, position: Option<&str>) -> Result<String, ExecError> {
    match action {
        "window.list" => {
            let windows = list()?;
            if windows.is_empty() {
                return Ok("No windows".into());
            }
            Ok(windows
                .iter()
                .map(|w| format!("{} — {}", w.wm_class, w.title))
                .collect::<Vec<_>>()
                .join(" · "))
        }
        "window.show_desktop" => {
            method_bool("ShowDesktop", None)?;
            Ok("Showing the desktop".into())
        }
        _ => {
            let windows = list()?;
            let target = target.ok_or_else(|| ExecError("a window target is needed".into()))?;
            let window = one(&windows, target)?;
            match action {
                "window.activate" => {
                    method_bool("Activate", Some((window.id,).to_variant()))?;
                    Ok(format!("Switched to {}", window.title))
                }
                "window.minimize" => {
                    method_bool("Minimize", Some((window.id,).to_variant()))?;
                    Ok(format!("Minimised {}", window.title))
                }
                "window.maximize" => {
                    method_bool("Maximize", Some((window.id,).to_variant()))?;
                    Ok(format!("Maximised {}", window.title))
                }
                "window.restore" => {
                    method_bool("Unmaximize", Some((window.id,).to_variant()))?;
                    Ok(format!("Restored {}", window.title))
                }
                "window.center" => {
                    let area = work_area()?;
                    method_bool("MoveResize", Some(center_for(&window, area)))?;
                    Ok(format!("Centred {}", window.title))
                }
                "window.move" => {
                    let position = position
                        .ok_or_else(|| ExecError("a position is needed".into()))?;
                    let area = work_area()?;
                    method_bool("MoveResize", Some(position_for(&window, area, position)?))?;
                    Ok(format!("Moved {} {}", window.title, position))
                }
                "window.close" => {
                    method_bool("Close", Some((window.id,).to_variant()))?;
                    Ok(format!("Closed {}", window.title))
                }
                other => Err(ExecError(format!("unknown window action {other}"))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: u32, class: &str, title: &str) -> WindowInfo {
        WindowInfo {
            id,
            title: title.into(),
            wm_class: class.into(),
            wm_class_instance: class.into(),
            workspace: 0,
            monitor: 0,
            x: 100,
            y: 80,
            width: 800,
            height: 600,
            focused: false,
        }
    }

    #[test]
    fn resolves_exact_then_substring() {
        let windows = vec![
            window(1, "firefox", "Mozilla Firefox"),
            window(2, "org.gnome.Nautilus", "Files"),
        ];
        assert_eq!(resolve(&windows, "firefox"), Resolve::One(windows[0].clone()));
        assert_eq!(resolve(&windows, "mozilla"), Resolve::One(windows[0].clone()));
        assert_eq!(resolve(&windows, "files"), Resolve::One(windows[1].clone()));
        assert_eq!(resolve(&windows, "terminal"), Resolve::None);
    }

    #[test]
    fn two_matching_windows_are_ambiguous() {
        let windows = vec![
            window(1, "google-chrome", "Inbox — Chrome"),
            window(2, "google-chrome", "Docs — Chrome"),
        ];
        match resolve(&windows, "chrome") {
            Resolve::Many(matches) => assert_eq!(matches.len(), 2),
            other => panic!("expected ambiguity, got {other:?}"),
        }
    }

    #[test]
    fn positions_stay_inside_the_work_area() {
        let w = window(7, "app", "App");
        let area = WorkArea {
            x: 0,
            y: 32,
            width: 1920,
            height: 1048,
        };
        let right = position_for(&w, area, "right").unwrap();
        let tuple = right.get::<(u32, i32, i32, i32, i32)>().unwrap();
        assert_eq!(tuple, (7, 1120, 80, 800, 600));
        let bottom = position_for(&w, area, "bottom").unwrap();
        let tuple = bottom.get::<(u32, i32, i32, i32, i32)>().unwrap();
        assert_eq!(tuple, (7, 100, 480, 800, 600));
    }

    #[test]
    fn centering_uses_the_work_area() {
        let w = window(7, "app", "App");
        let area = WorkArea {
            x: 100,
            y: 50,
            width: 1400,
            height: 900,
        };
        let value = center_for(&w, area);
        let tuple = value.get::<(u32, i32, i32, i32, i32)>().unwrap();
        assert_eq!(tuple, (7, 400, 200, 800, 600));
    }
}
