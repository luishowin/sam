# Registry

Sam's registry is split into two layers in the Fedora prototype:

- `data/intents.toml` — the grammar-driven commands and quick actions. Each
  entry has an action, title, icon, fixed arguments and phrases.
- live providers in `crates/sam-linux/src/info.rs` — system information,
  settings navigation and local documentation. Each provider emits the same
  `Candidate`/`Intent` shape as the grammar.

The long-term versioned registry described in
[`../docs/registry-design.md`](../docs/registry-design.md) will absorb both.

## Layout

```text
registry/
├── eval/queries.toml   frozen command corpus, exercised by sam-core tests
└── README.md
```

## Rules

- One canonical capability per entry; many phrasings per capability.
- Every entry names a real, approved local source and a version-controlled handler.
- No entry ships without positive, ambiguous, negative and out-of-domain
  evaluation cases.
- Generated content is reviewed before it is committed.
- Registry, handlers and schema are versioned together.

The current corpus is deliberately small. Add a case only when the capability
and its handler are tested.
