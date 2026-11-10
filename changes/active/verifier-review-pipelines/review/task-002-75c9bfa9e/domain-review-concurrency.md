# TASK-002 concurrency and resources review

Result: **PASS** within the assigned concurrency/resource boundary. No material source-proven finding.

Subject: base `990a85dc7155ff0d4de1acc17e4aeaa7a42d582f`, candidate `75c9bfa9e07163099eac40d5a556c4447c6a519d`. First review; no carried-forward TASK-002 acceptance. HEAD and relevant tracked sources still match the candidate at completion. Only this report was written.

## Authority and coverage

Reviewed the task, its publication supplement, applicable REQ-023 and scratch obligations in the supplied spec, architecture/wyrd-design.md:1970–2023, root contributor rules, architecture authority map, testing guidance, spec-driven workflow, and maintainer style. The packet remains revision 21/draft with approved_baseline_revision 18; this review applies the approved scratch/publication behavior and does not treat unrelated wait/approval additions as authority.

CodeGraph was used through `mise exec -- codegraph explore` before source navigation. Reviewed the full base-to-candidate changes to workspace and invocation dispatch, and expanded into these unchanged shared owners and sibling consumers:

| Boundary | Source coverage and result |
|---|---|
| Scratch admission and lifetime | `workspace.rs::Workspaces::open`, `Workspace::materialize`, `Workspace` permit and `TempDir`; `HostedTools::open` creates one fresh workspace per attempt. Admission is nonwaiting and bounded by the existing semaphore. The blocking write owns an `Arc<Workspace>`, preserving its directory and admission permit until actual completion. |
| All scratch writers | `tools.rs::Invocation::call` fs.write and fs.extract branches, `Invocation::run`, `workspace.rs::writer`, `write`, `extract`, `mounts`, `work_room`; all production mutation paths use the same per-workspace mutex. No second write budget or write lock was introduced. |
| Publication reader | `Publications::path` calls `Workspace::freeze`; freeze obtains that same writer lock and moves it and the workspace into its blocking snapshot. The frozen bytes are independent of later writes; upload happens after the snapshot releases the lock. |
| Concurrent provider calls | `skald-agent/src/loop_runtime.rs:566–703` bounds active tool futures with `buffer_unordered(concurrency_cap)` and restores response order. `blocks/tools.rs::RecordedTool::record` reserves record positions under its mutex, invokes outside that mutex, then fills the reserved slot. No lock is held across invocation by recording. |
| Deadline and cancellation owner | `blocks/engine.rs::execute` and attempt timeout bound the whole Agent future; `Invocation::run` further bounds sandbox work by action spend and deadline. `Sandbox::exec` bounds admission/checkout/execution, and cancellation drops the checkout. Blocking filesystem work can settle after caller cancellation, with guard, workspace, and admission lifetime preserved. No published success can be manufactured by a late scratch write. |

## Acceptance matrix

| Required property | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| fs.write and sibling writers share exclusion and byte accounting | `tools.rs:310–320`, fs.extract at 291–306, code.run at 386–413; `workspace.rs:692–705,1033–1119` | `workspace_writes_private_scratch`; public `tool_limits_release_capacity_and_preserve_provenance` sends two concurrent writes | PASS |
| Overwrite counts the replaced regular file once; refusal preserves old bytes | `write` checks target metadata, checks `work_room + replaced`, writes a temporary file and persists by rename | Sequential near-budget replacement and over-budget new-file refusal in `workspace_writes_private_scratch` | PASS |
| No unsafe parent traversal or host overwrite | `write` rejects invalid relative segments, checks every existing parent with `symlink_metadata`, rejects nonregular targets before mutation; all reachable writers share the private workspace lock | Unit cases cover traversal, input paths, file parent, directory target, parent link and target link; outside directory stays empty | PASS |
| Refused over-budget writes do not create parents | Budget check precedes directory creation and staging | Unit small-budget nested-path assertion checks that even first parent was not created | PASS |
| Snapshot is consistent with concurrent writes and stays immutable | `freeze` holds the same lock during blocking snapshot; `Publications::path` uploads owned frozen content | `cancelled_snapshot_keeps_writer_exclusion`; public publication journey asserts later scratch edits do not change downstream accepted content | PASS |
| Cancelled blocking operations retain exclusion and bounded workspace ownership until settled | fs.write moves guard and workspace into `spawn_blocking`; freeze uses the identical ownership mechanism; waiting on writer is cancellable | Snapshot unit test cancels with the read queued behind a single blocking thread and proves the guard remains held | PASS |
| Existing Python spend, admission and deadline boundaries remain in force | `Reservation::take/Drop`, `Invocation::run`, `Sandbox::exec`, engine timeout; workspace admission uses try_acquire_owned | Source inspection; task claims journey execution, not independently rerun here | PASS |

The new `write` operation is an extension of the existing workspace owner and reuses its writer, budget measurement, confinement model, temporary-file mechanism and admission lifetime. The invocation adapter adds no alternate scheduling, snapshot or storage owner.

## Findings and proof limits

No proposed findings. The source establishes the shared exclusion and ownership invariants without requiring additional concurrency machinery.

This was a static specialist review; no tests or broad gates were rerun. Existing implementation execution claims are not independent execution evidence. The public limits journey actually checks oversized JSON refusal, a paged read, two small concurrent writes, full-file publication and exact provenance. Its name does not prove capacity exhaustion/release or cancellation of a queued writer. The near-budget overwrite and unsafe-parent tests are sequential unit proof. The cancelled-snapshot test proves the existing blocking guard-transfer mechanism, not a separate cancellation test of fs.write. These limits do not establish a reachable source defect and are recorded without inventing a new hardening requirement.

Excluded from this boundary: authorization/revocation policy, durable acceptance/replay accounting, SDK projection parity, and whole-task test completeness; assigned specialists own those contracts. No synthetic host load, implementation changes, commits or pushes.

## Packet integrity

SHA-256 before and after review (unchanged):

| File | SHA-256 |
|---|---|
| `spec.md` | `5af4975a59d8aaba3702e5c2fde40d34b074760456a293cf61f30cd907a5e683` |
| `output-publication.md` | `a02ceaf9c6f40f07e5995925161c64dc38fb7563471e993d3212810533b773e9` |
| `execution-state-and-durability.md` | `08c4bf050e2f73cf5bb4eabcd91d412c3e73cf727b1a415165d504e9cb804d9f` |
| `tasks/TASK-002-agent-tooling.md` | `7c1773159a2d348d097a46fd5876f748bb32d1569447b102b664657117690bbe` |
