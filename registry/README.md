# Registry

This directory will hold Sam's versioned search registry: canonical intents, domains,
phrasings, synonyms, slots, sources, handler identifiers, response schemas, fallback rules,
cache policies and evaluation cases.

**Status: empty by design.** No registry entries exist yet. The schema is described in
[`../docs/registry-design.md`](../docs/registry-design.md) and is not final.

## Intended layout

```text
registry/
├── domains/          domain definitions and source mappings
├── intents/          canonical entries, grouped by domain
├── schemas/          machine-readable entry and response schemas
├── embeddings/       compiled vectors and encoder metadata (build artifact)
├── eval/             frozen evaluation sets
└── README.md
```

## Rules

- One canonical capability per entry; many phrasings per capability.
- Every entry names a real, approved local source and a version-controlled handler.
- No entry ships without positive, ambiguous, negative and out-of-domain evaluation cases.
- Generated content is reviewed before it is committed.
- Registry, handlers, embeddings and index schema are versioned together.

Do not add speculative entries to this directory before the schema and source catalogue are
settled. Proposals belong in an issue or a design discussion first.
