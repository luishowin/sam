# Window control

**Status:** Design · **Scope:** Minimising, maximising, moving, centring and closing application windows on Fedora GNOME

Sam can already open and launch things. Window control is the missing half: "minimise Spotify",
"maximise Chrome", "move Chrome to the right", "centre the Wikipedia window", "minimise
everything". On GNOME Wayland this needs a small Shell extension, because Wayland deliberately
prevents one client from manipulating another's surfaces.

---

## Why an extension

- `wmctrl` and `xdotool` only reach XWayland windows.
- `org.gnome.Shell.Eval` has been disabled since GNOME 41.
- The only process allowed to move windows is `gnome-shell` itself.
- The supported pattern is a GNOME Shell extension exposing a narrow D-Bus interface.

Sam ships its own minimal MIT extension. Third-party extensions exist (`window-control`,
`ws-dbus`, `window-commander`) but are broader or GPL-2.0; Sam's is narrow and auditable.

## Components

| Component | Role |
|---|---|
| `sam-window@luishowin.github.io` (GJS extension) | Runs in gnome-shell, registers a D-Bus object, owns the only code that touches `Meta.Window` |
| D-Bus interface | Session bus, destination `org.gnome.Shell`, path `/org/gnome/Shell/Extensions/SamWindow` |
| `WindowBackend` (Rust trait) | `sam-linux` calls D-Bus; tests use a fake backend |
| Registry actions | `window.*` entries with typed args, risk and confirmation |

## D-Bus interface

```text
List() -> s                      JSON array of windows
Activate(u id) -> b
Minimize(u id) -> b
Unminimize(u id) -> b
Maximize(u id) -> b
Unmaximize(u id) -> b
MoveResize(u id, i x, i y, i w, i h) -> b
GetWorkArea() -> s               JSON {x, y, width, height}
Close(u id) -> b
```

`List` returns `id`, `title`, `wm_class`, `wm_class_instance`, `pid`, `workspace`, `monitor`,
`x`, `y`, `width`, `height`, `focused` and `window_type`. No screenshots, no input injection,
no eval.

The extension has no user interface. Disabling it removes the D-Bus object.

## Actions

| Action | Arguments | Risk | Confirmation |
|---|---|---|---|
| `window.list` | none | Read | Never |
| `window.activate` | `target` | LowRisk | Never |
| `window.minimize` | `target` | LowRisk | Never |
| `window.maximize` | `target` | LowRisk | Never |
| `window.restore` | `target` | LowRisk | Never |
| `window.center` | `target` | LowRisk | Never |
| `window.move` | `target`, `position` | LowRisk | Never |
| `window.show_desktop` | none | LowRisk | Never |
| `window.close` | `target` | Destructive | Always |

`window.close` is included, but it always confirms. It can lose unsaved work, and neither a
spoken phrase nor an ambiguous match may close a window by itself.

## Resolution

`target` is a string. The resolver tries, in order:

1. exact app id (`com.spotify.Client`);
2. exact WM_CLASS or instance;
3. case-insensitive substring of the window title;
4. fuzzy match on app name.

Rules:

- multiple matches → clarify with a numbered list of window title and app;
- no match → `no_confident_match`, no action;
- "minimise everything" bypasses the resolver and applies to all normal windows on the current
  workspace by default.

"Centre Wikipedia" matches the window title, not the app. That is why title matching exists.

## Geometry

- Read the work area from `GetWorkArea`; never hard-code the screen size.
- `left`, `right`, `top`, `bottom`: half the work area.
- `center`: centred at the current size, clamped to the work area.
- Monitor moves are a later refinement, not v1.
- Maximise uses the Shell's maximise, not a manual rectangle, so tiling and struts are respected.

Accept both spellings: minimise/minimize, maximise/maximize, centre/center.

## Install and undo

- `sam setup` copies the extension to
  `~/.local/share/gnome-shell/extensions/sam-window@luishowin.github.io/`, enables it and
  records the change.
- On Wayland a Shell restart may be required for a new extension. `Alt+F2` → `r` is not
  available; a logout and login is the reliable path. The setup output says so.
- `sam setup --undo` disables and removes the extension.
- If the extension is missing or disabled, window actions return `source_unavailable` with a
  clear message. They stay listed so the user learns why.

## Failure behaviour

| Condition | Behaviour |
|---|---|
| Extension not installed | `source_unavailable`; suggest `sam setup` |
| Extension disabled or crashed | `source_unavailable`; log the D-Bus error |
| D-Bus timeout | `source_unavailable`; no retry loop |
| Window closed between resolve and act | `source_unavailable`; one clear message |
| Multiple matches | Clarify; never act on a guess |
| `window.close` without a yes | Pending confirmation; nothing happens |

## Security

- The extension exposes no eval, no shell, no input injection and no screenshots.
- Any process in the session can call the D-Bus interface; that is the session trust boundary.
  Sam documents it rather than pretending otherwise.
- `window.close` is the only destructive method, and Sam's policy gates it, not the extension.
- The extension is hash-checked with the rest of the bundle and is never updated silently.

## Tests

- A fake `WindowBackend` covers resolution, geometry, multiple matches and failure cases.
- One live check on the development machine: list, minimise, restore, move, and
  close-with-confirmation.
- Record the Shell version the extension was tested on. Treat a new major version as unverified
  until it is checked.

## Status

Design. No extension, D-Bus client or action exists in the current build.
