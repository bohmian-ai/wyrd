# TASK-004 R4 Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 13
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations:
  `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md`,
  `changes/active/skald-workflow-runtime/review/TASK-004-r2/TASK-004-R2-align-revision-and-source-contracts.md`,
  and
  `changes/active/skald-workflow-runtime/review/TASK-004-r3/TASK-004-R3-close-step-attempt-and-import-gaps.md`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5cde1b48aab0d70d8686ee8fb5f2978e26cd7f58`
- Scope: complete cumulative base-to-candidate range

The candidate remained unchanged through discovery, focused follow-up, and
independent validation. `.codegraph/` is absent, so reviewers used immutable
Git objects, repository search, and direct source inspection. The review ran no
build, test, Cargo, or mise command and relied on the diff and recorded
evidence, as directed.

## Independent review results

| Review | Result | Material outcome |
|---|---|---|
| Behavior | PASS | The cumulative caller-to-result paths and all mapped task obligations pass; no proposal. |
| Invariants | PASS | Producer-to-sink state, R3 attempt semantics, and prior finding closure pass; no proposal. |
| Repository standards | PASS | Applicable Rust, contract, generated-artifact, test, and architecture rules pass; no proposal. |
| Maintainer | PASS | Changed owners, methods, interfaces, tests, and documentation have no material maintainability finding. |
| System resilience | PASS | Preparation, execution, shutdown, restart, gateway, query-owner, and recovery paths otherwise pass. |
| Security/tenancy | PASS | Captured authority, fresh request authorization, audit, credential, and tenant boundaries pass. |
| Concurrency/lifecycle | FAIL | Proposed an earlier independently sampled query-tool deadline. |
| Query settlement | PASS | Query ownership, cancellation, settlement, pod loss, and follower stream-close release pass. |
| Provider contract | PASS | Revision 13 tagging, persistence, schemas, provider dispatch, and Vertex projection pass. |
| Focused follow-up | RESOLVED | Confirmed the two sampling points and the reachable post-tool-error continuation; narrowed the clock-origin wording. |
| Ponytail validation | COMPLETE | Revised and retained one finding, `FIND-TASK-004-21`; all other proposals were absent. |

## Reconciled acceptance matrix

| Obligation group | Result | Validated findings |
|---|---|---|
| Tracked preparation, scoped idempotency, admission, capacity, retention, and shutdown | PASS | Prior `FIND-TASK-004-1`, `-2`, `-9`, `-11`, and `-13` remain closed |
| Query opening, cancellation, settlement, owner loss, capacity recovery, and later serviceability | PASS | Prior `FIND-TASK-004-3`, `-4`, and `-10` remain closed |
| Tenant isolation, authorization, audit, credentials, and accepted authority independent of bearer lifetime | PASS | Prior `FIND-TASK-004-5`, `-8`, and `-14` remain closed |
| Built-in tool declarations, bounds, denial paths, and complete terminal handling | PASS | Prior `FIND-TASK-004-6` and `-7` remain closed |
| Gateway dialects, fallback, cancellation, provider tagging, persistence, and Vertex projection | PASS | Prior `FIND-TASK-004-12` and `-15` remain closed |
| Active Revision 13 authority, import rules, and grant-stream-close Oracle documentation | PASS | Prior `FIND-TASK-004-16`, `-17`, `-18`, and `-20` remain closed |
| Published `Running` reserves attempt one; interrupted published work stays `Cancelled` with timestamps | PASS | Prior `FIND-TASK-004-19` is closed |
| An unqualified `bifrost.query` consumes the prepared run's one absolute total deadline | **FAIL** | `FIND-TASK-004-21` |
| Rejected Oracle polling/refusal/release-ack machinery remains absent; no bespoke check, setting, option, compatibility path, or credential harness was added | PASS | None |

## Follow-up decision

A focused follow-up was required because the concurrency reviewer reported a
deadline-origin defect while the other implementation, system, and query
reviews passed that boundary. It traced the full path from server preparation
through Skald planning, tool invocation, Agent continuation, and terminal
selection.

The conflict is resolved. Tokio and standard-library instants share the normal
production monotonic origin, but the server samples `RunTools` before Agent
hydration and execution-plan construction, while `WorkflowExecutor` samples
the run deadline afterward. The existing forwarded-Oracle deadline journey
does not distinguish those boundaries because it leaves the post-tool provider
call pending until the later executor deadline.

## Validated finding ledger

The authoritative validation is in `findings-validation.md`.

- `FIND-TASK-004-21` — **REVISED / INCORRECT**: `RunTools` clips an
  unqualified query to an absolute deadline sampled before synchronous
  hydration and planning, while the prepared executor owns a later total
  deadline. After the earlier tool failure, the existing Agent loop may
  continue and produce `Succeeded`, `Failed`, or the later `TimedOut`, so one
  accepted run does not have one total-deadline boundary.

The smallest correction reuses the absolute deadline already owned by the
prepared Skald run and binds that exact value into the existing shared
`RunTools` before acceptance or execution. It adds no service, timer task,
channel protocol, retry, poll, setting, option, checker, dependency, or test
harness. No specification revision is required.

## Prior-finding closure

`FIND-TASK-004-1` through `FIND-TASK-004-20` remain closed. In particular,
R3 reserves attempt one before publishing `Running`, interrupted published
steps settle `Cancelled` with timestamps, and all four import corrections are
present. The fixed human decisions remain preserved: Oracle graph-drain
polling and supervisor idle refusal stay deleted; follower release is grant-
stream close with no leader acknowledgement; and foreign-tenant journeys do
not require a model step without fixture credentials.

## Verification limits

- No build, test, Cargo, or mise command was run during this review.
- Candidate-recorded results were treated as claims and checked against the
  current source and named assertions.
- Recorded evidence credibly closes prior findings and the other obligation
  groups, but no recorded assertion compares the deadline placed in
  `RunTools` with the deadline owned by `PreparedWorkflowRun`.
- The current forwarded-Oracle deadline branch can pass with two deadlines
  because its post-tool model call remains pending until the later boundary.
- Every required discovery, follow-up, and validation report is present; no
  reviewer, authority, source, caller trace, diff, or evidence record was
  unavailable.

## Verdict

**FIX_REQUIRED**

Remediation task: `TASK-004-R4-share-prepared-run-deadline.md`.
