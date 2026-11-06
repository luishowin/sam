# Tests

**Status:** Fedora prototype suite exists. It covers the core, adapters, voice and a frozen command corpus; the full golden set, model tiers, 4 GB and failure-injection gates are still pending.

Testing Sam is primarily the testing of routing quality, deterministic execution, resource
behaviour and the product boundary. The intended layers are below.

---

## Unit

- query normalisation and tokenisation;
- registry parsing, uniqueness and schema validation;
- lexical and synonym matching;
- embedding lookup and vector comparison;
- ranking arithmetic and threshold application;
- cache keys, TTL and invalidation;
- response template rendering and escaping;
- slot parsing and bounds.

**Pass condition:** 100% of the deterministic unit suite.

## Retrieval evaluation

A frozen golden set with a gold action and expected arguments.

- top-1 action accuracy on clean queries;
- exact argument accuracy;
- paraphrase accuracy;
- STT-noise handling;
- missing-argument clarification;
- follow-up handling;
- out-of-domain and negative recall;
- confident-wrong dispatch rate;
- destructive and adversarial cases.

The corpus, categories, scoring and targets are in [`../docs/evaluation.md`](../docs/evaluation.md).

## Model contract

The model tiers add their own tests, independent of model quality:

- an unknown action is rejected;
- a missing required argument is rejected;
- an extra argument is rejected;
- an out-of-range value is rejected;
- a valid call travels the registry and policy path;
- `[]` becomes a decline or handoff, never an error;
- a below-threshold call never executes;
- a schema fingerprint mismatch disables the tier;
- a killed sidecar falls back to Tier 0;
- a local answer is labelled and never claims a source.

**Pass condition:** a model can never cause an execution that a grammar match could not.

## Window and probe

- window resolver: app id, WM_CLASS, title, fuzzy, multiple matches, no match;
- window geometry: work area, halves, centre, clamping;
- `window.close` requires confirmation and declining leaves the window open;
- missing or disabled extension returns `source_unavailable`;
- D-Bus timeout and window-closed race are reported, not retried;
- probe registry rejects a non-read-only program, a shell, a pipe or a path outside its roots;
- probe timeout kills the process and reports truncation;
- adversarial probes (`cat ~/.ssh/id_rsa`, `delete`, `curl`, `chmod`) are refused;
- content search never scans a denied root.

**Pass condition:** no window method beyond the registered list, and no probe that can mutate.

## Source and adapter

- index freshness;
- permission scope and denied roots;
- source unavailability;
- handler timeout;
- provenance and freshness reporting.

**Pass condition:** no unauthorised reads; every failure is explicit.

## Latency and memory

- cold and warm p50/p95/p99 per stage, including Tier 1 and Tier 3 TTFT;
- encoder, router and answer model load and unload;
- index sizes;
- resident and peak memory in each profile: idle, voice warm, Tier 1 warm, Tier 3 warm;
- cache churn over long runs;
- behaviour under a 4 GB system with competing load.

**Pass condition:** meets the measured budgets without unbounded growth. See
[`../docs/performance-targets.md`](../docs/performance-targets.md).

## Safety and privacy

- path traversal and symlink escapes;
- permission changes between index and access;
- instruction-like text inside indexed documents;
- instruction-like text inside a tool schema or a model answer;
- invalid or forged action identifiers;
- a model call that fails validation;
- a probe that tries to mutate, use the network or leave its roots;
- a window action that resolves ambiguously or closes without confirmation;
- cache isolation across scopes;
- no network egress;
- no query-history store;
- no sensitive content in the cache.

**Pass condition:** zero policy violations and zero egress.

## Fault injection

- kill the indexer, encoder or router sidecar;
- disable the window extension or make its D-Bus call time out;
- make a probe hang, flood output or exit non-zero;
- corrupt the registry;
- present a stale index;
- present a schema fingerprint mismatch;
- fill the disk;
- remove a source;
- run offline with and without the local answer model.

**Pass condition:** documented fallback and an honest user-visible failure.

## Integration with Ida and Essential OS

- no duplicate indexes;
- correct handoff reasons;
- Sam works with Ida absent;
- Ida reports Sam unavailability without inventing an answer;
- a registry update does not silently change Ida's action behaviour;
- no action authority leaks into Sam, and no external model or network call is made.

## Method

- Tests must be reproducible from a clean checkout.
- Evaluation sets are frozen and versioned with the registry.
- Test halves are not used for tuning.
- Results are recorded with registry, handler, encoder, model and schema-fingerprint versions.
