# Tier 3 model experiment runbook

**Status:** In progress · **Scope:** How the Qwen3 / Qwen3.5 / Gemma 4 comparison is built and run

This is the practical companion to
[`tier3-model-experiments.md`](tier3-model-experiments.md). It records the
commands and the environment so the result can be reproduced. The harness
itself lives in [`../scripts/model-bench/`](../scripts/model-bench/).

---

## Environment

| Piece | Value |
|---|---|
| Machine | HP ProBook, AMD Ryzen 5 5600U, 6 cores / 12 threads |
| Memory | 14 GiB, no CUDA GPU |
| OS | Fedora Linux 45, kernel 7.2.15 |
| Runtime | `llama.cpp` 0.7.0-dev, commit `1a0b56c` |
| Python | 3.13 (stdlib only for the harness) |
| Context | 2048 tokens |
| Threads | 6 (`--threads`, `--threads-batch`) |
| Models | `~/.cache/sam-bench/models/` |

The machine is also a real desktop session, so the benchmark records baseline
memory. The 4 GiB tier is evaluated as headroom against that session, not as an
empty machine.

## 1. Build or pin llama.cpp

The measurements in this repository used the developer's existing build at
`/home/luishowin/src/llama.cpp`:

```sh
git clone https://github.com/ggml-org/llama.cpp.git
cd llama.cpp
git checkout 1a0b56cec73dac938e0602c974b21dc8e2862830
cmake -B build -DCMAKE_BUILD_TYPE=Release -DGGML_NATIVE=ON
cmake --build build -j 8
```

Record the commit in the run manifest. Do not compare runs from different
engine commits. On this machine the existing build is at
`~/src/llama.cpp/build/bin/llama-server`; the Stage A orchestrator falls back
to it if `target/llama.cpp/build/bin/llama-server` is absent.

## 2. Download the screen models

```sh
cd /home/luishowin/Documents/github/sam
scripts/model-bench/download.sh screen
```

The script caches under `~/.cache/sam-bench/models/` and skips files it
believes are complete. A previous interrupted download can leave a short file;
check the size against the Hugging Face tree before trusting a cached file.

The screen set is:

| Family | Sizes | Q4_K_M size |
|---|---|---:|
| Qwen3 | 0.6B, 1.7B, 4B | 0.40 / 1.11 / 2.50 GB |
| Qwen3.5 | 0.8B, 2B, 4B | 0.53 / 1.28 / 2.74 GB |
| Gemma 4 | E2B, E4B | 3.11 / 4.98 GB |

The developer machine already had Gemma 4 E2B/E4B and Qwen3.5 2B in
`~/models/`; the runbook uses symlinks into the benchmark cache so the
manifest can still hash the exact file.

## 3. Build the suites

```sh
python3 scripts/model-bench/make_suites.py
```

This writes `suites/tool_calls.jsonl`, `suites/answers.jsonl` and
`suites/tools.json`. The generated files are the versioned corpus.

Counts:

- tool calls: 120
- answers: 90
- tools: 39 + `clarify`

## 4. Pilot

Run two cases on one model before committing hours to a full pass:

```sh
python3 scripts/model-bench/run.py \
  --models gemma4-e2b-q4km \
  --suite tool --limit 2 \
  --server /home/luishowin/src/llama.cpp/build/bin/llama-server \
  --out target/bench/pilot
```

Check:

- the server starts and `/health` returns 200;
- the response is schema-valid JSON, not a markdown fence;
- `reasoning_content` is empty when thinking is disabled;
- TTFT and RSS look plausible.

## 5. Stage A — screen

```sh
python3 scripts/model-bench/run.py \
  --stage screen --suite both \
  --server /home/luishowin/src/llama.cpp/build/bin/llama-server \
  --threads 6 --ctx 2048 \
  --min-battery 20 \
  --out target/bench/stageA
```

The laptop cannot sustain the benchmark on its charger. Both `run.py` and
`run-stage-a.sh` take `--min-battery` (default through the orchestrator: 20).
The runner checks the first battery percentage every five cases, stops
cleanly before the next case at or below the threshold, writes the partial raw
JSONL and `summary.json`, and exits with code 42. The orchestrator writes
`target/bench/stageA/BATTERY_STOP` and does not write `DONE`, so a resume can
distinguish an interrupted run from a complete one.

The runner writes one raw JSONL per model and suite plus `manifest.json` and
`summary.json`.

Score it:

```sh
python3 scripts/model-bench/score.py target/bench/stageA
```

Read `report.md` per category, not just the overall percentage.

## 6. Stage B — quant sweep

Pick the best size in each family from Stage A, then run:

- `Q4_0`
- `Q5_K_M`
- `Q8_0`
- `Q3_K_M` on the 4B class, to test the 4 GiB tier
- Gemma 4 QAT `q4_0`, if the pinned engine accepts it

The prepared candidates are in `scripts/model-bench/quants.toml`. Fetch only
the files needed, rather than the full 27-file matrix:

```sh
scripts/model-bench/download-one.sh unsloth/Qwen3.5-2B-GGUF Qwen3.5-2B-Q5_K_M.gguf
```

Then run the selected ids with the alternate matrix:

```sh
python3 scripts/model-bench/run.py \
  --models-file scripts/model-bench/quants.toml \
  --models qwen3.5-2b-q5km,qwen3.5-2b-q4-0,qwen3.5-2b-q8 \
  --suite both --threads 6 --ctx 2048 \
  --out target/bench/stageB-qwen3.5-2b
```

Use the same prompts and context as Stage A. Do not compare a Stage B run
against a Stage A run made with a different system prompt.

## 7. Stage C — resources

```sh
python3 scripts/model-bench/resources.py \
  --models qwen3.5-2b-q4km,gemma4-e2b-q4km \
  --bench /home/luishowin/src/llama.cpp/build/bin/llama-bench \
  --threads 4,6,8,12 --runs 3 \
  --out target/bench/resources-finalists.json
```

`run.py` already records load RSS, peak RSS and swap for the finalists. Run the
cold/warm TTFT pass only while the machine is idle; if the Needle fine-tune is
active, log the load average and do not publish the timing.

## 8. Write the report

- update [`tier3-model-experiments.md`](tier3-model-experiments.md) with the
  tables, per-category results and the recommendation;
- record every command, failure and decision in
  [`tier3-model-log.md`](tier3-model-log.md);
- update [`model-tiers.md`](model-tiers.md) and
  [`performance-targets.md`](performance-targets.md) only where the data
  justifies a change;
- keep raw results under `target/bench/`; do not commit weights or raw files.
