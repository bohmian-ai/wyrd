# TASK-005 r1 verdict

## Verdict

**SPEC_REVISION_REQUIRED**

The validated ledger retains four findings. `FIND-TASK-005-2` through
`FIND-TASK-005-4` are bounded implementation corrections, but
`FIND-TASK-005-1` cannot be corrected safely within the currently approved
public surface. Revision 14 requires `--server` to select the endpoint for an
authored local Workflow, while the approved one-argument
`wyrd_client::Workflow::from_path(path)` surface gives the separate CLI crate no
way to supply that endpoint to lazy external-Card hydration and the retained
public-gateway client. Removing the parser conflict alone would accept and
ignore the option; ambient environment mutation would introduce a prohibited
workaround. The specification must first approve the shared-client composition
boundary.

No remediation task is written for this review. The task-review skill prohibits
prescribing an implementation remediation when a retained finding requires a
new public API or architecture decision.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `cc252339d5add2deff5c5cab826be92954076db0`
- Candidate: `f2f4b87dc783df790b831b6c818c6273106d661b`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 14
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-005-cli-and-integrated-proof.md`
- Review mode: source and recorded evidence only; no build, test, formatter,
  linter, generator, server, or executable verification command was run

The candidate remained at the supplied commit throughout discovery and
validation. Revision 14 superseded the task's Revision 12 wording wherever
they conflicted.

## Reconciled acceptance matrix

| Obligation | Implementation and recorded proof | Reconciled result |
|---|---|---|
| REQ-026 and Scenario 1: one source, compatible execution/detach choices, and `--server` remains the endpoint selector | `RunArgs` and the compiled CLI contract cover source selection, but the parser forbids `--server` with `--file`, and invalid execution choices read `--input-file` before refusal | **FAIL** — `FIND-TASK-005-1`, `FIND-TASK-005-2` |
| REQ-027: portable output, accepted ID before polling, interruption without cancellation/resubmission, stable failures | Direct `WorkflowRun` rendering and the recorded lifecycle journey cover output and successful SIGINT behavior; the fallible signal listener's error is discarded and reported as SIGINT | **FAIL** — `FIND-TASK-005-4` |
| REQ-024, REQ-047, INV-016: one shared loader/runtime/transport owner | CLI local and server paths delegate to `wyrd_client::{Workflow, Workflows}`, Cards, and Skald; no duplicate executor, transport, graph, parser, or configuration catalog entered the diff | PASS, subject to the missing approved endpoint-composition surface in `FIND-TASK-005-1` |
| Scenario 2 and AC-001–003/013: checked-in bundle runs before and after registration; apply executes nothing; exact/UID loading and team reuse remain pinned | Compiled CLI journey and Rust journey source plus recorded focused results | PASS |
| Scenario 3 and AC-004/018/019: accepted run, detach, status, idempotent cancel, dropped/interrupted wait, and process-local recovery boundary | Shared `Workflows` calls, server ownership, architecture/operations updates, and recorded lifecycle proof | PASS apart from the signal-listener error projection in `FIND-TASK-005-4` |
| Scenario 4, REQ-044, and AC-014–017/023/026: supported route/protocol/security matrix and pre-dispatch refusals | CLI route matrix, cumulative server/shared owners, and recorded aggregate evidence | PASS |
| Scenario 5, REQ-054–059, and AC-029–031: Rust/Python/TypeScript local journeys retain exact identities, bindings, outputs, routes, and no floating | Rust and TypeScript use Cards registration; Python uses installed `wyrd apply`; compiled CLI owns the separate apply proof, per the standing human decision | PASS |
| Architecture, doctrine, security, deployment, recovery, and user documentation match approved lifecycle and route behavior | Changed authority/docs and recorded `docs:check`/aggregate evidence | **FAIL** only at the CLI endpoint mismatch captured by `FIND-TASK-005-1`; other documented boundaries pass |
| Repository maintainer and error-handling rules | Two journey helpers hide Unix trait imports in function bodies; CLI discards the signal listener error | **FAIL** — `FIND-TASK-005-3`, `FIND-TASK-005-4` |
| CI selection, TypeScript packaging, and mock-scope behavior | The TypeScript package-selection expectation follows the required dependency cone; the approved dev-only wiremock allowlist uses the existing mechanism | PASS |
| Explicit non-goals and prohibited scope | No Python/TypeScript server lifecycle, MCP Workflow surface, persistent queue/recovery, credential administration, compatibility alias, migration path, or bespoke affinity implementation entered the diff | PASS |

## Independent review results

| Review | Result | Proposed material findings |
|---|---|---|
| Behavior review | FAIL | `BEHAVIOR-001`, `BEHAVIOR-002` |
| Invariant review | FAIL | `INVREV-001`, `INVREV-002` |
| Repository standards review | FAIL | `STD-001`, `STD-002` |
| Maintainer review | FAIL | `MAIN-001` |
| System-resilience review | FAIL | `SYS-001` |
| Security domain review | PASS | None |
| Lifecycle/concurrency domain review | PASS | None |
| Cross-runtime SDK domain review | PASS | None |
| Structured Ponytail validation | SPEC_REVISION_REQUIRED | `FIND-TASK-005-1` through `FIND-TASK-005-4` |

All required reports are present. The domain PASS reports do not contradict the
retained source claims: they address narrower security, accepted-run lifecycle,
and cross-runtime boundaries. Unique maintainer and error-handling claims were
independently validated rather than discarded for lack of duplicate discovery.

## Follow-up decision

No focused `followup-rev` was needed. Discovery exposed no materially
conflicting source interpretation, unreviewed reachable path, or repeated
remediation with an unresolved common source. The overlapping endpoint and
input-order claims agreed, while the unique import and signal-error claims were
complete enough for direct independent validation. The Ponytail reviewer read
all discovery reports and confirmed that the scope-specific PASS results did
not deny those claims.

## Validated finding ledger

| Finding | Status | Classification | Outcome |
|---|---|---|---|
| `FIND-TASK-005-1` | REVISED | DRIFT | The CLI forbids `--server` with authored files, but authored external refs and `wyrd_gateway` execution need the explicit endpoint. A safe correction requires an approved public shared-client composition surface; **SPEC_REVISION_REQUIRED**. |
| `FIND-TASK-005-2` | REVISED | VIOLATION | Invalid local-detach and server-file choices read `--input-file` before refusal, allowing IO/blocking and the wrong stable error. Validate mode compatibility before input reading. |
| `FIND-TASK-005-3` | CONFIRMED | VIOLATION | Two changed Rust journey helpers place `PermissionsExt` imports inside functions contrary to the repository module-import rule. Move the guarded imports to the module blocks. |
| `FIND-TASK-005-4` | REVISED | VIOLATION | `tokio::signal::ctrl_c()` listener failure is treated as user interruption. Handle its result in the existing select arm and project failure through the existing typed error path. |

The decision-complete evidence, correction boundaries, and focused closure
proof for every retained finding are preserved in `findings-validation.md`.

## Verification limits

This review intentionally performed no builds or tests. It relied on source
inspection and the task's recorded evidence: the final green aggregate gate,
the three exact ignored compiled-CLI journeys, the focused Rust/Python/
TypeScript journeys, the Python gateway regression, `docs:check`, and clean
`git diff --check`. Those results remain credible for the paths they exercised,
but the CLI contract test encodes the endpoint drift and omits the input-file
ordering case, while successful SIGINT proof cannot establish listener-failure
handling.

No live cloud or provider credential lane is required. Deterministic local
upstreams are the repository-approved proof class for these routes.

## Prior findings and standing decisions

The five TASK-004 r6 findings in
`changes/active/skald-workflow-runtime/review/TASK-004-r6/lead-disposition.md`
remain deliberately deferred and were not reopened or converted into TASK-005
findings. All Revision 13/14 provider decisions and prior TASK-004 human
decisions stand.

The review also preserved the following settled decisions: Rust and TypeScript
register through Cards while the compiled-CLI apply proof lives in the CLI
journey; Python runs installed `wyrd apply`; TypeScript packages the required
Skald dependency cone; and the development-only wiremock allowlist entries are
approved. No retained correction requires a bespoke check, setting, file,
option, harness, compatibility path, or mechanism without established Wyrd or
widely used-project precedent.
