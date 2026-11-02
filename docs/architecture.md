# Architecture

**Status:** Design · **Scope:** Sam runtime, process boundaries and failure behaviour

This document describes the intended architecture. Sam is not implemented yet; nothing here
is a claim about existing code.

---

## Position in Essential OS

```text
┌─────────────────────────────────────────────────────────────┐
│  Clients                                                    │
│  Essential OS global search          Ida (personal assistant)│
└───────────────┬──────────────────────────────┬──────────────┘
                │ typed local API              │ typed local API
                ▼                              ▼
┌─────────────────────────────────────────────────────────────┐
│  Sam                                                        │
│  normalise → classify → retrieve → rank → dispatch → format │
│  registry · lexical index · optional embeddings · cache     │
│  read-only handlers over approved local sources             │
└───────────────┬─────────────────────────────────────────────┘
                │ read-only
                ▼
┌─────────────────────────────────────────────────────────────┐
│  Shared data services                                       │
│  file catalogue · desktop entries · settings metadata ·     │
│  device inventory · documentation index · system adapters   │
└─────────────────────────────────────────────────────────────┘
```

Sam is a peer OS service. It is not a tier inside Ida, an Ida backend, and not an agent
framework. It can be used by the global search UI and by Ida, and it works without either.

## Components

| Component | Responsibility | Network |
|---|---|---|
| Query front end | Accept a typed request, validate caller and scope, normalise text | None |
| Classifier | Assign a domain and shortlist candidate intents | None |
| Retriever | Lexical, synonym, fuzzy and optional embedding retrieval | None |
| Ranker | Combine signals, apply confidence and ambiguity rules | None |
| Dispatcher | Resolve slots, select one handler, enforce permissions | None |
| Handler layer | Read an approved local source and return typed data | None |
| Result formatter | Apply response templates and attach provenance/freshness | None |
| Cache | Bounded LRU/TTL store with dependency invalidation | None |
| Registry loader | Load and validate compiled registry artifacts | None |

No component performs generation, planning, autonomous retries or conversation management.

## Request lifecycle

1. **Receive.** Validate caller identity, capabilities and requested scope.
2. **Normalise.** Trim, tokenise, expand common contractions, normalise units and known OS
   vocabulary. Preserve the original text only for display and diagnostics where policy
   allows.
3. **Classify.** Determine the likely domain from lexical and semantic signals.
4. **Retrieve.** Shortlist candidate registry entries within the relevant domain partitions.
5. **Rank.** Combine lexical score, semantic similarity, domain prior, slot compatibility,
   freshness and source availability.
6. **Decide.** Dispatch one handler, ask one clarification, or return a typed handoff.
7. **Execute.** Validate parameters and permissions, then run a predefined read-only handler.
8. **Format.** Return structured results with source, freshness and allowed actions.
9. **Cache.** Store an eligible result or plan with TTL and dependency tags.

## Process and capability boundaries

- Sam runs as a service with **no network egress**. This is a design invariant, not a
  configuration default.
- It has **read-only** access to approved local sources. It is not a privileged executor.
- It exposes a typed local API over an appropriate local IPC mechanism. The contract never
  accepts arbitrary executable instructions or unrestricted shell text.
- State-changing operations are outside Sam. A result action is a constrained proposal that
  the caller must revalidate and perform through the OS capability layer or Ida's executor.
- Sam never receives secrets, credentials or unrestricted filesystem access.

## Failure behaviour

| Condition | Behaviour |
|---|---|
| Low confidence | `no_confident_match`; optionally suggest a supported domain |
| Ambiguous entity | One short clarification, with a small numbered list where useful |
| No source data | Report the unavailable source; never substitute a guessed value |
| Stale index | Disclose staleness and refresh within the resource budget |
| Handler timeout or adapter failure | Stop, return a structured error, avoid partial claims |
| Embedding encoder unavailable | Fall back to the lexical path; do not block the UI |
| Out-of-domain query | Typed handoff with no attempt to answer from unrelated material |
| Malformed or adversarial input | Reject, clamp or hand off; never execute arbitrary text |

## Scaling stance

The first implementation should be a single local process with a flat registry and flat
vector index. Separate indexing workers, disk-backed indexes or partitioned services are
justified only when measurement shows the simple design cannot meet the targets.

## Explicit non-goals

- No large language model at runtime.
- No generation of prose, code or explanations.
- No chain-of-thought, planning or tool-selection reasoning.
- No conversational memory, user profile or personalisation.
- No autonomous background actions.
- No web browsing or external API calls.
- No plugin marketplace or arbitrary user-supplied handlers in v1.
- No replacement for Ida's action policy or Hermes' multi-step loop.

## Open questions

- Which local IPC mechanism and versioned API should Sam expose?
- Which source owns each shared catalogue, and what event or version invalidates results?
- Which result actions may Sam offer directly, and which must always route through Ida or a
  separate OS permission service?
- What are the exact minimum reference-machine specifications?
- Which local domains are enabled by default, and which require explicit opt-in?
