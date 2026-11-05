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

## Other gaps

- No standard way to ask the shell to show a persistent countdown.
- No standard API for “open this Settings page” beyond launching the
  `gnome-*-panel.desktop` entry.
- No portable way to read the focused window or control another application's
  UI without accessibility or compositor-specific tools.
