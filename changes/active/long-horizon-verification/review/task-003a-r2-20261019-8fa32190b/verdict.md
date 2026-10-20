# TASK-003a / R2 cumulative re-review verdict

**FIX_REQUIRED — FIND-TASK-003a-9 only; documentation and generated-reference parity.**

All three R2 implementation findings are closed. No runtime defect or additional
safeguard survived independent validation. The real-query DST test is accepted;
a literal historical date is not required.

## Immutable subject

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Branch: `change/long-horizon-verification-task-001`.
- Candidate: `8fa32190b26b11588721e8db44b3eb94139e3739`.
- Cumulative base: `22335c2c40a5ad24c27bfa0d4686f4da49f922c5`.
- Latest fix locator: `8376bb2aa71be18b9c7269cd9f2a601509137dc7..8fa32190b26b11588721e8db44b3eb94139e3739`.
- Complete cumulative diff: 89 files, +5579/-2481 including prior packet reports; latest fix: 4 files, +135/-31.
- Approved spec: `changes/active/long-horizon-verification/spec.md`, revision 20.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-003a-simplify-grouped-verification.md`.
- Prior R1/R2 and report identities: [subject.md](subject.md).

Source stayed immutable at the candidate. Only this new report directory was
written; prior untracked review/evidence artifacts were read, not changed. No
implementation, commit, merge, push or deployment occurred.

## Independent report completeness

| Fresh role | Report | Discovery result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | PASS, empty proposed ledger |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL, INV-R2-01 |
| Repository standards | [standards-review.md](standards-review.md) | FAIL, STANDARDS-1 |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL, MREV-R2-01 |
| System resilience | [system-review.md](system-review.md) | PASS, empty proposed ledger |
| Persistent data/concurrency | [domain-review-data.md](domain-review-data.md) | PASS, empty proposed ledger |
| Security/RBAC/tenancy | [domain-review-security.md](domain-review-security.md) | PASS, empty proposed ledger |
| Focused follow-up | [followup-review.md](followup-review.md) | RESOLVED, FU-R2-01 |
| Structured Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED, one consolidated FIND-9 |

All required roles were separate fresh agents. The orchestrator supplied a common
navigation map; reviewers expanded it independently. The orchestrator did not
replace independent validation with its own findings.

## Claim reconciliation and follow-up

[claims-comparison.md](claims-comparison.md) groups claims by invariant. The
behavior report's A9/A10 documentation PASS conflicted with precise current
prose/generated-output omissions found by other reviewers. A fresh focused
follow-up traced the new outcome and its consumed descriptions, resolving this
conflict against revision 20 and original A9. A separate fresh validator inspected
all reports and actual source and consolidated that one documentation boundary.
Behavior/runtime PASS evidence remains credible; its broad docs PASS is narrowed.

No fixed-date replacement, migration rebuild framework or separate security-
posture wording blocker was retained. The latter descriptive note has no proven
runtime security consequence. This review requires no new permissions mechanism,
publication recovery protocol or persisted details contract.

## Reconciled acceptance matrix

Detailed requirement/source/proof rows are preserved in both implementation
reports and domain/standards reports. Final reconciliation:

| Obligation | Implementation and proof | Result |
|---|---|---|
| A1/A2/A3; group propagation, exact sources, stable lists, immutable/idempotent snapshots and new evidence | Producer/GroupPoller/queue source traces; existing first-class journeys inspected; fresh P1/P2 | PASS |
| A4; settlement-order verdict change, baseline/equal/replay behavior | Existing queue lock/fence/dispatch owner; fresh P2 | PASS |
| A5/A6; fan-out/media/coding without retired source ceiling | Existing engine/SDK/MCP source and fixtures; historical journey evidence | PASS |
| A7; tenant/owner/source isolation, auth and no request SQL | Security/data/behavior reports; fresh tenant-isolation check and P1 | PASS |
| A8; scheduled samples and bounded outbox/admission remain existing owners | Cumulative source/pressure journey coverage; fresh P1 actual cron/capacity path | PASS |
| A11; ACK→pending→bounded poll→atomic handoff, rollback/capacity and no inactive rediscovery | Existing outbox/scheduler/queue; fresh P1/P2 | PASS |
| Revision19 authority distinction and earlier findings1/8 | Narrow internal snapshot reads, owner execution authority, all query errors retryable | PASS; #1 closed, #8 superseded |
| Revision20 incomplete snapshots | Check before pinning, PG >5-minute deadline, terminal refusal/consume; fresh incomplete scheduler test | PASS runtime |
| R2 FIND-2 | Epoch seconds-only SQL; real-query DST test and extra year/leap probes | PASS |
| R2 FIND-6 | Accurate visitor rustdoc; no decode change; fmt/lints | PASS |
| R2 FIND-7 | Additive constraint migration; valid refusal and six rejected mutation categories in P2 | PASS |
| A9 and dependent A10 functional/documentation closure; current public error projection | Existing guide/architecture omit current exception; generated error reference lacks emitted stable code | FAIL, FIND-9 |
| Non-goals and accepted tradeoffs | Existing owners reused; no new recovery/queue/clock/configuration/dependency framework; accepted loss/postponement/backlog/overlap retained | PASS |
| REQ-015/AC-009 performance qualification | Explicit subsequent benchmarking work | Pending, not claimed by this task review |

## Final validated finding and correction

**FIND-TASK-003a-9 — Published grouped behavior and generated error reference
omit the approved incomplete-snapshot outcome.** Discovery IDs INV-R2-01,
MREV-R2-01, STANDARDS-1 and FU-R2-01; consolidated REVISED validation.

The existing guide at how-verification-works.md:176–180 and grouped architecture
at wyrd-design.md:1324–1345 need the result-without-input retry/terminal exception.
The generated API error reference is missing
`WYRD_VERIFICATION_409_SNAPSHOT_INCOMPLETE`. Root and independent reviewers
reproduced the source-render mismatch read-only; this is an objective generated
reference gap, not a hypothetical runtime problem.

Update only those consumed explanations and regenerate existing documentation.
Keep legitimate null outcomes/selected values, IDs in the stored error message,
ordinary query retries, owner execution authority and all accepted tradeoffs.
No runtime code changes or additional tests/harnesses are selected.

Decision-complete handoff:
[TASK-003a-R3-align-grouped-verification-docs.md](TASK-003a-R3-align-grouped-verification-docs.md).
The full independently validated trace, minimality ladder and focused proof are
in [findings-validation.md](findings-validation.md).

## Verification and limits

[verification.md](verification.md) records fresh sequential managed-Postgres
migration tests (one each), P2 (1 selected/pass), P1 (2 selected/pass), plus
fmt/lints/tenant-isolation/diff checks passing. The DST fixture crossed the
investigated year/leap boundaries successfully. No source touches were needed
to embed the new migration; the candidate changes its owning Rust crate too.

The read-only error-reference comparison executed successfully but returned
`catalog_matches: False`, with exactly one missing generated row. This is not a
passing docs gate. The full docs task writes generated outputs before checking
drift and was not run during immutable review. No fresh full Rust/Python/TS/MCP
journeys, broad gate, docs build/a11y or benchmarks are claimed. The documentation
remediation requires its owning docs gate; integrated change review owns broader
qualification.

R2 is implementation-correct, but the cumulative task's existing documentation
acceptance obligation remains open. Route only FIND-9 to `$wyrd-implement`; do
not reopen closed findings or accepted architecture decisions.
