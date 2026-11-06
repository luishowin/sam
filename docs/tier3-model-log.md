# Tier 3 model experiment log

**Status:** In progress · **Scope:** Chronological record of the model comparison

This is the audit trail: commands, failures, environment changes and decisions.
Results live in [`tier3-model-experiments.md`](tier3-model-experiments.md);
commands live in [`tier3-model-runbook.md`](tier3-model-runbook.md).

---

## 2026-11-06

### Environment

- Fedora 45, kernel 7.2.15, AMD Ryzen 5 5600U (6C/12T), 14 GiB RAM, no CUDA.
- Existing `llama.cpp` build: `1a0b56c`, version `0.7.0-dev`.
- Existing models found in `~/models/`:
  - `chat/gemma4-e2b-it-Q4_K_M.gguf`
  - `chat/qwen3.5-2b-Q4_K_M.gguf`
  - `vision/gemma4-e4b-it-Q4_K_M.gguf`
- These three were symlinked into `~/.cache/sam-bench/models/` under the
  benchmark file names.

### Harness

- Added `scripts/model-bench/`:
  - `models.toml`, `download.sh`
  - `benchlib.py`, `make_suites.py`
  - `run.py`, `score.py`, `resources.py`
  - `suites/tool_calls.jsonl`, `suites/answers.jsonl`, `suites/tools.json`
- Suite counts: 120 tool cases, 90 answer cases, 39 tools + `clarify`.

### Findings

- Gemma 4 E2B/E4B are much larger than their "effective" parameter names:
  Q4_K_M is 3.11 GB and 4.98 GB respectively, versus 1.28 GB for Qwen3.5 2B
  and 2.74 GB for Qwen3.5 4B.
- Gemma 4 E2B/E4B still emit a thought block with the HF template's
  `enable_thinking=false`. `--reasoning off --reasoning-budget 0` is the
  reliable switch in the pinned `llama.cpp` build.
- Qwen3.5 2B, tool suite, 2-case pilot: TTFT 3.5–4.3 s, total 4.5–5.0 s,
  RSS ~2.0 GB, prompt ~360–380 tokens.
- Gemma 4 E2B, tool suite, 2-case pilot after the reasoning fix: TTFT 4.9–5.5 s,
  total 6.7–8.2 s, RSS ~4.3–4.5 GB.
- Qwen3.5 2B, answer suite, 2-case pilot: TTFT 0.26–1.06 s, total 2.6–3.9 s.
- The first full Qwen3.5 2B run was started before `cache_prompt: false` was
  added. It is kept as a cache-on comparison; the orchestrator reruns it with
  prompt caching disabled so every model pays the same full prefill cost.

### Download note

- Hugging Face throughput from this machine was roughly 0.2–0.8 MB/s; the
  screen set is therefore taking hours. The three locally cached models were
  used to start Stage A while the rest downloaded.
- A truncated `Qwen3-1.7B-Q4_K_M.gguf` was detected (163 MB instead of
  ~1.11 GB) and must be removed and re-fetched before scoring that model.

### Benchmark corrections

- The first Qwen3.5 2B full run exposed a scoring problem: `clarify` was in
  every candidate list, so the model used it as a generic fallback for
  negative and destructive queries. The suite now includes `clarify` only for
  missing-argument rows; negative and destructive rows must return `[]`.
  The affected run is kept for comparison and is not the final Stage A result.
- The first Stage A orchestrator used the repository-local `llama-server`
  path, which does not exist on this machine. It failed four models before
  being stopped. The orchestrator now falls back to the existing build at
  `~/src/llama.cpp/build/bin/llama-server`; the runbook records the path.
- The router prompt was tightened after the first Qwen3.5 2B run: every
  argument must be grounded in the query, defaults are forbidden, and
  out-of-domain / destructive / shell / credential / exfiltration requests
  must return `[]`. The 0.6B run that had started before this change was
  discarded, so all final Stage A models see the same prompt.
- The first resource numbers were taken with llama-server's default four
  parallel slots. Sam handles one request at a time, so the final runs use
  `-np 1`. The four-slot 0.6B and 1.7B runs are archived under
  `target/bench/archive/`; their memory numbers are not comparable.
  The 0.6B behavior results in the archived run are still useful, but the
  final report uses the one-slot run.
- The default batch size also inflated the logits buffer for Qwen's 150k
  vocabulary (0.6B RSS rose to ~1.8 GB after 20 cases). The final runs use
  `-b 256 -ub 256`; the five-case pilot dropped load RSS to ~0.9 GB. The
  report records both load RSS and peak RSS after the full suite, because the
  server's prompt/token cache still grows over the run.

### Decisions

- Use the existing `llama.cpp` commit `1a0b56c` for all runs; do not mix engine
  versions.
- Treat the GGUF/llama.cpp result as a lab measurement, not a shipping-runtime
  decision.
- Keep the 4 GiB conclusion as headroom against the real desktop session, not
  as a claim from an empty machine.

### Stage A — first two models

One server slot, batch 256, cache off, tightened prompt.

| Model | Tool pass | Answer pass | Conf-wrong | Unsafe | TTFT p50 | Load RSS | Peak RSS |
|---|---:|---:|---:|---:|---:|---:|---:|
| Qwen3 0.6B Q4_K_M | 63.3% | 72.2% | 15 | 8 | 1.08 s | 0.85 GiB | 2.58 GiB |
| Qwen3 1.7B Q4_K_M | 71.7% | 80.0% | 6 | 3 | 2.84 s | 1.94 GiB | 3.70 GiB |

Qwen3-4B started next. The 4B TTFT was already ~9 s on the first tool
cases, so it is a 30–40 minute model per suite on this CPU.

Stage B files downloaded for Qwen3 0.6B/1.7B/4B and Qwen3.5 0.8B/2B/4B,
including Q5_K_M, Q4_0, Q8_0 and the 4B Q3_K_M cases.

### 2026-11-06 — stopped for battery

The laptop reached 18% charge and the charger could not keep up with the
benchmark's power draw (`energy-rate: 0 W`, still charging). Stage A was
stopped after Qwen3 0.6B, 1.7B and 4B completed. Qwen3.5 0.8B had reached
30/210 tool cases and was discarded as partial. No further benchmark work was
run on battery.

Completed Stage A rows:

| Model | Tool pass | Answer pass | Conf-wrong | Unsafe | TTFT p50 | Load RSS | Peak RSS |
|---|---:|---:|---:|---:|---:|---:|---:|
| Qwen3 0.6B Q4_K_M | 63.3% | 72.2% | 15 | 8 | 1.08 s | 0.85 GiB | 2.58 GiB |
| Qwen3 1.7B Q4_K_M | 71.7% | 80.0% | 6 | 3 | 2.84 s | 1.94 GiB | 3.70 GiB |
| Qwen3 4B Q4_K_M | 75.0% | 90.0% | 2 | 2 | 7.22 s | 4.21 GiB | 6.33 GiB |

### Still to do

- Finish Stage A for Qwen3.5 0.8B/2B/4B and Gemma 4 E2B/E4B.
- Pick the best size in each family and run the staged quant sweep.
- Run `llama-bench` and the idle resource pass on the finalists.
- Write the final report and update the model-tier docs.

### 2026-11-07 — resumed with a battery guard

Battery was at 57% and charging at ~11 W idle. The benchmark was resumed with
`--min-battery 20`: the runner checks the battery every five cases, stops
cleanly before the next case at or below 20%, writes partial raw data and a
summary, and exits 42; the orchestrator writes `BATTERY_STOP` and stops.
Qwen3.5 0.8B started first. Under load the charger still reported 0 W net
input, so the guard is the only thing preventing a repeat of the previous
shutdown.
