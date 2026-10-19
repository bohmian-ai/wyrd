# TASK-001 independent behavior review

Result: **FAIL**. The cumulative implementation supplies the grouped producer,
registration, scheduler, judgment and inspection paths. Two prior findings are
not completely closed: the source ceiling is decided before source eligibility
is known, and the explicitly required failed/truncated snapshot proof remains
absent.

## Immutable subject and review limits

- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Candidate: `47e0c9ab7a5cf91cb8d292bdd96a86bdd966a3f9` (HEAD remained this commit).
- Specification: `changes/active/long-horizon-verification/spec.md`, explicitly
  approved **revision 13**. The file's draft-14 header and schema-reset addition
  are not treated as approval of revision 14. No retained-table upgrade scope
  is inferred.
- Original task: `tasks/TASK-001-grouped-verification.md`; previous verdict,
  validated findings and remediation: `review/task-001-e4b1e313a-20261017/`.
- Reviewed the cumulative base-to-candidate change and surrounding owners;
  `e4b1e313a...47e0c9ab7` was used to locate remediation, not as the acceptance
  range. Other current-round reports were not read.
- Authorities consulted: AGENTS, agent-rules, design/doctrine, Bifrost design,
  spec-driven development, maintainer style, testing workflows/TESTING, relevant
  manifests and mise lanes. CodeGraph was used before code navigation.
- No test, Cargo or source mutation was performed. Task/remediation command
  outcomes below are retained claims assessed against test bodies, not fresh
  execution results. The orchestrator coordinates fresh execution.
- During review the orchestrator reported fresh `build:monty-worker` PASS,
  five exact group-reader unit cases PASS (547 skipped), and seven exact Rust
  grouped journeys PASS (121 skipped), using mise and the Postgres wrapper/
  migrations. These establish those test bodies; neither proposed gap is
  exercised by them.
- The user's selected first-source-within-finality, newest-first, **one newest
  page** discovery bound is accepted. No continuing cursor is required here.
  TASK-002 schedule inputs, TASK-003 staging pressure/progress, and capacity
  qualification are outside this task review.

## Caller-to-result coverage

| Path | Producers, consumers and existing owner traced |
|---|---|
| Ordinary grouped calls | Shared `Observe::group_id/grouped`, `Target::request`, staged submissions; Python Observe and TS NativeRun/Observe; typed HTTP/MCP request; control `enqueue/stage_direct`; Principal owner copy; ResultPayloadBuilder and submission writer/queue |
| Authenticated attribution | `Principal::owner_service_uid`; direct input/result builders; submitted request authority; runner result projection. Owner is copied without a new producer lookup; ownerless calls remain ordinary calls. |
| Registration | Loader validation; server `validate_request`; `EffectiveSpecs::{validate_bindings,validate_binding,validate_sources,reaches}`, `GroupSite::ambiguous`; `BindingProjector::group_sources`; source grammar and exact refs; graph/reference walkers for both Operator lists |
| Group scheduling | Binding activation projection/authentication arming; scheduler `poll_groups`; `claim_due_group`; all GroupPoller/BindingPoll/Sources/Ended/Payloads operations; queue group coverage/outstanding state and enqueue fence |
| Judgment and evidence | Grouped queue claim → pinned inputs/coverage → runner; source-ceiling outcome; Vala conform/assertion/Agent execution; hosted tool authority; existing scheduled query terminal consumer and evidence reader |
| Settlement and inspection | Lease-fenced completion, preceding group verdict and frozen dispatch context; execution/binding projections; selected queued result authority; persisted Bifrost input/result metadata |
| Real surfaces | Rust/Python/TS five grouped stories, Rust late-work and owner-grant stories, shared supporting helpers, MCP discover/execute/enqueue/read and media story, SQL serialization/dispatch test, HTTP registration and served OpenAPI tests, generated TS/type checks and Python stubs |

The new group reader extends a missing source-selection workflow. The existing
Drift window reader does not select grouped execution values. The implementation
appropriately reuses the queue, query terminal consumer, authorization and
Vala/Operator owners; no second queue, transport or durable lifecycle store was
found. The remaining correction belongs in this existing group source selector.

## Cumulative acceptance matrix

| Obligation | Implementation evidence | Verification evidence and assessment | Result |
|---|---|---|---|
| REQ-001/011: optional exact `group_id`, all SDKs, HTTP/MCP, no resource creation/close | Shared Observe/Target; Python keyword and TS options/native projection; VerificationRequest; persisted columns | Three support stories and MCP discover-act-observe exercise grouped/ungrouped calls and inspection | PASS |
| REQ-001/014: authenticated nullable owner, no producer Postgres addition; ordinary ownerless/direct calls | Principal owner copy; control staging; runner metadata; no new owner/binding IO in producer diff | Support stories exclude admin/foreign rows; source tracing establishes no new lookup | PASS |
| REQ-002: exact grammar, whole values/outcomes, alignment, ID-only traces, genuine null | SourceReference parser; validate_sources; Sources tuples; Ended ordering; Payloads::value | Parser/validation tests; coding and frames stories; late-work story pins null then actual result | PASS |
| REQ-013: same-site name resolution, exact UID/version, ambiguity/cycle refusal, isolation | EffectiveSpecs/source_binding/GroupSite; frozen GroupSource; owner/subject/Verifier/group SQL | SDK coding refusal/external-reference paths; recover story newer-version/tenant exclusion; support owner exclusion | PASS |
| Reserved `inputs.group_id` registration refusal and rename success | Shared validate_slot; loader plus server validate_request → VerifierSpec::validate | Rust/Python/TS coding registration variants and Rust raw HTTP refusal; invalid Cards cannot persist | PASS |
| REQ-003/004: pinned timing, idle/max_age/finality and fixed binding cadence; accepted discovery page | TriggerDuration; candidate finality HAVING and newest-first LIMIT; Ended; fixed GROUP_POLL_INTERVAL | Timing pure tests, refund finality and late-work neighboring huge-duration binding | PASS subject to source-eligibility finding below |
| REQ-003/005: one outstanding review, coalescing, immutable retry/history, late work | Outstanding partial unique index; group_states/covers; snapshot columns; ClaimedRow Grouped input | SQL group serialization/dispatch test; Rust late-work story and replica/restart recover story | PASS |
| REQ-005: existing outbox visibility/loss, no readiness store | Existing ScribeOutbox input/results; no new membership/receipt state | Frame restart flush is explicitly the published positive variant; accepted loss is not reinterpreted as durability | PASS |
| REQ-008: zero each inconclusive everywhere, existing nonempty decisions | Engine::judge EMPTY_SELECTION branch; existing comparison/decide reused | Ordinary inline and step `empty-each` checks in coding, zero-frame group checks across SDKs/MCP | PASS |
| REQ-009: exactly ceiling accepted, count eligible logical IDs before payloads, no truncated judgment | counts → over → identities → refused filter → snapshot | Basic 20/21 stories pass claimed; eligibility and read-cut interleavings are wrong (BEH-R2-001) | FAIL |
| REQ-010: baseline/no reaction, changed-only reaction, replay fence, safe context | Queue complete/PREVIOUS_GROUP/INSERT_DISPATCH; GroupChange and existing Operators | SQL first-failed baseline/unchanged/recovery/replay; refund real dispatch/history stories | PASS |
| REQ-011 sensitive reads and live tool authority | Caller::accepted + ScheduledQueryCaller::authenticated for all group statements; hosted tool/query permission owners | Owner-grant story independently denies capability/table scope then restores; coding independently denies Tool/Invoke and trace query scope | PASS |
| AC-003: explicit live query, no automatic span snapshot; refused other tools | Narrow check-Agent validation and runtime guard; existing Bifrost tool | Coding supplies trace ID, exports spans after source calls, sees span names only after tool result; non-Agent support owner-grant story has no judge calls | PASS |
| AC-004: media failed/zero/ceiling/isolation over SDK and agent surfaces | Existing file transfer and per-item queue, group outcomes | Three SDK frames stories and new MCP media story exercise ordinary evidence planning/HTTP upload and MCP judgment | PASS for covered scenarios |
| AC-005/006 task share: concurrent groups, Window regression, serial recovery, failed/truncated reads retry without partial judgment | Queue/scheduler reuse and terminal-safe query owner | Recover covers two groups, scheduled drift, replicas/restart; source permission failure retries. No deliberate partial snapshot/stream failure proof (BEH-R2-002) | FAIL |
| REQ-011 contract parity and public inspection | Source-owned InputSlot union; GroupCoverage/settings schemas; authored stubs and regenerated TS | Schema/decode tests, TS phantom-object negative type case, served OpenAPI and codegen claims | PASS |
| Non-goals and ordinary workflows | No new producer IO/store/queue/close route/source syntax; schedule mapping and outbox bounds left to their tasks | Cumulative source and fixtures preserve existing inline/Window/ordinary Operator workflows; dev-only sha2 is MCP upload-test hashing | PASS |

## Proposed findings

### BEH-R2-001 — INCORRECT — eligibility is applied after the ceiling decision

Prior ledger: **FIND-TASK-001-2 remains open in part**.

Obligation: REQ-001/002/004/009, AC-001/004/006 and remediation finding 2 require
the source ceiling to count eligible distinct executions, with refused
submissions excluded before consuming source capacity, and no over-ceiling
payload collection or unmarked judgment.

Exact changed locations: `crates/wyrd/wyrd-server/src/verification/groups.rs:139`
(counts/over classification), `:144` (identities excludes over tuples), `:146`
(refusal filtering occurs only afterwards), `:246` (count query), `:282`
(separate identity read), `:320` (list construction), `:749` (explicit shortcut).

Reachable producer-to-result path:

1. Use the existing HTTP enqueue/idempotency workflow already present in all
   support stories. Twenty legitimate executions match a source tuple; submit
   one conflicting request under an existing key. Its staged execution-input
   row remains in Bifrost and its queue row is `errored`, attempts=0.
2. `counts` sees 21 distinct execution IDs. `over` classifies the tuple over
   the deployment ceiling 20. `identities` omits the entire tuple.
3. `refused_submissions` sees no IDs from that tuple, so cannot exclude the
   conflicting submission. Snapshot pins `exceeded_ceiling`; the runner
   produces completed `inconclusive/source_execution_limit_exceeded` although
   exactly twenty eligible executions should have been judged.

An entirely refused set can likewise create an apparent group instead of no
eligible activity. `Ended`'s comment acknowledges refused rows in over counts;
that shortcut was not authorized by the accepted **discovery-page** ceiling.

The same source owner also trusts a count from an earlier Bifrost read cut:
if count=20, then a 21st valid input becomes visible before `identities`, the
unbounded identity query returns all 21. There is no later distinct membership
ceiling check, so snapshot fetches/pins and judges 21 values with no overflow
marker. Choose an already max_age-due group to make the subsequent judgment
reachable regardless of the newest row's idle age. Bounded multi-query cuts
are allowed; trusting an earlier count as the later membership's ceiling is
not. Existing query output admission can reject a huge response but does not
make a small 21-row response correct.

Sibling writers/consumers checked: direct control staging, submitted input
staging/queue conflict settlement, queued runner results, selected_results,
refused_submissions, all Sources count/identity/payload SQL, Ended and runner
ceiling short-circuit. Both input and requirement references share the tuple
selection and are affected; no SDK-specific guard is appropriate.

Smallest required correction: keep GroupPoller/Sources as the one selection
owner and make the **actual eligible distinct membership** the authority for
the overflow decision before fetching that tuple's payloads. Reuse the queue's
existing refused-submission classification and existing overflow outcome.
Keep selection bounded and account for changed visibility between statements;
an earlier count cannot authorize an enlarged later membership. Do not add a
store, producer lookup, extra budget or downstream evaluator workaround.

Focused proof: extend the existing grouped support/media owner with ceiling
legitimate executions plus an idempotency-conflict row (correct ordinary
judgment, conflict excluded), an all-refused source set (no invented group),
and the existing query seam with a count/identity visibility change crossing
the ceiling (explicit overflow before payload collection). Preserve the
existing 20/21 and mixed-source behavior. This is source ceiling proof, not
load generation or TASK-003 progress work.

### BEH-R2-002 — MISSING — partial source-read failure/retry proof remains absent

Prior ledger: **FIND-TASK-001-8 remains open in part**.

Obligation: spec AC-006 (`spec.md:1026`) and original task scenario 5, explicitly
retained by remediation finding 8, require failed/truncated discovery/snapshot
reads to enqueue no partial judgment and later retry successfully.

Locations: `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:2800`
(`grouped_reads_need_the_owner_grant`); `crates/wyrd/wyrd-server/src/verification/groups.rs:377`
(`complete`), `:493` (`run`), and `:1144` (`reads_select_only_listed_membership`).
The remediation's finding-8e evidence table substitutes this owner-grant test
and finding-2 unit tests for the required partial-read case.

The owner-grant test is credible denied-admission/retry evidence. It fails
before source batches are emitted. The groups unit test inspects SQL strings;
none invokes `complete` with incomplete selected coverage or makes a source
read fail/truncate after a batch. Existing unchanged
`ScheduledQueryCaller::consume_to_terminal` tests prove its stream contract,
but the prior validated finding explicitly required the grouped integration
to prove it cannot enqueue a partial snapshot. No new grouped test injects
that failure. Incidental teardown failures remain non-asserted evidence.

Consequence: an explicitly required failure boundary remains unproved after
remediation; passing main stories and authorization denial do not establish
that a partially collected grouped read is discarded. This is a coverage
finding, not a claim that the inspected terminal-safe consumer is broken.

Smallest required correction/proof: reuse the existing real grouped story and
existing query fault/terminal facilities to interrupt one discovery or selected
payload read after some data, assert no group execution/result adopts that
prefix, remove the failure, and assert the next poll produces the complete
snapshot. Use the owning focused selector; no new transport/parser, generic
harness, broad suite or restart API is requested. Keep BEH-R2-001's logical
membership interleaving proof distinct from stream failure.

## Prior-finding closure

| Stable finding | Source-backed disposition |
|---|---|
| FIND-TASK-001-1 | CLOSED under the user's selected simpler bound: completion-time watermark and one-hour slack removed; candidates use first-source finality and newest page; queue covers/outstanding remain authority. Late-work test proves outstanding arrivals and null→actual follow-up. No cursor demand retained. |
| FIND-TASK-001-2 | PARTIAL: raw physical skip removed; mixed over tuples do not fetch their payloads; both payload SQL statements use listed IDs. Still open as BEH-R2-001 because refused eligibility follows over classification and later membership can grow beyond its earlier count. |
| FIND-TASK-001-3 | CLOSED: checked discovery/cutoff arithmetic and elapsed timing; maximum duration pure checks and neighboring forever binding. |
| FIND-TASK-001-4 | CLOSED: InputSlotSchema is four real typed object forms plus SourceText string; generated TS removes tagged-source object; schema/runtime/type negative tests agree. |
| FIND-TASK-001-5 | CLOSED: all group source reads use accepted owner caller/authenticated query owner; source capability and object grants denied independently, then restored. Existing Drift SYSTEM exception remains unchanged. |
| FIND-TASK-001-6 | CLOSED for named omissions: duration fmt/deserialize Errors docs, SourceText field, group test module and Forward sender field now document their contracts. |
| FIND-TASK-001-7 | CLOSED: active design/evaluation narrow exception matches validator/runtime: declared bifrost.query, ordinary permissions, no mutation/workspace, live spans outside snapshot. |
| FIND-TASK-001-8 | PARTIAL: SDK/HTTP reserved-slot refusal, registered external source, ambiguity/absent declaration, independent tool/query denial, late exported spans and MCP media negatives added. Failed/truncated grouped snapshot proof remains BEH-R2-002. |

No optional improvements, new scope or independent upgrade requirement is
proposed. Fresh verification remains orchestrator-owned; even green retained
lanes would not close the two source/proof gaps above.
