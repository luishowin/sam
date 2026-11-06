# Fine-tuning

**Status:** Design · **Scope:** How Sam builds, validates and ships model training data

The teacher is a build-time dependency. It never ships, never answers a query and never appears
in the installed footprint. Its output is data, and data enters the project only after the same
validation a human-written registry entry receives.

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

## Training and export

- Base model: a pinned Needle generation and slice size.
- Method: LoRA fine-tune, merged and exported as a single artifact.
- Tooling: Cactus's local tooling; no training data leaves the machine unless the user chooses
  to send it to a hosted teacher.
- Reproducibility: a clean checkout plus the recorded dataset and config reproduces the
  artifact.
- Size budget: record the artifact size; it is part of the memory budget.

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

- Which Needle generation and slice gives the best accuracy per megabyte on this CPU?
- Is a tuned Needle enough, or does the ambiguous tail need Tier 3?
- How much STT-noise data is needed before the router stops blaming transcription?
- Does a 1–4B teacher-generated answer set improve Tier 3, or does it teach confident guessing?
