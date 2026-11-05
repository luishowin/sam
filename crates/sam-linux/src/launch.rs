//! Starting things (§10). Everything SAM starts gets a systemd scope of its
//! own (F9), so restarting SAM never takes an app with it.

use std::path::Path;

use gio::prelude::*;
use gio_unix::prelude::*;

use crate::ExecError;

/// systemd's unit-name escaping (`systemd-escape`): `/` becomes `-`, and
/// anything else outside `[A-Za-z0-9:_.]` becomes `\xNN`, byte by byte. So
/// `google-chrome` → `google\x2dchrome`.
pub fn systemd_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, b) in s.bytes().enumerate() {
        let plain = b.is_ascii_alphanumeric() || b == b':' || b == b'_' || (b == b'.' && i > 0);
        if plain {
            out.push(b as char);
        } else if b == b'/' {
            out.push('-');
        } else {
            out.push_str(&format!("\\x{b:02x}"));
        }
    }
    out
}

/// `app-sam-<id>-<pid>.scope`, after systemd's desktop-environment convention.
pub fn scope_name(app_id: &str, pid: i32) -> String {
    let id = app_id.strip_suffix(".desktop").unwrap_or(app_id);
    format!("app-sam-{}-{pid}.scope", systemd_escape(id))
}

/// Moves `pid` into a transient scope. A failure is logged, not fatal: the
/// app is already running.
pub(crate) fn move_to_scope(app_id: &str, pid: i32) {
    let result = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).and_then(|bus| {
        let properties: Vec<(String, glib::Variant)> = vec![
            ("PIDs".into(), vec![pid as u32].to_variant()),
            ("CollectMode".into(), "inactive-or-failed".to_variant()),
            ("Slice".into(), "app.slice".to_variant()),
            (
                "Description".into(),
                format!("{app_id}, started by SAM").to_variant(),
            ),
        ];
        let aux: Vec<(String, Vec<(String, glib::Variant)>)> = Vec::new();
        let args = (scope_name(app_id, pid), "fail", properties, aux).to_variant();
        bus.call_sync(
            Some("org.freedesktop.systemd1"),
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
            "StartTransientUnit",
            Some(&args),
            None,
            gio::DBusCallFlags::NONE,
            2000,
            gio::Cancellable::NONE,
        )
    });
    if let Err(e) = result {
        crate::log::warn("scope-failed", Some("app.launch"), e);
    }
}

/// Launches a desktop entry, optionally with URIs.
pub(crate) fn app(
    app_id: &str,
    uris: &[&str],
    ctx: &gio::AppLaunchContext,
) -> Result<(), ExecError> {
    let info = gio_unix::DesktopAppInfo::new(app_id)
        .ok_or_else(|| ExecError("that application is not installed".into()))?;
    if info.boolean("DBusActivatable") {
        // The bus starts it, in a unit of its own.
        return info
            .launch_uris(uris, Some(ctx))
            .map_err(|e| ExecError(e.to_string()));
    }
    let id = app_id.to_owned();
    let mut on_pid = move |_: &gio_unix::DesktopAppInfo, pid: glib::Pid| move_to_scope(&id, pid.0);
    info.launch_uris_as_manager_with_fds(
        uris,
        Some(ctx),
        glib::SpawnFlags::SEARCH_PATH,
        None,
        Some(&mut on_pid),
        None::<std::os::fd::BorrowedFd<'_>>,
        None::<std::os::fd::BorrowedFd<'_>>,
        None::<std::os::fd::BorrowedFd<'_>>,
    )
    .map_err(|e| ExecError(e.to_string()))
}

fn default_app(content_type: &str) -> Option<String> {
    gio::AppInfo::default_for_type(content_type, false)
        .and_then(|a| a.id())
        .map(|id| id.to_string())
}

/// Opens a URI with its default application.
pub(crate) fn uri(uri: &str, ctx: &gio::AppLaunchContext) -> Result<(), ExecError> {
    let scheme = uri
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let handler = match scheme.as_str() {
        "http" | "https" => gio::AppInfo::default_for_uri_scheme(&scheme)
            .and_then(|a| a.id())
            .map(|i| i.to_string()),
        _ => {
            let file = gio::File::for_uri(uri);
            let content_type = file
                .query_info(
                    "standard::content-type",
                    gio::FileQueryInfoFlags::NONE,
                    gio::Cancellable::NONE,
                )
                .ok()
                .and_then(|i| i.content_type())
                .map(|t| t.to_string())
                .ok_or_else(|| ExecError("the file does not exist".into()))?;
            default_app(&content_type)
        }
    };
    match handler {
        Some(app_id) => app(&app_id, &[uri], ctx),
        None => gio::AppInfo::launch_default_for_uri(uri, Some(ctx))
            .map_err(|e| ExecError(e.to_string())),
    }
}

/// Opens a link with the application registered for its scheme, such as
/// OpenCode for `opencode://…` (F25), in a scope of its own.
pub(crate) fn scheme(uri: &str, ctx: &gio::AppLaunchContext) -> Result<(), ExecError> {
    let scheme = uri
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let app_id = gio::AppInfo::default_for_uri_scheme(&scheme)
        .and_then(|a| a.id())
        .ok_or_else(|| ExecError("no application here opens that kind of link".into()))?;
    app(&app_id, &[uri], ctx)
}

/// A `file://` URI whose target exists. Virtual locations (`trash:///`,
/// `recent:///`) have no target to check and pass through.
fn existing_file(uri: &str) -> Result<(), ExecError> {
    if !uri.starts_with("file://") {
        return Ok(());
    }
    gio::File::for_uri(uri)
        .query_info(
            "standard::name",
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        )
        .map(|_| ())
        .map_err(|_| ExecError("that file does not exist".into()))
}

/// Selects a file in the file manager. The target is checked first, so a
/// deleted file fails here with its cause — not over D-Bus in Files.
pub(crate) fn reveal(uri: &str, ctx: &gio::AppLaunchContext) -> Result<(), ExecError> {
    existing_file(uri)?;
    let token = ctx
        .startup_notify_id(None::<&gio::AppInfo>, &[])
        .map(|t| t.to_string())
        .unwrap_or_default();
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .map_err(|e| ExecError(e.to_string()))?;
    bus.call_sync(
        Some("org.freedesktop.FileManager1"),
        "/org/freedesktop/FileManager1",
        "org.freedesktop.FileManager1",
        "ShowItems",
        Some(&(vec![uri.to_owned()], token).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        5000,
        gio::Cancellable::NONE,
    )
    .map(|_| ())
    .map_err(|e| ExecError(e.to_string()))
}

/// Runs `argv` with the launch context's environment (the activation token
/// among it), in a scope of its own.
pub(crate) fn spawn(
    argv: &[&str],
    scope_as: &str,
    ctx: &gio::AppLaunchContext,
) -> Result<(), ExecError> {
    let token = ctx
        .startup_notify_id(None::<&gio::AppInfo>, &[])
        .map(|t| t.to_string());
    let mut env: Vec<String> = std::env::vars()
        .filter(|(k, _)| k != "XDG_ACTIVATION_TOKEN" && k != "DESKTOP_STARTUP_ID")
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    if let Some(token) = token {
        env.push(format!("XDG_ACTIVATION_TOKEN={token}"));
        env.push(format!("DESKTOP_STARTUP_ID={token}"));
    }
    let argv: Vec<&Path> = argv.iter().map(Path::new).collect();
    let envp: Vec<&Path> = env.iter().map(Path::new).collect();
    // Without DO_NOT_REAP_CHILD GLib double-forks: no zombie, and the pid is
    // the program's own.
    let pid = glib::spawn_async(
        None::<&Path>,
        &argv,
        &envp,
        glib::SpawnFlags::SEARCH_PATH,
        None,
    )
    .map_err(|e| ExecError(e.to_string()))?;
    move_to_scope(scope_as, pid.0);
    Ok(())
}

/// Opens the first installed terminal found, in its own scope.
pub(crate) fn terminal(ctx: &gio::AppLaunchContext) -> Result<String, ExecError> {
    const TERMINALS: &[&str] = &[
        "org.gnome.Ptyxis.desktop",
        "org.gnome.Console.desktop",
        "org.gnome.Terminal.desktop",
        "org.kde.konsole.desktop",
        "Alacritty.desktop",
        "com.raggesilver.BlackBox.desktop",
    ];
    for id in TERMINALS {
        if gio_unix::DesktopAppInfo::new(id).is_some() {
            app(id, &[], ctx)?;
            return Ok(format!("Opened {id}"));
        }
    }
    Err(ExecError("no terminal application is installed".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_matches_systemd() {
        // As seen on this machine: app-gnome-google\x2dchrome-7718.scope
        assert_eq!(systemd_escape("google-chrome"), "google\\x2dchrome");
        assert_eq!(
            systemd_escape("org.gnome.Calculator"),
            "org.gnome.Calculator"
        );
        assert_eq!(systemd_escape(".hidden"), "\\x2ehidden");
        assert_eq!(systemd_escape("a b/c"), "a\\x20b-c");
        assert_eq!(systemd_escape("é"), "\\xc3\\xa9");
        assert_eq!(
            scope_name("google-chrome.desktop", 42),
            "app-sam-google\\x2dchrome-42.scope"
        );
        assert_eq!(
            scope_name("org.gnome.Ptyxis.desktop", 7),
            "app-sam-org.gnome.Ptyxis-7.scope"
        );
    }

    #[test]
    fn systemd_agrees() {
        // `systemd-escape` is the reference, where it exists.
        let Ok(out) = std::process::Command::new("systemd-escape")
            .arg("google-chrome é.x")
            .output()
        else {
            return;
        };
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            systemd_escape("google-chrome é.x")
        );
    }
}
