# Independent structured Ponytail validation — TASK-001

Result: **FIX_REQUIRED**. Retain **FIND-TASK-001-2, FIND-TASK-001-6 and FIND-TASK-001-8**. The two ceiling failures share one source-selection invariant. Missing partial-read proof is an acceptance gap, not proof that the shared terminal consumer is defective. The new mandatory rustdoc omission is real although the original documented sites are fixed. No new FIND number or material specification decision is needed.

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Candidate: `47e0c9ab7a5cf91cb8d292bdd96a86bdd966a3f9`.
- Approved behavior: revision **13** of `changes/active/long-horizon-verification/spec.md`, under explicit user authority. The draft-14 header/history discrepancy is recorded; the draft schema-reset exception supplies no authority. No retained-table upgrade requirement is invented.
- Original task: `tasks/TASK-001-grouped-verification.md`; prior verdict, ledger and R1 task: `review/task-001-e4b1e313a-20261017/`.
- User amendment `f983d7b95` governs prior finding 1: first source within pinned finality, newest activity first, only the newest bounded page each tick. No continuing discovery cursor is required. The accepted page ceiling differs from REQ-009's eligible source-execution ceiling.
- TASK-002 schedule inputs, TASK-003 staging/pressure/progress and REQ-015 capacity qualification remain excluded.

Read all seven fresh reports: behavior, invariants, standards, maintainer, system, security/tenancy and data/concurrency. Compared proposals by invariant, then independently checked producer, selector, queue, evaluator and proof source. CodeGraph preceded navigation. The acceptance range is cumulative base-to-candidate; the remediation range was navigation only. Applied AGENTS, agent-rules, spec-driven development, maintainer style, design/doctrine, Bifrost visibility/terminal authority, reference router and relevant testing, contract, evaluation and tenancy guidance. Inspected manifests, lockfile and mise recipes: chrono is locked to 0.4.45, and the dependency delta is installed workspace sha2 as an MCP dev dependency.

No source/test mutation or Cargo/test command was performed. Only this report is written. Root owns sequential runtime verification. HEAD remained the candidate; concurrent untracked review reports are not source changes. A bare Python attempt to write this report failed at the unavailable mise shim before writing anything; the report was then written through the shell. This was not a test failure.

## Source and correction map

| Producer / authority | Callers, siblings and correction-owner bodies inspected | Correction implication |
|---|---|---|
| Observe/Target and Python/Node projections | verify, staged submit, enqueue request, GroupId and metadata projections | One shared implementation; no SDK-specific selection fix or producer IO. |
| VerificationControl enqueue/stage_direct and ResultPayloadBuilder | Inputs staged before asynchronous queue acceptance; direct and queued result writers retain exact tuple metadata | Retained input does not imply acceptance. Preserve enqueue/idempotency and outbox loss. |
| Queue accept_submissions/accept/insert_submission, refusal SQL/refused_submissions, selected_results | Conflicting attempt is errored under its distinct ID; manual errored attempts=0 identifies refused evidence; selected_results separately owns queued outcome authority | Reuse refusal authority before deciding source capacity; preserve queued result selection. |
| GroupPoller::poll; all BindingPoll methods; Sources new/any/surfaces/identities/payloads/rows; Ended judge/eligible/executions; Payloads::value | Counts → over → later identities → refusal filter → timing/coverage → payloads → GroupSnapshot; full correction-owner bodies read | Shared source for both ceiling failures; all input/outcome source slots consume it. |
| Scheduler tick/poll_groups; queue claim_due_group/group_states/enqueue_group/complete and coverage/outstanding SQL | Lock/cadence, owner authority, caller transaction, stored snapshot, leases/unique fence, baseline/change dispatch | Preserve owners. Failed read may advance cadence, never pin that group's prefix; next tick retries. Earlier whole groups may remain enqueued. |
| ScheduledQueryCaller authenticated/run_with/dispatch/consume_to_terminal; query service; Drift; BoundedQuery | Groups dispatch directly to stream_query. HTTP and BoundedQuery claim test stream faults separately | Existing terminal-safe consumer is appropriate; HTTP fault knob does not automatically reach groups. Preserve Drift's separate exception and ordinary permissions/audit. |
| EffectiveSpecs/source_binding/GroupSite, BindingProjector, server/loader validators | Same-site exact sources, ambiguity/cycles, reserved slots, artifact-manifest rule | Existing owners cover workflows; no replacement resolver/loader. |
| Engine/runner/source-limit outcome | Empty-each, source conformity, narrow check-Agent query exception, pinned marker | Runner correctly obeys marker; fixing after payload collection is too late. Genuine absent outcomes remain null. |
| Rust/Python/TS grouped stories; SQL, MCP and FrameAgent | Stable 20/21, below-limit conflict exclusion, late work, grants, registration/schema/media/isolation/recovery | Current bodies support those scenarios; not count/read growth or partial group-read retry. Called FrameAgent helper has undocumented explicit panic. |

Compared existing workflows, not just leaf calls: queue acceptance/settlement, scheduled query admission/terminal settlement, EffectiveSpecs loading, Vala judgment and Operator delivery remain owners. Drift's observation-window reader does not implement grouped source lists. The group reader retains meaningful state and supplies the missing required selection workflow; deleting it or forcing it into Drift would not preserve the task.

## Every proposal: disposition

| Discovery ID | Disposition | Source resolution |
|---|---|---|
| BEH-R2-001 | REVISED | Both false overflow from refused rows and later-cut overflow bypass confirmed. One FIND-2 boundary; no extra progress requirement or mandatory third all-refused journey. |
| BEH-R2-002 | REVISED | Partial-read/retry proof absent. FIND-8 is a proof gap; qualify existing fault reachability rather than prescribe HTTP injection. |
| INV-2 | CONFIRMED | Earlier raw aggregate and later eligible membership differ. FIND-2. |
| INV-8 | REVISED | Same explicit proof gap; no broken-terminal claim. FIND-8. |
| DATA-01 | CONFIRMED | Later identity statement has no ceiling tied to its IDs; snapshot trusts old map. FIND-2. |
| DATA-02 | CONFIRMED | Over tuple omitted before refusal authority receives its IDs. FIND-2. |
| SYS-001 | CONFIRMED | Same ceiling invariant; max_age-due/in-finality group makes newer-cut judgment reachable. FIND-2. |
| SYS-002 | REVISED | Transport-failure proof stays FIND-8; incomplete selected SQL coverage stays FIND-2 closure, not another finding. Internal fault reachability qualified. |
| MNT-R2-001 | CONFIRMED | Claimed source bound is not enforced on actual membership; behavioral consequence, not preference. FIND-2. |
| MNT-R2-002 | CONFIRMED | Called FrameAgent::submit asserts accepted upload but lacks Panics. FIND-6. |
| SR-R2-001 | CONFIRMED | Same new helper omission; original missing sites are fixed. FIND-6. |
| Security/tenancy | Validated empty proposal set | Accepted owner Caller/authenticated reads, exact tuple/RLS isolation and ordinary tool grants present; no material domain finding. |

No whole proposal is rejected as fictitious. Rejected portions are assuming the HTTP fault switch already reaches GroupPoller, treating missing proof as an actual stream defect, mandatory broad language/harness expansion, and reopening the accepted discovery page. They are excluded, not optional advice.

No material discovery conflict remains. Original-doc-sites closure versus a newly added helper omission concerns different items. The internal/HTTP fault distinction is resolved from dispatch/claim source and changes proof mechanics, not the diagnosed runtime invariant. No unresolved source conflict requires another discovery pass or BLOCKED.

## Deduplicated ledger

### FIND-TASK-001-2 — REVISED / INCORRECT — capacity is decided on different membership than the snapshot

**Discovery IDs:** BEH-R2-001, INV-2, DATA-01/02, SYS-001, MNT-R2-001; incomplete-selected-membership proof portion of SYS-002.

**Obligations:** REQ-002/004/005/009, task discovery/snapshot bounds and prior FIND-2 remediation. Distinct eligible executions after exclusion/dedup determine per-reference capacity; exactly the ceiling is allowed; overflow produces the existing inconclusive result before that source's payload collection. Never judge a bounded prefix as a complete list.

**Locations:** `crates/wyrd/wyrd-server/src/verification/groups.rs:139–147` (classification before refusal), `:246–273` (count cut), `:282–297,631–637` (independent unrestricted identity cut), `:320–358` (readable list/marker), `:749–752` (approximation). Producers/authority: `components/verification/service.rs:610–647`; `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:259–265,1230–1385,1721–1743`. Consumer: `verification/runner.rs:420–430`.

**Evidence and consequence:**

1. Enqueue stages its input before background acceptance decides a key conflict. accept() inserts the conflicting attempt errored, attempts=0, under its distinct ID. With N=20 accepted executions plus one settled conflict in the same tuple/group, counts sees 21 and marks over. identities excludes that entire tuple, so refused_submissions never receives the conflict ID. Snapshot omits valid source values and pins exceeded_ceiling; runner returns inconclusive/source_execution_limit_exceeded although the permitted 20 should be judged. The support journey already produces/excludes such conflicts below the ceiling.
2. Bifrost independently pins each statement. An eligible already-recorded input may become visible between a count of 20 and identities. For a group already max_age-due and still within finality, identities returns 21; its SQL and Ended::executions impose no bound on that later set. Snapshot fetches all payloads and marks no overflow because the earlier map omitted the tuple. The ordinary evaluator can report passed/failed over 21. This is a permitted source interleaving, not an executed reproduction. Both statements can terminate correctly.

One cause owns both: an aggregate over another cut/filter is authoritative for later selected membership. Binding locks do not freeze Bifrost visibility. Query-wide admission does not enforce a per-reference ceiling on a valid small 21-row response.

**Selected minimum correction:** keep GroupPoller/Sources as the selection owner and queue refused-submission classification as acceptance authority. Determine overflow from the actual distinct eligible identity selection before any overflowing tuple's IDs become readable. Refused and finality-excluded rows cannot consume capacity. Keep identity selection bounded, establishing complete-within-limit versus genuine overflow; an earlier raw count can assist but cannot authorize a larger later list or establish false eligible overflow. When existing admission/read bounds prevent complete valid selection, defer/retry instead of pinning a prefix. Reuse existing exceeded marker/outcome, exact readable-ID payload predicates and selected_results. Preserve deterministic aligned lists, genuine nulls, fixed cadence, finality, outstanding fence and immutable retries. No producer IO, inventory/readiness store, new limit, cross-query snapshot service, SDK guard or post-payload evaluator workaround.

Private query layout/helpers/control flow remain implementation-owned. This selects existing owners and required outcome, not a new persistent-data or concurrency contract.

**Focused proof:** at N, N accepted plus a settled conflict must judge exactly N accepted IDs normally; N+1 accepted must return the existing limit reason without collecting that reference's payloads. Deliberately expose an additional execution between count and identities and prove no ordinary oversized snapshot or overflowing payload read; preserve mixed under/over tuples, exact-ID predicates, input/outcome alignment and genuine nulls. Exercise a successfully terminated but incomplete selected-payload read to establish deferral and later complete recovery if complete() is claimed as closure. Reuse existing group/query/queue owners and exact selectors; no host load, generic harness or broad suite.

### FIND-TASK-001-6 — CONFIRMED / VIOLATION — new helper lacks mandatory panic contract

**Discovery IDs:** SR-R2-001, MNT-R2-002.

**Obligation:** AGENTS §16/agent-rules require Panics when a panic remains possible on new/materially changed helpers, including private test code; task-review explicitly retains this hard gate.

**Location:** `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:1547–1555`, FrameAgent::submit; assertion at `:1585–1588`; called by grouped_frames_over_mcp_are_bounded.

**Evidence/consequence:** Result docs describe tool/plan/upload errors, but non-success HTTP status reaches assert!, which panics, with no Panics section. Mandatory documentation gate fails. This is not a production upload-crash claim and does not reopen fixed original items.

**Selected minimum correction/proof:** document the rejected-upload panic while retaining the assertion/error behavior. Nearby group_binding separates Errors/Panics. Batch this prose correction with behavioral/proof remediation, source-inspect and run applicable existing format/lint checks. No behavior change/test, checker, dependency or separate style round is needed.

### FIND-TASK-001-8 — REVISED / MISSING — no deliberate partial grouped-read rejection/retry proof

**Discovery IDs:** BEH-R2-002, INV-8, SYS-002 transport-failure portion.

**Obligation:** AC-006 (`spec.md:1026–1032`), original task scenario 5 and R1 finding 8 explicitly require failed/truncated discovery/snapshot reads to produce no partial judgment and later retry successfully. FIND-2 separately owns SQL membership completeness/overflow proof.

**Locations:** Rust `tests/integration/verify_in_real_time.rs:2800–2900` owner-grant story; `verification/groups.rs:311–393,480–523,970–1250` snapshot/complete/run and five units; R1 evidence row 8e. Existing protocol evidence: `query/scheduled.rs:133–185,259–342,434–540`.

**Evidence/consequence:** denial/regrant credibly proves before-stream failure/retry, not partial delivery. Five group units exercise timing/SQL/value mapping, not partial snapshot/complete. Existing unchanged scheduled_terminal_requires_clean_eof proves shared consumer rejects failed/missing/malformed terminals after a batch. BindingPoll::run appropriately keeps batches local until run_with succeeds. Those support production safety but do not exercise changed grouped queue/cadence behavior after partial delivery. No new test asserts prefix not enqueued followed by successful complete retry. Explicit acceptance proof remains missing; no actual defective terminal consumer is asserted.

**Selected minimum correction/proof boundary:** one focused test through the real grouped polling/snapshot and queue/cadence owner: deliberately fail/truncate a discovery or snapshot statement after partial delivery; assert that affected group's prefix is not pinned/enqueued; then allow a later poll to produce one complete immutable snapshot. Reuse existing ScheduledQueryCaller terminal/frame facilities and server/queue fixture. A narrow test-only adaptation within that owner is sufficient implementation plumbing; no new parser, transport, generic harness or public fault API.

**Critical reachability qualification:** WyrdTestServer::fail_next_query_after_batch arms QueryStreamFaultController, claimed by HTTP `query/routes.rs:299–324` after stream_query and separately by BoundedQuery `query/collect.rs:288–305`. GroupPoller → ScheduledQueryCaller::dispatch (`query/scheduled.rs:194–212`) calls stream_query directly and claims neither. Merely arming that HTTP knob cannot prove group truncation. Target the actual internal query boundary; preserve existing fault behavior for siblings and keep any adaptation test-only/local. Do not route groups through HTTP or add a product fault subsystem.

Use exact owning selector; one shared Rust-runtime proof suffices here. No Python/TS restart controls, full language suites or broad aggregate. Successful terminal plus incomplete selected SQL membership remains FIND-2's distinct proof.

## Complete Ponytail correction ladder

| Step | FIND-2 | FIND-6 | FIND-8 |
|---|---|---|---|
| Delete | Delete raw count's authority over another set; cannot delete required ceiling/reader | No behavior change needed; docs gate cannot be waived | Cannot delete explicit acceptance; reject duplicate parser/harness |
| Repository reuse | GroupPoller/Sources, queue refusal/selected results, existing overflow marker | Existing helper docs/nearby pattern | ScheduledQueryCaller, group owner and queue/cadence inspection |
| Standard/conventional | Bound actual eligible deduplicated membership before payload; never prefix-as-complete | Errors/Panics distinction | Partial failed output invalidates statement; existing poll retries |
| Standard library/native | Sets/maps and existing timing suffice; native primitives do not unify separate query cuts | Rustdoc text | Existing async/stream test primitives |
| Installed dependency | Existing SQL/Arrow/query owners; no new dependency | None | Existing frames/terminal/fault facilities, with internal reachability respected |
| Minimum correction | Correct selection once and focused counterexamples | Add panic contract | Focused no-partial/retry proof and only local test adaptation if needed |

Payload completeness guard belongs at snapshot reader because it owns selected IDs; terminal guard belongs at stream trust boundary. A runner capacity guard after reads would duplicate the source invariant too late. Adjacent admission, authorization, durability and sibling availability remain untouched.

## Prior closure

| Stable ID | Source-backed disposition |
|---|---|
| FIND-TASK-001-1 | **CLOSED under user amendment.** Completion watermark/hour suppression removed. Checked first-source finality, newest ordering and accepted one page; queue covers/outstanding authority. Late-work asserts coalescing/null→actual follow-up without old-snapshot mutation. No cursor demand survives. |
| FIND-TASK-001-2 | **PARTIAL, retained.** Silent raw skip removed; known-over payloads omitted; both SQL payload reads restrict readable IDs; complete() defers missing selected inputs/direct results. Actual eligible membership still differs from count/cut, and incomplete-coverage recovery proof remains absent. |
| FIND-TASK-001-3 | **CLOSED.** Checked date operations/elapsed comparisons and span handle accepted maximum units; largest-duration and neighboring-work cases exist and root reports green. |
| FIND-TASK-001-4 | **CLOSED.** Source-owned schema is four typed objects plus source string; regenerated TS excludes phantom source object; runtime/schema/type-negative and served OpenAPI agree. Root focused schema/OpenAPI checks green. |
| FIND-TASK-001-5 | **CLOSED.** All group reads use Caller::accepted/authenticated scheduled query from recovered authority; no grouped manufactured SYSTEM grants. Owner capability/object denial/regrant story covers boundary. Drift exception unchanged. |
| FIND-TASK-001-6 | **Original sites closed; retained for new omission.** Duration Errors, SourceText field, groups test module and Forward sender docs present. New called FrameAgent helper lacks Panics; same rustdoc boundary, different source item. |
| FIND-TASK-001-7 | **CLOSED.** Active design/evaluation describe declared bifrost.query only, ordinary permissions, no workspace/mutation, live spans outside snapshot; validator/runtime agree. |
| FIND-TASK-001-8 | **PARTIAL, retained only for partial-read proof.** SDK/HTTP reserved slots, external/missing/ambiguous sources, independent tool/query denial, live exported spans and MCP media negatives added. Partial delivery/no-enqueue/retry still absent; pre-stream denial differs. |

## Verification and limits

Root reports fresh green: build:monty-worker; five exact group units (5 PASS, 547 skipped); seven exact Rust grouped journeys (7 PASS, 121 skipped; late-work 43.976s, owner grants 20.602s); three exact server-feature spec schema/manifest cases (3 PASS, 764 skipped); two exact managed-Postgres MCP support/media cases (2 PASS, 23 skipped); two exact test-support server OpenAPI/artifact refusal cases (2 PASS, 270 skipped). py:setup and ts:build:testing PASS; two exact Python support/coding cases PASS (20.16s); exact-prose TS support/coding selection PASS (2 PASS, 22 skipped, 25.29s). Canonical ts:typecheck also PASS, including its production/testing native dependencies. Managed Postgres used the wrapper/db:migrate:all:inner and Cargo-backed lanes were sequential. Root reports no test failures, tracked diff and whitespace checks clean, HEAD unchanged and reports-only untracked changes.

I ran no test/check lane. Other appended static/boundary/docs claims are not independently certified unless root reconciles them. Current green bodies do not prove the uncovered source interleavings or missing acceptance scenario. Correct stream termination does not prove SQL membership completeness; accepted discovery page does not waive eligible source capacity.

All retained corrections fit revision 13 and existing owners. No SPEC_REVISION_REQUIRED decision is established. A proposed new durable inventory, producer lookup, replacement transport, changed permission/finality contract or general fault subsystem would exceed this ledger and is not authorized by it.
