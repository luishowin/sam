# Windows 11 port plan

**Status:** Plan only, not started · **Date:** 2026-11-10 · **Scope:** A Windows 11
build friends can install and use, with apps/files search, settings deep-links,
basic system actions and voice input

This document records the plan agreed for the first Windows build. Nothing here
is implemented: the current tree is still the Fedora/GNOME prototype described
in [`fedora-prototype.md`](fedora-prototype.md). The purpose is to keep the
scope, the architecture and the known risks in one place until the work starts.

---

## Why this is a port, not a build

Sam is not a portable application that only needs a different Cargo target. The
current build assumes Fedora and GNOME at every layer above `sam-core`:

| Area | Current dependency |
|---|---|
| UI | GTK4 + libadwaita (`gtk4 0.11`, `libadwaita 0.9`), GNOME dark styling, `adw::Application` |
| Adapter | `crates/sam` depends unconditionally on `sam-linux` (about 5.4k lines): GNOME apps, TinySPARQL, D-Bus, `systemctl`, `gsettings`, `bluetoothctl`, `wpctl`, `notify-send`, `systemd-run` |
| IPC | Unix sockets (`std::os::unix::net`) in `sam serve` and the Needle client; `$XDG_RUNTIME_DIR`; systemd user units |
| Window control | A GNOME Shell 51 extension over D-Bus |
| Voice | cpal/ALSA (PipeWire), model under `~/.local/share`, Linux setup paths |
| Platform gating | No `cfg(target_os)` or `cfg(windows)` in the workspace |
| Build and packaging | No Windows target, no CI, no GTK runtime bundling, no installer |

`cargo build --target x86_64-pc-windows-msvc` therefore fails in `sam-linux`
before GTK is involved. The good news is the architecture: `sam-core` is
deliberately platform-free (its dependency list is held to an allowlist by
`crates/sam-core/tests/purity.rs`), and the seams a port needs already exist —
`Host`, `Provider`, and an `Executor` that accepts only an `Approved` intent.
The registry, grammar, ranking and policy can run on Windows unchanged. The
work is an adapter, a UI runtime and packaging, not a second engine.

## Scope of the first preview

The target is a single installable build that a non-developer can run on a
normal Windows 11 machine.

**In scope**

- the same GTK4/libadwaita panel, or a faithful equivalent;
- Start Menu application search and known-folder file search;
- places (Home, Desktop, Documents, Downloads, Pictures, Music, Videos);
- settings navigation through `ms-settings:` deep links;
- basic actions: open/reveal, lock, sleep, sign out, volume, empty trash,
  clock/calendar/notes;
- local push-to-talk voice input with the Moonshine model;
- `sam doctor` and `sam status` for diagnostics;
- a portable zip and an unsigned installer.

**Out of scope for the first preview**

- **Window control.** There is no GNOME Shell extension equivalent. `window.*`
  intents report `source_unavailable` cleanly until a Win32 window helper
  exists. This is the largest single piece of Windows-specific work and was
  explicitly deferred.
- **The Needle router.** Its sidecar is Python, `cactus-compute`, a Unix socket
  and a systemd user unit. Tier 0 is the product; the router can follow once a
  named-pipe transport and a Windows service story exist.
- **Tier 3 local answers** and semantic embeddings, which are not shipped on
  Fedora either.
- **Code signing.** The first artifacts will be unsigned and will trigger
  SmartScreen; see [Packaging](#packaging-and-release).

## Architecture

The core stays shared. The new crate mirrors the module surface the panel
already imports, so `crates/sam/src/panel.rs` needs few or no changes.

```toml
# crates/sam/Cargo.toml
[target.'cfg(target_os = "linux")'.dependencies]
sam-linux.workspace = true

[target.'cfg(windows)'.dependencies]
sam-windows = { path = "../sam-windows" }
```

```rust
// crates/sam/src/platform.rs
#[cfg(target_os = "linux")]
pub use sam_linux::{config, executor, log, timers, window};

#[cfg(windows)]
pub use sam_windows::{config, executor, log, timers, window};
```

`sam-windows` implements the public surface the rest of the binary expects:
`config::Config`, `executor::Executor`, `timers`, `log`, `window`, plus the
providers `world.rs` needs — `apps::AppIndex`, `files::FileIndex`,
`info::{Settings, SystemInfo}`, `paths::PathProvider`, `projects::ProjectIndex`
and `host::WindowsHost`. The panel then compiles for both platforms, which is
the point of the exercise.

Two Linux-shaped pieces need a Windows equivalent:

- **Local IPC.** `serve.rs` and `needle.rs` use Unix sockets. The JSON protocol
  stays; the transport becomes a named pipe on Windows. `serve`'s socket path
  becomes a pipe name; the client code moves behind the same trait.
- **Single instance and `sam show`.** GApplication's single-instance behaviour
  on Windows needs verifying during milestone 0. The fallback is a named mutex
  for the instance check plus a named pipe for `show` and `toggle`, reusing the
  existing JSON protocol.

## Milestones

Estimates assume focused work and no large surprises. The range is wide because
GTK-on-Windows and the ONNX Runtime link are the two places estimates usually
slip.

| # | Deliverable | Estimate |
|---|---|---|
| 0 | **Walking skeleton.** `sam-windows` crate, target gating, CI workflow, `sam --version` running on a Windows runner and uploaded as an artifact | 2–3 days |
| 1 | **Core on Windows.** Gate `serve.rs`, `needle.rs` and Linux-only CLI commands; add the named-pipe transport; move paths to `%APPDATA%`/`%LOCALAPPDATA%`; `cargo test -p sam-core` green on Windows | 3–4 days |
| 2 | **Adapter MVP.** Places via `SHGetKnownFolderPath`; Start Menu `.lnk` apps; known-folder file walk; `ms-settings:` links; `sysinfo` for live state; `ShellExecuteW`, lock, sleep, sign-out, volume and trash | 7–10 days |
| 3 | **Panel on Windows.** MSYS2 GTK4/libadwaita build, bundled icons/fonts/theme, single instance, global hotkey and tray icon | 4–6 days |
| 4 | **Voice.** cpal/WASAPI capture, model under `%LOCALAPPDATA%`, bundled `onnxruntime.dll`, push-to-talk through a low-level keyboard hook | 3–5 days |
| 5 | **Packaging.** Recursive DLL bundling, portable zip, unsigned Inno Setup installer and a tag-triggered release workflow | 3–4 days |
| 6 | **Feedback kit.** Windows `sam doctor`, a limitations page, log locations and a feedback template | 1–2 days |

**Total: roughly 4–6 weeks of focused work.** Milestones 0–2 produce a usable
CLI before any UI work; milestone 3 is the first build friends can try.

### Milestone 0 — walking skeleton

The de-risking step. Produce a real `.exe` from CI whose only behaviour is
`--version`, `--help` and a trivial `sam query` that returns
`no_confident_match`. That proves the toolchain, GTK provisioning, ONNX Runtime
and the artifact path before adapter code exists.

Exit gate: a GitHub Actions run on a tag or branch uploads
`sam-<version>-windows-x64.zip`, and the binary starts on a clean Windows 11
machine (or VM).

### Milestone 1 — core on Windows

- `crates/sam/src/platform.rs` introduced; `world.rs` selects providers by
  target.
- `serve.rs` and `needle.rs` compiled only where their transport exists;
  `needle` gets a null implementation on Windows so Tier 0 is unaffected.
- `cli.rs` Linux-only commands (`log`, parts of `status`, `setup`) are gated;
  Windows equivalents are added where they make sense.
- `sam-voice` model root moves from XDG paths to `%LOCALAPPDATA%\sam\models`.
- `cargo test -p sam-core` passes on Windows. Adapter tests are added with the
  adapter in milestone 2.

### Milestone 2 — Windows adapter

New crate `crates/sam-windows`, mirroring the `sam-linux` module layout:

| Module | Implementation |
|---|---|
| `host.rs` | Known folders via `SHGetKnownFolderPath`; clock via `SystemTime`; `file://` URIs |
| `apps.rs` | Start Menu shortcuts in the user and `ProgramData` trees, resolved with `IShellLinkW`; UWP (`shell:AppsFolder`) deferred |
| `files.rs` | Bounded walk of known folders with depth and time budgets; Windows Search integration deferred |
| `info.rs`, `system.rs` | `sysinfo` for CPU/RAM/disk/uptime/battery; `windows` crate for network and volume |
| `settings.rs` | A small map from settings intent to `ms-settings:` page |
| `executor.rs` | `ShellExecuteW` for open/reveal/URLs; `LockWorkStation`; `SetSuspendState`; `ExitWindowsEx`; endpoint volume |
| `config.rs`, `paths.rs` | `%APPDATA%\sam\config.toml`, `%LOCALAPPDATA%\sam`; home via `USERPROFILE` |
| `timers.rs` | In-process timers with toast notifications; Task Scheduler integration later |
| `projects.rs` | Reuse the existing logic with Windows path handling |
| `window.rs` | Returns `false` from `available()` and a clear `source_unavailable` error |

Exit gate: `sam query` and `sam run` behave on Windows 11 for apps, files,
places, settings, live state and the basic actions, with adapter tests.

### Milestone 3 — panel on Windows

- Build with MSYS2 `mingw-w64-x86_64-gtk4`, `libadwaita`, `adwaita-icon-theme`
  and `cantarell-fonts`.
- Bundle the Adwaita icon theme and font so the panel looks like the Fedora
  panel rather than falling back to Segoe.
- Replace Super+Space with **Ctrl+Alt+Space** (configurable), because Windows
  reserves Win+Space for keyboard layouts. Add a tray icon with Show, Settings
  and Quit.
- Autostart through a shortcut in `shell:startup` or
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
- Verify focus: Windows foreground-lock rules may block the panel from
  appearing. Fall back to flashing the taskbar entry or an attach-thread
  activation if `SetForegroundWindow` is refused.

Exit gate: the panel opens from the hotkey and tray, renders dark and correctly
on a 150% HiDPI display, and `sam show TEXT` fills the pill.

### Milestone 4 — voice on Windows

- cpal/WASAPI input; the existing resampling and the `sam-voice` protocol stay.
- `ort` currently uses its default `download-binaries` and `copy-dylibs`
  features. Under `windows-gnu` this may fail to link against the MSVC-built
  ONNX Runtime; in that case switch to `load-dynamic` and ship
  `onnxruntime.dll` beside the binaries, or move the whole build to MSVC with
  gvsbuild.
- The model archive is a plain `.tar.gz` with a SHA-256 check, so setup works
  unchanged; only the destination root changes.
- Push-to-talk needs a key-up event, which `RegisterHotKey` does not provide.
  Use a `WH_KEYBOARD_LL` hook, shared with the global hotkey module.
- Windows microphone privacy settings can deny capture; `sam doctor` must say
  so plainly rather than failing silently.

Exit gate: hold the push-to-talk key, speak, release, and the transcript
appears in the pill; `sam voice status` reports the model and microphone.

### Milestone 5 — packaging and release

- Collect `sam.exe`, the runtime DLLs (recursively via `objdump -p`), GLib
  schemas, GdkPixbuf loaders, icons, fonts and `onnxruntime.dll`.
- Produce `sam-<version>-windows-x64.zip` and
  `SamSetup-<version>.exe` (Inno Setup, per-user install, Start Menu shortcut,
  uninstaller).
- Expect roughly 40–80 MB compressed because of the GTK runtime. State the size
  in the release notes.
- Artifacts are unsigned at first, so Windows shows SmartScreen. The download
  page must say *More info → Run anyway*, and explain why. Code signing is a
  later decision (certificate cost and identity checks).
- The release workflow runs on `windows-latest`, caches the MSYS2 package
  download, `~/.cargo` and `target/`, and attaches both artifacts to the
  GitHub release.

### Milestone 6 — feedback kit

- `sam doctor` on Windows: OS build, GTK version, sources available, action
  count, microphone status, model status, config and log paths.
- `docs/windows-preview.md`: what works, what does not, the SmartScreen
  explanation and how to report a problem.
- A short feedback template: Windows build number, what was typed, what was
  expected, what happened, and the last log lines.
- Optional: a "Copy diagnostics" button in the panel.

## Risks

| Risk | Impact | Mitigation |
|---|---|---|
| GTK/Adw renders or bundles badly on Windows | The panel is unusable; the preview fails | Build a hello-world panel in milestone 0, before porting the real UI |
| `ort` does not link under `windows-gnu` | No voice build | `load-dynamic` plus a shipped DLL, or switch to MSVC plus gvsbuild |
| GApplication single-instance needs D-Bus | `sam show` and the hotkey cannot reach the running panel | Named mutex plus named-pipe transport; verify in milestone 0 |
| Windows foreground-lock refuses activation | Hotkey appears to do nothing | Test `SetForegroundWindow`; add a fallback path and document it |
| Start Menu and UWP coverage is uneven | Missing apps compared with GNOME | Ship `.lnk` apps first; add `shell:AppsFolder` after feedback |
| SmartScreen blocks the first run | Friends think the build is broken | Clear download instructions; sign later |
| No window control | Some registry rows are unavailable | `source_unavailable` with a plain explanation; add a Win32 helper later |
| No Needle router | The phrasing tail is weaker than on Fedora | Tier 0 only in the preview; the router is a follow-up, not a regression |
| Binary size | Slow download, surprise | Publish the size; keep the zip to one runtime |

## Open decisions

These can wait until the work starts:

- **Toolchain:** `windows-gnu` with MSYS2 (fast to provision) versus
  `windows-msvc` with gvsbuild (official, slower). Start with gnu; switch if
  ONNX Runtime forces it.
- **Hotkey default:** Ctrl+Alt+Space versus Alt+Space with a first-run prompt.
- **Installer:** Inno Setup versus MSIX. MSIX needs signing and suits a store
  path; Inno suits a quick preview.
- **Needle transport:** named pipe with a per-user token, or loopback TCP with
  a random port and token. Named pipe fits the existing security posture.
- **Code signing:** whether to buy a certificate before wider distribution.

## First step

Start with milestone 0 on a `windows-port` branch: the `sam-windows` skeleton,
target gating and a CI workflow that uploads a working `sam.exe`. That run is the
gate for everything else — if GTK, ONNX Runtime and the artifact path work, the
adapter and panel work is ordinary porting; if they do not, the plan changes
before weeks are spent.

Until then, the supported platform remains Fedora 45 / GNOME 51, as recorded in
[`fedora-prototype.md`](fedora-prototype.md) and
[`stable-baseline.md`](stable-baseline.md).
