# TASK-008 independent finding validation

Recommended verdict: **FIX_REQUIRED**.

Immutable subject: `cd0b8e18bd5737267f13c8e72110a06d379d7a0b...798b6b0e83595356c0d2328fa541119267e84fb2`. First review; no prior findings, closure obligations, or acceptance evidence carried forward. HEAD was independently checked at the candidate. This validator is separate from implementation and every discovery role. Production source was unchanged; no Cargo/test/benchmark process was launched by this validator.

## Inputs, authority, coverage and validation method

Read the skill, original task including Obligations, approved packet spec revision 8 and contracts Human approval, scope.md, claim-comparison.md, verification.md, and all seven selected discovery reports. The task's stale revision-5 frontmatter does not override subsequent approved amendments; its explicit maintainer clarification resolves approval-only queue classification under REQ-015. Required architecture amendments are authorized packet output. Correcting a newly introduced spelling inconsistency against the unambiguous approved wire contract does not require changing that contract.

Applied root AGENTS, applicable SQL/Python/TypeScript/docs rules, maintainer style, Rust/error guides and the architecture owners governing approval, execution deadlines, Bifrost custody and normal delegation. CodeGraph exploration preceded code inspection. Independently read actual diff and complete relevant workflow bodies: decision acceptance/replay/wake; ApprovalDecisions read/accept/error projection; RunState wait and control; engine drive/step/members/member and checkpoint/control seam; inspection status/undecided/approvals/retained; queue open/wake; runner decision restoration and deadline construction; Caller attribution and normal token exchange; Opened::start; HTTP problem mapping. Traced affected siblings and existing owners rather than accepting reviewer agreement as evidence.

| Selected role | Applicability independently validated | Coverage result |
|---|---|---|
| behavior-rev | Required first complete task acceptance audit | Complete obligation matrix, public adapters and test assertions; two proposals validated below |
| invariant-rev | Concurrent independent actions, parked/restored occurrence state and execution deadlines are materially complex | Complete assigned producer/consumer and restoration coverage |
| repo-rev | Contract, engine, server, SQL, three SDKs, CLI/MCP, generated artifacts and docs span independently governed tiers | Authority coverage includes every changed surface, actual TenantConn signatures, generated/native declaration comparison, test helpers and mandatory rustdoc |
| maintainer-rev | New approval custody owner and changed engine lifecycle need existing-owner comparison | Complete scope; justified reuse of common Scribe fence/ACK transport with distinct domain row/read semantics; no material maintainer finding |
| system-rev | Decision ACK, lost wake, park/reclaim and dependency outages change failure/recovery behavior | Complete deployed-path analysis and new/moved wait inventory; no process-fatal failure substitution |
| domain-rev-security | New approve permission, subject authority, tenant identity and authenticated decision attribution | Complete supported credential/authorization boundary, including delegation |
| domain-rev-data | First-wins custody, canonical visibility, queue coordination and deadline/concurrency boundary | Complete coherent persistent-data/concurrency scope |

No additional role is missing. A separate concurrency role would duplicate the selected data review's coherent decision-fence/queue domain and invariant review's engine lifecycle scope. No UI, provider protocol, or new deployment domain is materially introduced. No prior coverage is assumed merely from unchanged files.

## Proposal disposition

| Proposal IDs | Disposition | Final finding / reason |
|---|---|---|
| BEH-008-1, INVREV-01, SYS-001 | **REVISED** | FIND-008-1; source confirms invisible reached occurrence. Narrow correction/proof to timely accepted identity and decision while independent work continues, followed by consumption through the sole owner/same-execution continuation. Do not independently require approval-dependent work to run before the independent peer finishes: that timing promise is not explicit and could force unnecessary live-update machinery. |
| BEH-008-2 | **CONFIRMED** | FIND-008-2; the explicit stale/non-pending refusal fails even though discarded output remains fenced from authorizing work. |
| INVREV-02, SYS-002, DATA-001 | **CONFIRMED** | FIND-008-3; same effective deadline cause, one correction. |
| SEC-008-1 | **CONFIRMED** | FIND-008-4; verified actor differs from effective subject under ordinary supported delegation. |
| REPO-001 | **CONFIRMED** | FIND-008-5; explicit changed-item rustdoc gate, batched with material corrections. |
| REPO-002 | **CONFIRMED** | FIND-008-6; inaccurate new owner amendment, no new wire decision needed. |
| REPO-003 | **CONFIRMED** | FIND-008-7; raw diagnostics reach the public problem body without redaction. |

Rejected proposals: none. No new blocker was discovered solely by this validator. No followup-rev was necessary: the stale-output versus stale-receipt distinction resolves the only apparent difference in reviewer emphasis from the existing authority and source. Approval ACK overlapping a concurrently passing deadline/supersession is not promoted into a new cross-store atomicity requirement.

## Final deduplicated ledger

### FIND-008-1 — Reached approvals are absent while independent work runs

Status **REVISED**; classification **MISSING**; sources BEH-008-1, INVREV-01, SYS-001.

Obligation: TASK-008 Input → pending requires the sole owner to create/record pending identity when reached and put identity/message in live state while independent branches continue. REQ-010/016 and contracts require inspection of that pending occurrence from accepted state.

Location/evidence: engine `blocks/engine.rs:1543–1558` returns an ApprovalOccurrence; `step` returns Done::Awaiting, and `drive:1094–1098` retains it only in a local map. Its only exit carrying approvals is guarded by `tasks.is_empty()` at `1069–1082`. Runner `verification/runner.rs:579–581` then calls RunState::wait, whose `verification/state.rs:814–843` is the sole registration/ID producer. Inspection reads accepted state; no sibling registers a reached occurrence sooner. With an ordinary root approval beside a long Agent/code action, the occurrence cannot be inspected or decided until that peer completes; a peer using the remaining execution period can prevent any decision opportunity. This is an explicit parallel journey failure, not ordinary asynchronous persistence delay: the transition is never staged during the interval.

Correction: record the reached occurrence through the existing engine checkpoint/live RunState owner at the pending transition, preserving deterministic execution/step/item/iteration identity and its background persistence. Inspection/decide must continue to use accepted state. Retain the accepted decision for the sole execution owner to consume through the existing same-execution continuation, without replaying or interrupting the independent action. Keep all-blocked compute release and park custody as a separate later transition. No new wait service, per-approval SQL, engine ACK wait, alternate state writer or scheduler is warranted. The run-start decision snapshot must not cause an accepted early decision to be lost; consuming it after existing live work reaches the existing continuation boundary is sufficient. Immediate dependent progress before a peer settles is not an additional acceptance criterion.

Burden: a bounded extension of the existing state/checkpoint seam plus affected engine/runner consumers. Deleting approval, exposing process-local awaiting state, or waiting until all-blocked cannot deliver authoritative timely identity. An extra notification framework is unnecessary to prove this correction.

Focused proof: hold a normal independent action active, reach approval, inspect its accepted ID/message and obtain a durable decision before releasing the peer. Release it and observe the same execution consume the accepted receipt without replaying settled work or replenishing deadline/spend. Existing all-blocked resource-release and stable occurrence tests remain intact. The current engine assertion examines Waiting only after the peer finished, so its passing result misses this case.

### FIND-008-2 — Discarded speculative history is treated as a current pending occurrence

Status **CONFIRMED**; classification **INCORRECT**; source BEH-008-2.

Obligation: REQ-010 and TASK-008 Decision acceptance require stale/non-pending occurrences to refuse; contracts specify 409. No claim is made that discarded work escapes the existing speculation fence.

Location/evidence: `service/approvals.rs:75–87` checks historical membership, then `124–133` checks only execution-level open status. `RunState::wait` appends and retains history; inspection `undecided`/`approvals` at `service/inspection.rs:279–321` never consult accepted disposition. `Step::prerequisites` excludes speculative guard references (`validate.rs:189–211`); `input_slots` does not transitively inherit the guard producer's slot. Thus an approval speculating on a guard step blocked by deferred input can reach pending and park beside another deferred branch. Supply only the guard input resolving false. Engine `step` records/restores Discarded, or `drive:1014–1052` removes awaiting work and records ControlRecord::Disposition; the other deferred branch keeps the execution open. Accepted state now has both old approval history and authoritative discarded disposition. Inspection still calls the old ID pending; a first approve/reject passes membership/open and returns a durable success for an occurrence the engine will never use. This violates the explicit stale-receipt boundary and misleads an ordinary approver; it is material independently of work-release safety.

Correction: reuse the accepted lifecycle/disposition authority (`ExecutionState::speculation`, existing status mechanisms) to determine current undecided occurrence eligibility for both inspection/waiting projection and fresh decision acceptance. Preserve history and already accepted receipt replay. An occurrence with accepted Discarded/Blocked disposition must cease being offered as pending and fresh decisions must receive existing APPROVAL_NOT_PENDING. Genuine pending occurrences remain unchanged. These projections/control guards belong at their boundary because they enforce accepted lifecycle state; do not reevaluate authored guards, duplicate speculation, or create a new approval lifecycle store.

Burden: bounded accepted-state eligibility correction in existing retained inspection/decision owners. Deleting history loses required inspection; execution-only terminal checks cannot distinguish a discarded occurrence in an open execution. Existing accepted disposition is sufficient, so no new persistence, public status, dependency or timer is needed.

Focused proof: park a speculative approval whose deferred guard is unresolved, resolve its guard false while another branch stays waiting, inspect history and submit the old ID. It must no longer appear pending and a first decision must return typed 409, preserving unaffected branch progress and occurrence identity.

### FIND-008-3 — Decision admission omits the shorter authored execution timeout

Status **CONFIRMED**; classification **INCORRECT**; sources INVREV-02, SYS-002, DATA-001.

Obligation: REQ-010/011, contracts and TASK-008 require refusing fresh decisions after the one original execution expiration. Owning architecture `wyrd-design.md:2126–2131` states both operator ceiling and Verifier limits count from admission.

Location/evidence: `service/approvals.rs:124–129` passes only runtime execution_timeout to `open_for_decision`; SQL `verifier_executions/approvals.rs:20–24` derives admission + that duration on statement_timestamp(). Runner `verification/runner.rs:1449–1460` correctly additionally caps by exact target.spec.limits.timeout_seconds minus admission age. Under a five-minute operator ceiling and short authored limit, park and stop the runtime using the existing supported test controls. After authored expiry but before five minutes, a fresh decision request deterministically passes the queue check and is durably accepted; the restarted runner then times out. The failure does not depend on a nanosecond race or an extra atomicity promise.

Correction: decision admission must use the same minimum effective admission-relative execution duration as the queued runner, resolving the accepted exact Verifier through the existing target/cache owner and passing that duration to the existing PostgreSQL deadline predicate. Do not use a newer Verifier version, reset admission age, store another deadline, or add an approval timer. Preserve already-accepted identical receipt replay, actor/permission checks, ACK semantics and queue wake recovery. A later engine timeout cannot undo an invalid positive receipt.

Burden: bounded deadline-source correction; the SQL already takes the needed duration and owns the clock. No schema or lifecycle redesign is needed.

Focused proof: short authored timeout beneath a longer operator ceiling; park, stop worker, decide after authored expiration and assert 409 with no new accepted decision; restart and observe timeout. Keep before-expiration success and ordinary runtime-ceiling behavior. Existing expiry test waits for terminal status under a short operator ceiling, so it misses the mismatch.

### FIND-008-4 — Receipt and replay confuse delegated subject with deciding actor

Status **CONFIRMED**; classification **INCORRECT**; source SEC-008-1.

Obligation: same actor/decision/reason replay and server-derived actor in REQ-010/contracts; normal supported credential flow and existing subject/current-actor attribution authority.

Location/evidence: `service/approvals.rs:138` stores caller.principal.id and `:201` compares it on replay, including the raced-acceptance replay caller at `:162`. Caller documents that principal as effective subject; `Caller::attribution:100–104` resolves the verified outermost actor via delegated_attribution. Normal token exchange (`components/auth/routes.rs::delegate`) accepts subject/actor tokens and existing security authority explicitly supports it. For actors A and B separately authorized to act for subject S with intersected read/approve/Card scope, A's receipt records S, while managed Scribe attribution records A. B, or direct S, can replay the same decision as if it were A. This is incorrect durable provenance and violates the explicit actor-conflict contract; it is not characterized as permission escalation.

Correction: reuse Caller::attribution().act_principal_id for receipt actor and the shared replay comparison. Leave authorization on effective principal and preserve managed subject/actor attribution, tenant isolation, first-wins fence and continuation. Both ordinary and raced replay must use the same actor comparison. No new credential restrictions, token profile, column, login flow or attribution helper is justified.

Burden: two local identity uses and focused delegated proof. Altering delegation or prohibiting otherwise authorized Service credentials is larger and unauthorized; an inspection-only change cannot fix stored receipts or replay.

Focused proof: ordinary real token exchange for A acting for S; receipt/inspection actor is A; identical A retry replays exactly; a separately authorized B with S and direct S conflict on replay. Preserve direct-principal behavior and scope refusal.

### FIND-008-5 — New MCP lifecycle helper lacks mandatory Panics rustdoc

Status **CONFIRMED**; classification **VIOLATION**; source REPO-001.

Obligation: root AGENTS and agent_docs/rust.md explicitly require changed-item rustdoc including test helpers and Panics for possible panics; missing docs block merge.

Location/evidence: new `Opened::start` at `wyrd-mcp/tests/bifrost/mcp/github.rs:386–455` documents Errors but has no Panics section despite assert_eq at `419` and `439`. Both approval and deferred-evidence MCP journeys use it. This is a mandatory gate, not an optional wording preference.

Correction/burden: document the actual connect/delivery assertion failures in this helper's Panics contract. Keep assertions/behavior intact; no new test or fixture redesign. Batch this bounded correction with the material remediation. Focused proof: inspect complete helper rustdoc against its assertions and format the affected file.

### FIND-008-6 — New architecture amendment states unaccepted wire decision values

Status **CONFIRMED**; classification **INCORRECT**; source REPO-002.

Obligation: task requires an accurate Verifier authority amendment consistent with approved Human approval wire contract.

Location/evidence: new `architecture/wyrd-design.md:1647` says decision `approve | reject`; approved contracts.md:299–300 and serde ApprovalDecision at `wyrd-spec/src/verification/execution.rs:229–240` accept only `approved | rejected`. The mounted typed route decodes that enum. A literal caller following the new owner sentence receives a 400 instead of deciding an occurrence.

Correction/burden: correct the new owner sentence to the approved `approved | rejected` values. No compatibility variants, aliases or API/spec change. This two-word owner correction preserves the selected public contract and needs no new test harness. Focused proof: compare owner sentence to approved contract and closed schema/serde values; existing route/receipt checks remain unchanged.

### FIND-008-7 — Approval unavailability exposes raw SQL diagnostics publicly

Status **CONFIRMED**; classification **VIOLATION**; source REPO-003.

Obligation: agent_docs/errors.md forbids raw database/transport diagnostics in public payloads and requires structured internal tracing with a safe typed error.

Location/evidence: new `verification/approvals.rs::unavailable:226–230` interpolates Display(error) into public ServiceUnavailable.message. `ApprovalDecisions::decided` maps tenant_conn/committed_batches failures through it at `88/91`; retained inspection at `service/inspection.rs:520–522` propagates it, as does public decide. The HTTP mapper `http/error.rs:145–167` calls as_problem_json without sanitizing the message; MCP likewise projects the canonical error. Supported database outages therefore expose driver/database diagnostics. Runner's sibling consumer logs and defers, which does not redact inspection/decision responses. The pre-existing evidence sibling's similar code is not a waiver for this new boundary and is outside this bounded finding.

Correction/burden: keep typed ServiceUnavailable, safe table detail, retryability and unavailable-versus-absence behavior; record the source diagnostic with structured tracing at approval custody and return a stable safe public message. Reuse existing redaction conventions exemplified by `http/error.rs::internal_failure`, without changing this error to an unrelated status. One local correction suffices; no broad error migration, retry service or dependency.

Focused proof: feed an internal diagnostic marker through this approval error projection and assert the public problem contains none of it while retaining stable code/status/table detail. Existing unavailable decision/retry proof stays intact.

## Reconciled acceptance and remaining limits

| Requirement / constraint | Validated evidence and reconciliation | Result |
|---|---|---|
| REQ-010a closed approval/message, no per-approval expiry, registration queue_only | Spec enum/validation, existing queue derivation, declaration assertions; spec/runtime unit suite observed passing | PASS |
| REQ-010b stable logical occurrence IDs, fanout/repeat separation, no pending SQL | Pure ID producer keys execution/step/item/iteration; observed stable-ID and engine tests; no SQL added at reach | PASS for identity/no-SQL; FAIL for timely reached state, FIND-008-1 |
| Independent work and existing all-blocked capacity release | Existing drive/park owner retained; peer settles once in engine test | PASS for continued work/park; no immediate-dependent-progress requirement added |
| REQ-010c exact tenant/Verifier/subject permission and audit | Shared authorization and tenant_conn remain authoritative; direct negative journeys source-checked and five pg tests observed passing | PASS for permission/tenancy; FAIL for actor replay, FIND-008-4 |
| Current/stale/expired decision acceptance | Historical membership and runtime-only limit are insufficient | FAIL, FIND-008-2/3 |
| Durable ACK/first-wins/conflict/unavailability | Full custody/fence/ACK read path and replay traced; pg held-ACK/retry observed passing | PASS for custody; FAIL safe public error boundary, FIND-008-7 |
| REQ-011 same ID, restored predecessors, budgets/authority, rejection data, lost wake | Existing checkpoint owner and queue recheck reused; no new reset/writer; observed engine/pg assertions support ordinary paths | PASS within traced paths; preserve during correction |
| REQ-016 SDK/CLI/MCP typed projections, errors, docs/architecture | Thin shared-client calls, existing public journeys source-checked, native/generated parity reviewed by repo specialist | PASS for ordinary projection; FAIL owner typo, FIND-008-6 |
| Mandatory Rust documentation | Changed helper's missing Panics section explicitly violates gate | FAIL, FIND-008-5 |
| Non-goals/INV-001–007 | No new UI/login/proof/token restrictions, SQL payload store, service or generic workflow owner; accepted execution authority and supersession release fences preserved | PASS for inspected exclusions and ordinary regression paths |

The implementation's existing `assert status == approved` requirement mechanism satisfies the standard require-approved outcome without another evaluator/helper abstraction. No DRIFT is retained merely because approvals have a distinct domain row owner: complete workflow comparison shows shared transport/fence/ACK extended, while DeferredEvidence's subscriber/source matching remains separate. No cross-store atomicity, per-request grant refresh or new credential profile is demanded. Existing normal token-snapshot and old-frozen-revision authority remains governing.

System review's wait inventory is source-backed: known approval fence reads, conditional Bifrost decision query, request open SQL/commit, explicit bounded decision ACK, best-effort wake SQL/commit and the existing background park ACK. No pending-step PostgreSQL query, direct-verify SQL/Scribe wait, Vault wait or new direct object-storage wait was found. Shared VerificationEngine initialization/drive is nevertheless modified for all verification; the recorded maintainer hot-path preference therefore prevents a measured PASS claim without a clean full L1 verification record and same-host baseline comparison. None is supplied here. Preserve that as remaining qualification, not as an invented functional finding or broad remediation suite. This source-backed FIX_REQUIRED does not claim performance acceptance.

Verification.md accurately separates executed checks from source-inspected proof: five approval pg tests, focused engine/ID tests, 855 spec/runtime tests, fmt/dependency/tenant and diff checks passed. Full language journey lanes, Clippy/types/codegen/docs build and full benchmark were not rerun; runtime restart test is not a whole server/Scribe process restart. Do not turn these limits into new product requirements or new fault hooks. Focused remediation proof must address the retained gaps at existing owners, with broad integrated qualification left to change review.

All retained corrections fit already approved behavior and existing resource/persistence ownership. No new public, security, compatibility, concurrency, architecture or persistent-data decision is required by this ledger. There is no prior-finding closure or two-round convergence assessment on this first review.
