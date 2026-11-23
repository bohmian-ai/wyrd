# TASK-001-R1 verdict: FIX_REQUIRED

Four original findings close, and the approved removal of old performance
targets is implemented. **FIND-TASK-001-2 and FIND-TASK-001-3 remain incomplete**:
the supervisor can still reject an allowed process teardown and can renew an
exhausted cleanup allowance. These are behavioral defects, independent of TDD
history. The [R2 task](TASK-001-R2-finish-lifecycle-deadlines.md) is limited to
those two existing findings.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
- Original base: `2418dc7848375c00ac6a991cafca88f722bcfdfe`.
- Prior fully reviewed candidate: `1bcc403bf2c81c3cfa2c78a78f114cb9c5a49aaf`.
- Current candidate: `98a58443afac1f74cfefd607f3cd429c3b1cdee1`.
- Remediation: ten commits beginning `be099fd8e12a5054fc38e916817bcb52d842ecec`,
  based on `19fd9cfcf8a112c6d92440c9168bb03c8f838229`, through candidate.
- Reviewed diff: `git diff 1bcc403b 98a58443a`; implementation-only:
  `git diff 19fd9cfcf 98a58443a`; cumulative: `git diff 2418dc78...98a58443a`.
- Continuation since the blocked attempt: `git diff f436ada92 98a58443a`;
  stop-path lint rewrite plus committed task evidence. No rebase.
- Approved [SPEC-l1-benchmark-protocol revision 2](../../spec.md), blob
  `ee02662273dab166972ee289bb6cd64477d4bc32`.
- Original [TASK-001](../../tasks/TASK-001-standardize-l1-benchmark-sweeps.md)
  and [TASK-001-R1](../task-001-1bcc403b-review-1/TASK-001-R1-l1-protocol-boundaries.md),
  whose current evidence blob is `a359d379fa61df9410614e0cd5741682397403b7`.
- Original [verdict](../task-001-1bcc403b-review-1/verdict.md) and
  [validated findings](../task-001-1bcc403b-review-1/findings-validation.md).

HEAD and tracked source remained at the pinned candidate throughout this
review. Existing untracked review reports are outside the candidate. The
previous [blocked attempt](../task-001-r1-f436ada92-review-1/verdict.md) made
no final closure decision; its provisional observations were rechecked against
current committed source. See [subject record](subject.md).

Applicable authority remains root AGENTS.md, benchmarks/AGENTS.md,
changes/spec-driven-development.md, agent_docs/{maintainer-style,rust,agent-surfaces}.md,
TESTING.md and architecture/bifrost-design.md § Durability and visibility.

## Independent review and coverage

Fresh `r1_final_rev` independently checked prior closure, changed owners,
callers/consumers, assertions, committed execution evidence and the three
recorded diagnoses. Its [validation report](findings-validation.md) records the
two retained findings once, with exact paths, triggers, consequences and
corrections. The orchestrator collected identities/checks and packaged this
verdict; it did not discover or validate findings.

One fresh remediation reviewer is the skill's default and covers the known
registry, runner, subprocess cleanup and saved-report boundaries. No distinct
new production auth, tenant SQL, domain durability or SDK wire boundary needs
a specialist. The reviewer found no missing expertise and no new finding
requiring another validator. Original stable IDs are preserved.

| Existing finding / scenario / authority | Implementation and proof checked | Result |
|---|---|---|
| FIND-TASK-001-1 / 1 / REQ-001, AC-001 | One TSV owner feeds shell and Rust. Isolated checkout listing test at `capacity/main.rs:1015–1068` asserts exact category parity and no Cargo/mise invocation; focused GREEN recorded. | PASS — closed |
| FIND-TASK-001-2 / 2 / REQ-002/003, AC-003 | Runner test at `experiment.rs:7509` proves stop charged to teardown. Supervisor tests at `capacity/sweep.rs:1047,1237` prewrite a record; current `attempt` still cuts off an allowed unrecorded stop at 510 s. | FAIL — incomplete closure |
| FIND-TASK-001-3 / 3 / REQ-002/003, AC-002 | Hung/late Docker tests at `capacity/sweep.rs:1184,1284` cover bounded calls. Admitted wrapper expiry still renews stop/confirmation at `sweep.rs:347–349,382–388` and may authorize continuation late. | FAIL — incomplete closure |
| FIND-TASK-001-4 / 4 / REQ-003/005, AC-004 | Persist duration retained in sweep and hydrated by `summary.rs:171–187`; tests at `sweep.rs:1089,1352` cover measured duration, unavailable reference and late-save outcomes. | PASS — closed |
| FIND-TASK-001-6 / 4 / REQ-005/006, AC-004 | Full snapshots retained/hydrated; `sweep.rs:1089–1174` crosses child handover, saves timing/scan/counter/gauge evidence, deletes artifacts and asserts full report plus identical sweep output. | PASS — closed |
| Approved target removal / 5 / revised REQ-004/006, AC-003/004 | Case performance criteria inapplicable, integrity/sample checks retained. `experiment.rs:7607` and `main.rs:877` assert ordinary/profiled no-target output; affected mise/README guidance matches. | PASS; FIND-TASK-001-5 stays WITHDRAWN |
| FIND-TASK-001-7 / 6 / REQ-003/007, AC-002 | Case smoke judges integrity. Focused test at `experiment.rs:7666`; connected `pg_tests.rs:893–938` checks two saved INVALID Gateway rows, typed `no-errors` reasons and confirmed cleanup. | PASS — closed |
| Unchanged assigned behavior | Prior membership/order, selection independence, aggregate/pooling, measurement/late populations, report ordering/units and historical comparison/calibration evidence carried forward; affected judgment and snapshot seams rechecked above. | PASS, subject to lifecycle failures |
| AC-005 | Approved full-size execution hold and feasibility disclosure remain. | PASS for disclosure; feasibility UNVERIFIED |

Rust paths above are relative to `crates/wyrd/wyrd-testing/src/`.

## Execution, diagnostics and limits

The committed task records exact focused commands and outcomes plus final
`mise run fmt:check`, `mise run lints`, diff checks, 110 capacity tests
(3 skipped), 26 experiment tests and one connected `mise run test:benchmark:l1`
pass (284 s). These results were inspected alongside actual assertions and
their connected boundaries; they are recorded implementation evidence, not
fresh runtime passes by this review. The review reran candidate-relative and
working-tree `git diff --check` successfully. External Cargo work was active;
no duplicate Cargo or Docker suite was launched.

The cleanup loop's `now >= deadline` correction matches the reported spin,
and the connected assertion correctly switched to the typed criterion field.
The total-envelope reserve fixes the recorded total overrun but does not close
the two phase/continuation defects. Diagnostician agreement was treated as
context, not acceptance proof. Passing existing tests cannot prove these
unexercised paths.

The prior interrupted attempt is not a failed remediation round. This is the
first completed remediation audit; no two-round convergence assessment applies.
No new requirement, finding ID, spec revision or old performance target is
introduced. Remediation stays at the existing lifecycle owners.

TDD history remains disclosed as VIOLATED, including original history and the
recorded remediation gaps. Per maintainer direction it is not a blocker and
creates no repair work. No historical RED reconstruction or production churn
is requested. Full-size feasibility remains deferred under approved AC-005.
Review did not implement, commit, push, merge or deploy.
