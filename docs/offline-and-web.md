# Offline answers and the web handoff

**Status:** Design · **Scope:** What Sam does when a request is outside its local domain

Sam's domain is the computer it runs on. A general question — "what's the capital of France" —
is outside that domain. Sam must decline it honestly, but declining is not the same as leaving
the user with nothing. The default is a handoff to the browser; when there is no connectivity,
or the user asks for it, a compact local model may answer, clearly labelled.

---

## The default: open in the browser

A general question produces one `url.open` row:

```text
Search the web for "what's the capital of France"
```

Selecting it opens the default browser at a Google search URL. Sam never fetches the page, never
parses the result and never stores the query. The browser performs the request under its own
policy and the user's existing session.

This is a handoff, not an answer. It is honest about where the information comes from, it keeps
Sam's no-egress invariant intact, and it costs nothing until the user picks it.

The existing `data/sites.toml` table already provides `url.open` rows. A search template is a
small addition: a `url.open` row whose URL is `https://www.google.com/search?q=…` with the
query percent-encoded and validated as an absolute `https` URI.

## Why a handoff rather than an answer

- Sam is a search execution engine, not a general knowledge base.
- A browser result has a source the user can inspect.
- A small local model's answer does not, and at 1–4B it will sometimes be confidently wrong.
- A silent switch between a sourced answer and an unsourced guess is the worst outcome.

The handoff row is the default for exactly this reason. The local model is an option, not a
replacement.

## Connectivity

The decision to offer a local answer depends on connectivity, not on a timeout. Ask
NetworkManager for its connectivity state; do not probe an arbitrary host.

| State | Rows offered |
|---|---|
| Full connectivity | "Search the web for …" |
| Limited or none | "Search the web for …" and "Answer locally (unverified)" |
| No browser available | "Answer locally (unverified)" or a clean decline |

Both rows are always visible when the state is degraded. Sam does not silently choose for the
user, because the two answers differ in trust and privacy.

## The local answer

Tier 3 in [`model-tiers.md`](model-tiers.md): a 1–4B model, q4 or q5, running on CPU in a
sidecar process. It answers in a few sentences.

Rules:

- The row and the answer carry a **"local model, unverified"** label.
- The answer never claims a source it does not have.
- The answer never contains a command the user is expected to run.
- If the model is unsure, it says so or declines. A short honest refusal is better than a
  confident wrong fact.
- The model is loaded on demand and unloaded after idle; a cold start may take several seconds.
  The UI says "loading local model" rather than appearing to hang.
- The query and the answer stay on the machine.

The local answer is for offline use, privacy-sensitive questions and explicit requests. It is
not the default path for a general question while the network is available.

## Trust labels

Every result says where it came from.

| Result | Label | Leaves the machine |
|---|---|---|
| Registry answer | Source and freshness | No |
| Browser handoff | "Search the web for …" | Yes, when picked |
| Local model answer | "Local model, unverified" | No |

The labels are not decoration. They are the contract that lets a user decide whether to trust
the result.

## Failure behaviour

| Condition | Behaviour |
|---|---|
| Browser missing | Offer the local answer or decline |
| Local model missing | Offer the browser or decline |
| Both missing | One clean decline; no invented answer |
| Model crashes mid-answer | Withdraw the partial answer; offer the browser row |
| Connectivity check fails | Treat as unknown, not offline; show both rows |

## Configuration

- Browser handoff: on by default.
- Local answer: on when a model is installed; off when it is not.
- Offline behaviour: `ask` (show both rows) by default; `browser` and `local` are explicit
  alternatives.
- Model tier and hardware policy: see [`performance-targets.md`](performance-targets.md).

## Status

Not implemented. The current prototype offers a web handoff row and has no local answer tier.
The search template, connectivity check and trust labels are future work.
