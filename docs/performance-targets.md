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

The model tiers make the goal conditional:

| Hardware | Tier 0 | Tier 1 (Needle) | Tier 3 (local answer) |
|---|---|---|---|
| 4 GB | Required | Yes | 1.7B q4 only, lazy; eviction pressure expected |
| 8 GB | Required | Yes | 1.7B or 4B q4 |
| 16 GB+ | Required | Yes | 4B q4/q5, optionally warm |

Tier 0 alone must remain usable on every tier. The 4B model does not fit comfortably on 4 GB.
See [`model-tiers.md`](model-tiers.md).

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
| Service RSS, voice eager | ~248 MB PSS / 276 MB RSS |
| Service RSS, voice lazy or off | ~46 MB RSS |

The speech model takes the process above the provisional 250 MB typical target. The two
speech rows differ because they were taken in different modes and on different builds; treat
them as a range and re-measure before publishing a gate. On a 4 GB machine the next step is
lazy load or idle unload, measured rather than assumed.

Model tiers are only partly measured. The first Needle numbers are in the targets section; the
Tier 3 budgets are still provisional.

## Targets

| Metric | Initial target | Notes |
|---|---:|---|
| Cached query | Under 5 ms | Internal lookup, warm process |
| Simple registry match | Under 10 ms | Excludes source access and UI rendering |
| Complete local search | Under 50 ms | Typical indexed metadata query |
| Simple live system information | Under 100 ms | Simple metrics when source APIs respond normally |
| Tier 1 router, warm | Under 250 ms p95 | Query plus at most five schemas |
| Tier 3 TTFT, warm | Under 1 s p95 | Short prompt, model resident |
| Tier 3 TTFT, cold | Under 4 s p95 | Includes load from disk |
| Window action, warm | Under 150 ms p95 | Session-bus round trip |
| Probe execution | Under 1 s typical, 3 s hard timeout | Fixed argv, output cap |
| Typical resident memory, Tier 0 | Under 250 MB | Normal search path |
| Idle memory, Tier 0 | Under 100 MB | Encoder and models unloaded, no active query |
| Tier 1 resident | 30–120 MB | Sidecar, warm |
| Tier 3 resident, 1.7B q4 | 1.1–1.3 GB | Sidecar, warm |
| Tier 3 resident, 4B q4 | 2.3–2.6 GB | Sidecar, warm |
| Idle CPU | Near zero | No continuous inference or polling |

The first local tune measured a warm 40-call median of 227 ms for the base and 267 ms for the
20-layer tuned artifact on the development laptop, with a four-tool subset. That is below the
250 ms target only in the base case; the tuned number is a median, not a p95, and not a sidecar
measurement. The 20-layer 4-bit archive is 63.4 MB and the base 2-bit archive is 35.3 MB, both
inside the 30–120 MB Tier 1 allowance.

## Suggested memory allocation

| Component | Allowance |
|---|---:|
| Core process | 5–25 MB |
| Registry and lexical indexes | 10–100 MB |
| Optional embedding encoder | 50–200 MB, loaded on demand |
| Tier 1 router sidecar | 30–120 MB, loaded on demand |
| Tier 3 answer sidecar | 1.1–2.6 GB, loaded on demand |
| Bounded cache | 16–64 MB, hard cap |
| Temporary working memory | 10–50 MB |

A hard ceiling around 512 MB is a reasonable initial guard for Tier 0 and Tier 1 together.
Tier 3 is a separate, explicit budget that the hardware tier must allow. Exceeding a budget
should degrade to a lower tier or fail honestly, not grow without bound.

## Design rules that protect the budget

- Precompute registry embeddings at build time.
- Keep the hot registry resident and small; load large domain indexes on demand.
- Prefer flat structures and memory-mapped or disk-backed indexes where useful.
- Make the embedding encoder optional and lazily loaded.
- Do not load a general model in the routing path. The router is small and optional; the answer
  model is larger, optional and unloaded after idle.
- Do not run continuous background indexing; throttle and pause it.
- Cap the query cache and expire it.
- Index metadata by default; make full-content indexing opt-in and budgeted.
- Avoid heavyweight databases until measurement justifies them.
- Report cold and warm TTFT separately; never publish a warm-only number.

## Measurement method

A benchmark harness should report, at minimum:

- cold start and warm start;
- p50, p95 and p99 for each stage: normalise, Tier 0, Tier 1, Tier 2, Tier 3 TTFT, dispatch,
  handler, probe, window action, format;
- resident, shared and peak memory, with and without the encoder and each model;
- probe timeout and truncation rates; window D-Bus failure rate;
- model load and unload time, and the idle period before unload;
- cache hit rate, cache size and eviction behaviour;
- behaviour under competing load (browser, build, index refresh);
- behaviour with the encoder removed, a model sidecar killed, the index stale and a source
  unavailable.

Measure on the minimum reference machine, not only on the development laptop. Record the
machine, OS version, registry version, model version and schema fingerprint with every result.

## Honesty rule

Do not publish a latency or memory claim that has not been measured on named hardware with a
reproducible harness. Update this document only when measurements justify it.
