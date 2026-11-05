//! The desktop's own switches (§7), over D-Bus, GSettings and `wpctl`.
//! Everything that changes state is `pub(crate)`: only the executor calls it.

use gio::prelude::*;

use crate::ExecError;

const RFKILL: &str = "org.gnome.SettingsDaemon.Rfkill";
const NM: &str = "org.freedesktop.NetworkManager";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const MPRIS: &str = "org.mpris.MediaPlayer2.";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

fn err(e: impl std::fmt::Display) -> ExecError {
    ExecError(e.to_string())
}

fn bus(kind: gio::BusType) -> Result<gio::DBusConnection, ExecError> {
    gio::bus_get_sync(kind, gio::Cancellable::NONE).map_err(err)
}

fn call(
    kind: gio::BusType,
    dest: &str,
    path: &str,
    iface: &str,
    method: &str,
    args: Option<glib::Variant>,
) -> Result<glib::Variant, ExecError> {
    bus(kind)?
        .call_sync(
            Some(dest),
            path,
            iface,
            method,
            args.as_ref(),
            None,
            gio::DBusCallFlags::ALLOW_INTERACTIVE_AUTHORIZATION,
            10_000,
            gio::Cancellable::NONE,
        )
        .map_err(err)
}

fn get_property(
    kind: gio::BusType,
    dest: &str,
    path: &str,
    iface: &str,
    name: &str,
) -> Result<glib::Variant, ExecError> {
    let reply = call(
        kind,
        dest,
        path,
        PROPERTIES,
        "Get",
        Some((iface, name).to_variant()),
    )?;
    // (v) → the inner value.
    reply
        .child_value(0)
        .as_variant()
        .ok_or_else(|| err(format!("{name}: unexpected reply {reply}")))
}

fn set_property(
    kind: gio::BusType,
    dest: &str,
    path: &str,
    iface: &str,
    name: &str,
    value: glib::Variant,
) -> Result<(), ExecError> {
    // A Variant inside a tuple is boxed as `v` by `to_variant`; box it once.
    let args = (iface, name, value).to_variant();
    call(kind, dest, path, PROPERTIES, "Set", Some(args)).map(|_| ())
}

/// `on`/`off`/`toggle` against the current state.
fn want(state: &str, current: bool) -> Result<bool, ExecError> {
    match state {
        "on" => Ok(true),
        "off" => Ok(false),
        "toggle" => Ok(!current),
        other => Err(err(format!("unknown state {other:?}"))),
    }
}

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

pub(crate) fn lock() -> Result<String, ExecError> {
    call(
        gio::BusType::Session,
        "org.gnome.ScreenSaver",
        "/org/gnome/ScreenSaver",
        "org.gnome.ScreenSaver",
        "Lock",
        None,
    )?;
    Ok("Locked".into())
}

pub(crate) fn suspend() -> Result<String, ExecError> {
    let login1 = "/org/freedesktop/login1";
    call(
        gio::BusType::System,
        "org.freedesktop.login1",
        login1,
        "org.freedesktop.login1.Manager",
        "Suspend",
        Some((true,).to_variant()),
    )?;
    Ok("Suspending".into())
}

fn session(method: &str, args: Option<glib::Variant>) -> Result<String, ExecError> {
    call(
        gio::BusType::Session,
        "org.gnome.SessionManager",
        "/org/gnome/SessionManager",
        "org.gnome.SessionManager",
        method,
        args,
    )?;
    Ok("GNOME will ask to confirm".into())
}

/// Mode 0: GNOME shows its end-session dialog first.
pub(crate) fn logout() -> Result<String, ExecError> {
    session("Logout", Some((0u32,).to_variant()))
}

pub(crate) fn restart() -> Result<String, ExecError> {
    session("Reboot", None)
}

pub(crate) fn power_off() -> Result<String, ExecError> {
    session("Shutdown", None)
}

/// Whether Bluetooth is on (read-only).
pub fn bluetooth_on() -> Result<bool, ExecError> {
    let path = "/org/gnome/SettingsDaemon/Rfkill";
    let has = get_property(
        gio::BusType::Session,
        RFKILL,
        path,
        RFKILL,
        "BluetoothHasAirplaneMode",
    )?;
    if has.get::<bool>() != Some(true) {
        return Err(err("there is no Bluetooth adapter"));
    }
    let airplane = get_property(
        gio::BusType::Session,
        RFKILL,
        path,
        RFKILL,
        "BluetoothAirplaneMode",
    )?;
    Ok(airplane.get::<bool>() == Some(false))
}

pub(crate) fn bluetooth(state: &str) -> Result<String, ExecError> {
    let on = want(state, bluetooth_on()?)?;
    let path = "/org/gnome/SettingsDaemon/Rfkill";
    set_property(
        gio::BusType::Session,
        RFKILL,
        path,
        RFKILL,
        "BluetoothAirplaneMode",
        (!on).to_variant(),
    )?;
    Ok(format!("Bluetooth is {}", on_off(on)))
}

/// Whether Wi-Fi is on (read-only).
pub fn wifi_on() -> Result<bool, ExecError> {
    let v = get_property(
        gio::BusType::System,
        NM,
        "/org/freedesktop/NetworkManager",
        NM,
        "WirelessEnabled",
    )?;
    v.get::<bool>()
        .ok_or_else(|| err("WirelessEnabled is not a boolean"))
}

pub(crate) fn wifi(state: &str) -> Result<String, ExecError> {
    let on = want(state, wifi_on()?)?;
    set_property(
        gio::BusType::System,
        NM,
        "/org/freedesktop/NetworkManager",
        NM,
        "WirelessEnabled",
        on.to_variant(),
    )?;
    Ok(format!("Wi-Fi is {}", on_off(on)))
}

fn wpctl(args: &[&str]) -> Result<String, ExecError> {
    let out = std::process::Command::new("wpctl")
        .args(args)
        .output()
        .map_err(|e| err(format!("wpctl: {e}")))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        Err(err(format!(
            "wpctl: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

/// The default sink's volume, 0–100, and whether it is muted (read-only).
pub fn volume() -> Result<(f64, bool), ExecError> {
    // "Volume: 0.70" or "Volume: 0.70 [MUTED]"
    let text = wpctl(&["get-volume", "@DEFAULT_AUDIO_SINK@"])?;
    let level = text
        .split_whitespace()
        .nth(1)
        .and_then(|v| v.parse::<f64>().ok())
        .ok_or_else(|| err(format!("wpctl said {text:?}")))?;
    Ok(((level * 100.0).round(), text.contains("[MUTED]")))
}

/// Whether the default source (the microphone) is muted (read-only). A
/// muted microphone is exactly silence to a listener (SPIKE-E), so voice
/// setup and status ask `wpctl` instead — and never change it: muting is
/// the user's own switch.
pub fn source_muted() -> Result<bool, ExecError> {
    let text = wpctl(&["get-volume", "@DEFAULT_AUDIO_SOURCE@"])?;
    Ok(text.contains("[MUTED]"))
}

pub(crate) fn volume_step(direction: &str) -> Result<String, ExecError> {
    let step = match direction {
        "up" => "5%+",
        "down" => "5%-",
        other => return Err(err(format!("unknown direction {other:?}"))),
    };
    wpctl(&["set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SINK@", step])?;
    Ok(format!("Volume {:.0}%", volume()?.0))
}

pub(crate) fn volume_set(level: f64) -> Result<String, ExecError> {
    let level = level.clamp(0.0, 100.0);
    wpctl(&[
        "set-volume",
        "@DEFAULT_AUDIO_SINK@",
        &format!("{}%", level.round()),
    ])?;
    Ok(format!("Volume {level:.0}%"))
}

pub(crate) fn mute(state: &str) -> Result<String, ExecError> {
    let muted = want(state, volume()?.1)?;
    wpctl(&[
        "set-mute",
        "@DEFAULT_AUDIO_SINK@",
        if muted { "1" } else { "0" },
    ])?;
    Ok(if muted {
        "Muted".into()
    } else {
        "Unmuted".into()
    })
}

/// MPRIS players on the session bus, the playing one first.
pub fn players() -> Result<Vec<(String, String)>, ExecError> {
    let names = call(
        gio::BusType::Session,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ListNames",
        None,
    )?;
    let names: Vec<String> = names.child_value(0).get().unwrap_or_default();
    let mut players: Vec<(String, String)> = names
        .into_iter()
        .filter(|n| n.starts_with(MPRIS))
        .map(|n| {
            let status = get_property(
                gio::BusType::Session,
                &n,
                "/org/mpris/MediaPlayer2",
                PLAYER,
                "PlaybackStatus",
            )
            .ok()
            .and_then(|v| v.get::<String>())
            .unwrap_or_default();
            (n, status)
        })
        .collect();
    let rank = |s: &str| match s {
        "Playing" => 0,
        "Paused" => 1,
        _ => 2,
    };
    players.sort_by_key(|(_, s)| rank(s));
    Ok(players)
}

pub(crate) fn media(command: &str) -> Result<String, ExecError> {
    let method = match command {
        "play_pause" => "PlayPause",
        "play" => "Play",
        "pause" => "Pause",
        "next" => "Next",
        "previous" => "Previous",
        "stop" => "Stop",
        other => return Err(err(format!("unknown media command {other:?}"))),
    };
    let (player, _) = players()?
        .into_iter()
        .next()
        .ok_or_else(|| err("no media player is running"))?;
    call(
        gio::BusType::Session,
        &player,
        "/org/mpris/MediaPlayer2",
        PLAYER,
        method,
        None,
    )?;
    Ok(format!(
        "{method} sent to {}",
        player.trim_start_matches(MPRIS)
    ))
}

fn settings(schema: &str) -> Result<gio::Settings, ExecError> {
    gio::SettingsSchemaSource::default()
        .and_then(|s| s.lookup(schema, true))
        .map(|_| gio::Settings::new(schema))
        .ok_or_else(|| err(format!("{schema} is not installed")))
}

/// Whether the dark style is on (read-only).
pub fn dark_on() -> Result<bool, ExecError> {
    Ok(settings("org.gnome.desktop.interface")?.string("color-scheme") == "prefer-dark")
}

pub(crate) fn dark(state: &str) -> Result<String, ExecError> {
    let on = want(state, dark_on()?)?;
    settings("org.gnome.desktop.interface")?
        .set_string("color-scheme", if on { "prefer-dark" } else { "default" })
        .map_err(err)?;
    gio::Settings::sync();
    Ok(format!("Dark style is {}", on_off(on)))
}

/// Whether Do Not Disturb is on (read-only).
pub fn dnd_on() -> Result<bool, ExecError> {
    Ok(!settings("org.gnome.desktop.notifications")?.boolean("show-banners"))
}

pub(crate) fn dnd(state: &str) -> Result<String, ExecError> {
    let on = want(state, dnd_on()?)?;
    settings("org.gnome.desktop.notifications")?
        .set_boolean("show-banners", !on)
        .map_err(err)?;
    gio::Settings::sync();
    Ok(format!("Do Not Disturb is {}", on_off(on)))
}

/// Whether the night light is on (read-only).
pub fn night_light_on() -> Result<bool, ExecError> {
    Ok(settings("org.gnome.settings-daemon.plugins.color")?.boolean("night-light-enabled"))
}

pub(crate) fn night_light(state: &str) -> Result<String, ExecError> {
    let on = want(state, night_light_on()?)?;
    settings("org.gnome.settings-daemon.plugins.color")?
        .set_boolean("night-light-enabled", on)
        .map_err(err)?;
    gio::Settings::sync();
    Ok(format!("Night light is {}", on_off(on)))
}

pub(crate) fn night_light_temperature(kelvin: f64) -> Result<String, ExecError> {
    let kelvin = kelvin.clamp(1700.0, 6500.0).round() as u32;
    settings("org.gnome.settings-daemon.plugins.color")?
        .set_uint("night-light-temperature", kelvin)
        .map_err(err)?;
    gio::Settings::sync();
    Ok(format!("Night light warmth is {kelvin} K"))
}

/// Whether airplane mode is on (read-only).
pub fn airplane_on() -> Result<bool, ExecError> {
    let path = "/org/gnome/SettingsDaemon/Rfkill";
    let v = get_property(
        gio::BusType::Session,
        RFKILL,
        path,
        RFKILL,
        "AirplaneMode",
    )?;
    v.get::<bool>()
        .ok_or_else(|| err("AirplaneMode is not a boolean"))
}

pub(crate) fn airplane(state: &str) -> Result<String, ExecError> {
    let on = want(state, airplane_on()?)?;
    let path = "/org/gnome/SettingsDaemon/Rfkill";
    set_property(
        gio::BusType::Session,
        RFKILL,
        path,
        RFKILL,
        "AirplaneMode",
        on.to_variant(),
    )?;
    Ok(format!("Airplane mode is {}", on_off(on)))
}

const POWER_PROFILES: &str = "net.hadess.PowerProfiles";

pub(crate) fn power_profile(profile: &str) -> Result<String, ExecError> {
    if !["balanced", "performance", "power-saver"].contains(&profile) {
        return Err(err(format!("unknown power profile {profile:?}")));
    }
    let path = "/net/hadess/PowerProfiles";
    set_property(
        gio::BusType::System,
        POWER_PROFILES,
        path,
        POWER_PROFILES,
        "ActiveProfile",
        profile.to_variant(),
    )?;
    Ok(format!("Power profile is {profile}"))
}

pub(crate) fn media_seek(direction: &str, seconds: f64) -> Result<String, ExecError> {
    let offset = match direction {
        "forward" => (seconds.clamp(1.0, 60.0) * 1_000_000.0) as i64,
        "back" => -((seconds.clamp(1.0, 60.0) * 1_000_000.0) as i64),
        other => return Err(err(format!("unknown seek direction {other:?}"))),
    };
    let (player, _) = players()?
        .into_iter()
        .next()
        .ok_or_else(|| err("no media player is running"))?;
    call(
        gio::BusType::Session,
        &player,
        "/org/mpris/MediaPlayer2",
        PLAYER,
        "Seek",
        Some((offset,).to_variant()),
    )?;
    Ok(format!("Seeked {direction} {seconds:.0}s"))
}

fn bluetooth_devices() -> Result<Vec<(String, String)>, ExecError> {
    let out = std::process::Command::new("bluetoothctl")
        .args(["devices"])
        .output()
        .map_err(|e| err(format!("bluetoothctl: {e}")))?;
    if !out.status.success() {
        return Err(err("bluetoothctl could not list devices"));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("Device ")?;
            let (mac, name) = rest.split_once(' ')?;
            Some((mac.to_owned(), name.to_owned()))
        })
        .collect())
}

pub(crate) fn bluetooth_connect(direction: &str, wanted: &str) -> Result<String, ExecError> {
    let devices = bluetooth_devices()?;
    let wanted = wanted.trim().to_lowercase();
    let matches: Vec<_> = devices
        .into_iter()
        .filter(|(_, name)| name.to_lowercase().contains(&wanted))
        .collect();
    let (mac, name) = match matches.as_slice() {
        [] => return Err(err(format!("no paired Bluetooth device matches {wanted:?}"))),
        [one] => one.clone(),
        many => {
            let names = many
                .iter()
                .map(|(_, n)| n.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(err(format!("more than one device matches: {names}")));
        }
    };
    let verb = match direction {
        "connect" => "connect",
        "disconnect" => "disconnect",
        other => return Err(err(format!("unknown Bluetooth action {other:?}"))),
    };
    let out = std::process::Command::new("bluetoothctl")
        .args([verb, &mac])
        .output()
        .map_err(|e| err(format!("bluetoothctl: {e}")))?;
    if out.status.success() {
        Ok(format!("{verb}ed {name}"))
    } else {
        Err(err(format!(
            "bluetoothctl {verb} {name}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states() {
        assert!(want("on", false).unwrap());
        assert!(!want("off", true).unwrap());
        assert!(want("toggle", false).unwrap());
        assert!(want("maybe", false).is_err());
    }

    /// Reads only; nothing on the machine changes.
    #[test]
    #[ignore = "needs the live session"]
    fn live_reads() {
        println!("bluetooth on: {:?}", bluetooth_on());
        println!("wifi on: {:?}", wifi_on());
        println!("volume: {:?}", volume());
        println!("players: {:?}", players());
        println!("dark: {:?}  dnd: {:?}", dark_on(), dnd_on());
        assert!(volume().is_ok());
        assert!(dark_on().is_ok());
        assert!(dnd_on().is_ok());
        assert!(players().is_ok());
    }

    /// Sets each switch to the value it already has: the code path runs,
    /// nothing changes.
    #[test]
    #[ignore = "needs the live session"]
    fn live_writes_are_no_ops() {
        let was_dark = dark_on().unwrap();
        dark(on_off(was_dark)).unwrap();
        assert_eq!(dark_on().unwrap(), was_dark);
        let was_dnd = dnd_on().unwrap();
        dnd(on_off(was_dnd)).unwrap();
        assert_eq!(dnd_on().unwrap(), was_dnd);
        if let Ok(bt) = bluetooth_on() {
            bluetooth(on_off(bt)).unwrap();
            assert_eq!(bluetooth_on().unwrap(), bt);
        }
        let (level, muted) = volume().unwrap();
        volume_set(level).unwrap();
        mute(on_off(muted)).unwrap();
        assert_eq!(volume().unwrap(), (level, muted));
    }
}
