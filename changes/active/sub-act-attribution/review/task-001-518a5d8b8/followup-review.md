# Focused follow-up review

Result: **RESOLVED**. The source distinctions are resolved. One persistent-attribution decision remains absent from approved authority; that is a specification decision, not missing review evidence. The general authentication-refusal gap is a bounded implementation/proof gap. No required reachable scheduled-query object-denial failure was established.

## Subject and scope

- Base: `b22718f2cf5da0ebcb7f26bc376157e5eec7768f`.
- Candidate: `518a5d8b822a4e649a5fe90b14f3eb2b1fc8a0aa`; HEAD remained this value.
- Approved spec: `changes/active/sub-act-attribution/spec.md`, revision 4.
- Original task: `changes/active/sub-act-attribution/tasks/TASK-001-sub-act-attribution.md`.
- Reviewed only the three uncertainties assigned by the orchestrator. Read the cumulative changed-path inventory, applicable cumulative diffs and actual owners/callers, the cited discovery claims, spec/task, and applicable agent, specification, maintainer, identity, security, catalog and audit authority. CodeGraph preceded targeted source searches. This is fresh follow-up discovery, not validation of the complete finding union.

## 1. SYSTEM and built-in creators

**Resolved source conclusion:** authenticated tenant provisioning already has usable creator context; autonomous startup/migration/fixture creation does not have an approved universal subject source. These must not be conflated.

`TenantProvisioning::provision` passes `Attribution::direct(caller.principal_id())` into `establish_and_promote` (`components/platform/provisioning.rs:226–231`). `establish_tenant_administration` receives that value (`:447–452`) and uses it for its administrator and credential rows (`:491`, `:514`), but calls `provision_system_principal(&mut conn)` without it (`:456`). The defining SQL owner writes the newly minted SYSTEM id as both creator columns (`20260601000028_system_principal.sql:74–79`). `establish_and_promote` subsequently calls `BuiltinTables::ensure_tenant` without the already available attribution (`provisioning.rs:287–290`); the catalog supplies `Attribution::unresolved()` (`bifrost_catalog.rs:926–932`). Thus production caller-caused creation demonstrably drops an available verified source. Extending these existing owners is sufficient for this caller; no new identity or registration owner is necessary.

That correction does not settle all actual callers:

| Actual caller | Available context | Why no approved universal creator can be inferred |
|---|---|---|
| Startup `boot/mod.rs:499–505` → `BuiltinTables::reconcile` (`builtins.rs:103–106`) → `ensure_tenant` | Operator database capability, active tenant IDs, catalog | No authenticated Wyrd caller or accepted operation snapshot is passed. Reconciliation creates newly introduced built-ins for old active tenants. It includes `SYSTEM_OWNER` for its audit table (`builtins.rs:72–84`). A database login is not a Wyrd PrincipalId. |
| Defining SYSTEM migration backfill (`20260601000028_system_principal.sql:98–109`) | Tenant ID bound transaction-locally | No creator ID or delegation context. Picking an existing tenant administrator would be attribution by convention, not retained verified causality. |
| `PgFixture::start_seeded`/additional tenant methods → `seed_tenant` (`pg.rs:667–685`) | Fixture database capability and new tenant ID | Inserts tenant then provisions SYSTEM before a tenant user/admin is seeded. There is no verified subject to forward. |
| First startup of reserved audit owner | Seeded `wyrd-system` tenant from `20260601000015_seed_system_tenant.sql` | The reserved tenant is a storage owner, not an ordinary principal. It deliberately gets no tenant SYSTEM writer. Initialization of `PLATFORM_ROOT_NAME` is a separate operator command (`boot/init.rs:118–160`), so no root principal is guaranteed during table creation. |

Existing platform-root initialization demonstrates an operator boundary, but does not define the root as the creator of all later callerless work. Looking it up at startup would select a new persistent identity convention, would not cover pre-initialization audit-table creation, and could change boot availability. Tenant admin lookup likewise does not cover the reserved audit tenant or pre-admin fixture provisioning. Neither can be prescribed as an already approved correction.

Approved REQ-001/005 includes these creation rows and requires non-null immutable attribution. INV-001 permits verified identity, binding owner, or the REQ-010 unresolved-*audit* sentinel; INV-002 prohibits SYSTEM as subject. REQ-010's tenant-lookup exception is for platform/sentinel **audit records**, not ordinary built-in catalog rows. `Attribution::unresolved` rustdoc (`attribution.rs:37–43`) and `ensure_builtin` rustdoc (`bifrost_catalog.rs:895–896`) broaden the sentinel to ordinary autonomous creation in this implementation; they cannot approve their own new persistent convention. No stronger inspected identity authority defines another admissible creator for all the rows above.

**Decision boundary:** DATA-001/002, IV-002/003 and TEN-001 share missing creator propagation/convention at existing provisioning owners. The authenticated part has a bounded correction: retain existing verified provisioning attribution and immutable creation values. The autonomous part requires a narrow approved specification decision selecting what subject/actor represents creation without a verified caller, including reserved-audit-owner built-ins, defining migrations, and test fixtures. It is not safe to prescribe a new sentinel, SYSTEM-sub exception, platform-root convention, removal of eager startup reconciliation, or new boot prerequisite as task remediation. This supports `SPEC_REVISION_REQUIRED` if retained after independent validation. It does not require a general identity architecture redesign.

Focused proof after that decision: the existing provisioning integration reads SYSTEM/catalog creator pairs and checks trusted caller causality and unchanged pairs on replay; the existing startup/fixture paths check the explicitly approved autonomous rule. No additional identity owner or test harness is needed.

## 2. Existing unresolved audit versus general HTTP authentication refusal

**Resolved distinction:** the security report correctly describes existing unresolved audit producers; it does not establish coverage of general protected HTTP authentication failures.

- `wyrd-auth/src/jwt_bearer.rs:58–85,98–123` catches workload exchange failures. When no principal was resolved, it stages `Attribution::unresolved()` with `AuditDetail::AuthFailure` through the issuer's existing stage. This is an existing exchange event under its independently selected tenant, not an audit of an ordinary Card request.
- `server/src/oracle/peer_audit.rs:68–86` uses `audit_event_unauthenticated` and adds `BifrostSecurityViolation` detail for refused private peers. The unverified-ticket path uses the reserved platform audit storage. Again this is a separate producer.
- Ordinary `/v1` and Bifrost HTTP requests use `require_authenticated`/`require_bifrost_authenticated` → `authenticate_on` (`http/middleware/authenticate.rs:101–116`). That function calls `verify_authenticated_principal` and returns the rejection immediately. Its error branch has no outbox operation. The shared verifier (`components/auth/token_extract.rs:150–178`) returns errors while extracting, parsing and verifying; it has no audit sink. The extractor fallback (`principal_extractor.rs:91–110`) also returns these same errors without staging. Missing credentials, malformed credentials and a forged signature therefore reach no general authentication-refusal producer before the handler's permission check.
- V6 (`tests/bifrost/server/audit_retention.rs:173–241`) actually drives those three HTTP refusals, but explicitly documents they leave no decision, then queries only the four authenticated Card decisions. Its predicate `resource = card:<uid>` does not inspect reserved unresolved history or correlate the refused requests.

This conclusion is **not** derived from an RFC or an assumption that every authentication refusal is an authorization decision. AGENTS' universal permission-decision rule alone would not demand a new event before permission evaluation. The applicable additional approved outcome is AC-015: “Invalid/absent credentials use the existing unavailable-identity sentinel pair, never nil or unverified token claims.” Original task Scenario 6 explicitly says “Authentication refusals use” the pair and drives missing/invalid/misleading credentials through HTTP. These statements make the omitted producer and omitted proof part of this task. REQ-010 alone, worded as “Audit records without a verified identity use”, would only constrain already-existing records; AC-015 plus the original task resolves the scope ambiguity.

**Decision boundary / proposed finding:** retain BEH-002 as a bounded `MISSING` requirement, scoped to the specified authentication refusals rather than manufacturing permission decisions. Reuse existing request correlation, reserved audit storage, unresolved pair, structured failure detail and sole nonblocking ScribeOutbox at the authentication owner; rejected tenant/identity claims must not select the stored subject or tenant. Trace sibling entry points when selecting that boundary to avoid duplicate emission. Preserve original 400/401 behavior and do not require audit persistence before refusing. The pure verifier should not acquire server IO merely to satisfy this. V6 should locate the refused attempts by trusted correlation in retained reserved audit history, while keeping existing authenticated/platform/outbox-failure proof. The security report's sentinel correctness and this finding can both be true.

## 3. ScheduledQueryCaller served-subject conversion

**Resolved source conclusion:** `ScheduledQueryCaller::record_object_denial` drops `AuthorizedQueryContext.served_subject` (`query/scheduled.rs:223–232`) and would construct SYSTEM/SYSTEM for a served read. However, no required real producer of that failure state was established. Do not promote this syntactic possibility into a material finding.

Actual production callers using `ScheduledQueryCaller::new` are:

1. Eval `BifrostReader::new` (`verification/eval.rs:559–587`): `SystemReadAuthority::resolve` grants precisely `eval.observations` and `traces.spans`, then `serving(subject)` adds attribution. Its only query producers are `record` (`:621–650`, fixed observations table) and `spans` (`:704–738`, fixed spans table). Values are escaped or typed, including `quoted` (`:761–763`); callers do not supply arbitrary SQL/table names.
2. Drift `Reader::authority/read` (`verification/drift.rs:882–900,938–952`): grants precisely `drift.observations`. Every aggregate, including `DriftProfile::Custom`, is generated by `ObservationWindow`; “custom” is a metric name, not custom SQL (`:146–161,282–287,715–724`). All reads stay on the granted table. Drift returns no read when that table was absent at authority resolution.

Oracle's actual `QueryForbidden` producers are `authorize_resolved_tables` (`oracle/mod.rs:3464–3482`) and `authorize_payload_columns` (`:3500–3548`). The first checks registered table UID grants. The catalog explicitly makes registrations/UIDs immutable, with no production delete/update path (`catalog/bifrost_catalog.rs:260–275`). The second concerns only `gateway.calls` payload columns, absent from both Verifier SQL producers. Eager built-in provisioning happens before tenant promotion and server activation, so a normally serving Verifier does not resolve an absent built-in and race its first registration. An object-store or catalog dependency error is not translated into QueryForbidden by `OraclePlanner::read_and_materialize` (`oracle/planner.rs:99–154`).

Authenticated scheduled callers use `ScheduledQueryCaller::authenticated`, preserve their original Caller, and dispatch through `stream_query`; they bypass this reconstruction. Normal successful served read decisions use Oracle's `query_audit_event` (`oracle/mod.rs:724–747`), which retains served subject/kind correctly. Other security-violation events use that same context-aware builder.

**Decision boundary:** a test-only deliberately undergranted served context could reach the conversion, but the approved task does not require a new public or arbitrary caller for it. Changing production structure or adding a guard/test solely for that dormant state would be speculative. No new follow-up finding is proposed here. Reassess only if independent validation identifies an existing required caller or lifecycle that actually makes the mismatched table-grant state reachable.

## Verification limits

Source review only; no Cargo, database, SDK runtime, compilation, or broad verification lane was run. Existing reported green checks do not decide the authority gap above. No implementation file was changed; only this report was written.
