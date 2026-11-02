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
  with its own registry, handlers, evaluation criteria and roadmap.
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

Sam had a specification and a boundary agreement, and no implementation. This repository is
therefore documentation-led. No source code was moved from Ida into Sam, and no Ida
functionality was removed.

## Related records

- Essential OS project record: <https://github.com/luishowin/essential-os>
- Ida, the personal assistant: <https://github.com/luishowin/ida>
