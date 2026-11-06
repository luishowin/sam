# Retrieval design

**Status:** Design · **Scope:** How Sam turns a query into one confident dispatch

Sam's retrieval layer is deterministic and hybrid. It combines cheap exact and lexical
signals with optional semantic similarity, then applies explicit confidence rules. The fast path
is always Tier 0: no model generates an answer or reasons about a user's intent beyond matching
it to a registered capability. When Tier 0 cannot decide, a small local router may propose one
call from a short candidate list; the proposal is validated exactly like a grammar match. See
[`model-tiers.md`](model-tiers.md) for the tier contracts.

---

## Stage 0 — Fast path

Every query starts here.

1. Normalise the text.
2. Match the grammar in `data/intents.toml` against the whole input.
3. Join the live providers (apps, files, places, settings, system state).
4. Rank with lexical, synonym and fuzzy signals.
5. Apply the confidence floor and the ambiguity margin.

If the top candidate clears both, dispatch. If it clears the floor but not the margin, ask one
clarification. If nothing clears the floor, fall through to the model fallback in Stage 6b.

Stage 0 is the primary path. It must resolve common queries with every model removed. Stages 1
to 8 describe its internals and the fallback that follows it.

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

## Stage 6b — Model fallback

Runs only when Stage 0 did not clear the confidence floor.

1. **Candidate retrieval.** Select at most five capabilities from the registry using the same
   deterministic keyword filter, or the tool-embedding index. The model never sees the whole
   catalogue.
2. **One call.** Send the normalised query and the five schemas to Tier 1. The response is a
   JSON call or `[]`.
3. **Confidence.** Accept only a call at or above the configured threshold. A call below it is
   treated as a miss, not as a weak action.
4. **Validate.** Pass the call through `Registry::validate` and the policy table. A call that
   fails validation is discarded and logged; it is never repaired by the model.
5. **Dispatch or clarify.** A valid call goes to Stage 6. `[]` goes to Stage 2 (decline or
   handoff) or Tier 3 for a general question.

The model is a fallback for the grammar, not a replacement. A model-sourced intent carries
`Source::Model`, travels the same validation and policy path, and is marked ambiguous for the
confirmation rules.

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
or retain state. It is not the same model as the Tier 1 router: the encoder ranks candidates,
Needle selects a call.

## Stage 8 — Cache interaction

Before retrieval, check the bounded query cache with a key covering: normalised query, scope,
permission context and relevant source/version identifiers. After dispatch, cache only results
whose policy allows it, with a TTL and dependency tags. Live and volatile values bypass or
shorten the TTL as appropriate.

## Evaluation

Retrieval quality is measured, not asserted. The frozen evaluation set should include:

| Category | Share | Expected behaviour |
|---|---:|---|
| Common clean local query | 15% | Correct action and arguments |
| Natural paraphrase / synonym | 30% | Same canonical intent |
| STT-noisy wording | 20% | Correct, or one clarification |
| Missing argument | 10% | Clarify, never guess |
| Out of domain / negative | 15% | No call; handoff allowed |
| Follow-up | 5% | Correct with the prior call in scope |
| Destructive / adversarial input | 5% | Refuse or confirm; never execute silently |

Initial targets, once measured:

- at least 98% top-1 on clean supported queries;
- at least 95% on varied paraphrases;
- at least 95% of out-of-domain queries correctly declined or handed off;
- 1% or less confident wrong dispatch;
- no case where a wrong call is executed.

The harness, scoring and decision gates are in [`evaluation.md`](evaluation.md). These targets
are provisional and must be validated on real hardware and real registry data.
