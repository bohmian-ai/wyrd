# TASK-002 system resilience review

Result: **PASS**. No material finding established in the assigned failure-propagation and recovery boundary.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
- Base: `990a85dc7155ff0d4de1acc17e4aeaa7a42d582f`.
- Candidate and observed HEAD: `75c9bfa9e07163099eac40d5a556c4447c6a519d`.
- First acceptance review; no carried-forward TASK-002 acceptance.
- Read the supplied subject, original TASK-002, publication supplement, durability supplement, applicable baseline requirements REQ-052–058/INV-017–019/AC-046–050, root rules, workflow and maintainer guide, architecture authority map, hosted-tools/direct-execution sections of `wyrd-design.md`, Bifrost system boundary and execution-state sections, operations deployment/readiness/recovery sections, and testing tier rules. The current spec frontmatter is revision 21/draft with approved baseline revision 18; this review does not apply unrelated draft wait/approval behavior.
- Spec SHA-256 before and after: `5af4975a59d8aaba3702e5c2fde40d34b074760456a293cf61f30cd907a5e683`.
- Task SHA-256 before and after: `7c1773159a2d348d097a46fd5876f748bb32d1569447b102b664657117690bbe`.

Only this report was written. Source was not changed. CodeGraph was used through `mise exec -- codegraph explore` before source navigation; gaps in its returned source were inspected directly.

## Deployed path and coverage

The logical server can consist of several role-targeted replicas; it remains one public surface. Per `architecture/bifrost-design.md:15`, Verification owns hosted blocks and queued execution, Bifrost owns the sole process Scribe outbox, and the server supervisor owns their lifecycle. `HostedBlocks` holds one process-wide workspace semaphore and Python sandbox pool (`hosted/blocks.rs:51–59,96–108`). A tool invocation does not start another supervisor, worker queue, Agent loop, publication writer, or readiness owner.

| Surface inspected | Caller-to-owner path and relevant behavior |
|---|---|
| Direct runtime | `components/verification/service.rs:492–550` creates fresh request-owned RunState, passes it to HostedBlocks and engine checkpoints, and awaits the engine in the request future. Capacity failure maps to a request ServiceUnavailable response (`:566` onward); it does not terminate the process. |
| Queued runtime | `verification/runner.rs:700–726` recovers accepted state; `:800–849` passes the same Arc to HostedBlocks and checkpoints and restores recorded settlements. `:1363–1375` races execution against lease loss and shutdown abandonment. |
| Agent invocation | `vala-verify/blocks/agent.rs:347–443` opens the existing ToolSession, resolves declared tools, drives the existing Agent, and races process-capacity refusal. Step execution supplies the engine attempt (`:451–473`). |
| Tool dispatch and recording | `skald-agent/loop_runtime.rs:632,699` invokes the tool with the provider call ID within the existing bounded concurrent stream. `vala-verify/blocks/tools.rs:248–335` forwards that ID through the existing recorder, keeping the same failure/size/refusal handling. |
| Shared publication state | `hosted/blocks.rs:151–178` binds both TenantEvidence and HostedTools to the same RunState. `hosted/tools.rs:342–369` derives PublishCall and awaits existing Publications. `hosted/publications.rs:112–309` reuses freeze, File completion, receipt replay, and run acceptance. |
| Successor content | `hosted/workspace.rs:110–145` selects accepted producer/name from TenantEvidence; `hosted/evidence.rs:117–144` reads accepted state and the retained exact descriptor, without inventory rediscovery. |
| Resource ownership | `hosted/tools.rs:153–193,307–321` binds workspace to the session, and moves workspace plus owned writer guard into blocking writes. `hosted/workspace.rs:78–88,169–182,296–309,550–593,1050–1124` owns workspace admission, temporary storage, lock, snapshots, byte validation, and atomic replacement. |

The actual new machinery extends existing owners: per-invocation producer/run binding and call-ID forwarding. Publication persistence, receipt identity, upload completion, checkpointing, recovery, and successor mounting remain in their existing owners. There is no separate execution-state lifecycle to reconcile after a crash.

## Failure and recovery assessment

| Credible path | What stops, what survives, and how service resumes | Proof assessment |
|---|---|---|
| Direct caller disconnects while a model reply is pending | Dropping the request-owned verification future drops the Agent loop and pending tool stream. The gateway call has its existing cancellation-on-drop boundary (`components/gateway/invocation.rs:472–510`). No queue promotion is added. Already staged/accepted state can remain retained; no new successor or model work is launched by an independent tool task. | New `hosted_agent_tools.rs:1434–1450` drops direct verification after the scripted second request is pending, then observes exactly two drafter requests beyond the reply delay. This tests actual SDK/server cancellation, not just cancellation of a local publication future. |
| Queued process replacement after a producer settled and before its consumer settled | The queue/lease owner admits a replacement; accepted Bifrost state restores the settled producer and charges the interrupted consumer attempt. Its retry mounts the original publication descriptors. Scratch is disposable and not required to reconstruct publications. | New journey `:1451–1510` flushes Bifrost, restarts the server, expires the prior lease, and proves consumer attempt 2 reads exact report/null content while producer request count stays unchanged. It proves restart at this selected cut, not every possible crash cut. |
| Lost tool reply / repeated provider call ID | Existing receipt lookup checks original producer/name/call-ID/argument digest. Path replay returns the stored receipt before touching scratch or uploading again. Another call reusing the same name conflicts. | New replay journey plus `hosted/publications.rs:136–148,240–262` and `state.rs:746–827,955–976`. The tool extension preserves the provider ID rather than manufacturing a new replay identity. |
| Scribe capacity/ACK dependency unavailable during publish | Existing outbox reservation and ACK wait remain bounded by the execution deadline (`state.rs:325–353`). `RunState::durable` shares pending acceptance without holding the memory lock over IO (`:900–952`). Failure breaks this execution's state; publish returns an error rather than success. A stopped waiter neither creates a new Agent task nor retracts an accepted write. | Source proof plus existing state tests; no new publication-specific retry queue or unbounded retry is introduced. The receipt in `tools.rs:369` follows awaited acceptance. |
| Lease loss/cancellation during queued tool work | Runner drops active execution on lease loss. Publication acceptance checks existing durable state and confirms the lease before returning success (`state.rs:814–827`). Late local blocking scratch work may finish, but owns its private workspace/lock and cannot itself publish or start a later model call. | New cancellation journey `hosted_agent_tools.rs:1224–1320` retains early publication, cancels while the next model reply is held, and proves no late publication/later Agent request. Existing acceptance is reused rather than replaced. |
| Inventory lookup unavailable after acceptance | Successor reads state-held exact descriptors; it does not repeat File inventory lookup. Storage unavailability still yields existing retryable evidence failure; integrity failure remains terminal. | `TenantEvidence::published` and Workspaces mount path above. Existing `pg_durable_verifier_work.rs:872–1004` explicitly first proves an inventory-backed read is unavailable, then mounts accepted content with that same caller. New restart journey drives the Agent successor path over this owner. |
| Workspace/Python admission exhausted | Workspace admission uses `try_acquire_owned` and reports Busy. Existing ToolSession refusal propagates Busy out of the Agent loop; engine forgets the capacity-refused attempt (`blocks/engine.rs:875–879`). Direct returns unavailable; queued `runner.rs:1707–1710` defers. No process abort/readiness transition is added. | Source-traced unchanged failure mechanism; new scratch tools participate in the same workspace owner. No broader availability promise is inferred. |
| Write is refused, fails, or caller is cancelled | Validation/byte budget precedes writing; replacement uses a temporary file and persist. Blocking write owns the Arc and writer guard, so dropping its async waiter cannot free the workspace permit or allow a concurrent writer before it finishes. Once blocking work finishes, RAII releases those resources. | `tools.rs:310–320`, `workspace.rs:1050–1124`; focused workspace tests cover refused writes, overwrite budget, unsafe parents. The new limits journey covers oversized JSON refusal and subsequent valid writes/read/publication. |

## Acceptance matrix for the assigned boundary

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| One shared runtime/state path for direct and queued Agent tools | HostedBlocks shared RunState binding; direct service and queued dispatch above | New lifecycle journey checks both paths | PASS |
| Direct disconnect starts no background Agent work | Request-owned engine/loop; no queue admission in direct path | Direct disconnect case checks requests after delayed reply | PASS |
| Publication success follows durable acceptance and original receipt identity | Invocation::publish → Publications → RunState acceptance; provider call ID forwarding | Replay journey; existing backend receipt tests | PASS |
| Queued replacement preserves accepted references and does not rerun settled producer | Runner recovery + restored settlements; exact descriptor mounting | Restart lifecycle journey, exact consumer bytes and producer count | PASS |
| Inventory unavailable does not force successor rediscovery | TenantEvidence::publications/published and Workspaces::open | Existing unavailable-inventory mount test plus new real Agent successor journey | PASS |
| Cancellation, deadlines, capacity refusal remain bounded to the operation/execution | Existing engine deadline/runner lease races, owned workspace guards and refusal mechanism | New cancellation/limits cases and owner source | PASS |
| Shared server remains available outside the affected runtime component | No new process termination or supervisor failure path; Busy maps to direct error/queued deferral | Caller/failure mapping source proof | PASS |

## Findings and limits

Material proposed findings: **none**. No optional resilience machinery or unrelated availability work is requested.

This was static acceptance review: test bodies and callers were inspected, and the original task records seven Rust journey passes and owner checks; those commands were not rerun by this reviewer. The restart proof uses a flushed cut and orderly test-server restart, so it does not independently qualify power loss during an unacknowledged write. That durability boundary remains the existing Task-001/Scribe owner, which this change calls without replacing. The test named `tool_limits_release_capacity_and_preserve_provenance` does not by itself exhaust a workspace semaphore or prove concurrent server availability; the latter assessment rests on the traced bounded owners and existing failure mapping, not its title.

Skipped broad aggregates, load generation, and unrelated deployment fault injection. Operational SLO/RPO/RTO qualification remains deployment-owned.
