# Roadmap

**Status:** Fedora prototype built; Essential OS work remains · **Last updated:** 2026-11-05

Sam has a working Fedora prototype. This roadmap still describes the intended implementation
order toward Essential OS. Each phase ends at a measured gate, and a safe stop at any gate
should leave something usable. See [docs/fedora-prototype.md](docs/fedora-prototype.md) for
what is actually built.

---

## Guiding constraints

- No generative model in the runtime.
- No conversational memory or personal profiling.
- No network egress from the Sam process.
- Useful on a 4 GB system.
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

## S3 — Semantic retrieval

**Status:** Deferred. Lexical, synonym and fuzzy retrieval only.

**Work**

- Precomputed registry embeddings.
- Compact query encoder loaded on demand.
- Hybrid lexical and semantic ranking.
- Lexical-only fallback.

**Exit gate:** paraphrase recall improves enough to justify latency and RAM; lexical-only mode
remains usable.

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

**Status:** Partial. Fedora GTK4 panel, result rows, structured answers and web handoff are
implemented. Essential OS shell integration is not.

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

- Test on 4 GB systems, offline, under index load and with encoder cold start.
- Failure injection and long-running cache churn.
- Package and version the registry, handlers and index schema together.

**Exit gate:** memory, latency, accuracy, offline, privacy and no-memory acceptance gates pass.

## Open decisions

These must be settled before or during implementation:

1. ~~**Licence.**~~ **Settled 2026-11-03: MIT.** See [`LICENSE`](LICENSE).
2. **Local IPC mechanism and API versioning.**
3. **Catalogue ownership and invalidation events.**
4. **Which result actions Sam may offer directly.**
5. **Default-enabled domains and opt-in domains.**
6. **Whether a dedicated embedding encoder is worth its cost.**
7. **Minimum reference hardware and measured baselines.**
8. **Registry authoring tooling and review workflow.**
9. **Whether any domain needs full-content indexing.**

## Out of scope

- Becoming an agent framework.
- Replacing Ida's action policy or Hermes.
- General knowledge, web research or creative writing.
- Conversational memory or personalisation.
- A plugin marketplace.
- Multi-user or multi-device behaviour in v1.
