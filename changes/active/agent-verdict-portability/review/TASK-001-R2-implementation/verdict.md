# TASK-001 — FIX_REQUIRED

## Immutable subject and round

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Base: `911a8e278d829ab2722fc20ae881b67481dcf9f3`.
- Candidate: `c82414d38561cc3f7e17ac3f3ff52223378abde8` on `fix-download-flush`.
- Reviewed range: `git diff 911a8e278...c82414d38`, all 38 changed files.
- Approved authority: [SPEC-agent-verdict-portability revision 2](../../spec.md).
- Original task: [TASK-001](../../tasks/TASK-001-portable-verdict-and-repair.md).

This is the first full implementation review. The preserved
`../TASK-001-R1-authority/verdict.md` was intake-only BLOCKED and accepted no
implementation behavior. Its approval prerequisite is now satisfied. No prior
acceptance or finding closure carries forward; no convergence assessment is
applicable. The candidate and tracked working tree remained unchanged through
review. Only report artifacts were added; no implementation, commit or push.

## Independent coverage

The common starting map, authority inventory, selection rationale and claim
comparison are in [scope.md](scope.md). Reviewers independently expanded it.

| Fresh role | Coverage | Report / result |
|---|---|---|
| behavior-rev | Complete task, requirements, invariants, negative flows, ownership/reuse, isolation, maintainer concerns and all proof surfaces | [task-review-behavior.md](task-review-behavior.md), FAIL |
| repo-rev | All changed Rust, Python, TypeScript, MCP, generated/docs, fixtures, dependency boundaries and mandatory rustdoc | [standards-review.md](standards-review.md), FAIL |
| system-rev | Shared Agent request/repair loop, hooks, budgets, timeout/cancellation, session/journal and Vala failure propagation | [system-review.md](system-review.md), FAIL |
| ponytail-rev | Independent source, reachability, authority, materiality, correction burden, deduplication and coverage validation | [findings-validation.md](findings-validation.md), FIX_REQUIRED |

No additional invariant or maintainer specialist was selected: behavior review
covers per-call state and the extension of existing owners. No new auth,
tenancy, durable-data or shared-concurrency boundary required a domain reviewer.
Failure propagation warranted the separate system review. All selected reports
are complete. No follow-up discovery was necessary: overlapping schema and
refusal claims share proven causes rather than conflict; independent validation
resolved the refusal correction against approved authority. No validator-origin
new finding needed a separate source-validation pass.

## Reconciled acceptance

| Obligation | Implementation and proof | Result |
|---|---|---|
| REQ-001 / AC-001 portable generated schema and retained types/bounds | `Verdict::response_format` projects generated nodes before embedding data; runtime structure/negative/decode tests, independently rerun | PASS |
| REQ-002/003 / AC-002 authored data/local references; named non-strict and existing inline/native policy | Existing resource embedding and authoring owners retained; focused exact regressions independently rerun | PASS |
| REQ-004/005 / AC-003 exact schema, repair and budgets | Existing conversation/iteration/route/journal owners extended; four-encoding and budget tests pass, but validator can use stale schema and refusals can trigger repair | FAIL: FIND-TASK-001-1, -2 |
| REQ-006 / AC-004 / INV-002 reachable slots and isolation | Vala collects existing input/workspace sources before invocation, edits a per-call Prompt copy, leaves no-source schema unchanged; restriction and concurrent/Card tests pass | PASS |
| REQ-007 / AC-005 / INV-001/003 final typed/data/evidence guards and safe errors | Duplicate action validator removed; final `verdict_from` and hosted code guards retained; terminal diagnostics use schema paths/fixed text, with redaction tests and supplied hosted-guard proof | PASS |
| AC-006 real server/Postgres on four shipped surfaces | Rust, Python, TS and MCP recovery/exhaustion source coverage plus supplied focused journey results | PASS on supplied execution evidence |
| AC-007 generated-schema live compatibility and validated replies | Runtime generation and four request modes exist; handwritten assertion accepts invalid live replies | FAIL: FIND-TASK-001-3 |
| Mandatory changed-item rustdoc gate | New/widened helpers and panic contracts have documented source gaps | FAIL: FIND-TASK-001-4 |
| Non-goals and existing owners | No second runner, retry setting, author rewrite, schema artifact, external fetch, deployment/storage change or code-check repair; installed validator extends the Agent loop | PASS apart from explicit refusal exclusion above |

## Validated finding ledger

Only independently confirmed or revised findings are retained. Full source
traces, materiality, burden, correction decisions and closure proof are in the
[validated ledger](findings-validation.md#validated-ledger).

| ID | Discovery IDs / status | Classification and source | Required correction |
|---|---|---|---|
| FIND-TASK-001-1 | BEH-001, SYS-001 / CONFIRMED | INCORRECT; `skald-agent/src/loop_runtime.rs:378–383,496` | Validate against the outgoing request after binding/hooks, reusing the existing response-type owner and validator while its schema remains unchanged. |
| FIND-TASK-001-2 | BEH-002, SYS-002 / REVISED | VIOLATION; `skald-agent/src/loop_runtime.rs:489–516` | Explicit no-answer provider refusals terminate without a correction or additional call; preserve safe terminal/inconclusive behavior through the approved replacement code. |
| FIND-TASK-001-3 | BEH-003 / CONFIRMED | MISSING; Python `test_gateway_live_providers.py:103–113,425,478` | Validate each live reply against the exact runtime schema with the installed validator. |
| FIND-TASK-001-4 | REPO-001 / CONFIRMED | VIOLATION; complete changed Rust item inventory in validation report | Complete required rustdoc and panic contracts in the same remediation. |

The refusal exclusion is already approved. Using the accepted replacement
structured-invalid family with fixed diagnostics preserves the existing
terminal boundary; no new public refusal code, old-code restoration or
specification revision is required. Documentation must not claim that this
early refusal consumed the entire iteration budget.

## Verification and limits

[verification.md](verification.md) records this review's execution: 303 distinct
owner tests passed, five exact selectors also passed, and the range whitespace
check was clean. These tests do not cover the two retained runtime defects.
Behavior discovery and validation independently demonstrated that the unchanged
live assertion accepts non-array citations and a string page value.

The task records wider owner checks, 23 hosted guards, focused Postgres journeys
on all four surfaces, four live provider modes and repository/language checks.
Those wider runs are supplied evidence, not reruns here. Full journey suites
remain for change review. No failure was hidden or implementation changed to
clear a gate. This verdict does not accept the incomplete live schema proof.

## Verdict and next scope

**FIX_REQUIRED**. Route the single self-contained
[TASK-001-R1 remediation](TASK-001-R1-structured-output-boundaries.md) directly
to `$wyrd-implement`. It packages the four validated causes; it is not a new
design plan. The next review checks closure, the candidate-to-fix diff and
affected consumers, retaining this cumulative subject and unaffected acceptance
evidence. This review does not implement, merge, push or deploy.
