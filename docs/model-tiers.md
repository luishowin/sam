# Model tiers

**Status:** Design; no model tier ships in the Fedora prototype · **Scope:** Sam's routing, model boundaries and failure behaviour

Sam is a search execution engine. Most requests are answered by deterministic code. Two small
local models extend coverage: **Needle** routes phrasing the registry did not anticipate, and a
**compact 1–4B model** answers general questions when the machine is offline or the user
explicitly asks for a local answer. Neither model is an assistant. Neither plans, remembers or
executes.

This document is the canonical description of the tier model. Other documents summarise it and
link here.

---

## The invariant

> **Models propose; the runtime decides.** A model output can only ever become an inert
> `Intent`. It is schema-constrained, validated against the registry, checked by the policy
> table, and executed by a fixed handler. No model writes answer prose in the routing path,
> plans a sequence, retains memory, or reaches the network.

The earlier rule "no generative model in the runtime" was a proxy for this. It is too blunt:
Needle is a language model, but it is not a generative chat model. It reads a query and a small
tool list and emits a JSON call, or `[]` when no declared tool applies. The rule that matters is
the one above, not the model's family name.

What remains forbidden:

- free-text generation in the routing or execution path;
- a model choosing an open-ended sequence of actions (the bounded read-only probe loop in
  Tier 3 is the one exception, and it is capped at three probes);
- a model retaining state between requests;
- a model reaching the network;
- a model output that is executed without validation;
- a model-written shell command, ever.

## The tiers

| Tier | Name | Runs in | Input | Output | Latency | Memory | Network | Status |
|---|---|---|---|---|---|---|---|---|
| 0 | Fast path | `sam-core`, in-process | text | `Intent`s | 3–10 ms | 0 | none | Implemented |
| 1 | Needle router | sidecar | text + ≤5 tool schemas | JSON call / `[]` | 100–250 ms | ~30–120 MB | none | Design |
| 2 | Clarify / confirm / refuse | `sam-core`, in-process | partial intent or empty call | one question or decision | ~0 ms | 0 | none | Partial |
| 3 | Local answer / escalation | sidecar (1–4B) | text | short prose; optionally a call on the same schema | 0.3–1 s warm, 1–4 s cold TTFT | 1.1–2.6 GB | none | Design |
| 4 | Cloud | caller-owned | — | — | — | — | opt-in | Out of scope |

Latency and memory are provisional targets; see [`performance-targets.md`](performance-targets.md).

## Routing

```text
query
  │
  ├─ Tier 0  grammar · lexical · fuzzy · live providers
  │            confident? → validate → policy → execute → template
  │
  ├─ Tier 1  candidate retrieval → ≤5 schemas → Needle
  │            call at or above the confidence threshold? → validate → policy → execute
  │            call below the threshold? → Tier 2
  │            [] → Tier 2 or Tier 3
  │
  ├─ Tier 2  missing argument · ambiguous entity · destructive · no call
  │            one clarification, one confirmation, or an honest decline
  │
  └─ Tier 3  out of domain, or the user asked for a local answer
              browser handoff by default; local model when offline or requested
```

Rules:

1. Tier 0 always runs first. A model never replaces a deterministic match.
2. Tier 1 runs only on a miss or a low-confidence match.
3. Tier 2 is deterministic. It never calls a model.
4. Tier 3 is not a router for ordinary commands. It answers questions and, at most, proposes a
   call for the ambiguous tail through the same schema.
5. One pass. A follow-up buffer may carry the previous call and result for about 15 seconds;
   there is no loop.

## Tier 0 — fast path

The current path: `data/intents.toml`, the live providers, lexical and fuzzy ranking, confidence
and ambiguity rules. It must remain useful with every model removed.

- Handles common canonical and paraphrase phrasings.
- Returns typed `Intent`s and typed results.
- Runs in milliseconds, cold or warm.
- Is the fallback when a model is missing, slow or wrong.

The grammar is not a fallback for the model. The model is a fallback for the grammar.

## Tier 1 — Needle router

Needle is a small encoder-decoder "Simple Attention Network" from Cactus. The base artifact is
35.3 MB at 2 bits; a local 4-bit tune is 63.4 MB, and its 8-layer slice is 27.4 MB. Parameter
counts depend on the slice (29–121M). It is built for tool calling:

- text goes in, a JSON call comes out;
- a byte-level grammar compiled from the schemas constrains every token;
- `[]` is the refusal contract for a request no declared tool can serve;
- a calibrated confidence score accompanies the call on the base and platform-tuned models;
- **no tool-retrieval head**: Needle 3 reads the whole declared toolset, so Sam must pre-filter
  to 4–8 candidate schemas before the call;
- it runs on the Cactus engine as a single `.cact` artifact and can be fine-tuned locally.

Sam uses it as a **fallback router**, not a primary one:

- **Candidate retrieval.** `Registry::candidates_for` or the exported schema index selects 4–8
  capabilities. Needle never sees the whole catalogue; it has no retrieval head of its own.
- **Schema.** The call must match the exported JSON Schema. Unknown tools, extra arguments,
  missing required arguments and out-of-range values are rejected before they reach policy.
- **Confidence.** Act at or above the configured threshold. Below it, clarify or escalate; never
  execute. The failure mode must be escalation, not a wrong action. A locally tuned archive
  reports `confidence = None`; in that case calibrate a threshold on the held-out set or keep
  the base model for gating.
- **Refusal.** `[]` means Sam says "I can't do that here" or hands off. It is not an error.

The tier runs as a sidecar over a Unix socket. `sam-core` does not link the model, and the Sam
process still makes no network calls. The speech worker in
[`../crates/sam-voice/README.md`](../crates/sam-voice/README.md) is the existing pattern.

Fine-tuning is what makes this tier worth its footprint: the base model knows tool calling, and
the tuned artifact knows Fedora GNOME vocabulary, Sam's action names and the argument shapes.
A first local tune on 360 rows moved the frozen holdout from 23/32 to 26/32 tool names and from
20/32 to 24/32 raw arguments (26/32 after normalisation), with negatives improving from 7/8 to
8/8 and no false positives. The 8-layer slice was not viable. The tuned artifact is a candidate
pending the real-use and safety gates; it reports `confidence = None`.
See [`fine-tuning.md`](fine-tuning.md) and the command-level
[`fine-tuning-runbook.md`](fine-tuning-runbook.md).

## Tier 2 — clarification and confirmation

Deterministic. It handles:

- a missing required argument ("set a timer" → "for how long?");
- an ambiguous entity (two apps, two files, two devices → one short numbered list);
- a destructive or privileged action (confirm first, always);
- an empty call when a tool was clearly expected ("I didn't catch that");
- a recognised but disallowed request ("I can't delete files here").

The templates are fixed. Tier 2 never generates an apology or an explanation. It asks one
question and waits.

## Tier 3 — local answer and escalation

A compact 1–4B model, quantised to q4 or q5, running on CPU. Three jobs:

1. **Offline answer.** When the browser handoff is unavailable (no connectivity) or the user
   explicitly asks for a local answer, answer a general question in a few sentences. The answer
   is labelled **"local model, unverified"** and never claims a source.
2. **Ambiguous tail.** For a request that Tier 1 could not route but that clearly maps to a
   capability, propose a call through the same schema and validator. This is a second router
   only for the tail; it does not replace Needle for ordinary misses.
3. **Investigation.** Answer a question about the machine by selecting up to three fixed
   read-only probes from the registry, then summarise their output once. See
   [`investigation.md`](investigation.md).

It does not plan beyond selecting probes, retain context, or execute. It is loaded on demand and
unloaded after idle.

The tool set available to Tier 3 is a subset of the registry: the read-only probes, the live
information providers and the window actions described in [`window-control.md`](window-control.md).
Window actions are proposals like any other; `window.close` still confirms.

Hardware guidance, provisional:

| Machine | Tier 1 | Tier 3 |
|---|---|---|
| 4 GB | Yes | 1.7B q4 only, lazy; expect eviction pressure |
| 8 GB | Yes | 1.7B or 4B q4 |
| 16 GB+ | Yes | 4B q4/q5, optionally warm |

The 4B model does not fit comfortably on 4 GB alongside GNOME. The hardware tier is a
configuration decision, not a hidden fallback. See [`performance-targets.md`](performance-targets.md).

### The bounded probe loop

The investigation path is the one exception to "one call, no loop":

- at most **three** probes per question;
- every probe is a fixed registry entry, read-only, with a timeout and an output cap;
- the model cannot add a command, a flag or a path outside the probe's slots;
- no mutations, no network, no state between turns;
- every probe is logged with its argv and exit code;
- one summary, then stop.

If the probes do not answer the question, Sam says so and offers the browser handoff. It does
not keep trying.

The offline path is described in [`offline-and-web.md`](offline-and-web.md).

## Tier 4 — cloud

Not part of Sam. Sam has no network egress and the caller owns escalation. If a cloud tier is
ever added, it is opt-in, off by default, and requires an explicit amendment to this document
and to [`security-and-privacy.md`](security-and-privacy.md). The current handoff is the browser:
Sam opens a search URL and the browser performs the request.

## Tool schemas are the source of truth

Every model reads the same artifact the validator reads.

- `ActionSpec` in [`../crates/sam-core/src/action.rs`](../crates/sam-core/src/action.rs) is
  canonical.
- An exporter emits JSON Schema: `name`, `description`, typed parameters with bounds and enums,
  and required flags.
- The same export feeds Needle, the fine-tuning teacher and the golden-set generator.
- A fingerprint over the schemas and the model version is recorded with every run. A changed
  schema invalidates cached tool embeddings and the model artifact.
- Registry, handler bundle, schemas and model artifact are released together.

The description is part of the model's input. It must say what the capability does and what it
does not do, in words a reader with no world knowledge can use.

## Process model

- Tier 1 and Tier 3 are separate processes. They speak a small local protocol over a Unix
  socket, as `sam-voice` does.
- `sam-core` stays platform-free and dependency-light; `tests/purity.rs` continues to hold the
  allowlist.
- The Sam process makes no network calls. The browser handoff is a `url.open` action; the
  browser does the fetching.
- Model artifacts live under `~/.local/share/sam/models/`, are hash-checked at startup, and are
  never replaced silently.
- A missing or crashed sidecar is a normal failure: Sam falls back to Tier 0 and reports it.

## Lifecycle

- **Lazy.** Load on first use.
- **Warm.** Keep the model resident while the surface is in use.
- **Unload.** Release after an idle period; the exact period is a measured setting.
- **Pre-warm.** Optionally load on AC power. This trades memory for cold-start latency and is
  off by default.

Report cold and warm TTFT separately. A single "time to first word" number that hides a
three-second cold start is not honest.

## The fine-tuning loop

1. Log the request, the tier, the chosen call and the outcome (metadata by default).
2. Add corrected failures to the golden set in [`evaluation.md`](evaluation.md).
3. Fine-tune with the teacher's schema-validated data; see [`fine-tuning.md`](fine-tuning.md).
4. Re-run the full golden set and the safety suite.
5. Ship only on a measured win over the current tier.

A fine-tune that improves the training distribution but loses on the frozen set does not ship.

## Failure behaviour

| Condition | Behaviour |
|---|---|
| Tier 1 sidecar missing or crashed | Tier 0 only; log the failure; do not block typing |
| Tier 1 returns `[]` | Tier 2 decline or Tier 3 handoff |
| Tier 1 confidence below threshold | Clarify or escalate; never execute |
| Tier 1 call fails validation | Reject the call; fall back to Tier 2 |
| Schema fingerprint mismatch | Refuse to use the model; reload or fall back |
| Tier 3 sidecar missing | Browser handoff or a clean decline |
| Tier 3 asks for a mutation or a raw command | Refuse; only fixed read-only probes are callable |
| Probe times out or is truncated | Report the partial result and the limit |
| Probe needs root | Report "needs privilege"; do not escalate |
| Window extension missing or disabled | `source_unavailable`; suggest `sam setup` |
| Window target matches several windows | Clarify; never act on a guess |
| `window.close` without a yes | Pending confirmation; nothing happens |
| No connectivity | Offer the local answer or decline; never pretend a web answer exists |
| Model answer requested for a command | Route to Tier 0/1; Tier 3 does not execute |

## What this does not change

- Sam is not an agent. It does not delegate or run an open-ended loop. The three-probe read-only
  investigation is the only bounded exception.
- Sam has no personal memory and no conversation history.
- Sam has no network egress.
- Sam never builds a shell command from user text. It selects a fixed probe.
- Destructive and privileged actions always confirm; `window.close` is one of them.
- The registry, handler and policy path is unchanged; models only supply candidates.

## Open decisions

- Needle generation and slice size; q4 versus q5.
- 1.7B versus 4B as the default Tier 3, and the hardware tier that enables it.
- The confidence threshold for Tier 1 and the escalation rule.
- The idle unload period and the pre-warm policy.
- Whether Tier 3 may propose window actions directly, or only probes and answers.
- Transcript logging for real-use review (the default remains metadata-only).
