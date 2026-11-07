# Needle sidecar package

This directory carries the local router sidecar and its user service. It is a development
install until the `sam-needle` RPM exists.

## Pieces

| Piece | Installed path |
|---|---|
| Sidecar | `~/.local/libexec/sam-needle/sam_needle_sidecar.py` |
| Model | `~/.local/share/sam/needle/sam-needle.cact` |
| Runtime | `~/.local/share/sam/needle/venv` (`cactus-compute==4.0.1`, ~68 MB) |
| Service | `~/.config/systemd/user/sam-needle.service` |
| Socket | `$XDG_RUNTIME_DIR/sam/needle.sock`, mode `0600` |

The sidecar has no network code, no shell and no execution path. It returns one call (or `[]`)
for the 4–8 tool schemas in the request; Sam validates and decides.

## Build the runtime once

```sh
python3.12 -m venv ~/.local/share/sam/needle/venv
~/.local/share/sam/needle/venv/bin/pip install "cactus-compute==4.0.1"
```

Do not use the 750 MB training venv at runtime; it carries JAX and training extras.

## Install

```sh
NEEDLE_RESOURCES="$HOME/Desktop/needle fine tune resources" \
  scripts/install-needle-dev.sh
```

`NEEDLE_RESOURCES` must contain `sam-needle.cact`. `SAM_NEEDLE_PYTHON` overrides the interpreter.

The service starts with the graphical session. Check it with:

```sh
systemctl --user status sam-needle.service
sam doctor
```

`sam doctor` reports `needle: ready (...)` only when the socket is present.

## Remove

```sh
scripts/install-needle-dev.sh --uninstall
```

This does not remove the Sam binary or the frozen 0.1.0 RPM. See
[`../../docs/needle-lts.md`](../../docs/needle-lts.md) for the full install and rollback.

## Current limits

- The current artifact uses the first-run catalogue (`app`, `path`, `kelvin`); the Rust adapter
  resolves those to the canonical `app_id`, `uri`, `temperature`.
- Cold start for a new tool subset is about 1–2.5 s. Warm calls are roughly 140–270 ms.
- The sidecar reports `confidence = None`; policy and the reviewed allowlist are the gate, not
  a confidence threshold.
