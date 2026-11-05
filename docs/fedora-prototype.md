# Fedora prototype

**Status:** Implemented on the development machine · **Date:** 2026-11-05

This is the first running Sam. It replaces Ida on one Fedora workstation and
proves the deterministic core, the Fedora adapters, the panel, the quick
actions and the local speech path. It is not the final Essential OS service.

---

## What runs

- **`sam-core`** — pure Rust. Registry, normalisation, grammar, lexical and
  fuzzy ranking, confidence, typed intents, policy and execution results.
- **`sam-linux`** — Fedora/GNOME adapters. Apps, LocalSearch files, projects,
  paths, live system state, hardware, networking, settings navigation,
  documentation, fixed actions and systemd user timers.
- **`sam-voice`** — local, offline English speech input. Push-to-talk, no
  generation, no network.
- **`sam`** — GTK4/libadwaita panel, CLI, setup and the GApplication service.

The panel is a 728×58 pill on an onyx surface with a results card below it,
following `docs/design/search-bar.svg` and `docs/design/app-icon.svg`.

## Search domains

| Domain | Source | Examples |
|---|---|---|
| Applications | GIO desktop entries | `chrome`, `screenshot tool` |
| Files and folders | LocalSearch via TinySPARQL, bounded walk fallback | `find the invoice PDF`, `~/Downloads` |
| Projects | configured roots | `photodesk` |
| Settings | app catalogue plus a curated panel map | `where is night light settings` |
| Live system | `/proc`, `/sys`, `statvfs`, fixed `ps` | `what is using my RAM`, `disk space`, `uptime` |
| Hardware | sysfs, DRM, thermal, PipeWire | `what GPU do I have`, `audio output`, `how hot` |
| Networking | NetworkManager, BlueZ | `which network am I connected to`, `bluetooth` |
| Local documentation | `/usr/share/man`, `/usr/share/doc` names | `man page for printf` |
| Web handoff | a `url.open` result only | `how many planets…` |

The file index is LocalSearch's. Sam does not build a second file index. The
documentation index is names and section labels only in this slice.

## Actions

Safe, immediate:

- open, launch, reveal, copy
- lock, volume, mute, media play/pause/next/previous/seek
- Bluetooth on/off and connect/disconnect a paired device
- Wi-Fi on/off, airplane mode
- night light on/off and colour temperature
- dark style, focus / Do Not Disturb
- power profile, terminal
- timers and alarms

Confirmation required by policy:

- suspend, log out, restart, power off

There is no `>command` path, no file deletion, no app scripting and no
arbitrary shell. Every action is a registry entry with typed arguments and a
fixed handler.

Timers and alarms use transient systemd user timers plus
`~/.local/state/sam/timers.json`. They are user-requested state, not personal
memory. The notification is sent by `sam notify <id>`.

A timer started from the panel shows a live countdown card with a Cancel
button. The card stays until cancelled or finished; closing it hides the card
but the timer continues. GNOME Clocks has no start-timer API, so this remains a
Sam-owned timer until the Essential OS Clock app exists. See
[`gnome-limitations.md`](gnome-limitations.md).

## Speech

The guide is preserved at [`design/SPEECH_GUIDE.md`](design/SPEECH_GUIDE.md).

- Moonshine Small Streaming English only.
- Push-to-talk: hold the microphone, release to stop.
- On the final transcript, Sam runs one safe result immediately: an explicit
  open/launch verb, or a quick-action command. Risky actions show the normal
  confirmation; information and file results stay on screen for review.
- Models live under `~/.local/share/sam/models/moonshine-streaming/`.
- `sam voice status` reports the model path; `sam voice setup` downloads and
  verifies the pinned archive (`sha256 dbb3e1c1…`). `--archive PATH` installs
  an offline copy.
- The model is loaded once by the service and kept warm. Measured load is about
  0.44 s warm and about 1.0 s cold; a 4 s clip transcribes in about 0.34 s on
  this CPU (≈11× realtime).
- If the model or microphone is unavailable, the panel still opens and typing
  still works.

The model is an input method. It never answers a query, and no audio or text
leaves the machine.

## CLI

```sh
sam                         # show the panel
sam show "what is using my RAM"
sam query "what GPU do I have"
sam query "does htop exist" --json
sam run "lock my computer" --dry-run
sam run "set a timer for 10m"
sam voice status
sam serve                   # JSON lines on $XDG_RUNTIME_DIR/sam.sock
sam config                  # terminal configuration menu
sam status                  # service, voice, model and memory
sam doctor
sam bench
sam log
```

`sam query --json` returns the same structured rows the panel uses, with
`title`, `subtitle`, `kind`, `score` and the inert `intent`.

`sam serve` exposes the same answer over a Unix socket:

```sh
printf '%s\n' '{"query":"what is using my RAM"}' | nc -U "$XDG_RUNTIME_DIR/sam.sock"
```

It returns `{"status":"ok","query":…,"elapsed_ms":…,"results":[…]}` or
`{"status":"no_confident_match",…}`. There is no TCP listener and no network
door.

## Build and install

```sh
cargo test --workspace
cargo build --release
packaging/rpm/make-vendor-tarball.sh
rpmbuild -ba packaging/rpm/sam.spec
sudo dnf install ~/rpmbuild/RPMS/x86_64/sam-0.1.0-1.fc45.x86_64.rpm
sam setup
```

`sam setup` takes `<Super>space`, remembers what it displaced and enables
`io.github.luishowin.Sam.service`. `sam setup --undo` restores the binding and
disables the service.

`sam config` is the terminal menu: status, version, service control, voice
on/off and eager/lazy/off modes, model setup, memory/CPU, config path and
re-running setup. The same values can be changed directly:

```sh
sam config get voice.enabled
sam config set voice.enabled false
sam config set voice.mode lazy
sam config set voice.auto_run false
```

## Measured baseline

The numbers below are from the development laptop. They are not release gates
and must be re-measured on the minimum reference machine.

| Metric | Observed |
|---|---|
| Registry/engine query, warm | 3–6 ms typical |
| Complete LocalSearch query | 10–55 ms |
| Live system answer | 3–30 ms |
| Idle service RSS, speech model warm | ~295 MB |
| Idle service RSS, without speech | ~15 MB |
| Network egress | none in the Sam process |
| Generative model | none in the runtime |

The `sam bench` command prints current medians for a fixed phrase set.

The speech model takes the process above Sam's provisional 250 MB typical
target. On this 16 GB machine that is an accepted trade for instant
push-to-talk. `sam config` can switch to `lazy` (first press loads the model)
or `off` (no speech worker; roughly the non-voice footprint). Idle CPU, not
resident memory, is the battery-relevant number; it is near zero after warm-up.

## Known limitations

- The panel is a Fedora/GNOME prototype, not the Essential OS shell.
- GNOME Clocks cannot be told to start a timer; see
  [`gnome-limitations.md`](gnome-limitations.md).
- File search depends on LocalSearch indexing; the fallback walk is bounded.
- Local documentation is name-level only, not full-text.
- The speech model is downloaded by `sam voice setup`; it is not shipped in
  the RPM. Its 158 MB on disk and ~240–300 MB resident cost exceed the
  provisional 4 GB budget. The config menu makes `lazy` and `off` available.
- Settings navigation uses GNOME panel IDs; other desktops need their own map.
- The typed local API is a JSON-lines Unix socket today; versioning, capability
  checks and a formal schema are still to come.
- Registry entries are still hand-authored. The build-time authoring pipeline
  from the specification is not implemented.
- `ort` fetches its ONNX Runtime binary during the first build. The runtime is
  then linked into the package and makes no network calls, but a fully offline
  RPM builder needs that binary pinned in the vendor set.

## What this proves

- A broad local search surface can feel immediate without a language model.
- Live system answers and quick actions fit the same registry and policy path.
- Voice can be an input method without becoming a reasoning system.
- Sam works with no network and no conversational memory.
