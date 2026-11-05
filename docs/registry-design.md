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
intents, paraphrases, synonyms, slot definitions, response templates, test cases and candidate
handler mappings. That model is a development dependency:

- it is not shipped with Sam;
- it is not called to answer user queries;
- it is not part of the installed memory footprint.

Every generated entry must be reviewed, validated against a real local source, tested and
versioned before release.

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
9. a nearest-neighbour check shows no unresolved confusion with a related intent.

## Growth rule

Add coverage when evidence shows a real gap: production-like misses, corrections, failed
evaluations or a new domain. Do not chase unlimited phrase variants for an intent that already
performs well. Prefer fewer precise intents over a large, noisy registry.

## Versioning

- Registry entries carry their own schema version.
- The compiled registry, handler bundle, embedding model version and index schema are released
  together.
- A registry update must not silently change Ida's action behaviour.
- Migrations must be reversible or rebuildable from approved local sources.
- Installed bundles are hash-checked at startup; nothing is downloaded or replaced silently.
