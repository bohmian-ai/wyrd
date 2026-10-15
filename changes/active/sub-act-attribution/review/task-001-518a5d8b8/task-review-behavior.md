# TASK-001 independent behavior review

Result: **FAIL**.

## Subject and method

- Base: `b22718f2cf5da0ebcb7f26bc376157e5eec7768f`.
- Candidate: `518a5d8b822a4e649a5fe90b14f3eb2b1fc8a0aa`.
- Approved authority: `changes/active/sub-act-attribution/spec.md`, revision 4; original task `changes/active/sub-act-attribution/tasks/TASK-001-sub-act-attribution.md` (original instructions at base; candidate adds implementation evidence).
- Reviewed the cumulative changed-path inventory and source/diff across request authentication, runtime context, direct verification, queued authority, background acceptance, gateway admission/capture/accounting, local and hosted Workflows, SQL creation/replay, Gate/Scribe stamping and peer forwarding, query read attribution, tenant span export, contracts and SDK consumers, and named acceptance tests.
- Started with CodeGraph, then followed concrete callers and consumer source. Applied AGENTS.md, agent rules, spec-driven development, maintainer style, testing workflows, and the relevant design/security/Bifrost/doctrine contracts. Approval is not derived from the implementation record.
- Verification is the reported task record; no Cargo, runtime, provider, or database checks were run by this reviewer. The candidate HEAD remained the stated commit; the only untracked material was the review directory.

## Acceptance matrix

`PASS` means implementation and available supporting evidence support the obligation under this review's caller-to-result lens. It does not independently attest that the reported commands ran. Findings below explain all `FAIL` entries. Grouped rows enumerate every approved requirement, invariant and acceptance criterion.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 one non-null server pair | `Attribution`; defining SQL schemas; `Caller::attribution`; Scribe `ManagedValues` and `managed_arrays`; audit payload fields | V4/V5/V6/V9; fresh SQL/codegen reported | PASS |
| REQ-002 identity semantics | verified chain last actor; direct original caller result pair; queued owner/SYSTEM and judge `Caller::accepted`; hosted Workflow caller pair | V1/V2/V7/V10/V11 and hosted V8 | PASS |
| REQ-003 permissions/cost follow subject | `GatewayInvocation::run` retains effective `Caller.principal`; `GatewayLedger` charges that principal; execution actor changes only attribution; narrow SYSTEM reads remain separate | V1 asymmetric single-hop denials, V2 owner denial | PASS |
| REQ-004 Card roles and Workflow Card | result payload `card_uid`/`verifier_card_uid` and hosted Workflow are changed correctly; shared local Workflow producer still sends no Card | V8 local path checks pair only; hosted path checks Card | **FAIL — BEH-001** |
| REQ-005 schema inventory/immutability/policy | defining migrations and SQL query projections; identity keys retained; Observation managed columns; Variant sampling now uses subject | V4/V5/V9, test:sql, inventory | PASS |
| REQ-006 public projections/reserved names/no aliases | spec types, managed constants, queue/client reserved-name copies, SDK queries and examples updated | V5/V9; codegen and language checks reported | PASS |
| REQ-007 SYSTEM provisioning | production provisioning and `wyrd-dev-fixtures/src/pg.rs`; active SYSTEM lookup reused | V2 explicitly reads seeded active SYSTEM before runtime fixture provisioning | PASS |
| REQ-008 deferred accepted authority | credential-free `AcceptedAuthority`, queue JSON persistence, original principal/history in claimed run, `Caller::accepted` | V3 proves persisted bytes and result attribution, but bypasses actual protected execution after reload | **FAIL — BEH-003 (required proof)** |
| REQ-009 pair replay scope | registration lookup/unique scope, manual advisory lock/lookup/index include both IDs; background occurrence and Workflow keys retained | V4; existing Workflow owner tests | PASS |
| REQ-010 unresolved audit identity | authenticated and platform payload pair preserved; JWT workload refusal replaces nil; general protected HTTP authentication refusals still disappear | V6 expressly expects no authentication-refusal record | **FAIL — BEH-002** |
| REQ-011 realtime/further propagation | original effective principal retained through direct executor and gateway; no subject-only permission reconstruction | V1 proves single hop, but no multi-hop denied model journey | **FAIL — BEH-003 (required proof)** |
| INV-001 trusted source of pair | Caller produced from verified token; Gate derives pair; peer capture is private authenticated transport; managed names reserved | V5/V9, peer owner tests | PASS |
| INV-002 SYSTEM actor only | accepted principal remains subject; narrow input reads use `served_subject`; judge pair override changes actor | V1/V2/V3 result rows | PASS |
| INV-003 unchanged tenant isolation | TenantConn/RLS remains boundary; owner/SYSTEM resolve on same tenant transaction | SQL and tenant checks reported; source tracing | PASS |
| INV-004 origin/Card role meanings | result payloads, binding/run views and hosted Workflow use role names; local Workflow Card missing | V8/V7/V10 | **FAIL — BEH-001** |
| INV-005 tracing never supplies identity | tenant route binds verified identity; gateway operation rebinds same tenant sink from Caller; existing eligibility/sampling retained | telemetry route unit test; V7/V10/V11 | PASS |
| INV-006 attribution is not authority | accepted effective Principal remains authorizer; pair used in capture; narrow internal read capability separate | V1 asymmetric denials; source trace | PASS |
| INV-007 immutable creators/authority | SQL conflict/update paths leave creator fields; idempotency replays return original run; claim does not replace JSON | V4 and V3 snapshot equality | PASS |
| AC-001 schedule owner/SYSTEM including results/judge | scheduler now resolves BackgroundAuthority and shared enqueue stores pair | V2 only exercises observation origins; required schedule journey absent from recorded proof | **FAIL — BEH-003 (required proof)** |
| AC-002 observation owner versus writer | observation binding lookup followed by owner authority; Eval uses accepted run caller | V2 A/B owners, separate admin writer, owner denial and result pair | PASS |
| AC-003 direct gateway/verify/audit | original caller pair on request and direct result; gateway pair in capture | V1/V5/V6 and all-language rows/timelines | PASS |
| AC-004 delegated gateway/audit | `Caller::attribution`, capture projection, audit builder | V1/V5/V6/V9 | PASS |
| AC-005 spoofed managed columns refused | shared reserved-name lists and server decoder checks | V5/V9 each SDK | PASS |
| AC-006 old-name cleanup | source inventory; contracts/queries/examples/docs replace work-attribution names; remaining identity keys classified | inventory and codegen reported | PASS |
| AC-007 no new migration/fresh apply | defining schemas edited; specified migration removed | test:sql reported | PASS |
| AC-008 fixture SYSTEM active | tenant fixture seed inserts active SYSTEM | V2 checks actual seeded row | PASS |
| AC-009 required scoped proof | owner/static checks and V1–V11 recorded, but test names alone conceal missing acceptance branches | BEH-003 describes omitted branches | **FAIL — BEH-003** |
| AC-010 request versus judge spans and joins | operation route rebind, result Card role fields and join queries | V7/V10 direct; V11 delegated, telemetry subtree unit test | PASS |
| AC-011 Workflow caller/Card/null application Run | hosted preparer correct, local producer leaves Card null | local V8 assertion omits Card and hosted test does not exercise local producer | **FAIL — BEH-001** |
| AC-012 originating actor cannot widen judge | direct verifier passes captured effective Principal into gateway | V1 delegated C-denied and subject-denied cases | PASS |
| AC-013 reloaded accepted restriction enforced | persisted claim authority exists, but V3's scripted executor does not perform any gateway permission decision | V3 JSON comparison is necessary but insufficient | **FAIL — BEH-003** |
| AC-014 separate replay identities and immutable retry | pair-scoped SQL/locks/lookups; unchanged Workflow/background discrimination | V4 and owner tests reported | PASS |
| AC-015 platform/unresolved audit/nonblocking | platform payload and workload refusal are correct; general invalid/absent credentials not retained | V6 currently expects disappearance; JWT workload owner test covers only different surface | **FAIL — BEH-002** |
| AC-016 every writer/read projection; sampling | Gate→frame→Scribe pair; internal capture peer pair; tables/contracts; subject sampling | V5/V9; source inventory; owner tests | PASS |
| AC-017 fail closed missing owner/SYSTEM, frozen authority, narrow reads/operators | background resolver rejects inactive identities; claimed JSON retained; tokenless read grant remains table-scoped; operators still recheck | V2 suspended owner, narrow authority tests, V3 snapshot equality; broader missing/foreign acceptance proof is limited | PASS for implemented behavior; verification limits noted |
| AC-018 full realtime path and further delegation | direct path effective authority propagation is present | V1 never exchanges an already delegated subject into a subsequent actor before verifying | **FAIL — BEH-003** |
| Constraint: reuse existing owners; no parallel engine/permission evaluator/store/writer | extended Caller, queue, issuer recovery, gateway, Scribe outbox, telemetry route; no replacement orchestration | cumulative diff and caller trace | PASS |
| Constraint: no compatibility/migration/backfill or new dependencies/features | defining files edited, old conversion/fields removed; no manifest additions | diff inventory, dependency/codegen checks reported | PASS |
| Non-goals: identity keys, Variant payload policy, engine lineage, operator dispatch state/rechecks | identity keys retained; managed sampling changes axis only; operator implementation only renames Card fields | source/diff; owner checks | PASS |
| Non-goals: token/delegation issuance semantics, roles, Workflow durability/idempotency, trace continuity | issuance changes audit projection only; Workflow retains captured caller lifecycle; queue/trace cause preserved | source diff and Workflow/timeline tests reported | PASS |

## Proposed findings

### BEH-001 — Registered local Workflow steps lose their Workflow Card

- Classification: **MISSING**.
- Violated obligations: REQ-004, INV-004, AC-011; original scenario 7 and V8 loaded-Workflow requirement.
- Changed location exposing the gap: `sdks/wyrd-sdk-rust/tests/integration/gateway_inference.rs:636` accepts the local run after checking only pair; source producer is `crates/shared/wyrd-client/src/workflow/local.rs:103–107`, reached from `Workflow::run_on` (`workflow/mod.rs:260–277`). Hosted correction at `components/workflow/host.rs:346–363` covers a different producer.
- Reachable path: load an exact registered Workflow with `Cards::workflow().load` → local `Workflow::run` → `SelectedRoutes::dependencies` → `PublicWyrdGatewayCaller::new(client)` → public gateway. `new` initializes `card_uid: None`; no `with_subject` call occurs, so no Card header is sent and capture records null `card_uid`.
- Consequence: model evidence/cost cannot be joined to the Workflow Card for this supported SDK execution path, despite the same exact registered Workflow recording its Card when hosted.
- Existing alternative and sibling consumers: public `PublicWyrdGatewayCaller::with_subject` already projects a Card header independently from application Run. `observe/invoke.rs` already uses it. Hosted Workflow and Verifier internal callers use the resolved gateway subject. Keep all those owners; no new caller or transport is needed.
- The reported Card-scope limitation does not justify a universal omission: `GatewayInvocation::attribute` (`invocation.rs:742–756`) allows an unbound User/admin to attribute to any registered observation-target Card in its tenant, and `CardKind::is_observation_target` includes Workflow. Therefore the existing V8 administrator/direct and delegated-subject case can carry this exact Card without changing authorization. Scoped callers must continue to satisfy the existing scope and foreign/unregistered Cards must remain denied.
- Smallest correction: carry the already loaded registered Workflow root UID into the existing local dependency preparation and reuse the public caller's existing subject projection with no application Run. Preserve existing Card validation; do not mint credentials, widen Card scope, add a new trust bypass, or change hosted execution. An authored unregistered Workflow with no registered UID does not justify fabricating one.
- Closure proof: extend the existing local V8 direct/delegated run to assert the exact Workflow `card_uid` and null `run_id` as the hosted test does, retaining the provider-call and pair checks. Exercise the existing scoped refusal as a regression if its owner changes.

### BEH-002 — General authentication refusals still vanish instead of recording the unresolved pair

- Classification: **MISSING**.
- Violated obligations: REQ-002 authentication-refusal row, REQ-010, AC-015, task scenario 6.
- Exact changed location: `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_retention.rs:173–181,218–226,239–249` deliberately specifies and tests no record for missing, malformed and forged credentials. Production consumer: `http/middleware/authenticate.rs:116` immediately returns the rejection; the shared token verification helper has no audit collaborator. The authenticated extractor fallback similarly returns the shared verification error before constructing Caller.
- Reachable path: unauthenticated or forged-token request to `/v1/cards/by-uid/...` → `authenticate_on` → shared verifier refusal → response, with no call to the Scribe outbox. The JWT-workload refusal fix is a sibling `/auth/token` exchange path and does not cover these requests.
- Consequence: the exact unresolved authentication attempts required by AC-015 leave no retained accountability record. V6's green result currently proves the opposite of its task's requested behavior.
- Existing alternative: `audit::audit_event_unauthenticated` already constructs the reserved sentinel pair, and `ScribeOutbox` already carries reserved-platform audit history. The existing workload JWT refusal path demonstrates unresolved failure detail. Reuse these; do not trust rejected principal or tenant claims to select an identity or tenant.
- Smallest correction: stage one unresolved authentication-refusal event at the existing authentication boundary that owns the refusal and request metadata, through the sole nonblocking outbox. Route before-tenant resolution through existing reserved audit storage rather than an unverified token's claimed tenant. Trace sibling protected authentication boundaries (normal/Bifrost HTTP, extractor fallback, MCP/gRPC/gateway where applicable) so a shared refusal is staged once rather than patched per handler. Preserve original response/status and avoid adding fake resource permission decisions.
- Closure proof: V6 must locate the missing/malformed/forged attempts by trusted request correlation in the reserved retained audit history and assert sentinel/sentinel, unresolved detail, no nil/claimed identity, and unchanged statuses. Keep authenticated allow/deny/platform assertions and existing terminal audit-failure nonblocking proof. Do not count unrelated token-exchange audit as the rejected request's event.

### BEH-003 — Required propagated-authority acceptance branches are not exercised

- Classification: **MISSING** (acceptance evidence, not a claim that the source necessarily grants excess authority).
- Violated obligations: AC-001, AC-009, AC-013, AC-018; scenarios 1–3 explicitly require these branches. This is not optional coverage expansion: these conditions are stated in the approved criteria and tasked proof.
- Evidence:
  1. `verification_runtime.rs:2914–2949` supplies `EngineScript` outcomes for both runtime attempts. V3 compares durable JSON and writes a canned Drift result, but no recovered caller performs a judge/gateway permission decision. It cannot falsify a later consumer that reconstructs unrestricted subject authority. No token expiry case is exercised.
  2. `eval_verification.rs:3174–3201` covers direct/single-actor success and asymmetric single-actor refusal. It never further delegates a narrowed token to a later permissive actor before invoking verify. Existing auth exchange integration tests cover nested issuance/intersection, but do not cover the new attribution and verifier-to-gateway handoff end to end.
  3. `background_owner_subject_system_actor` is entirely observation-triggered; its row query explicitly filters `origin = 'observation'` (`eval_verification.rs:2795–2797`). It does not prove AC-001's scheduled run/result/judge pair despite the implementation record citing V2 for it.
- Consequence: passing all named selectors does not establish the approved restart and further-delegation security property or scheduled origin behavior. These are the principal trust/lifecycle seams this task changes.
- Minimal correction/reuse: extend existing V1/V2/V3 owners, not a new test harness or production framework. V1 should further delegate an already restricted subject token to a permissive actor, then verify via the real server and assert original subject/current actor acceptance, A/SYSTEM judge decision, preserved history, refusal and zero extra provider requests. V2 should drive one scheduled occurrence through the existing scheduler/runner and read its pair/result (and permitted judge when supported). V3 must exercise the real claim-to-accepted-caller protected gateway boundary after durable reload and changed grants/initiating expiry, proving the excluded model remains denied. The manual public contract is Drift-only: do not add queued manual Eval or another public API merely to test it. Use the existing persisted-claim integration seam with actual protected execution where that boundary cannot be expressed as a public manual judge journey; retain the current storage/retry result assertions as supporting proof and explicitly document that seam.
- Closure proof: the exact focused V1/V2/V3 commands, plus only the exact existing-owner seam test needed for reloaded authority. Assert no provider request on protected denial; comparing serialized snapshots alone is insufficient. No broad aggregate is requested for this remediation.

## Reuse/root-cause audit

The new `AcceptedAuthority` value owns meaningful operation authority and uses existing Principal/PermissionSet serialization; it earns existence because deferred execution previously had no durable effective-authority snapshot. `Caller.executed_by` preserves the original effective authorizer while distinguishing the executor, rather than adding another permission algebra. `BackgroundAuthority` calls existing issuer recovery on the queue transaction rather than duplicating grant resolution. `ServerWyrdGatewayCaller` remains the common gateway pipeline. Scribe batching remains content-derived and single-outbox; peer transport changes carry the same pair. Telemetry extends the existing route registry rather than creating another export service. No broader redesign is required to close the findings.

The two observable implementation gaps are missing use of existing mechanisms: Workflow's public subject header and the unresolved-audit builder/outbox. Their correction must occur at the producer/owning boundary, preserving Card authorization and refusal response behavior. Required proof gaps must not be corrected by expanding production product capabilities.

## Limits and disposition

No implementation edits or runtime checks were made. No optional improvements, unrelated debt, naming-only concerns, or extra feature requests are proposed. Missing trusted request context is largely prevented by the required concrete Caller/AcceptedAuthority types; no speculative generic context framework is requested. Multi-hop issuance already has supporting auth tests, but that is not the approved end-to-end further-delegation proof. The local Workflow and authentication-refusal gaps are observed from complete reachable source paths, not inferred only from missing tests.

Prior task findings: none (first review of this task). Prerequisite review records are not findings for this task.

Skipped runtime reruns; reported green selectors still leave the specific required branches in BEH-003 unproven. Candidate source was left immutable.
