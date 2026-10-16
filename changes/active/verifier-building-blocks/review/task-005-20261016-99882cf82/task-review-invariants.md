# TASK-005 invariant acceptance audit

Overall result: **FAIL**.

## Immutable subject and review basis

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
- Base exclusive: `f2a726c90edbf8956c7b91c843379175ee20eac0`.
- Candidate: `99882cf824e70a601033aba4cfa5c5b27a8dfc7c`.
- Approved authority: `changes/active/verifier-building-blocks/spec.md`, revision 8; original task `tasks/TASK-005-server-integration.md`.
- Reviewed the complete cumulative file inventory/diff with source tracing concentrated on admission, ACK handoff, queue transactions, accepted authority, engine/resource error propagation, scheduled input selection, result staging, status, dispatch, and their SDK consumers. No peer reports were read. No implementation was changed.
- Applied AGENTS.md, architecture/agent-rules.md, the spec-development and maintainer references, Wyrd design/doctrine and Bifrost ownership, and evaluation/drift references. CodeGraph was used before code navigation.
- Supplied verification: full gate on `03712af5e`, recorded in candidate task evidence; external language tests reported passing. These are supplied results, not independently rerun results. Local external source repositories have clean tracked working trees, but the reported locally built artifacts have no recorded immutable build digest.

## Producer-to-consumer and reuse map

| Authority/state | Producers, transport, consumers | Existing alternative and conclusion |
|---|---|---|
| Manual accepted authority | HTTP/MCP → VerificationControl::enqueue → Submission::new/AcceptedAuthority → ScribeWrite::ExecutionInputs → ScribeSink::write ACK → SubmissionOutbox → VerifierExecutionQueue::accept_submissions → claim → Caller::accepted → HostedBlocks | Existing outbox and leased queue workflows are extended; no alternate queue service. However the old subject-scope decision was deleted instead of moved into asynchronous acceptance (I-01). |
| Stable request identity | RequestKey canonical request hash and tenant/subject/actor → deterministic ExecutionId → transaction advisory lock/key lookup → insert or conflict row | Existing key/hash/transaction authority is reused. Identical retries and conflicting content have different appropriate identities. No finding from duplicate batches. |
| Direct execution | execute authorization → read-only target resolution → HostedBlocks shared engine → stage_direct → process ScribeOutbox | Direct and queued engines share the new HostedBlocks composition, with caller-bound registry/evidence/tools/gateway, rather than duplicate orchestration. Direct inputs have queue_execution=false. |
| Result and reactions | engine outcome → ResultPayloadBuilder → lease-fenced completion and Scribe staging → canonical results and detail tables; failed scheduled completion → existing dispatch queue → existing worker/frozen template context | Existing result/outbox and Operator workflow is retained. Delivery failure is independent of execution verdict; result visibility can lag completion as approved. |
| Runtime capacity | startup config → HostedLimits → one Sandbox/Workspaces owner → engine calls → runner transition | Owners are reused, but capacity refusals disappear into ordinary inconclusive outcomes and scratch admission has an unbounded waiter list (I-02). Existing queue deferral/release is the correction owner, not another scheduler or process-wide worker reservation for model calls. |
| Scheduled source declaration | binding inputs persisted → CLAIM_SQL schedule_inputs → ClaimedRow::into_claimed slots → runner's Scheduled arm | Selector map reaches the runner but is ignored there; DriftEngine scores each check from its spec regardless of whether the check's optional slot was mapped (I-03). Existing DriftChecks missing-evidence behavior should be reused. |
| File evidence | shared client RunEvidence::pin → evidence plan → existing UploadPlan/storage client → TenantEvidence authorized tenant read and digest/format validation | Existing upload vocabulary is reused. A newly exposed local HTTP upload route is nevertheless a public surface requiring authority (I-04); the evidence authorization also excludes exact Verifier grants (I-05). |

## Acceptance matrix

PASS here means the inspected implementation and supplied verification support that obligation; failures are narrowly identified below.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Direct exact execution through the shared engine; no queue activation or Operator | service.rs execute/stage_direct; hosted/blocks.rs; inputs queue_execution=false | pg_verification_routes direct tests; real SDK verification journeys recorded in gate | PASS |
| Exact invocation, accepted authority, tenant isolation, signed subject scope on queued work (INV-003) | authorize_run, AcceptedAuthority and tenant SQL retained; prior subject_in_scope decision removed | Direct exact-Verifier tests exist, but no replacement queued signed-scope refusal proof | FAIL I-01 |
| Asynchronous 202, no request Postgres writes; ACK before batched acceptance | enqueue is synchronous staging; scribe_outbox.rs write ACK → submissions.rs; accept_submissions tenant transaction | Replay/conflict/read-back and loss runtime tests recorded | PASS |
| Replay/conflict identity and no duplicate logical dispatch | RequestKey; accept transaction/key lock; dispatch unique fence and complete | pg_verifier_executions, pg_verification_routes, pg_operator_delivery reported passing | PASS |
| Status/result distinction; status may precede analytical result; bounded reconnect wait | get_execution/read_result; Execution polling; result staging independent of ACK | Rust/Python/TS resumed execution tests and runtime publication tests recorded | PASS |
| Dispatch status survives delivery failure; binding latest uses admitted identity | execution_status DISPATCHES_SQL; binding status; existing complete/dispatch worker | pg_operator_delivery and three-language drift journeys recorded | PASS |
| Correct required/optional scheduled slot selection and honest missing evidence | selectors arrive on ClaimedExecution but runner discards slots; checks pre-score all drift requirements | Supplied journeys cover mapped happy paths; no omitted optional-slot case | FAIL I-03 |
| Existing PSI/SPC/Custom scoring, readiness, fixed windows and failed-only reaction | DriftEngine, baseline requirement keys, scheduler and queue; engines::drift_counts | Scheduled drift and baseline suites recorded | PASS, subject to I-03 |
| Startup limits and shared pool; queued work remains queued under saturation (Scenario 6, INV-007) | config startup parse and validation; HostedBlocks sharing; runner cannot observe sandbox capacity after engine conversion | Configuration tests exist; queue/Operator caps do not establish sandbox saturation behavior | FAIL I-02 |
| Existing HTTP/MCP shape and no new public API (material stop) | five specified operations plus new local evidence PUT in routes.rs/OpenAPI | Served OpenAPI test explicitly asserts the extra route | FAIL I-04 |
| First-class clients support JSON and file evidence under exact invocation permission | shared verify client reused in Rust/Python/TS; evidence demands global verifier_run_any | Exact grant test uses JSON; file journeys use broader editor authority | FAIL I-05 |
| Published correlation/timing fields, canonical results and protected server tables | results.rs/table declarations; Gate reserves execution_inputs/results/detail | Real Bifrost read assertions and Gate reserved-write tests recorded | PASS |
| Preserve history while removing obsolete live Eval/Judge/observation paths | additive queue migration; old nonterminal work cancelled explicitly; old rows retained; obsolete live modules deleted | SQL migration and cutover gate evidence | PASS |
| Existing Operator queue, immutable context, templates, no multi-check invented aggregate | existing operator_dispatches/worker; execution identity; drift_counts returns None for multiple reports | Immutable retry/template and delivery-isolation tests recorded | PASS |
| MCP, OpenAPI, SDK exports, CLI/UI/docs aligned; no compatibility aliases | mcp/verification.rs, CLI removal, generated declarations, docs and design changes | codegen/docs/MCP/CLI lanes reported in full gate | PASS except I-04 |
| All action/check rows and matched example repositories | three-language verifier_building_blocks/real-time/drift journeys and standalone example sources | Gate and external 5/5 TS, 5/5 Python, 1/1 Rust supplied | PASS with artifact/live-provider limits |
| Formatting/lints/dependency/tenancy/generation relevant suites (AC-010) | task proof records full gate on prior evidence commit; final commits record evidence | Supplied gate exit 0; no independent rerun in this static review | PASS as supplied evidence, not proof against findings |
| No new Card kind, runtime, durable result outbox, receipt, reservation, or second Operator loop | cumulative removals and new composition inspected | Static cumulative diff and supplied checks | PASS |

## Proposed findings

### I-01 — VIOLATION: queued manual verification drops the signed subject scope decision

**Obligation:** INV-003; task Scenario 2 and reuse map require preserving accepted authority and signed Card scope. The base's queued start_run explicitly enforced this scope; direct verification intentionally had a separate observation-scope rule, which need not change.

**Location:** `crates/wyrd/wyrd-server/src/components/verification/service.rs:553-593`; `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:1105-1169`.

**Evidence/path:** enqueue checks only `Permission::verifier_run(request.verifier_card_uid)` then snapshots AcceptedAuthority. `accept` resolves tenant Cards/readiness but never evaluates `submission.authority.principal.authorizes_card` against the exact subject. `Caller::accepted` preserves scope but does not enforce it. runner dispatch attributes result and model calls to request.card_uid. Neither HostedBlocks nor the engine validates this attribution. The prior `subject_in_scope` helper (base service.rs:708-724) and start_run call (base :337) were removed. All siblings were considered: direct execute intentionally did not apply this restriction; scheduled work keeps binding-owner acceptance; public Bifrost writes continue their own attribution boundary.

**Observable consequence:** a Card-bound principal allowed to invoke one Verifier can enqueue it against another same-tenant subject outside its signed graph, producing verification/model evidence attributed to that subject. RBAC permission to invoke the Verifier does not authorize this attribution.

**Smallest correction:** retain no-Postgres request admission and restore the decision at the existing batched queue acceptance boundary, which has the accepted principal and exact tenant subject row. Reuse Principal::authorizes_card/CardAttribution semantics and the shared Scribe audit stage; refuse out-of-scope queued work before it becomes runnable. Preserve the intentional direct behavior and scheduled owner rule; do not mint new authority or add per-request SQL.

**Closure proof:** real authenticated Card-bound queued submission with an exact Verifier grant and out-of-scope same-tenant subject must never execute; verify refusal via approved existing execution/error behavior and audit, plus in-scope success and direct behavior unchanged. Exercise delegated narrowing as well.

### I-02 — INCORRECT: resource saturation settles queued work instead of deferring it

**Obligation:** Deployment limits: background executions remain in the existing queue until runtime capacity is available; in-process waiting is bounded. Task Scenario 6 and INV-007. Wyrd design at the runner paragraph explicitly says resource refusal returns to the queue without consuming an attempt.

**Location:** `crates/wyrd/wyrd-server/src/verification/runner.rs:192,410-415`; `crates/wyrd/wyrd-server/src/verification/hosted/sandbox.rs:159-163`; `crates/vala/vala-eval/src/blocks/engine.rs:353-361,407-409`; `crates/wyrd/wyrd-server/src/verification/hosted/workspace.rs:65-68`.

**Evidence/path:** ClaimLoop is composed with no permit and keeps claiming/spawning due executions. Sandbox::exec returns ordinary BlockError::Retryable when its bounded running/waiting admission is full. The engine retries that only up to authored action retries, then marks the step failed, or immediately makes a code check inconclusive. Engine.run returns Ok(outcome), which dispatch maps to Completed and persists. That path never reaches EngineOutcome::Deferred/Transition::Defer. Workspaces::open additionally waits on acquire_owned without a bounded waiting admission, while the queue claim loop keeps spawning work; long occupation consumes execution deadlines. Sibling Bifrost pressure already has the Deferred path; OperatorPermits correctly limits Operator work and is not a substitute for runtime capacity.

**Observable consequence:** valid background work permanently completes inconclusive solely because another execution temporarily occupies Python capacity; scratch waiters can accumulate in process and time out instead of waiting durably in the existing queue.

**Smallest correction:** preserve typed runtime-capacity refusal through the hosted engine/runner seam and return it to the existing fenced queue deferral/release mechanism without spending attempts. Bound scratch admission waiters through the existing runtime owner. Do not reserve Python workers for Agent/model/non-Python waits, add another queue, or turn genuine check/provider failures into admission retries. This is existing approved capacity semantics, not a new scheduling product.

**Closure proof:** use a deliberately small real runtime pool and controlled held work (not synthetic host load). Submit another background code execution and workspace execution; prove they remain queued/retryable without inconclusive/timed_out terminal outcomes, then complete normally once capacity is released. Assert bounded admitted waiters and unchanged lease/shutdown behavior.

### I-03 — INCORRECT: an unmapped optional samples slot is silently inferred from stored measurements

**Obligation:** REQ-014 explicit named scheduled mappings, INV-006 honest missing evidence, Scenario 4 correct slot/source selection.

**Location:** `crates/wyrd/wyrd-server/src/verification/runner.rs:363-369`; `crates/wyrd/wyrd-server/src/verification/drift.rs:694-700,718`.

**Evidence/path:** `VerifierSpec::validate_schedule` (:883-911) requires mappings only for required slots, so an optional samples slot may legitimately be omitted. SQL CLAIM_SQL returns binding inputs; ClaimedRow::into_claimed stores them as ExecutionInput::Scheduled.slots. The runner matches `{ window, .. }` and ignores slots. DriftEngine iterates every drift check and scores its spec signal/profile for the subject/window, never consulting `check.on` or the binding selectors. Reports are keyed by requirement and then `DriftChecks::judge` returns the pre-scored report before its existing missing-slot branch. Thus a Custom drift check on optional `inputs.latency` can pass using retained latency rows although the binding never mapped that slot. Required-slot happy paths remain valid because registration constrains mapped measurement names.

**Observable consequence:** missing declared evidence becomes a real pass/fail judgment and may cause a failed background Operator dispatch; results no longer reflect the binding's declared sources.

**Smallest correction:** use the existing frozen scheduled slots at DriftEngine's report-production boundary: score a requirement only when its on slot is actually supplied by that binding. Reuse DriftChecks' missing-evidence semantics for the absent slot rather than fabricate an unscored-success report or infer names. Keep existing aggregation/scoring algorithms and source selectors; no new source registry or broad loader is needed.

**Closure proof:** real scheduled binding with one mapped required slot and one omitted optional samples slot; retain rows for both measurement names. Assert the omitted slot's requirement is inconclusive, cannot contribute a fabricated pass, and cannot alone cause an Operator; adding the explicit mapping must enable the expected existing statistical verdict.

### I-04 — DRIFT/material boundary: the extra local upload endpoint is a new public API

**Obligation:** TASK-005 Material Stop Conditions prohibit new public API; REQ-012 fixes the verification public routes, with retained binding inspection. REQ-010 requires existing upload plans/storage behavior; implementation precedent cannot approve a new endpoint.

**Location:** `crates/wyrd/wyrd-server/src/components/verification/routes.rs:45-46,162-200`; storage service's evidence_upload_plan; pg_openapi_contract test asserting the additional upload.

**Evidence/path:** authenticated clients receive UploadPlan::LocalFs pointing to `/v1/verification/evidence/local/{sha256}` and perform a new public PUT. The endpoint is mounted on local storage, published in OpenAPI, has its own permissions/body/errors and stores caller bytes. Conditional mounting and reusing UploadPlan do not make this an internal helper. Card upload's existing public endpoint is precedent for implementation, not approval of this contract.

**Observable consequence:** delivery adds a deploy/backend-specific public HTTP contract outside the approved route set and explicit no-new-public-API stop. Removing it without replacement also breaks approved local File journeys, so this cannot be silently treated as deletable surplus while claiming task completion.

**Required disposition:** obtain an approved specification decision for the local upload behavior, or demonstrate an already-approved existing upload endpoint can fulfill the full evidence namespace/authorization contract and remove the added API. Do not prescribe a newly invented replacement protocol in remediation. As it stands this requires SPEC_REVISION_REQUIRED rather than assuming approval from green OpenAPI tests.

**Closure proof:** after approved disposition, served OpenAPI and real local file journeys must match the resulting approved contract, alongside remote single-PUT behavior.

### I-05 — INCORRECT: exact Verifier invocation grants cannot use file evidence

**Obligation:** INV-002/003 first-class same workflows and exact Verifier invocation authorization; REQ-010/011 file verification and submission. Permission's own public contract supports custom roles granting one exact Verifier UID.

**Location:** `crates/wyrd/wyrd-server/src/components/verification/service.rs:323-329,408-414`; `crates/shared/wyrd-client/src/verify/mod.rs` RunEvidence::pin/upload planning → execute/enqueue.

**Evidence/path:** authorize_run checks `Permission::verifier_run(exact_uid)` and accepts a scoped grant. Both evidence planning and local upload require `Permission::verifier_run_any()` whose scope is All (`wyrd-runtime/src/permission.rs:511`), which a scoped grant does not cover. Shared client File pinning requests evidence plans before invoking either execution entry point; this happens on all language wrappers. Existing upload-plan and exact-Verifier tests separately pass because file journeys use broad editor authority and the scoped test submits inline JSON. Even pre-existing objects cannot help: the permission check precedes storage dedup lookup.

**Observable consequence:** a least-privilege user authorized to run a particular Verifier can run its JSON form but cannot verify or submit its File inputs without being granted authority to invoke every Verifier in the tenant.

**Smallest correction:** reuse the existing permission decision machinery to authorize evidence staging for callers holding applicable Verifier run authority, including an exact scoped grant, while keeping exact invocation permission checked at execute/enqueue and tenant-qualified digest storage. Do not require broadening customer roles to All or relax exact invocation checks. If selecting an exact Verifier for evidence planning changes the approved request contract, route that choice through specification revision rather than adding fields silently.

**Closure proof:** same exact-Verifier-only principal drives a real SDK File verify and submit through local upload and reads the result; it remains unable to invoke a different Verifier. Repeat evidence plan authorization on the remote-plan boundary without live cloud credentials.

## Verification assessment and limits

No test command was rerun and no runtime/provider credentials were used in this static acceptance review. The gate is useful evidence for exercised paths, but the gaps above are untested producer-to-consumer states and the new route is expressly asserted by a test; green tests cannot approve either. The acknowledged trace tie uses the documented deterministic SQL comparator and is not independently promoted to a finding here. The Oracle teardown failure lies outside this candidate's changed owner and is not reported as an unrelated defect.

The report makes no claim that live provider examples were verified. It makes no merge/push/deployment action and does not mark the task complete.
