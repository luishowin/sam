# Security and privacy

**Status:** Fedora prototype · **Scope:** Sam's trust boundary and data handling

Sam is a read-oriented local service. Its security posture is simple to state: it can read
only what it is given permission to read, it has no network egress, and it cannot change the
system.

---

## Principles

1. **Read-oriented.** Sam retrieves and presents. Its Fedora panel performs only the fixed,
   registry-approved actions listed in [`fedora-prototype.md`](fedora-prototype.md); the
   search engine itself returns inert proposals.
2. **No egress.** The Sam process makes no outbound network connections, including to local
   model endpoints.
3. **Least access.** Only approved roots, sources and interfaces are readable.
4. **Data, not instructions.** Indexed documents and help pages are untrusted data. Sam does
   not follow instructions found inside them.
5. **Constrained actions.** A result action is an identifier, not a command, callback or shell
   string.
6. **No personal memory.** Sam stores no conversation, profile or long-term query history.

## Permissions and indexing

- Index only permitted roots and registered sources.
- Respect filesystem permissions, symbolic links, deny lists and application privacy
  boundaries.
- Resolve paths before checking them, and check again at access time where the platform
  allows.
- Separate metadata indexing from full-content indexing. Content indexing is optional,
  explicitly scoped and resource-budgeted.
- Never index secrets or credential material: tokens, private keys, password stores, keyrings
  and credential databases.
- Indexing must not create a route around the operating system's access controls.

## Prompt injection

Because Sam does not use a generative model, a document cannot redirect its behaviour through
persuasive text. Even so:

- file contents and documentation are treated as data;
- handlers never evaluate text as code or as a query language command;
- any text inserted into a response template is escaped and bounded;
- a result never claims more than its source supports.

## Speech input

- Speech is an input method, not a generative path. Moonshine Streaming transcribes audio to
  text; the text enters the same registry pipeline as typed text.
- Audio is held in memory for the utterance and discarded. It is not written to disk unless
  the user asks for a diagnostic capture.
- The model is loaded from `~/.local/share/sam/models`; nothing is fetched at runtime.
- `sam voice setup` is the only speech path that uses the network. It downloads a pinned,
  hash-verified archive once.

## Fixed actions

The Fedora panel is the caller for its own result actions. Every action is validated against
the registry, decided by the policy table and run by one fixed handler. User text never
becomes a shell command. The risky actions (suspend, log out, restart, power off) require a
second confirmation.

## Cache sensitivity

- Cache keys include normalised query, scope, permission context and source version.
- Results are not shared across users, scopes or permission levels.
- Sensitive file contents, secrets and protected metadata do not enter the cache by default.
- Dynamic values use short TTLs or event-driven invalidation.
- The cache is disposable and can be cleared at any time.
- Query-frequency statistics must not become a permanent per-user history.

## Logging and retention

Default records:

- operational errors;
- handler timing and outcome;
- index health and freshness.

Not recorded by default:

- transcripts;
- per-user search history;
- file contents;
- secrets.

Optional diagnostics are local, minimised and explicitly enabled. Logs are retained for a
bounded period and can be cleared by the user.

## Updates and integrity

- Registry, handlers, embedding artifacts and index schema are versioned as one compatible
  bundle.
- Installed bundles are hash-checked at startup.
- Nothing is downloaded or replaced silently.
- Index schema migrations are reversible or rebuildable from approved local sources.

## Threat model summary

| Threat | Mitigation |
|---|---|
| Unauthorised file read | Approved roots, permission checks, deny lists |
| Secret leakage through search or cache | Never index or cache credential material |
| Local content sent externally | No network egress; handoff performed by caller under its own policy |
| Prompt injection through documents | No generative runtime; text treated as data; escaping |
| Arbitrary execution through a result action | Constrained action identifiers; caller revalidates |
| Cache leaking between scopes | Permission-aware keys; no cross-scope reuse |
| Unbounded resource use | Hard cache cap, lazy loading, graceful degradation |
| Silent registry change | Versioned bundle, hash checks, no silent updates |
