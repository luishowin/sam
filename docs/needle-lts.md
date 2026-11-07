# Needle LTS package

**Status:** Sam 0.2.0 installed as a local candidate, 2026-11-07; not yet the declared LTS
· **Scope:** The local router, its sidecar, and the install path

This document records what is built, what is measured, and what is still a
candidate rather than a release claim.

---

## What the package contains

| Piece | Where |
|---|---|
| Sam binary | installed by `scripts/install-user.sh` or the RPM |
| Needle sidecar | `packaging/needle/sam_needle_sidecar.py` |
| Model | `~/.local/share/sam/needle/sam-needle.cact` |
| Runtime | `~/.local/share/sam/needle/venv` (`cactus-compute==4.0.1`, ~68 MB) |
| User service | `~/.config/systemd/user/sam-needle.service` |
| Socket | `$XDG_RUNTIME_DIR/sam/needle.sock` |
| Window extension | installed by `sam setup` into the user extension directory |
| Probe registry | compiled into `sam-linux` |

The sidecar is optional. If the socket is absent, Sam is the deterministic
Tier 0 engine and nothing waits for a model.

## The model-facing contract

The current 20-layer artifact was trained on a hand-written catalogue that
uses `app`, `path` and `kelvin`. The runtime validates `app_id`, `uri` and
`temperature`. `sam/src/world.rs` is the documented adapter:

- `app.launch {app}` → resolve the installed app name to a desktop ID;
- `file.open {path}` → resolve an existing path to a `file://` URI;
- `display.night_light_temperature {kelvin}` → `temperature`;
- `search.content {query}` → `search.files {query}`;
- `clarify {slot}` → a question, never an action;
- everything else is copied only when the registry already knows the action
  and the argument names validate.

This is not silent translation. An unresolved app, a missing file, an
out-of-range value or an unknown tool is rejected and the deterministic result
stands.

The canonical exporter is in `sam-core/src/schema.rs`; `sam schema` prints it
and `sam schema --fingerprint` prints the compatibility hash. The next
training run should use that exporter so the adapter can become a pure
pass-through.

## Policy

`policy::MODEL_AUTO_ACTIONS` is the reviewed reversible allowlist. A local
model may run those without a person picking the row. It may **not** run:

- `system.lock`, suspend, logout, restart or power-off;
- `window.close`;
- anything destructive, privileged or shell-like;
- an unresolved app, file, window or probe.

Model proposals use `Origin::Model`, never `Origin::UserSelected`. The panel
shows a model result only when the deterministic engine has no confident row;
typing never waits for the model.

## Install

The candidate was installed on 2026-11-07 under `~/.local`; the frozen RPM
remains the rollback. The sidecar is a development install until the
`sam-needle` RPM exists.
The runtime is deliberately separate from the 750 MB training venv:

```sh
python3.12 -m venv ~/.local/share/sam/needle/venv
~/.local/share/sam/needle/venv/bin/pip install "cactus-compute==4.0.1"

# New Sam binary, keybinding, service and window extension:
scripts/install-user.sh

# Sidecar and model:
scripts/install-needle-dev.sh
```

`NEEDLE_RESOURCES` can point at another fine-tune resource directory;
`SAM_NEEDLE_PYTHON` can point at another pinned interpreter.

Rollback:

```sh
systemctl --user disable --now sam-needle.service
scripts/install-needle-dev.sh --uninstall
scripts/install-user.sh --uninstall
sudo dnf install ~/rpmbuild/RPMS/x86_64/sam-0.1.0-1.fc45.x86_64.rpm
```

The frozen baseline is in [`stable-baseline.md`](stable-baseline.md).

## What was measured

On this development machine, using the existing 20-layer artifact:

| Path | Observed |
|---|---:|
| Sidecar warm call, 4–8 tools | ~140–270 ms |
| Sidecar cold call, new tool subset | ~1–2.5 s |
| `turn why fi off` | model → `wifi.set {state: "off"}` |
| `make it quieter` | model → `audio.volume {direction: "down"}` |
| `close chrome` | model → `window.close`, confirmation |
| `list windows` | live GNOME 51 `window.list` returned the open windows |
| Missing/unknown model output | deterministic result stands |

These are development observations, not a p95 release measurement. The
sidecar cold start is real and should be pre-warmed or reported honestly.

## What is still open

- Retrain on the canonical `sam schema` contract, so `app_id`, `uri` and
  `temperature` are the model's own fields.
- A fresh frozen set, real-use set and per-risk safety suite.
- A sidecar p95/p99 and peak-RSS measurement on the minimum reference machine.
- A `sam-needle` RPM with a pinned Python/Cactus runtime and checksums.
- A per-action live GNOME 51 window check: minimise, maximise, move, centre and close with the
  real extension. `window.list` is already verified; the extension loads at login.
- The “install and forget” support window: this is a pinned personal LTS,
  not a multi-year support contract.
