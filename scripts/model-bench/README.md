# Tier 3 model benchmark

This directory holds the throwaway harness used to choose a Tier 3 model for
Sam. It is a lab tool, not part of the Sam runtime. The results and the decision
are written up in [`docs/tier3-model-experiments.md`](../../docs/tier3-model-experiments.md).

The harness is deliberately small and CPU-only:

- `download.sh` fetches the GGUF files into `~/.cache/sam-bench/models/`.
- `make_suites.py` builds the frozen tool-call and answer suites.
- `run.py` starts one `llama-server` per model and records streamed TTFT,
  total latency, output, peak RSS and swap.
- `score.py` scores a run and writes `scores.json`, `report.md` and an audit
  sample.
- `resources.py` runs `llama-bench` across thread counts.

## Requirements

- a recent `llama.cpp` build with `llama-server` and `llama-bench`; the run
  records the commit in `manifest.json`
- Python 3.11+ (stdlib only)
- enough disk for the model set; the screen stage is about 17 GB

Set `SAM_BENCH_MODELS` to use a different model cache.

## Suites

`suites/tool_calls.jsonl` has 120 rows: 20 canonical, 30 paraphrase, 20
STT-noisy, 15 missing-argument, 15 negative and 20 destructive/adversarial.
Each row expects either an exact call, a `clarify` call with the missing slot,
or `[]`.

`suites/answers.jsonl` has 90 rows: local system facts, general knowledge,
maths/conversions, current events, safety, ambiguity and citation honesty.
Each row records whether the correct behaviour is an answer, a refusal or a
clarification, plus key-point regexes and forbidden patterns.

`suites/tools.json` mirrors the public action descriptions in
`crates/sam-core/src/action.rs`. Each query gets a deterministic 5-tool
shortlist; the expected tool is always included, so the score measures
selection and arguments rather than retrieval. `clarify` is added only for
rows that are allowed to ask (missing-argument), not as a generic fallback for
out-of-domain or destructive requests.

## Commands

```sh
# screen-stage downloads
scripts/model-bench/download.sh screen

# rebuild the suites after editing make_suites.py
python3 scripts/model-bench/make_suites.py

# one model, both suites
python3 scripts/model-bench/run.py \
  --models qwen3.5-2b-q4km \
  --server target/llama.cpp/build/bin/llama-server \
  --out target/bench/stageA-qwen3.5-2b

# score it
python3 scripts/model-bench/score.py target/bench/stageA-qwen3.5-2b

# raw throughput across threads
python3 scripts/model-bench/resources.py \
  --models qwen3.5-2b-q4km \
  --bench target/llama.cpp/build/bin/llama-bench \
  --out target/bench/resources-qwen3.5-2b.json
```

## Measurement rules

- Temperature 0, top-p 1, seed 0, context 2048, one server slot (`-np 1`),
  `--threads` and `--threads-batch` recorded in the manifest. Sam serves one
  request at a time; the default four slots inflate RSS.
- Prompt caching is disabled (`cache_prompt: false`) so every case pays the
  full prompt-processing cost. The first Stage A run accidentally left the
  server default on; it is kept as a cache-on comparison and is not used for
  the final ranking.
- Thinking is disabled with `--reasoning off --reasoning-budget 0`. Gemma 4
  E2B/E4B otherwise emit a thought block even when the chat template's
  `enable_thinking` is false; the zero budget is the only reliable switch in
  the pinned build.
- Tool calls use constrained JSON decoding from the same candidate schema the
  scorer validates.
- TTFT and RSS runs must happen while the machine is idle. Accuracy runs may
  run alongside other work but the manifest still records the conditions.
- Model files are hashed into `manifest.json`. Raw output stays under
  `target/bench/` and is not committed.

## What it does not measure

- It does not test the future Sam sidecar protocol, Unix-socket transport or
  idle unload. Those belong to the Tier 3 integration phase.
- It does not test vision or audio, even though Gemma 4 and Qwen3.5 are
  multimodal. The text GGUF is loaded without a projector.
- It does not treat GGUF as the shipping runtime. The winner must be re-tested
  on the runtime Sam actually adopts.
