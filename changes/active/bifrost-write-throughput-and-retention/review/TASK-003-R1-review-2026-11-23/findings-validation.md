# TASK-003-R1 independent remediation validation

Recommendation: **PASS for the remediation and pre-benchmark code review**.
All five retained findings are closed. This does not complete TASK-003 or
qualify measured performance: REQ-020 remains deliberately pending the
maintainer's later final benchmark.

## Immutable subject and scope

- Original base: `74034459d468fd0adf277e3b51c32b1a676e8e55`.
- Prior reviewed candidate: `c0900bff523e83d3666658440daa81355067cb34`.
- Remediation candidate: `0d8c3fed690d51fac5b9c4eb51af951025927f5f`.
- Actual remediation diff: `git diff c0900bff5 0d8c3fed6`; cumulative context is
  `git diff 74034459d...0d8c3fed6`.
- Approved contract: SPEC-bifrost-write-throughput-and-retention revision 4,
  original TASK-003, prior validated findings and verdict, and TASK-003-R1's
  in-place execution evidence. Uncommitted packet files are intentional;
  unrelated working-tree benchmark/capacity edits are excluded.

This is a fresh independent remediation review. I read root AGENTS, the task
review skill, relevant workflow/testing/maintainer guidance, the approved
requirements, original task, prior validation/verdict and remediation record.
There is no nested AGENTS in the affected Vala subtree. CodeGraph preceded
source tracing. HEAD stayed at the candidate during this pass.

The changed production boundary remains the existing shard coordinator:
`run` → retained `GroupPipeline` stages → `finish_stage`, and
`dispatch_next` → scheduler eligibility/collection → existing group dispatch.
The only other changed owner is Bifrost's scheduling authority paragraph.
SQL, WAL execution lanes, insertion/visibility, resource APIs, cleaner,
recovery, Sandbox and SDK projections are unchanged by the remediation.

## Prior finding closure matrix

Locations below refer to
`crates/vala/vala-bifrost-redux/src/scribe/shards.rs` unless another file is named.

| Prior finding / approved requirements | Correction and actual assertion evidence | Result |
|---|---|---|
| FIND-TASK-003-1 / REQ-015, REQ-019; R1 scenario 1 | `run:1568–1574` services `ready_stage` before mailbox routing/dispatch. `ready_stage:4017–4024` polls older then younger with a ready fallback; `stage_done:4041–4069` polls the retained future using the actual context and removes a slot only on readiness. Unfinished operations remain owned across selection. `drain_mailbox:1651–1657` routes at most 64 commands per turn. `a_ready_head_completion_is_acknowledged_before_the_next_group_prepares:11106` drives the actual run loop with a ready older fence and 129 queued appends, holds the start of later preparation, and asserts earlier ACK and visible rows plus exactly one chunk removed from the mailbox. The hold is `cfg(test)` and absent from shipping state. | **PASS — CLOSED**. |
| FIND-TASK-003-2 / REQ-015–016, REQ-019; R1 scenario 2 | `eligible:735–754` counts only the table prefix before a held/repeated identity, capped at the requested target. `dispatch_next:4219–4225` uses that count rather than raw queue length. `GroupCollection::due:3922–3935` keeps the first eligible deadline, target, idle and predecessor-completion rules. `held_retries_do_not_satisfy_the_collection_target:11179` drives coordinator dispatch with 31 held retries and one eligible request, advances controlled time, adds arrivals without moving the deadline, then asserts all 40 eligible requests drain and held retries remain queued. `a_predecessor_completion_dispatches_a_short_eligible_group:11260` proves settling the older group immediately dispatches the short group and preserves released-table order. | **PASS — CLOSED**. |
| FIND-TASK-003-3 / REQ-015; aligned authority update | `architecture/bifrost-design.md:924–934` now distinguishes the 32-request waiting target from ready draining to existing byte/64-request limits, fixes the first-eligible deadline, and records held-identity exclusion and ready-completion service. The correction is consistent with the approved spec and current `pop_group`; content review suffices. | **PASS — CLOSED**. |
| FIND-TASK-003-4 / REQ-014–016, REQ-018–019; R1 scenario 3 and explicit maintainer instruction | `tenants_and_tables_interleave_through_both_slots_with_independent_acks:12671–12851` uses a migrated PostgreSQL fixture, real WAL, and one production shard loop. Four waves mix two tenants and two tables each. Each real older fence is held while the next wave syncs; releasing that fence must ACK the older wave while younger IO remains held. Per-wave assertions check pending younger ACKs, no later fences, full earlier fences and expected counts under every tenant/table key; final assertions check exact counts, four syncs, retry convergence and released transient bytes/pending count. The last wave's held retry is excluded: equal WAL growth and `!first_commit` catch a second attempt, with a recorded mutation failure when exclusion is removed. | **PASS — CLOSED**. |
| FIND-TASK-003-5 / REQ-017, REQ-019; R1 scenario 4 | `an_unavailable_commit_reconciliation_retains_both_groups_for_restart:13094–13236` holds a real deferred COMMIT trigger, syncs the younger group with its completion held, revokes SELECT, and releases the older trigger. This drives the production fresh `resolve` failure/poison branch at `commit_batch_control_fences:5540–5554`. Assertions prove poisoned readiness, both waiters pending, retained charges, no fences, shutdown still queued/task running while younger IO is held, then both waiters closed without a response after IO drains. Restoring privilege and replaying the next epoch restores no provisional rows. The recorded mutation removing that branch's `poison()` fails the intended assertion. | **PASS — CLOSED**. |

The healthy connected test now supplies the missing real tenant/table pipeline
composition. It is not claimed to independently prove PostgreSQL RLS or every
fairness permutation: existing scheduler FIFO/fairness assertions and unchanged
tenant-bound production query ownership remain part of the carried-forward
proof. The held retry behind its table's fresh appends also exercises the FIFO
interaction with cross-slot exclusion. There is no need to multiply every fault
across every tenant/table combination.

The shard fixture has pod-global request admission, not separately exposed
per-tenant/table request counters. Its zero transient-byte and pending-request
assertions demonstrate settlement through the actual global owner; adding a new
counter API is neither required nor justified. Tenant/table row ownership is
observed through the four distinct memtable keys.

## Affected-path and carried-forward coverage

I traced `run`, `ready_stage`, `stage_done`, `drain_mailbox`, `route_command`,
`dispatch_next`, eligibility, collection and `finish_stage` through their
existing consumers. The nonblocking readiness poll uses the task's real waker;
it does not create a detached future, replace pending futures or abandon IO.
Older-first stage selection and oldest-only fencing still apply. Poisoned
draining takes its existing branch before new preparation. Deferred controls,
rotation, pressure and shutdown continue through their original ownership
barriers.

Bounded mailbox service changes when a later command becomes visible to the
tenant/table scheduler: the first idle group may see only the first 64 queued
commands. It does not reorder commands within a table or alter the scheduler's
tenant/table round-robin policy. The approved contract requires bounded service,
not a snapshot of all future/queued tenants before each group; this tradeoff is
not a new correctness finding.

| Existing acceptance coverage | Carry-forward assessment |
|---|---|
| REQ-014 bounded slots, repeated overlap and oldest-only fence | Unchanged pipeline storage and dispatch guards; new mixed-wave connected test strengthens the previous one-table handshake evidence. |
| REQ-015 visibility/mirror-before-ACK, fairness and collection | Prior insertion/mirror/publication ordering remains unchanged; R1 closes completion service and eligible collection defects, with existing independent-ACK regression evidence. |
| REQ-016 tenant SQL, retained-version/cross-pod arbitration and identity durability | Production fence/retry queries unchanged; R1 changes collection eligibility only and adds a mixed-tenant real SQL test. Prior source-backed SQL/identity evidence remains applicable. |
| REQ-017 failures and canonical restart recovery | Production failure/recovery semantics unchanged; the new unavailable-reconciliation test closes the previously missing producer path, supplementing the retained partial-tenant/canonical recovery proof. |
| REQ-018 lifecycle, resources and receipt custody | Ownership APIs, generation controls, segment references and safe-frontier/cleaner code unchanged. The bounded service/readiness changes preserve control routing; prior lifecycle and custody analysis carries forward, with reported focused shutdown/partial-failure regressions. |
| REQ-019 deterministic progress and failure proof | All five closures use actual owner paths and observable assertions at their appropriate tiers. No elapsed speed threshold or quiet host is needed. |
| REQ-020 final performance qualification | **UNRESOLVED intentionally**. No benchmark run; maintainer execution follows code acceptance and required integrated checks. This is not a remediation defect and does not close the original task. |
| Prior collateral Sandbox/SDK/Forge assessment | No changes to these boundaries in R1; prior validated source and execution evidence remains applicable. |

One fresh remediation reviewer is sufficient for this bounded correction and
test-only fault coverage. The preceding review already selected independent
state/durability and collateral specialists; no ownership architecture, SQL
semantics, security boundary, resource API or cross-tier contract changes here
invalidate their evidence. There is no missing selected report, unresolved
specialist boundary or new proposed blocker requiring separate validation.

## Execution, diagnosis and TDD limits

The orchestrator independently ran the exact five new selectors through the
repository PostgreSQL wrapper at the candidate: **5/5 PASS**, nextest run
`040fdc9a-2068-4622-864e-f0ffccbf13fa`, with 1010 tests not selected. The
remediation diff's `git diff --check` also passed. This reviewer did not run
Cargo, avoiding concurrent builds in the shared checkout.

The task records the full lib suite **1015/1015**, 11 focused regressions,
`mise run lints`, formatting and diff checks as successful. These full-suite,
RED/GREEN and mutation results remain implementation-record evidence, not
independently recreated execution history. Actual test assertions and production
paths were inspected independently; passing names/counts alone were not used as
proof. Broader Redux integration and journey suites are left to integrated
change review as prescribed, rather than demanded again for this remediation.

The recorded first failed-reconciliation fixture diagnosis is consistent with
the test and production path: a row policy cannot force a failed lookup when
there are no matching rows; statement-level privilege revocation does. The
current test therefore controls actual lookup availability rather than inferring
it from the original failed trigger. No production change was made for that
fixture correction.

R1 scenarios 1–2 report expected behavioral RED then GREEN: completion before
preparation and eligible-collection failures, rather than setup or compilation
errors. R1 scenarios 3–4 are **evidence-only** with mutation checks, not a
retroactive test-first claim. Preserve the original TASK-003 **TDD VIOLATED**
history separately; this remediation does not erase it or require historical
RED reconstruction.

No reviewed source, approved spec or original/remediation task was edited by
this reviewer. No benchmark was run; measured throughput and complete TASK-003
acceptance remain pending the maintainer's final qualification.
