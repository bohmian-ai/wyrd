# TASK-006 independent behavior review

Subject: base `e6d7d2681e588f118ec7a8e9bbcfab24757a654b` → candidate
`bfbdf016a5487cac17e8dc6fbb73cc8ae17a147f`, read directly with
`git diff <base>...<candidate>`. HEAD remained the candidate at the final check.
This report proposes findings for separate validation; it is not the verdict.

Authority: verification-workflow-ui approved spec revision 8, original
`tasks/TASK-006-waiting-and-approval.md`, authorization-model revision 9;
root AGENTS, `changes/spec-driven-development.md`,
`agent_docs/maintainer-style.md`, `TESTING.md`, `wyrd-ui` skill and its testing
reference; `architecture/wyrd-design.md:1794` and security posture's approval
boundary. No `.codegraph/` exists and no nested AGENTS governs the changed UI
or fixture paths. Current user instructions accept the two wire-driven mock
differences and transfer header/step-panel differences to TASK-004/005.

## Navigation and coverage

The existing status loader supplies waiting slots and all reached approval
occurrences. `VerificationExecution.svelte` builds definition-independent step
selection from invocations plus occurrences, presents each exact occurrence,
and emits the schema/catalog's `approval.decide` host action. The conventional
execution page submits to its session BFF route and invalidates its status.
`executionStates` feeds graph node/edge wait presentation. Unsupported hosts
receive the same component and issue no decision. The domain reviewer owns
independent permission/session/tenant validation; this report retains coverage
of the user-visible refusal and recovery flows.

Paths below are relative to
`crates/wyrd/wyrd-server/wyrd-ui/`, unless qualified.

| Existing requirement / scenario | Implementation and assertions inspected | Assessment |
|---|---|---|
| REQ-005/008, AC-004; scenario 1: waits, readable occurrences without definition/details | `VerificationExecution.svelte:69–85,256–289,363–382`; unit `ui-library.test.ts:3213`; real Node/MCP `ui-library.integration.test.ts:2485` checks two reached approvals, deferred `ci`, refused definition, no final result | Correct connected implementation for parked work. Real running-peer proof missing: B2. |
| REQ-008/014; scenario 2: exact decisions and same-execution continuation | Schema/catalog exact IDs and strict decision enum; component `:123–127`; host `+page.svelte:64–92`; unit/page exact request assertions; real integration rejects iteration 0, approves iteration 1, asserts same execution, receipt replay and requirement progress; browser `ui-pages.integration.test.ts:2643` approves with reason and rejects using keyboard | PASS for inspected behavior. No new server API, permission evaluator, browser runner or read-executor write. |
| REQ-008/014, AC-008; scenario 3: refusal and unsupported host | Component hides controls after refusal/unavailable host, unit asserts no emit; integration tests parent-only, approve-only, wrong Verifier, foreign tenant, custom role, grant revocation and unchanged pending state | PASS for presentation; permission implementation/evidence also receives independent domain review. A supported host with no permission hint may submit and display refusal, expressly allowed by REQ-008. |
| REQ-008, AC-004; scenario 4: lost/conflicting answer and explicit retry | Page `:76–90`, page test `:739–826`; integration drops a committed answer, reads exact occurrence, explicitly replays same decision/reason and compares receipt | Conflict/recorded readback paths are covered. Failed-readback path violates retry ordering: B1. |
| REQ-005/008/009, AC-005; scenario 5: expiry/cancel and retained progress | Integration cancellation expires release, subsequent decision refuses, invocations remain readable, no Approve; component timed-out fixture; unchanged backend prerequisites named in task evidence | PASS for bounded affected behavior; full terminal/publication journeys remain predecessor/change-review proof, not new implementation here. |
| REQ-014, AC-009; visual/shared delivery | Mock Inspector approval snippet, StepNode and source alignment; component tokens, keyboard/native controls; browser capture helper checks width 1440/390, both modes and no page overflow, captures pending/decided page | User explicitly supplies comparison and rulings. No remaining in-scope visual defect established. No local capture files located in searched target/mock locations; independent pixel comparison was not performed. Long-message/touch coverage remains a disclosed limitation; no defect inferred from absent screenshot alone. |

## Proposed B1 — INCORRECT: failed readback permits another decision

Authority: REQ-008 resolves ambiguous submission through recorded decision
readback; AC-004 explicitly says “Read back an ambiguous submission before
explicit retry.” Original scenario 4 likewise reads the exact occurrence before
an explicit retry.

Location: execution page `src/routes/t/[tenantKey]/verify/executions/[executionId]/+page.svelte:64–88`;
shared controls `packages/ui/src/lib/components/VerificationExecution.svelte:117,273–280`;
existing `src/lib/ui-pages.test.ts:783–803` demonstrates the path.

Start with a pending occurrence. The reader approves; its response is lost
and `invalidateAll()` fails. The page correctly says the outcome is unknown and
“Refresh before deciding again,” but changes action state to `error`. Shared
`off()` disables only pending/unavailable states, so both decision buttons and
reason field become usable immediately. The host guard likewise only checks
`pending`. The existing test explicitly asserts Approve is enabled and clicks
it again without a successful intervening readback.

Expected: resolve the unknown exact occurrence through authoritative status
before issuing another decision. Actual: another POST is possible first,
including after changing the reason or switching Approve to Reject. The backend
still protects its first durable decision, so this is a UI recovery-contract
violation, not duplicate durable execution or authorization bypass.

Smallest correction boundary: the existing host decision/refresh owner and its
action availability. Keep unknown outcomes refreshable and prevent resubmission
of that unresolved occurrence until authoritative readback succeeds. Preserve
exact IDs, no automatic write retry, server idempotency, ordinary failures and
other readable evidence. Closure: lost answer plus failed readback, attempt
both decision buttons, assert no extra POST; refresh successfully, then either
show its recorded decision or permit an explicit pending-occurrence retry.
Update the existing page test; exercise the ordering around a real committed
decision through the existing integration owner where transport is controlled.

## Proposed B2 — EVIDENCE_GAP: no live approval with a running peer

Authority: REQ-008 includes running peers; AC-004 says exercise a blocked branch
with a running peer. AGENTS Testing and TESTING's tier rule require executable
journey proof for user-facing behavior rather than unit-only substitution.

Location: original task scenario 1 and its evidence limitation;
`src/lib/ui-library.test.ts:3213` and
`src/lib/ui-library.integration.test.ts:2485`; fixture
`fixtures/cards/approvals/repeated.yaml`.

The real integration/browser verifier has approvals and a missing `ci` input
with all runnable branches parked. The sole running peer is `audit started` in
the disconnected unit fixture, as task evidence explicitly acknowledges.
Therefore current proof does not establish the connected state where real
public verification has reached an approval while another branch is running,
the reader selects/decides the exact occurrence, and waiting remains distinct
from the peer's activity with no final verdict.

This is missing proof, not an identified implementation bug. The minimum closure
is one focused extension of the existing real-server shared UI journey using a
controlled in-progress peer and normal public verification, consuming its actual
status/invocations into the shared loader/component. Assert missing input,
pending exact occurrence, running peer and absent verdict together, including
definition-unavailable selection already covered by scenario 1. No backend
behavior change or full journey-suite rerun is required. Use the task's existing
managed-Postgres UI integration runner with an exact selector.

## Verification and process record

The orchestrator reports fresh candidate `mise run ui:check` PASS (117/117,
svelte-check clean), `mise run codegen:check` PASS and range whitespace check
PASS. I independently ran `git diff --check e6d7d2681...bfbdf016a` successfully.
I inspected actual unit, page, real-server and browser assertions; the domain
reviewer is rerunning focused real-server proof. I did not launch overlapping
Cargo or broad suites.

The original task records focused real-server/browser and two backend approval
prerequisite passes, identity UI pass and a full-suite lifecycle startup failure
at occupied port 8000. That broad failure does not replace focused TASK-006
proof. This review did not reproduce or independently diagnose that startup.

TDD is separate: task evidence records missing-label, missing-emit and missing
host-dispatch REDs followed by GREEN. The first implementation and its tests
share commits (`9dce2d583`, `6f7faae6f`); git history cannot independently prove
the within-commit ordering. No contrary test-first history was established.
Do not invent historical RED evidence or require production churn to repair it.

Skipped: full suites, benchmarks (not authorized or relevant to these UI paths),
independent screenshot recapture and pixel comparison. No implementation files
were changed.
