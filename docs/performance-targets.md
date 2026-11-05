# Performance targets

**Status:** Provisional engineering targets · **Scope:** Sam runtime on low-resource hardware

> **These are planning budgets, not measurements.** The Fedora prototype has development-
> laptop measurements, but the minimum reference machine and release gates are still pending.
> See [`fedora-prototype.md`](fedora-prototype.md).

---

## Compatibility goal

Sam should run comfortably on a computer with **4 GB of total system RAM**, alongside a real
desktop session. That is a system-compatibility goal, not permission for Sam to consume all
available memory.

## Measured on the development laptop

| Metric | Observed |
|---|---:|
| Registry/engine query, warm | 3–6 ms typical |
| Complete LocalSearch query | 10–55 ms |
| Live system answer | 3–30 ms |
| Speech model load, warm | ~0.44 s |
| Speech inference, 4 s clip | ~0.34 s |
| Idle service RSS, speech warm | ~295 MB |
| Idle service RSS, no speech | ~15 MB |

The speech model takes the process above the provisional 250 MB typical target. On a 4 GB
machine the next step is lazy load or idle unload, measured rather than assumed.

## Targets

| Metric | Initial target | Notes |
|---|---:|---|
| Cached query | Under 5 ms | Internal lookup, warm process |
| Simple registry match | Under 10 ms | Excludes source access and UI rendering |
| Complete local search | Under 50 ms | Typical indexed metadata query |
| Simple live system information | Under 100 ms | Simple metrics when source APIs respond normally |
| Typical resident memory | Under 250 MB | Normal search path |
| Idle memory, no encoder resident | Under 100 MB | Encoder unloaded, no active query |
| Idle CPU | Near zero | No continuous inference or polling |

## Suggested memory allocation

| Component | Allowance |
|---|---:|
| Core process | 5–25 MB |
| Registry and lexical indexes | 10–100 MB |
| Optional embedding encoder | 50–200 MB, loaded on demand |
| Bounded cache | 16–64 MB, hard cap |
| Temporary working memory | 10–50 MB |

A hard process ceiling around 512 MB is a reasonable initial guard. Exceeding it should
degrade to lexical search or fail honestly, not grow without bound.

## Design rules that protect the budget

- Precompute registry embeddings at build time.
- Keep the hot registry resident and small; load large domain indexes on demand.
- Prefer flat structures and memory-mapped or disk-backed indexes where useful.
- Make the embedding encoder optional and lazily loaded.
- Do not load a language model.
- Do not run continuous background indexing; throttle and pause it.
- Cap the query cache and expire it.
- Index metadata by default; make full-content indexing opt-in and budgeted.
- Avoid heavyweight databases until measurement justifies them.

## Measurement method

A benchmark harness should report, at minimum:

- cold start and warm start;
- p50, p95 and p99 for each stage: normalise, classify, lexical retrieve, semantic retrieve,
  rank, dispatch, handler, format;
- resident, shared and peak memory, with and without the encoder;
- cache hit rate, cache size and eviction behaviour;
- behaviour under competing load (browser, build, index refresh);
- behaviour with the encoder removed, the index stale and a source unavailable.

Measure on the minimum reference machine, not only on the development laptop. Record the
machine, OS version, registry version and encoder version with every result.

## Honesty rule

Do not publish a latency or memory claim that has not been measured on named hardware with a
reproducible harness. Update this document only when measurements justify it.
