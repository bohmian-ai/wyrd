# Data integrity domain review — TASK-002

Verdict: **FAIL**. One material publication replay-integrity finding, DATA-001.

## Immutable subject and scope

Base `990a85dc7155ff0d4de1acc17e4aeaa7a42d582f`; candidate and observed HEAD `75c9bfa9e07163099eac40d5a556c4447c6a519d`. Reviewed the recorded base-to-candidate diff and affected unchanged owners. No source edits or test modifications. First TASK-002 review; no prior TASK-002 acceptance carried forward.

Boundary: provider call → recorded hosted publication adapter → existing Publications/File lifecycle → accepted RunState → downstream mount and recovered-state consumption. This review does not reopen unrelated Task-001 recovery, terminal delivery, or Bifrost implementation debt. The finding below concerns an explicit replay obligation of the newly exposed hosted tool and the specific unchanged dependency it invokes.

## Authority and source coverage

| Authority / invariant | Inspected evidence |
| --- | --- |
| Root AGENTS.md; changes/spec-driven-development.md; agent_docs/maintainer-style.md and rust.md; architecture authority map | Existing owner reuse, immutable review, no second storage/replay implementation, meaningful integrity findings only. No nested AGENTS.md exists in the Rust source/SDK trees inspected. |
| TASK-002 publish dispatch and Proofs 2–3 / lifecycle; spec approved baseline 18 REQ-046/047, AC-039/040; output-publication.md | Exactly one path/value, explicit null, host producer/call identity, immutable snapshots, same-call replay and conflicting-argument refusal, separate attempts. Spec frontmatter is revision 21/draft with approved_baseline_revision 18; unrelated wait/approval draft behavior was excluded. |
| execution-state-and-durability.md, accepted publication custody and recovery sections | One existing File upload lifecycle and RunState acceptance/recovery owner; exact descriptors retained from completion onward; no inventory rediscovery by successors. |
| architecture/wyrd-design.md:1491–1507, 1763–1769, 1867–1899, 1930–1943; architecture/bifrost-design.md:635–652 | Named/collection mount semantics, exact completed association, acceptance acknowledgement before receipt, same state across direct and queued paths. |
| TESTING.md | Journey assertions assessed separately from lower-level/source evidence; implementation execution claims are not treated as review reruns. |
| skald-agent/src/loop_runtime.rs:555–632, 991–1009; skald-tool/src/trait_.rs:24–35 | Original provider call ID reaches invoke_call without duplicate-ID rejection; no second execution loop introduced. |
| vala-verify/src/blocks/agent.rs:357–385, 444–478; blocks/engine.rs:831–895; blocks/tools.rs:251–336 | Host attempt becomes Producer; selected ToolSession and RecordedTool forward original call identity. Current action execution is plain-step only; item/iteration indices are None, consistent with this task's scope. |
| wyrd-server verification/hosted/{blocks.rs,tools.rs}; publications.rs:85–319,351–379,393–418 | Same Arc<RunState> bound to tools and evidence; typed argument dispatch; existing snapshot, upload completion, inventory ACK, accepted descriptor and receipt workflow reused whole. |
| verification/state.rs:746–855,883–981 | Existing state serialization/acceptance, durable-before-receipt, budget accounting, producer/name/call/digest matching and recovered replay data. |
| hosted/workspace.rs:85–165,223–274,554–660; hosted/evidence.rs:109–145 | Exact producer/name lookup, JSON serialization, manifest-to-descriptor file reads and preserved directory paths, writer-excluded snapshot. Successors obtain publications from the bound in-memory RunState and exact StoredObjectRef, not ordinary input File inventory discovery. |
| Rust SDK tests/integration/hosted_agent_tools.rs:140–185,310–363,790–974,998–1090,1434–1514 | Real Agent-loop file/tree/JSON/null receipt/readback, immutable post-publication content, fresh attempts, identical-call replay, collection discovery, queued restart with settled drafter reuse. |

CodeGraph was consulted first through `mise exec -- codegraph explore` for the publication, state, workspace, invocation and journey owners. Focused source inspection expanded its navigation results. `mise tasks ls --hidden` was inspected; no broad aggregate was run.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence inspected | Result |
| --- | --- | --- | --- |
| Thin adapter; no separate store/upload/replay mechanism | Invocation::publish builds PublishCall and calls existing Publications::path/json; existing complete uses file_upload_init → put_object → reconcile_upload → inventory record; state acceptance remains in RunState | Diff has no new uploader, publication ledger, recovery owner or snapshot implementation | PASS |
| Host owns execution/step/attempt and original call ID | HostedBlocks shares execution Arc; AgentExecutor constructs Producer from BlockCall; original provider ID passes through loop/RecordedTool/Builtin | Retry journey asserts separate attempts and earlier content; forged producer field is refused | PASS |
| Exactly one source; explicit JSON null | PublishArgs denies unknown fields; present deserializer preserves Some(Null); Invocation::publish matches exactly one source | Publication journey checks null readback and both/neither-source refusals | PASS |
| File/tree snapshots retain exact bytes/paths/descriptors; later scratch writes cannot change accepted content | Existing freeze retains writer guard through blocking snapshot; existing Publications completes files then accepts manifest with exact objects; published verifies descriptor tenant/digest/length and stored bytes | Publication journey checks arbitrary tree names/empty directory metadata, post-publication scratch mutation, public manifest/download digests and downstream content | PASS |
| Receipt follows durable acceptance, including replay | Adapter awaits existing Publications; accept_publication waits receipt_once_accepted → durable and confirmation before success | Source proves acknowledgement ordering; implementation reports successful publication/lifecycle journeys. No ACK-fault test rerun by this reviewer | PASS |
| Same-call identical arguments replay once; different-call same-name conflict; new attempts independent | Existing state compares stored call/digest within producer/name; new adapter forwards original identity | Identical replay and fresh-attempt journey assertions; same-name conflict asserted in publication journey | PASS |
| Conflicting arguments under the original call ID fail, including changed name | Existing receipt searches only producer/name, so a changed name bypasses stored call/digest identity | Existing replay journey repeats identical arguments only; no changed-name case | **FAIL — DATA-001** |
| Successors and queued recovery consume exact accepted references without rediscovery | Same bound RunState → TenantEvidence::publications → Mount::publication → TenantEvidence::published(exact descriptor); ordinary File inventory discovery is a different branch | Lifecycle journey restarts after drafter settlement and observes exact report/null with no drafter rerun. Inventory failure is not injected in this new journey; absence of an inventory call on the publication branch is source-backed | PASS |

## DATA-001 — Changing a publication name bypasses original-call conflict detection

Classification: **INCORRECT**. Violated obligation: output-publication.md Publish contract explicitly requires “conflicting same-call arguments fail”; REQ-047 incorporates those receipt rules, and TASK-002 owns binding/dispatch of the original tool-call identity and replay behavior.

Changed seam: `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:343–368` (`Invocation::publish`). Shared cause: `crates/wyrd/wyrd-server/src/verification/state.rs:960–979` (`receipt`), used by both `publication_receipt` and `accept_publication`.

Source-proven reachable sequence in one producer attempt:

1. Provider emits `publish`, call ID `p1`, arguments `{"name":"a","value":1}`. The loop forwards `p1` unchanged; state accepts producer/name `a`, call `p1`, and the canonical argument digest.
2. A subsequent provider call reuses `p1` with `{"name":"b","value":2}`. Duplicate IDs are accepted by the current loop (the existing replay journey deliberately relies on this).
3. `receipt` searches only entries matching the incoming producer **and name**. The existing `a` entry does not match `b`, so it returns None without comparing the already-used call ID or arguments.
4. `accept_publication` appends a second AcceptedPublication, persists it, and the new adapter returns success `{name:"b",kind:"json"}`. Both entries remain visible in collections/recovery. The path branch has the same acceptance gap and can additionally complete files before the second acceptance.

Consequence and materiality: a replay identity can acquire two distinct durable effects and receipts in its original attempt, rather than refusing the changed arguments. This breaks the explicitly required publication idempotency contract at the new hosted tool boundary and can add unintended retained content to downstream collections. The input is an adversarial replay case expressly covered by the task's contract, not a request for additional general provider hardening.

Sibling owners/writers checked: both Publications::receipt (path replay probe) and Publications::accept (JSON and file/tree acceptance) call the same RunState receipt helper; the production search found no other direct accept_publication/publication_receipt callers. The correction belongs there, where the accepted entries already retain producer, call ID, name and argument digest. A new adapter cache or store would duplicate durable authority and would not survive recovery.

Smallest testable correction: enforce both existing attempt/name uniqueness and original-call argument identity against the existing accepted publications in the shared RunState owner. Within the same producer, an already accepted call ID may replay only its exact original name and arguments; a changed name/source/value must conflict, while a different call ID claiming an occupied name also conflicts. Preserve fresh-attempt isolation and existing acceptance-before-receipt behavior. No new persisted shape, index/store, uploader or lifecycle is necessary.

Focused closure proof: extend the existing hosted Agent replay journey with sequential same-ID changed-name calls and assert a typed conflict plus only the original publication/receipt retained. Keep its identical replay, different-call same-name refusal and fresh-attempt assertions. A focused owner regression can cover restored accepted entries using the same existing data, without another recovery harness.

## Verification limits and packet identity

Static review only. I inspected executable assertions and traced source; I did not rerun Rust/Postgres, Python, TypeScript, MCP, format/lints or broad gates. Implementation Evidence reports seven Rust journeys and language/MCP closure passing; those are supplied claims, not this reviewer's execution results. No claim is made to have independently fault-injected WAL acknowledgement, inventory outages or cloud storage.

Authority SHA-256 at first read (rechecked unchanged before delivery):

- spec.md: `5af4975a59d8aaba3702e5c2fde40d34b074760456a293cf61f30cd907a5e683`
- output-publication.md: `a02ceaf9c6f40f07e5995925161c64dc38fb7563471e993d3212810533b773e9`
- execution-state-and-durability.md: `08c4bf050e2f73cf5bb4eabcd91d412c3e73cf727b1a415165d504e9cb804d9f`
- tasks/TASK-002-agent-tooling.md: `7c1773159a2d348d097a46fd5876f748bb32d1569447b102b664657117690bbe`

No unrelated Task-001 debt or optional cleanup was added. Risk requiring correction: conflicting original-call arguments can currently create an additional accepted publication.
