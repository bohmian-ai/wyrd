# TASK-004 invariant review

## Review Findings

### Critical

None.

### Important

- **INVREV-001 — MISSING — accepted-job authority is implemented but absent from both governing architecture and security authority.** The approved specification makes the distinction explicit and mandatory before completion: a server-hosted accepted Workflow keeps its immutable captured execution authority after submission-token expiry or later grant changes, while every new create/replay, get, and cancel request authenticates and authorizes afresh (`changes/active/skald-workflow-runtime/spec.md:830-859`, `:1938-1950`). The candidate implements and journeys that behavior (`crates/wyrd/wyrd-server/src/components/workflow/host.rs:75-123`, `:131-203`, `:244-279`; `crates/wyrd/wyrd-server/src/components/auth/caller_extractor.rs:11-53`; `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:1209-1300`), but the complete diff does not change `architecture/wyrd-design.md` or `architecture/wyrd-security-posture.md`. The latter still describes ordinary token lifetime and per-request revalidation without defining the accepted-job exception (`architecture/wyrd-security-posture.md:180-183`). Consequently the durable security authority contradicts by omission a live privilege-lifetime boundary: an operator or later maintainer following the governing documents cannot tell that revoking a role or letting the submission token expire intentionally does not stop an already accepted run, nor that the captured context is bounded to that run and excludes bearer/refresh/API-key material. **Required correction:** update the existing `architecture/wyrd-design.md` and `architecture/wyrd-security-posture.md` authorities—no new file, check, option, or custom mechanism—to state the approved server-hosted accepted-job boundary, its run/deadline/terminal limits, the prohibition on credential retention or widening, the fresh-auth requirement for later HTTP requests, and continued owner-specific authorization/audit for gateway and tool calls. Closure proof is source review showing both established authorities contain the distinction and remain consistent with REQ-032A; the recorded `accepted_authority_outlives_submission_only` journey remains the behavioral proof.

### Suggestions

None. No optional refactor or nonstandard mechanism is required.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Recorded verification evidence | Result |
|---|---|---|---|
| REQ-057, REQ-059, AC-030, AC-031: no Workflow principal/root or obsolete loader/graph machinery | Server graph preparation is `PinnedWorkflowGraph` plus Skald hydration (`components/cards/resolve.rs:520-663`; `skald-workflow/src/bodies.rs`); no Workflow principal or client HTTP loader appears in the diff | Recorded `codegen:check`, client-tier check, shared/Wyrd lanes | PASS |
| REQ-014, REQ-015, INV-001: one validated Skald executor, server only hosts it | `Preparation::prepare` pins/hydrates and calls `Workflow::prepare`; `Preparation::run` accepts then calls `PreparedWorkflowRun::execute` (`host.rs:244-388`) | S1/S6/S7 plus Skald prepared-run unit evidence recorded | PASS |
| REQ-017, REQ-018, REQ-019, REQ-048: bounded ready work, peer drain, cancellation/deadline, no step task survives executor | Existing Skald `JoinSet` owner retains bounded concurrency and abort/join behavior; server cancellation is the accepted run token (`workflow.rs`; `host.rs:261-279`) | S6 and recorded Workflow/Oracle cancellation journeys | PASS |
| REQ-020–023, REQ-045: complete deterministic snapshots and terminal invariants | Whole-snapshot observer and terminal-only `finish` (`workflow.rs`; `runs.rs:575-613`); ledger owns deterministic step/result projection | S6/S7 and prepared-run transition test recorded | PASS |
| REQ-029, INV-005, INV-006, INV-023: exact active pinned graph, tenant isolation, bounded steps/edges/unique bodies | Tenant connection plus exact active UID/identity reads and pre-retention charging (`resolve.rs:548-710`; `host.rs:309-323`) | S1/S7; tenant-isolation check recorded | PASS |
| REQ-030, REQ-034A–C, INV-013, INV-018, INV-019: one process-local accepted run, atomic idempotency/reservation, bounded retention | `WorkflowRuns` owns key/run tables, short lock, active slots, tracked tasks, terminal timestamps and eviction (`runs.rs:62-220`, `:249-399`, `:458-613`) | S2/S6 recorded | PASS |
| REQ-032, REQ-033, AC-010: fresh create/get/cancel authorization and canonical transactional audit precede lookup or acceptance | Host calls `audit::authorize` before admission or run lookup (`host.rs:75-123`, `:131-203`) | S1/S3 and principals lanes recorded | PASS |
| REQ-032A, INV-022, AC-027: captured authority outlives submission only; no bearer retention or widening | `Caller` contains verified tenant/principal/request/delegation and no token (`caller_extractor.rs:11-53`); run stores its clone and later requests authorize afresh | `accepted_authority_outlives_submission_only` recorded PASS | **FAIL — INVREV-001: required governing authority update is missing** |
| REQ-034, REQ-036A, REQ-038, REQ-039, REQ-041–043, INV-009–012, INV-020: route ownership, exact dialect/request, live gateway governance, direct external binding | Server gateway adapter delegates to `GatewayInvocation::run(..., false, ...)`; external bindings are tenant-qualified and resolved only during preparation (`gateway/workflow.rs:34-131`; `host.rs:324-370`) | S3/S5 and gateway journey recorded | PASS |
| REQ-052, INV-008, AC-025: only declared built-in tools, per-call object authorization/audit | Suitability check rejects unknown/duplicate names; per-Agent resolver supplies only `bifrost.query` and `cards.get`; Cards exact read and query service remain owners (`host.rs:420-456`; `tools.rs:37-315`) | S4 and MCP query journeys recorded | PASS |
| Query ownership across abort: waiter drop signals, tracked owner retains response and settlement, terminalization waits for owner | Query runs on the run's `TaskTracker`; waiter drop guard cancels; `RunTools::drain` precedes `AcceptedRun::finish` (`tools.rs:42-107`, `:163-207`; `host.rs:277-279`; `query/collect.rs:216-256`) | S4/S6 plus forwarded cancel/deadline/pod-loss journey recorded | PASS |
| Query terminal integrity and original deadline | Shared collector rejects partial/protocol/ceiling failures and invokes `cancel_and_settle`; opening cancellation retains the same open (`query/collect.rs:305-460`; `oracle/lifecycle_controls.rs:33-140`, `:279-305`) | Collector units, MCP journeys, lifecycle-control unit, forwarded journey recorded | PASS |
| REQ-050, AC-019, AC-021, AC-022, AC-028: exact configuration, contract, replay/race/bounds | Exact defaults and validation (`config.rs:1660-1790`); public routes and status codes (`workflow/routes.rs:24-180`); atomic key/run transitions (`runs.rs`) | Config units, OpenAPI/codegen, S2/S6/S7 recorded | PASS |
| Shutdown ordering and one shared budget | Workflow admission closes, tasks cancel/drain first, then supervised/gateway/MCP/Bifrost owners consume the same deadline (`app/server.rs:744-830`; `runs.rs:388-399`) | S6 and recorded Bifrost journey | PASS |
| Non-goals: no durable queue/table/lease/recovery, client-owned tenancy, new MCP/language surface, arbitrary tool platform, bearer renewal, or second audit writer | Diff contains process-local maps/trackers and narrow existing-owner adapters only; no persistence migration or public tool registration surface | Diff/source review; boundary and codegen checks recorded | PASS |
| Human standing direction: do not require unsupported bespoke mechanism/check/file/setting/option | The sole correction uses the repository's existing architecture/security authority documents; no custom mechanism is requested. Candidate removal of bespoke graph-drain polling is not treated as a defect or required remediation. | Source review | PASS |

## Invariant trace summary

- **Producer to sink — authority:** the authenticated extractor produces a token-free `Caller`; create audits it and captures it in `Preparation`; gateway and tool adapters reuse its tenant, effective principal, permission snapshot, credential attribution, and delegation; tools mint only fresh request IDs; terminalization drops run dependencies. The code path satisfies the approved runtime invariant, but its durable architecture/security description is missing (INVREV-001).
- **Producer to sink — run state:** `admit` installs one scoped preparation and slot; `Reservation::accept` atomically publishes one queued run and idempotency mapping; the Skald observer replaces only nonterminal whole snapshots; tool owners drain; `AcceptedRun::finish` installs one terminal snapshot, timestamps retention, and releases capacity.
- **Producer to sink — query ownership:** the Agent waiter creates a child cancellation token and tracked owner before awaiting; waiter drop signals cancellation; the owner keeps the Oracle response through terminal-safe collection/settlement; the Workflow terminal is committed only after the owner tracker closes and empties.
- **Sibling consumers:** MCP and Workflow share `BoundedQuery`; scheduled queries share `RunningQueryControls::open_cancellable`; HTTP Cards and Workflow Cards tool share `get_card_by_ref_for`; public and in-process gateway paths converge at `GatewayInvocation`.

## Open Questions

None affecting the invariant verdict. The missing authority update is an explicit obligation, not an interpretation question.

## Verification Notes

- Review was strictly read-only for source and execution. No build, test, Cargo, or mise command was run.
- Evidence reviewed from the immutable candidate's TASK-004 implementation record includes all seven named server journeys, exact focused selectors, forwarded Oracle cancel/deadline/pod-loss coverage, MCP query journeys, config/collector/lifecycle unit tests, and the recorded Wyrd/shared/principals/gateway/Bifrost/codegen/boundary/format/lint results.
- Recorded runtime evidence is credible for the implemented authority behavior, but tests cannot satisfy the explicit requirement to update governing architecture and security authority.

## Overall result

**FAIL** — one bounded `MISSING` finding, INVREV-001.
