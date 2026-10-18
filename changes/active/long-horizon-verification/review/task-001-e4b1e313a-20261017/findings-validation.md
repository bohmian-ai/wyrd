# Independent findings validation — TASK-001

Result: **FIX_REQUIRED**. Eight bounded findings remain after source validation and deduplication. No material product, authority, concurrency-semantics, resource-ownership or retained-data migration decision is needed for these corrections. None of the recommendations authorizes TASK-002, TASK-003 or REQ-015 work.

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`; candidate: `e4b1e313aacab50d7eb0f0a2060b8547bf19a632`.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-001-grouped-verification.md`.
- Specification: `changes/active/long-horizon-verification/spec.md`, **user-approved revision 13 requirements**. At both commits the file header says draft revision 14; its history records revision 13 approval and describes revision 14 as retaining the requirements/acceptance criteria while adding a schema-reset exception. The user's explicit authority selects revision 13 and excludes that draft exception. This is a recorded header/history limitation, not approval of revision 14 and not evidence for inventing retained-state upgrade scope. No retained-upgrade finding is added here.
- Reviewed all eight supplied discovery/follow-up reports: behavior, invariants, standards, maintainer, system, security/tenancy, data/concurrency and focused follow-up. Proposals were compared by invariant, not reviewer count. No prior TASK-001 validation ledger was supplied; IDs below are the first deduplicated IDs for this review.
- Followed CodeGraph first, then inspected the cumulative base-to-candidate diff, relevant current owner bodies, callers, sibling writers/consumers, generated declarations, test bodies, manifests/lockfile and mise tasks. Authorities include AGENTS, agent rules, design/doctrine, Bifrost design, security posture, reference router, spec-driven development, maintainer style and applicable evaluation/reliability guidance. The task-review skill and Ponytail ladder govern this validation. HEAD remained the candidate; the only changes were reports. I wrote only this report and ran no tests or Cargo commands.

## Source and correction map

The following is the traced shared graph, not a list of isolated line matches.

| Authority / producer | Callers and consumers inspected | Correction boundary / preserved siblings |
|---|---|---|
| `Observe` / shared `Target`; Python Observe and Node NativeRun wrappers | Ordinary verify/submit, HTTP/MCP request projection, staged submit metadata | Already share transport and typed `GroupId`; no client implementation or producer lookup is needed. |
| `VerificationControl::{enqueue,stage_direct}`, `ResultPayloadBuilder::{inputs,summary}`, queued runner result projection | Direct input/result staging; submitted input → submission writer → queue; runner → result outbox | Activity times precede asynchronous Bifrost visibility. All group source forms consume these same rows. Preserve one ScribeOutbox and accepted crash-loss boundary. |
| `EffectiveSpecs::{validate_binding,validate_sources,reaches}`, `source_binding`, `GroupSite::ambiguous`; `BindingProjector::group_sources` | All owner/component registration forms and pinned exact source tuples | Existing resolution/graph owner already supports the behavior. Proof gaps do not justify a second resolver or universal source binding provenance. |
| `VerificationScheduler::{tick,poll_groups}`, `GroupPoller::poll`, all `BindingPoll` methods, `Sources::{new,rows}`, `Ended::{judge,executions}`, `Payloads::value` | Discovery → identities → refused removal → timing → queue coverage → payloads → snapshot → enqueue | Full bodies read. This is the common source of discovery/bounds/timing findings; no per-SDK defect or new lifecycle owner. |
| `VerifierExecutionQueue::{claim_due_group,group_states,selected_results,refused_submissions,enqueue_group,complete}`, their SQL; `GroupState::covers` | Scheduler claim/cadence, pinned inputs, outstanding fence, authoritative queued result, lease settlement, previous verdict and dispatch | Full relevant correction/coverage bodies read. Preserve TenantConn/RLS, caller transaction, SKIP LOCKED, partial unique outstanding index, lease fence, immutable retries and transactional reaction creation. |
| `BackgroundAuthority::resolve` → `TenantTokenIssuer::recover` → `ExecutionAuthority` | Group poll and scheduled work accept current owner grants with SYSTEM executor | Owner runtime activity is distinct from source-table read permission. Recover does not restore a revoked role. |
| `SystemReadAuthority::{resolve,table_uid,serving,context,into_context}` | Exactly two production resolve callers: groups and Drift `Reader::authority` | Full bodies read. `serving` changes attribution, explicitly not grants. Remove only grouped use; keep Drift's approved observation-table exception. |
| `Caller::accepted`, `ScheduledQueryCaller::{new,authenticated,run_with,dispatch,record_object_denial,consume_to_terminal}`, query authority/context | Ordinary result inspection and trace-input reader use authenticated caller; grouped path currently uses manufactured SYSTEM context; hosted tools additionally check tool permission | Reuse authenticated query owner and non-blocking audit; preserve stream settlement, cancellation, per-query admission, Oracle resolved-table checks and separate tool permission. |
| `InputSlot`, `SourceText`, source-owned schemas; `gen_schemas::contracts` → `scripts/gen_ts_types.py` → public `card-types.ts` | SDK typed Card authoring → existing path registration decoder; separately served OpenAPI | Runtime source form is a string. Fix projection at source, regenerate; no TS-only validator or new accepted object form. |
| `TriggerDuration::{parse,fmt,deserialize}`, `span`, group timing owner | Trigger decode → pinned settings → candidates / Ended; scheduler runtime supervisor | Full bodies read. Locked chrono is **0.4.45**. Its DateTime Add/Sub use checked operations followed by `expect`; accepted nine-digit days exceed DateTime range. Use installed checked/elapsed facilities, not a new cap or timer. |
| Existing Rust/Python/TS grouped story owners, Rust deployment restart, MCP story, served contract test | Real SDK/server proof; shared local provider and server fault seams | Reuse these owners. Rust media test really recovers the one running item; Python/TS normal flows do not prove every required edge. Proof closure stays focused. |

The new group reader owns meaningful state and a missing analytical selection capability; the existing Drift window reader is not an equivalent group-source workflow. Reusing its narrow SYSTEM authority, however, is invalid for these protected payload reads. Existing queue/query/evaluator/Operator owners should be extended, not replaced.

## Every proposed finding: disposition

| Source ID | Disposition | Source-validated resolution |
|---|---|---|
| BEH-001 | CONFIRMED | `groups.rs:222–229` compares source activity to review completion and filters before coverage. Consolidated in FIND-TASK-001-1. |
| BEH-002 | REVISED | Unsupported hour expiry confirmed at `:53–57,218–221`; classification is incorrect required discovery, with the deletion of that extra policy in FIND-TASK-001-1. Not a new retention or upgrade requirement. |
| BEH-003 | CONFIRMED | Raw-row skip precedes dedup/finality/refused filtering and actual ceiling at `:159–165,263–273,293–308`. FIND-TASK-001-2. |
| BEH-004 | REVISED | Panic path confirmed; locked chrono is 0.4.45, not 0.4.44. Fault is shared scheduling capability, not an assertion that the entire server exits. FIND-TASK-001-3. |
| BEH-005 | CONFIRMED | Source is `String`; exported TS additionally advertises `{type:"source"}`. FIND-TASK-001-4. |
| BEH-006 | REVISED | Explicit omitted acceptance cases retained in FIND-TASK-001-8; discovery/absent-result/overlap proofs belong to findings 1–2. Reject blanket suite expansion, a new harness, and treating missing Python/TS restart controls alone as a new product defect. Static no-IO tracing is credible; do not invent producer instrumentation. |
| INV-REV-001 | CONFIRMED | Same pre-coverage completion-time filter as BEH-001; FIND-TASK-001-1. |
| INV-REV-002 | CONFIRMED | Same unsupported discovery expiry as BEH-002; FIND-TASK-001-1. |
| INV-REV-003 | CONFIRMED | Logical membership is evaluated only after raw cap; FIND-TASK-001-2. |
| INV-REV-004 | REVISED | Same timing fault, with precise shared-capability scope and locked version; FIND-TASK-001-3. |
| SR-001 | CONFIRMED | Specific missing Errors/field/module docs exist on new items. Mandatory rustdoc exception makes this blocking; FIND-TASK-001-6. |
| SR-002 | REVISED | Active protocol/evaluation text contradicts an already approved tool exception. Synchronization is bounded documentation correction, not an unresolved new security decision; FIND-TASK-001-7. |
| MNT-001 | CONFIRMED | Phantom generated source object is reachable typed authoring; FIND-TASK-001-4. |
| MNT-002 | CONFIRMED | Same two fallible-method rustdoc omissions within SR-001; FIND-TASK-001-6. |
| SYS-001 | CONFIRMED | Slow outstanding review/late publication reaches same completion-time suppression; FIND-TASK-001-1. |
| SYS-002 | CONFIRMED | Recovery advances cadence without a historical source cursor and cannot rediscover expired retained rows; FIND-TASK-001-1. |
| SYS-003 | REVISED | Raw cutoff confirmed in FIND-TASK-001-2; its first-page starvation consequence belongs with discovery scan progress in FIND-TASK-001-1, not another independent worker defect. |
| SEC-001 | REVISED | Security claim confirmed, requirement label corrected to **REQ-011** (`spec.md:524–528`), not REQ-010. FIND-TASK-001-5. |
| DC-001 | REVISED | Completion-time suppression, hour expiry and repeating first-page starvation share discovery eligibility/progress owner; all retained as FIND-TASK-001-1. TASK-003 tenant pressure/progress remains excluded. |
| DC-002 | REVISED | Mixed-tuple overread and raw-row skip confirmed; payload-prefix loss requires changed visibility, not the stable-row-set case. Consolidated source-selection finding 2. |
| FUP-001 | REVISED | Small mixed-tuple overread is deterministic. Fresh-cut prefix loss is a permitted interleaving, not an executed reproduction or inevitable scan order. Reject claiming terminal validation itself is broken. FIND-TASK-001-2. |
| FUP-002 | CONFIRMED | Full authority chain proves owner grants are discarded and SYSTEM grants manufactured before protected reads. FIND-TASK-001-5. |

No whole proposal is rejected as fictitious. The rejected portions above are excluded from the ledger and remediation; they are not optional advice.

## Final deduplicated ledger

### FIND-TASK-001-1 — REVISED / INCORRECT — discovery can permanently hide retained uncovered work

**Discovery IDs:** BEH-001/002, INV-REV-001/002, SYS-001/002, discovery-progress portion of SYS-003, DC-001.

**Obligation:** REQ-003–005, AC-002/003/006 and TASK-001 scenarios 2/5: retained eligible activity, including arrivals during a review and late publication, must reach a subsequent serial whole-group snapshot; failed discovery stays retryable with bounded work.

**Exact locations:** `crates/wyrd/wyrd-server/src/verification/groups.rs:53–57,158–178,217–240`; existing coverage authority `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:985–994,1620–1686`.

**Evidence / consequence:** Source inputs/results are stamped before outbox publication. Snapshot A can finish at J after an uncovered source row's activity time T; the result can publish before that source row. `HAVING MAX(s.at) > j.judged` rejects T ≤ J before `GroupState::covers` can detect increased membership. Rows are retained, so accepted outbox loss does not excuse this. Independently, `clock-idle-1h` excludes unjudged retained rows forever after an outage/arming delay. Finally the earliest-64 page restarts unchanged: 64 groups with post-finality activity newer than their last group judgment qualify in discovery but are skipped by eligible coverage; a later due group never enters the page. The same issue appears for persistent failed/oversized candidates. The shortcut claim that they drain is false. This is bounded *within-binding discovery* correctness, not TASK-003's tenant-resource fairness.

**Selected minimal correction:** Keep discovery on GroupPoller and the existing binding scheduler/cursor owner. Delete review completion-time and arbitrary one-hour age as coverage/eligibility authority. Use the existing queue-backed source coverage/outstanding state for no-op/serialization. Select bounded, continuing discovery over retained source groups, ensuring covered/post-finality/unprocessable first-page groups do not reset progress ahead of later work. Conventional bounded pagination/continuation belongs in this existing binding discovery owner; it is scan progress only, not per-group membership, readiness, revision, retention, or a new lifecycle/durability store. Preserve `next_run_at` as fixed cadence and queue fences/immutable snapshots. Do not increase the magic hour, replace coverage with judgment time, scan unboundedly in API memory, or add producer IO. Private query shape, helper/control flow and continuation representation remain implementation-owned.

**Focused closure proof:** Extend existing recovery/coding/refund owners only for (1) an outstanding review plus older-timed uncovered activity and a null→actual late result, proving one next full snapshot and unchanged old snapshot; (2) retained due history beyond idle+1h, judged once after recovery; (3) a page of covered/post-finality groups ahead of another eligible group, which eventually progresses under bounded reads. Use fixture evidence times and cursor controls, not long sleeps or load. A failed query may defer work; the next tick must still find it. No broad journey aggregate is required for this correction.

### FIND-TASK-001-2 — REVISED / INCORRECT — bounds apply to physical rows and post-read filtering rather than selected source executions

**Discovery IDs:** BEH-003, INV-REV-003, raw-bound portion of SYS-003, DC-002, FUP-001.

**Obligation:** REQ-002/004–006/009, AC-004/006, TASK-001 scenario 4: count distinct eligible execution IDs per reference, detect overflow before its payloads are fetched, produce completed inconclusive/source_execution_limit_exceeded, and never pin a partial query prefix as complete evidence.

**Exact locations:** `crates/wyrd/wyrd-server/src/verification/groups.rs:59–61,159–168,263–273,293–308,376–438`; existing result authority `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:250–265,1699–1745`; existing outcome `verification/runner.rs:420–429`.

**Evidence / consequence:** More than 100000 physical input/result identities causes an unconditional skip before logical dedup, refused removal, finality and source ceiling. For example 50001 ordinary completed direct calls generate 100002 identities; the required ceiling result is absent rather than inconclusive. Overlap/duplicates can cause the same absence without logical overflow. For a small mixed group at ceiling 20, A=21 executions and B=1, `readable` excludes A but B triggers payload collection. Both SQL statements still select all source tuples; Arrow batches already contain A's payloads before Rust `readable.contains` filters. The later runner short circuit cannot undo these reads.

The payload LIMIT is not independently shown to truncate an unchanged accepted identity set. With different pinned cuts per statement, additional qualifying rows can become visible after identity selection, fill the broad unordered SQL LIMIT, and omit an earlier selected payload. The stream can correctly terminate for that SQL prefix; `Payloads::value` then turns omission into null. This is a permitted source-level interleaving, not a measured reproduction or a claim that a particular scan order always occurs.

**Selected minimal correction:** Use the existing GroupPoller source-selection owner to bound and deduplicate eligible logical identity selection per tuple before payload collection, excluding finality/refused rows without their consuming source capacity. Reuse the existing overflow marker/runner outcome; remove the silent permanent raw-row skip. Carry the existing pinned readable membership into **both** payload SQL selections, preserving tenant/owner/exact tuple/group/cutoff and existing `selected_results` authority. Deduplicate/bound selected payload results so a complete read means complete selected coverage; any physical cap that cannot establish that must return a retryable read failure, not an ordinary null or truncated snapshot. Preserve null for genuinely absent result/slot values, direct/queued authority and aligned input-time/ID ordering. This is not a new ceiling, budget, query consumer, snapshot service or evaluator guard.

**Focused closure proof:** At the existing source-query seam prove mixed 21+1 tuples do not fetch over-limit input/outcome payloads and still finish with the explicit ceiling result. Prove physical overlap does not increase logical membership and a source set crossing the old raw boundary receives the required result without collecting its payloads. Prove a successfully terminated but incomplete selected prefix cannot enqueue and can retry; preserve genuine absent-result null and authoritative queued result behavior. Normal-volume fixtures/query inspection suffice; no synthetic load or new harness. Discovery starvation proof is owned by finding 1.

### FIND-TASK-001-3 — REVISED / INCORRECT — accepted Trigger duration can repeatedly panic scheduling

**Discovery IDs:** BEH-004, INV-REV-004.

**Obligation:** REQ-004 and AGENTS input/panic/runtime rules: accepted Trigger timing must be safe and a single binding cannot terminate unrelated scheduling.

**Exact locations:** `crates/wyrd-spec/src/card/trigger.rs:94–115`; consumers `crates/wyrd/wyrd-server/src/verification/groups.rs:218–220,636–644,721–723`.

**Evidence / consequence:** `999999999d` parses successfully to 86399999913600 seconds. That fits TimeDelta but exceeds DateTime range. Discovery subtracts it; Ended adds finality/idle/max_age. Locked chrono 0.4.45 DateTime Add/Sub `expect` their checked operation, so normal clocks plus these legal settings panic the scheduler future. Runtime restarts do not remove the due binding, allowing repetition. Runner/Operator and unrelated server capabilities are separate; the supported consequence is shared scheduling degradation, not whole-process death.

**Selected minimal correction:** Make the existing group timing boundary total for the accepted duration domain using elapsed comparisons and installed chrono checked timestamp operations where SQL/cutoff needs an instant. Preserve mathematically valid long idle/max_age/finality semantics, including bounds outside representable timestamp range. Do not reject previously accepted values under a new arbitrary authored maximum, silently shorten their duration, add another timer/runtime, or crash a shared component to refuse one binding. Removing the unsupported discovery horizon in finding 1 also removes that subtraction, but Ended still needs this independent correction.

**Focused closure proof:** Pure timing tests at normal fixture timestamps cover maximum accepted s/m/h/d durations for all three settings and exact boundary eligibility; a focused registration/scheduler seam proves an extreme binding cannot prevent a neighboring ordinary binding from scheduling. No million-year date fixture or wall-clock delay is necessary.

### FIND-TASK-001-4 — CONFIRMED / INCORRECT — generated public InputSlot advertises an undecodable object

**Discovery IDs:** BEH-005, MNT-001.

**Obligation:** REQ-002/011 and AGENTS shared-wire/generated-artifact rules require declarations and runtime acceptance to agree.

**Exact locations:** `crates/wyrd-spec/src/card/verifier/mod.rs:128–176`; `card/verifier/reference.rs:183–185`; `sdks/wyrd-sdk-ts/wyrd/src/card-types.ts:2364,2429–2431`.

**Evidence / consequence:** The mixed enum's Source branch decodes transparent String, while generated InputSlot includes both string and `InputSlotVariant5 {type:"source"}`. Typed Card authoring accepts that phantom object, then the existing registration decoder refuses it. The schema export uses schemars `gen_schemas::contracts`, lowered by the existing TS emitter; codegen reproducibility is not runtime/schema parity. The served OpenAPI test currently checks grouped field presence, not all InputSlot alternatives.

**Selected minimal correction:** Correct InputSlot's source-owned schema projection to represent exactly four real typed objects plus source-reference string, preserving Serde/runtime source validation. Regenerate through the existing schema/TS pipeline and keep the served schema coherent. Do not hand-edit generated output, special-case the TS client, or add runtime acceptance for `type: source`.

**Focused closure proof:** A focused schema/runtime parity check accepts a valid source string and rejects the phantom object; TS type evidence rejects the object after regeneration. Use codegen:check, owning typecheck and the focused served-contract selector when the served projection changes. No new schema framework or broad suite.

### FIND-TASK-001-5 — REVISED / VIOLATION — group evidence reads widen the accepted owner's permission

**Discovery IDs:** SEC-001, FUP-002.

**Obligation:** **REQ-011** (`spec.md:524–528`), INV-003, design execution identity and security posture (`architecture/wyrd-security-posture.md:110–137,212–219`): SYSTEM adds no authority, and protected recorded inputs/results require the accepted owner's existing permissions. Drift's explicit exception is restricted to its observation table.

**Exact locations:** `crates/wyrd/wyrd-server/src/verification/groups.rs:130–150`; manufactured authority `verification/authority.rs:93–129,178–189`; acceptance producer `verification/scheduler.rs:276–288`.

**Evidence / consequence:** BackgroundAuthority recovers owner grants, but GroupPoller ignores them for every discovery/identity/payload statement. SystemReadAuthority constructs new table-UID grants over execution_inputs/results; `.serving` only changes served subject/history. Its nonempty check tests those manufactured grants. ScheduledQueryCaller::new bypasses authenticated admission and Oracle correctly authorizes the supplied SYSTEM grants. A runtime-active owner without query capability, or scoped away from a source table, still has sensitive values pinned into inputs and delivered to a judge. Later Caller::accepted in the runner cannot undo that read. Exact owner/tuple filters and tenant RLS do not confer sensitive-read permission. This resolves the standards report's permission PASS against the security/follow-up reports: helper reuse/audit-writer shape was correct, selected authority was not.

**Selected minimal correction:** In GroupPoller only, build the accepted owner caller with existing `Caller::accepted` and SYSTEM executor attribution, and reuse `ScheduledQueryCaller::authenticated` for all grouped source statements. Existing public query admission, resolved-table authorization and non-blocking denial audit then enforce the required boundary. Remove this caller's manufactured SYSTEM grant; do not add an extra permission check followed by the same privileged read. Preserve Drift Reader::authority, its table exception, producer no-IO stamping, exact source filters, terminal-safe read settlement and cursor retry. No role restoration, new permission, credential, grant or audit writer is needed.

**Focused closure proof:** Keep owner active with invocation/model permission while independently (a) omitting query capability and (b) granting query scope excluding a verification source table. No protected payload may reach snapshot/judge; ordinary denial audit must be visible. Grant the existing needed table access and prove a later poll succeeds. Exercise both input/outcome references through this same owner and retain existing Drift authority regression proof. Live-tool denial tests exercise a different later consumer.

### FIND-TASK-001-6 — CONFIRMED / VIOLATION — mandatory rustdoc is incomplete

**Discovery IDs:** SR-001, MNT-002.

**Obligation:** AGENTS §16 and agent-rules require substantive rustdoc on new/materially changed Rust items/fields and Errors on fallible methods; task-review explicitly preserves this blocker.

**Exact locations:** `crates/wyrd-spec/src/card/trigger.rs:137–155` (fmt/deserialize lack Errors), `card/verifier/reference.rs:185` (SourceText tuple field), `crates/wyrd/wyrd-server/src/verification/groups.rs:804–805` (new module docs), `sdks/wyrd-sdk-rust/tests/integration/support/mod.rs:499` (Forward sender field).

**Evidence / consequence:** These are added items in the cumulative range and lack the required contracts. fmt can fail writing; deserialize can reject non-string and malformed duration. This is the repository's explicit rustdoc completion gate, not an invented style blocker. Existing Clippy does not check every private field/module contract.

**Selected minimal correction / proof:** Complete these docs in the same remediation; inspect other materially changed items in that remediation write set against the same rule. Explain writer failure, decode/parse refusal and retained field/module role. Do not create a documentation checker, unrelated cleanup or separate style-only round. Source inspection and owning format/lint/doc checks suffice; no new behavior test.

### FIND-TASK-001-7 — REVISED / VIOLATION — active protocol still forbids the approved check-Agent read tool

**Discovery ID:** SR-002.

**Obligation:** Design authority preamble (`architecture/wyrd-design.md:8–17`), REQ-002/AC-003 and TASK-001 approach 5 require the already approved live-read exception to be synchronized with authority.

**Exact locations:** `architecture/wyrd-design.md:1200,1405–1406`, `architecture/references/domain/evaluation.md:55–68`; contrasting implementation `wyrd-spec/src/card/verifier/validate.rs:558–571`, `vala-verify/src/blocks/agent.rs:327–351`, public doc `docs/src/content/docs/understand/how-verification-works.md:102`.

**Evidence / consequence:** Active design says check Agents are tool-less and evaluation says judges cannot invoke tools, while registration/runtime/public guidance now admits explicitly declared bifrost.query. A future conforming implementation/review would reject the approved J-003 form. This is authoritative public-contract contradiction, not cosmetic wording.

**Selected minimal correction / proof:** Update these authoritative descriptions to exactly the approved declared bifrost.query exception, ordinary tool/query/payload authorization, no workspace/mutation tools, and live evidence outside pinned group inputs. Preserve every other tool refusal and accepted authority. Do not remove approved behavior or expand the tool set. Compare prose against validator/hosted execution and existing live/refusal proof; docs:check applies if site docs are touched. Approval already chose the behavior, so SPEC_REVISION_REQUIRED is not warranted.

### FIND-TASK-001-8 — REVISED / MISSING — explicit acceptance variants remain unproved

**Discovery ID:** BEH-006, narrowed; proof implications in other reports.

**Obligation / exact locations:** TASK-001 scenarios 1/3/5 and acceptance; spec AC-001 registration paragraph (`spec.md:735–748`), AC-003 (`:806–851`), AC-004 repeated agent negatives (`:867–923`), AC-006 (`:979–1039`). Existing owners: Rust `verify_in_real_time.rs:1276,1660,1998,2288`; Python `test_verify_in_real_time.py:467–945`; TS `verify-in-real-time.test.ts:525–1085`; MCP `verification.rs:1325–1539`.

**Validated omissions / consequence:** (1) Reserved typed/source `inputs.group_id` refusal and rename success are not driven through the required three SDK registration paths plus server HTTP/MCP enforcement; spec unit validation does not prove persistence/activation refusal. (2) Coding stories always install a trace/live-read check, publish the “late” span before waiting for the snapshot, and revoke tool+query as one role. They do not prove clean non-trace/no-query behavior, span visibility changing *after* pinned inputs, or independent tool/query denials. (3) Registration journeys cover child/unbound/cycle only, not the required same-request/registered external reference success and participating ambiguity/missing declared target refusals. (4) MCP's added story is JSON support only; explicit media failed/zero/ceiling/isolation negatives are required on agent HTTP/MCP as well. (5) No grouped recovery path deliberately forces failed/truncated source discovery/snapshot then proves no partial enqueue and successful retry. Existing terminal-consumer tests plus incidental teardown logs do not prove that integration. These are explicit acceptance omissions, not evidence of an additional runtime defect.

**Selected minimal correction:** Add these cases to the existing grouped story/registration/agent test owners, using the existing real server/local provider/evidence transfer and query fault facilities. Share setup only where it already belongs. Do not create a general harness or duplicate query terminal parsing. Cover SDK-specific registration/authoring where explicitly required, and agent-specific media/scopes through its existing surface. Shared scheduler/worker recovery can reuse the existing Rust process proof; missing Python/TS restart controls alone does not justify building new control APIs. Absence/late-result, outstanding/coalescing, distinct physical overlap and source-denial proof are already required by findings 1/2/5 and must not become duplicate test mandates. Static complete producer tracing already supports no-new-IO; no monitoring subsystem is requested.

**Focused closure proof:** Run only exact modified/added test selectors under repository Postgres/migration setup, with nonzero selected counts and retained outcomes. A bounded failure/retry case can use existing query discovery-outage/EOF-after-batch controls at its real owner; use the same facility rather than a new test transport. The source-selection prefix issue in finding 2 is distinct from transport truncation. Do not prescribe full language/journey suites, a broad aggregate, capacity runs or new restart harnesses to close this finding.

## Ladder and adjacent-behavior reconciliation

| Finding | Delete / reuse / standard / native / installed / minimum decision |
|---|---|
| 1 | Delete false completion-time coverage and unsupported age expiry; reuse queue coverage and binding scheduler; conventional bounded continuing discovery; standard collections/SQL already installed. Minimum is correcting the selection/progress source, not adding per-group state or consumer retries. |
| 2 | Delete silent physical-row terminal policy; reuse logical source ceiling, selected-results authority and terminal-safe query owner; standard SQL selected membership/dedup with complete bounded results. Installed Bifrost/DataFusion supports these reads. No new budget or transport. |
| 3 | Cannot delete required timing. Reuse Ended/group timing; ordinary elapsed/checked arithmetic using installed chrono/std. No dependency, platform mechanism or arbitrary new public maximum. |
| 4 | Delete phantom schema alternative; reuse source-owned schemars/utoipa and existing generator. Standard tagged objects/string union; no second validator or compatibility object. |
| 5 | Delete manufactured SYSTEM grants for this caller; reuse Caller::accepted and authenticated query/audit. Existing protected-read boundary is the right guard owner. Keep Drift's separate exception. |
| 6 | Delete no behavior; add missing rustdoc in place. Existing doc/format/lint process, no new checker/test. |
| 7 | Replace stale prohibitions with approved narrow exception. Reuse current protocol/reference owners, no product redesign. |
| 8 | Delete broad suite/harness mandate; retain exact required missing cases on established story/server/provider/fault seams. Reuse existing shared-runtime recovery evidence where appropriate. |

No correction moves admission, durability or resource ownership into the client or group evaluator. Preserve ordinary Window/Drift, ordinary on_failure, group verdict baseline/change dispatch, exact tenant/owner/subject/Verifier/group isolation, ownerless invocation, selected queued result, finality/non-refusal, outbox loss, fixed cadence, one outstanding execution and immutable retry inputs. No new dependency, route, lifecycle kind, group membership/revision store, source readiness barrier or span snapshot is justified.

## Verification reconciliation and limits

The orchestrator independently reports green: Monty worker build; Rust recovery exact selector (1 selected, 25.029s) plus one union of the other four exact selectors (4 selected: support 9.275s, coding 15.543s, frames 31.348s, refund 51.646s); MCP exact grouped selector (1 selected, 8.564s); served OpenAPI exact selector (1 selected, 0.988s). All used managed Postgres/migrations. Also py:setup PASS plus five exact Python node IDs (5 selected/pass, 112.51s), and ts:build:testing PASS plus five prose-name regex selections (5 pass, 19 skipped, 120.58s). There are **no reported test failures**. I did not rerun these commands.

Those runs validate their actual test bodies. They do not disprove uncovered source interleavings or omitted acceptance variants. Other appended format/lint/codegen/dependency/tenancy/docs claims were not independently rerun by the orchestrator and are not independently certified here. Codegen reproducibility cannot certify schema/runtime equivalence; correct stream termination cannot certify an application's SQL membership selection. The standards permission PASS and invariant schema-parity PASS must be narrowed by findings 4/5, and live-span proof PASS by finding 8's pin-before-flush omission.

All retained findings are bounded corrections under approved behavior. There is no unresolved material decision requiring SPEC_REVISION_REQUIRED in this ledger. Any implementation proposal to change accepted permissions, shorten retention/finality, add producer IO or a per-group lifecycle/persistent inventory instead would exceed this recommendation and must not be silently substituted.
