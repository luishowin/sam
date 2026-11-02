# Retrieval design

**Status:** Design · **Scope:** How Sam turns a query into one confident dispatch

Sam's retrieval layer is deterministic and hybrid. It combines cheap exact and lexical
signals with optional semantic similarity, then applies explicit confidence rules. No part of
this layer generates an answer or reasons about the user's intent beyond matching it to a
registered capability.

---

## Stage 1 — Normalisation

- Trim, collapse whitespace and normalise punctuation.
- Expand common contractions and abbreviations.
- Normalise units, dates and plural forms.
- Map known OS vocabulary onto canonical terms (for example, "task manager" → process
  viewer, "night mode" → night light).
- Preserve the original query only for display and diagnostics where policy permits.
- Never translate user text into a shell command.

## Stage 2 — Domain classification

Assign one or more candidate domains using domain-specific keywords and a lightweight
classifier. Domain partitions keep the candidate set small and reduce false matches between
unrelated areas.

If no domain crosses its threshold, stop early with `no_confident_match` rather than searching
everything vaguely.

## Stage 3 — Candidate retrieval

Run in order, stopping when confidence is sufficient:

1. **Exact phrase** match against canonical phrases and aliases.
2. **Token / keyword** match against entry keywords, entity names and metadata.
3. **Synonym and paraphrase** expansion from the registry.
4. **Fuzzy** match for typos, truncation and partial names.
5. **Semantic** retrieval using a compact embedding encoder and precomputed registry vectors.

The lexical path must resolve common queries without loading an embedding model.

## Stage 4 — Ranking

Combine signals with fixed, versioned weights:

| Signal | Role |
|---|---|
| Lexical score | Exact and token overlap |
| Semantic similarity | Paraphrase and synonym recall |
| Domain prior | Likelihood of the intent within its domain |
| Slot compatibility | Whether required parameters can be resolved |
| Source availability | Whether the handler's source is readable now |
| Freshness | Whether indexed data is current enough for the intent |
| Ambiguity margin | Gap between the top two candidates |

Ranking is deterministic for a given registry version, query, scope and source state.

## Stage 5 — Confidence and ambiguity

Two thresholds matter:

- **Dispatch floor** — the minimum score for running a handler at all.
- **Ambiguity margin** — the minimum lead over the runner-up.

If the top candidate clears both, Sam dispatches. If it clears the floor but not the margin,
Sam asks one short clarification, optionally with a small numbered list. If nothing clears the
floor, Sam returns `no_confident_match`. A low-confidence match is never presented as a
confident result.

Thresholds are calibrated on a frozen evaluation set. Confident wrong dispatches are treated
as more costly than a clarification.

## Stage 6 — Slot resolution and dispatch

Slots are typed: enum, number with unit, duration, time, path reference, application
reference, device reference, player reference. Entity values are resolved against live local
catalogues with ranked candidates. Sam dispatches exactly one handler; it does not improvise a
sequence.

## Stage 7 — Embeddings

If used:

- Registry embeddings are precomputed at build time and shipped with the registry.
- A compact sentence-embedding encoder encodes only the incoming query.
- The encoder is loaded lazily and can be unloaded.
- Vectors are stored compactly, partitioned by domain.
- A flat vector index is the default; a vector database is introduced only if measurement
  justifies it.
- If the encoder is unavailable or over budget, retrieval falls back to lexical matching and
  returns a safe no-match where confidence is insufficient.

Embedding inference is vectorisation, not reasoning. It does not generate text, plan actions
or retain state.

## Stage 8 — Cache interaction

Before retrieval, check the bounded query cache with a key covering: normalised query, scope,
permission context and relevant source/version identifiers. After dispatch, cache only results
whose policy allows it, with a TTL and dependency tags. Live and volatile values bypass or
shorten the TTL as appropriate.

## Evaluation

Retrieval quality is measured, not asserted. The frozen evaluation set should include:

| Category | Share | Expected behaviour |
|---|---:|---|
| Common clean local query | 25% | Correct handler and result |
| Natural paraphrase / synonym | 25% | Same canonical intent |
| Incomplete or colloquial wording | 10% | Resolve with sufficient context; otherwise clarify |
| Ambiguous entity or intent | 10% | One concise clarification |
| Stale or unavailable source | 5% | Report source status; do not guess |
| Out of domain | 15% | Typed handoff |
| Adversarial or malformed input | 10% | Reject, clamp or hand off |

Initial targets, once measured:

- at least 98% top-1 on clean supported queries;
- at least 95% on varied paraphrases;
- at least 95% of out-of-domain queries correctly handed off;
- 1% or less confident wrong dispatch.

These are provisional and must be validated on real hardware and real registry data.
