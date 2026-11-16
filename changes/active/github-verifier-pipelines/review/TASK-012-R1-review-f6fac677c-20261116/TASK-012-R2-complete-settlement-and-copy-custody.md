---
id: TASK-012-R2
kind: remediation
status: ready
spec: SPEC-github-verifier-pipelines
spec_revision: 15
parent_task: TASK-012
remediates: [FIND-012-1, FIND-012-2, FIND-012-6, FIND-012-7, FIND-012-8]
requirements: [REQ-015, REQ-017, INV-003, INV-004, INV-005, INV-006]
depends_on: []
---

# Complete settlement and copy custody

A verification may stop dispatch at its original deadline or speculative discard, but must still settle started host work before accepting the corresponding progress. Queued terminal and native-media copies must remain accounted while retained, and a cancelled gateway cold load must relinquish its request-owned cache state. Complete these existing lifetimes without replacing the engine, resource ledger, provider runtime or cache workflow.

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004.
Approved specification: changes/active/github-verifier-pipelines/spec.md, revision 15.
Original task: changes/active/github-verifier-pipelines/tasks/012-integrate-code-mode.md, including Maintainer scope direction.
Original base: c24e1db8f6641cca9912376a4587e63e2fe74dd1.
Prior reviewed candidate: 9016393004dc0eb319075245c5f6a1d7a6c2cd46.
R1 candidate and this task's immutable input: f6fac677cd71208871fa5c21e4035085026a99c4 on task/github-verifier-pipelines/TASK-012-integrate-code-mode.
Prior remediation: review/TASK-012-first-review-20261116/TASK-012-R1-preserve-settlement-and-resource-custody.md in this packet.
Current diagnosis and independently validated ledger: findings-validation.md and verdict.md beside this task. Earlier accepted behavior and R1-3/4/5 closure remain acceptance context.

## FIND-012-1: preserve active settlement, not only queued futures

R1 now transfers queued settlement futures from a dropped ToolCallsOwner into HostWork, and the engine/queued runner await HostWork before deadline results. However, blocks/tools.rs:190–205 removes a settlement from its vector before awaiting it. When an attempt is already inside ToolCallsOwner::settle, the active future belongs only to that cancellable stack; ToolCallsOwner::drop at :142–150 sees no entry to transfer. The same loss occurs if a speculative disposition is awaiting HostWork when the execution deadline drops it. Concurrent HostWork settlers can also treat an empty vector as complete while another waiter still owns pending work.

The production path is HostedTools::open's TaskTracker settlement → AgentExecutor registration → engine attempt/check settlement → speculative disposition or original deadline → queued compute/terminal. Hosted blocking extraction continues after its waiter is dropped. An authored attempt timeout followed by original execution expiry, or a speculative discard during settlement, can therefore accept progress before started work ends. This remains OBL-3/R1-1's original dropped-future defect. The new tests hold the executor pending before it starts draining, or use a runner script that never begins a competing drain; their passing results do not cover this state.

Complete cancellation-safe ownership at the existing ToolCallsOwner/HostWork settlement boundary. Active completion must remain owned and observable until the underlying work ends, even if a waiting future is dropped, and another waiter must not infer completion merely because someone else is polling the work. Preserve transfer to the enclosing engine/runner and the existing TaskTracker completion source. Fixing only runner, disposition or individual tools would repeat guards around the same shared cause. Do not add a scheduler, service, detached execution workflow, persistence wait or timeout extension.

Proof must first establish that settlement has begun, then expire the original deadline or discard the speculative step while work stays held. Also cross the original deadline during disposition settlement and exercise concurrent dispositions with held work. No corresponding result/checkpoint/disposition or queued terminal may outrun its required work. After release, preserve DeadlineExceeded classification, withheld discarded output, spend and producer identity.

## FIND-012-2: account returned terminal decisions through their consumer

R1 correctly charges Held.live, frozen revisions and publication/restoration reads. Terminal reads remain a sibling omission. state.rs:157–163 Decision::of clones TerminalSelection, which owns the variable-size ExecutedOutcome. RunState::decision at :820–821 returns that clone without a charge. RunState::decide at :924–946 charges the clone retained in Held.state but also returns an independently owned Decision. runner.rs:540–541 and :1595–1613 retain such decisions through conclude's persistence wait and subsequent result construction/delivery; no enclosing copy guard covers them.

An ordinary queued outcome with retained step/Agent details, or recovery of an already-selected terminal outcome, can thus hold two outcomes while the process ledger promises only one. Scribe or delivery backpressure prolongs this state. Frozen revision accounting ends at ACK and cannot account for a separate consumer copy that survives ACK. This is the terminal/read custody explicitly required by OBL-7, REQ-017 and R1-2, rather than a new result-format feature. Existing new tests prove live/revision/publication accounting but never hold this terminal read.

Extend the existing Charged/ByteCharge state-read mechanism to terminal selection and recovery. Obtain custody before creating an independent terminal copy and retain it through the decision/result handoff for as long as that payload is owned. Cover both newly selected and recovered decisions at their common owner; do not permanently inflate live-state accounting or retain retired checkpoint charges to conceal the gap. Existing move/ownership reuse may eliminate copies where it preserves first-selection/recovery semantics; otherwise independently retained payloads need independent custody. Account additional result materialization before allocation, retain its promise through reservation, and reuse the existing charged-transfer mechanism when the actual outbox owner takes over. Keep memory-first transitions, first-decision identity, persistence retry, ACK-before-retirement and queue fencing unchanged. No second ledger, state service, schema or synchronous persistence boundary is justified.

Use a sizable outcome to prove both initial and recovered terminal reads. Hold the returned consumer, acknowledge checkpoint revisions, and show its separate bytes remain charged and unavailable to a sibling; dropping it returns only its share. Follow the affected result handoff so the correction does not merely move the undercharge to the next persistence/delivery wait.

## FIND-012-6: cover simultaneous native-media encodings

R1's guard correctly crosses EvidenceReader::media, bind and the provider call. It holds only N raw bytes plus E base64 bytes, from hosted/evidence.rs:770–788. Agent binding at blocks/agent.rs:352–358 calls Prompt::bind_media_mut, which creates provider-owned encoded values per placeholder. Prompt::render and the Agent call path retain further request/message copies while the original bound Prompt remains alive (skald-spec/src/prompt.rs:936–1007,1771–1800,1911–1950; skald-agent/src/loop_runtime.rs:202–274,372–455). Even an ordinary single image/PDF can retain multiple E-sized representations beyond N+E; repeated placeholders enlarge the gap. The opaque guard's lifetime alone does not account for those copies.

This violates R1-6's explicit simultaneous-copy requirement and OBL-7/inherited REQ-027 on a supported provider call. The held-call test uses an Arc<()> guard and proves only lifetime; the hosted media assertion expects N+E. Neither can detect insufficient byte accounting or sibling admission while media remains resident.

Complete accounting at the existing runtime evidence/binding seam using ByteCharge. Admit a conservative peak for native-media representations before binding/rendering copies are made, based on the actual bound occurrence count and the existing provider/Agent path's simultaneously retained encodings. The validated one-PNG OpenAI path retains bound prompt, rendered template and seed messages (3P, where P is base64 length plus the 22-byte data-URL prefix), reaches 4P during Chat rebuild and 5P during Responses rebuild; also cover the existing tool-session/before_model hook phase and other supported provider shapes. Count the Arc-shared bound prompt only once and preserve independent transport/body charges. Keep custody through the corresponding call and release it on refusal, completion or cancellation. Reuse the existing binding and guard ownership; do not add a general allocation tracker or account unrelated Skald values. A guard that cannot cover the actual copies is insufficient; removing native media or changing provider output is also outside the correction. Pure MediaRef/wire types, citations, validation and provider formatting remain unchanged. This is bounded internal accounting, not a new public API/resource-policy decision.

Use the existing real binding/held-provider test seam with a ByteLedger-backed reader and a meaningful media size. Exercise one ordinary occurrence and repeated occurrences with capacity between the previous N+E promise and the actual peak. Refusal must precede unadmitted copies; with sufficient capacity, a held call must keep the conservative native-media charge and prevent an overlapping sibling promise. Retain completion, refusal and cancellation release assertions. Do not substitute guard strong counts for byte assertions.

## FIND-012-8: retire abandoned gateway load cells

G1's approved cold-load behavior remains required: the first configured call after restart loads through the shared GatewaySnapshots owner instead of being refused merely because its snapshot is absent. The new loads HashMap at snapshots.rs:109 is populated at :162–168; retirement at :177–183 happens only after get_or_init(...).await. If the last caller's original deadline expires during SQL, the initializer drops but the map retains its OnceCell indefinitely. reset/invalidate/background reload/warm do not remove it, and subsequent cached hits bypass retirement. Repeated ordinary cold-load cancellation across tenant churn therefore accumulates request-ended cells outside the bounded snapshot cache.

The retained state is newly introduced by G1; G1 itself is a human-approved addition, not a retroactive review finding. The defect violates preserved bounded gateway/process-state obligations (wyrd-design gateway quota enforcement and Bifrost resource/failure invariants). It is not a claim that one cancelled call exhausts memory. Multi-tenant cancellation and recovery are supported use, and completed requests must not leave a process-lifetime registry of abandoned load state.

Tie load-entry retirement to the existing GatewaySnapshots caller lifetime, including cancellation of the last participant, and remove only the matching cell. Preserve a cell while another caller still participates so same-tenant misses share one load/result. Normal completion/error retirement, retry after failure, caller deadline, warm memory hits and generation fencing remain intact. A cache entry retaining an ended request is the cause; a new LRU capacity/refusal policy, arbitrary tenant-load limit, detached loader, janitor or service is unnecessary and excluded. SQL-pool limits alone cannot retire completed request state. No new cold-cache refusal contract is authorized.

Hold a load, join a second same-tenant caller, cancel one and prove the surviving peer still shares that load. Cancel the last participant and show the matching entry is retired; repeat across distinct tenants and show no accumulation. Complete/error/retry and listener/invalidation races must retain existing results. Reuse the current gateway test seam and loader; no new generic cache harness is needed.

## FIND-012-7: finish mandatory documentation in this correction

The original enumerated omissions are fixed. R1 adds or materially changes further items without required rustdoc: outbox.rs:602,766,771 lack the actual same-ledger panic contract propagated from ByteCharge::merge; tools.rs:169 HostWork Debug::fmt lacks its formatter error contract; snapshots.rs:403's changed test lacks its assertion panic contract; state.rs:363's new Deref::Target and verification/mod.rs:619's changed Counted field lack item documentation. Independently validated particulars are in findings-validation.md. This is the mandatory AGENTS.md/agent_docs/rust.md gate, not optional style.

Document the retained declarations with their actual workflow/invariants/errors/panics, and apply the same gate to any items changed by the behavioral correction. Keep behavior and visibility unchanged. Batch these local corrections here; no new doc framework/check/test or style-only round is required. Source inspection and formatting provide proof.

## Preserved behavior, scope and exclusions

Work at the existing settlement, state/runner/outbox, evidence/binding and gateway snapshot owners and their affected callers/tests. Runtime-only changes may reach the actual provider/Agent seam where necessary to account media copies, but may not become a general provider refactor or new wire contract. Preserve the already closed workspace encoding, transient scratch and failed/abandoned cleanup fixes (FIND-012-3/4/5), successful staging bytes, atomic replacement, no-overwrite and process-lifetime custody of surviving files.

Preserve borrowed walk scheduling, dynamic pipeline identities, accepted spend/retries/deadlines, discarded-output invisibility, direct versus queue-only behavior, RBAC/audits/tenancy, one shared process ledger, memory-first engine progress, background group commit, ACK-before-retirement, lease fences and claim backpressure. Preserve G1's approved load-through and G2's independent sandbox-ceiling assertions. No migrations, new public controls, dependencies, queue policy, compatibility path or architecture rewrite.

Do not edit architecture/wyrd-design.md, the local tasks/README.md or untracked TASK-011. No benchmark, synthetic host load, multi-pod proof, gate or full journey suite. TASK-013/014 own combined-server qualification. PostgreSQL remains coordination time; accepted gateway server-clock admission remains accepted. Review is not authorization to commit, push, configure Git or stash. Route this task directly to wyrd-implement.

## Acceptance and focused verification

| Finding | Observable closure |
|---|---|
| FIND-012-1 | Already-active settlement survives waiter cancellation and remains visible to concurrent waiters; required engine/disposition/queued progress waits for held host work and preserves terminal/spend/output identity. |
| FIND-012-2 | Initial and recovered terminal consumer copies are accounted before allocation and through their actual handoff/ACK waits; independent release returns exactly once without changing live-state or durability behavior. |
| FIND-012-6 | Single and repeated native media bind/render representations are admitted before copying and charged through held calls; sibling promises cannot reuse those bytes; refusal/completion/cancellation release correctly. |
| FIND-012-8 | Last-caller cancellation retires only the matching load entry, peer cancellation preserves single-flight, ended requests do not accumulate cells, and successful/error retry and warm-hit semantics remain. |
| FIND-012-7 | Every retained listed and newly changed declaration satisfies the mandatory substantive rustdoc/error/panic gate. |

Use existing engine, blocks_agent, state, outbox, gateway and queued runtime test owners. Record exact new selectors before RED. Source inspection proves reserve-before-copy order; held local work/provider/SQL seams prove ownership. Cargo commands run sequentially. Postgres proof uses scripts/postgres/with-test-postgres.sh. No allocation framework, synthetic host load or new test-control surface.

Preserve and run these exact existing regression commands:

```bash
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::refused_attempts_keep_their_tool_spend)'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::execution_deadline_waits_for_started_host_work)'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::discarded_speculation_records_after_its_host_work_settles)'
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::admitted_items_charge_process_memory_until_written)'
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::charged_items_are_taken_over_without_a_second_charge)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::state::tests::held_changes_coalesce_into_the_next_revision)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::state::tests::live_state_stays_charged_through_acknowledged_revisions)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::workspace::tests::writes_charge_process_scratch)'
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::bound_media_stays_held_until_the_agent_call_ends)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_verification_runtime::deadline_terminal_waits_for_started_host_work)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli --run-ignored=all -E 'test(=pg_tests::init_keeps_the_gateway_managed_secret_key)'
```

Run each newly named regression exactly, plus only the narrow module lanes necessary for the final write set, then mise run fmt:check and the existing scoped Rust checks:

```bash
mise exec -- cargo clippy --locked -p wyrd-runtime -p vala-verify -p wyrd-server -p wyrd-testing --all-features --all-targets -- -D warnings
mise exec -- cargo clippy --locked -p wyrd-server --bin wyrd-server -- -D warnings
```

If the minimum media fix changes an additional actual Skald owner, include that touched crate in the scoped check and its narrow existing regression lane. Do not run unrelated packages, full journeys or gate for this remediation. Record RED cause, GREEN assertions, final commands and honest limits. The next review retains this immutable subject and prior accepted rows, checking these five corrections and their affected paths without restarting full discovery.

## Obligations

| ID | Promise (contract source) | Required cases | Planned proof |
|---|---|---|---|
| R2-1 | Started host work settles before any dependent progress, even when its settlement waiter is cancelled (FIND-012-1; TASK-012 OBL-3; R1-1) | active settlement survives a dropped waiter and transfers to the enclosing owner; concurrent waiters do not infer completion while another polls; original deadline during attempt (and the shared check) settlement and during disposition settlement; speculative discard during settlement; concurrent dispositions with held work; queued terminal waits; existing TaskTracker completion source kept; DeadlineExceeded, withheld output, spend and producer identity preserved after release | `vala-verify` tools/engine unit tests holding a settling toolset; existing queued `pg_verification_runtime` selector |
| R2-2 | Independently owned terminal decision copies are charged before allocation and through their handoff (FIND-012-2; REQ-017; OBL-7; R1-2) | newly selected and recovered decisions; charge survives checkpoint ACK and blocks a sibling; dropping returns only its share; live-state charge not inflated and retired checkpoint charges not kept; result materialization promised before allocation, held through reservation and taken over by the outbox; first-decision identity, memory-first transitions, persistence retry, ACK-before-retirement and queue fencing unchanged | `wyrd-server` `verification::state` unit test with a sizable outcome; runner source order; existing outbox takeover test |
| R2-3 | Native-media representations are admitted at their conservative simultaneous peak before copying and held through the call (FIND-012-6; OBL-7; REQ-027; R1-6) | one occurrence and repeated occurrences at a meaningful size; capacity between N+E and the peak refuses before copies; held call keeps the charge and blocks a sibling; Arc-shared bound prompt counted once; tool-session/hook phase and other provider shapes covered by the bound; release on refusal, completion and cancellation; transport/body charges, MediaRef/wire, citations and provider formatting unchanged | `vala-verify` `blocks_agent` integration test; `pg_hosted_files` production reader on a real `ByteLedger`; source inspection |
| R2-4 | Cancelled cold-load cells retire with their last caller (FIND-012-8; wyrd-design gateway quota enforcement; Bifrost resource/failure invariants; G1) | peer cancellation keeps single-flight sharing; last-caller cancellation retires only the matching cell; distinct tenants do not accumulate; complete/error/retry, caller deadline, warm hits, listener/invalidation races and generation fencing unchanged | `wyrd-server` `components::gateway::snapshots` unit tests |
| R2-5 | Every retained listed and newly changed Rust item has substantive rustdoc with Errors/Panics (FIND-012-7; AGENTS.md; agent_docs/rust.md) | outbox.rs send_reserved/stage_charged/stage_held panics; HostWork Debug errors; snapshots test panics; Charged Deref::Target; Counted field; items changed by R2 | source inspection, `mise run fmt:check`, scoped clippy |

## Evidence

Commits on `task/github-verifier-pipelines/TASK-012-integrate-code-mode` after `f6fac677c`: `0b297ec6d` (R2-4), `43af4b0ab` (R2-1), `f5a412ccf` (R2-2), `d3aed3fc7` (R2-3), `d47ccbe50` and the evidence commit (R2-5).

Reuse: R2-1 extends the existing `ToolCallsOwner`/`HostWork` list (each entry now stays listed behind a `tokio::sync::Mutex` until it resolves; no new owner). R2-2 reuses `Charged`/`ByteCharge` and `Reservation::stage_charged`. R2-3 extends the existing `EvidenceReader::media` guard with a `copies` argument; `bound_media_bytes` lives at the binding owner. R2-4 adds a `Participant` guard over the existing `loads` map.

RED (each run against the fix reverted, test kept):
- R2-4 `components::gateway::snapshots::tests::miss_loads_retire_with_their_last_caller`: with cancellation retirement disabled, `only the other tenant's live load remains` failed (cells accumulated).
- R2-1 `blocks::tools::tests::an_awaited_settlement_stays_owed_when_its_waiter_is_dropped` against the old pop-and-await drain: `a later waiter still waits` failed. The four new engine tests below all failed on the old drain.
- R2-2 `verification::state::tests::terminal_decisions_hold_their_copy_charge` with the decision charge dropped: `memory holds 67100 bytes, not 133396`.
- R2-3 `blocks_agent::bound_media_charges_its_resident_copies`: passing 0 copies gave `left: [0] right: [6]`; ignoring the occurrence count gave `left: [6] right: [12]`.

| Obligation and required cases | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| R2-1: dropped active waiter, concurrent waiters, transfer to HostWork | `Owed`, `drain`, `ToolCallsOwner::drop` in `crates/vala/vala-verify/src/blocks/tools.rs` | `tools.rs` test `an_awaited_settlement_stays_owed_when_its_waiter_is_dropped` | exact selector PASS | PASS |
| R2-1: deadline during attempt settlement; discard during settlement; deadline during disposition settlement; concurrent dispositions; withheld output; DeadlineExceeded | same | `engine/tests.rs` `execution_deadline_waits_for_an_attempt_already_settling`, `discard_during_settlement_waits_for_its_host_work`, `execution_deadline_during_a_disposition_waits_for_its_host_work`, `concurrent_dispositions_wait_for_all_held_host_work` | exact selectors PASS | PASS |
| R2-1: check settlement, queued terminal, TaskTracker source, spend/producer identity | the step and check paths both await `ToolCallsOwner::settle` (engine.rs attempt/check settlement), and the runner awaits `HostWork::settle`. Production `with_settlement`/TaskTracker are unchanged. Producer records are untouched | source; `refused_attempts_keep_their_tool_spend`, `execution_deadline_waits_for_started_host_work`, `discarded_speculation_records_after_its_host_work_settles`, `pg_verification_runtime::deadline_terminal_waits_for_started_host_work` | exact selectors PASS | PASS |
| R2-2: new and recovered reads charged before clone, survive ACK, block sibling, release exactly once, identity kept, live charge not inflated | `Held::decision`, `RunState::decide/decision/select/fail` in `state.rs` | `state.rs` test `terminal_decisions_hold_their_copy_charge`; preserved `live_state_stays_charged_through_acknowledged_revisions`, `held_changes_coalesce_into_the_next_revision`, `duplicate_selections_share_one_result`, `a_recorded_failure_is_never_superseded` | `verification::state` lane 16/16 PASS; `pg_durable_verifier_work::terminal_selection_and_inspection_use_run_state` PASS | PASS |
| R2-2: result handoff (persist wait, materialization, reservation, outbox takeover); recovery path | `conclude`/`deliver`/`fit` in `runner.rs`. The charged decision lives across `persisted(..).await`; the promise is taken before `result()` and kept across `reserve_until`, then handed over by `stage_charged`. Recovery now reads `state.decision()` after `RunState::at` | runner.rs source; `outbox::tests::charged_items_are_taken_over_without_a_second_charge` | source inspection; exact selector PASS | PASS (source-proven; see limits) |
| R2-3: peak promise before encoding; one and repeated occurrences; refusal before the provider; held call blocks a sibling; release on refusal/completion/cancellation | `RESIDENT_MEDIA_COPIES`, `bound_media_bytes`, `media_occurrences`, `bind` in `blocks/agent.rs`; `TenantEvidence::media` in `hosted/evidence.rs` | `blocks_agent.rs` `bound_media_charges_its_resident_copies` (48 KiB PNG); `pg_hosted_files::evidence_binds_native_media_only` (real `ByteLedger`: exact peak charge, refusal one byte short holds 0) | exact selectors PASS; `pg_hosted_artifacts` 18/18 PASS | PASS |
| R2-3: copy bound (Arc prompt counted once, render, seeds, Chat/Responses rebuild, before_model hook, other shapes); wire/formatting unchanged | doc on `RESIDENT_MEDIA_COPIES` follows the validated copy trace in findings-validation.md. The prefix bound covers Anthropic/Gemini bare encodings. No Skald, MediaRef or provider-formatting change | source inspection | `bound_media_stays_held_until_the_agent_call_ends` PASS | PASS |
| R2-4: peer cancellation keeps sharing; last-caller cancellation retires only its entry; no accumulation; error retry; warm hit | `Participant` and `share` in `snapshots.rs` | `miss_loads_retire_with_their_last_caller` | exact selector PASS | PASS |
| R2-4: invalidation/generation fencing, listener reset, caller deadline | `insert`/`fill_generation`/`reset` unchanged; caller deadlines drop the future, which is the cancellation case above | preserved snapshot tests | `components::gateway::snapshots` lane 5/5 PASS; `wyrd-cli pg_tests::init_keeps_the_gateway_managed_secret_key` PASS | PASS |
| R2-5: listed items and R2-changed items documented | outbox.rs `send_reserved`/`stage_charged`/`stage_held` Panics; `HostWork` Debug Errors; snapshots test Panics; `Charged` Deref::Target; `Counted` field; `Owed` field, `Participant`, `walk`, `fit`, `conclude`/`deliver` Panics, test helpers | source | `mise run fmt:check` PASS; both scoped clippy commands PASS | PASS |

Commands (all PASS, run sequentially): the eleven preserved regression selectors listed above; new selectors `blocks::tools::tests::an_awaited_settlement_stays_owed_when_its_waiter_is_dropped`, the four `blocks::engine::tests` selectors above, `blocks_agent::bound_media_charges_its_resident_copies`, `verification::state::tests::terminal_decisions_hold_their_copy_charge`, `components::gateway::snapshots::tests::miss_loads_retire_with_their_last_caller`; `vala-verify --lib --test integration` 114/114; `pg_hosted_files::evidence_binds_native_media_only` and `pg_hosted_artifacts::*`; `mise run fmt:check`; both scoped clippy commands; `git diff --check f6fac677c HEAD`.

Mechanical deviations:
- The R2-3 vala-seam test uses a test-local byte budget rather than `ByteLedger`. `vala-verify` has no `wyrd-runtime` dependency, and adding one is excluded. The production reader's charge on a real `ByteLedger` is pinned at the pg tier instead.
- `pg_hosted_artifacts` calls gained the new `copies` argument.

Limits:
- The R2-2 result promise is sized at the decision's retained bytes before `result()` builds the write. `fit` then adjusts it infallibly to the write's own weight, so any write bytes beyond the decision's size are charged right after they are built, not before.
- The handoff across the persistence and reservation waits is proven by source lifetime, not a runtime test. The runner has no lease-free seam, and the pg harness shares one noisy process ledger.
- The six-copy bound rests on the validated static copy trace; it is not measured.
- Non-goals stayed excluded: no migrations, dependencies, public controls, architecture edits, benchmarks or journeys. Pre-existing local edits to `tasks/`, `design/` and `.agents/` were left untouched and uncommitted.
