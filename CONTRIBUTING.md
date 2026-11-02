# Contributing

Sam is at the design stage. The most useful contributions right now are architectural review,
domain inventories, registry coverage proposals and evaluation cases. Please do not open a pull
request with a speculative runtime implementation before the interfaces and registry schema are
settled.

---

## Ground rules

Every contribution must respect the product boundary:

- Sam is not an LLM and must never depend on one at runtime.
- Sam does not reason, generate open-ended content or hold a conversation.
- Sam stores no personal memory, profile or long-term query history.
- Sam has no network egress.
- Sam never performs state-changing OS actions.
- New capabilities enter through a registry entry, a handler and tests.
- Handlers never build shell commands from user input.
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
- Any runtime call to a language model, web service or cloud provider.
- Telemetry, profiling or personalisation.
- A vector database introduced without measurements.
- A licence assumption. Licensing is undecided.

## Pull request expectations

Once implementation begins:

- one capability per change where practical;
- registry entry plus handler plus tests;
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

Sam is licensed under the MIT Licence; see [`LICENSE`](../LICENSE). By
contributing, you agree that your contribution is licensed under the same terms.

Dependencies and third-party components keep their own licences. Do not
contribute code you cannot license under MIT.
