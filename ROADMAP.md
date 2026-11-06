# Roadmap

**Status:** Fedora prototype built; model tiers designed; Essential OS work remains · **Last updated:** 2026-11-06

Sam has a working Fedora prototype. This roadmap still describes the intended implementation
order toward Essential OS. Each phase ends at a measured gate, and a safe stop at any gate
should leave something usable. See [docs/fedora-prototype.md](docs/fedora-prototype.md) for
what is actually built and [docs/model-tiers.md](docs/model-tiers.md) for the model track.

---

## Guiding constraints

- Models propose; the runtime decides. A local model may supply one inert call; the registry
  validates it, policy approves it and a fixed handler runs it.
- No free-text generation in the routing or execution path.
- No conversational memory or personal profiling.
- No network egress from the Sam process. Local model sidecars use a Unix socket.
- Useful on a 4 GB system with Tier 0 and Tier 1; the local answer tier needs 8 GB or more.
- A new capability is a registry entry, a handler and tests.
- Claims are measured before they are published.

## S0 — Scope and measure

**Status:** Partial. Fedora 45 sources inventoried and measured; the 4 GB reference machine
is still pending.

**Work**

- Inventory local domains, data sources and OS interfaces.
- Map each domain to a read-only source and an owner.
- Benchmark process overhead, indexing and cold/warm query latency.
- Establish the minimum reference machine and the 4 GB environment.
- Decide whether a semantic encoder earns its latency and memory cost.

**Exit gate:** documented resource baseline; domain source map; no critical dependency on a
large model or the network.

## S1 — Deterministic core

**Status:** Implemented for the Fedora prototype.

**Work**

- Query normalisation and tokenisation.
- Domain registry and compiled registry format.
- Lexical matching, confidence thresholds and ambiguity rules.
- Typed result schema, provenance and freshness.
- Typed handoff outcomes.

**Exit gate:** unit tests pass; unknown and ambiguous queries fail safely; clean-query
precision meets the initial target.

## S2 — Registry authoring

**Status:** Not started. The current registry is hand-authored.

**Work**

- Build-time model drafts domain taxonomies, intents, paraphrases and evaluation cases.
- Human review, deduplication and nearest-neighbour confusion checks.
- Handler validation against real local sources.
- Frozen evaluation sets and a reproducible build.

**Exit gate:** registry is versioned, reviewable and reproducible; untested entries cannot
ship.

## S3 — Semantic and model retrieval

**Status:** Deferred. Lexical, synonym and fuzzy retrieval only; the model tiers are design.

**Work**

- Precomputed registry embeddings.
- Compact query encoder loaded on demand.
- Hybrid lexical and semantic ranking.
- Lexical-only fallback.
- A small Needle router for the tail, fed by candidate retrieval and the exported schemas.

**Exit gate:** paraphrase recall improves enough to justify latency and RAM; the router beats
the grammar alone on the frozen set without regressing negatives; lexical-only mode remains
usable.

## M0–M4 — Model track

The model work is tracked separately because it is optional and gated on measurement. The full
contracts are in [`docs/model-tiers.md`](docs/model-tiers.md); the harness is in
[`docs/evaluation.md`](docs/evaluation.md).

| Phase | Work | Exit gate |
|---|---|---|
| **M0 — Harness** | Golden set (~220 cases), `sam eval`, stage timings, tool-schema export. | Per-category accuracy and latency on a frozen set; a regression exits non-zero |
| **M1 — Needle router** | Sidecar, candidate retrieval, confidence threshold, `[]` contract. | Beats Tier 0 on paraphrase and STT categories without regressing negatives |
| **M2 — Fine-tune** | Schema-validated teacher data, `cactus finetune`/`cactus build`, local LoRA, artifact versioning. | Beats the base router on the frozen and real-use sets; safety unchanged; `confidence` handled |
| **M3 — Local answer** | 1–4B sidecar, offline path, trust labels, hardware tier. | Warm TTFT under 1 s, honest refusal, no unvalidated call |
| **M4 — Escalation** | Ambiguous-tail routing and the tested recipe corpus. | Recovers most router failures; free-form shell stays display-only |

M2 is partially met: the 20-layer tune beat the base on the frozen set (26/32 names, 24/32 raw
arguments, negatives 8/8); the real-use and safety gates, and a sidecar p95 measurement, are still
open. See [`docs/fine-tuning-runbook.md`](docs/fine-tuning-runbook.md#first-run-record-2026-11-07).

A phase that fails its gate is dropped, not carried forward.

## W0–W1 — Window control

| Phase | Work | Exit gate |
|---|---|---|
| **W0 — Spike** | Minimal GNOME Shell extension, one D-Bus method, call it from Rust. | Works on the development machine; version and fragility recorded |
| **W1 — Actions** | Registry entries, resolver, geometry, fake backend, `sam setup` install/undo. | Minimise, maximise, move, centre and close-with-confirmation pass the live check |

Window control is deterministic and does not wait for a model.

## I0–I3 — Investigation

| Phase | Work | Exit gate |
|---|---|---|
| **I0 — Probe registry** | Fixed argv, read-only validation, executable and path allowlists, timeout, output cap, redaction. | Adversarial probes are refused; no mutation is reachable |
| **I1 — Templates** | Deterministic answers for uptime, boot history, battery and failed services; copy button. | Answers are grounded in output; no invented causes |
| **I2 — Content search** | LocalSearch first, bounded `rg`/`grep` fallback, ranked paths, open/reveal. | Known document found; denied roots untouched |
| **I3 — Tier 3 investigation** | Bounded three-probe loop, summary, trust label, fine-tune data. | Correct probe selection; no raw command; safety unchanged |

I3 depends on M0. I0–I2 do not.

## S4 — Sources and handlers

**Status:** Partial. Fedora apps, files, settings, live system, hardware, networking and
name-level documentation are implemented. Full-content indexing is not.

**Work**

- Files, applications, settings, hardware, system, media, networking and local documentation.
- Bounded incremental indexing.
- Timeouts, source freshness and explicit error reporting.
- Typed result actions.

**Exit gate:** access control, freshness, timeouts and errors pass integration tests.

## S5 — Search UI

**Status:** Partial. Fedora GTK4 panel, result rows, structured answers, live timer card,
terminal configuration menu and web handoff are implemented. Essential OS shell integration
is not.

**Work**

- Essential OS global search integration.
- Result templates, lists, comparisons and provenance.
- Disambiguation and handoff UX.

**Exit gate:** common discovery tasks succeed without memorising phrases; search feels
immediate in task-based testing.

## S6 — Ida integration

**Work**

- Typed local API.
- Route Ida's local-information lookups to Sam.
- Preserve Ida's executor and privacy policy.

**Exit gate:** no duplicate indexes; Ida uses Sam without granting it action authority or
model-routing responsibility.

## S7 — Low-resource validation and release

**Status:** Partial. Development-laptop timings are recorded; 4 GB validation and a release
gate are pending.

**Work**

- Test on 4 GB systems with Tier 0 and Tier 1, offline, under index load and with encoder cold
  start.
- Test Tier 3 on an 8 GB or larger machine, including cold and warm TTFT and idle unload.
- Failure injection and long-running cache churn.
- Package and version the registry, handlers, tool schemas, model artifacts and index schema
  together.

**Exit gate:** memory, latency, accuracy, offline, privacy and no-memory acceptance gates pass
on the hardware tier each tier claims to support.

## Open decisions

These must be settled before or during implementation:

1. ~~**Licence.**~~ **Settled 2026-11-03: MIT.** See [`LICENSE`](LICENSE).
2. **Local IPC mechanism and API versioning.**
3. **Catalogue ownership and invalidation events.**
4. **Which result actions Sam may offer directly.**
5. **Default-enabled domains and opt-in domains.**
6. **Whether a dedicated embedding encoder is worth its cost.**
7. **Minimum reference hardware and measured baselines, including the Tier 3 hardware tier.**
8. **Registry authoring tooling and review workflow.**
9. **Whether any domain needs full-content indexing.**
10. **Which Needle generation and slice size; q4 versus q5.**
11. **The Tier 1 confidence threshold and the escalation rule.**
12. **Whether Tier 3 may propose calls or only answer questions.**
13. **Transcript logging for real-use review (default remains metadata-only).**
14. **Window extension packaging and the supported GNOME version range.**
15. **The probe allowlist and the default content-search roots.**
16. **Whether `window.close` needs a typed confirmation or a single yes.**

## Out of scope

- Becoming an agent framework.
- Replacing Ida's action policy or Hermes.
- General knowledge, web research or creative writing inside Sam.
- Conversational memory or personalisation.
- A plugin marketplace.
- Multi-user or multi-device behaviour in v1.
- A cloud tier inside Sam; escalation belongs to the caller.
- Model-written shell commands, including in the recipe tier.
- Screenshots, input injection or arbitrary window control.
- Free-form shell investigation; only fixed read-only probes.
