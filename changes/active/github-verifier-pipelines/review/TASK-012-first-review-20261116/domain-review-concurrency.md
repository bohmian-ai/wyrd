# Concurrency and resource ownership review

Result: FAIL.

Subject: first review of c24e1db8f6641cca9912376a4587e63e2fe74dd1...9016393004dc0eb319075245c5f6a1d7a6c2cd46, TASK-012. HEAD and the candidate branch both resolved to the recorded candidate during this review. No prior acceptance is carried forward.

## Boundary and authorities

Reviewed the common process memory/scratch ledger, hosted admission, direct/nested tool concurrency and spend, worker envelopes, shared combined owner, workspace staging/replacement/cleanup and retained Bifrost cache ownership. The task's Maintainer scope direction and pipeline revisions 13–15 govern. No benchmark, multi-pod qualification, architecture edit or struck proof is required here.

Applicable authority: root AGENTS.md; agent_docs/maintainer-style.md; changes/spec-driven-development.md; pipeline spec REQ-015, REQ-017, INV-005 and AC-012; task OBL-3 through OBL-7 and combined-owner portion of OBL-9; amended code-mode spec REQ-027 through REQ-030 and INV-010; Bifrost Admission and memory. These require per-use ownership, independent charges for new encodings/copies, charges through cleanup failure and no premature release. The necessary server/dependency baseline is accepted; the findings below concern allocations and scratch files these owners explicitly undertake to charge.

## Source coverage and acceptance matrix

| Obligation or invariant | Implementation evidence | Assertion or static proof inspected | Result |
|---|---|---|---|
| OBL-7, OBL-9: one process ledger and combined hosted owner, no idle queued reservation | runtime/resources.rs ByteLedger and ProcessResources; Bifrost ResourceState::charging; boot/mod.rs:1319–1368 takes process from the Bifrost root and gives it to HostedBlocks; hosted/blocks.rs:66–145 shares one sandbox, workspace owner and model-call semaphore | runtime/resources.rs:276–306 process_grants_follow_use_without_double_promises asserts limit refusal, split/merge without double charge and zero usage after release; boot source proves identity of the process owner | PASS |
| OBL-6, code-mode REQ-029: atomic byte grant, tenant FIFO/round robin, waiting-byte custody and cancellation | hosted/admission.rs:119–367; ByteLedger atomic try_update, ByteCharge drop; Sandbox::exec holds retained bytes then acquires an envelope | admission_rotates_tenants_and_accounts_waiting_bytes asserts order a1,b1,a2 and held waiting bytes; a_non_fitting_head_does_not_block_another_tenant asserts smaller eligible tenant runs; cancelled_waiters_leave_no_bytes asserts both queue and retained-byte release | PASS |
| OBL-4/5: nested calls progress at concurrency one; shared spend across origins/retries and restored attempts | vala blocks/tools.rs ToolSession::resolve, Dispatch::call/permit, ToolBridge; code.run holds no operation permit; same ToolCalls shared by Dispatch; engine.rs:1885 and :2025 attach attempt recorder; RunState::spent takes max, protecting against reordered callbacks | blocks/tools.rs:1071 resumed_invocation_continues_from_its_recorded_spend asserts restored 2/4 permits exactly two more and reports 3,4; hosted artifacts shared-concurrency and budget journeys inspect common call handling; source proves excluded nested writable tools cannot acquire another dispatch path | PASS |
| OBL-3: settlement precedes retry/result and blocking operations retain permit/writer | engine.rs:1917 and :2040 await ToolCallsOwner::settle; ToolSession registers its settlement once; HostedTools::open attaches work.close/wait; Invocation::blocking owns writer and hold_permit inside tracked blocking task | blocks/tools.rs ToolCallsOwner::settle drains registered settlements; hosted/tools.rs:420–446 retains guard inside spawned closure; engine retry branches execute after settlement | PASS |
| OBL-6: suspended worker memory stays charged, Python runnable permit returns between turns, reset/reap controls worker custody | sandbox.rs:388–818; Workers/Lease keep process charge until process no longer exists; run_turn scopes runnable permit | sandbox_worker.rs:847 suspended_sessions_release_runnable_capacity asserts another program returns 7 while first is suspended, memory remains at least its envelope, first then returns written and runnable permit returns; failed/prewarmed-worker tests assert PID/charge recovery | PASS |
| OBL-6/7: retained cache allocation ownership after eviction | storage/range_cache.rs ChargedRange placed in Bytes::from_owner; footer_cache.rs ChargedFooter retains decoded Parquet handle and charge until swept after last external reference | range_cache_charges_retained_bytes_until_released and footer_cache_charges_held_parquet_footers_until_released assert used bytes unchanged after eviction while a scan retains the object, then return after holder drops (footer sweep accepted) | PASS |
| OBL-6/7: workspace JSON encodings are admitted before allocation | Workspaces::stage_json workspace.rs:308–315 and Workspace::extract :1416–1423 | Both production paths allocate full JSON Vec before try_charge/try_grow. Existing extraction test asserts final usage and refusal before disk mutation, not allocation ordering | FAIL: CONC-01 |
| OBL-6/7: replacement staging never promises process scratch twice; old contents survive refusal | Workspace::write workspace.rs:1540–1567 | Delta-only admission followed by full temporary replacement file while old target remains; writes_charge_process_scratch asserts final totals and therefore does not cover transient disk ownership | FAIL: CONC-02 |
| OBL-6, code-mode REQ-027/INV-010: scratch remains owned through deletion failure | Workspace::drop workspace.rs:473–497 and remove :503–506 | Removal errors only log, then scratch drops. Existing cleanup tests cover successful deletion/cancellation. Oracle ChargedSpillFile::drop already retains surviving-file ownership | FAIL: CONC-03 |

## Proposed material findings

### CONC-01 — Workspace encodings allocate before their process-memory admission

Classification: INCORRECT. Obligations: TASK-012 OBL-6/7, amended code-mode REQ-027's independent charge for new encodings/copies, and the task's reserve-before-copy constraint.

Locations: crates/wyrd/wyrd-server/src/verification/hosted/workspace.rs:308–310 and :1416–1423.

Reachable path: a declared Agent/code workspace uses CallValue::Json or a JSON publication, through HostedTools::open -> Workspaces::open -> stage_json. At line 308 the entire pretty JSON Vec is allocated before process-memory admission at line 310. A declared direct or nested fs.extract similarly follows Invocation::call -> Workspace::extract and allocates its encoded output at line 1416 before growing the charge at 1423. Neither path requires malformed input or an unsupported tool. When other capabilities hold all remaining memory, both construct their extra encoding before returning Busy. Concurrent opens/extractions can therefore allocate the very bytes the shared ledger refuses, while Bifrost and other executions still own the promised memory. JSON staging permits up to the configured combined file budget, 100 MiB by default, so this is not just fixed bookkeeping overhead.

All sibling callers of stage_json were traced: direct JSON inputs, JSON publication inputs and collection manifest generation in Workspaces::open/stage_publication. Both direct/model fs.extract and nested fs.extract share Workspace::extract through Invocation::call. The extraction source buffer is correctly charged before std::fs::read; the defect is the separately allocated encoding.

Smallest correction boundary: keep Workspaces/Workspace as the owners, the same ByteLedger/ByteCharge and the same public Busy behavior. Count the exact pretty encoding through serde_json's writer API without retaining bytes, following the existing no-allocation counting pass in verification/state.rs Checkpoint::charge, then acquire/grow the encoding charge before constructing its Vec. Keep the charge through the write and retain the existing extraction destination/no-overwrite behavior. This needs only a bounded local encoding change, not a new allocator, resource manager, serialization framework or whole-extractor rewrite. The finding does not require separately instrumenting every dependency allocation or redesigning the accepted baseline.

Focused proof: extend the workspace owner tests so memory sufficient for the source but insufficient for the encoded output refuses before allocating that encoding; cover JSON input staging and fs.extract through their shared owner. An allocation observation at the existing owner boundary can distinguish pre-admission from post-allocation refusal. Preserve successful serialization bytes, Busy classification and unchanged destinations.

### CONC-02 — Atomic fs.write replacement undercharges its temporary file

Classification: INCORRECT. Obligations: TASK-012 OBL-6/7, charge before mutation and new copies independently charged; amended code-mode REQ-027/INV-010 aggregate scratch ownership.

Locations: crates/wyrd/wyrd-server/src/verification/hosted/workspace.rs:1540–1543 and :1563–1566.

Reachable path: an ordinary declared model fs.write reaches Workspace::write on Invocation's tracked blocking pool while holding the workspace writer. When replacing a file, admission asks only for new_size minus old_size. It then writes all new_size bytes into a NamedTempFile beside the unchanged old target, and only afterward renames the temporary file over that target. For an equal-size replacement at a full process scratch ledger, admit_write(0) succeeds while a second full copy is written with no scratch charge. The per-workspace final-size rule is valid, but it cannot also account for the temporary physical copy. Sibling workspaces and Oracle/Forge spill can be admitted using those already-consumed disk bytes; near the filesystem budget this defeats cooperative scratch refusal and exposes unrelated work to ENOSPC. Authored work/file limits do not repair an aggregate ownership gap.

Sibling writers: Python writes are separately mediated by Sandbox::exec and Workspace::admit_write; extraction writes a new target and admits its full output; archive staging retains source charge until deletion. The missing charge is specifically the replacement temporary file, whose lifecycle Workspace::write owns. The existing writes_charge_process_scratch test proves final retained size once and does not observe this staging interval.

Smallest correction boundary: retain the existing per-workspace replacement-size calculation, atomic rename and writer exclusion. Separately charge the process scratch ledger for the temporary file's complete bytes before creating/writing it, while the old target remains charged. After successful replacement/deletion, transfer/reconcile to the final retained-file charge, returning only bytes no longer on disk. If temporary ownership cannot fit, return the existing busy host stop before mutation and preserve the old file. Do not weaken atomic replacement or introduce downstream guards, a second ledger, or a reservation policy. This is a local temporary-file lifecycle change; deleting atomic staging would violate unchanged-content-on-failure.

Focused proof: with the process ledger filled by an existing file plus another owner, a same-size replacement that cannot hold its temporary copy must refuse before writing and retain the old bytes. With enough temporary room it succeeds and ends charged once for the resulting file. Test the failed-write path as well so staging custody survives until its file is removed.

### CONC-03 — Workspace cleanup failure returns charges for surviving scratch

Classification: INCORRECT. Obligations: TASK-012 OBL-6 releases scratch only after deletion or ownership transfer; amended code-mode REQ-027 explicitly retains charges through cleanup failure; INV-010 forbids promising still-owned scratch twice.

Locations: crates/wyrd/wyrd-server/src/verification/hosted/workspace.rs:473–497 and :503–506.

Reachable path: after a charged hosted workspace completes or is cancelled, Drop runs empty_root on the blocking pool. If the scratch filesystem rejects unlink/removal (for example a transient read-only filesystem or IO failure), empty_root fails, the fallback remove_dir_all can fail for the same reason, remove merely logs, and line 490 drops the full scratch charge despite surviving files. The nonrecycled partial-open cleanup does the same at lines 475–478. Once filesystem writes recover, later work is admitted against a ledger that forgets the orphaned bytes. Repeating the failure can consume the filesystem outside the process cap and deny Bifrost/other tenants scratch capacity. A next-boot cleanup does not protect the still-running process. This is an explicitly required failure boundary, not a proposed cleanup availability guarantee.

Sibling cleanup owners: workspace successful recycling, partial-open failure and ordinary cleanup share this cause; boot/data_root.rs clears verification scratch only after exclusive boot ownership. Oracle ChargedSpillFile::drop at oracle/spill.rs:224–245 already implements the minimal safe rule: a surviving file retains its ByteCharge for the process lifetime, with next-boot cleanup reclaiming it. No workspace retry/garbage-collection service is required.

Smallest correction boundary: let the Workspace cleanup owner distinguish confirmed deletion from failure, and retain the charge for surviving/unknown bytes instead of returning it. Follow the existing Oracle lifetime-retention policy and preserve logging plus next-boot cleanup. Successful deletion still returns charges and empty roots remain reusable. Retaining the conservative whole-workspace charge on failed cleanup is sufficient; exact partial-failure remeasurement, repeated retries and a new cleanup authority are unnecessary. Apply the same rule to the cancelled/unrun cleanup closure if it can return a charge before cleanup completes.

Focused proof: deterministic workspace cleanup failure leaves the directory's charge held while the owner ends and prevents a sibling grant from reusing those bytes; normal successful cleanup returns it. Use the existing workspace owner/test tier, without a server crash hook or broad journey suite.

## Verification limits and exclusions

This is a source-based domain review. No Cargo process, Postgres lane, benchmark, synthetic host load or source edit was run. The candidate's recorded green lanes were treated as claimed verification, and specific assertions above were inspected directly; none proves the three missing transient/failure boundaries. The immutable subject remained unchanged. The report proposes no findings for struck proof cases, gateway clock admission, multi-pod qualification, architecture wording or optional performance tuning.

No nonblocking suggestions are retained. Independent validation must confirm or reject these source-local proposals before they enter the final finding ledger.
