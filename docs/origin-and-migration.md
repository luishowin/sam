# Origin and migration

**Status:** Record · **Date:** 2026-11-03

Sam was established as an independent Essential OS component in November 2026. Before that,
some planning material treated operating-system search, command interpretation and personal
assistance as one assistant architecture. That is no longer the design.

This page records the separation briefly. The full engineering history lives in the Essential
OS project record.

---

## What changed

- **Sam** was defined as Essential OS's native, deterministic search and retrieval service,
  with its own registry, handlers, evaluation criteria and roadmap. It now also performs a fixed
  set of registered actions; see the 2026-11-06 update below.
- **Ida** remained the voice-first personal assistant, with its Tier 0 / Tier 1 / Tier 2
  architecture, policy-enforcing executor, privacy gate and Hermes for multi-step work.
- Ownership of local search, application discovery and system-information retrieval moved to
  Sam. Ida may call Sam through a typed interface.
- The two systems keep separate registries, interfaces and responsibilities, even where they
  share low-level OS data services.

## What did not change

- Ida's identity, personality, purpose and voice.
- Ida's trust rule: intelligence emits inert intents, policy approves, an executor acts.
- Ida's action model, security boundaries and offline behaviour.
- The requirement that Essential OS remain fully usable without either assistant.

## Status at separation

Sam had a specification and a boundary agreement, and no implementation. At separation no
source code was moved from Ida into Sam.

## Update, 2026-11-05

Sam now has a Fedora prototype. Its panel shell, Fedora adapters and setup logic began from
Ida's MIT-licensed code, were stripped of AI, network, voice, weather, shell and conversation
paths, and were extended with Sam's registry, live system providers, settings map, timers
and speech worker. On the development machine Ida has been removed and Sam installed in its
place; see [`migration-from-ida.md`](migration-from-ida.md). The Ida repository remains a
separate project.

## Update, 2026-11-06

Sam's architecture now allows small local model tiers. The product boundary is unchanged: Sam
remains a search execution engine, and Ida remains the assistant. The models are a router
(Needle) and an offline answer tier, both sidecars, both validated, both optional. They do not
plan, remember or execute, and Sam still has no network egress. The revision is recorded in
[`model-tiers.md`](model-tiers.md); the earlier "no model at runtime" wording is superseded by
the narrower invariant there.

## Related records

- Essential OS project record: <https://github.com/luishowin/essential-os>
- Ida, the personal assistant: <https://github.com/luishowin/ida>
