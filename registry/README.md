# Registry

Sam's registry is split into two layers in the Fedora prototype:

- `data/intents.toml` — the grammar-driven commands and quick actions. Each
  entry has an action, title, icon, fixed arguments and phrases.
- live providers in `crates/sam-linux/src/info.rs` — system information and
  the curated GNOME settings map. Each provider emits the same
  `Candidate`/`Intent` shape as the grammar.

The long-term versioned registry described in
[`../docs/registry-design.md`](../docs/registry-design.md) will absorb both.
The same entries also export the tool schemas the model tiers read; see
[`../docs/model-tiers.md`](../docs/model-tiers.md).

## Layout

```text
registry/
├── eval/queries.toml   frozen command corpus, exercised by sam-core tests
├── eval/golden.toml    full golden set with expected arguments (planned)
└── README.md
```

## Rules

- One canonical capability per entry; many phrasings per capability.
- Every entry names a real, approved local source and a version-controlled handler.
- No entry ships without canonical, paraphrase, missing-argument, negative and out-of-domain
  evaluation cases.
- Evaluation cases carry expected arguments, not just the action name.
- The exported tool schema is generated from the same entry; it is never hand-maintained.
- Generated content, including model training data, is validated against the schema and
  reviewed before it is committed.
- Investigation entries are fixed read-only probes: an allowlisted program, a fixed argument
  vector, a timeout and an output cap. A probe that can mutate anything is rejected.
- Window entries use the fixed extension methods and declare their risk; `window.close` is
  destructive and always confirms.
- Registry, handlers, exported schemas and model artifacts are versioned together.

The current corpus is deliberately small. Add a case only when the capability
and its handler are tested. The full harness is in
[`../docs/evaluation.md`](../docs/evaluation.md).
