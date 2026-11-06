# Architecture

**Status:** Fedora prototype · **Scope:** Sam runtime, process boundaries and failure behaviour

This document describes the intended architecture and the first implemented slice. The
Fedora prototype is real; the model tiers, Essential OS service and release gates are not. See
[`fedora-prototype.md`](fedora-prototype.md) for what runs today and
[`model-tiers.md`](model-tiers.md) for the model contracts.

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
│  normalise → classify → retrieve → rank → route → dispatch   │
│  registry · lexical index · optional embeddings · cache     │
│  registered handlers and fixed actions                      │
│  optional local model sidecars (no network)                  │
└───────────────┬─────────────────────────────────────────────┘
                │ read-only reads; fixed actions
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
| Router tiers | Run Tier 0; on a miss, ask a model sidecar for a candidate call | Local socket only |
| Tool-schema exporter | Emit JSON Schema from `ActionSpec` for the router and the teacher | None |
| Model client | Speak the small local protocol to the Needle and local-answer sidecars | Local socket only |
| Probe executor | Run one fixed read-only probe with a timeout, output cap and redaction | None |
| Window backend | Call the GNOME Shell extension over D-Bus; fake backend in tests | Session bus only |
| Dispatcher | Resolve slots, select one handler, enforce permissions | None |
| Handler layer | Read an approved local source and return typed data | None |
| Result formatter | Apply response templates and attach provenance/freshness | None |
| Cache | Bounded LRU/TTL store with dependency invalidation | None |
| Registry loader | Load and validate compiled registry artifacts | None |

No component performs free-text generation in the routing or execution path, plans an open-ended
sequence of actions, retries autonomously, or manages a conversation. The one bounded exception
is Tier 3's read-only probe loop: at most three fixed probes, then one summary. A model may
propose one inert call; the dispatcher, policy table and a fixed handler decide what happens.
The tier contracts are in [`model-tiers.md`](model-tiers.md).

## Request lifecycle

1. **Receive.** Validate caller identity, capabilities and requested scope.
2. **Normalise.** Trim, tokenise, expand common contractions, normalise units and known OS
   vocabulary. Preserve the original text only for display and diagnostics where policy
   allows.
3. **Classify.** Determine the likely domain from lexical and semantic signals.
4. **Retrieve.** Shortlist candidate registry entries within the relevant domain partitions.
5. **Rank.** Combine lexical score, semantic similarity, domain prior, slot compatibility,
   freshness and source availability.
6. **Route.** If Tier 0 clears the confidence and ambiguity rules, dispatch. Otherwise ask Tier
   1 for a candidate call from at most five schemas. If that also fails or is below the
   confidence threshold, ask one clarification or return a typed handoff (Tier 2).
7. **Decide.** Dispatch one handler, ask one clarification, or return a typed handoff.
8. **Execute.** Validate parameters and permissions, then run a predefined handler.
9. **Format.** Return structured results with source, freshness and allowed actions.
10. **Cache.** Store an eligible result or plan with TTL and dependency tags.

A general question does not enter this path. It becomes a browser handoff, or a labelled local
answer when the machine is offline; see [`offline-and-web.md`](offline-and-web.md). Window
actions and read-only probes are ordinary registry capabilities; the probe executor and the
window backend are described in [`investigation.md`](investigation.md) and
[`window-control.md`](window-control.md).

## Process and capability boundaries

- Sam runs as a service with **no network egress**. This is a design invariant, not a
  configuration default. A model sidecar is a separate process on a local socket; it does not
  open the door.
- It has **read-only** access to approved local sources. It is not a privileged executor.
- It exposes a typed local API over an appropriate local IPC mechanism. The contract never
  accepts arbitrary executable instructions or unrestricted shell text.
- The Sam engine returns constrained action proposals. In the Fedora prototype the panel is
  also the caller: it revalidates an intent through the registry and policy table, then runs
  one fixed handler. It never builds a shell command from user text. On Essential OS the same
  proposals may be performed by the OS capability layer or Ida's executor.
- Model tiers run out of process. Needle and the local answer model are sidecars with a small
  local protocol, like `sam-voice`. `sam-core` does not link them and keeps its dependency
  allowlist.
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
| Tier 1 sidecar missing or crashed | Tier 0 only; log the failure; do not block typing |
| Tier 1 returns `[]` or a low-confidence call | Clarify or hand off; never execute |
| Tier 1 call fails validation | Reject the call; fall back to Tier 2 |
| Schema fingerprint mismatch | Refuse the model; reload or fall back |
| Tier 3 sidecar missing | Browser handoff or a clean decline |
| Probe timeout or output cap reached | Report the partial result and the limit |
| Probe requests a mutation, network or privilege | Refuse; only read-only fixed probes run |
| Window extension missing or disabled | `source_unavailable`; suggest `sam setup` |
| Window target matches several windows | Clarify; never act on a guess |
| Out-of-domain query | Browser handoff or labelled local answer; never a fabricated source |
| Malformed or adversarial input | Reject, clamp or hand off; never execute arbitrary text |

## Scaling stance

The first implementation is a single local process with a flat registry and no vector index.
Separate index workers, model sidecars, disk-backed indexes or partitioned services are
justified only when measurement shows the simple design cannot meet the targets. The model tiers
are already separate processes for isolation and unload, not because of scale.

## Explicit non-goals

- No free-text generation in the routing or execution path.
- No open-ended planning or multi-step tool use. Tier 3 may run at most three fixed read-only
  probes in one turn, then summarise once.
- No model-written shell command, ever.
- No screenshots, input injection or arbitrary window control; only the registered methods.
- No conversational memory, user profile or personalisation.
- No autonomous background actions or retries.
- No web browsing or external API calls from the Sam process; the browser handoff is the
  user's action.
- No cloud tier inside Sam in v1; escalation belongs to the caller.
- No plugin marketplace or arbitrary user-supplied handlers in v1.
- No replacement for Ida's action policy or Hermes' multi-step loop.

## Open questions

- Which local IPC mechanism and versioned API should Sam expose?
- Which source owns each shared catalogue, and what event or version invalidates results?
- Which result actions may Sam offer directly, and which must always route through Ida or a
  separate OS permission service?
- What are the exact minimum reference-machine specifications, and which hardware tier enables
  the local answer model?
- Which local domains are enabled by default, and which require explicit opt-in?
- Which model tier owns the ambiguous tail: Needle, the local answer model, or a clarification?
- What is the confidence threshold for Tier 1, and what evidence changes it?
