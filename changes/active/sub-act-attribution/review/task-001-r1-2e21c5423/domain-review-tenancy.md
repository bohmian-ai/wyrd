# Tenancy domain review — TASK-001 / R1

**Result: PASS. No material tenancy findings proposed.**

## Immutable subject and boundary

Reviewed the cumulative `b22718f2cf5da0ebcb7f26bc376157e5eec7768f` →
`2e21c5423036460d5da2ca22a51efde63635e352` candidate, using the R1 locator
diff only to locate remediation. HEAD still matched the candidate at the end
of inspection; the only working-tree additions were this review directory.
`git diff --check b22718f2c 2e21c5423` passed. No production source was changed.

The reviewed boundary is authenticated tenant identity through SQL, accepted
run authority, owner/SYSTEM resolution, creator provenance, Bifrost ingress,
internal capture forwarding, audit storage, and server-span routing. Attribution
must neither select a foreign tenant nor replace accepted authority. This is a
domain result, not a verdict on every task acceptance criterion.

## Authority

Applied `AGENTS.md` §§2–3, 9–12; `architecture/agent-rules.md` SQL capability,
RLS, transaction ownership, audit and tenant-boundary rules; approved spec r6
REQ-001/002/003/007/008/009/010/011 and INV-001 through INV-007; the original
TASK-001 and R1 obligations. Spec approval records explicitly settle platform
operator versus callerless sentinel creation in r5, and local registered
Workflow Card scope in r6. These supersede the earlier unresolved mappings.

Architecture consulted: `wyrd-design.md` runtime identity, verification,
Workflow, audit and Bifrost contracts; `wyrd-doctrine.mdx`; `bifrost-design.md`
managed rows, physical tenant binding, built-in provisioning and private
capture boundary; `wyrd-security-posture.md` platform/tenant separation,
SYSTEM, accepted authority, peer trust and audit integrity. Reference routing:
`references/README.md`, `languages/spec-driven-development.md`,
`maintainer-style.md`, `testing-workflows.md`,
`doctrine/architecture-constraints.md`, `domain/olap-serving.md`, and
`domain/telemetry-observations.md`.

## Source coverage and acceptance evidence

| Boundary / obligation | Source evidence and assessment | Proof inspected | Result |
|---|---|---|---|
| SQL capability types and unchanged RLS | Inspected actual production signatures/fields, not only check output: `BackgroundAuthority::resolve` accepts `&mut TenantConn`; queue tenant operations use `TenantConn`; scheduler and runner retain `WyrdPostgres` for tenant transactions and `OperatorPool` only for discovery/depth; `TenantProvisioning` retains `OperatorPool` for platform lifecycle and borrows an owned tenant transaction for establishment; `BuiltinTables::reconcile` lists through `OperatorPool` and ensures each tenant separately. Card registration/auth projection, service-account, operator-connection and OLAP writes remain tenant-connection operations. The cumulative additions introduce no raw pool/connection/transaction production signature. Existing raw pools found in inspected source are fixture/assertion or cfg-test paths, not a new production escape. | Source plus cumulative diff; reported `check:tenant-isolation` and `check:deps` are supporting evidence only. | PASS |
| Owner and SYSTEM resolve in the run tenant | `verification/authority.rs:232–259` resolves active SYSTEM via `system_principal_id(conn)` and the owner via existing `TenantTokenIssuer::recover(conn, owner)`, on the same accepting transaction. Issuer recovery reuses current tenant admission, active-principal, role and Card-scope resolution (`wyrd-auth/src/issuance.rs:535–631`); `service_account_by_id` excludes SYSTEM. Observation binding selection is under RLS and matches the owner Card; absent owner has no recovery fallback. Scheduler uses the same resolver before `schedule_due`. The modified run schema retains forced RLS and tenant-qualified Card/binding FKs. | `eval_verification::background_owner_subject_system_actor` asserts suspended C, deleted D and a principal moved to a foreign tenant E accept no run, log each rejected owner, and leave healthy A/B independent; removing SYSTEM then rejects all five owners and leaves total runs at two. `scheduled_owner_authority_is_retained` covers actual scheduled owner authority and result/read attribution. | PASS |
| Accepted authority stays bound to the accepted operation | `VerificationService` opens the verified Caller's tenant transaction and captures its effective principal/history with `AcceptedAuthority::new`; internal background producers resolve on the same transaction. `RunAuthority`, insert/observation SQL and `ClaimedRunRow::into_claimed` persist and reload that authority without re-resolving full subject grants. Queue selection and replay remain RLS-bound. `Caller::accepted` takes tenant from the persisted principal and preserves permissions/history; the Eval judge and gateway use this accepted principal, while SYSTEM changes only recorded actor. Run views omit the snapshot. No untrusted payload/header constructs it. A new defensive validator for arbitrary forged internal snapshots is not required to repair a reachable producer path. | `queued_subject_authority_survives_restart` checks stored authority over restart/retry and grant changes; `reloaded_authority_cannot_widen_gateway_access` accepts delegated work, grants B later permission, proves initiating bearer expiry, claims the stored run, refuses a real gateway call before provider traffic, retains A/SYSTEM with [B], and proves B's fresh token can use the new grant. Multi-hop realtime proof preserves the earlier narrowing. | PASS |
| Explicit creator exceptions preserve tenant storage | SYSTEM's defining SQL now takes `creator_sub/creator_act` and inserts them once, retaining them on conflict. `queries/auth/service_accounts.rs:257–265` forwards the typed pair through `TenantConn`. `TenantProvisioning::provision` → `establish_and_promote` → `establish_tenant_administration` forwards the authenticated platform pair to SYSTEM, administrator/key and built-ins. `BuiltinTables::ensure_tenant` → existing `BifrostCatalog::ensure_builtin` → `create_table_locked` → OLAP upsert retains the pair; upsert updates omit creator columns. Startup reconciliation, reserved audit-owner tables, defining SQL backfill and fixture SYSTEM seeding explicitly use the existing unresolved sentinel. No tenant-principal lookup rejects the approved platform/sentinel exceptions or turns them into authorization. | `platform_admin_e2e::new_and_existing_tenants_receive_every_builtin` checks operator creator pairs for newly provisioned SYSTEM/all built-ins, sentinel pairs for startup reconciliation/reserved audit owner, and immutable built-in creator after restart. `pg_migration::pg_tests::system_creation_uses_explicit_creator_or_sentinel` checks defining backfill, fixture sentinel, explicit distinct creator IDs and same SYSTEM ID/first pair on retry. | PASS |
| Registration and replay isolation | Card operation lookups/unique keys and manual-run replay keys contain subject and actor within the RLS tenant. Creator updates remain insert-only in Cards, service-account projection, operator connections and OLAP tables. The pair is derived from Caller, not request JSON. Existing Workflow ownership/idempotency is unchanged. | `pg_cards_register::subject_actor_creation_and_replay`, `pg_verifier_runs::subject_actor_manual_replay`; existing queue tenant-isolation test remains applicable. | PASS |
| Public Bifrost ingress and sampling | Gate HTTP/gRPC paths derive `AuthContext` from `VerifiedToken`, derive its pair from that principal/history, and populate both `principal` and `authenticated_tenant` on the ingress frame. `validate_logical_transport_frame` retains principal/tenant equality and the reserved-owner audit exception before physical binding/materialization. Managed field declaration/reservation and native/canonical materialization stamp trusted pairs; card scope remains independently checked. Claim sampling uses subject. Attribution adds no tenant selector and does not change Variant policy. | Existing/new SDK managed-row/reserved-column journeys, gRPC ingest updates and source-inspected Scribe reserved-owner tests. | PASS |
| Trusted internal forwarding and audit payload/storage split | `ScribeBatch::into_frame` retains one explicit tenant, pair and closed destination. Audit transport uses the reserved sentinel while audit row content retains the event's platform/tenant subject and actor. `ScribeCapturePeerGrpc::batch` admits only the six internal destinations, parses the mandatory pair and refuses reserved-owner non-audit writes. Its listener remains on the authenticated private peer plane. Content-derived batch identity includes tenant and both principals. A trusted cluster process can submit internal attribution, as before; it is not a public identity assertion. | Capture peer closed-inventory/reserved-owner/malformed-ID unit test; Scribe outbox frame grouping/content identity tests; retained-audit journey. | PASS |
| Narrow input-read authority and attribution history | `SystemReadAuthority::resolve` constructs only existing table-UID-scoped SYSTEM read grants using same-tenant catalog resolution (including the no-local-catalog TenantConn path). `serving(&AcceptedAuthority)` now supplies subject and accepted history through the existing query context. Drift and Eval both call this producer. `AuthorizedQueryContext::try_new` retains tenant/principal equality; Oracle authorization still uses SYSTEM's narrow grants, while audit projects served subject/SYSTEM. Serialized peer context retains history; Eval media still validates the run tenant's storage prefix. | `delegated_input_reads_keep_history` checks retained Drift read audit A/SYSTEM/[B]; shared `serving_keeps_accepted_history_without_widening` checks grants unchanged and serialized history; existing remote-Oracle runtime proof covers the forwarding owner. | PASS |
| Local/hosted Workflow Card and tenant scope | Registered local loading retains the resolved root UID and passes it through existing `PublicWyrdGatewayCaller::with_subject(None, card_uid)`. Public ingress treats this as Requested correlation: `GatewayInvocation::attribute` checks the signed Card scope, or uses a tenant registry transaction for an unbound principal. Hosted/Verifier Resolved contexts remain trusted server-owned paths; their producer resolves Cards in the tenant. No foreign UID grants authority. | Updated Rust loaded-Workflow journey checks direct/delegated pair, exact Card UID/null Run and out-of-scope refusal before provider traffic; existing hosted proof retained; Python/TS use the shared owner. | PASS |
| Trace identity never selects the tenant or actor | Gate/Caller route binding uses verified principal and pair. `TenantRoutes::start` follows local server-generated span IDs; remote ancestry only enables a new unbound route. `bind` supplies authenticated request identity. `ServerWyrdGatewayCaller::invoke` rebinds the emitting operation to its captured Caller pair without changing sibling request routes. `ServerSpanSink` stages the route's explicit tenant/pair on the existing outbox. | Rust/Python/TS timelines and MCP delegated span assertions; source review of `TenantRoutes::{start,bind,rebind,end}` and gateway operation-route binding. | PASS |

## Prior-finding closure within this domain

- **FIND-TASK-001-1 (prior TEN-001): closed.** SYSTEM no longer self-stamps;
  verified provisioning and callerless bootstrap follow r5 through the existing
  owners, with immutable first pair. Both SQL and real provisioning tests check
  stored values, rather than relying on transport attribution.
- **FIND-TASK-001-4: closed for tenancy.** Missing/inactive/foreign owners and
  absent SYSTEM accept no observation run, each binding refusal is observable,
  and healthy siblings remain independent. No writer/SYSTEM fallback was added.
- **FIND-TASK-001-6: closed for the shared read-context boundary.** Drift and
  Eval reuse the changed `serving` producer and retain table grants; history is
  propagated into the serialized query context and retained read audit.
- **FIND-TASK-001-7: relevant proof gaps closed.** Actual scheduled execution,
  missing/foreign owner, missing SYSTEM, nested delegation narrowing, accepted
  snapshot gateway refusal after bearer expiry, and source-inspected retry/
  restart proof cover the required tenancy/authority seams.
- **FIND-TASK-001-2:** r6's local Card rule is implemented using the unchanged
  gateway Card/tenant owner. No tenancy widening was found.

Other prior findings' full non-tenancy obligations belong to their respective
independent reviewers; this report does not claim to validate their entire
closure.

## Verification limits

No Cargo, SQL, SDK, or server runtime commands were rerun by this reviewer,
per coordination instructions. Runtime PASS claims above distinguish
source-inspected assertions from the R1/original task's recorded green results;
this review independently ran only immutable-subject/status/diff checks and
source navigation. Shared unit serialization proof is narrower than a live
delegated Eval read across two peers; the shared producer and existing peer
reader path were inspected rather than claiming that exact combined scenario
was executed. Arbitrary database tampering and compromised authenticated cluster
members are not added threat models. No speculative hardening or additional
mechanism is proposed.

## Proposed findings

None. **PASS** for the reviewed tenancy boundary.
