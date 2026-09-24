# TASK-005 round 2 findings validation

## Immutable subject

- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate and checked-out HEAD: `81d2221b4d3b1a9fb6bd36c591d421385cc8c67d`
- Authority: approved `changes/active/verified-change-contract/spec.md` revision 36, original `tasks/TASK-005-production-drift-verifier.md`, `AGENTS.md`, `architecture/agent-rules.md`, `architecture/references/languages/spec-driven-development.md`, and the applicable Wyrd, Bifrost, security, and Drift authorities.
- Input: complete cumulative diff, all five round 2 Wave 1 reports, and the round 1 verdict and findings. `.codegraph/` is absent. This validation inspected source and recorded test evidence; it did not rerun broad lanes.

## Wave 1 disposition

| Source ID | Decision | Reason |
|---|---|---|
| `TASKREV2-001` | **CONFIRMED** | The approved AC-013 explicitly requires two Services sharing a Trigger and manual binding dispatch. The Rust journey has one Service and one scheduled binding; the route test only proves manual enqueue. Retain prior `FIND-TASK-005-2` for the remaining activation proof. |
| `REPO-R2-001` | **CONFIRMED** | The active, ready task still declares revision 35 and directs a local typed Oracle plan, which revision 36 supersedes. This is a current task-authority defect, not a reason to change the approved behavior. New `FIND-TASK-005-10`. |
| `DATA-R2-001` | **REVISED** | Cancellation is checked after decoding but before fitting, not within the expensive fit. The proposed per-feature check alone could leave a single large feature running. Retain prior `FIND-TASK-005-9` and require cancellation observation during potentially long fit work while preserving the existing fit semantics and fenced settlement. |

The security/tenancy and statistical domain reports propose no findings. Their empty proposed ledgers were independently checked against the scoped token, audited query path, fixed SQL, and PSI/SPC/Custom production paths; no additional material finding is retained. Neither a new security nor a new resource-ownership decision is needed for the bounded corrections below.

## Final deduplicated ledger

### FIND-TASK-005-2 — CONFIRMED — MISSING: AC-013 binding journey proof

- **Wave 1 source:** `TASKREV2-001`.
- **Obligation:** `spec.md:1453-1463` and TASK-005 Scenario 6 require a real Service binding journey that proves two Services sharing one Trigger produce separate binding runs, and that manual binding invocation follows the configured Operator path; direct invocation dispatches none.
- **Location and evidence:** `sdks/wyrd-sdk-rust/tests/drift_verification.rs:463-547,974-1028` registers one Service, makes its only binding due, and checks its two Operator dispatches. Its direct-run helper checks zero dispatches at lines 311-327. `crates/wyrd/wyrd-server/tests/pg_verification_routes.rs:318-360` checks that a manual binding run enqueues, without running or dispatching it. The Rust, Python, and TypeScript Drift journeys contain no two-Service shared-Trigger or manual-binding-to-dispatch assertion. Generic scheduler tests establish their own seam behavior but do not exercise this Card projection and Drift journey.
- **Reachability and consequence:** Service registration projects each binding; the generic scheduler claims due bindings and the generic runner consumes those claims. A shared Trigger could project or route the wrong binding, and a manual binding could enqueue but fail to dispatch, while current Drift journeys stay green.
- **Smallest correction:** Extend the existing Rust SDK/server Drift journey and fixture, using the existing Service, Trigger, Operator, binding-run, and due-wakeup mechanisms. Register two active Services referencing the same Trigger; make the same occurrence due for both; assert separately attributable completed results and the configured dispatches for each binding. Invoke a binding manually and assert its failed result's configured dispatches; retain the existing direct-run zero-dispatch assertion. No second scheduler, client logic, or new harness is warranted.
- **Focused proof:** Run the exact Rust Drift journey target through the repository-managed Postgres setup and assert both binding identities, result identities, dispatch sets, manual binding dispatch, and direct-run non-dispatch.

### FIND-TASK-005-9 — REVISED — VIOLATION: blocking baseline fit can outlive cancellation

- **Wave 1 source:** `DATA-R2-001`.
- **Obligation:** `spec.md:1318-1332` (`REQ-146`) requires a 30-second shutdown drain followed by cancellation of remaining work, with its fenced lease released or expired. The round 1 finding requires the blocking decode and fit to stop before settlement.
- **Location and evidence:** `crates/wyrd/wyrd-server/src/verification/fitter.rs:215-232,258-294` selects shutdown/timeout, cancels a token, then awaits `fit` before fenced settlement. The blocking closure checks the token after `decode_bounded`, then calls `vala_drift::fit_baseline` without passing cancellation. `crates/vala/vala-drift/src/baseline/mod.rs:34-78` calls PSI/SPC fitting; `psi/mod.rs:66-89,328-426` and `spc/mod.rs:42-115` perform per-feature collection, binning or control-limit work with no cancellation observation. `feature.rs:27-69` collects entire columns, and SPC control-limit fitting scans chunks. Existing `cancelled_decode_returns_before_fitting` tests cancellation before fit, not after it begins.
- **Caller and boundary trace:** `BaselineFitter::run` calls `pass`, which acquires the shared permit and calls `fit_next`; `fit_next` is the sole production caller of `fit`. The server's blocking closure is the sole production caller of `vala_drift::fit_baseline`; its ordinary public fit entry points also serve scorer tests and other Vala callers. `VerificationRuntime::run` awaits the fitter capability's exit. Dropping or aborting a Tokio `spawn_blocking` handle cannot stop its running closure, so settlement must await cooperative completion to avoid release while work remains active.
- **Observable consequence:** A fit cancelled after decode can keep CPU and memory occupied past the execution timeout or shutdown drain; its lease can expire while it still computes, allowing another process to claim the row. The token fence protects final row settlement but does not stop duplicate work.
- **Smallest correction:** Let the server's existing cancellation signal be observed during potentially long PSI/SPC fit work, including work within a large feature, through the existing `vala-drift` fit owner. Keep the ordinary fit API's result/profile semantics, the decoded-size bound, the shared permit, and the fenced SQL settlement. Do not detach blocking work, add a second worker or timeout framework, or release the lease before that work ends.
- **Focused proof:** A controlled cancellation after fitting begins must make the blocking fit return before `fit_next` releases or fails its lease; a non-cancelled fit must produce the same profile. Exercise the exact focused test through `mise exec --` and the owning broader verification lane.

### FIND-TASK-005-10 — CONFIRMED — VIOLATION: the active task points at superseded revision 35 mechanics

- **Wave 1 source:** `REPO-R2-001`.
- **Obligation:** `architecture/references/languages/spec-driven-development.md:12-23,134-172` requires a ready task to identify and agree with its approved spec revision. `AGENTS.md` gives current approved design authority over planning drift.
- **Location and evidence:** `changes/active/verified-change-contract/tasks/TASK-005-production-drift-verifier.md:1-43,85,118,158` remains `status: ready`, `spec_revision: 35`, and directs local typed Oracle plans. The approved spec is revision 36 and requires a scoped SYSTEM read token and fixed SQL through query service, Gate, and local or peer Oracle; the task's later r1 evidence itself records that replacement.
- **Reachability and consequence:** The ready task is the implementation/review entrypoint. Following its metadata and approach would recreate the forbidden local Oracle boundary even though the cumulative candidate implements revision 36.
- **Smallest correction:** Bring the active task's revision and displaced plan/owner wording into agreement with revision 36 while preserving original obligations and the historical r1 evidence. Supersession plus a replacement task is allowed by the workflow but adds an unnecessary packet for this same task. No implementation change or new design is needed.
- **Focused proof:** Read the final task frontmatter, outcome, owner/approach, and scenario wording against revision 36; no active ready instruction should prescribe the displaced local plan. No runtime test is needed.

## Prior finding closure

`FIND-TASK-005-1` is closed by the never-written-table inconclusive path and three SDK journeys; `-3` by removal of unrelated skill edits; `-4` by recorded exact commands; `-5` by bounded SPC history; `-6` by approved revision 36 and the token/query path; `-7` by test-module rustdoc; and `-8` by shared fit/run permits. `-2` and `-9` remain open only at the bounded gaps above. The candidate's F6 token scope and audit route match revision 36; no additional security finding is retained. Uncalled pre-existing `Oracle::query_plan` is outside this task and is not a finding.

## Validation result

Three bounded findings remain: `FIND-TASK-005-2`, `FIND-TASK-005-9`, and `FIND-TASK-005-10`. The approved specification resolves the security boundary, and each remaining correction fits an existing owner. **FIX_REQUIRED**.
