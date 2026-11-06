# Registry design

**Status:** Design with a hand-authored prototype registry · **Scope:** Sam's search registry

The Fedora prototype's registry is `data/intents.toml` for grammar and actions, plus live
providers for system information and documentation. The build-time authoring pipeline below
is still the intended long-term form.

The registry is the central source of truth for what Sam supports. Each canonical capability
is represented once and may carry many phrasings. Registry data is versioned, reviewable,
testable and compiled into runtime-friendly artifacts.

---

## Principles

- **One capability, many phrasings.** Do not create a script per sentence.
- **Dense but precise.** Coverage comes from breadth of domains and high-quality aliases, not
  from arbitrary thousands of low-quality examples.
- **Grounded.** Every handler maps to a real local source that can be inspected and tested.
- **Versioned.** Registry, handlers, embeddings and evaluation cases ship as one compatible
  bundle.
- **Independently testable.** A registry change must be verifiable without a running desktop.

## Entry schema

| Field | Purpose |
|---|---|
| `id` | Stable intent identifier |
| `version` | Schema version for the entry |
| `domain` | Filesystem, applications, settings, hardware, system, documentation, Essential OS, or another approved domain |
| `canonical_intent` | The single operation or retrieval intent represented by this entry |
| `description` | Human-readable definition and explicit boundaries |
| `phrases` | Seed queries, paraphrases, synonyms, colloquial wording, abbreviations and common misspellings |
| `keywords` | Fast exact and token-based matching features |
| `embedding_text` | Canonical and paraphrase text used to produce precomputed vectors |
| `slots` | Typed parameters: path, file kind, application, device, time range, result count |
| `source` | Authoritative local index, OS interface, settings catalogue or documentation collection |
| `handler` | Stable identifier for a prewritten, version-controlled handler |
| `permissions` | Read scope, protected paths, required capabilities, allowed actions |
| `response_schema` | Typed output contract |
| `response_template` | Concise deterministic presentation |
| `fallback` | Clarification rule, no-match threshold and caller handoff reason |
| `eval_cases` | Positive, ambiguous, negative, out-of-domain and adversarial cases |
| `cache_policy` | Eligibility, TTL, dependency tags and invalidation triggers |
| `related_intents` | Nearest neighbours used for confusion checks |

Illustrative entry (schema not final):

```json
{
  "id": "system.memory.top_consumers",
  "version": 1,
  "domain": "system.live_state",
  "canonical_intent": "list_memory_consumers",
  "description": "List processes using the most resident memory",
  "phrases": [
    "what is using my RAM",
    "what is eating my memory",
    "show the biggest memory users",
    "which apps are using the most RAM"
  ],
  "keywords": ["ram", "memory", "process", "usage", "consuming"],
  "embedding_text": [
    "Find applications and processes with the highest memory use"
  ],
  "slots": {
    "limit": {"type": "integer", "default": 5, "min": 1, "max": 20}
  },
  "source": "procfs.process_snapshot",
  "handler": "system.list_memory_consumers",
  "permissions": {"scope": "read_system_metrics"},
  "response_schema": "process_list.v1",
  "response_template": "Top {count} memory consumers",
  "cache_policy": {
    "mode": "short_ttl",
    "ttl_ms": 1500,
    "invalidate_on": ["process_table_changed"]
  },
  "fallback": {
    "on_source_error": "source_unavailable",
    "on_low_confidence": "no_confident_match"
  },
  "eval_cases": [
    "what is using my RAM",
    "show biggest memory users",
    "tell me the latest election result"
  ]
}
```

The handler is fixed and reviewed. It reads the approved process source, applies a bounded
limit and returns structured metrics. It does not execute the query text or generate its own
explanation.

## Model-facing artifacts

The registry is also the source of truth for the model tiers. The same entry produces two
artifacts:

| Artifact | Consumer | Contents |
|---|---|---|
| Runtime catalogue | `sam-core` | Grammar, providers, params, validation and policy |
| Exported tool schema | Tier 1 router, fine-tuning teacher, golden-set generator | Name, description, JSON Schema parameters, risk tier, confirmation rule |

Rules:

- The exporter reads `ActionSpec`, not a hand-maintained second list.
- The schema uses typed parameters with bounds, enums and required flags, matching
  `Registry::validate` exactly.
- A fingerprint over the schemas is recorded with every model and every evaluation run.
- A schema change invalidates cached tool embeddings and requires a new model artifact.
- Registry, handlers, schemas and model artifact are released as one compatible bundle.
- Descriptions are written for a reader with no world knowledge; they say what the capability
  does and what it does not do.

The contracts are in [`model-tiers.md`](model-tiers.md).

## Probes and window actions

Two capability families have their own validation rules.

**Probes** are read-only investigation entries. Each declares:

| Field | Rule |
|---|---|
| `program` | Absolute path from the executable allowlist; never a shell |
| `args` | Fixed vector; slots only where declared |
| `read_only` | Must be true; the registry refuses a probe that is not |
| `timeout_ms` | Bounded, default 3000 |
| `max_output` | Bounded, default 64 KiB |
| `roots` | Optional path allowlist for file-reading probes |
| `template` | Deterministic answer where one is possible |

A probe is not a recipe for a mutation. Any program that can write, install, delete, move or
reach the network is rejected at validation time, not at run time. The executor rules are in
[`investigation.md`](investigation.md).

**Window actions** are `LowRisk` or `Destructive` entries backed by the GNOME Shell extension:

- `window.list` is `Read`;
- minimise, maximise, restore, centre, move, activate and show-desktop are `LowRisk` with
  `Confirmation::Never`;
- `window.close` is `Destructive` with `Confirmation::Always`.

The target is a string resolved against app id, WM_CLASS and window title. A target that matches
several windows must clarify. The contract is in [`window-control.md`](window-control.md).

## Domains

Initial domains and their source classes:

| Domain | Source class | Read-only |
|---|---|---|
| Filesystem and local content | File catalogue, metadata index, optional content index | Yes |
| Applications | Desktop-entry catalogue, package metadata, local help | Yes |
| Settings and OS navigation | Settings taxonomy and setting-page identifiers | Yes |
| Hardware and devices | Device inventory and OS hardware interfaces | Yes |
| Live system state | Approved metrics, process and service interfaces | Yes |
| Networking | NetworkManager / BlueZ state and diagnostics | Yes |
| Media and codecs | Media metadata, player state, codec information | Yes |
| Local documentation | Bundled help, manuals, installed man pages | Yes |
| Essential OS | OS documentation, component metadata, update state | Yes |

## Authoring pipeline

A highly capable model may be used **at build time** to draft domain taxonomies, canonical
intents, paraphrases, synonyms, slot definitions, response templates, test cases, candidate
handler mappings and training data for the model tiers. That model is a development dependency:

- it is not shipped with Sam;
- it is not called to answer user queries;
- it is not part of the installed memory footprint.

Every generated entry must be reviewed, validated against a real local source, tested and
versioned before release. Generated training examples are additionally validated against the
exported schema, as described in [`fine-tuning.md`](fine-tuning.md).

## Validation

A registry entry is valid only when:

1. its schema parses and its id is unique;
2. every phrase and keyword is normalised and deduplicated;
3. its slots have types, bounds and defaults;
4. its source exists in the approved source catalogue;
5. its handler is implemented and covered by tests;
6. its response schema is registered;
7. its fallback and cache policies are explicit;
8. its evaluation cases include at least one negative and one out-of-domain case;
9. a nearest-neighbour check shows no unresolved confusion with a related intent;
10. a probe is read-only, uses an allowlisted program and fixed argv, and declares a timeout
    and output cap;
11. a window action uses the fixed D-Bus contract and declares its risk and confirmation.

The evaluation corpus is separate from the entry and is defined in
[`evaluation.md`](evaluation.md). A capability is not shipped until the corpus has canonical,
paraphrase and negative cases for it.

## Growth rule

Add coverage when evidence shows a real gap: production-like misses, corrections, failed
evaluations or a new domain. Do not chase unlimited phrase variants for an intent that already
performs well. Prefer fewer precise intents over a large, noisy registry.

## Versioning

- Registry entries carry their own schema version.
- The compiled registry, handler bundle, exported tool schemas, embedding model version, model
  artifact and index schema are released together.
- A registry update must not silently change Ida's action behaviour.
- Migrations must be reversible or rebuildable from approved local sources.
- Installed bundles are hash-checked at startup; nothing is downloaded or replaced silently.
- A schema fingerprint mismatch disables the model tier rather than running it against a
  changed registry.
