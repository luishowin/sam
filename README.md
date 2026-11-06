# Sam

**Sam is a lightweight, deterministic, semantic search and execution engine for Essential OS.**

Sam answers questions about the computer it runs on — files, applications, settings, hardware,
networking, running processes and the operating system itself — and performs a fixed set of
registered actions. Most requests are handled by a hand-authored registry of domains, intents,
retrieval rules and handlers. A small local tool-selection model (Needle) can route phrasing the
registry did not anticipate, and a compact local model can answer general questions when the
machine is offline. No model generates prose in the routing path, and no model executes anything:
the model proposes a typed call, and deterministic code validates, confirms and runs it.

> **Status: Fedora prototype.** Sam runs on one Fedora 45 / GNOME 51 workstation. The
> deterministic core, Fedora adapters, GTK4 panel, quick actions and local speech input
> exist and are tested. The model tiers, the Essential OS service, the full registry authoring
> pipeline and the low-resource release gates do not. See
> [docs/fedora-prototype.md](docs/fedora-prototype.md) and
> [docs/model-tiers.md](docs/model-tiers.md).

---

## Contents

- [What Sam is](#what-sam-is)
- [Why Sam exists](#why-sam-exists)
- [Sam and Ida](#sam-and-ida)
- [Knowledge boundary](#knowledge-boundary)
- [The retrieval pipeline](#the-retrieval-pipeline)
- [Model tiers](#model-tiers)
- [Window control](#window-control)
- [Investigation](#investigation)
- [Registry-driven architecture](#registry-driven-architecture)
- [Embeddings and lexical retrieval](#embeddings-and-lexical-retrieval)
- [Handlers and response templates](#handlers-and-response-templates)
- [Caching, not memory](#caching-not-memory)
- [Performance and RAM targets](#performance-and-ram-targets)
- [Evaluation](#evaluation)
- [Interface with Ida and Essential OS](#interface-with-ida-and-essential-os)
- [Privacy and security](#privacy-and-security)
- [Current development status](#current-development-status)
- [Planned milestones](#planned-milestones)
- [Contributing](#contributing)
- [Licence](#licence)

---

## What Sam is

Sam is the native search and execution component of Essential OS. Its entire knowledge domain
is the computer it runs on, and everything it returns must be grounded in a local index, a
registered handler or live operating-system data.

Sam is **not** an assistant and not a general-purpose intelligence. It does not:

- reason through open-ended problems;
- plan a sequence of actions or run an agent loop;
- maintain a conversation or any personal memory;
- choose a cloud provider or reach the network;
- expand its scope by guessing.

It recognises a supported intent, retrieves the matching registry entry, runs a fixed handler
and formats a structured result. When it cannot answer confidently from its domain, it returns a
typed handoff or a labelled local answer rather than an invented fact.

A useful mental model is a high-end scientific calculator: fast, reliable, domain-specific
and equipped with many precise operations. It is a multi-tool for the computer, not a
general-purpose intelligence. Sam should feel remarkably capable because of its coverage,
retrieval accuracy, speed and careful execution — not because it simulates reasoning.

The model tiers do not change that. Needle is a router, not a brain: it picks a registered
capability and fills its arguments, and deterministic code does the rest. The offline answer
model is optional and clearly labelled. `Ida` remains the assistant; Sam remains the calculator
that Ida can call. See [docs/model-tiers.md](docs/model-tiers.md).

## Why Sam exists

Operating-system search is usually either a narrow, literal file-finder or a generative
assistant bolted onto the desktop. The first misses natural phrasing; the second is slow,
non-deterministic, hard to audit and often sends private context off the machine.

Sam takes a third path. Intelligence is moved into deliberate engineering at build time:
a dense registry of canonical intents, phrasings, synonyms and evaluation cases, paired with
deterministic handlers and a hybrid retrieval layer. Everyday local search then feels
immediate and dependable without a general model in the loop. A small local router can cover
the phrasing the registry missed; it does not replace the registry.

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
| Product role | Personal assistant | OS search and execution engine |
| Primary purpose | Understand, assist and act | Retrieve, present and perform registered OS actions |
| LLM | Yes, through Tier 2 | No hosted model; a small local router and an offline answer tier |
| Reasoning | Yes, through Tier 2 | No |
| Voice | Voice-first | Primarily a search UI; integration as required |
| Conversational memory | May maintain controlled context | None |
| Cache | Where useful | Bounded performance cache |
| Domain | General assistance and computer interaction | The computer itself |
| Web search | Can request it | Opens a browser handoff; never fetches |
| Multi-step work | Yes, through Hermes | No |
| OS integration | Uses approved executors and tools | Uses registered local sources, handlers and fixed actions |

Sam is a specialised utility that Ida can call when it needs information about the computer.
Ida interprets the result, explains it conversationally or decides that further reasoning is
needed. Sam never calls Ida, Hermes or a cloud provider. It may call a local model sidecar to
route a request or answer an offline question; that stays inside Sam's boundary and never
becomes an escalation to Ida. Sam works independently as part of Essential OS without Ida
present.

The analogy is a calculator and a server room: Sam is fast, precise and narrow; Ida is general,
expensive and supervised. Neither pretends to be the other.

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

An out-of-scope question is not answered from unrelated local material. Sam offers a browser
handoff — "Search the web for …" — and the browser performs the request. When the machine is
offline, or the user asks for it, a compact local model may answer, labelled **"local model,
unverified."** See [docs/offline-and-web.md](docs/offline-and-web.md).

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
confident? ── no ──→ shortlist ≤5 capabilities → Needle → validate
  │ yes
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

Natural-language variation is handled first by the registry and the ranking layer, not by a
model and not by one script per sentence. For example, these should resolve to one capability:

- "What's eating my RAM?"
- "Why is memory usage so high?"
- "Which apps are consuming the most memory?"
- "Show me the biggest memory consumers."

So should these:

- "Find my latest screenshots."
- "Where are my recent screen captures?"
- "Show me the screenshots I took recently."

When the registry misses, a small local router may propose one call from a short candidate list.
It is a fallback, not the primary path.

## Model tiers

Sam defines five tiers: four inside its boundary and one owned by the caller.

| Tier | What runs | Job |
|---|---|---|
| 0 | Registry, live providers, lexical and fuzzy ranking | Answer the common case in milliseconds |
| 1 | Needle, a small local tool-calling model | Route unusual phrasing; emit one typed call or `[]` |
| 2 | Deterministic clarify / confirm / refuse | Ask once, or decline honestly |
| 3 | A compact 1–4B local model | Answer offline, propose a call for the ambiguous tail, or run up to three read-only probes |
| 4 | Cloud, owned by the caller | Escalation beyond the machine; not part of Sam |

Two rules hold every tier together:

1. **Models propose; the runtime decides.** A model output is an inert `Intent` until the
   registry validates it and the policy table approves it.
2. **No model executes or remembers.** Sam is one request and one result. The only sequence it
   may run is the bounded read-only probe loop in Tier 3: at most three probes, no mutations,
   then one summary.

Tier 4 (cloud) is not part of Sam. Escalation beyond the machine belongs to the caller, and the
browser handoff is the default for a general question. See
[docs/offline-and-web.md](docs/offline-and-web.md).

The early "no generative model in the runtime" rule has been replaced by the narrower invariant
above. Needle is a language model, but it is not a generative chat model: it emits a JSON call,
not prose. The ban that matters is on free-text generation in the routing path and on
unvalidated model output.

## Window control

On GNOME Wayland, only `gnome-shell` can move another application's windows. Sam ships a minimal
GNOME Shell extension that exposes a narrow D-Bus interface; the actions are ordinary registry
entries:

- minimise, maximise, restore, centre, move, activate and show-desktop;
- `window.close` is destructive and always confirms;
- a target resolves against app id, WM_CLASS and window title, and a target that matches several
  windows asks rather than guessing;
- if the extension is missing, the action reports `source_unavailable`; it never falls back to
  an X11 tool or a shell command.

The extension is installed and removed by `sam setup`. It is version-sensitive and documented
as such. See [docs/window-control.md](docs/window-control.md) and
[docs/gnome-limitations.md](docs/gnome-limitations.md).

## Investigation

Investigation answers questions whose answer lives in command output: "how long has my laptop
been on", "when did it last boot", "what failed at startup", "find documents that mention
essential os".

The model never writes a shell command. It selects a **probe** from a fixed registry, and the
registry owns the command:

- fixed argv, no shell, no pipes, no redirection;
- read-only programs only, with a path allowlist and a secret deny list;
- a hard timeout, an output cap and redaction before logging;
- template-first answers, so the model cannot invent a cause the output does not contain;
- at most three probes per question, then one summary.

Content search is a first-class tool: LocalSearch first, a bounded `rg`/`grep` scan over
allowlisted roots as a fallback. See [docs/investigation.md](docs/investigation.md).

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
- the exported tool schema the router reads;
- typed slots and entity resolvers;
- an authoritative local source;
- a version-controlled handler identifier;
- permissions and read scope;
- a response schema and template;
- fallback and escalation rules;
- evaluation cases;
- a cache policy.

A new capability is introduced through a registry entry, a handler and tests — not by retraining
a general model. A fine-tune may improve how the router reaches that capability, but the
capability itself must still exist in the registry, with a handler and tests.

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

The embedding encoder is not the Tier 1 router. The encoder ranks candidates; Needle selects a
call from a short list of already-ranked capabilities. Either can be absent: the architecture
must remain useful with no embedding model and no router, using deterministic fallback
retrieval.

A heavyweight vector database is not a default requirement. The intended starting point is a
flat vector index with domain partitions, measured before any heavier infrastructure is
introduced. The registry is compiled into runtime-friendly artifacts and validated in CI once
implementation begins.

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
small fixed set of quick actions (volume, media, screen brightness, radios, night light,
power profile, timers).
On Essential OS those route through the OS capability layer or Ida's executor. Sam never
builds a shell command from user text.

Results are structured records, not generated prose: title, concise deterministic
description, entity type, source, location or state, last-updated time, confidence band and
allowed actions. Short summaries use fixed templates. Volatile values carry a measurement
time or are refreshed on demand.

A model may choose *which* handler to propose, but it never chooses *how* the handler works.
The handler is fixed, reviewed and covered by tests; a model-sourced call and a grammar match
arrive at the same dispatcher.

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
| Tier 1 router, warm | Under 250 ms p95 |
| Tier 3 TTFT, warm | Under 1 s p95 |
| Tier 3 TTFT, cold | Under 4 s p95 |
| Typical Sam resident memory, Tier 0 | Under 250 MB |
| Idle memory, Tier 0 | Under 100 MB |
| Idle CPU | Near zero |

The model tiers add separate memory profiles, reported independently rather than folded into one
number:

| Profile | Provisional cost |
|---|---:|
| Tier 0 only | 15–100 MB |
| Tier 1 warm | 30–120 MB more |
| Tier 3 warm, 1.7B q4 | 1.1–1.3 GB more |
| Tier 3 warm, 4B q4 | 2.3–2.6 GB more |

A 4B model does not fit comfortably on 4 GB alongside GNOME. The 4 GB target therefore means
Tier 0 and Tier 1; Tier 3 is enabled on an 8 GB or larger machine. The hardware tier is a
configuration decision, not a hidden fallback.

Design considerations that follow from the budget:

- precompute registry embeddings;
- keep the hot registry small and memory-resident;
- load large indexes on demand and use disk-backed or memory-mapped structures where useful;
- make embedding inference optional and lazily loaded;
- avoid background work when idle;
- keep the router small and optional, and unload the answer model after idle;
- avoid heavyweight databases unless measurements justify them;
- cap the query cache;
- index only what a domain needs, and separate metadata from full-content indexing.

The 4 GB target is a compatibility goal, not an allowance to consume all available memory.
If the optional encoder or a model sidecar is unavailable or over budget, Sam degrades to a
lower tier and reports a safe no-match where confidence is insufficient.

See [docs/performance-targets.md](docs/performance-targets.md).

## Evaluation

A model, a fine-tune or a new tier is adopted only when it beats the current setup on a frozen
golden set and does not regress safety. The set covers canonical commands, paraphrases, STT
noise, missing arguments, negatives, follow-ups and destructive input; a case passes only when
the action **and** its arguments match.

`sam eval` runs the set, prints accuracy by category and latency p50/p95, and exits non-zero on
regression. The corpus is never used for training. See
[docs/evaluation.md](docs/evaluation.md) and [docs/fine-tuning.md](docs/fine-tuning.md).

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
needs the web or a cloud language model, the **caller** owns that escalation, subject to the
caller's privacy rules. Sam has no provider policy, cloud selector or network egress.

Sam may run a local router and an offline answer model, but those are local sidecars on a Unix
socket, not an escalation path. A local answer is returned as an `ok` result whose source class
is `local_model_unverified`; it never claims a source.

Sharing a file, application or device catalogue is allowed where it avoids duplicate indexes.
Sharing an index does not mean sharing registries, memory, conversation state or decision
policies. Sam's search registry and Ida's action registry remain separate.

See [docs/integration-with-ida.md](docs/integration-with-ida.md).

## Privacy and security

- Sam is read-oriented, performs only fixed registered actions, and has no network egress.
- It queries only approved local indexes and OS interfaces.
- It never receives privileged write access merely because it supplies the search UI.
- Indexing respects filesystem permissions, allowed roots, denied paths, symlinks and
  application privacy boundaries.
- Secrets such as tokens, private keys, password stores and credential databases are never
  indexed or cached.
- Indexed document text, tool schemas and model output are data, not instructions. Sam does not
  follow instructions found in them.
- A local model may propose a call; the registry validates it and the policy table decides.
  A model can never approve its own action.
- Result actions are constrained identifiers, never shell text or callbacks.
- Local model sidecars speak a Unix socket, not a network address.
- Operational logs contain handler timing, errors and index health, not transcripts, prompts or
  per-user search history by default.
- Content returned by Sam stays local unless the user explicitly authorises a separate
  handoff by Ida or the OS. The browser handoff is the user's action, performed by the browser.

See [docs/security-and-privacy.md](docs/security-and-privacy.md).

## Current development status

**Fedora prototype.** What exists today:

- `crates/sam-core` — deterministic registry, grammar, ranking, policy and typed results;
- `crates/sam-linux` — Fedora/GNOME read-only adapters and fixed actions;
- `crates/sam-voice` — local, offline push-to-talk speech input;
- `crates/sam` — GTK4/libadwaita panel, CLI, setup and GApplication service;
- a hand-authored registry covering apps, files, folders, settings, live system state,
  hardware, networking, brightness and display controls;
- push-to-talk voice (hold Tab or the microphone), immediate safe execution and a live timer card;
- a terminal configuration menu (`sam config`) and a Unix-socket JSON API;
- unit tests, live-source checks, a benchmark command and an RPM build.

This machine now runs Sam instead of Ida. The migration is recorded in
[docs/migration-from-ida.md](docs/migration-from-ida.md). Ida's repository remains a
separate project.

What does **not** exist yet:

- the final Essential OS shell integration;
- the build-time registry authoring pipeline and frozen evaluation corpus;
- semantic embeddings and a query encoder;
- the model tiers: tool-schema export, router seam, Needle sidecar, fine-tune and offline
  answer model;
- window control: the GNOME Shell extension, D-Bus client and `window.*` actions;
- investigation: the probe registry, probe executor, content search and the bounded Tier 3
  loop;
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
| **S3 — Semantic and model retrieval** | Precomputed embeddings, compact query encoder, hybrid ranking, then a Needle router for the tail. | Paraphrase recall improves enough to justify latency and RAM; the router beats the grammar alone on the frozen set; lexical-only mode remains usable |
| **S4 — Sources and handlers** | File, application, settings, hardware, system, media, networking and documentation sources with bounded incremental indexing. | **Partial:** Fedora sources and handlers implemented; documentation rows are not exposed in this build |
| **S5 — Search UI** | Integration with Essential OS global search, result templates, actions, disambiguation and handoff UX. | **Partial:** Fedora GTK4 panel and web handoff implemented; Essential OS integration pending |
| **S6 — Ida integration** | Typed local API; route Ida's local-information lookups to Sam. | No duplicate indexes; Ida uses Sam without granting it action authority or model routing |
| **S7 — Low-resource validation** | 4 GB systems, offline, index load, encoder cold start, model load/unload, failure injection, cache churn. | Memory, latency, accuracy, offline, privacy and no-memory gates pass |

The model work has its own track, described in [docs/model-tiers.md](docs/model-tiers.md):

| Phase | Work | Exit gate |
|---|---|---|
| **M0 — Harness** | Golden set, `sam eval`, stage timings, tool-schema export. | Per-category accuracy and latency on a frozen set; regression exits non-zero |
| **M1 — Needle router** | Sidecar, candidate retrieval, confidence threshold, `[]` contract. | Beats Tier 0 alone on paraphrase and STT categories without regressing negatives |
| **M2 — Fine-tune** | Schema-validated teacher data, local LoRA, artifact versioning. | Beats the base router on the frozen and real-use sets; safety unchanged |
| **M3 — Local answer** | 1–4B sidecar, offline path, trust labels, hardware tier. | Warm TTFT under 1 s, honest refusal, no unvalidated call |
| **M4 — Escalation** | Ambiguous-tail routing and recipe corpus. | Recovers most router failures; free-form shell stays display-only |

Two deterministic tracks can ship before the model work:

| Track | Work | Exit gate |
|---|---|---|
| **W0–W1 — Window control** | Shell extension, D-Bus backend, `window.*` actions. | The live check passes; missing extension degrades cleanly |
| **I0–I2 — Investigation** | Probe registry, templates, content search. | Adversarial probes refused; answers grounded in output |

Tier 3 investigation (I3) depends on the M0 harness.

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

With voice enabled, press Super+Space and hold **Tab** while speaking; release
to transcribe. `sam config` switches voice between eager, lazy and off.

## Contributing

Sam now has a running Fedora prototype. Useful contributions are registry coverage,
evaluation cases, source adapters, documentation fixes and measured performance reports.
New capabilities still enter through a registry entry, a handler and tests.

The rules that will govern code contributions:

- models propose; the registry validates, the policy table decides, a fixed handler runs;
- no free-text generation in the routing or execution path;
- no model executes, plans, remembers or reaches the network;
- no conversational memory or personal profiling;
- no network egress from the Sam process; local model sidecars use a Unix socket;
- new capabilities enter through a registry entry, a handler and tests;
- handlers never build shell commands from user input;
- model changes ship with a frozen-set comparison and the safety result;
- performance and memory claims must be measured, not asserted.

See [CONTRIBUTING.md](CONTRIBUTING.md).

## Licence

**MIT.** See [LICENSE](LICENSE).

Sam's own code and documentation are MIT-licensed. Dependencies and third-party
components keep their own licences. `essential-os` and `ida` are separately
MIT-licensed projects.

## Related

- Model tiers: [docs/model-tiers.md](docs/model-tiers.md)
- Window control: [docs/window-control.md](docs/window-control.md)
- Investigation: [docs/investigation.md](docs/investigation.md)
- Evaluation: [docs/evaluation.md](docs/evaluation.md)
- Fine-tuning: [docs/fine-tuning.md](docs/fine-tuning.md)
- Offline and web handoff: [docs/offline-and-web.md](docs/offline-and-web.md)
- Fedora prototype: [docs/fedora-prototype.md](docs/fedora-prototype.md)
- Migration from Ida: [docs/migration-from-ida.md](docs/migration-from-ida.md)
- GNOME limitations: [docs/gnome-limitations.md](docs/gnome-limitations.md)
- Essential OS project record: <https://github.com/luishowin/essential-os>
- Ida, the personal assistant: <https://github.com/luishowin/ida>
