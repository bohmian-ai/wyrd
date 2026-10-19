# Independent behavior review — FAIL

## Subject and authority

- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Candidate/HEAD: `ae9c5a6aee22b691e2dc04c805cb5d1dddfcb9bf`.
- Reviewed the cumulative `git diff ef3a57b07...ae9c5a6ae`; the latest remediation range was navigation only. No implementation summary or current-round reviewer report was used as a conclusion.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-001-grouped-verification.md`.
- Authority: the user's approved revision **13** requirements, AGENTS, agent rules, spec-driven development, maintainer style, active Wyrd design/doctrine, Bifrost query/visibility and outbox contracts, and evaluation reference. The checked-in draft-14 header/history does not approve its schema-reset exception and does not establish a retained-upgrade requirement.
- Read both earlier verdicts and validated ledgers, TASK-001-R1 and TASK-001-R2, including their appended implementation evidence. Evidence is a claim; source and exercised assertions determine what it proves.
- Preserve the explicit finding-1 amendment: first-source finality, newest-active single page, accepted ponytail ceiling, **no continuing cursor**. Preserve the R2 decision: the one selected identity result decides capacity and snapshot membership, with refused submissions excluded. Nothing below requests the removed count, a cursor, a producer lookup, readiness barrier or retained-table upgrade.
- TASK-002 scheduled-input changes, TASK-003 staging/pressure/progress and REQ-015 capacity qualification are excluded.

I ran no Cargo, test, build, mutation of source/tests, or verification lane. Root owns sequential verification. HEAD was checked at the beginning and end and remained the candidate. Only this report was written in the repository.

## Navigation and complete caller coverage

CodeGraph was used first for grouped polling and subsequently for result settlement and its producer/caller relationships. The cumulative diff and surrounding source were inspected across:

| Boundary | Producer → owner → consumer traced | Existing workflow/reuse assessment |
|---|---|---|
| SDK invocation | Rust Observe/Target; Python PyObserveHandle/stubs; TS Observe/NativeRun/declarations → typed VerificationRequest → control staging | Metadata is projected through the shared client; no parallel transport or new hot-path durable owner. Stage captures the chosen group for later enqueue. |
| Identity/authentication | Principal owner_service_uid → enqueue/direct ResultRun → queued runner result | Available authenticated Service UID is copied without IO; Agent/human/SYSTEM/null cases do not infer a parent Service. |
| Registration | loader/spec validation and artifact manifest → server validate_request/EffectiveSpecs/source_binding/GroupSite/reaches → BindingProjector → frozen binding | Existing registration/resolution owner is extended; the shared manifest rule closes loader-only enforcement instead of adding a second workflow. Both Operator lists enter existing walkers/validation. |
| Group discovery/snapshot | binding cursor → GroupPoller/BindingPoll → Sources identities → queue refusal/result selection → Ended/coverage → Payloads → GroupSnapshot | Grouping needs this missing ordered cross-execution selection; existing Drift window reads cannot replace it. Actual source eligibility must be shared by discovery, coverage and payload selection, not merely by the capacity check. |
| Queue/runtime | SQL binding activation/claims, group unique index, enqueue/claim/status/group_states/complete → scheduler/runner → Bifrost result builder → Operator dispatch | Existing queue, lease, retries and transactional dispatch owners retained. Group snapshots, baseline and verdict-change comparison live on existing executions. |
| Queries/faults | authenticated ScheduledQueryCaller → stream/terminal consumer; collect/routes and QueryStreamFaultController siblings | The shared consumer owns terminal validation. R2 applies the existing test fault at the internal consumer instead of routing groups through HTTP or adding a transport. |
| Evaluation/tools | source declarations → Vala conform/scope/assert/decide, Agent executor → HostedTools/BifrostQuery | Empty each handled once; declared check tool exception remains query-only, ordinarily authorized, no workspace. Source traces remain IDs, not implicit span documents. |
| Proof/public consumers | all three grouped SDK story bodies and support; MCP support/media/query; server pg_verification_groups/OpenAPI/registration; SQL queue/binding tests; schema/type tests; query lifecycle | Inspected actual assertions, not only test names. Runtime ownership stays with each language; shared recovery uses the Rust server/queue owner. |
| Remaining cumulative changes | managed input/result schema definitions, migrations, config/runtime composition, fixture cards, docs/design/evaluation, dependency manifests/lockfile and mise lanes | No new queue/group table, compatibility route, Card kind, media format, budget system or production dependency. The installed sha2 MCP dev dependency hashes uploaded evidence. |

Relevant manifests and canonical mise tasks were inspected; this review does not claim any lane passed from source inspection alone.

## Cumulative acceptance matrix

PASS below means source-supported behavior with the named existing proof; independent runtime results remain root-owned. FAIL identifies the narrower gap described below.

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| REQ-001/011 exact optional group_id on verify/submit, wire/storage/inspection; omission unchanged | Observe/Target, Python/TS native projections, request/result rows and execution state | Three SDK support/recovery stories; MCP support; OpenAPI | PASS |
| REQ-001/AC-008 reserved typed/source slot; ordinary slot names retained | validate_slot and server validate_request → VerifierSpec::validate | Three SDK coding registration refusals; Rust direct HTTP/no-persistence; schema/type proof | PASS |
| REQ-014 nullable authenticated Service owner without producer binding/parent lookup or write | Principal accessor, control enqueue/stage_direct, runner ResultRun | Static complete producer trace; ownerless/wrong-owner/UID stories | PASS |
| REQ-002 whole slot/outcome grammar only; global requirement ID; no child/reason/status selector | SourceText/SourceReference with existing Tokens and exact-spec validation | Parser/schema tests, child/undeclared/cycle fixtures and coding stories | PASS |
| REQ-013 same exact owner occurrence, metadata.name, pinned source version, zero/multiple/tuple ambiguity refusal | EffectiveSpecs/source_binding/GroupSite; BindingProjector::group_sources | Sibling/external/ambiguous/undeclared/cycle registration and wrong-owner/tenant/new-version stories | PASS |
| REQ-002 ordering and aligned null/failed values across input/outcome references | Ended::executions, Payloads::value and selected_results | Unit order/value checks; frame IDs in submission order, coding failed outcome, late-work null→actual | PASS for projection; authority/coverage race below remains |
| REQ-003/004 serial outstanding work, immutable snapshots/retries, independent groups | binding SKIP LOCKED, unique outstanding index, frozen queue input and fencing | SQL group execution serialization/dispatch; Rust late-work, frames, replica/restart | PASS |
| Amended finding 1 discovery policy, no completion watermark/hour cutoff/cursor | candidates finality HAVING/newest page and existing queue covers | Late-work/coalescing and restart stories; accepted page ceiling | PASS for amended policy; eligibility and authority gaps below are separate |
| REQ-003/004 safe duration domain, pinned settings and no producer finality refusal | checked chrono operations, elapsed comparisons, GroupEndSettings | largest_accepted_durations_judge_without_overflow; neighboring forever binding; refund | PASS |
| REQ-005 selected authoritative queued outcome and later available result reaches next serial review | selected_results used only during snapshot, after coverage decision | Ordinary late-work proves newly arriving result; no already-visible/unselected→selected race | FAIL — BEH-R3-001 |
| REQ-005 existing outbox loss, visibility trade-off, no new producer receipt/lifecycle store | Existing outbox/submission/runner paths retained | Published frame restart positive variant and static source trace | PASS |
| REQ-006/009 one identity selection owns capacity and membership; no over-limit payload reads | selection plus ranked identities; Ended::over before snapshot; exact readable-ID payload predicates | Refused-at-ceiling integration, stable mixed tuple/frame 20/21 and SQL predicate unit | PASS for the R2 count/list correction |
| Refused submissions are not source rows and cannot anchor discovery/finality | candidates aggregates raw surfaces before selection's refusal filtering | Refused-at-ceiling case puts rejected attempt after the first accepted row | FAIL — BEH-R3-002 |
| R2 FIND-2 explicit successful-terminal incomplete selected-payload deferral/recovery proof | Sources::complete and snapshot return None | incomplete_selections_defer tests only an in-memory map predicate; new seam truncates discovery transport | FAIL — BEH-R3-003 |
| REQ-008 non-vacuous each everywhere, nonempty comparison/decide preserved | Shared Vala engine empty-array rule | Inline/step engine proof; three SDK empty story; MCP empty frames | PASS |
| REQ-002/011 declared live query, narrow tool set, existing authority, no span injection | Agent validator/executor, HostedTools and BoundedQuery | Coding request contains IDs and later tool results; Rust independent tool/query denial; owner source-query denials | PASS for implementation |
| AC-003 pin group inputs before later span publication, then explicit query sees it | Live tool implementation allows it | All three coding bodies publish late span before establishing a pinned group execution | FAIL — BEH-R3-004 |
| REQ-010 first failed baseline, change-only dispatch including recovery; replay fence; ordinary reactions | queue complete/PREVIOUS_GROUP/INSERT_DISPATCH and existing worker | SQL failed→failed→passed baseline/replay proof; refund passed→failed→failed story; ordinary schedule fixture | PASS |
| REQ-011 source reads use accepted owner's grants and ordinary audit | Caller::accepted → authenticated ScheduledQueryCaller | grouped_reads_need_the_owner_grant denial/scope/regrant/audit assertions | PASS |
| REQ-011 bounded inspection/generated/runtime parity | Shared coverage/settings/state, schema projection, authored stubs and generated TS | OpenAPI, schema decode parity, TS negative type assertion, SDK/MCP inspections | PASS |
| AC-004 agent/media scoped workflow, failed/empty/overflow/isolation | Existing evidence plans/upload/execute/enqueue and source owner | SDK frames plus MCP grouped_frames_over_mcp_are_bounded | PASS for exercised stable selections |
| AC-006 deliberately partial internal read, no enqueue, later complete immutable snapshot | ScheduledQueryCaller now applies controller; poll ? aborts affected read | New pg_verification_groups::partial_group_reads_enqueue_nothing_until_a_complete_poll | PASS; closes previous transport-only remainder of FIND-8 |
| Prior rustdoc gate and active authority tool exception | FrameAgent Panics added; original duration/field/module docs; design/evaluation synced | Source inspection | PASS |
| INV-001–005 ownership, exact tuple/tenant scope, portability, one outbox and exclusions | Existing owner graph described above; no platform mechanism/new lifecycle | Cumulative diff/source audit; no new runtime certification claimed | PASS apart from identified behavioral/proof gaps |

## Proposed source-local findings

These are independently discovered proposals for validation, not a final deduplicated ledger. Existing FIND IDs should be retained for prior obligations; no retroactive ID is assigned to a human amendment.

### BEH-R3-001 — INCORRECT — a visible queued result can be covered while its selected outcome is null, then never reconsidered

**Obligation:** REQ-002/003/005 and AC-003/006 require authoritative queued outcomes, genuine nulls while absent, then a subsequent serial snapshot when the actual selected result becomes available, without editing history. This revisits the earlier FIND-TASK-001-1 late-work outcome, without reopening its accepted discovery/page amendment.

**Locations:** `crates/wyrd/wyrd-server/src/verification/groups.rs:139–151` compares queue coverage before reading result authority; `:311–324` obtains selected_results later; `:421–432` omits a queued result when its SQL selection is None; `:638–649` permits that genuine queued null. `Ended::judge` at `:750–761` counts every visible result surface by execution. `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:992–994` treats unchanged newest/member count as covered.

**Reachable producer → consumer:** runner::settle (`verification/runner.rs:474–525,565`) applies queue completion within its transaction, takes/stages the result on ScribeOutbox, and only then commits. Its documented failed-commit path and attempt/persist (`:708–715,753–759,942–960`) retry the same transition/result identity without staging it again while the lease holds. Thus a result can already be visible in Bifrost while another transaction still sees the source execution as running/result_id=NULL. This is explicitly permitted by existing staging/commit ownership, not hypothetical malformed storage.

For a max_age-due group still within finality, a group poll during that interval selects the input and visible result row. selected_results returns `(execution, None)`, so Payloads correctly contributes null. But Ended/coverage already count that result surface as covered. Let settlement retry succeed, selecting the exact result already visible. No additional Bifrost row is written. Every later poll has identical through/members, so covers() skips before consulting selected_results: the group remains permanently judged over null despite the selected result being visible. A simple delayed source result story does not hit this ordering. No claim of a healthy-path deterministic reproduction is made; this is a source-proven allowed failure/recovery interleaving.

**Sibling trace:** input/outcome slots share the same selected identity and coverage; all queued turn writers use runner settlement; direct results use the separate direct authority path; manual, group and scheduled execution result selection share selected_results. The source of the error is coverage claiming unselected result activity, not Vala null comparison, Operator delivery, or the Scribe durability boundary.

**Smallest correction:** reuse existing GroupPoller selection and queue selected_results authority before deciding which result activity a snapshot covers. An unselected queued payload must not permanently count as coverage of the eventual selected outcome. When that already-visible result becomes authoritative, the next poll must see uncovered evidence. Preserve genuine input-only null snapshots, direct authority, serial/outstanding fences and immutable historical inputs. Do not add a readiness barrier, producer IO, another result inventory, or move result staging after commit; that would change adjacent durability semantics. Filtering/coverage belongs at the source-selection owner because it defines the values the snapshot actually consumes.

**Focused proof:** through the existing group/queue seam, make a source result visible before its SQL completion commits; allow one due group snapshot to pin null; then complete/select that same visible result without republishing it. After prior group settlement, exactly one next whole snapshot must contain its actual outcome, and the old null snapshot stays unchanged. An initially visible abandoned result plus later authoritative selection should obey the same authority, without reading abandoned outcome values.

### BEH-R3-002 — INCORRECT — refused rows still establish discovery finality before eligibility is resolved

**Obligation:** REQ-001/004/005/009/013, R2 FIND-TASK-001-2 and the user's refusal exclusion: refused submissions are not source executions; finality anchors on the first eligible source row. This is the same source-eligibility boundary as prior FIND-2, applied to the earlier discovery consumer.

**Exact location:** `verification/groups.rs:201–221` computes HAVING MIN(at) on raw `surfaces`; refusal filtering only occurs after a candidate was admitted at `:248–278`. `Sources::surfaces` at `:571–588` applies owner/tuple/group predicates but no accepted/refused classification. Queue authority is REFUSED_SUBMISSIONS_SQL and refused_submissions in `wyrd-sql/src/queries/verifier_executions.rs`.

**Reachable path:** an original keyed enqueue under group A is accepted. Reusing that key with different request group B stages a distinct B input before the submission writer settles it as an idempotency conflict. That refused row may be B's only row. After B's short configured finality has elapsed, the caller sends B's first legitimate turn using a fresh key. Its first eligible source is current and within finality, but discovery's raw MIN(at) still names the old refused input. The HAVING clause permanently excludes B before selection can remove it. This occurs with a single group, not an accepted newest-page starvation case, and does not require any producer contract change.

**Observable consequence:** a valid ended B group never receives its required group judgment. The new refused-at-ceiling test starts with an accepted same-group row, so its conflict cannot alter the anchor and does not prove this path.

**Smallest correction:** apply existing queue refusal authority consistently before a refused row can disqualify a group at the discovery/finality decision. Reuse the current source selection/query owner, keep the accepted first-eligible-source finality/newest-page policy, and do not add a continuing cursor or producer lookup. A raw prefilter cannot be final when its excluded rows are not source activity. No new durable group or receipt store is warranted.

**Focused proof:** in the existing bound-server group fixture use short finality and server-owned fixture row times: retain an old refused-only B attempt and a new first accepted B source, then tick. B must enqueue one normal snapshot containing only its accepted source. Retain finality refusal for a genuinely old accepted first row and ordinary N/N+1 refusal exclusion.

### BEH-R3-003 — MISSING — the explicit successfully terminated incomplete-payload recovery proof remains absent

**Obligation:** R2 FIND-TASK-001-2 focused closure explicitly requires a successfully terminated but incomplete selected-payload read to defer, then recover, preserving genuine absent-result nulls. The same obligation is in R1 FIND-2 and AC-006's complete selected snapshot boundary.

**Location/evidence:** `verification/groups.rs:311–334,632–653` owns payload completeness; `:1178–1237` incomplete_selections_defer only mutates Payloads maps and calls complete(). New `tests/integration/pg_verification_groups.rs:268–313` arms EOF on the first statement, discovery, then proves transport retry. Its `:323–356` stable ceiling test does not hide a selected payload. No test drives successful terminal/selected-ID omission through snapshot → no enqueue → later complete poll.

**Consequence:** the specifically retained acceptance seam remains unproved; this is not a claim that the current complete() guard is broken. Transport rejection and a unit predicate do not establish the changed group queue/cadence behavior for successful SQL with incomplete selected coverage.

**Minimum correction/proof:** one focused case through the existing poller/queue/read fixture with a selected input or direct-result payload temporarily missing while the statement terminates normally. Assert no affected prefix enqueues, make the complete selected payload visible, then assert exactly one complete immutable snapshot. Reuse existing query/group owners and local test plumbing; no parser, public fault API, generic harness or language suite. Keep this under stable FIND-TASK-001-2.

### BEH-R3-004 — MISSING — live-span proof still publishes the late span before establishing the pinned snapshot

**Obligation:** AC-003 explicitly says: pin group inputs before a later span is flushed, then let the Agent query after that span is visible; pinned trace IDs stay unchanged. This was an explicit R1 FIND-TASK-001-8 subcase. The prior verdict's closure claim is not supported by current source ordering.

**Exact locations:** Rust `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:1770–1778` creates late-test-run and force-flushes before the first group_execution call. Python `tests/integration/test_verify_in_real_time.py:663–669` does the same with state.flush; TS `tests/integration/verify-in-real-time.test.ts:703–707` exports/shuts down the provider before groupExecution. The coding Trigger has idle=3s and max_age=1h; no earlier assertion or barrier establishes snapshot pinning. Reading tool results containing the span at Rust `:1824` only proves it was present by query time.

The late-work story concerns queued requirement results, not trace publication. Owner-grant/no-Agent stories credibly cover no implicit query and ordinary source-read permissions; neither proves this temporal live-data variant. Python/TS combined-role denial does not reopen a separate finding because the Rust story independently exercises the shared tool/query boundaries.

**Consequence:** the approved live-versus-pinned acceptance behavior is still not demonstrated. No production automatic span-fetch bug is asserted.

**Smallest correction/proof:** adjust one focused shared Rust real-server coding story to establish a pinned group execution and hold its judge before the declared query; then publish the later span, release the Agent and assert the live tool result includes it while the stored input snapshot remains the same IDs. Use existing queue inspection/local provider synchronization, not a new span snapshot product or general harness. Keep stable FIND-TASK-001-8, and keep the newly completed partial-read proof closed.

## Prior finding closure, independently checked

| Stable ID | Source-backed result |
|---|---|
| FIND-TASK-001-1 | Original completion watermark/hour bug removed; accepted first-source/newest-page amendment preserved. Late-work coalescing/null→new-row proof present. **Residual authority-transition coverage path proposed in BEH-R3-001**; no cursor demand. |
| FIND-TASK-001-2 | Old raw skip, broad payload predicates and separate count authority removed. Ranked one-result capacity, refused compensation and exact readable payload IDs present. **Residual discovery refusal eligibility and required incomplete successful-payload proof: BEH-R3-002/003.** |
| FIND-TASK-001-3 | CLOSED: checked finality timestamp arithmetic and elapsed comparisons; max accepted duration and neighboring ordinary scheduler story inspected. |
| FIND-TASK-001-4 | CLOSED: source-owned schema contains four typed objects or string; runtime and served projection/type negative agree. |
| FIND-TASK-001-5 | CLOSED: accepted owner Caller/authenticated query used for every group statement; denial/scope/regrant/audit story; Drift exception preserved. |
| FIND-TASK-001-6 | CLOSED: duration Errors, SourceText/Forward fields and groups module documented; FrameAgent::submit now documents rejected-PUT panic. |
| FIND-TASK-001-7 | CLOSED: active design/evaluation now describe only declared bifrost.query, ordinary grants, no workspace/mutation, live spans outside snapshot. |
| FIND-TASK-001-8 | Registration/external/ambiguous/missing-target and MCP-media additions inspected. R2 internal fault reaches actual scheduled consumer; explicit discovery after-batch/no-enqueue/retry proof closes that remainder. **Pin-before-late-span proof remains BEH-R3-004.** |

## Verification limits and correction discipline

The task appendices claim green focused units, group server seam tests, seven Rust journeys, MCP/query lifecycle tests and format/lint/tenancy checks. I did not rerun or certify those claims; root's fresh sequential runs are forthcoming. The new partial-discovery test reaches the real internal consumer because scheduled.rs calls the same controller apply used by BoundedQuery under test-support. The test does not prove a successful incomplete SQL payload selection or pin-before-span timing merely because it is green.

All proposals retain the existing owners. Delete false coverage/eligibility authority where possible; reuse queue refusal/selected-result classification, GroupPoller, current SQL/Arrow/query semantics and installed collections before adding code. No new dependency, lifecycle resource, admission system, source readiness barrier, schema upgrade, producer IO or platform-specific mechanism is selected. A single shared selection correction can close multiple affected source consumers; do not repair the same invalid state separately in each SDK, evaluator or Operator.

**Overall result: FAIL.** Material source-backed eligibility/coverage paths and explicit acceptance-proof gaps remain. The independent validator should reconcile stable IDs and the smallest shared correction boundary; this report does not supply a final task verdict or remediation artifact.

Root update received after this report was written: seven exact Rust grouped journeys, six group units and both new managed-Postgres group seam tests independently passed. These results support the exercised bodies and do not execute the separately identified source interleavings or omitted temporal/payload proof variants. I ran no additional command on that account.
