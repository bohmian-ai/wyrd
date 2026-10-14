---
id: SPEC-mcp-investigation
revision: 1
status: draft
---

# Investigate a failing system through Wyrd's MCP

## Objective and user value

A service pages at 3 am. An on-call engineer points their coding agent (Claude
Code, Codex, an internal agent) at Wyrd's MCP and asks: what is going wrong,
since when, and why?

Answering that today means the agent:

- discovers Bifrost tables;
- learns the correlation keys (`card_uid`, `run_id`, `wyrd_request_id`);
- writes DataFusion SQL by hand; and
- walks lineage one hop at a time through `inbound_refs` and `outbound_refs`.

This uses a lot of tokens, is easy to get wrong, and silently misses evidence
the agent did not think to query.

Wyrd alone holds the version-locked identity spine (Card, Run, Observation)
and the lineage graph that joins verification, drift, gateway, trace, log,
deployment, and dispatch evidence across a service and its dependencies. This
change exposes that join as server-owned investigation capabilities. The agent
gets a correlated, bounded, citable account of system behavior instead of raw
primitives. The investigation's conclusion can then be kept as durable,
attributable evidence.

Secondary value: the same MCP teaches a developer's agent how to instrument a
codebase with Wyrd (register Cards, bind Verifiers, open Runs, emit OTel) in
Rust, Python, and TypeScript.

## Required behavior (draft)

### REQ-001 Lineage

Given version-locked root Cards, a direction (`upstream | downstream | both`),
and a bounded depth, return the reachable Cards and edges by exact `CardRef`.
This implements `ProvenanceQuery` from `architecture/wyrd-design.md` (Audit).
Cards the caller cannot read are reported as withheld, not silently omitted.

### REQ-002 Timeline

Given subject Card(s), a closed time window, and a lineage depth, return one
time-ordered, bounded event stream across the subject and its neighborhood.
The stream covers at least:

- Card version registrations;
- failed or degraded judgments and drift results;
- Operator dispatches;
- gateway error and latency excursions;
- audit-recorded policy denials and credential revocations; and
- error spans and logs.

Each event cites its source (table and row identity or result ID) and the
exact Card version it is attributed to.

### REQ-003 Window comparison

Given a subject, a baseline window, and an incident window, return per-signal
changes for the subject and its neighbors, ranked by magnitude. Signals cover
at least: error rate, latency percentiles, verification failure rate, drift,
token and cost usage, and top error codes. Each change carries the query or
evidence handle that reproduces it.

### REQ-004 Honest evidence

Every composed result states, per section:

- complete;
- empty (no matching evidence);
- withheld (caller lacks permission, naming the permission); or
- failed (with the structured Wyrd problem).

A composed tool never presents withheld or failed evidence as empty. Output is
bounded, and truncation is explicit.

### REQ-005 Record the investigation

An authorized caller records the investigation as an `Audit` Card with
purpose `Incident`. The Card carries the subject refs, the replayable query
and observation criteria, findings, and status. The investigator identity is
server-derived from the caller.

### REQ-006 Guided entry points

- MCP prompts provide an investigation runbook. Claude Code shows each prompt
  as a slash command (for example, investigate a service since a time).
- `wyrd.guide` gains instrumentation topics covering the Rust, Python, and
  TypeScript SDKs.

### REQ-007 Surface parity

Every investigation capability is a server-owned contract available over HTTP
and MCP. MCP is not the only path (`AGENTS.md` §2: headless, agent-first,
not UI- or MCP-only).

## Constraints and invariants

- INV-001 Tenant isolation on every path. Lineage traversal crosses spaces only
  where RBAC already permits, consistent with the Audit design's "no `space`
  filter" rule.
- INV-002 Each composed tool evaluates every underlying permission it relies
  on. Each evaluation is audited by the existing non-blocking audit path.
- INV-003 Read tools are always advertised; `REQ-005` writes require explicit
  scopes.
- INV-004 Results are bounded by explicit row, byte, and deadline ceilings,
  matching or reusing Oracle's query limits.
- Durable logic stays in the server. SDKs project the contracts.

## Non-goals

- A general graph database or query DSL. `ProvenanceQuery` stays parametric.
- Automated root-cause verdicts or LLM-generated conclusions from the server.
  Wyrd returns correlated evidence, and the agent or human concludes.
- Automatic Audit creation (the design keeps Audits on-demand).
- Alert routing or paging changes; Operators are unchanged.

## Expensive-to-reverse decisions to settle before approval

1. **Tool and contract shapes.** Names, request and response schemas, and
   HTTP routes for lineage, timeline, compare, and audit creation are public
   contracts.
2. **Neighborhood definition.** Should "related services" be only declared
   lineage (`card_edges`), or declared lineage plus edges observed in traces?
   Observed edges find undeclared dependencies but need a defined
   trace-to-Card mapping.
3. **Uncorrelated telemetry.** Rows without `card_uid` are invisible to
   Card-scoped views. Do we define a Service Card ↔ OTel `service.name`
   mapping, and is it declared on the Card or inferred?
4. **Audit spec drift.** The implemented `AuditSpec` has `subject_refs`,
   `policy_refs`, `evidence_refs`, and `details`. The design adds `purpose`,
   `query`, `lineage`, `observation_criteria`, `status`, `investigator`,
   `snapshot_at`, and `digest`. REQ-005 needs the design shape, which is a
   persisted-contract change.
5. **Timeline event model.** Is the event set closed (an enum) or open, and is
   the stream computed per request or materialized?
6. **SDK projection scope.** Do Rust, Python, and TypeScript SDKs project the
   investigation contracts in this change, or only HTTP and MCP first?

## Acceptance obligations (draft)

- AC-001 A seeded-incident journey: a Service Card version bump followed by
  verification failures and gateway errors in a dependency. Through MCP,
  lineage finds the dependency, timeline orders the bump before the failures,
  and compare ranks the dependency's error change first.
- AC-002 Negative journeys: an under-privileged caller gets `withheld`
  sections, not empty ones. An oversized window is refused or explicitly
  truncated. A foreign-tenant root is indistinguishable from a missing one.
- AC-003 The HTTP surface has journeys equivalent to the MCP ones, plus SDK
  journeys per the decision on item 6.
- AC-004 Format, lints, codegen, OpenAPI contract, tenant-isolation, and
  owning `mise` lanes pass.

## Revision history

- r1 (2026-10-14): draft from owner review of SPEC-mcp-compat. Not approved.

## Authority

- `architecture/wyrd-design.md`: Observation identity, Audit,
  `ProvenanceQuery`, `ObservationCriteria`
- `architecture/bifrost-design.md`: correlation columns, query limits
- `architecture/wyrd-doctrine.mdx`
- `AGENTS.md` §2, §9, §11
