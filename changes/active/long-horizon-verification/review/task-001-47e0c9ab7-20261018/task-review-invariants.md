# TASK-001 independent invariant review

Result: **FAIL**. The source-selection ceiling still has reachable correctness gaps, and the explicitly required partial/truncated group-read proof is absent.

## Subject and authority

- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Immutable candidate: `47e0c9ab7a5cf91cb8d292bdd96a86bdd966a3f9`; HEAD was this candidate at inspection.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-001-grouped-verification.md`.
- Approved authority: revision **13** of `changes/active/long-horizon-verification/spec.md`, as explicitly selected by the user. Its revision-14/draft header is a recorded authority mismatch. The unapproved schema-reset exception is excluded; this review invents no retained-state upgrade obligation.
- Prior inputs: `review/task-001-e4b1e313a-20261017/{verdict.md,findings-validation.md,TASK-001-R1-close-grouped-verification-gaps.md}`. Stable finding IDs are preserved below.
- Reviewed the cumulative `git diff ef3a57b07...47e0c9ab7`; the remediation range was used to locate changed owners, not as the acceptance subject. No other current-round reports were read. No source/test edits, Cargo commands or test executions were performed. The only write is this report.

Applied AGENTS, agent-rules, spec-driven-development, maintainer style, the applicable verification sections of wyrd-design/doctrine, Bifrost design, evaluation guidance and TESTING. Inspected manifests, lockfile and mise lanes for the touched owners. Used CodeGraph before locating/reading code.

TASK-002 scheduled-input simplification, TASK-003 staging/pressure/tenant-progress work and capacity qualification remain excluded. The user's accepted discovery policy is first-source-within-pinned-finality, newest activity first, one newest page per poll, with its `ponytail:` ceiling. No continuing cursor or broader progress guarantee is required here.

## Producer-to-consumer coverage

| Authority / value | Producers, siblings and consumers traced | Assessment |
|---|---|---|
| Caller group and authenticated owner | Shared Observe/Target; Python Observe; TS Observe/native Run; VerificationRequest; Principal owner accessor; verification service; ResultPayloadBuilder; submission acceptance; runner result publication; managed input/result tables | Optional group spelling survives the wire and staging; owner is copied without new producer IO; ordinary ownerless invocation remains permitted. |
| Source identity and declarations | Reference parser/SourceText, InputSlot, slot validation; loader validation; server validate_request; EffectiveSpecs binding/source resolution and cycle walk; BindingProjector group_sources; generated TS and served schema tests | Existing registration and projection owners are extended. Exact source tuples are pinned; unsupported child/status/reason forms are not introduced. |
| Group discovery and selected membership | Scheduler, BackgroundAuthority, GroupPoller/BindingPoll, Sources, Ended, Payloads; queue refused_submissions/selected_results/group_states/enqueue_group; hosted engine | The query owner now preserves accepted authority and terminal settlement. The separate preliminary count is incorrectly treated as authority for the later selected list; see INV-2. |
| Snapshot lifecycle | Binding cursor claim, queue partial unique outstanding index, frozen input/coverage columns, claims/leases/settlement, runner retry inputs, retained results | Same binding/group serializes; inputs and coverage persist on the execution; replayed settlement cannot produce a second logical dispatch. |
| Result interpretation and reactions | Vala assertion/blocks engine, check-Agent toolset/hosted tools, engine limit outcome, queue complete/previous-group/dispatch context, Operator rendering | Empty each is inconclusive; live Bifrost tools retain ordinary permissions and stay outside pinned inputs; baseline/unchanged/changed verdict handling remains owned by settlement. |
| Real caller proof | Rust/Python/TS verify-in-real-time stories/support; MCP JSON/media stories; OpenAPI and HTTP registration tests; SQL group settlement test; schema/type tests; design/site docs | Main paths and many missing negatives are added. Group-specific partial/truncated read refusal and retry is not deliberately exercised; see INV-8. |

The meaningful new group reader has a missing group-source analytical selection responsibility. Existing Drift observation-window reading does not implement this workflow; reusing its SQL literal rendering is sufficient and reusing its privileged observation authority would not be. The remediation correctly removes that privileged use and retains Drift's own behavior. Queue/query/evaluator/registry workflows remain with their existing owners. No second transport, group lifecycle table, membership store, media processor or fan-out engine is justified or introduced.

## Cumulative acceptance matrix

PASS below is a source/proof assessment, not a claim that I independently ran the appended commands. Parent-coordinated verification is forthcoming.

| Requirement, criterion, constraint or non-goal | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| REQ-001/011 optional `group_id` across all producer surfaces, omission stays ungrouped | Observe/Target, request contract, Python and TS/native projections, service/results/table columns | Three SDK support/recovery stories; MCP grouped execute/enqueue; served contract | PASS |
| REQ-001 reserved typed/source `inputs.group_id`, ordinary renamed slots | validate_slot; server validate_request now calls VerifierSpec::validate; loader shared validation | SDK coding registration refusals/no persistence; raw HTTP reserved-slot refusals; ordinary renamed fixture slots | PASS |
| REQ-014 owner metadata without new hot-path lookup/write | Principal::owner_service_uid; enqueue/stage_direct; ResultPayloadBuilder; runner fallback | Principal unit proof; ownerless/Service/UID invocation source trace and support story | PASS |
| REQ-002 minimal whole-slot/outcome reference grammar | SourceText/SourceReference parser and shared validation | Parser/source-slot negatives; coding-child refusal | PASS |
| REQ-013 same exact owner occurrence, source name and pinned UID/version | EffectiveSpecs/source_binding/GroupSite; BindingProjector::group_sources | Sibling/registered external fixtures; ambiguous/unbound/missing-target/cycle refusals; newer-version exclusion | PASS |
| REQ-013 tenant/owner/subject/Verifier/group eligibility | Sources::eligible and group predicates; TenantConn/RLS | SDK other-owner/tenant/new-version/ungrouped paths; MCP cross-tenant read refusal | PASS, subject to eligible membership count defect below |
| REQ-002 deterministic source ordering, alignment and nulls | Ended::executions; Payloads::value; selected_results | Ended/Payloads units; coding input/outcome lists; late-work null then selected result | PASS |
| REQ-003 immutable whole snapshots/retries/history | GroupSnapshot/GroupCoverage frozen on queue rows; runner Grouped input | SQL group claim/settlement; Rust restart/frame and late-work stories | PASS |
| REQ-003/004 one outstanding binding/group; coalescing arrivals | Binding claim transaction and partial outstanding unique index; group_states | SQL outstanding enqueue; Rust late-work story and recovery/replica story | PASS |
| REQ-003–005 late eligible source/result visibility reaches later serial snapshot | No completed-at discovery suppression; queue through/member coverage | grouped_reviews_cover_late_work; frames recovered-result follow-up | PASS within accepted discovery page/finality bounds |
| REQ-004 discovery policy and duration domain | candidates checked subtraction; Ended elapsed/checked addition; fixed cadence | Extreme-duration unit and forever binding beside late-work progress | PASS; no cursor obligation |
| REQ-003 finality is background-only, no producer refusal/deletion | Discovery/Ended eligibility; ordinary producer path unchanged | Refund after-finality turn still judged without another group review | PASS |
| REQ-005 one outbox and accepted loss boundary | Existing source/result ScribeOutbox; queue handoff; no new source ledger | Source trace and flush-before-restart positive variant | PASS |
| REQ-006/009 selected logical ceiling before payload reads | counts/identities/snapshot use different membership authorities | Normal exactly-20/21 media stories; no refusal-adjusted or changed-visibility proof | **FAIL — INV-2** |
| REQ-004/006 failed/truncated reads cannot yield a partial group, then retry | run_with terminal query owner; complete deferral; scheduler read-error handling | New owner-denial/regrant path proves discovery refusal; no deliberately partial/truncated source read | **FAIL — INV-8 proof gap** |
| REQ-008 empty each is inconclusive throughout | VerificationEngine::evaluate rule before shared item comparator | Inline and step engine test; SDK empty-each and no-frame group | PASS |
| REQ-002 explicit live spans; no automatic expansion | Trace declaration maps to ID; check-Agent BifrostQuery only; ordinary hosted tools | Coding ID-only prompt then live late span names; independent Rust tool/query denial; non-Agent support no judge requests | PASS |
| REQ-010 first result baseline, changed-only dispatch and ordinary on_failure | Existing complete transaction, previous-group query and context extension | Refund changed/unchanged story; SQL failed baseline/failed→passed/replayed settlement | PASS |
| REQ-011 accepted owner permissions for every source read, audit non-blocking | Caller::accepted plus ScheduledQueryCaller::authenticated; Drift untouched | Owner lacks query capability; scope excludes verification tables; regrant later succeeds; denial audit | PASS |
| REQ-011 execution/binding inspection, HTTP/MCP and typed schema parity | Group coverage/settings/status contracts; existing routes/tools; InputSlotSchema | MCP discovery/inspection/media; OpenAPI test; source schema runtime parity; generated TS negative object test | PASS |
| AC-001/J-001 resumed group and replay/conflict | Existing support story and service owners | Rust/Python/TS support; MCP support | PASS for exercised flows |
| AC-002/J-002 repeated whole-group judgment/finality/reaction | Group lifecycle/settlement above | Three SDK refund stories | PASS |
| AC-003/J-003 declared coding inputs, live query trade-off and resolution negatives | Registration and group/tool owners above | Three SDK coding stories plus added Rust independent denial/live variants | PASS |
| AC-004/J-004 media failure/zero/ceiling/isolation and recovery | Existing upload and per-frame queue, group list source | Three SDK media stories; MCP media evidence-plan/upload/enqueue; Rust restart | PASS for stable membership; INV-2 governs ceiling race/refused rows |
| AC-005/006 group coexistence/recovery and compatibility | Unchanged Window flow, queue/cadence and shared state owners | Group recovery beside drift submit; replica/restart quiet proof; source/contract negatives | PASS except explicit partial-read proof |
| INV-001–005 server ownership, portability, immutable judgments and excluded machinery | Cumulative owner changes; pure spec contracts; thin SDKs; existing query/queue/outbox | Static caller/dependency/diff assessment | PASS except source ceiling correctness |
| Prior mandatory docs and active architecture synchronization | Duration codec/SourceText/Forward/group test docs; design/evaluation narrow tool exception | Changed-item source and prose comparison | PASS for prior cited omissions |

## Proposed findings

### INV-2 — FIND-TASK-001-2 remains open: the ceiling is decided on a different set than the snapshot reads

**Classification:** INCORRECT. **Obligation:** REQ-009 counts distinct *eligible* source executions after eligibility/deduplication and permits exactly the ceiling, while REQ-006 forbids reading/evaluating a larger or truncated source selection. REQ-005 excludes submissions the queue refused from execution evidence.

**Locations:** `crates/wyrd/wyrd-server/src/verification/groups.rs:139–147` (count → overflow classification → identity/refusal filtering), `:248–253` (raw per-tuple count), `:282–297` (independent identity query), `:320–330,355–358` (list used without a ceiling recheck), and `:749–751` (comment acknowledging refused submissions in overflow aggregates).

**Producer-to-sink path and concrete consequences:**

1. The service stages an input row for an enqueue before background queue acceptance (`components/verification/service.rs:610–647`). Queue acceptance can settle an idempotency-conflicting request errored, retaining its distinct staged input row; `VerifierExecutionQueue::accept` and `refused_submissions` own that distinction (`verifier_executions.rs:1270–1340,1720–1748`). With ceiling 20, one group can contain 20 genuine source executions plus one such refused same-tuple input. `counts` returns 21 and marks the tuple over. `identities` omits the entire tuple, so its refused ID is never passed to `refused_submissions`. Every source slot of that tuple receives an empty list with `exceeded_ceiling=20`; runner returns `source_execution_limit_exceeded`, although exactly 20 eligible executions should be judged normally. The code comment is not approval to change REQ-009. The user's accepted one-page discovery limit is unrelated.
2. Conversely, Oracle visibility can change between the count and identity statements, as the approved multi-query model allows. Let counts observe 20 distinct executions. A 21st already-recorded input becomes visible before identities. The identity SQL has no statement limit or frozen count membership; it returns all 21. `Ended::executions` selects 21 eligible IDs, `snapshot` reads all their payloads, and no check compares that selected list against the ceiling. `exceeded_ceiling` remains absent because it is derived only from the old `over` map. Runner therefore evaluates 21 inputs and can produce a passed/failed verdict where the required result is inconclusive without reading that over-ceiling source payload. This is a source-validated permitted interleaving, not a claimed executed reproduction.

Both failures have one cause: a preliminary aggregate is treated as authoritative for a later, independently read and differently filtered selection. All source slot forms share this owner, so no SDK/tool-specific correction is needed. The normal 20/21 journey, SQL-string membership test and Ended aggregate unit do not exercise either gap.

**Smallest correction boundary:** keep selection with the existing GroupPoller/Sources and queue acceptance/refusal owner. Make the actual distinct eligible selected identities authoritative for the ceiling before payload reads, including queue-refused exclusion, and retain a bounded way to decide overflow without fetching arbitrary payloads. A preliminary raw count may be a bound/optimization, but cannot by itself establish eligible overflow or authorize a later over-ceiling list. Preserve accepted multi-query snapshots, deterministic aligned lists, selected queued results/nulls, query terminal handling, finality, one outstanding execution and the one outbox. Do not introduce a group inventory, producer IO, new limit or replacement reader.

**Focused closure proof:** same tuple with exactly ceiling accepted executions plus one settled conflicting submission must select/evaluate exactly ceiling and omit the conflict; a count/read visibility transition crossing ceiling must produce the existing named limit outcome without source payload reads. Reuse the current group/queue/query fixture and focused journey/seam, not a new harness or broad suite.

### INV-8 — FIND-TASK-001-8 is only partly closed: no deliberate partial/truncated group-read proof

**Classification:** MISSING. **Obligation:** original TASK-001 scenario 5/AC-006 explicitly includes `failed/truncated reads, snapshot retry` (`TASK-001-grouped-verification.md:164`); prior FIND-8 remediation explicitly requires no partial enqueue followed by successful retry using existing query fault/terminal facilities.

**Locations/evidence:** new `grouped_reads_need_the_owner_grant` (`sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:2800–2899`) revokes query grants and later restores them. It proves a failed **discovery authorization**, with no source batches consumed. The five group unit tests (`verification/groups.rs:970–1250`) cover Ended/SQL generation/value mapping, not `BindingPoll::complete` or a failed/truncated identity/payload read. The added MCP media story proves other negatives. No newly added test forces group discovery/snapshot to receive a partial/truncated statement and asserts no partial enqueue followed by retry. The prior incidental teardown error claim also does not force or identify that state.

**Consequence:** the specific cross-boundary safety/retry acceptance remains unproved. A clean successful stream and a permission denial before execution cannot demonstrate that a partially delivered identity/payload statement is discarded, or that missing payload coverage defers instead of pinning a partial judgment. This does not assert that `ScheduledQueryCaller::run_with` is defective; the source reuse is appropriate.

**Smallest correction/proof:** retain existing query owner and group reader. Add only focused proof on existing fault/terminal facilities that interrupts/truncates a group read after partial delivery (or exercises the real relevant bounded-output terminal), asserts no partial execution, and then succeeds on a later poll. Exercise incomplete selected payload coverage if claiming `complete()` closes it. No broad language-suite rerun, new transport parser, restart API or general harness is required. Other FIND-8 variants now have credible source/test bodies and are closed in this review.

## Prior finding closure

| Stable ID | Source-based disposition |
|---|---|
| FIND-TASK-001-1 | CLOSED under the user's accepted policy: completed-at suppression and idle+hour expiry removed; checked finality-bound discovery newest first; queue coverage/outstanding fence; late-work/null→actual result proof. Accepted newest-page ceiling is not reflagged. |
| FIND-TASK-001-2 | OPEN as INV-2. Exact readable-ID predicates and incomplete coverage deferral improve the previous payload-prefix defect; eligible membership and actual selected-list ceiling still diverge. |
| FIND-TASK-001-3 | CLOSED: checked/elapsed duration arithmetic; largest accepted domain unit and neighbouring runtime story. |
| FIND-TASK-001-4 | CLOSED: source-owned schema union now contains typed slots/string; generated TS removes phantom source object and type test rejects it; served schema tests agree. |
| FIND-TASK-001-5 | CLOSED: accepted owner Caller/authenticated scheduled query replaces manufactured grants; owner capability/scope/regrant/audit story; Drift authority remains separate. |
| FIND-TASK-001-6 | CLOSED for the prior named rustdoc omissions. |
| FIND-TASK-001-7 | CLOSED: active design/evaluation permit only explicit bifrost.query with ordinary permissions, no workspace/mutation and live spans outside snapshot. |
| FIND-TASK-001-8 | PARTLY CLOSED: reserved registration/shared server validation, resolution variants, independent live tool/query denial and MCP media negatives added. Deliberate partial/truncated group-read retry proof remains INV-8. |

## Verification limits

Read the appended implementation/remediation verification claims; no command was independently executed by this reviewer. Those claims include seven Rust grouped journeys, focused schema/HTTP/SQL units, two MCP grouped stories, two Python and two TS stories, and formatting/lint/codegen/typing/boundary checks. The parent subsequently reported `build:monty-worker` PASS, five exact group-reader units selected/passed (547 skipped), and seven exact Rust grouped journeys selected/passed in one managed-Postgres invocation (121 skipped), including late-work 43.976 s and owner-grants 20.602 s. No failures or source edits were reported; schema/manifest checks were still running. These results establish the current test bodies. Green stable-input stories do not falsify the selection interleaving or the refused-row counterexample. No synthetic load, provider credentials, broad gate or unrelated test mandate was introduced.

No optional improvements or additional architecture decisions are proposed. **FAIL** pending independent validation of INV-2 and INV-8.
