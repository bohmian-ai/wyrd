# TASK-001 behavior review

**Result: FAIL.** One source-local proposal, **BEH-R4-001**, retains a queue-eligibility problem within prior FIND-TASK-001-2. The previous selected-result, capped-tail, refused-first, temporal-proof and documentation defects are closed at their cited sites. This report makes no claim that the newly described interleaving was executed.

## Subject and method

Reviewed immutable cumulative base `ef3a57b0776dd7bc451273b9176498fcca700550` to candidate `9f9f7743311aec0f2191f775e7bf054aca00ff91` in the requested worktree, against approved specification revision 15, the original TASK-001, AGENTS/agent rules, the reference router, spec-driven development, maintainer style, design/doctrine, Bifrost and the applicable evaluation, analytical reliability and agent-surface authority. Used CodeGraph before source navigation. Read prior R1/R2/R3 verdicts, validation/remediation records and appended evidence as claims, then checked retained issues against current producers and consumers. No current discovery report or other reviewer's conclusion was received.

The complete range remains the subject; the latest fix diff was navigation only. TASK-002, TASK-003 and REQ-015 are excluded. Owner decisions accepting newest 64 without continuation, uncapped identity reads, payload-only ceiling, PostgreSQL-anchored ingest times and a single membership/payload statement are binding. No additional cursor, inventory, quota, terminal-source readiness or upgrade workflow is demanded.

No tests, builds or check lanes were run by this reviewer. Root reports `build:monty-worker`, five exact group units and all four exact `pg_verification_groups` tests passed; seven Rust journeys were still running at the last update. Prior packet results are retained evidence, not fresh independently certified execution by this reviewer. HEAD remained the candidate and the worktree was clean before this report.

## Caller and owner coverage

- Invocation: shared Observe/Target/StagedRun, Python Observe wrapper/authored stubs, Node NativeRun/TS options, HTTP/MCP typed requests, VerificationControl execute/enqueue/stage_direct, Principal owner copy and ResultPayloadBuilder input/result projection.
- Registration: InputSlot/SourceText and source grammar, TriggerDuration/activation, Operator lists, EffectiveSpecs load and same-site source validation, GroupSite ambiguity, reaches cycle walk, BindingProjector/group_sources and exact SQL binding pins. Also checked shared manifest validation against the loader and server caller.
- Runtime: scheduler claim/cursor/authority, GroupPoller/BindingPoll/Sources/Ended/Payloads, queue group_submissions/group_states/covers/enqueue_group, outstanding index, submission acceptance and idempotency, runner/result staging and transactional completion/previous-result dispatch, query authenticated entry and terminal-safe consumption.
- Evaluation: empty each, source conformance, source-limit outcome/summary, check Agent restriction and shared HostedTools invocation. Ordinary Window/Drift, direct execution and non-group Operator paths remain sibling consumers.
- Proof/projections: Rust grouped support/refund/coding/media/recovery/late-work/grant stories and shared support; Python/TS grouped stories and native projection; MCP support/media/query tests; server group and served OpenAPI tests; schema/types, managed tables, design/evaluation/runtime-limit docs, manifests/lock change and verification recipes.

Compared complete existing workflows: registration continues through EffectiveSpecs and binding projection; source judgment uses the existing queue, query admission/terminal settlement, Vala engine and Operator delivery. Drift's window reader cannot provide the missing cross-execution grouped lists. GroupPoller owns meaningful query dependencies and the required snapshot workflow, rather than wrapping existing full behavior. The new GroupSubmissions query replaces two narrower classifications at the same queue authority; no second acceptance owner is justified. The remaining defect is the source reader failing to distinguish the existing pre-acceptance queued state, not a reason to strengthen or replace the outbox/queue workflow.

## Acceptance matrix

PASS means source and available proof support that row within this review's execution limits; it does not certify a broad verification lane.

| Requirement / criterion / constraint | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001/011 group_id through ordinary verify/submit, no inferred Run grouping | Observe::group_id/grouped, Target request, StagedRun, Python keyword and TS option, VerificationRequest and existing HTTP/MCP | Three-language support/recovery stories, MCP support and served contract retained | PASS |
| REQ-001 reserved typed/source inputs.group_id, renamed-slot success and server enforcement | validate_slot, server validate_request, loader shared validation | Coding registration negatives in Rust/Python/TS; HTTP enforcement/resolve refusal and served/schema tests | PASS |
| REQ-001 enqueue replay/conflict exclusion, REQ-005 eligible membership | Sources::read removes only already committed refusal IDs; pre-acceptance queued rows are indistinguishable from direct rows | Existing support/conflict-at-ceiling tests wait for queue acceptance; no pre-acceptance conflict poll | **FAIL: BEH-R4-001** |
| REQ-014 IO-free owner copy, ownerless/unbound invocation preserved | Principal::owner_service_uid, direct/enqueue builders and runner fallback; no producer binding lookup | Owner-copy unit, support/recovery ownerless and wrong-owner negatives | PASS |
| REQ-002 exact whole-slot/outcome grammar and local selector rules | SourceText/SourceReference, validate_slot; unchanged local Reference roots; Payloads::value | Grammar/schema/values units and coding stories | PASS |
| REQ-013 same-site resolved sibling/external exact versions, declared targets, ambiguity/cycle refusal | EffectiveSpecs/source_binding/GroupSite/reaches; BindingProjector group_sources | Coding external/sibling, child/unbound/cycle/missing/ambiguous negatives; exact-version recovery | PASS |
| REQ-002 aligned lists, input ingest-time/ID order, result-only fallback, genuine nulls and failed outcomes retained | Ended::executions and shared tuple list; earliest payload maps; selected result filter | Group order/value units, coding/media/late-work stories, selected-result server case | PASS, subject to eligibility finding |
| REQ-003/004 complete eligible lifecycle activity and first-row finality | One row read feeds Ended; candidates HAVING MAX, then filtered first/newest; no capped identity prefix | Group end/duration units; finality_anchors_on_the_first_eligible_row | PASS for settled classifications; pre-acceptance caveat above |
| Revision 15 PostgreSQL-anchored times, one membership/payload statement | Sources::surfaces uses wyrd_ingested_at for both surfaces; DueGroupBinding clock is statement_timestamp; Sources::read window count and conditional payload | one_read_selects_eligible_rows_and_gates_payloads; PG tick uses database elapsed time/durations/cursor, no row timestamp rewrite | PASS |
| REQ-003/004 serial outstanding work, immutable retry snapshots and later whole review | Binding claim transaction, partial unique index, queue frozen Grouped input and covers | Recovery/late-work and SQL queue lease/settlement retained evidence | PASS |
| REQ-005 authoritative queued results before coverage | group_submissions selected map filters results before decode/Ended/covers | a_visible_result_counts_once_its_settlement_selects_it: null first, same published result selected, actual next, no unchanged duplicate; root fresh PG pass | PASS |
| REQ-006/009 bounds, exactly N versus N+1, payloads withheld on overflow, no partial-prefix judgment | One COUNT(DISTINCT execution_id) window over eligible row set; conditional payload; query admission/deadline and terminal settlement | SQL-shape/decode units, refused-at-ceiling PG test, media ceiling stories | **FAIL only for pre-acceptance refusal eligibility: BEH-R4-001** |
| AC-006 failed/truncated query defers and retries | BindingPoll::run keeps batches local until run_with succeeds; shared internal fault reaches ScheduledQueryCaller | partial_group_reads_enqueue_nothing_until_a_complete_poll, root fresh PG pass; negative control retained | PASS |
| Prior successful selected-payload-omission proof obligation | Separate selected-identity and payload cuts/complete guard deleted; a row's identity and payload are now one statement and one output row | Query shape plus existing failed-terminal PG proof; no longer a separately reachable incomplete selection cut to inject | PASS; obsolete proof need not preserve deleted machinery |
| REQ-008 globally non-vacuous Assert each; ordinary comparisons retained | Engine::judge empty array -> empty_selection; original evaluate_single/evaluate_item_mode for nonempty | Engine tests and coding inline/step negatives; group media zero-frame stories | PASS |
| REQ-010 baseline, changed verdict-only dispatch, immutable result/history, ordinary on_failure retained | validate_binding lists, queue complete/previous group lookup and unique dispatch intent | Refund story and PG queue baseline/change/replay evidence | PASS |
| REQ-011 accepted owner grants on source reads and narrow live bifrost.query | Caller::accepted + ScheduledQueryCaller::authenticated; check validator/runtime; HostedTools ordinary query/tool authorization | Owner query capability/scope denial-regrant, independent tool/query denial, MCP scopes/isolation | PASS |
| AC-003 trace IDs pinned before later span, live query sees late data | HeldReview observes/holds first request before late span export; release permits ordinary tool query; final evidence stays IDs | Rust coding body now establishes pin -> publish -> query; root runtime result pending at report time | PASS source/proof design; execution limit explicit |
| AC-001–005 stories across SDK/HTTP/MCP; media sampling and restart | Existing real-server/local-provider stories; application sends per-item calls, server groups ordinary evidence | Retained 3SDK stories and MCP media; shared Rust restart/replica/Drift proof | PASS apart from eligibility counterexample |
| AC-006/008 tenancy/exact-owner/subject/version, read permissions and schema parity | RLS TenantConn, tuple filters, exact pins, typed contracts/generated TS/authored Python | Recovery/owner-grant, MCP other tenant, served OpenAPI/schema/type negatives | PASS |
| INV-001–005 / non-goals / pre-release schema rule | Existing client/server boundaries, one ScribeOutbox, in-place managed table definition changes; no new group store/producer write/upgrade | Cumulative source/manifest/diff inspection; prior boundary checks retained | PASS within TASK-001 (TASK-003 pressure/REQ-015 excluded) |

## Proposed finding

### BEH-R4-001 — INCORRECT — queued input can enter a permanent snapshot before acceptance refuses it

**Prior relationship:** same acceptance/eligibility authority as FIND-TASK-001-2, not a new per-SDK defect. The already-settled refusal cases fixed by R3 remain closed; this is the earlier producer state in that same workflow.

**Violated obligations:** REQ-001 existing idempotency/conflict semantics; REQ-002/005 eligible aligned snapshots; REQ-009 count eligible executions, not refused contributions; immutable judgments and AC-006 refusal/isolation behavior. No terminal-source readiness guarantee is demanded.

**Locations:** `crates/wyrd/wyrd-server/src/verification/groups.rs:141–151,430–435,465–480`; `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:250–256,989–994,1275–1336,1690–1729`; producer `crates/wyrd/wyrd-server/src/scribe_outbox.rs:907–928` and `components/verification/service.rs:610–648`.

**Producer-to-consumer proof:**

1. VerificationControl::enqueue stages the queued input containing the request's group and authenticated owner. ResultPayloadBuilder::inputs already records `queue_execution=true`; direct inputs record false.
2. ScribeSink::write submits the input frame first. Only after acknowledgment, at lines 921–928, it stages SubmissionOutbox. Thus an attributed, query-visible queued input can exist before its queue row. SubmissionOutbox's independent write may be delayed/retried, including waiting on the existing request-key advisory lock. Group scheduling uses the same available database but neither locks that key nor waits for this writer.
3. `group_submissions` can consequently return no row for that execution. Sources::surfaces drops the existing queue_execution distinction; Sources::read filters only the committed `refused` set. An absent queued ID therefore enters the identity count, timing, payload list and coverage exactly like an ordinary direct execution. At max_age or idle, a snapshot can consume it.
4. Later acceptance can determine that this is a conflicting request under an existing key and commit `errored`, attempts=0. The next classification removes it, but the prior immutable snapshot/result already contains a request that never executed. If its removal only lowers through/members, GroupState::covers (`>=` comparisons) suppresses a corrective review. A 20-valid-plus-one-not-yet-classified-conflict case can also finish inconclusive/source_execution_limit_exceeded instead of judging the permitted 20.

This needs no source-server crash or global Postgres outage: queue handoff is deliberately asynchronous and the key advisory lock is a real independent wait. A process crash after Scribe acknowledgment can prolong the absent queue row, but the ordinary delay alone proves reachability. Existing tests `Support::turn` wait for an execution-status row before polling; their green results cannot falsify this state. This is not the distinct selected-result race: R3 correctly prevents an unselected result from counting after its queue row is known.

**Observable consequence:** immutable group evidence/verdict may permanently include a conflicting/refused submission, or report false overflow, despite later queue rejection; no new analytical row is needed to expose the misclassification and no later review is guaranteed to fix it.

**Smallest correction boundary:** keep all producers, Scribe acknowledgment order, submission outbox, queue idempotency and selected-result settlement unchanged. At the existing GroupPoller/Sources eligibility owner, retain the already-recorded queued/direct discriminator and use the existing queue's committed acceptance classification for queued input eligibility. A queued input with no known accepted queue row must not be treated as direct/accepted evidence. An accepted pending/running/retrying source remains eligible with genuine null outcome; do not wait for terminal state or introduce pending-turn tracking. Reuse GroupSubmissions' accepted keys and refused set, and the existing single statement, rather than add a store, producer IO, queue receipt, downstream runner/Operator guard or another resolver. Preserve direct inputs, same-ID replay dedup, timing of genuinely eligible rows, accepted uncapped identity scan and payload ceiling.

**Focused closure proof:** through the existing group/query/queue seam, expose a queued input before its handoff is accepted, while an earlier request owns the same idempotency key. A due group must never pin/count that unresolved conflicting input or its payload. Commit the existing refusal and poll again; valid members must receive one normal snapshot (and exactly N valid IDs at ceiling must remain permitted). Also show that a later accepted pending source becomes eligible without waiting for its result, and a direct source absent from the queue remains eligible. No broad language suites, host load or general harness is required.

## Prior findings and evidence diagnoses

| Prior stable ID | Current source-backed disposition |
|---|---|
| FIND-TASK-001-1 | Closed at prior locations: no completion watermark/hour bound; accepted newest-page policy; selected queued outcome filtered before Ended/covers and exercised by new PG case. |
| FIND-TASK-001-2 | Settled-refusal/count-cut/capped-tail/old-refused-first and separate-payload-cut defects closed. Retained only for BEH-R4-001's pre-acceptance classification state. One statement makes identity-without-its-own-payload across cuts inexpressible, so retaining the old completeness machinery/test is unnecessary. |
| FIND-TASK-001-3 | Closed: checked DateTime operations, elapsed spans and safe maximum duration domain; no arbitrary new authored ceiling. |
| FIND-TASK-001-4 | Closed: schema is source string plus four real typed forms; runtime/generated/served projections agree; no phantom object compatibility added. |
| FIND-TASK-001-5 | Closed: accepted owner caller and authenticated ordinary query path apply to every group statement; Drift's existing exception remains separate. |
| FIND-TASK-001-6 | Closed at all previously cited items, including Support::tick Errors and partial-progress contract. No replacement rustdoc blocker proposed. |
| FIND-TASK-001-7 | Closed: active design/evaluation matches declared bifrost.query only, ordinary grants, no workspace/mutation and live data outside snapshot. |
| FIND-TASK-001-8 | Closed in source/proof design: registration/media/grants and actual internal EOF recovery remain; HeldReview now establishes the temporal pin-before-late-span sequence. Fresh coding execution was pending when this report was written. |

Reviewed evidence diagnoses against their actual changes: waiting for Bifrost publication before positive restart keeps accepted outbox loss honest; fixture provider-scope correction does not alter production routing; newly reserved-slot refusal belongs in shared server validation; shared artifact-manifest validation deletes the loader-only duplicate workflow; query fault application stays test-support-only and reaches the real internal stream; managed ingest-time ordering justifies replacing submission-order expectations with the declared tie-break order and aligned expected outcomes. No unrelated timeout, ignore, lint suppression or removed assertion is proposed as a way to clear a failure.

## Limits and disposition

Source review supports the corrected mechanisms and previous closures, but green stable-input tests do not exercise the pre-acceptance classification interval. BEH-R4-001 requires independent validation before promotion into the final stable ledger. No optional improvement, cursor extension, capacity target, retained-table upgrade or general framework is requested.

Skipped all test/build/check execution as assigned; root owns sequential verification. The material risk is an immutable judgment that includes queued evidence later refused by the existing acceptance owner.
