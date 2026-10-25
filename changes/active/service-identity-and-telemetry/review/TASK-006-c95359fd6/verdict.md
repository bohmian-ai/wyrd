# TASK-006 verdict: FIX_REQUIRED

Current scope after maintainer disposition: **FIND-TASK-006-3 only**. Findings 1 and 2 require no further work; see [Maintainer disposition](#maintainer-disposition). The original review results below are preserved as history.

## Immutable subject and scope

Repository: `/Users/stevenforrester/Documents/GitHub/schema-change-task-6`.
Base: `5fe833547c66916c8c4868afc8e07d985dcba7d7`.
Candidate: `c95359fd608fa65d4f0422820751490f61c99202`.
First review of `git diff 5fe833547...c95359fd6`, including all seven commits beginning with f456eb702. Approved authority: revision 16 of `../../spec.md`; original task: `../../tasks/TASK-006-verification-trace-evidence.md`. No prior TASK-006 verdict, remediation, or carried-forward evidence. Candidate and tracked source remained unchanged through review. Only this new report directory was written; no implementation, commit, push, or deployment occurred.

[Scope and navigation](scope.md) records authorities, changed owners/callers/consumers, reviewer selection and claim comparison. The four fresh discovery reviewers received the same map and independently expanded it. No CodeGraph directory exists.

## Independent review and coverage

| Reviewer/report | Scope | Result |
|---|---|---|
| [behavior-rev](task-review-behavior.md) | Complete task, realistic caller paths, non-goals, reuse/maintainability, diagnoses and all shipped surfaces | FAIL: execution-integrity proposals; no runtime finding |
| [invariant-rev](task-review-invariants.md) | Attempt collection sharing; accepted state, authority, retry budget and fenced publication | PASS |
| [system-rev](system-review.md) | Deployed worker lifecycle, failure propagation, timeout, restart, recovery and publication | PASS |
| [repo-rev](standards-review.md) | Cross-tier rules, Rust/Python/TS, helpers/fixtures, architecture, rustdoc and evidence gates | FAIL: execution proof and helper documentation |
| [fresh ponytail-rev](findings-validation.md) | Independent source/materiality/correction validation of every proposal and reconciled coverage | FIX_REQUIRED: three retained findings |

Separate maintainer/domain reviewers were not selected: ownership was extended rather than replaced; established query/security/SQL capability boundaries remain owned by their existing mechanisms, with preservation traced by behavior/invariant/system reviewers. No selected report is missing. No follow-up discovery was needed: proposals did not conflict on material source facts, and newly observed consumer failures passed unchanged in focused serial reruns. The fresh validator discovered no additional blocker requiring separate validation.

## Reconciled acceptance matrix

Detailed source and test evidence is in the discovery matrices and [independent validation](findings-validation.md#reconciled-acceptance-and-execution-coverage).

| Obligation | Implementation and proof | Result |
|---|---|---|
| Whole authorized exact-ID trace, individual identities, stable order, explicit projection and overflow refusal | CallerTraces statement/fetch; J1 and direct source tracing | PASS |
| One read per trace per attempt, shared concurrent/later checks, fresh retries/runs | Engine run-local TraceReads/OnceCell; U1 independently passed and invariant tracing | PASS |
| Only queued absent evidence retries; same identity/authority/inputs and bounded PostgreSQL backoff survive restart | Runner final-attempt mode and existing claim/retry owners; I1/I2 and invariant/system tracing | PASS |
| No intermediate result; exhausted absence judged under normal decide; fenced final publication | Existing settlement/aggregation owner; I1/I2 | PASS |
| Direct absence, negative errors, timeout/default limits and generic exhaustion preserved | Direct deferral false; exact NotYetLanded match; I3 and existing deadline/exhaustion tests | PASS |
| Immediate nonempty judgment, immutable completed result and fresh late-arrival execution | Existing completion lifecycle; J2 | PASS |
| Tenant/query authorization, audit and binding/source attribution preserved | Existing accepted-caller query owner; J1/delegated-history/I1 | PASS |
| Rust/HTTP, Python, TS, MCP public proof; shared fixtures safe | J/P/T/M source and recorded results; independent Rust consumers all passed across suite and serial rerun | PASS |
| Reuse, non-goals, architecture and tier/generated-contract rules | Actual full diff and existing-owner comparison; no new durable/wire contract | PASS |
| Observed new-behavior RED proof | Explicitly absent from original task evidence | FAIL: FIND-TASK-006-1 |
| Required independent diagnosis record | Four self-diagnoses now independently source-validated, but recovery record absent and historical ordering missed | FAIL: FIND-TASK-006-2 |
| Mandatory changed-item panic documentation | Three new helper wrappers omit inherited fixture panic invariants | FAIL: FIND-TASK-006-3 |

## Validated ledger and correction

Only the independently confirmed/revised findings below are retained. Their full obligation, locations, reachable paths, materiality, correction burden and closure proof are in [findings-validation.md](findings-validation.md#final-deduplicated-ledger).

- **FIND-TASK-006-1 — CONFIRMED, VIOLATION** (B-006-01/REPO-01): recover focused observed counterfactual RED and candidate GREEN for the new U1/J1/I1/I2 behavior. Preserve already-correct I3/J2 regression evidence and public-surface traceability. Retain the truthful original production-before-tests deviation.
- **FIND-TASK-006-2 — REVISED, VIOLATION** (B-006-02): link the existing fresh behavior diagnosis assessment and validation beside the four diagnoses. State that assessments are retrospective and original traces unavailable. A duplicate diagnostician, reverted assertions, or production guards are unnecessary.
- **FIND-TASK-006-3 — CONFIRMED, VIOLATION** (REPO-02): add truthful `# Panics` sections to trace_batch, write_spans and shared_trace_verifier, preserving existing fixture invariants.

One self-contained [TASK-006-R1 remediation](TASK-006-R1-evidence-and-helper-contracts.md) packages all three for `$wyrd-implement`. No product, architecture, public API, security, concurrency or persistent-data decision is required. No runtime defect or material DRIFT was established. This review does not waive explicit execution or rustdoc gates merely because runtime source appears correct.

## Verification and limits

[Independent verification](verification.md) records exact selectors and counts: U1 1/1; engine module 24/24; exact deferral unit 1/1; server build and diff check passed. The previously omitted Rust building-block consumers selected 23: 21 passed initially, and both failed code-check tests passed unchanged in a focused serial 2/2 rerun. TESTING.md requires serialization for shared telemetry; no full serial suite rerun is claimed and no source change cleared those failures. Both initial failures omit optional trace input and do not reach changed trace fetching/deferral.

Other owner/journey/lint/type results remain implementation-recorded evidence, independently checked against source/test structure rather than rerun here. The original negative observations and pre-fix diagnostician order were not supplied; later recovery must not claim they occurred historically. No prior finding closure or convergence assessment applies to this first review. Remediation review should check these three closures, its incremental diff and affected paths, carrying forward unaffected source-backed acceptance.

**Verdict: FIX_REQUIRED.**

## Maintainer disposition

After review, the maintainer explicitly waived FIND-TASK-006-1 and accepted the existing retrospective independent source validation for FIND-TASK-006-2, directing that the remediation cover only FIND-TASK-006-3. Findings 1 and 2 no longer block acceptance and require no further evidence or record work. This disposition does not claim that historical RED or pre-fix independent diagnosis occurred; the original reports remain unchanged as the review record.

The current verdict remains **FIX_REQUIRED solely for FIND-TASK-006-3**. The revised [TASK-006-R1 remediation](TASK-006-R1-evidence-and-helper-contracts.md) requires only three truthful helper `# Panics` sections and static/format verification. Its closure review should inspect that documentation diff and carry forward the accepted runtime evidence; counterfactual execution and repeat diagnosis are excluded.

## Maintainer approval

After TASK-006-R1 added the three helper `# Panics` sections (verified by
`mise run fmt` and `git diff --check`), the maintainer approved TASK-006
directly, closing FIND-TASK-006-3 without a separate closure review.
