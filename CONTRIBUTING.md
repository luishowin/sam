# Contributing

Sam has a running Fedora prototype. The most useful contributions now are registry coverage,
source adapters, evaluation cases, documentation fixes and measured performance reports. New
capabilities still enter through a registry entry, a handler and tests.

---

## Ground rules

Every contribution must respect the product boundary:

- Sam is a search execution engine. It does not reason, generate open-ended content or hold a
  conversation.
- Models propose; the runtime decides. A local model may supply one inert call. The registry
  validates it, the policy table decides, and a fixed handler runs it. A model never executes,
  plans a sequence or approves itself.
- No free-text generation in the routing or execution path. The one exception is the labelled
  offline answer tier, which is not a command path.
- Sam stores no personal memory, profile or long-term query history.
- Sam has no network egress. Local model sidecars speak a Unix socket, not a network address.
- The Sam engine returns constrained action proposals. The Fedora panel is the caller and
  performs only the fixed actions the registry allows; it never builds a shell command from
  user text.
- New capabilities enter through a registry entry, a handler and tests.
- Handlers never build shell commands from user input.
- Investigation uses fixed read-only probes. A model may select a probe; it never supplies the
  program, argument vector or path.
- Window control goes through the registered GNOME Shell extension methods. There is no X11
  fallback and no arbitrary window scripting.
- Performance and memory claims are measured, not asserted.

## What is useful now

1. **Domain inventories.** A precise list of a local domain, its authoritative source and the
   queries users actually ask about it.
2. **Registry proposals.** Canonical intents, natural phrasings, synonyms, slots and negative
   examples — with a real source for each.
3. **Evaluation cases.** Clean, paraphrased, ambiguous, out-of-domain and adversarial queries.
4. **Design review.** Corrections to the architecture, retrieval or boundary documents.
5. **Prior art.** Systems that solve local desktop search well, with what is worth borrowing.

## What is not accepted

- A generated list of thousands of low-quality phrases.
- A handler that scrapes a source the user has not permitted.
- Unvalidated model output, or a model that can reach an executor without the registry and
  policy path.
- A model-written shell command, or a model asked to generate one.
- A probe that can write, delete, move, install, reach the network or read outside its path
  allowlist.
- A window method beyond the registered list, or an X11 tool used as a Wayland fallback.
- Any runtime call to a cloud provider or web service from the Sam process.
- Telemetry, profiling or personalisation.
- A vector database introduced without measurements.
- A licence assumption. Sam is MIT; do not introduce differently licensed code
  without discussion.

## Pull request expectations

For code changes:

- one capability per change where practical;
- registry entry plus handler plus tests;
- a schema change includes an updated golden set and a recorded `sam eval` result;
- a model change includes the frozen-set comparison and the safety result;
- no unrelated refactors in the same pull request;
- documentation updated with the code;
- no secrets, credentials, personal data or machine-specific paths;
- no dependency without a recorded reason.

## Documentation style

- State status honestly: design, prototype or implemented.
- Label targets as provisional until measured.
- Prefer short, literal descriptions over marketing language.
- Link to the relevant document instead of duplicating content.

## Licence

Sam is licensed under the MIT Licence; see [`LICENSE`](LICENSE). By
contributing, you agree that your contribution is licensed under the same terms.

Dependencies and third-party components keep their own licences. Do not
contribute code you cannot license under MIT.
