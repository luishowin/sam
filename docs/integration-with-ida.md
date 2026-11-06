# Integration with Ida

**Status:** Design; Fedora prototype uses the panel as caller · **Scope:** The boundary between Sam and Ida

Sam and Ida are separate products that cooperate through a typed interface. This document
defines that boundary. It does not describe an implemented API.

---

## Summary

- **Sam** is the native Essential OS search and execution service. It answers questions about
  the local computer from registered sources and performs a fixed set of registered actions.
- **Ida** is the personal assistant. It interprets requests, converses, plans, performs
  approved OS actions and decides when to escalate.
- Sam is **not** a tier of Ida, an Ida backend or an Ida subsystem.
- Ida may call Sam. Sam never calls Ida, Hermes or a cloud provider. Sam may call a local model
  sidecar to route a request or answer an offline question; that is a local capability, not an
  escalation to Ida.
- Both may share low-level OS data services, but their registries and responsibilities remain
  separate.

A useful shorthand: Sam is a scientific calculator, Ida is a server room. The calculator is
fast, precise and narrow; the server is general, expensive and supervised. Neither pretends to
be the other.

## Ownership by function

| Function | Owner |
|---|---|
| Find a file, application, setting, local document or device | Sam retrieves and presents; Ida may request the same lookup |
| Read live system state (RAM, disk, battery, network) | Sam returns structured data from approved sources |
| Open a found file, launch an app, navigate to a setting | Sam may propose a constrained result action; the caller performs it |
| Change volume, create a timer, toggle radios, set focus, adjust night light | Sam performs the fixed registered action |
| Minimise, maximise, move, centre, activate or close a window | Sam performs the fixed registered action; close always confirms |
| Investigate uptime, boot history, battery or failed services | Sam runs fixed read-only probes and returns a grounded answer |
| Suspend, log out, restart, power off | Sam proposes; the action always confirms; the OS or caller may perform it |
| Move or delete files | Not a Sam action in v1; Ida interprets and authorises |
| Route unusual phrasing to a registered local capability | Sam's local router (Tier 1), validated by Sam's registry |
| Answer a general question with no local source | Browser handoff by default; labelled local answer when offline; Ida or the caller may do better |
| Explain a concept, write, plan or reason | Ida Tier 2, or the OS caller's chosen external handler |
| Current external information or research | Web search chosen by the caller; Sam never browses |
| Multi-step agent work | Hermes, under Ida |

Ownership is decided by function, not by keyword. A file lookup that is part of an Ida command
workflow may remain an Ida action; the same lookup requested as a search is Sam's job. Ida must
not duplicate Sam's registry, and Sam must not absorb multi-step work or file-destroying
actions. The state-changing actions Sam does own are fixed, registered and confirmed by policy.

## Typed interface (intended)

### Request

```json
{
  "query": "why is my laptop getting hot",
  "scope": ["hardware", "system.live_state", "applications"],
  "caller": "essential_os_search",
  "capabilities": ["read_system_metrics", "read_process_metadata"],
  "locale": "en"
}
```

### Successful response

```json
{
  "status": "ok",
  "intent": "system.thermal_diagnostic_snapshot",
  "source_freshness": "live",
  "generated_at": "2026-11-03T00:00:00+03:00",
  "results": [
    {"type": "metric", "label": "CPU temperature", "value": 87, "unit": "°C"},
    {"type": "process", "label": "Top CPU consumer", "value": "example-process", "cpu_percent": 42.1}
  ],
  "actions": [
    {"id": "open_system_monitor", "label": "Open System Monitor"}
  ],
  "cache": {"eligible": false, "reason": "live_thermal_data"}
}
```

### Handoff response

```json
{
  "status": "handoff",
  "reason": "out_of_domain",
  "matched_domain": null,
  "message_key": "outside_local_computer_scope",
  "caller_action": "select_external_handler"
}
```

## Contract invariants

- Responses are structured and versioned; no generated prose is required. A local answer from
  Tier 3 is an exception, and it is labelled as unverified.
- Every result names its source or source class and says whether data is live, cached or
  indexed. A local model answer claims no source.
- A low-confidence match is not returned as a confident result.
- Sam never returns arbitrary shell commands or unrestricted executable text.
- The caller revalidates any result action before executing it.
- No local content is sent externally as part of the Sam contract. The browser handoff is the
  user's action, performed by the browser.
- Sam has no provider policy, cloud selector or network egress. Its local model tiers are
  sidecars on a Unix socket, and their output is validated like any other proposal.

## Escalation ownership

Sam returns one of the typed outcomes below. It never decides what happens next.

| Outcome | Meaning | Caller's next step |
|---|---|---|
| `ok` | Supported intent and result | Present it |
| `out_of_domain` | Outside the local-computer boundary | Browser handoff, Sam's labelled local answer, or another registered capability chosen by the caller |
| `no_confident_match` | No supported intent cleared the threshold | Clarify locally, or escalate according to caller policy |
| `unsupported_operation` | Recognised but not permitted | Explain, or route to an authorised action service |
| `source_unavailable` | Required local source cannot be read | Report source status; do not guess |

When the caller is Ida, Ida's router and privacy gate select the backend using its own reason
tags and data-class rules. When the caller is global search, Essential OS chooses among the
actions the user has enabled. A handoff must not silently transfer file contents, paths,
secrets or private metadata to an external provider.

Sam's own local tiers are not an escalation to Ida. Tier 1 proposes a typed call; Tier 3 answers
an offline question or proposes a call on the same schema. Both stay inside Sam's process
boundary and its no-egress rule.

## How Ida uses results

Ida remains responsible for the user-facing interaction. It can:

- speak or display Sam's results;
- combine them with conversation context;
- ask a clarification;
- explain what the result means;
- continue with a separate action through its executor.

Sam returns data, not personality and not a conversational response. Ida must not present
Sam's retrieval as her own reasoning, and Sam must not adopt Ida's voice.

## Shared infrastructure

Sharing is permitted where it prevents duplicate indexes:

- one file catalogue;
- one desktop-entry catalogue;
- one settings metadata source;
- one device inventory;
- one documentation index;
- one set of read-only system-information adapters.

Shared data does not imply shared registries, memory, conversation state, policies or
decision-making. Each product consumes the data service through a stable, versioned API.

## Failure independence

- Sam must work without Ida.
- Ida must work without Sam; if Sam is unavailable, Ida reports that local lookup is
  unavailable rather than inventing an answer or silently rebuilding a parallel index.
- A Sam registry update must not silently change Ida's action behaviour.

## Evaluation

Sam and Ida have independent evaluation criteria and roadmaps:

- Sam: retrieval and routing accuracy, out-of-domain handling, confident-wrong rate, latency,
  memory, offline operation, validated local models and absence of personal memory.
- Ida: intent and slot accuracy, escalation quality, action safety, privacy-gate compliance,
  latency and task success.

Integration tests cover the boundary: no duplicate indexes, correct handoff reasons, no action
authority leaking into Sam, and no network access from Sam. A local model sidecar is allowed;
an external model or cloud call is not.
