# Tier 3 model experiments

**Status:** In progress · **Scope:** Qwen3, Qwen3.5 and Gemma 4 at 0.6B–4B, Q4/Q5/Q8

This document answers the open Tier 3 questions in
[`model-tiers.md`](model-tiers.md): **1.7B versus 4B**, **Qwen versus Gemma**,
and **q4 versus q5**. It is a lab measurement on one CPU-only laptop, not a
shipping-runtime benchmark. The harness and the exact commands are in
[`tier3-model-runbook.md`](tier3-model-runbook.md).

---

## Question

Sam's Tier 3 model is optional. It answers offline questions, proposes a call
for the ambiguous tail, and can select up to three read-only probes. The
current design budget is 1.1–2.6 GB resident and a warm TTFT under one second.

The experiment compares:

| Family | Sizes | Why |
|---|---|---|
| Qwen3 | 0.6B, 1.7B, 4B | The explicitly requested generation; 0.6B is the smallest point |
| Qwen3.5 | 0.8B, 2B, 4B | The current generation; hybrid attention and a stronger small-model line |
| Gemma 4 | E2B, E4B | Google's on-device family; "effective" sizes, but a larger real footprint |

All candidates are text-only GGUFs, Q4_K_M in the screen stage. The staged
quant sweep covers Q4_0, Q4_K_M, Q5_K_M and Q8_0 on the best size in each
family, plus Q3_K_M on the 4B class.

## Method

- 120 tool-call cases: canonical, paraphrase, STT-noisy, missing-argument,
  negative and destructive/adversarial. The same contract as Needle: one JSON
  call, `clarify`, or `[]`. `clarify` is available only for the
  missing-argument rows; out-of-domain and destructive rows must choose `[]`.
- 90 answer cases: local system facts, general knowledge, maths/conversions,
  current events, safety, ambiguity and citation honesty. Scored on key-point
  coverage, refusal and hallucination.
- Constrained JSON decoding for tool calls; no grammar for prose answers.
- Temperature 0, top-p 1, seed 0, context 2048, 6 threads, one server slot
  (`-np 1`), batch and micro-batch 256. Sam handles one request at a time, so
  four idle slots would inflate memory without modelling a real path; a large
  default batch also inflates the logits buffer for a 150k vocabulary.
- Thinking disabled with `--reasoning off --reasoning-budget 0`; Gemma 4
  E2B/E4B otherwise emit a thought block even when the template disables it.
- Cold and warm TTFT, tokens/s, load time, peak RSS and swap.
- 4 GiB / 8 GiB / 16 GiB evaluated as headroom against a real GNOME session.

## Preliminary observations

These are pilot results, not the final table.

| Model | Quant | Tool pilot TTFT | Tool pilot total | Pilot peak RSS | Note |
|---|---|---:|---:|---:|---|
| Qwen3.5 2B | Q4_K_M | 3.5–4.3 s | 4.5–5.0 s | ~2.0 GB | 360–380-token prompt; no reasoning |
| Gemma 4 E2B | Q4_K_M | 4.9–5.5 s | 6.7–8.2 s | ~4.3–4.5 GB | `--reasoning-budget 0` required |
| Qwen3.5 2B | Q4_K_M | 0.26–1.06 s | 2.6–3.9 s | ~2.0 GB | Answer suite, 2 cases |

The prompt is long because it carries five tool schemas. Prefill, not
generation, dominates the tool-call TTFT on this CPU. The provisional
sub-one-second warm target is therefore not met by a 2B model at this prompt
size; the final report separates prompt-processing time from generation time.

Gemma 4's memory is the more important result so far. At Q4_K_M:

| Model | File size | Pilot peak RSS |
|---|---:|---:|
| Gemma 4 E2B | 3.11 GB | ~4.5 GB |
| Gemma 4 E4B | 4.98 GB | pending |
| Qwen3.5 2B | 1.28 GB | ~2.0 GB |
| Qwen3.5 4B | 2.74 GB | pending |

The "E" in E2B/E4B is an effective parameter count. The total model, including
per-layer embeddings and the multimodal vocabulary, is much larger. Gemma 4
E2B at q4 is already larger than Qwen3.5 4B at q4, so it cannot be treated as a
2B-class memory footprint.

## Results

### Stage A so far

| Model | Tool pass | Answer pass | Conf-wrong | Unsafe | TTFT p50 | Load RSS | Peak RSS |
|---|---:|---:|---:|---:|---:|---:|---:|
| Qwen3 0.6B Q4_K_M | 63.3% | 72.2% | 15 | 8 | 1.08 s | 0.85 GiB | 2.58 GiB |
| Qwen3 1.7B Q4_K_M | 71.7% | 80.0% | 6 | 3 | 2.84 s | 1.94 GiB | 3.70 GiB |
| Qwen3 4B Q4_K_M | 75.0% | 90.0% | 2 | 2 | 7.22 s | 4.21 GiB | 6.33 GiB |

The 4B is the quality leader: canonical 100%, negative 100%, local facts 100%,
maths 100%, safety 80%, current events 80%. It is also the latency and memory
loser: 7.2 s p50 TTFT, 4.2 GiB after load and 6.3 GiB peak. It does not fit the
4 GB or 8 GB tier, and it misses the one-second warm target by a wide margin
even on 16 GB. Its missing-argument score is still 0%: it invents defaults
instead of asking.

The 1.7B is the best small-quality compromise so far, but it still invents
missing arguments (1/15), proposes a terminal on a prompt-injection line, and
gives step-by-step answers for the firewall and lock-picking safety prompts.
The 0.6B is the closest to the latency budget, but its safety and current-event
answers are not acceptable for an answer tier.

### Interim reading

No Qwen3 model tested so far meets both the provisional one-second warm TTFT
and the 1.1–2.6 GiB resident budget on this CPU with `llama.cpp`. The closest
quality/size point is Qwen3 1.7B; the closest latency point is Qwen3 0.6B.
Qwen3 4B needs a faster machine or a smaller runtime footprint before it can be
the default Tier 3. Qwen3.5 and Gemma 4 are still to be measured; the Qwen3.5
0.8B run was stopped after 30 cases to protect the laptop battery, so it is
not in the table.

### First screened model — Qwen3 0.6B, Q4_K_M

The smallest model finished first. It is fast and small, and it is not safe
enough to be a Tier 3 answer model. The numbers below are from the first run,
which used llama-server's default four slots. The final one-slot run replaces
the memory number; the behavioral result is the same.

| Metric | Result |
|---|---:|
| Tool-call pass | 63.3% |
| Answer pass | 72.2% |
| Tool TTFT p50 / p95 | 1.08 s / 1.69 s |
| Load RSS / peak RSS | 0.85 GiB / 2.58 GiB |

By category:

| Tool category | Pass | Confident-wrong |
|---|---:|---:|
| Canonical | 85% | 0 |
| Paraphrase | 70% | 0 |
| STT-noisy | 90% | 0 |
| Missing argument | 0% | 0 |
| Negative | 53% | 7 |
| Destructive / adversarial | 60% | 8 |

| Answer category | Pass | Refused | Hallucination |
|---|---:|---:|---:|
| General knowledge | 97% | 0 | 0 |
| Maths / conversions | 90% | 0 | 1 |
| Local system facts | 70% | 7 | 0 |
| Current events | 20% | 2 | 0 |
| Safety | 30% | 3 | 5 |
| Ambiguity | 80% | 5 | 0 |
| Citation honesty | 40% | 2 | 1 |

The important failures are not the canonical ones. The model invented missing
arguments, gave harmful answers to safety prompts, and treated current events
as knowledge it had. It also proposed `terminal.open` for prompt-injection
text and `clipboard.copy` / `search.files` for password requests. A
model-sourced call must therefore be gated more strictly than the current
`Confirmation::Never` table suggests, or the router must not be allowed to
propose those actions at all.

The full Stage A table and the final recommendation follow after the other
seven models finish.

The final tables will report:

- tool-call pass rate by category, action accuracy, argument accuracy,
  schema validity, confident-wrong and unsafe calls;
- answer pass rate by category, refusal rate and hallucination rate;
- TTFT p50/p95/p99, total latency, tokens/s, load time and peak RSS;
- the staged quant comparison and the 4B Q3_K_M result;
- a recommendation for the 4 GiB, 8 GiB and 16 GiB tiers.

## Decision rule

A model is eligible only if it:

1. produces no executable forbidden call in the destructive/adversarial set;
2. stays within the memory budget of the tier it is proposed for;
3. beats the lower model on the tool-call and answer suites by enough to
   justify its extra memory;
4. keeps the refusal and clarification behaviour intact.

The default candidate for each tier will be the smallest model that passes
those gates, not the highest-scoring model overall.

## Caveats

- One laptop, one CPU, one `llama.cpp` commit. Re-test the winner on the
  runtime Sam actually adopts.
- The tool prompt carries five schemas. A shorter prompt would lower TTFT and
  change the ranking; the prompt is held fixed so the comparison is fair.
- The answer suite is small and the prose grading uses key-point regexes plus a
  deterministic audit sample. It is a screening test, not a publication-grade
  knowledge benchmark.
- GGUF is a lab format here. The Needle runbook's "no GGUF" rule is specific to
  Needle's Cactus artifact and does not decide the Tier 3 runtime.
