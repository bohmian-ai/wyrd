# Independent findings validation — TASK-001

Result: **SPEC_REVISION_REQUIRED**. The approved creator-attribution rules do not select an admissible identity for actual callerless bootstrap paths. The candidate supplies two incompatible local conventions instead. The other retained findings below are bounded implementation or required-proof gaps; none licenses a broader redesign.

## Subject, inputs, and method

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/sub-act-attribution`.
- Immutable base: `b22718f2cf5da0ebcb7f26bc376157e5eec7768f`.
- Immutable candidate: `518a5d8b822a4e649a5fe90b14f3eb2b1fc8a0aa`.
- Approved specification: `changes/active/sub-act-attribution/spec.md`, revision 4.
- Original task: `changes/active/sub-act-attribution/tasks/TASK-001-sub-act-attribution.md` at the base; candidate adds execution evidence.
- Read subject/navigation map, claim comparison, both implementation reports, standards, maintainer, system, security, data, tenancy, and focused follow-up reports. No proposed finding was accepted by reviewer agreement alone.
- Used CodeGraph before source navigation. Examined the cumulative 222-path change inventory, applicable cumulative diff and actual producer/caller/consumer bodies. Applicable authority includes AGENTS §§2–11/14–16, agent rules, reference router, spec-driven-development, maintainer style and testing guidance, and the identity, accepted-authority, audit, Workflow, managed-ingest, query and peer contracts in Wyrd design, doctrine, Bifrost design and security posture. Implementation-added prose is evidence to assess, not approval.
- Independently traced each proposed correction boundary and sibling writers/consumers. No production source, approved spec, task, or existing report was changed. HEAD was rechecked as the candidate; only review artifacts are untracked.
- No Cargo, SQL, SDK, provider, deployment or runtime lane was rerun. Recorded implementation results are reported evidence whose assertions were inspected. They are not independently reproduced green runs.

This is the first review of this task. No earlier FIND IDs exist for it. The prerequisite verify-attribution review is a different task and supplies no inherited finding numbering.

## Proposal disposition

| Discovery source | Disposition | Ledger |
|---|---|---|
| DATA-001, DATA-002, IV-002, IV-003, TEN-001; follow-up §1 | REVISED: one creator-provenance boundary, with an unresolved autonomous-creation decision distinguished from already available authenticated context | FIND-TASK-001-1 |
| BEH-001, IV-001 | CONFIRMED: known registered local Workflow identity is lost at dependency preparation | FIND-TASK-001-2 |
| BEH-002; follow-up §2 | CONFIRMED: general HTTP authentication refusals are distinct from already audited workload exchange/private-peer refusals | FIND-TASK-001-3 |
| SYS-001, IV-004 | CONFIRMED: missing owner is consumed without the observable refusal required by the spec | FIND-TASK-001-4 |
| SEC-001 | CONFIRMED: credential completion event reconstructs direct attribution and discards verified history | FIND-TASK-001-5 |
| SEC-002 | CONFIRMED: served input-read context drops accepted history before Oracle | FIND-TASK-001-6 |
| BEH-003, IV-005, DATA-003; standards verification limits | REVISED: required negative/lifecycle proof remains missing; correction must use supported capabilities and existing seams | FIND-TASK-001-7 |
| REPO-001, MAINT-001 | CONFIRMED and consolidated: required rustdoc on changed items | FIND-TASK-001-8 |
| MAINT-002 | CONFIRMED: public result-actor description contradicts direct result behavior; peer prose describes removed envelope | FIND-TASK-001-9 |
| Maintainer scheduled-query denial handoff; follow-up §3 | REJECTED: no required reachable producer of the undergranted served-read denial was established | No finding |

The broad security/tenancy report statements that sentinel handling is correct concern existing audited producers, not proof that all required refusals are audited. They do not contradict FIND-TASK-001-3 after the surfaces are separated. A reported PASS on structural non-null columns does not establish the provenance of those values.

## Validated finding ledger

### FIND-TASK-001-1 — Creator provenance is dropped, and callerless creation has no approved identity rule

**Status:** REVISED. **Classification:** INCORRECT / DRIFT. **Sources:** DATA-001/002, IV-002/003, TEN-001, follow-up §1.

**Obligations:** REQ-001/002/003/005; INV-001/002; AC-016; the task's material stop condition for a new persisted identity convention.

**Locations:** `crates/wyrd/wyrd-sql/migrations/20260601000028_system_principal.sql:74–79`; `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:926–932`; `crates/wyrd-spec/src/auth/attribution.rs:37–43`. Context is lost in `components/platform/provisioning.rs:287–290,456`.

**Source trace and consequence:** Authenticated `TenantProvisioning::provision` constructs the real platform creator pair and passes it through `establish_and_promote` to `establish_tenant_administration`. That operation correctly uses it for the administrator and credential but calls the existing SYSTEM provisioning wrapper without it. The defining SQL function writes the newly generated SYSTEM ID as row ID, subject and actor. Thus a live inventory row demonstrably has SYSTEM as subject. Its conflict branch preserves this false provenance. The same provisioning operation then ensures built-ins without its creator context. `ensure_builtin` supplies `Attribution::unresolved()` unconditionally to the existing catalog registration owner, which persists it through the OLAP upsert. Caller-caused built-ins therefore lose a known creator and acquire the audit-unavailable sentinel instead.

The siblings cannot all be corrected by forwarding an authenticated caller:

- Startup `boot/mod.rs:499–505` calls `BuiltinTables::reconcile`; its complete body lists active tenants under OperatorPool and calls `ensure_tenant`, with no verified Wyrd principal or accepted operation.
- `BuiltinTables::ensure_tenant` includes the reserved audit owner, which gets only its audit table. This tenant is a storage owner, not a principal.
- The defining SYSTEM migration backfill has tenant IDs and database capability, but no creator.
- `wyrd-dev-fixtures/src/pg.rs::seed_tenant` provisions SYSTEM immediately after creating the tenant, before a tenant administrator/user exists.
- Platform root initialization is a separate operator command in `boot/init.rs`; it is not guaranteed before audit-table reconciliation and does not declare the root creator of later autonomous work.

These are real production/bootstrap and explicitly tasked fixture paths, not dormant APIs. A database login is not an existing Wyrd creator. REQ-010 permits the sentinel for unresolved audit; INV-001 does not extend that exception to catalog creation. The pre-existing `principal_id.rs` sentinel rustdoc does mention platform-internal operations without a caller, but within its stated scope of unauthenticated audit events; it does not establish non-audit catalog/SYSTEM creator attribution. New `Attribution::unresolved` documentation and the OLAP schema comment cannot approve their own expansion. Selecting a tenant administrator, SYSTEM, platform root or a new sentinel would choose new persisted meaning; changing eager reconciliation would also change availability/lifecycle.

**Minimum correction boundary:** Preserve already available authenticated provisioning attribution through the existing SYSTEM and built-in/catalog owners, retaining tenant transaction ownership, one active tokenless SYSTEM, eager built-in provisioning/reconciliation, conflict/idempotency and immutable creation pairs. Public dataset creation already uses the catalog's attribution input and must remain unchanged.

**Required authority decision:** Before a complete correction can be prescribed, revise the specification to select the subject/actor convention for creation genuinely lacking a verified caller, covering SYSTEM bootstrap, built-in reconciliation, reserved-audit-owner tables, defining SQL and fixtures. Also state whether this is an explicit exception to the trusted-source/tenant-lookup rules. This validator deliberately selects no identity convention. This is **SPEC_REVISION_REQUIRED**, not permission to implement a new convention under a remediation task.

**Focused closure after approval:** Extend existing tenant-provisioning/catalog integration coverage to read SYSTEM and built-in creation pairs, prove real caller causality where present, and prove retries/reconciliation preserve the first pair. Existing startup/fixture paths must assert the expressly approved autonomous rule. No new identity service, migration, bootstrap prerequisite or test harness is justified.

### FIND-TASK-001-2 — Registered local Workflow calls omit the Workflow Card

**Status:** CONFIRMED. **Classification:** MISSING. **Sources:** BEH-001, IV-001.

**Obligations:** REQ-004, INV-004, AC-011; original Scenario 7/V8.

**Locations:** `crates/shared/wyrd-client/src/workflow/local.rs:103–107`, reached through `workflow/mod.rs::Workflow::run_on`; changed local proof at `sdks/wyrd-sdk-rust/tests/integration/gateway_inference.rs:615–636`.

**Source trace and consequence:** `WorkflowCards::load` hydrates an exact registered Workflow and retains the loading client. `run_on` selects routes and constructs dependencies. The full `SelectedRoutes::dependencies` body constructs `PublicWyrdGatewayCaller::new(client)` without `with_subject`. That constructor sets both correlation fields to None; `call` therefore sends no Card header. Gateway capture correctly records the resulting null Card. The local V8 executes direct/delegated cases but only checks their pair, whereas the hosted path separately supplies the known Card.

The proposed scope excuse is not supported: `GatewayInvocation::attribute` accepts a registered observation-target Card for an unbound caller, Workflow is such a target, and V8 uses an administrator as subject. Existing Card-scoped/foreign/unregistered refusals must remain. This is missing evidence linkage for supported local registered Workflow execution, not a request to weaken Card authorization or attribute an unregistered authored Workflow.

**Smallest correction:** Retain the already loaded registered root UID through the current shared Workflow preparation and reuse `PublicWyrdGatewayCaller::with_subject(None, Some(uid))`. Rust/Python/TS share this owner. Keep hosted execution and null application Run semantics. No transport, credential or permission change is needed.

**Closure proof:** Extend the existing exact local V8 test to assert its registered Workflow UID and null run for direct and delegated calls, retaining pair/upstream assertions. Run that exact test and the existing hosted counterpart; preserve existing Card-scope refusal proof if that boundary is touched.

### FIND-TASK-001-3 — General HTTP authentication refusals leave no unresolved audit record

**Status:** CONFIRMED. **Classification:** MISSING. **Sources:** BEH-002, follow-up §2.

**Obligations:** REQ-002 refusal row, REQ-010, AC-015 and original Scenario 6. This finding rests on the explicit requested refusal behavior, not an inference that a failed authentication evaluated a resource permission.

**Locations:** `crates/wyrd/wyrd-server/src/http/middleware/authenticate.rs::authenticate_on`, error branch at line 116; `tests/bifrost/server/audit_retention.rs:173–181,218–241`.

**Source trace and consequence:** General protected and Bifrost HTTP layers call `authenticate_on`; its complete body returns the verifier rejection without staging anything. The shared extraction/verification helpers only return errors. The extractor fallback in `principal_extractor.rs:91–110` does likewise. The test drives missing credentials, malformed credentials and forged claims but expressly says those attempts are never retained; it then reads four authenticated Card decisions only. The green test therefore cannot satisfy the required unresolved-refusal outcome. Workload token exchange in `wyrd-auth/src/jwt_bearer.rs` and private peer rejection in `oracle/peer_audit.rs` are separate audited events and do not repair this absence.

Sibling public protocol ingress has its own `GatewayIngress::authenticate`, also returning a protocol refusal directly. OTLP has its API-key branch and otherwise delegates to the common HTTP path. These boundaries must not create duplicate events through the downstream extractor. The pure verifier must remain free of a new audit sink.

**Smallest correction:** Stage one authentication-refusal record where the server authentication owner has trusted request metadata, using the existing unresolved builder, structured failure detail, request correlation, reserved audit storage and sole nonblocking ScribeOutbox. Use no rejected token tenant/identity claim to choose payload identity or storage. Apply the same producer policy to sibling protected authentication entry points within the required surface, preserving their original status/envelope and no-next behavior. Do not invent a resource permission decision, alter verification or wait for persistence.

**Closure proof:** V6 must read the missing/malformed/forged attempts from retained reserved audit history by trusted correlation and assert sentinel/sentinel, unresolved detail, no nil/claimed ID, and unchanged response statuses. Preserve authenticated allow/deny/platform assertions and existing nonblocking loss tests. A workload exchange row does not count as the refused Card request.

### FIND-TASK-001-4 — Missing observation owner is silently consumed

**Status:** CONFIRMED. **Classification:** MISSING. **Sources:** SYS-001, IV-004.

**Obligations:** REQ-008, AC-017; Scenario 2 observable identity refusal.

**Locations:** `crates/wyrd/wyrd-server/src/verification/observations.rs:116–119`; producer `wyrd-sql/src/queries/verifier_runs.rs::LOCK_OBSERVATION_SUBJECTS_SQL` and `lock_observation_bindings`.

**Source trace and consequence:** The SQL binding query uses an optional owner-principal subquery. Its binding foreign key is to the owner Card, not to a principal, and readiness filtering does not supply the absent principal. Thus the returned state explicitly represents owner None; this missing-identity case is also directly required by the approved spec. `ObservationRunSink::write` immediately continues without a log, count or refusal outcome. It commits healthy authorized bindings and returns success, so the outbox consumes the slice. No rejected run/status is created. The trigger disappears without an observable binding failure; restoring the identity cannot replay that consumed trigger.

The other refusal paths do not cover it: `BackgroundAuthority::resolve` logs absent SYSTEM/recovery failures only after receiving an owner ID; the scheduler records skip telemetry. The sink is the only production caller of this binding-list owner.

**Smallest correction:** At this binding-acceptance owner, record the existing structured diagnostic with tenant/binding/owner-Card identity before consuming the nonretryable refusal. Keep valid sibling bindings processing. No downstream execution guard, new durable failure store, audit decision, batch retry loop or process/service failure is needed; the failed boundary is one binding acceptance.

**Closure proof:** Through an existing observation sink/journey fixture, omit the matching owner, prove no run/provider/fallback, prove the binding-specific diagnostic, and prove a healthy sibling still enqueues. Suspended-owner coverage cannot enter this None branch.

### FIND-TASK-001-5 — API-key completion audit discards the issuing actor and history

**Status:** CONFIRMED. **Classification:** INCORRECT. **Source:** SEC-001.

**Obligations:** REQ-002/010, INV-001; authenticated creation and audit history.

**Locations:** `crates/wyrd/wyrd-auth/src/issue_api_key.rs:149–172`; sole production caller `server/src/components/auth/routes.rs:604–609`.

**Source trace and consequence:** `issue_key` derives `audited_caller` from the verified request. Its permission decision and key insert receive A/B correctly. After committing the key it calls `IssueApiKey::audit` with only the effective Principal A. The complete audit body constructs `Attribution::direct(actor.id)` and CredentialIssuance detail without chain, yielding A/A. The event naming the issued key therefore contradicts the earlier permission decision and hides B. Both are reachable under a permitted delegated issuance; the correct decision row does not make the credential-specific completion row true.

**Smallest correction:** Carry the existing verified pair and originating chain through this existing completion-event owner. Reuse the runtime typed audit-delegation projection and established optional/empty chain encoding in the existing credential detail. Preserve target principal/key ID, expiry, resource/permission and nonblocking staging; never audit the key secret. Do not parse a token or add a second event/sink. Existing direct grant producers remain direct. The approved requirement to retain history already authorizes this projection; no new issuance behavior is selected.

**Closure proof:** Extend the existing owning issue-key integration/audit coverage for direct A/A and delegated A/B. Read the completion event keyed by issued key ID and compare its pair with the creation row and exact verified history; assert no plaintext credential. Run only that exact test and applicable contract/codegen checks if the detail shape changes.

### FIND-TASK-001-6 — Verifier input-read audit loses the accepted delegation history

**Status:** CONFIRMED. **Classification:** MISSING. **Source:** SEC-002.

**Obligations:** REQ-008/010; retain originating history while recording run-subject/SYSTEM for narrow reads.

**Locations:** `server/src/verification/authority.rs:172–177`; Drift caller `drift.rs:896`; Eval callers `eval.rs:151–156,565–582`.

**Source trace and consequence:** AcceptedAuthority stores original principal and chain. Drift supplies only `run.authority.principal` to `serving`; Eval similarly strips its reader input to Principal. `SystemReadAuthority::resolve` creates a new SYSTEM query context; `serving` sets only ServedSubject ID/kind. Oracle correctly builds subject/SYSTEM and serializes `context.delegation_chain` into its read-decision/security detail, but that chain is empty. Local and peer contexts therefore retain incomplete accountability for a delegated accepted run.

Both reader producers share this authority builder. Oracle already has `AuthorizedQueryContext::with_delegation_chain` and its serialized context projection; no new mechanism is needed. Successful input reads are reachable in supported queued manual Drift as well as Eval. The sibling outer scheduled-query object-denial reconstruction is not required to demonstrate this loss.

**Smallest correction:** Carry the accepted history with the served subject into this existing SYSTEM read-context producer, using the existing chain projection/serialization. Preserve SYSTEM as execution actor and the exact table-UID-scoped internal authority. Do not replace the capability with caller permissions, recover fresh subject grants or evaluate historical actors as authority.

**Closure proof:** Execute an existing delegated queued Drift input read and assert retained A/SYSTEM plus the exact accepted actor history. Cover the shared Eval reader construction and existing context serialization with focused supporting proof. Keep table-grant scope unchanged and check that the caller gains no general input-read permission.

### FIND-TASK-001-7 — Explicit authority/recovery/scheduled proof remains absent

**Status:** REVISED. **Classification:** MISSING (required proof, not an observed permission escalation). **Sources:** BEH-003, IV-005, DATA-003; standards evidence limits.

**Obligations:** AC-001/009/013/017/018; Scenarios 1–3 and AGENTS §11.

**Locations:** `tests/bifrost/server/eval_verification.rs::realtime_subject_actor_authority` and `background_owner_subject_system_actor`; `verification_runtime.rs::queued_subject_authority_survives_restart`; task Implementation Evidence/Exact commands.

**Source validation:**

1. V1 succeeds direct and one-hop delegated, and denies both asymmetric permission cases. It never delegates an already narrowed token again to a permissive later actor. Existing exchange-only tests do not prove the new Verifier→judge→gateway handoff under that history.
2. V2 drives observation-triggered Eval only; its run query explicitly selects observation origin. No scheduled execution/result proves AC-001. Suspended owner is covered, but missing SYSTEM, absent owner and foreign-owner acceptance rejection are not directly exercised as claimed for AC-017.
3. V3 does persist an actual delegated manual Drift run, edit grants and create fresh runtime owners across retry. Both runtime attempts use EngineScript, which returns canned outcomes before a Verifier/gateway permission decision. Snapshot byte equality and result pair are useful storage/lifecycle proof, but cannot detect a consumer replacing the loaded snapshot with unrestricted subject grants. Initiating-token expiry is not exercised.
4. Reported commands omit the required docs lane, touched support-desk example execution/equivalent and served OpenAPI contract lane after changed wire fields. Codegen does not prove runtime OpenAPI. These are unproven checks, not assertions that any lane fails.

**Smallest correction:** Extend existing V1 with further delegation and refusal/no extra provider request. Extend V2 or the existing scheduled owner journey with one supported scheduled occurrence and persisted owner/SYSTEM result; exercise missing/foreign identity acceptance refusal at the nearest existing real boundary, including observable refusal and healthy siblings. Use the existing supported scheduled implementation: do not invent scheduled Eval merely to add a judge when that origin supports Drift.

Keep V3's storage/retry proof. Add the smallest existing-owner integration seam that reloads persisted narrowed accepted authority after changed grants and initiating-token expiry into the real protected gateway caller, proving an otherwise-permitted model stays denied with no provider request. A supported public manual Drift run has no judge; do not add queued manual Eval, a public API, second authority store, fake permission evaluator, crash harness or new fixture framework to satisfy this proof. The seam must use actual claimed authority and the actual protected consumer, not only compare JSON. Record why that supporting seam is needed alongside the real public queue journey.

Obtain missing docs/example/served-contract proof through existing tasks or a demonstrated already-run equivalent. Record exact selectors for any new focused tests; do not claim the same green name proves branches it lacks.

**Closure proof:** Run only the exact amended V1/V2/V3 and added existing-owner seam selectors, plus narrow write-set lanes and the missing mandated docs/examples/served OpenAPI checks. Broad user-journey suites/aggregate reruns remain change-review work. No production behavior change is prescribed unless these tests expose one.

### FIND-TASK-001-8 — Changed Rust operations and new tests lack mandatory rustdoc

**Status:** CONFIRMED. **Classification:** VIOLATION. **Sources:** REPO-001, MAINT-001.

**Authority:** AGENTS §16, agent-rules rustdoc hard gate; explicitly blocking under task-review skill.

**Validated changed items:**

- `wyrd-sql/src/queries/cards/register.rs`: `lookup_existing_operation` (144), `lookup_expired_operation` (168), `lookup_operation_by_id` (192), `insert_registration_operation` (211), `insert_card_row` (235).
- `wyrd-sql/src/row_types/cards.rs:122`: `ParsedCardRow::try_from`.
- `server/src/components/cards/service.rs:636`: `replay`.
- New tests `wyrd-runtime/src/request_context.rs:317`, `vala-sql/tests/integration/pg_olap_catalog.rs:84`, and `wyrd-sql/tests/integration/pg_cards_register.rs:239`.

Complete bodies show fallible SQL/serialization/domain conversion and caller-owned transaction semantics without required Errors sections (or any method rustdoc for try_from). New tests contain expects/assertions without Panics sections. Cumulative changes alter their identity/replay contracts; these are not an unrelated untouched-documentation sweep. An unchanged capture-peer helper is excluded.

**Smallest correction:** Document the enumerated items accurately in place: absent/conflict result meaning, actual error conditions, transaction/cancellation/partial-progress where relevant, and actual test panic conditions. Reuse neighboring registration/queue documentation. Do not change behavior, suppress lints, add a scanner or create utility code. This is an explicit repository completion gate even without a runtime defect.

**Closure proof:** Source inspection of docs against full bodies plus applicable formatting/whitespace checks; no new runtime test is justified for prose alone. Batch with other corrections rather than a style-only round.

### FIND-TASK-001-9 — Result and peer documentation describes a different contract

**Status:** CONFIRMED. **Classification:** INCORRECT. **Source:** MAINT-002.

**Obligations:** REQ-002/006 and coverage inventory documentation consistency.

**Locations:** `docs/src/content/docs/understand/how-verification-works.md:167`; related `architecture/wyrd-design.md:1264–1269` and `architecture/bifrost-design.md:1406–1413`.

**Source trace and consequence:** Direct `VerificationControl::stage_result` stages `caller.attribution()`, preserving A/A or A/B. Queued `runner::result` stages run subject/SYSTEM. Judges/input reads use SYSTEM. The changed public column description says SYSTEM executes every check and is every result's actor, misleading a query that filters realtime result rows by SYSTEM. Architecture repeats the overgeneralization. Bifrost peer prose says attribution is present only on result destinations; the new closed capture wire supplies its pair for every destination, while audit preserves its content identity.

**Smallest correction:** Correct prose to distinguish direct result attribution from queued execution and judge/input-read attribution, and describe pair-bearing peer capture consistently with the actual wire. Keep production behavior unchanged. When editing related paragraphs, remove contradictory old capture-transport-as-payload statements and ephemeral AC references noted by standards, without expanding to unrelated docs.

**Closure proof:** Compare revised prose with direct stage_result, queued result, ScribePeers submission and capture-peer batch decoding; run docs:check for docsite edits. Existing source/tests already establish the implementation distinction; no new runtime test or field is required.

## Rejected and narrowed proposals

**ScheduledQueryCaller served object-denial handoff — REJECTED.** The reconstruction would lose ServedSubject if QueryForbidden reached it, but actual served readers construct fixed queries over exactly the table UIDs granted by SystemReadAuthority: Eval observations/spans and Drift observations. Drift Custom is a metric name, not arbitrary SQL. Oracle object denial checks the immutable registered table UID grants; its other QueryForbidden path concerns gateway payload columns, which these readers do not query. Eager built-in ensure precedes serving; registrations do not mutate/delete. Dependency failures are not converted to this object denial. Authenticated scheduled callers retain Caller through their other dispatch branch. A deliberately undergranted test-only context is not an existing required producer. Do not add speculative guards or a new public query path.

**Proof correction narrowed.** Reject any reading of IV-005 that mandates new queued manual Eval, scheduled Eval, a process-kill harness or a complete gateway journey suite. Those are not needed to exercise the existing persisted-authority consumer. Reject a separate finding solely because Rust/Python/TS timelines use direct calls while MCP proves delegated operation spans over shared server code; no additional independent source defect was demonstrated there.

**No authorization exploit inferred.** None of the creator/audit/history findings demonstrates a cross-tenant read, SYSTEM authority gain or permission union. The gateway still uses the retained effective Principal; the data provenance and explicit proof failures are sufficient reasons to withhold acceptance.

## Reconciled 36-obligation matrix

PASS means inspected implementation plus credible reported evidence supports the obligation; it is not an independent runtime attestation. FAIL includes explicitly required proof gaps and partial failures. F1–F9 refer to the stable findings above.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 non-null pair | Defining SQL, Attribution, managed_arrays and audit content retain non-null fields; provenance assessed separately | V4/V5/V9, fresh SQL reported | PASS |
| REQ-002 correct work identities | Caller/accepted execution correct generally; bootstrap creators and issuance completion/refusals incomplete | V1/V2/V6/V8; F1/F3/F5 counterexamples | FAIL |
| REQ-003 subject authority/cost; never SYSTEM sub | Gateway retains effective Principal and ledger subject; SYSTEM creation violates final invariant | V1 asymmetric denials/V2; F1 | FAIL |
| REQ-004 Card roles and Workflow Card | Results/run/bindings use correct roles; local registered Workflow drops Card | Hosted V8 versus local V8; F2 | FAIL |
| REQ-005 full schema/writer inventory | Columns/ordinary updates correct; creator provenance wrong in inventory owners | SQL/codegen plus F1 | FAIL |
| REQ-006 wire/SDK/docs/reserved names | Wire and SDK fields/reservation aligned; public actor documentation wrong | V5/V9/codegen; F9 | FAIL |
| REQ-007 active SYSTEM | Production and fixture provisioning create active tokenless SYSTEM | V2 checks seeded row before fixture provision | PASS |
| REQ-008 deferred authority/history/refusal | Stored credential-free snapshot and claim preserved; read history and missing-owner visibility fail | V3 storage proof; F4/F6/F7 | FAIL |
| REQ-009 replay scope | Existing registration/manual locks/lookups/uniqueness include pair; Workflow/background keys retained | V4/owner tests reported | PASS |
| REQ-010 audit payload/history/storage | Transport/content separation correct; refusals and two history projections incomplete | Existing platform/JWT/MCP evidence; F3/F5/F6 | FAIL |
| REQ-011 complete propagation/further narrowing | Direct/queued gateway retains Principal; required further-hop negative not exercised | V1 single-hop only; F7 | FAIL |
| INV-001 trusted allowed sources | Verified Caller/Gate correct; autonomous catalog uses unauthorised sentinel convention | F1 source trace | FAIL |
| INV-002 SYSTEM actor only | Execution pair correct; SYSTEM self-creation counterexample | F1 | FAIL |
| INV-003 tenant isolation | TenantConn/RLS, same-tenant owner recover/SYSTEM lookup remain | SQL queue isolation and tenant check reported; AC-017 proof gap separate | PASS |
| INV-004 origin/Card meanings | Field roles correct generally; registered Workflow call loses required Card | F2 | FAIL |
| INV-005 trace never authority | Trusted operation binding/rebind; no trace-derived attribution | Telemetry subtree test/V7/V10/V11 | PASS |
| INV-006 attribution never grants | Gateway Principal authorization; separate scoped internal reads | V1/V2 negatives and source | PASS |
| INV-007 immutable creators/snapshot | Updates/conflicts/claim/retry do not replace accepted values | V3 JSON equality/V4; wrong original provenance separate | PASS |
| AC-001 scheduled pair/results | Scheduler capture/runner source present | V2 observation only, no required scheduled journey; F7 | FAIL |
| AC-002 observation owner over writer | Observation sink/recovered owner/Eval gateway | V2 permitted owner and denied owner, different writer | PASS |
| AC-003 direct rows/audit | Caller/result/gateway and language row projections | V1/V5/V6/V7/V9/V10 | PASS |
| AC-004 delegated gateway/audit | Last verified actor, direct shared gateway capture | V1/V5/V6/V9 | PASS |
| AC-005 reject reserved names | Queue preflight/Gate/Scribe reservations | V5/V9 each SDK | PASS |
| AC-006 old attribution cleanup | Source-backed inventory; remaining identity/owner keys distinct | Disposition and codegen reported | PASS |
| AC-007 no new migration/fresh DB | Defining declarations changed; specified incremental file removed | test:sql reported 203 passes | PASS |
| AC-008 fixture active SYSTEM | PgFixture seed uses existing provisioning | V2 pre-provision assertion | PASS |
| AC-009 required scoped verification | Named commands credible but explicit branches and mandatory lanes absent | F7 | FAIL |
| AC-010 request/judge spans and joins | Trusted route rebind; Card role joins | V7/V10 direct, V11 delegated, telemetry unit | PASS |
| AC-011 Workflow pair/Card/null Run | Hosted path correct; local known Card omitted | F2 | FAIL |
| AC-012 actor-denied realtime judge | Retained narrowed Principal reaches gateway | V1 refusal/no extra provider call | PASS |
| AC-013 reloaded authority denied model/expiry | Queue persists snapshot; Eval consumer uses it | V3 scripts bypass protected call and no expiry; F7 | FAIL |
| AC-014 identity-specific replay | Pair-scoped existing SQL/locks/conflict semantics | V4/unchanged owner tests reported | PASS |
| AC-015 platform/unresolved/nonblocking audit | Existing payload separation/platform correct; ordinary invalid/absent auth disappears | F3; V6 expects omission | FAIL |
| AC-016 all writers/read projections | Managed ingress correct; creator and audit projections incomplete | V5/V9 and F1/F5/F6 | FAIL |
| AC-017 observable identity refusal/frozen authority/narrow reads | Tenant resolution and scoped reads correct; missing owner silent; negative proof incomplete | F4/F7; V2 suspended-owner subset | FAIL |
| AC-018 every hop and further delegation | Direct propagation source and single-hop tests correct | Required further-hop refusal absent; F7 | FAIL |

Constraints/non-goals assessed separately: existing Caller, queue, issuer recovery, gateway evaluator, catalog, outbox, telemetry route and language client owners are reused. AcceptedAuthority earns existence as durable credential-free operation authority; BackgroundAuthority retains the existing issuer rather than duplicating resolution. No new permission algebra, token/delegation issuance contract, role, dependency, configuration knob, queue/store, audit transport, compatibility alias, schema-evolution path or durable Workflow lifecycle was found. Variant policy, subject sampling algorithm, operational lineage, Operator rechecks, background deduplication and Workflow subject-based ownership remain within scope. F1 identifies the material exception to approved-authority compliance.

## Verification and disposition limits

The implementation record reports focused V1–V11, SQL, owner and static checks passed. Source inspection supports much of that coverage while showing why the omitted branches cannot be inferred from selector names. General refusals are expressly unrecorded in V6; V3's scripts cannot prove recovered authorization; local V8 omits Card assertions. Missing docs/examples/served OpenAPI evidence remains missing, not known-red.

The candidate stayed immutable throughout validation. No runtime reruns, implementation fixes, staging, commits, merge, push or deployment were performed. Rejected proposals are omitted from the retained ledger rather than softened into optional advice. Findings 2–9 have bounded correction outcomes, but the complete task cannot receive a decision-complete remediation under r4 until the narrow autonomous creator-attribution decision in finding 1 is approved. The orchestrator should return SPEC_REVISION_REQUIRED, link this ledger and the final verdict, and avoid inventing a bootstrap convention or silently changing availability.

Skipped runtime/build/check reruns; validation was static against the immutable candidate.
Risk: creator provenance is not implementable consistently under the current approved bootstrap rules, and the retained bounded gaps still prevent task acceptance.
