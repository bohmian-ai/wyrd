# TASK-003 r4 task-review verdict

**Verdict: FIX_REQUIRED**

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-verification-closeout-task3`
- Base: `7f79fb3417db651adedac194ada8908f0a0372d7`
- Candidate: `f6c841d57fb19517ddefe83c826b24085b853845` (HEAD at review close)
- Approved authority: `changes/active/verification-closeout/spec.md` revision 3; original task: `changes/active/verification-closeout/tasks/TASK-003-r4-canonical-support-desk-closeout.md` (task cites revision 2). The full 220-file cumulative diff was reviewed, including `740129647..f6c841d57`.

## Reconciled acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| Declared Service tables, optional activation, unbound writer activation | Card registration, table ensure, verification binding and observation paths; card, verification, and SDK journeys reported green | PASS |
| Application Run in continuous and direct results; direct Task result through the shared outbox with no durable run or Operator | Verification result builder, direct route, readback and payload permission tests; server journeys reported green | PASS |
| Paired gateway Run/Card UID correlation, authorized before dispatch, captured without a CardRef alias | Gateway ingress/invocation/capture, shared client and OpenAPI; focused refusal/capture tests and codegen reported green; security domain review PASS | PASS |
| Gateway-owned Agent and judge calls, telemetry and Run scope across Rust/Python/TypeScript | Gateway/verification paths and three SDK projections; language journeys reported green | PASS |
| Support-desk deploy verifies the exact Prompt deployment and gives actionable missing-model refusal | Three examples check model name only; same-name foreign-provider deployment can pass; FIND-TASK-003-1 | FAIL |
| Same public support-desk workflow and joined evidence in all three SDKs, with stated refusals and no test-only flush | Three checked-in examples and SDK journeys, reported green; the deployment refusal has the gap above | FAIL |
| One shared Scribe outbox, late peer routing, process poller and role heartbeats | Boot route and lifecycle source; Scribe/Forge/Oracle lanes 28/28, 22/22, 50/50 reported green; late-Scribe proof starts Scribe before staging, FIND-TASK-003-6 | FAIL |
| Oracle bounded shutdown drains its own queries and reservations without blocking sibling roles | Shared resource ledger and ordered Bifrost drain; mixed-role path can wait on Scribe/Forge bytes, FIND-TASK-003-2 | FAIL |
| Forge worker recovery reclaims prior attempts only after prior work stops | Worker supervisor, detached plan/heartbeat tasks, self-reclaim SQL; panic path can overlap prior work, FIND-TASK-003-7 | FAIL |
| Postgres role bootstrap uses container `psql`, fails on SQL error, and proves idempotency | Wrapper and contract checks use container client; role-idempotency rerun omits `ON_ERROR_STOP`, FIND-TASK-003-5 | FAIL |
| Active architecture, generated/public contracts, and mandatory Rust documentation agree | Codegen/fmt/lints reported green; current design still prescribes observation `card_ref`, FIND-TASK-003-4; two changed Rust items lack required rustdoc, FIND-TASK-003-3 | FAIL |
| Excluded tools, structured output, Workflow correlation, new MCP tools, table drop/evolution, UI, aliases, replacement sinks | No validated discovery finding of prohibited machinery in the cumulative diff | PASS |

## Independent review results

| Report | Result | Proposed claim disposition |
|---|---|---|
| `task-review-behavior.md` | FAIL | BEH-001 confirmed as FIND-TASK-003-1 |
| `task-review-invariants.md` | FAIL | INV-1 confirmed as FIND-TASK-003-2 |
| `standards-review.md` | FAIL | RS-1 and RS-2 confirmed as FIND-TASK-003-3 and -4 |
| `maintainer-review.md` | FAIL | MAINT-001 confirmed as FIND-TASK-003-5 |
| `system-review.md` | FAIL | SYS-001 confirmed as FIND-TASK-003-6 |
| `domain-review-security.md` | PASS | No material security finding |
| `domain-review-data.md` | FAIL | DATA-1 confirmed with a narrower consequence as FIND-TASK-003-7 |
| `findings-validation.md` | Complete | Independently validated all seven claims against source; no rejected proposals |

No focused follow-up was needed: the discovery claims did not materially conflict, and each unique reachable path was traced in its owning report. The structured Ponytail reviewer independently validated every proposed claim. This is TASK-003's first task-review round, so there are no prior `FIND-TASK-003-*` findings to close. Earlier TASK-001 findings concern a separate task and are not reassigned here.

## Validated finding ledger

| ID | Classification | Required correction boundary |
|---|---|---|
| FIND-TASK-003-1 | INCORRECT | Compare provider and model in each support-desk deploy check; name the missing exact deployment and configuration action. |
| FIND-TASK-003-2 | REGRESSION | Use Oracle-holder memory for Oracle shutdown completion/reporting, retaining shared capacity and both pre-enabled wakeups. |
| FIND-TASK-003-3 | VIOLATION | Add mandatory item rustdoc and fallible `main` error documentation. |
| FIND-TASK-003-4 | VIOLATION | Reconcile active observation-correlation design prose with authorized Card UID, preserving legitimate CardRef uses. |
| FIND-TASK-003-5 | VIOLATION | Make the role-idempotency `psql` rerun stop on SQL error. |
| FIND-TASK-003-6 | MISSING | Exercise an already staged outbox slice across a genuinely late Scribe registration. |
| FIND-TASK-003-7 | REGRESSION | Permit unexpired self-reclaim only after the prior Forge incarnation's physical work has stopped; preserve lease-expiry recovery. |

The full producer-to-consumer evidence, exact locations, validation dispositions, and closure proof are in `findings-validation.md`. One self-contained remediation task is `TASK-003-R1-canonical-closeout.md` in this directory. All seven corrections stay within the approved behavior.

## Verification limits

The task records `mise run -c gate` exiting 0 on 2026-10-09, Scribe 28/28, Forge 22/22, Oracle 50/50, and passing codegen, fmt, lints, and `git diff --check`. Reviewers inspected source and named tests but did not rerun those lanes. Green existing lanes do not exercise the cross-provider collision, mixed-role Oracle memory, panic-overlap Forge recovery, or staged-before-Scribe transition. The exact focused proof and narrow lanes belong to the remediation task; broad aggregate verification runs at change review.
