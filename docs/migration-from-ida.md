# Migration from Ida

**Status:** Applied on the development machine · **Date:** 2026-11-05

This machine now runs Sam instead of Ida. The Ida repository is untouched; this
page records what changed here and how to reverse it.

---

## Why

Ida was the first keyboard-first surface for this machine. The Sam
specification later separated OS-wide search from personal assistance: Sam owns
the deterministic search registry, local sources and results; Ida owns
assistant reasoning, conversation and state-changing policy.

The decision here is narrower than the repository history: **this machine's
installed panel is replaced by Sam**, and Ida's assistant features are removed
from it. The Sam and Ida projects remain separate.

## What was removed

- the `ida` RPM and its `/usr/bin/ida`, `/usr/bin/ida-voice` binaries
- `io.github.luishowin.Ida.service` and its user-unit enable link
- Ida's D-Bus service and desktop entry
- `/usr/share/ida`, `/usr/share/doc/ida`, `/usr/share/licenses/ida`
- Ida's icon
- `~/.config/ida` (only `setup-state.toml` on this machine)
- `~/.cache/ida`, `~/.local/share/ida`, `~/.local/state/ida` if present
- Ida's GNOME custom keybinding at
  `/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/ida/`
- any Ida keyring entry, Ollama action model or whisper artefact found

The `Super+Space` key was restored first with `ida setup --undo`, so Ida's
custom shortcut and unit were removed in the order Ida itself expects.

## What was installed

- the `sam` RPM
- `/usr/bin/sam`
- `/usr/bin/sam-voice`
- `io.github.luishowin.Sam.service` and its D-Bus service
- `io.github.luishowin.Sam.desktop`
- `io.github.luishowin.Sam` icon (PNG and SVG)
- `~/.config/sam/config.toml` only if the user creates one
- `~/.local/state/sam/timers.json` only when a timer or alarm is set
- `~/.local/share/sam/models/` only when speech models are installed

`sam setup` then took `Super+Space`, recorded the original binding in
`~/.config/sam/setup-state.toml`, and enabled the service.

## Verification

```sh
rpm -q sam
rpm -q ida                 # must say "not installed"
systemctl --user status io.github.luishowin.Sam.service
gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings
sam doctor
sam query "what is using my RAM"
sam query "chrome"
```

Expected:

- `sam` is installed and `ida` is not.
- the user service is active.
- the custom-keybinding list contains `…/custom-keybindings/sam/` and not
  `…/custom-keybindings/ida/`.
- the binding name is Sam and its command is
  `gapplication action io.github.luishowin.Sam toggle`.
- `sam doctor` reports the app ID and no network egress.

## Reversal

The migration is not automatically reversible after Ida is removed. To restore
Ida, reinstall its RPM and run `ida setup`; its old configuration is gone by
design (the user chose deletion over an archive).

To remove Sam without reinstalling Ida:

```sh
sam setup --undo
sudo dnf remove sam
rm -rf ~/.config/sam ~/.local/state/sam ~/.local/share/sam
```

## Provenance

Sam's panel shell, Fedora adapters and setup logic began from Ida's MIT-licensed
code. The copied files were renamed, stripped of AI, network, voice, weather,
shell and conversation paths, and then extended with Sam's registry, live
system providers, the settings map, timers and speech worker. Ida's
licence and copyright remain in `LICENSE` and this record.

No Ida source was moved into the Sam repository as an active assistant. The
Ida repository remains its own project.
