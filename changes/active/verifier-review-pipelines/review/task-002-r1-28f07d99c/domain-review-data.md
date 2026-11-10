# DATA remediation review — TASK-002 R1

Verdict: **FAIL**. FIND-TASK-002-1 and FIND-TASK-002-2 are closed in this boundary. One narrowly retained proof gap remains under FIND-TASK-002-5: DATA-R1-001 (O5.2). This is not evidence of a production replay defect.

## Immutable subject and boundary

Prior candidate `75c9bfa9e07163099eac40d5a556c4447c6a519d`; candidate and observed HEAD `28f07d99cee44669cd4a8209d1f096ee19c5dfd4`; cumulative base `990a85dc7155ff0d4de1acc17e4aeaa7a42d582f`. Inspected the actual prior-to-candidate diff and the affected unchanged state producers/consumers. Only this report was written.

Scope: original-call/name/digest receipt identity; accepted invocation/publication custody through forget, Busy, recovery and attempt reconstruction; changed server lifecycle proof O5.1–O5.3. Prior findings and execution claims were hypotheses, not conclusions. No original whole-task rediscovery was performed. No other current reviewer report was read.

## Authority, source coverage and carry-forward

Read the task-review skill, immutable subject, original TASK-002 and remediation obligations; root rules, changes/spec-driven-development.md, agent_docs index/rust/maintainer-style, architecture authority map; publication and durability supplements and the applicable durability/retained-execution sections of wyrd-design.md (1867–1902, 1930–1943) and bifrost-design.md (635–652). The approved contract conserves attempts and exact accepted references, requires acknowledgement before publication success, and explicitly calls for the lost-reply/replay proof. Revision-21 draft waits are excluded.

CodeGraph was used first through `mise exec -- codegraph explore` for RunState receipts, checkpoints/forget, recovery/engine and Agent/evidence paths. Inspected existing mise task inventory. No nested AGENTS.md was found in the Rust server/Vala/Rust-SDK trees covered here.

Production and caller coverage:

- `verification/state.rs:748–834,864–953,961–1063`: shared receipt preflight/acceptance, durable ticket/fence ordering, change, invocation begin/forget/settle. Both receipt entry points use the corrected single helper; no second replay authority was added.
- `hosted/publications.rs:100–331`: JSON acceptance and path preflight before freeze/upload; accepted canonical arguments/call identity remain in the same state. File permission reuse introduces no new state or upload owner.
- `vala-verify/src/blocks/engine.rs:319–397,750–794,832–895`: maximum recorded attempt reconstruction, settlement restoration, and every production forget path (requirement Busy/definition failure; action Busy; action definition failure). `runner.rs:799–849,1690–1725` binds the same recovered state and maps Busy to queue deferral.
- `pg_durable_verifier_work.rs::Leased::{writer,accepted,revision,reclaim_on}` (180–329): tests recover through real Bifrost accepted state and a new real PostgreSQL queue claim; they do not merely reuse a live snapshot as recovery.
- New server Agent seam cases, `agent_engine`, `HeldRevisions`; SDK replay fixture and assertions; Agent tool-result delivery in `skald-agent/src/loop_runtime.rs:555–695` and AgentExecutor refusal/producer propagation.

Carry forward the prior DATA report and validated ledger's source-backed acceptance for unchanged snapshots/path safety, JSON/null, host producer construction, ordinary direct/queued composition, exact-descriptor mount/storage validation, and no duplicated store/loop/recovery owner. The receipt and forget bodies are reopened because their corrections affect those identities; O5 is reopened because the earlier review explicitly required this seam proof. No changed dependency invalidates the other retained invariants. Existing concurrency/system evidence remains applicable: these corrections add no IO/lock/task/queue mechanism, and this review covers the changed attempt conservation and disconnect consumer. No additional missing specialist scope was identified within this boundary; File authority/audit remains assigned separately.

## Closure and evidence matrix

The following rows preserve the remediation's exact observation IDs. Runtime executions below are supplied claims from its Remediation Evidence, not reviewer reruns. PASS reflects inspected source/assertions together with that supplied execution; a test title alone supplies no proof.

| Obligation / required observation | Implementation evidence | Actual verification assertion inspected | Result |
| --- | --- | --- | --- |
| O1.1 accepted call ID/name a cannot acquire name b; typed publication_name refusal in Agent loop | `state.rs:966–978` searches same producer plus name OR call ID, then requires exact call/name/digest | Rust `hosted_agent_tools.rs:1080–1113`: four original-ID answers, last two typed conflicts, exact listing excludes second name | PASS |
| O1.2 changed value conflicts and original remains retained | Same shared receipt helper; existing accepted entry is untouched | SDK `:1089–1095,1135–1145`: typed conflict and retained JSON `once` | PASS |
| O1.3 identical replay, other-call name conflict, fresh-attempt reuse preserved | Exact-match replay; competing-call conflict; full Producer equality preserves attempt isolation | `state.rs:1436–1470`: exact replay kind, competing call refused, attempt 2 accepted, exact two retained values; SDK per-attempt listing/readback | PASS |
| O1.4 restored entries follow same rule | `RunState::at` reads the existing AcceptedPublication identity fields; same helper serves both entry points | `a_call_keeps_its_original_publication` rebuilds state from accepted snapshot and asserts replay, changed name/value, competing call and new attempt | PASS |
| O2.1 post-publication Busy conserves attempt, old content, and next-attempt name reuse | `RunState::forget:1022–1037` keeps attempts with matching step/attempt publications; engine reconstructs maximum attempt from invocations | Server `:1600–1635` reads accepted/reclaimed attempts; draft succeeds as attempt 2; both values remain inspectible via Bifrost-backed reads | PASS |
| O2.2 no remaining retry does not restart attempt 1 | Engine `:838–844` returns interrupted before dispatch when recorded attempts exceed retries | Server `:1644–1656`: failed step, attempts=1, two queued mock responses remain untouched, old retained value still reads | PASS |
| O2.3 before-effect Busy remains free deferral | Guard only retains an invocation with accepted publication; existing unsettled removal remains | Server `:1600–1606` excludes early from accepted attempts; after releasing admission `:1658–1663` succeeds with attempts=1 and consumes pending responses | PASS |
| O2.4 action Busy, unavailable-definition and requirement forget callers follow single rule | Engine calls at `:786,878,882` all route through `:390–397` to RunState; requirements cannot publish; settlement records remain protected by existing retain predicate | Static full caller/owner inspection; no per-caller guards or changes to scheduling | PASS |
| O5.1 real Agent content, inventory unavailable, recovered consumer exact bytes, no settled-producer rerun | New test constructs real HostedTools/AgentExecutor bound to recovered RunState; ordinary File inventory lookup is refused while publication descriptor reads remain available | Server `:1745–1787,1820–1905`: actual Busy, recovered settled draft alone, exact inventory-unavailable reason, expected step/attempts, two expected final mock responses only, mounted KEPT bytes and consumer output digest/byte count | PASS |
| O5.2 reply lost after acceptance replays original call/attempt without acceptance/upload | Shared receipt/source ordering is sound; test replays recovered original `call_1`/draft/1 with absent scratch and inaccessible local evidence directory | `:1797–1817` proves original File receipt with unchanged revision and no scratch/upload dependency. It does **not** lose the original reply: `:1725–1750` first consumes the successful publication and subsequent drafter completion before the consumer Busy | FAIL — DATA-R1-001 |
| O5.3 deterministic direct disconnect starts no later Agent call, no sleep | `dropped_agent_work_makes_no_later_call` drops the owned engine future while publication ACK is held; shared Scribe future may complete but cannot own a later Agent turn | `:1960–1979` first waits for attempt and publication Scribe arrivals with exact mock counts; drops run, releases gate, verifies two remaining responses and no `after` publication after reclaim | PASS |

Supplied claims include owner receipt test PASS, real Agent replay journey RED→PASS, retained-Busy server case RED→PASS, and both lifecycle server cases PASS. Their successful execution cannot establish an absent fault assertion.

## Proposed finding

### DATA-R1-001 — O5.2 still simulates an ordinary later replay, without losing the first reply

**Classification:** MISSING. **Prior finding:** retain only the O5.2 portion of FIND-TASK-002-5.

**Obligation:** remediation FIND-5 explicitly distinguishes successful duplicate receipts from losing a reply after acceptance, then replaying the original call/attempt. O5.2 repeats that required observation. This is an explicit scoped proof deliverable, not speculative failure hardening.

**Location/evidence:** `crates/wyrd/wyrd-server/tests/integration/pg_durable_verifier_work.rs:1725–1750,1797–1817`. The script enqueues the publish turn, a successful final drafter reply, and the consumer Busy turn. It runs until Busy and asserts all three responses were consumed; recovery then finds the drafter settled. The normal Agent loop awaits `invoke_call` and builds its tool result before the next turn (`skald-agent/src/loop_runtime.rs:632–695`). Therefore the original publication receipt already reached the Agent loop before recovery. The later direct Publications::path invocation obtains a second receipt for an already-observed successful call. No code drops or suppresses the first response after acceptance and before delivery.

**Consequence/materiality:** exact-descriptor recovery and replay behavior are positively covered, but the specific previously missing uncertain-response seam remains untested while the test comments and evidence claim it. This is narrower than the prior FIND-5: no runtime defect or new protocol decision is established. The approved remediation explicitly requires this observation, so a green successful-replay test cannot close it.

**Smallest testable correction:** use the existing server invocation/publication seam to withhold/drop the first publication response after confirming accepted state, then retry the original call/producer against that retained state. Assert the original receipt, unchanged accepted entry/revision and no new upload with scratch unavailable. Keep the existing real-Agent recovered-consumer case for O5.1; no public resume API, scheduler, production replay cache, new state store or broad restart matrix is needed. A bounded extra seam assertion is sufficient; repeating all journeys is not.

## Verification limits

Reviewer performed static diff/source/assertion tracing and `git diff --check` (clean); no Cargo, generator, benchmark or runtime commands were run. Candidate HEAD was rechecked unchanged. Supplied execution results are not independently reproduced. Historical production-before-RED is carried as the recorded deviation and is not repaired by this review.

Skipped unrelated whole-task rediscovery and broad reruns. Remaining risk is an explicit lost-reply proof gap, not an observed production data-integrity failure.
