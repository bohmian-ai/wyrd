# TASK-009 behavior acceptance review

Overall result: **FAIL**. The shared admission implementation is supported by source and existing integration evidence. Final-source gate proof is missing, and the specifically planned public Run/Window ACK-gate journey is not implemented as specified. Measurement/publication closeout is separately outstanding; it must follow the review stage in the order authorized by V13–V16.

## Immutable subject and scope

- Base: `6f38db16c524417301ffc4a5dc0b478daf997ed1`.
- Candidate: `f0ea3b4023e544d0f95e9b5e38d398105dbe2e33`; HEAD rechecked unchanged.
- Authority: approved `changes/active/benchmark-experiments/spec.md` revision 24; original `tasks/TASK-009-unified-queue-and-closeout.md` revision 2, including its acceptance criteria and V1–V18.
- First review, complete cumulative diff, all 35 changed files and all 15 commit patches/write sets. No prior acceptance carried forward. The starting `scope.md` was navigation, not proof.
- Root AGENTS, all four applicable nested AGENTS (SQL, Python, TypeScript, docs), `agent_docs/index.md`, Rust and maintainer guides, spec-driven workflow, architecture authority map, verification/queue sections of Wyrd design, Bifrost verification-table contract, security Postgres tenant binding, TESTING.md and benchmark methodology were consulted. CodeGraph was used before locating/reading code, followed by Git patches and precise source/caller searches.
- Static review only. No source changes, Cargo runs, gate runs, benchmark runs, commits, pushes or publication operations by this reviewer.

The maintainer's acceptance of the five L1 records on `19f4190a8` is honored. Driver saturation and their original labels are not reopened. Their source attribution is assessed independently of that accepted validity.

## Commit coverage

| Commit | Patch coverage and conclusion |
|---|---|
| `5ead5a633` | Gateway audit assertion filters the intended operation; cache budget expectation includes both existing caches; WAL corruption occurs after owner shutdown; audit barriers cover every cluster server; SDK three-table/staging expectations match actual owners; generated text changes correspond to existing source. No count weakening of the protected V7/V8 regressions. |
| `b6d856818` | Complete shared handoff, scheduler/group preparation, SQL prepare/accept split, additive generation migration, result marker and caller/test signature adaptations. The new production server path no longer calls early SQL enqueue methods. |
| `0ba3b2601` | Removes tenant field from exported writer span, extends test flush to admission settlement, updates owner contract and input docs. |
| `2c5848feb` | Adds table-scoped existing ingest barrier and initial scheduled ACK test; later strengthening inspected separately. |
| `51bc7d183` | Existing scheduler cancellation/commit assertions now test no-execution cursor settlement; group tests settle publication before asserting consumption. These reflect the superseded early-admission promise rather than retaining a second scheduler writer. |
| `4621e16ad` | Atomic rollback, genuine overlapping schedule acceptance, edited cursor, generation update/recreation, same-content group dedupe tests. |
| `20995edb2` | Background candidates share tenant lane 0, extending existing writer batching; test observes equal execution/cursor transaction IDs for schedule and group. |
| `929a1d911` | Publication refusal and failed SQL connection leave source work pending; retry keeps IDs/Trigger; accepted source selector stays fixed. Shared test setup factoring preserves assertions. |
| `33f202555` | Two replicas share queue/Bifrost; producer notification cannot wake the consumer; existing scripted execution isolates polling/claim proof. |
| `6e1b0a674` | Strengthens unrelated own-input ACK test through a real second tenant submission and adds SQL RLS/input-identity proof; helper extraction inspected. |
| `4778abf71` | Real-engine public transport journey exercises submitted and background admission/readback; bounded proof gap described in B-02. |
| `e9e3d95cb` | Shared outbox dispatch and wake calculation use one clock sample; deterministic regression covers the deadline crossing without sleeping/load. |
| `19f4190a8` | Evidence-only task amendment. Its assertions were checked against source/logs, not accepted as proof by themselves. |
| `da366eefe` | Borrows the existing hydrated Verifier spec; removes JSON round trip. `Observe::target` and bound caller lifetimes track the borrow. Source change follows the recorded green gate and measurements. |
| `f0ea3b402` | Existing paced upstream prebuilds identical buffered bytes once; request timing/recording and stream construction remain. Source change follows recorded gate/measurements. |

## Acceptance matrix

Source references are repository-relative. Verification references name assertions inspected, not new executions by this reviewer. Earlier run provenance and limits are in `verification.md`; external log directory is the scratchpad recorded there.

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-035 / AC-015: concrete producers and closed admission contract | SQL `Admission::{Submitted,Scheduled,Grouped}`; existing service enqueue, scheduler selector and GroupPoller prepare their existing payload shapes | Actual producer->Scribe->writer call paths and V1/V4/V18 assertions inspected | PASS |
| Only own successful input ACK admits; marker/output ACK cannot confer authority | `scribe_outbox.rs:239` private-field `Acknowledged`; only `ScribeSink::write` constructs it for non-lost `ExecutionInputs`; `SubmissionSink::Item= Acknowledged` | V1 holds input and admits unrelated second-tenant input; V4 permanently refuses both background inputs; V18 holds submitted/background write | PASS |
| Preparation leaves source state due and releases SQL before staging/ACK | `scheduler.rs:481,635` drops candidate transaction; settled skips/refusals retain existing commits; reservations precede preparation | V1 cursor/no-row assertions; V4 refused input leaves cursor and pending group unchanged | PASS |
| REQ-036 / AC-016: atomic queue/source transition, replay and concurrent activity | `accept_scheduled` inserts and conditionally advances expected cursor on same TenantConn; `accept_grouped` conditionally deletes selected UUID generation; existing unique occurrence/snapshot keys retained | V2 rollback, concurrent schedule contenders, changed cursor, updated and consumed/recreated pending row; V6 tenant and fixed identity checks | PASS |
| Frozen command survives ACK and retry | ScheduledAdmission/GroupedAdmission carry frozen target/authority/budget/window/digest; writer does not re-resolve background binding/authority | V4 terminates writer backend, edits Trigger and observes original IDs/Trigger after retry; selector readback unchanged after later source item | PASS |
| Existing group row only, additive migration/data preservation | `20261028000000_verification_group_generation.sql` adds NOT NULL UUID with PG random default; every existing activity conflict update obtains new token | SQL tests exercise existing/new/recreated activity rows; migration is new, not applied-file edit | PASS |
| REQ-036 / AC-017: bounded asynchronous preparation and common mixed-origin tenant transaction | Existing Outbox/reservations; `InFlightSet` RAII claim retained by Scribe/admission; background lane 0; `SubmissionSink::write` commits submitted and sorted background admission in one transaction | V3 blocks first admission while further schedule/group preparation succeeds and verifies same transaction for later mixed origins; V4 retries failed transaction | PASS |
| No lost-notification/singleton correctness dependence | Existing runner ClaimLoop, shared SQL leases, post-commit writer Notify hint; unchanged runner implementation | V5 runs submitted/scheduled/group on another replica without producer wake; existing V17 claims each queued execution once | PASS |
| REQ-037 / AC-018: Bifrost worker inputs, one engine, fixed input per retry | Unchanged `runner.rs:425` dispatch calls existing recorded/supplied InputResolver and drift checks before HostedBlocks engine; existing execution ID selects archived input | V18 real engine first attempts and group source IDs; V10 owner/readback regression; existing source-unavailable/trace behavior retained | PASS |
| Payload owners and stale lease fence; async settlement preserved | Existing ResultPayloadBuilder build; runner result stages only result payload, no second queued input; `settle`/Completions fence staging on applied lease | V9 stale-result assertions and V10 separate table readback; source path inspected | PASS |
| Direct immediate delivery, no added request SQL/ACK wait, 50 ms linger and capacity | Direct service logic unchanged; queued marker is origin-based; direct still reserves/stages after execution; outbox linger unchanged | V1 real direct HTTP execution returns with input held; V18 direct calls while archive held; SDK staging pressure tests preserve refusal/recovery | PASS |
| INV-001/002/003: real authority, tenancy, audit, source selection and durability boundaries | TenantConn/RLS admission, frozen accepted background authority, existing Scribe activity audit/attribution owner and canonical input resolver remain | V6 cross-tenant invisibility/refusal, V11 negative/readback journeys, inspected SDK 50-story log and older complete gate | PASS |
| REQ-038: no temp table/hold/infinity/PG-loader/recovery framework | Full source diff introduces only group generation column; production scheduler early queue writes removed; existing queue reader retained | Static cumulative diff and each commit; no new table, dependency, knob or public protocol | PASS |
| Exact original work/query counts unchanged | Protected runtime attempts test and group lookup query assertions untouched; unrelated staging record count includes selector input | V7 log exact test passed; V8 retained exact query assertions; complete diff contains no target-of-12 substitution | PASS |
| SDK shared-admission journey actually uses planned Run submit and Window submit | New V18 calls raw `WyrdClient::request_json` enqueue/direct HTTP, Bifrost write_batch, private fixture bind/activate; it never calls Run submit/Window submit | Existing SDK stories cover these APIs separately, but lack new paused-ACK assertion through them | **FAIL B-02** |
| V12 generated/static/format/lint and REQ-038 final integrated gate | Older complete gate at `e9e3d95cb`; candidate subsequently changes client production path and benchmark upstream | `gate2-sha.txt`, `gate2.clean` end `gate=0`; no renewed complete gate for `f0ea3b402` | **FAIL B-01** |
| REQ-039 / AC-019 same-final-SHA five accepted full records | Five accepted records committed separately in history `8c066985a`, all attributed to `19f4190a8`, before two later source commits | Accepted validity preserved; current candidate differs from measured source | FAIL: downstream final-source closeout pending |
| V15/V16 actual publication/report and main-based PR evidence | Existing history commit exists; caller explicitly reports PR unopened; task evidence has no final PR URL/history/targets closeout | No fabricated delivery inferred; no external publication attempted during read-only review | FAIL: downstream delivery pending |
| INV-006/007/008 and non-goals: privacy, immutable history, no benchmark concurrent load/L2/merge/release | Candidate does not change record schema/judgment or add benchmark workloads to gate; separate history commit appends task records | No validity re-adjudication, benchmark runs, history mutation, L2 or publishing by reviewer | PASS within inspected scope |

## Proposed findings

### B-01 — Final integrated candidate lacks the mandatory renewed gate

Classification: **MISSING**. Obligation: REQ-038, AC-019, TASK-009 V13 explicitly requires affected proof and renewed final gate after any final source/test/config change.

Locations: `crates/shared/wyrd-client/src/state.rs:888`, `observe/mod.rs:511`; `crates/wyrd/wyrd-testing/src/gateway_capture.rs:450`; task evidence `TASK-009-unified-queue-and-closeout.md:724`.

Evidence: independent `gate2-sha.txt` is `e9e3d95cb57249a37238235259437dd8a64fab98`; `gate2.clean` ends `gate=0`. `git log` and the patches show `da366eefe` and `f0ea3b402` change executable source after that proof. The former affects production bound-verifier resolution used by Observe verify/submit, while the latter affects the actual measured Gateway upstream. No complete final-source gate evidence was supplied. A single final-candidate outbox test reported by the orchestrator cannot establish the aggregate obligation.

Consequence/materiality: the explicit integrated acceptance gate cannot be attributed to the code being approved, including all language/HTTP/MCP consumers. This is a mandatory evidence violation, not a claim that either optimization is wrong.

Smallest correction: keep the implementation, run the required affected proof and complete final gate on the settled source candidate, recording exact SHA, command, all lane results and log paths. Reuse existing mise lanes; no new harness/check/framework. If retaining later source changes, final measurement attribution must subsequently follow V14. The review's prohibition on benchmarks is respected: measurement belongs to the subsequent authorized execution stage.

### B-02 — Planned shared ACK-gate public journey bypasses the Run/Window paths

Classification: **MISSING**. Obligation: TASK-009 V18 says to use actual Run submit, scheduled Window observations and grouped source inputs, with the new ACK-gate assertion through the real-client journey; REQ-038 requires real client journeys and preserved Window behavior.

Location: `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:7632` (`queued_origins_share_ack_gated_admission`).

Evidence: the journey builds raw `request_json` bodies for `/v1/verification/enqueue` and `/execute`; writes the observation with `Bifrost::write_batch`; and uses `VerificationFixture::bind_schedule/activate`. It reads status via typed Execution, but never resolves a bound Run/Observe submission nor exercises Window submit's observation route. Existing SDK stories prove those paths independently under normal conditions; they do not add the required held-ACK assertion through the planned public authoring flow. In particular `Observe::submit` reaches `bound_verifier`, which is also changed by the last production-client commit.

Consequence/materiality: the new public-journey promise is only partially proved. The test would still pass if the SDK's Run/Window dispatch failed before reaching the otherwise-correct server admission owner. This is a bounded required-test gap, not an independently demonstrated production defect.

Smallest correction: extend the existing V18 journey to submit its supplied/group source through the real Run API and produce the scheduled observation through Window submit, retaining public status/readback and held-ACK assertions. Keep private fixture controls for deterministic timing only. Reuse the existing SDK fixtures/story construction and existing ingest barrier; no new transport, SDK API, raw SQL story or test framework is warranted. Run the exact V18 selector and narrowly affected existing SDK regression, then include this source/test change in the renewed final gate.

## Reuse, failure paths and rejected suspicions

The candidate extends existing lifecycle owners instead of wrapping them: concrete preparers own selection, Scribe owns ACK filtering, Outbox owns bounded staging/retry, SubmissionSink owns tenant commit, VerifierExecutionQueue owns uniqueness/cursor/generation, and the existing runner owns lease/input/engine/result. No alternate loader, executor, durable spool or public prequeue state is justified or present. Existing lower SQL convenience methods remain lower-layer APIs and tests use them; production admission was traced through Acknowledged, so their mere existence is not an ACK bypass finding.

The RAII claim is necessary process-local duplicate suppression, not a replacement durable lease. It is held by shared queued items across publication and SQL retries, drops on loss/cancellation, and is bounded by reserved outbox capacity. Cross-replica duplicates use existing unique keys. Orphan Bifrost inputs and changed preacceptance selections remain approved limits; no stronger physical exactly-once guarantee is inferred.

The shared outbox two-clock race has a source-backed diagnosis in external `codex-diag.md`, and its fix addresses the common owner rather than a timeout. That diagnostician explicitly did not prove causation of the captured integration failure. The deterministic regression and older green aggregate justify the fix itself; the task's stronger historical attribution should not be used as additional runtime proof.

## Closeout sequencing and limits

V13 deliberately places immutable correctness review before V14 measurements and V16 PR creation. The missing final-source five-record set and unopened PR are recorded as outstanding task-completion obligations, **not extra admission-code findings or an instruction to publish during review**. The valid accepted `19f4190a8` results remain valid evidence for that earlier source; they cannot silently become results for the later source. After correctness approval, reuse V14–V16 to complete final source attribution, separate history publication/report regeneration and the actual main-based PR, or report the exact credential-blocked operation. No merge/release is authorized.

No broad verification or benchmark was rerun here. The remaining risk is unverified final-source integration and incomplete final delivery, plus the bounded public-journey proof gap above; no additional source-proven runtime defect was established.
