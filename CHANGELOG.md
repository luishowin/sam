# Changelog

## 0.2.0 — 2026-11-07 (candidate)

Local candidate. Not the declared LTS: the real-use set, per-risk safety suite, sidecar p95 and
canonical retrain are still open.

- **Needle Tier 1**: optional sidecar on a Unix socket, candidate retrieval, `[]`/clarify
  contract, model-to-runtime adapter and a reviewed low-risk automatic allowlist.
- **Schema exporter**: `sam schema` and `sam schema --fingerprint` generate the canonical
  catalogue from the registry.
- **Window control**: GNOME Shell 51 extension, D-Bus client, target resolver, geometry,
  fake backend and `sam setup` install/undo. The extension loads at the next login.
- **Probes**: fixed read-only uptime, boot history, battery, failed services, disk, network and
  temperature probes with timeouts and output caps.
- **Everyday actions**: `clock.open` and `notes.open`.
- **Packaging and install**: `scripts/install-needle-dev.sh`, a pinned 68 MB Cactus runtime,
  model and user service; the frozen 0.1.0 RPM remains the rollback.
- **Policy**: `Origin::Model` is distinct from `UserSelected`; `window.close`, lock, session
  changes, destructive and privileged actions confirm.
- **Docs**: `docs/stable-baseline.md`, `docs/needle-lts.md`, updated window, investigation,
  model-tier, evaluation, security and performance notes.

## 0.1.0 — 2026-11-05 (frozen baseline)

The installed Fedora 45 RPM that is known good on this machine.

- Deterministic registry, grammar, ranking, policy and typed results.
- Fedora/GNOME apps, files, folders, settings, live state, hardware, networking and display.
- Local push-to-talk speech, timers and alarms, terminal configuration and a Unix-socket API.
- No model tier, no window extension, no probes.

Checksums and rollback are in [`docs/stable-baseline.md`](docs/stable-baseline.md).
