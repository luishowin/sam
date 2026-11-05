# Sam

**Sam is a lightweight, deterministic, semantic search and retrieval engine for Essential OS.**

Sam answers questions about the computer it runs on: files, applications, settings,
hardware, networking, running processes and the operating system itself. It does this
through a carefully engineered registry of domains, intents, retrieval rules and
predefined handlers, with no generative model in the runtime.

> **Status: Fedora prototype.** Sam runs on one Fedora 45 / GNOME 51 workstation. The
> deterministic core, Fedora adapters, GTK4 panel, quick actions and local speech input
> exist and are tested. The Essential OS service, full registry authoring pipeline and
> low-resource release gates do not. See
> [docs/fedora-prototype.md](docs/fedora-prototype.md).

---

## Contents

- [What Sam is](#what-sam-is)
- [Why Sam exists](#why-sam-exists)
- [Sam and Ida](#sam-and-ida)
- [Knowledge boundary](#knowledge-boundary)
- [The retrieval pipeline](#the-retrieval-pipeline)
- [Registry-driven architecture](#registry-driven-architecture)
- [Embeddings and lexical retrieval](#embeddings-and-lexical-retrieval)
- [Handlers and response templates](#handlers-and-response-templates)
- [Caching, not memory](#caching-not-memory)
- [Performance and RAM targets](#performance-and-ram-targets)
- [Interface with Ida and Essential OS](#interface-with-ida-and-essential-os)
- [Privacy and security](#privacy-and-security)
- [Current development status](#current-development-status)
- [Planned milestones](#planned-milestones)
- [Contributing](#contributing)
- [Licence](#licence)

---

## What Sam is

Sam is the native search and information-discovery component of Essential OS. Its entire
knowledge domain is the computer it runs on, and everything it returns must be grounded in
a local index, a registered handler or live operating-system data.

Sam is **not** a language model. It does not:

- reason through open-ended problems;
- generate original explanations, prose or code;
- maintain a conversation or any personal memory;
- choose or call a web-search, local-model or cloud provider;
- expand its scope by guessing.

It recognises a supported intent, retrieves the matching registry entry, runs a fixed
handler and formats a structured result. When it cannot answer confidently from its domain,
it returns a typed handoff rather than an invented answer.

A useful mental model is a high-end scientific calculator: fast, reliable, domain-specific
and equipped with many precise operations. It is a multi-tool for the computer, not a
general-purpose intelligence. Sam should feel remarkably capable because of its coverage,
retrieval accuracy, speed and careful execution — not because it simulates reasoning.

## Why Sam exists

Operating-system search is usually either a narrow, literal file-finder or a generative
assistant bolted onto the desktop. The first misses natural phrasing; the second is slow,
non-deterministic, hard to audit and often sends private context off the machine.

Sam takes a third path. Intelligence is moved into deliberate engineering at build time:
a dense registry of canonical intents, phrasings, synonyms and evaluation cases, paired with
deterministic handlers and a hybrid retrieval layer. Everyday local search then feels
immediate and dependable without a model in the loop.

The design goals are:

- **Fast first.** A common local query should feel instant, with no network round-trip and
  no model start-up.
- **Honest boundaries.** Unsupported, ambiguous or stale requests produce a clarification
  or a handoff, never a fabricated answer.
- **Deterministic execution.** A registry entry selects a prewritten, typed handler. User
  text never becomes a shell command.
- **Auditable behaviour.** Every result names its source and reports freshness.
- **Low resource cost.** Sam should remain useful on a 4 GB machine alongside a real desktop.

## Sam and Ida

Sam and Ida are two separate systems with different jobs. Sam does not become a tier of Ida,
an Ida backend, or a renamed Ida subsystem. Ida does not absorb Sam's registry or duplicate
its responsibilities.

| Characteristic | Ida | Sam |
|---|---|---|
| Product role | Personal assistant | OS search assistant |
| Primary purpose | Understand, assist and act | Retrieve and present OS information |
| LLM | Yes, through Tier 2 | No |
| Reasoning | Yes, through Tier 2 | No |
| Voice | Voice-first | Primarily a search UI; integration as required |
| Conversational memory | May maintain controlled context | None |
| Cache | Where useful | Bounded performance cache |
| Domain | General assistance and computer interaction | The computer itself |
| Web search | Can request it | Never invokes it itself |
| Multi-step work | Yes, through Hermes | No |
| OS integration | Uses approved executors and tools | Uses registered local sources and handlers |

Sam is a specialised utility that Ida can call when it needs information about the computer.
Ida interprets the result, explains it conversationally or decides that further reasoning is
needed. Sam never calls Ida, Hermes or a model directly, and Sam works independently as part
of Essential OS without Ida present.

See [docs/integration-with-ida.md](docs/integration-with-ida.md) for the interface and the
ownership rules.

## Knowledge boundary

Sam's domain is the computer and its permitted local information. Initial domains:

| Domain | Examples |
|---|---|
| Filesystem and local content | Find a PDF; count images; locate the latest modified spreadsheet; search a local project |
| Applications | Find the screenshot tool; show installed browsers; locate the app that edits RAW photos |
| Settings and OS navigation | Where is night light? Find keyboard accessibility settings |
| Hardware and devices | What GPU do I have? Which audio output is active? Is the second display detected? |
| Live system state | What is using RAM? How much disk space is free? Is Wi-Fi enabled? |
| Networking | Which network am I connected to? Is Bluetooth on? |
| Media and codecs | What is playing? Which application can open this video? |
| Local documentation | How do I change this setting in the installed app? Find the guide for the backup utility |
| Essential OS itself | What does Essential OS include? Check update status; find the network panel documentation |

Everything outside that domain is out of scope: general knowledge, current affairs, external
research, open-ended explanation, creative writing, planning, multi-step agent work and
general conversation.

If a query mixes local and external requirements, Sam may answer the local portion if it is
explicitly supported, but it must mark the remainder as a handoff. It never pretends that a
local index substitutes for current external information.

## The retrieval pipeline

The intended runtime pipeline is:

```text
query
  ↓
normalise and tokenise
  ↓
classify domain and likely intent
  ↓
retrieve candidate registry entries
  ↓
rank: lexical · synonyms · embeddings · confidence rules
  ↓
resolve required parameters
  ↓
invoke the registered handler
  ↓
retrieve or execute the information request
  ↓
format with predefined response templates
  ↓
return a structured result or typed handoff
```

Natural-language variation is handled by the registry and the ranking layer, not by a model
and not by one script per sentence. For example, these should resolve to one capability:

- "What's eating my RAM?"
- "Why is memory usage so high?"
- "Which apps are consuming the most memory?"
- "Show me the biggest memory consumers."

So should these:

- "Find my latest screenshots."
- "Where are my recent screen captures?"
- "Show me the screenshots I took recently."

The pipeline is described in more detail in [docs/architecture.md](docs/architecture.md) and
[docs/retrieval-design.md](docs/retrieval-design.md).

## Registry-driven architecture

The registry is Sam's most important component. Each canonical capability appears once and
may carry many phrasings. Entries are versioned, reviewable, validated and independently
testable.

A registry entry includes:

- a stable id and schema version;
- a domain and one canonical intent;
- a description with explicit boundaries;
- seed phrases, paraphrases, synonyms, colloquialisms and common misspellings;
- keywords and lexical matching rules;
- text used to produce precomputed embeddings;
- typed slots and entity resolvers;
- an authoritative local source;
- a version-controlled handler identifier;
- permissions and read scope;
- a response schema and template;
- fallback and escalation rules;
- evaluation cases;
- a cache policy.

A new capability is introduced through a registry entry, a handler and tests — not by
retraining a model. The registry is compiled into runtime-friendly artifacts and validated
in CI once implementation begins.

See [docs/registry-design.md](docs/registry-design.md) and [registry/README.md](registry/README.md).

## Embeddings and lexical retrieval

Sam is designed for hybrid retrieval:

1. **Exact matching** for simple, unambiguous requests.
2. **Keyword and lexical retrieval** over names, aliases and metadata.
3. **Synonym and phrase expansion** for natural wording.
4. **Fuzzy matching** for typos and partial names.
5. **Compact semantic embeddings** when lexical retrieval is insufficient.
6. **Deterministic ranking and calibrated confidence thresholds.**

A compact sentence-embedding encoder is permitted because it performs semantic vectorisation,
not generation or reasoning. It never answers a query. Registry embeddings are precomputed at
build time; only the incoming query is encoded at runtime, and only when the lexical path is
not confident.

A heavyweight vector database is not a default requirement. The intended starting point is a
flat vector index with domain partitions, measured before any heavier infrastructure is
introduced. The architecture must remain useful with no embedding model at all, using
deterministic fallback retrieval.

## Handlers and response templates

Each canonical intent maps to one reusable, version-controlled handler. A handler:

- reads from an approved source;
- validates typed inputs and permissions;
- returns a typed result;
- reports errors and source freshness explicitly;
- never assembles a shell command from user text.

Opening a file, launching an application or navigating to a settings page is represented as a
constrained **result action**, not arbitrary execution. Sam proposes actions; the caller
validates and performs them. In the Fedora prototype the panel is the caller and performs a
small fixed set of quick actions (volume, media, radios, night light, power profile, timers).
On Essential OS those route through the OS capability layer or Ida's executor. Sam never
builds a shell command from user text.

Results are structured records, not generated prose: title, concise deterministic
description, entity type, source, location or state, last-updated time, confidence band and
allowed actions. Short summaries use fixed templates. Volatile values carry a measurement
time or are refreshed on demand.

## Caching, not memory

Sam may cache frequent queries and eligible results. That cache is a performance mechanism,
not personal memory. It is bounded, temporary and disposable.

| Allowed cache behaviour | Disallowed memory behaviour |
|---|---|
| Bounded in-memory LRU of recent query plans or safe results | Persistent conversation history or a transcript archive |
| Short TTL for dynamic system data; invalidation on state change | Remembering what a user asked yesterday or inferring preferences |
| Caching stable metadata while its source version is unchanged | Building a user profile, personal timeline or behavioural model |
| Cache keys that include query, scope, permissions and source version | Reusing a result across users, scopes or permission levels |
| A cache that can be cleared at any time | Treating the cache as an authoritative source of truth |

A total-storage query may be cached briefly because the underlying value changes slowly. A
query about how much RAM a specific application is using must read fresh state or use a cache
with appropriate invalidation. Sensitive file contents, secrets and protected metadata do not
enter the result cache by default, and query-frequency statistics do not become a permanent
per-user history.

Sam must never remember previous conversations, store personal facts, learn preferences or
maintain a profile.

## Performance and RAM targets

The primary compatibility goal is a system with **4 GB of total RAM**. The numbers below are
**provisional engineering targets**, not measured results. They must be benchmarked on the
minimum reference machine before becoming release gates.

| Metric | Initial target |
|---|---:|
| Cached query | Under 5 ms |
| Simple registry match | Under 10 ms |
| Complete local search | Under 50 ms |
| Simple live system-information retrieval | Under 100 ms |
| Typical Sam resident memory | Under 250 MB |
| Idle memory without a resident embedding encoder | Under 100 MB |
| Idle CPU | Near zero |

Design considerations that follow from the budget:

- precompute registry embeddings;
- keep the hot registry small and memory-resident;
- load large indexes on demand and use disk-backed or memory-mapped structures where useful;
- make embedding inference optional and lazily loaded;
- avoid background work when idle;
- never load a language model;
- avoid heavyweight databases unless measurements justify them;
- cap the query cache;
- index only what a domain needs, and separate metadata from full-content indexing.

The 4 GB target is a compatibility goal, not an allowance to consume all available memory.
If the optional encoder is unavailable or over budget, Sam degrades to lexical retrieval and
reports a safe no-match where confidence is insufficient.

See [docs/performance-targets.md](docs/performance-targets.md).

## Interface with Ida and Essential OS

Sam is a peer OS service with a small typed local API. It accepts a query, an optional search
scope, caller identity and capabilities, and bounded context needed for disambiguation. It
returns typed results or a typed handoff.

Illustrative outcomes:

```text
ok                  → structured results with source and freshness
out_of_domain       → outside the local-computer knowledge boundary
no_confident_match  → no supported intent cleared the confidence threshold
unsupported_operation → recognised but not allowed
source_unavailable  → required local source cannot be read
```

Clients are the Essential OS global search experience and Ida. Ida may use Sam for local
lookup, entity resolution and system information, then decide what to say or do. When a query
needs the web or a language model, the **caller** owns that escalation, subject to the caller's
privacy rules. Sam has no provider policy, model selector or network egress.

Sharing a file, application or device catalogue is allowed where it avoids duplicate indexes.
Sharing an index does not mean sharing registries, memory, conversation state or decision
policies. Sam's search registry and Ida's action registry remain separate.

See [docs/integration-with-ida.md](docs/integration-with-ida.md).

## Privacy and security

- Sam is read-oriented and has no network egress.
- It queries only approved local indexes and OS interfaces.
- It never receives privileged write access merely because it supplies the search UI.
- Indexing respects filesystem permissions, allowed roots, denied paths, symlinks and
  application privacy boundaries.
- Secrets such as tokens, private keys, password stores and credential databases are never
  indexed or cached.
- Indexed document text is data, not instructions. Sam does not follow instructions found in
  files.
- Result actions are constrained identifiers, never shell text or callbacks.
- Operational logs contain handler timing, errors and index health, not transcripts or
  per-user search history by default.
- Content returned by Sam stays local unless the user explicitly authorises a separate
  handoff by Ida or the OS.

See [docs/security-and-privacy.md](docs/security-and-privacy.md).

## Current development status

**Fedora prototype.** What exists today:

- `crates/sam-core` — deterministic registry, grammar, ranking, policy and typed results;
- `crates/sam-linux` — Fedora/GNOME read-only adapters and fixed actions;
- `crates/sam-voice` — local, offline push-to-talk speech input;
- `crates/sam` — GTK4/libadwaita panel, CLI, setup and GApplication service;
- a hand-authored registry covering apps, files, folders, settings, live system state,
  hardware, networking and local documentation;
- unit tests, live-source checks, a benchmark command and an RPM build.

This machine now runs Sam instead of Ida. The migration is recorded in
[docs/migration-from-ida.md](docs/migration-from-ida.md). Ida's repository remains a
separate project.

What does **not** exist yet:

- the final Essential OS shell integration;
- the build-time registry authoring pipeline and frozen evaluation corpus;
- semantic embeddings and a query encoder;
- a versioned local API schema and capability checks (a small JSON-lines Unix
  socket exists);
- full-content documentation indexing;
- measured 4 GB release gates and a packaged release beyond this prototype.

The measured numbers in [docs/fedora-prototype.md](docs/fedora-prototype.md) are from the
development laptop, not a minimum reference machine.

## Planned milestones

The intended implementation sequence is:

| Phase | Work | Exit gate |
|---|---|---|
| **S0 — Scope and measure** | Inventory local domains, data sources and OS interfaces. Benchmark process overhead, the 4 GB environment, indexing and cold/warm latency. Decide whether a semantic encoder earns its cost. | **Partial:** Fedora sources inventoried and measured; 4 GB validation still pending |
| **S1 — Deterministic core** | Query normalisation, domain registry, lexical matching, confidence thresholds, typed results and handoffs. | **Implemented** for the Fedora prototype; unit tests pass; a frozen evaluation corpus is still to come |
| **S2 — Registry authoring** | Build-time model drafts and expands registry data. Human review, deduplication, handler validation and frozen evaluation sets. | Registry is versioned, reproducible, reviewable; untested entries cannot ship |
| **S3 — Semantic retrieval** | Precomputed embeddings, compact query encoder loaded on demand, hybrid ranking. | Paraphrase recall improves enough to justify latency and RAM; lexical-only mode remains usable |
| **S4 — Sources and handlers** | File, application, settings, hardware, system, media, networking and documentation sources with bounded incremental indexing. | **Partial:** Fedora sources and handlers implemented; documentation is name-level only |
| **S5 — Search UI** | Integration with Essential OS global search, result templates, actions, disambiguation and handoff UX. | **Partial:** Fedora GTK4 panel and web handoff implemented; Essential OS integration pending |
| **S6 — Ida integration** | Typed local API; route Ida's local-information lookups to Sam. | No duplicate indexes; Ida uses Sam without granting it action authority or model routing |
| **S7 — Low-resource validation** | 4 GB systems, offline, index load, encoder cold start, failure injection, cache churn. | Memory, latency, accuracy, offline, privacy and no-memory gates pass |

These phases are planning intent, not commitments. See [ROADMAP.md](ROADMAP.md).

## Building the prototype

Fedora 45 with GTK 4.24, libadwaita 1.10, Rust >= 1.92 and the speech build
dependencies:

```sh
sudo dnf install gtk4-devel libadwaita-devel alsa-lib-devel openssl-devel
cargo test --workspace
cargo build --release
sam voice setup              # once: downloads the local speech model
sam setup                    # Super+Space and the user service
```

The RPM path is in [`docs/fedora-prototype.md`](docs/fedora-prototype.md).

## Contributing

Sam now has a running Fedora prototype. Useful contributions are registry coverage,
evaluation cases, source adapters, documentation fixes and measured performance reports.
New capabilities still enter through a registry entry, a handler and tests.

The rules that will govern code contributions:

- no generative model in the runtime;
- no conversational memory or personal profiling;
- no network egress from the Sam process;
- new capabilities enter through a registry entry, a handler and tests;
- handlers never build shell commands from user input;
- performance and memory claims must be measured, not asserted.

See [CONTRIBUTING.md](CONTRIBUTING.md).

## Licence

**MIT.** See [LICENSE](LICENSE).

Sam's own code and documentation are MIT-licensed. Dependencies and third-party
components keep their own licences. `essential-os` and `ida` are separately
MIT-licensed projects.

## Related

- Fedora prototype: [docs/fedora-prototype.md](docs/fedora-prototype.md)
- Migration from Ida: [docs/migration-from-ida.md](docs/migration-from-ida.md)
- Essential OS project record: <https://github.com/luishowin/essential-os>
- Ida, the personal assistant: <https://github.com/luishowin/ida>
