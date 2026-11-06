# Investigation

**Status:** Design · **Scope:** Read-only probes, content search and bounded Tier 3 investigation

Sam can already answer from structured live state. Investigation extends that to questions whose
answer lives in command output: "how long has my laptop been on", "when did it last boot", "what
failed at startup", "find documents that mention essential os".

The model never writes a shell command. It selects a **probe** from a fixed registry; the
registry owns the command. This is the same rule as every other action: models propose, code
resolves.

---

## Why not free-form shell

"Read-only" is not a property a model can be trusted to preserve:

- `cat ~/.ssh/id_rsa` is read-only;
- `git status` can write index locks;
- `find /` can run for minutes;
- `less` opens a pager and hangs;
- `curl` reaches the network;
- a flag typo can turn a read into a write.

A sandbox would reduce the risk but not remove it, and it would still allow secret reads inside
the allowlist. A typed probe registry is testable, auditable and smaller.

## The probe

A probe is a registry entry:

| Field | Meaning |
|---|---|
| `id` | `probe.uptime` |
| `description` | What it answers, in plain words |
| `program` | Absolute executable path from the allowlist |
| `args` | Fixed argument vector; slots only where declared |
| `slots` | Typed parameters, with bounds and enums |
| `read_only` | Must be true; the registry refuses otherwise |
| `timeout_ms` | Hard cap, default 3000 |
| `max_output` | Hard cap, default 64 KiB |
| `roots` | Optional path allowlist for file-reading probes |
| `template` | Deterministic answer template where possible |

The executor:

- runs `Command::new(program).args(argv)`, never `sh -c`;
- sets a timeout and kills the process group on expiry;
- caps output and marks truncation;
- runs on a worker thread, `nice`-d, so the UI never blocks;
- redacts before logging; logs argv, exit code, duration and output hash, not the output;
- rejects any probe whose `read_only` is false.

## Probe list

| Probe | Command | Answer |
|---|---|---|
| `probe.uptime` | `uptime -p`, `who -b`, `/proc/uptime` | "Up 3 h 24 min; booted at 09:12" |
| `probe.boot_history` | `journalctl --list-boots --no-pager`, `last -x reboot -n 5` | Recent boots and shutdowns |
| `probe.battery` | `upower -d` | Charge, capacity, time to empty |
| `probe.failed_services` | `systemctl --user --failed --no-pager` | Failed user units |
| `probe.disk` | native `statvfs` | Free and total per mount |
| `probe.network` | `nmcli` or native | Connection, IP, radios |
| `probe.temperature` | `/sys/class/thermal` | Sensors and top consumers |

Anything requiring root is out of scope or reports "needs privilege". No `git`, editors, pagers,
package managers or network clients.

## The honest answer

The output is the source. The summary may not add a cause the output does not contain.

`uptime` can say "up 3 h 24 min, booted at 09:12". It cannot say the previous shutdown was a dead
battery. `journalctl` may show a shutdown time, and `upower` may show a battery state, but
attributing the cause is inference. The rule:

- state what the probe returned;
- if the cause is not in the output, say "I can't tell why it shut down";
- never invent a machine name, a reason or a timeline.

This is the main guardrail to fine-tune for.

## Templates first, model second

Common probes render a deterministic template. The local model is used only for open-ended
synthesis across several probes ("why is it hot?"). A 1–4B model summarising `journalctl` from
scratch will confidently invent things.

## Bounded Tier 3 investigation

Tier 3 may run at most **three** fixed read-only probes in one turn, then summarise once.

- The probes are chosen from the registry; the model cannot add a command.
- A hard cap of three, a per-probe timeout and an output cap.
- No mutations, no network, no state between turns.
- Every probe is logged.
- If three probes do not answer the question, Sam says so and offers a browser handoff.

This is the one bounded exception to "one call, no loop". It is not an agent: it cannot plan
beyond selecting probes, cannot write, and cannot act on the result.

## Content search

`search.content { query, roots? }` is a first-class tool, not a shell probe:

1. ask LocalSearch/TinySPARQL for indexed content matches;
2. if the index is unavailable or empty, run a bounded `rg` (or `grep -rIl`) scan over
   allowlisted roots;
3. apply the timeout and result cap;
4. return a list of paths with a snippet;
5. open or reveal through the existing `file.open` and `file.reveal` actions.

Default roots: Documents, Downloads, Desktop and the configured project roots. Never the whole
home directory by default. Deny secrets and browser profiles.

## Copy answer

Answer cards get a **Copy** button that uses the existing `info.copy` action. No model
involvement.

## Evaluation

Add categories to the golden set:

- investigation: uptime, boot history, battery, failed services;
- content search: a known document, no match, a large tree;
- adversarial probes: "cat ~/.ssh/id_rsa", "delete everything", "curl …", "chmod …";
- window control: minimise, maximise, move, centre, multiple matches, close confirmation;
- model: correct probe selection, correct slots, refusal when no probe applies.

## Status

Design. No probe registry, executor or window backend exists in the current build.
