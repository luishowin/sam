# Tests

**Status: planning only.** No test suite exists yet.

Testing Sam is primarily the testing of retrieval quality, deterministic execution, resource
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

A frozen query set with a gold intent and expected result source.

- top-1 intent accuracy on clean queries;
- paraphrase accuracy;
- ambiguity handling;
- out-of-domain recall;
- confident-wrong dispatch rate;
- negative and adversarial cases.

Initial provisional targets are listed in
[`../docs/retrieval-design.md`](../docs/retrieval-design.md).

## Source and adapter

- index freshness;
- permission scope and denied roots;
- source unavailability;
- handler timeout;
- provenance and freshness reporting.

**Pass condition:** no unauthorised reads; every failure is explicit.

## Latency and memory

- cold and warm p50/p95/p99 per stage;
- encoder load and unload;
- index sizes;
- resident and peak memory;
- cache churn over long runs;
- behaviour under a 4 GB system with competing load.

**Pass condition:** meets the measured budgets without unbounded growth. See
[`../docs/performance-targets.md`](../docs/performance-targets.md).

## Safety and privacy

- path traversal and symlink escapes;
- permission changes between index and access;
- instruction-like text inside indexed documents;
- invalid or forged action identifiers;
- cache isolation across scopes;
- no network egress;
- no query-history store;
- no sensitive content in the cache.

**Pass condition:** zero policy violations and zero egress.

## Fault injection

- kill the indexer or encoder;
- corrupt the registry;
- present a stale index;
- fill the disk;
- remove a source;
- run offline.

**Pass condition:** documented fallback and an honest user-visible failure.

## Integration with Ida and Essential OS

- no duplicate indexes;
- correct handoff reasons;
- Sam works with Ida absent;
- Ida reports Sam unavailability without inventing an answer;
- a registry update does not silently change Ida's action behaviour.

## Method

- Tests must be reproducible from a clean checkout.
- Evaluation sets are frozen and versioned with the registry.
- Test halves are not used for tuning.
- Results are recorded with registry, handler and encoder versions.
