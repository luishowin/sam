# GNOME limitations

**Status:** Observed on Fedora 45 / GNOME 51 · **Date:** 2026-11-05

These are places where Sam would like a native desktop API and GNOME does not
provide one. They are recorded so the eventual Essential OS Clock app can
replace the workaround with a proper interface.

---

## Clocks: start a timer

GNOME Clocks has **no external API to start a timer**.

- `org.gnome.clocks` exposes `org.gtk.Actions` with only `stop-alarm`,
  `snooze-alarm`, `shortcuts`, `add-location` and `quit`.
- The timer list is stored in GSettings as
  `org.gnome.clocks timers`, type `aa{sv}`, with `duration` and optional
  `name`.
- On startup Clocks loads those entries as **stopped** timers. Writing the
  setting does not start a countdown.
- `timer-setup.start-timer` is an internal widget action, not an exported
  application action.

Sam therefore owns its timers: `timer.set` creates a transient systemd user
timer and `sam notify <id>` sends the completion notification. The panel shows
a live countdown card for timers started in the panel.

### Desired Clock API for Essential OS

When the Essential OS Clock app exists, it should expose a small typed interface
that Sam and Ida can call:

```text
clock.timer_start(duration_seconds, label?) -> timer_id
clock.timer_cancel(timer_id) -> ok
clock.timer_list() -> [timer_id, label, remaining_seconds, state]
clock.alarm_set(time, label?) -> alarm_id
clock.alarm_cancel(alarm_id) -> ok
```

The caller owns confirmation. Sam may propose `clock.timer_start`; the Clock
app performs it. Until then, `timer.set` remains a Sam-owned action and is
documented as such.

## Window management

GNOME Wayland has **no window-management API for external clients**.

- `wmctrl` and `xdotool` only reach XWayland windows.
- `org.gnome.Shell.Eval` has been disabled since GNOME 41.
- Wayland prevents one client from manipulating another client's surfaces.
- The only process with the authority is `gnome-shell` itself.

Sam therefore ships a minimal GNOME Shell extension exposing a narrow D-Bus interface for list,
activate, minimise, maximise, restore, move-resize, work area and close. There is no eval, no
shell and no input injection. It is installed and enabled by `sam setup`, and removed by
`sam setup --undo`.

The extension is version-sensitive. Record the Shell version it was tested on and treat a new
major version as unverified until it is checked. If the extension is missing or disabled,
window actions report `source_unavailable`; they never fall back to an X11 tool or a shell
command. See [`window-control.md`](window-control.md).

## Other gaps

- No standard way to ask the shell to show a persistent countdown.
- No standard API for “open this Settings page” beyond launching the
  `gnome-*-panel.desktop` entry.
- No portable way to read the focused window or control another application's
  UI without accessibility or compositor-specific tools.
