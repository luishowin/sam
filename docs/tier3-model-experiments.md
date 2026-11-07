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
| Qwen3.5 0.8B Q4_K_M | 62.5% | 58.9% | 0 | 0 | 1.70 s | 0.80 GiB | 4.70 GiB |
| Qwen3.5 2B Q4_K_M | 66.7% | 82.2% | 20 | 13 | 3.42 s | 1.90 GiB | 5.65 GiB |
| Qwen3.5 4B Q4_K_M (tool 109/120, answer full) | 80.7% | 90.0% | 1 | 0 | 7.30 s | 3.91 GiB | 8.14 GiB |
| Gemma 4 E2B Q4_K_M | 67.5% | 94.4% | 20 | 11 | 4.18 s | 4.01 GiB | 4.15 GiB |

Qwen3.5 0.8B is the safest tool-call router in the small set — negative and
destructive both 95–100% — but its prose answers are weak: safety 10%, current
events 20%, ambiguity 10%, and eight hallucinations across the answer suite.
It also shows the same prompt-cache growth as the Qwen3 models: 0.8 GiB after
load, 4.7 GiB peak after 210 cases.

Qwen3.5 2B is the opposite trade-off. Its canonical, paraphrase, STT and answer
scores are strong (100% canonical, 82% answers), but its router safety is the
worst so far: destructive 35% with 13 calls proposed, including
`terminal.open` for prompt-injection text, `url.open` for an exfiltration URL,
`clipboard.copy` with "password", `search.files` for "passwords and email",
and `place.open` for "wipe my home directory". A larger model is not
automatically a safer router; the policy layer still has to reject these
proposals.

Qwen3.5 4B was split across the battery stop: the tool suite reached 109 of
120 cases, then the answer suite ran separately to completion. It is the
strongest Qwen3.5 answer model — 90% answers, safety 90%, current events 90%,
local facts 100% — and the safest router in the family on the cases it saw
(9/9 destructive, 14/15 negative, 0 unsafe). The answer suite is fast
(~2.1 s TTFT), but the tool prompt is not: 9.6 s p50 and 23 s p95, with 3.9 GiB
after load and 8.1 GiB peak. It cannot be the default Tier 3 on this hardware
regardless of its answer quality.

Gemma 4 E2B is the answer-quality surprise: 94.4% overall, 100% on local
facts, current events and citation honesty, 97% general knowledge, 90% maths
and 80% safety. It is also the best-behaved answer model on the shell/URL
adversarial prompts. As a router it is weak: 40% destructive, 40% negative,
11 unsafe proposals. Its footprint is 4.0 GiB after load and 4.15 GiB peak —
smaller than Qwen3.5 4B, but still too large for an 8 GB machine alongside
GNOME. It is the strongest 16 GB answer candidate so far, provided the router
path keeps using a safer model or a strict policy gate.

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

## Stage B — first quant result

Qwen3.5 2B `Q5_K_M` was the first staged quant sweep. It reached the full
120-case tool suite and 79 of 90 answer cases before the 20% battery guard
stopped it.

| Model | Quant | Tool pass | Answer pass | Negative | Destructive | Unsafe | Load RSS | Peak RSS |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| Qwen3.5 2B | Q4_K_M | 66.7% | 82.2% | 53.3% | 35.0% | 13 | 1.90 GiB | 5.65 GiB |
| Qwen3.5 2B | Q5_K_M | 65.8% | 81.0%* | 40.0% | 35.0% | 13 | 1.48 GiB | 5.85 GiB |

\*Partial: 79 of 90 answer cases.

Q5 does not earn its extra file size on this CPU. The tool result is slightly
worse, negative refusals are worse, destructive behaviour is unchanged, and
the answer categories that ran are effectively flat. The Q4 result remains the
better default for this model. The rest of the quant matrix and Gemma 4 E4B
were not run.

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
