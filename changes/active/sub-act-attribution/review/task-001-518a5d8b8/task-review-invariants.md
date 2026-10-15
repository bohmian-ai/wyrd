# Independent invariant review — TASK-001

Result: **FAIL**.

## Subject and method

Reviewed `b22718f2cf5da0ebcb7f26bc376157e5eec7768f..518a5d8b822a4e649a5fe90b14f3eb2b1fc8a0aa`, approved `changes/active/sub-act-attribution/spec.md` revision 4, and the original task with its candidate execution evidence. HEAD remained the candidate. Only this report was written. This is independent discovery; no other reviewer's findings were consumed.

CodeGraph was used first for request authority, provisioning, and Workflow owners, followed by the complete changed-path inventory, cumulative diffs and surrounding producer/consumer source. Authorities consulted: AGENTS.md, agent-rules, spec-driven-development, maintainer-style, and applicable attribution, runtime, ingestion, storage, identity and audit sections of wyrd-design, doctrine, bifrost-design and security-posture. Tests below are source-inspected and implementation-reported passes, not reruns by this reviewer.

## Value/state trace and reuse assessment

| Boundary | Producer → retained state → consumer | Existing owner/reuse assessment |
|---|---|---|
| Delegated direct request | VerifiedToken → AuthenticatedPrincipal → Caller principal/history → executor with `executed_by` SYSTEM → ServerWyrdGatewayCaller → GatewayInvocation | Effective Principal remains the authorization input; last flattened actor supplies request attribution; SYSTEM only replaces recorded execution actor. Existing permission evaluator and gateway admission/accounting are reused. No replacement executor or delegation engine found. |
| Durable run | AcceptedAuthority strips credential id → RunAuthority → verifier_runs authority JSON and pair → ClaimedRun → EvalEngine → Caller::accepted → gateway | Existing queue, lease, retry and Principal serialization extended. Authority is not refreshed from subject grants on claim. Public run view omits snapshot. New values retain actual invariants rather than creating a parallel store. Missing required execution proof is IV-005. |
| Background acceptance | locked binding owner → BackgroundAuthority::resolve on tenant transaction → TenantTokenIssuer::recover → RunAuthority owner/SYSTEM → shared enqueue | Existing recover and tenant-bound queue mechanisms reused; observation writer no longer supplies authority. Scheduler preserves occurrence locks/dedup. Missing-owner observation path silently disappears (IV-004). |
| Creation/replay | Caller::attribution → existing registration/auth/connection/catalog query owners → immutable inserted pair | Query updates preserve creator pair; registration and manual replay locks/lookups/uniqueness include sub and act. Provisioning exceptions violate the universal invariant (IV-002/003). |
| Managed ingest | AuthContext pair → ScribeIngressFrame → decode/native preprocess → managed arrays → WAL/storage → query | HTTP/gRPC Gate and trusted internal frames carry attribution separately from principal authorizing Card/tenant. Existing policy controls stamping; Variant payload remains content. Subject supplies sampling stratum. |
| Audit/capture/results | audit event/capture/result owner → ScribeWrite → grouped Frame keyed by pair → content-derived batch id → local/peer ScribeBatch | Existing nonblocking outbox, peer transport, retry fencing and loss accounting retained. Audit policy None retains payload pair rather than overwriting with transport identity. |
| Server span | trusted request Caller pair → TenantSpanRoute; gateway operation rebind → bounded ServerSpanSink → same outbox | Extends current route owner; request and sibling spans retain request pair, gateway subtree changes to executor pair. Trace context remains correlation, not attribution authority. |
| Workflow | hosted Preparation captures Workflow UID → resolved subject → gateway; local WorkflowCards load → SelectedRoutes dependencies → uncorrelated public caller | Hosted path reuses gateway correlation correctly. Local path omits the existing `with_subject` mechanism despite knowing registered Workflow identity (IV-001). |
| Public projections | wyrd-spec fields/managed constants → shared client → Rust/Python/TS queries and typed rows → docs/examples | Renaming is coordinated; identity-only keys remain deliberately. No compatibility alias or new dependency/feature found in cumulative diff. |

## Obligation matrix

PASS means the inspected implementation and available scoped evidence support the obligation. FAIL refers to findings below; it is not a claim that every subcase of that obligation is wrong.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 non-null server pair | managed_arrays; SQL defining tables; provisioning writes | V4/V5/V9 and SQL reported green; wrong provenance in IV-002/003 | FAIL |
| REQ-002 identity by work role | Caller::attribution; direct service; EvalEngine; results; hosted Workflow | V1/V2/V6/V8; local Workflow and provisioning exceptions | FAIL |
| REQ-003 authorization/cost | effective Caller principal retained; GatewayInvocation and ledger use subject principal | V1 both asymmetric denials; V2 owner denial | PASS |
| REQ-004 Card roles/Workflow Card | result builders/schemas rename verified Card vs Verifier; host.rs attributes UID | hosted Workflow proof exists; local V8 omits assertion (IV-001) | FAIL |
| REQ-005 schema inventory | SQL declarations, managed_columns, result/audit/gateway tables, query projections | SQL and codegen reported green; non-null structure inspected | PASS |
| REQ-006 wire/SDK/reserved columns | wyrd-spec auth/verification/vala contracts, queue reservation, SDK reads | V5/V9 each SDK and codegen reported green | PASS |
| REQ-007 active SYSTEM | production provisioning and PgFixture seed call existing provisioning owner | V2 checks seed before fixture provisioning | PASS |
| REQ-008 deferred authority | AcceptedAuthority → stored JSON → ClaimedRun → Caller::accepted | V3 only scripted Drift; missing-owner failure silent, IV-004/005 | FAIL |
| REQ-009 idempotency | registration operation scope and queue lock/key SQL carry pair | V4 pair isolation/replay; existing background/Workflow keys retained | PASS |
| REQ-010 audit identity/storage | audit payload independent of Scribe transport, unresolved sentinel replaces nil | V6/platform/MCP reported green; sentinel extended outside approved audit meaning (IV-003) | FAIL |
| REQ-011 propagation/further delegation | original effective Principal retained through direct and queued gateway callers | V1 one-hop only; further delegation not proven (IV-005) | FAIL |
| INV-001 trusted sources only | Gate/Caller use verified sources; built-in registration creates unresolved pair without approved audit exception | IV-003 source | FAIL |
| INV-002 SYSTEM never sub | ordinary execution uses SYSTEM actor only; SYSTEM creation inserts its own id as sub | IV-002 source | FAIL |
| INV-003 tenant isolation | TenantConn/RLS binding; owner recover and SYSTEM lookup on same tenant transaction | SQL/tenant check reported; cross-tenant background journey absent (IV-005) | FAIL |
| INV-004 origin meanings | run, verified Card, Verifier Card and binding/source fields remain separate | result row and timeline journeys | PASS |
| INV-005 traces not authority | explicit trusted route binding and gateway rebind; no trace-derived pair | telemetry subtree unit; V7/V10/V11 | PASS |
| INV-006 pair is not permission | gateway authorizes retained Principal; SYSTEM reads retain narrow table grants | V1/V2 negative invoke and source inspection | PASS |
| INV-007 immutable capture | no claim/retry snapshot replacement; creator updates exclude pair | V3 stored JSON equality, V4; actual queued judge not proven (IV-005) | FAIL |
| AC-001 scheduled owner/SYSTEM journey | scheduler resolves/captures owner; runner writes owner/SYSTEM | V2 exercises observations, not scheduled execution; SQL scheduler tests are supporting (IV-005) | FAIL |
| AC-002 observation owner vs writer | observation binding capture and EvalEngine accepted caller | V2 owner A/B judgments and differing observation writer | PASS |
| AC-003 direct rows/audit across SDKs | direct Caller and typed projections | V1/V5/V6/V7/V9/V10 | PASS |
| AC-004 delegated rows/audit | delegation pair uses last chain entry | V1/V5/V6 | PASS |
| AC-005 reject reserved pair | queue row preflight and Gate/Scribe reservation | V5/V9 Rust/Python/TS | PASS |
| AC-006 remove stale work names | complete changed-path inventory and task source-backed disposition; retained identity locals distinct | codegen and disposition reported | PASS |
| AC-007 no new migration/fresh DB | requester migration deleted; defining tables changed | test:sql reported 203 passes; no added migration path | PASS |
| AC-008 seeded active SYSTEM | PgFixture seed invokes production owner | V2 and fixture integration assertion | PASS |
| AC-009 complete relevant proof | focused suites and static lanes reported | Required scenarios missing in IV-005 | FAIL |
| AC-010 request/judge spans | route binding and operation rebind | V7/V10 direct pairs; V11 delegated request and judge pairs | PASS |
| AC-011 Workflow Card/pair/null run | hosted host.rs correct; local SelectedRoutes creates caller with no subject | IV-001; local V8 only checks pair | FAIL |
| AC-012 actor-denied realtime judge | executor retains originating intersection | V1 C-for-A denied before provider | PASS |
| AC-013 queued authority through actual judge | production Eval consumes stored authority correctly | V3 replaces real engine with scripted Drift, never invokes judge (IV-005) | FAIL |
| AC-014 replay isolation | pair-scoped keys, locks, lookup; creator immutability | V4 and retained existing dedup tests | PASS |
| AC-015 platform/unresolved audit | real platform payload pair; explicit unresolved pair | V6 and platform/JWT/MCP focused evidence | PASS |
| AC-016 all writers/projections | managed paths and content audit inspected | SDK ingest proof; provisioning attribution exceptions IV-002/003 | FAIL |
| AC-017 missing identities/retry/input reads | tenant recover/narrow input scope; missing-owner observation silently skipped | IV-004; missing SYSTEM/foreign owner journeys absent IV-005 | FAIL |
| AC-018 every hop/further delegation | direct/gateway authority path retains original effective Principal | V1 confirms one level only, no further-delegation case IV-005 | FAIL |
| Non-goals: no new evaluator/token engine/store/queue | existing issuer recover/evaluator, existing run queue and outbox used | cumulative production diff | PASS |
| Non-goals: preserve Variant, operational lineage, Operator rechecks, Workflow lifetime | policy-aware projection retains Variant boundary; dispatch/run ownership unchanged | source plus existing owner evidence | PASS |
| Constraint: approved authority cannot be redefined by implementation | Attribution::unresolved rustdoc invents platform-internal exception | IV-003 | FAIL |

## Proposed findings

### IV-001 — MISSING: local registered Workflow calls lose the Workflow Card

- Obligations: REQ-002, REQ-004, AC-011; task reuse map and Scenario 7/V8 cover loaded Workflow consumers without a hosted-only exception.
- Locations: `crates/shared/wyrd-client/src/workflow/local.rs:103–107`; `workflow/mod.rs:499–508`; changed acceptance test `sdks/wyrd-sdk-rust/tests/integration/gateway_inference.rs:615–630` and new hosted-only test below it.
- Producer-to-consumer evidence: WorkflowCards::load has the exact registered Workflow and hydrates it, but run preparation only passes a WyrdClient to SelectedRoutes::dependencies. That method constructs `PublicWyrdGatewayCaller::new(client)` with no `with_subject`, so gateway.rs sends no Card header and GatewayInvocation captures null card_uid. The local V8 now executes direct and delegated calls but asserts only pairs. Hosted Preparation supplies a resolved Workflow UID and its new test checks Card/null run; this does not fix the local path.
- Reachable consequence: a registered Workflow loaded and executed through any SDK records A/A or A/B but cannot join its gateway call to its Workflow Card, contrary to the approved AC. This is an admitted implementation limit, not an approved scope exclusion.
- Smallest correction: retain the known registered Workflow UID through the existing local Workflow owner and use existing `PublicWyrdGatewayCaller::with_subject(None, Some(uid))`; preserve existing server Card authorization and the unchanged null application run. Do not create another gateway transport or loosen Card checks. The existing `CardKind::is_observation_target` includes Workflow (`wyrd-spec/src/envelope.rs:627`), so the unbound administrator used by this journey is already eligible for registered Workflow correlation; a new permission model is unnecessary.
- Siblings: authored/native builder Workflows may lack a registered UID and remain outside this exact known-registered-identity failure. Rust/Python/TS share this client owner; fix once there. Hosted path remains correct.
- Focused closure: extend existing local V8 to assert the same Workflow UID and null run for both direct/delegated calls; rerun that exact test and the hosted test, not a full gateway suite.

### IV-002 — INCORRECT: SYSTEM provisioning creates SYSTEM as subject

- Obligations: REQ-001/002, INV-002, AC-016.
- Location: `crates/wyrd/wyrd-sql/migrations/20260601000028_system_principal.sql:74–79`.
- Evidence: the defining provisioning function inserts `candidate` as the SYSTEM principal id and also as both `sub_principal_id` and `act_principal_id`. This is a real auth_service_accounts creation row, explicitly within the inventory. Production TenantProvisioning::establish_tenant_administration calls the wrapper, as do fixture seeding and test server provisioning. Every provisioned tenant therefore has a SYSTEM-sub work row. The approved rule has no self-creation exception.
- Consequence: creator attribution is false and contradicts the universal SYSTEM actor-only invariant, even though later Verifier rows are correct.
- Correction boundary: existing SQL provisioning owner and callers. Production already has the verified platform caller pair in establish_tenant_administration; preserve that creator context rather than assigning the created principal as its own subject. Keep idempotent creation and immutable stored creator attribution. Trace fixture/bootstrap callers as well; do not substitute the audit sentinel or invent a bootstrap identity exception without approval. If a caller genuinely has no approved creator source, resolve that conflict through spec revision rather than silently picking a new identity convention.
- Focused closure: provisioning integration reads the SYSTEM row and proves creator pair is the trusted provisioning pair, never SYSTEM sub, and a repeated provision does not change it.

### IV-003 — DRIFT: built-in registration extends the unresolved-audit sentinel into ordinary creation attribution

- Obligations: INV-001, REQ-002/005/010, task material stop conditions.
- Locations: `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:926–932`; `crates/wyrd-spec/src/auth/attribution.rs:37–43`.
- Evidence: ensure_builtin unconditionally supplies Attribution::unresolved to CreateTableRequest; upsert_table persists it on vala.bifrost_tables. New rustdoc explicitly broadens unresolved from an unauthenticated audit marker to platform-internal operations. Approved INV-001 allows only verified identity, binding owner, or the explicit unresolved-*audit* sentinel, and REQ-010 limits the lookup exception to platform/sentinel audit records. The spec does not approve the broadened convention.
- Reachability/siblings: production tenant provisioning (`platform/provisioning.rs:287–290`) and startup reconciliation (`platform/builtins.rs:103–106`) both call the same ensure path; reserved platform audit table provisioning also uses it. All normal built-in table creation rows therefore carry a principal that is not the registering caller or a tenant creator. Public dataset registration correctly supplies caller.attribution and must remain unchanged.
- Consequence: the inventory's table-creator provenance is replaced with an unavailable-identity marker even when a verified provisioning caller exists; startup-created rows use an unapproved attribution convention.
- Decision-complete boundary: do not solve this by adding another sentinel or synthesizing SYSTEM sub. For caller-caused provisioning, extend the existing BuiltinTables/catalog path to carry the existing verified provisioning pair. For autonomous startup creation, the current approved spec does not choose an admissible creator subject: route that narrow missing decision through spec revision if no approved existing context can be reused. The implementation cannot approve the exception by rustdoc. Preserve existing eager reconciliation, idempotency and creator immutability.
- Focused closure: inspect the persisted catalog creator pair after tenant provisioning and after restart/reconcile creates a missing built-in, under the explicitly approved autonomous-creation rule. No new harness or audit mechanism.

### IV-004 — MISSING: absent observation-binding owner is silently discarded

- Obligations: REQ-008, AC-017: missing owner prevents acceptance with an observable binding execution failure.
- Location: `crates/wyrd/wyrd-server/src/verification/observations.rs:116–119`.
- Evidence: lock_observation_bindings uses a nullable subquery for owner principal; when no matching principal exists it returns an ObservationBinding with owner None. ObservationRunSink immediately `continue`s without log, count, or binding outcome, then commits successfully. BackgroundAuthority logs missing SYSTEM/inactive owners, but this branch never calls it. The observation outbox consumes the batch, so there is no run/status from which a caller can discover why verification vanished.
- Consequence: a matched binding with a missing principal loses its trigger silently, violating the approved observable refusal. This finding does not request execution under fallback authority.
- Smallest correction: at the existing observation acceptance owner, emit the existing structured failure/skip diagnostic keyed by binding and tenant before consuming the refusal. Retain successful processing of sibling bindings; do not fail the shared server, add a queue/store, or retry permanently missing identity forever. Consolidate all equivalent owner-resolution refusal diagnostics there as needed rather than adding guards to Eval or gateway.
- Focused closure: an existing observation journey removes or omits the matching owner principal, proves no run/fallback/provider call and proves the binding-specific observable failure, while another valid binding still executes. Use the existing fixture/log capture facilities.

### IV-005 — MISSING: required propagation and recovery scenarios are not actually exercised

- Obligations: AC-001/013/017/018, task Scenarios 1–3 and AGENTS §11.
- Locations: `eval_verification.rs:2818–3000` and `3103–3259`; `verification_runtime.rs:2813–2996`; task Implementation Evidence AC-001/013/017/018 rows and Limits.
- Evidence: V1 creates direct and single-hop B-for-A/C-for-A clients only; no second delegated actor is introduced. V2 creates observation-triggered runs and a suspended owner case, not a scheduled execution, missing SYSTEM, missing owner, or cross-tenant owner case. V3 enqueues a Drift window, then injects EngineScript Retry/Completed reports; neither worker reaches EvalEngine or a gateway judge. Its JSON equality checks prove persistence, not that the reloaded narrowed authority is used at the protected consumer. It does not exercise token expiry. SQL scheduler/claim tests are useful supporting seams but not the requested real execution journeys.
- Consequence: the green named selectors can pass if further delegation regains a permission, if a restarted queued judge reconstructs full subject authority, or if scheduled execution ignores its stored owner authority. The current completion matrix overclaims these explicit requirements. This is a bounded missing-proof finding, not a claim that such permission widening was observed in production source.
- Minimal correction: extend existing V1 with a second delegated actor that has a permission excluded earlier and prove gateway refusal/no provider request; extend existing V2/nearest scheduled journey with an actual scheduled owner/SYSTEM execution and required identity rejection cases; extend V3/nearest real queued Eval journey to reload a stored delegated authority into the actual judge/gateway after retry/restart and grant changes, proving denied model invocation stays denied and retained history/pair are correct. Include the specified accepted-token expiry case using existing test controls. Keep existing SQL snapshot round-trip assertions as supporting proof, not a substitute. Reuse fixtures and production engines; no new executor/harness/dependency. Run only the affected exact tests and narrow write-set lanes during remediation.

## Verification and limits

The recorded SQL, static, SDK and focused test passes are credible reports but do not close the source defects or missing cases above. No runtime tests, compilation or broad suites were run during this read-only pass. No implementation change, optional refactor, new mechanism or unrelated debt is requested.

Final discovery result: **FAIL**. IV-001/002/004 are bounded implementation gaps; IV-003 contains an unresolved autonomous-provisioning attribution decision that must not be prescribed as a new convention by remediation. Independent validation should determine whether an already-approved source resolves that case or the final verdict requires specification revision.
