# Fine-tuning

**Status:** Design; first tune measured (2026-11-07) · **Scope:** How Sam builds, validates and ships model training data

The teacher is a build-time dependency. It never ships, never answers a query and never appears
in the installed footprint. Its output is data, and data enters the project only after the same
validation a human-written registry entry receives.

The commands, environment and hardware paths are in
[`fine-tuning-runbook.md`](fine-tuning-runbook.md).

---

## What is fine-tuned

| Model | Fine-tuned for | Not fine-tuned for |
|---|---|---|
| Needle (Tier 1) | Tool selection, argument extraction, refusal (`[]`), Fedora GNOME vocabulary | General knowledge, prose, shell commands |
| Local 1–4B (Tier 3) | Optional: concise local answers and the ambiguous tail | Planning, execution, memory, unrestricted shell |

The distinction matters. Needle has no useful world knowledge and no free-text output; tuning it
on trivia wastes capacity and confuses the training signal. If general answers matter, that is a
separate model with a separate evaluation set.

## The teacher

A frontier model (for example Opus 5.5) may generate training conversations at build time. It is
a development dependency:

- it is not shipped with Sam;
- it is not called to answer user queries;
- it is not part of the installed memory footprint;
- its generated examples are not trusted until validated.

The teacher writes against the **exported tool schemas**, never against prose descriptions of
them. Schema drift is the main failure mode: an example that looks correct but names an action
the runtime does not have teaches the model to hallucinate.

## Generating data from the schemas

For each capability, the exporter provides:

- the action id;
- the description and boundaries;
- typed parameters, bounds, enums and required flags;
- the risk tier and the confirmation rule;
- two or three canonical phrasings from the registry.

The teacher then produces, per capability:

- canonical commands;
- natural paraphrases, including colloquial and indirect wording;
- commands with one missing argument;
- STT-style variants (dropped words, homophones, number formats);
- near-miss negatives that mention the domain but need a different tool;
- adversarial text (instructions inside note content, destructive phrasing).

Every generated example carries its source prompt so a reviewer can see why it exists.

## Validation before the set

An example enters the training set only when all of these hold:

1. It parses as the expected JSON shape.
2. `Registry::validate` accepts the action and every argument.
3. Every argument value is grounded in the query or in the declared facts. No invented files,
   apps, devices or numbers.
4. Required arguments are present, or the example is labelled `clarify` on purpose.
5. The example is not a near-duplicate of an existing one.
6. A human has reviewed a sample of each batch.

Rejected examples are logged with the reason. A high rejection rate is a schema-description
problem, not a model problem.

## Dataset composition

Approximate shares for the first tuned artifact. Adjust only with a measured reason.

| Category | Share | Notes |
|---|---:|---|
| Canonical | 15% | One per capability at minimum |
| Paraphrase | 30% | The main gain over the base model |
| STT-noisy | 20% | Homophones, dropped words, number formats |
| Missing argument | 10% | Expected to clarify, not guess |
| Negative / out of domain | 15% | Label is `[]` |
| Follow-up | 5% | Prior call in scope |
| Destructive / adversarial | 5% | Refuse or confirm |

Every capability gets at least a few canonical and paraphrase examples. Rare or dangerous
capabilities get more negatives than positives.

## Data shape for Sam

Needle 3 reads the whole declared toolset. It has no retrieval head, so Sam's candidate
retrieval must pre-filter before the call. The training data has to match that shape:

- `tools.json` holds the full catalogue;
- each training row carries a realistic subset of **4–8 tools**: the correct one, a few
  plausible confusions and one or two irrelevant entries;
- the subset is produced by the same candidate retrieval used at inference, or by hand until it
  exists;
- every row is one JSON object with `query`, `tools`, `answers`, optional `reasoning` and
  optional `system`;
- negatives use `"answers": []`; missing arguments call a `clarify` pseudo-tool rather than
  guessing.

Do not train on the full catalogue and deploy on a subset. The model learns the distribution it
is shown.

Rows must fit the training context (`--max-len`, default 1024). Short descriptions are part of
the model input, not decoration.

## Speech-to-text noise

Typed text and voice text share the router, so the training set must look like the transcript,
not the sentence the user meant. Sam's own logs are the best source.

Sources of noise to include:

- homophones ("wifi" / "why fi", "cache" / "cash");
- dropped articles and verbs;
- swapped numbers ("fifteen" / "fifty");
- app and file names spelled approximately;
- missing punctuation and capitalisation;
- British and American spellings.

Do not train on the clean sentence and hope the model generalises. It will not.

## Negatives and refusals

`[]` is a first-class label. The model must learn to refuse:

- general knowledge and current affairs;
- requests for capabilities Sam does not have;
- destructive requests phrased politely;
- instruction-like text inside file or note content;
- ambiguity that should produce a clarification rather than a call.

A model that never refuses is worse than one that sometimes misses, because the failure is an
action rather than a question.

## Shell and recipe safety

Sam never runs a model-written shell command. That rule is older than the model tiers and does
not change here.

- Do not train Needle to emit raw shell.
- Do not train it to emit command strings that are passed to an executor.
- If shell help is in scope, it is a **recipe**: an intent, a command template with slots, what
  it changes, and a preview of the effect.
- Needle may select a recipe and fill its slots. It may not invent a recipe at runtime.
- Unknown shell requests are displayed, never executed.

The recipe corpus is described in the execution plan's escalation phase and belongs to a later
milestone.

## Holdout and leakage

- The golden set in [`evaluation.md`](evaluation.md) is frozen before generation.
- The teacher never sees the holdout queries.
- Generated examples that resemble a holdout query too closely are removed.
- The dataset hash, seed, teacher version and base-model version are recorded with the artifact.
- A tuned artifact is evaluated on the holdout, the real-use set and the safety suite.
- The holdout is never used for model selection. Choose the epoch, rank and slice on the
  validation split; touch the frozen set once, for the final comparison.
- Argument exact match is recorded raw and after normalisation. Normalisation covers case,
  whitespace, durations, clock times and numbers; it never rewrites the tool name or a path
  extension. Raw match remains the primary metric.

## Training and export

- Base model: a pinned Needle generation and slice size (Needle 3, 20 layers).
- Method: LoRA rank 16 on the attention projections, base frozen, merged at export.
- Local precision: 4-bit at export, which made the first 20-layer archive 63.4 MB; the shipped base
  is 2-bit at 35.3 MB. The hosted platform path trains and exports at 2-bit.
- Package: `cactus-compute`. `[train]` for CPU, `[train,gpu]` for NVIDIA, `[train,metal]` for
  Apple. Inference does not need the training extra.
- Pipeline:
  ```sh
  pip install "cactus-compute[train]"
  cactus finetune data.jsonl --epochs 20 --val-split 0.1 --seed 0 --out adapter.safetensors
  cactus build --lora adapter.safetensors --layers 20 --out sam-needle.cact
  ```
- Slice: `cactus build --layers N` writes a smaller subnetwork from the same adapter. The first
  run's 8-layer slice was 27.4 MB but fell from 26/32 to 14/32 on the frozen holdout; 20 layers is
  the current choice. Intermediate 12/16/18 slices remain untested.
- Confidence: local fine-tuning does **not** train the confidence head. The tuned archive reports
  `confidence = None`. Calibrate a threshold on the validation split (never the frozen holdout),
  keep the base model for gating, or use the hosted platform path if the calibrated score is
  required.
- Versioning: the `.cact` format is tied to the engine version. Pin `cactus-compute` wherever
  the artifact is trained, built and run; rebuild when the engine changes.
- Reproducibility: a clean checkout plus the recorded dataset, seed, package version and config
  reproduces the artifact.
- Size budget: record the artifact size; it is part of the memory budget.
- No training data leaves the machine unless the user chooses to send it to a hosted teacher.

### Measured result

The first completed tune is recorded in
[`fine-tuning-runbook.md`](fine-tuning-runbook.md#first-run-record-2026-11-07). On the frozen
40-row holdout, with the system date fixed:

| Model | Name | Args raw | Args normalised | `[]` | False pos |
|---|---:|---:|---:|---:|---:|
| base Needle 3 | 23/32 | 20/32 | 22/32 | 7/8 | 1 |
| tuned, 20 layers | 26/32 | 24/32 | 26/32 | 8/8 | 0 |
| tuned, 8 layers | 14/32 | 10/32 | 11/32 | 4/8 | 4 |

The 20-layer tune is a candidate, not a shipped artifact: the real-use set, the safety suite and
a sidecar p95 latency measurement are still outstanding. Local tuning also leaves
`confidence = None`.

The exact commands, small-GPU notes and troubleshooting table are in
[`fine-tuning-runbook.md`](fine-tuning-runbook.md).

## Shipping rule

A tuned artifact ships only when:

1. it beats the current base model on the frozen golden set;
2. it beats it on real logged utterances;
3. it does not regress negatives or the safety suite;
4. its size, latency and memory are recorded;
5. the docs name the artifact version and the date it replaced the previous one.

If it wins on the training set and loses on the holdout, it does not ship. That is the whole
point of the holdout.

## Privacy

- Training data comes from the registry, the schemas and the user's own corrected logs.
- Logged utterances are used only with the user's explicit opt-in.
- Paths, secrets and personal content are stripped before an example enters the set.
- Generated data is local unless the user chooses a hosted teacher.
- The artifact contains no personal data; it contains tool schemas and phrasings.

## Open questions

- Which slice gives the best accuracy per megabyte? The 8-layer export collapsed on the first
  holdout (14/32); 12, 16 and 18 layers are untested.
- Is a 2-bit export viable for the local tune, and what does it cost in accuracy?
- Does LoRA rank 32 close the remaining argument-grounding gap (24/32 raw, 26/32 normalised)?
- Is a tuned Needle enough, or does the ambiguous tail need Tier 3?
- How much STT-noise data is needed before the router stops blaming transcription?
- Does a 1–4B teacher-generated answer set improve Tier 3, or does it teach confident guessing?
