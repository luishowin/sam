# Stable baseline — Sam 0.1.0

**Status:** Frozen known-good build, 2026-11-07 · **Scope:** The installed Fedora
package that is currently trusted on this machine

This is the build to keep when the Needle, window-control and probe work
starts. It is the reference for "does the new build still behave like the
version I liked?"

Do not replace it while building the next release. Install candidates into a
test prefix, or build an RPM and compare first.

**Candidate install (2026-11-07):** Sam 0.2.0 is now installed under
`~/.local` (`scripts/install-user.sh` plus `scripts/install-needle-dev.sh`).
That includes the window extension, probes and the Needle sidecar. The RPM
below is still the rollback; the candidate is not the new baseline until its
gates pass. See [`needle-lts.md`](needle-lts.md).

Candidate paths: `~/.local/bin/sam`, `~/.local/bin/sam-voice`,
`~/.local/libexec/sam-needle/`, `~/.local/share/sam/needle/`,
`~/.config/systemd/user/sam-needle.service`, and the extension under
`~/.local/share/gnome-shell/extensions/sam-window@luishowin.github.io/`.

---

## What is installed

| Field | Value |
|---|---|
| Package | `sam-0.1.0-1.fc45.x86_64` |
| Install date | 2026-11-06 11:10 EAT |
| Build date | 2026-11-06 11:01 EAT |
| Build host | `probook` |
| Fedora | 45 |
| Session | GNOME Shell 51.1, Wayland |
| Service | `io.github.luishowin.Sam.service`, active |
| Action count | 38 |
| Nearest source commit | `38dab7bd8c273d8f0bbc7b867d2821442a5e37bd` — "feat: hold Tab for push-to-talk and resolve the voice model" |

The repository HEAD is later (`f572942` at the time of writing) and contains
documentation and model-benchmark work that is not part of this installed
build. The RPM checksum below is the authoritative artifact; the commit is
the nearest source point, not a proven bit-for-bit match.

## Artifact checksums

| Artifact | SHA-256 |
|---|---|
| `sam-0.1.0-1.fc45.x86_64.rpm` | `6e42fb9fea6ff3e25a8800d80ab086f95095397ce8aab4ed8d04dd521654909b` |
| `/usr/bin/sam` | `d62cc82dbc8a9bde823168998b8251118d6245d965b58d117fc309814378edeb` |
| `/usr/bin/sam-voice` | `6d589c0f557bffd386f1a3ae63f63a11a058a891fc7fbedffd7f92cb3324275f` |

The RPM is kept at:

```text
~/rpmbuild/RPMS/x86_64/sam-0.1.0-1.fc45.x86_64.rpm
```

## Voice

| Field | Value |
|---|---|
| Engine | Moonshine Small Streaming, local and offline |
| Model root | `~/.local/share/sam/models` |
| Selected model | `moonshine-streaming` |
| State | ready |
| Mode | eager |
| Auto-run | on |
| Threads | 4 |
| Max clip | 15 s |
| Silence cutoff | 900 ms |

Push-to-talk is hold `Tab` or the microphone. The tested voice command is
**"open [app]"**, which works and is snappy.

## Installed configuration

`~/.config/sam/config.toml`:

```toml
[projects]
roots = [
    "~/Documents/GitHub",
    "~/Documents/github",
    "~/Projects",
    "~/Desktop",
    "~/src",
    "~/code",
]
depth = 3

[voice]
enabled = true
mode = "eager"
auto_run = true
model = "moonshine-streaming"
threads = 4
max_seconds = 15
silence_ms = 900
```

## Observed baseline

| Metric | Value |
|---|---|
| `sam doctor` actions | 38 |
| Apps indexed | 91 entries |
| Project roots | 11 |
| File source | LocalSearch adapter |
| Service RSS, idle sample | ~29–46 MB without voice; ~298 MB with voice warm |
| Service peak RSS | ~477 MB |
| `sam voice status` | ready |

The memory rows come from different modes and builds; treat them as a range
and re-measure before publishing a release gate.

## What was tested

- `open [app]` by voice, including the panel appearing and the app launching.
- Timer and alarm actions in ordinary use.
- The panel and service start at login.

This is deliberately a short list. "Known good" means **do not regress these
paths**; it does not mean the whole registry has been exercised.

## What this build does not include

- No Needle sidecar, no model tier, no `Needle` confidence path.
- No `window.*` actions or GNOME Shell extension.
- No `probe.*` actions.
- No `search.content` action with bounded roots.
- No `clock.open` or `notes.open` actions. They are added after this baseline.

## Install and rollback

The package is already installed. To reinstall the frozen RPM:

```sh
sudo dnf install --allowerasing \
  ~/rpmbuild/RPMS/x86_64/sam-0.1.0-1.fc45.x86_64.rpm
sam setup
systemctl --user restart io.github.luishowin.Sam.service
```

To roll back after a later package:

```sh
sudo dnf downgrade \
  ~/rpmbuild/RPMS/x86_64/sam-0.1.0-1.fc45.x86_64.rpm
systemctl --user restart io.github.luishowin.Sam.service
```

To remove only the per-user setup (keybinding and service):

```sh
sam setup --undo
```

## Freeze rule

Until the next release passes its gates:

1. Do not overwrite `/usr/bin/sam` or `/usr/bin/sam-voice`.
2. Build candidates from a worktree and install them under a test prefix.
3. Keep this RPM and its checksum.
4. Compare the new build against the tested paths above before replacing it.
5. Record any regression here before changing the baseline.
