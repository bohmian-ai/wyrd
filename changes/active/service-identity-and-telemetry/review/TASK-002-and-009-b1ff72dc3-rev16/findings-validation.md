# Independent revision-16 findings validation

Recommendation: **FIX_REQUIRED**. Six bounded findings remain on unchanged source. **FIND-009-4 is REJECTED under superseding human-approved authority**, not closed by an implementation fix. Revision 16 removes the required state-bound external exporter producer and the associated public API decision. No further specification decision prevents packaging the six bounded corrections below.

Superseding maintainer disposition: FIND-009-1 is limited to committed spec
REQ-012/015, INV-004 and AC-005 binding-occurrence preservation. FIND-009-2 as
written is rejected; its replacement requires unchanged accepted correlation
context on direct/queued inputs/results across restart, never execution input.
No implementation fix or new runtime proof is claimed.

## Subject and method

Cumulative source: `036fc48fe26fb87648a277b10099bc475d293b92...b1ff72dc34c344b596c85cf408e6a9217685be35`, TASK-002/TASK-009. HEAD was checked as the candidate. Applicable approval is [spec revision 16](../../spec.md), particularly REQ-050–053 and AC-026, explicitly approved by the maintainer's two-path direction.

This fresh independent structured Ponytail continuation reuses the completed eight independent discovery reports, focused auth follow-up, full cumulative source audit, changed-item rustdoc inventory, verification, ledger and original verdict in [the prior review](../TASK-002-and-009-b1ff72dc3/subject.md). Those reports remain historical against revision 15; unchanged discovery was not repeated. CodeGraph preceded source searches. I rechecked the unchanged critical seams: direct admission/engine construction, state bound selection, native production mount/refusal versus OTLP optional-root selection, claimed/runtime execution context, extended tenant target reader, and Python/TypeScript exporter authentication. This is an authority-resolution continuation, not a post-fix review or a new runtime verification claim. Only this report was written.

## All seven dispositions

| Finding | Discovery inputs | Status and disposition under revision 16 |
| --- | --- | --- |
| FIND-002-1 | B-001, INV-DIRECT-001, FUP-AUTH-001 | CONFIRMED / INCORRECT: direct verification still omits subject admission. |
| FIND-002-2 | B-002, FUP-AUTH-002 | REVISED / MISSING: User bearer native state IO lacks server-owned optional-root composition; keep fail-closed Gate verification. |
| FIND-002-3 | STD-002009-02 | CONFIRMED / VIOLATION: unchanged changed-item rustdoc inventory remains required. |
| FIND-009-1 | B-003, INV-BINDING-002, DATA-01, MAINT-01, SYS-001 | REVISED / MISSING: exact occurrence is still discarded before acceptance; do not invent activation for explicit-only bindings. |
| FIND-009-2 | B-004, INV-ENV-003, DATA-02, MAINT-02, SYS-002 | REVISED / MISSING: SUPERSEDED: reject execution handoff; require unchanged context on direct/queued inputs/results across restart. |
| FIND-009-3 | STD-002009-01 | REVISED / VIOLATION: extended target read still accepts raw connection plus independent tenant authority; no cross-tenant exploit is claimed. |
| FIND-009-4 | CONC-001 | REJECTED: revision 16 removes required state-bound external exporters. No new exporter API is remediation. |

The other prior reconciliations remain: native refusal is locally safe but does not prove required positive-flow completeness; MCP wording is non-blocking; speculative provider retry defects were rejected because snapshot-before-await exists; security/data reports supply no additional validated findings. Their boundary PASS results do not close these specific consumers.

## Retained bounded ledger

### FIND-002-1 — CONFIRMED / INCORRECT: direct verification omits subject admission

Discovery: B-001, INV-DIRECT-001, FUP-AUTH-001. Obligation: REQ-010/012/033, TASK-002 scenarios 2/3/7, auth supplement Authorization and Direct verification (`:383–405`, `:782–787`).

Exact location: `crates/wyrd/wyrd-server/src/components/verification/service.rs:490`, with `authorize_run:932–973`.

Source trace: issuance `ResolvedContext::resolve` (`wyrd-auth/src/issuance.rs:355–380`) produces RootRequired without a root or Scoped with a resolved root. Runtime `RbacCheck::check` (`wyrd-runtime/src/permission_check.rs:65`) checks only operation permissions. HTTP `components/verification/routes.rs:348–365` and MCP `mcp/verification.rs:197–224` both call `VerificationControl::execute`, whose entire engine path is `run_direct:472–636`. Its `None` subject skips both Card-access refusal arms. `resolve:849–895`, `Target::resolved` and `VerificationTarget::refusal` check target existence/readiness, not caller membership. Hosted blocks can run a supplied scalar/code check without a gateway call or trace read; there is no universal downstream repair. Direct result staging uses the requested subject. Sibling enqueue (`:683–689`) already passes the subject to the same decision/audit owner.

Reachable consequence: a family account holding a verifier operation grant can execute Card-sensitive work rootless; a rooted caller can execute a same-tenant subject outside its signed graph. This was not made acceptable by the old `None` call: the cumulative task introduces the required explicit RootRequired/Scoped contract at this consumer.

Decision-complete correction: pass the direct request's subject through existing `authorize_run`, before reservation/engine work, exactly as enqueue does. No second authorization service, live-role re-resolution or gateway-only guard. Preserve the single audit decision, System execution attribution and authorized AnyRegistered rootless verification.

Closure proof: focused direct HTTP/MCP cases for RootRequired refusal, scoped outside/inside graph and authorized AnyRegistered rootless success. Refusals must produce no engine/provider dispatch or staged result/input. Reuse current real-server verification test owners.

### FIND-002-2 — REVISED / MISSING: native state IO refuses an authorized User's selected root

Discovery: B-002, FUP-AUTH-002. Obligation: REQ-013; TASK-002 scenarios 3/4; auth supplement User access token and WyrdState (`:408–419`) and rootless selector semantics (`:396–405`).

Exact locations: `crates/vala/vala-bifrost-redux/src/gate/auth.rs:157–184`; production composition `crates/wyrd/wyrd-server/src/grpc/mod.rs:360–365`; contradictory added owner prose `architecture/wyrd-security-posture.md:495–499`.

Source trace: state `client/pinned` (`wyrd-client/src/state.rs:762–787`) binds the state root. `AuthMiddleware::bearer:966–980` returns an existing User bearer unchanged. `bifrost/grpc.rs:272–314` sends it beside that root. `build_app_grpc` mounts Gate directly; Gate `write_batch:1145–1157` calls its synchronous authenticate. `needs_root_resolution` (`wyrd-runtime/src/request_context.rs:91–100`) correctly answers true for a rootless User bearer. Gate then refuses rather than resolving it. The existing OTLP sibling (`server/src/grpc/otlp.rs:275–305`) already verifies, calls `AuthenticatedPrincipal::select_root:86–105`, and supplies the verified context to Gate. `wyrd-auth::select_request_root:397–426` reuses bounded root resolution, narrows the signed Card snapshot and preserves account/grants/chain/expiry, without recording activity.

Consequence: the approved existing User bearer state can use rooted HTTP operations but cannot acknowledge its native observations. A User is not required to possess an exchangeable machine credential. The refusal is safe locally but incomplete behavior.

Authority reconciliation: the signed-only native exception was introduced by this cumulative implementation, as shown by its architecture diff. The approved packet explicitly replaces owning prose and includes User-bearer native state IO. It also allows registry work when a rootless request explicitly selects a root, while normal signed-token verification stays synchronous/store-free. Keeping the exception would require narrowing approved behavior; implementing the already approved flow does not require a new security policy.

Decision-complete correction: compose the existing server-owned optional-root selection before native Gate admission, reusing the HTTP/OTLP shared auth owner. Update the introduced exception accordingly. Keep the store-free verifier, native payload/permission/row attribution, limits, readiness, lifecycle and acknowledgement ordering. A consumer-side selector guard remains necessary at this real trust boundary; do not remove it without a validated replacement context, ignore the selector, require Service credentials for Users, add per-record SQL, or crash the shared server on resolution failure.

Closure proof: existing User Bifrost bearer + loaded state native observation reaches acknowledgement with unchanged User identity and validated owner/subject. Unknown/foreign/wrong-kind roots, signed-root mismatch, missing operation grant and out-of-graph subject refuse. Already-rooted machine and bare rootless generic admission remain valid. Exercise the actual native production mount, not only selector/parser or OTLP tests.

### FIND-009-1 — Binding occurrence preservation only

Superseding maintainer disposition: retain only committed spec REQ-012/015,
INV-004 and AC-005. They require exact owner, subject occurrence and Verifier
to survive direct/submitted work, retry and recovery. The Run's selected site
must not collapse to a first subject/Verifier match before acceptance.

Preserve that occurrence through the existing request, binding resolution and
accepted-work owners. Reuse existing containing Card and occurrence identity;
no new identity, resolver, registry or activation mechanism is required. Preserve
explicit-only bindings, authorized unbound work and existing schedule/group
semantics. Prove distinct selected occurrences through direct/submitted work
and restart, with false owner/subject/Verifier combinations refused.

### FIND-009-2 — Superseded by correlation context retention

FIND-009-2 as written is rejected by the maintainer: environment must not reach
execution. Replace its engine/gateway/evaluator/population handoff requirement
with correlation context retention:

Direct and queued verification capture one context
{service_name, service_version, deployment_environment} at acceptance. Queued
work stores it with the accepted execution; both paths write it unchanged to
the result and inputs rows. Nothing else reads it.

Prove direct and queued inputs/result row readback, including queued retry and
restart, with optional tags preserved unchanged. A status-only assertion does
not prove row retention. Preserve existing authority, leases, deadlines and
evidence semantics; use the existing accepted-work and row projection owners.
TASK-003 owns final canonical columns and full warehouse readback.

### FIND-009-3 — REVISED / VIOLATION: extended tenant read bypasses TenantConn

Discovery: STD-002009-01. Obligation: SQL AGENTS Connections/Transactions; security Postgres tenant binding: tenant functions/fields accept TenantConn, never bare PgConnection; only the boundary binds tenant authority.

Exact locations: `crates/wyrd/wyrd-sql/src/queries/verification_target.rs:78–145`; server `verification/cache.rs:217–224,319–330`; new `20261026000000_verification_deployment_environment.sql:46–52`.

Source trace: the new owner-card read materially changes VerificationTargets::read and its SQL function. It retains raw connection + independent tenant ID, adds a fourth function argument and reads the owner through manually bound SQL. The cache miss takes `postgres.conn()`, while submission (`submissions.rs:175–182`) and runner (`runner.rs:321–329`) erase their TenantConn into its transaction. SQL refuses a differently bound connection and binds the supplied tenant locally. These protections matter: no request-level tenant crossover is proven. Nevertheless the expanded tenant capability still accepts an unbound raw connection and independently supplied authority, violating the explicit security/type boundary on the changed workflow. It is not a request to clean all old SQL debt.

Decision-complete correction: retain the existing reader/cache and use TenantConn throughout its tenant read, taking the existing tenant connection constructor on pool misses and preserving callers' current tenant transactions for acceptance/claim reads. Remove redundant tenant rebinding from the replacement read path; keep target readiness, cache invalidation, one logical target read, same-tenant visibility and accepted snapshots. Do not widen to OperatorPool or invent a connection wrapper. Respect immutable applied migration rules when altering the function.

Closure proof: production reader/cache signatures and retained fields expose only approved capabilities; focused existing target/tenant/environment tests verify foreign rows remain invisible, cross-tenant rebinding cannot be requested, and acceptance/claim stay in their original transaction. Directory-limited tenant-isolation checks do not replace signature review. No exploit or higher severity is inferred solely from the raw type.

### FIND-002-3 — CONFIRMED / VIOLATION: changed Rust items miss the mandated rustdoc contract

Discovery: STD-002009-02. Obligation: root AGENTS Code and agent_docs/rust.md Rustdoc explicitly cover every changed/private/test item, require Errors for fallible contracts and Panics for possible panic paths, and make missing rustdoc merge-blocking. This one correction spans baseline TASK-002 and supplemental TASK-009; it must not be split into repeated style rounds.

Exact locations and individual confirmation are in the inventory below. The fallible-owner list has eleven function items (the source report groups the two exchange functions in one row). Each is changed in the cumulative range and lacks the stated section; `run_direct` returns a tuple containing Result and is still fallible. Trait implementation inheritance does not repair the missing changed-item contract: the changed trait method also lacks its Errors section.

Consequence: required private/test failure and invariant contracts are absent despite reported format/clippy success. This is the explicit rustdoc exception to the review's usual non-blocking style threshold.

Decision-complete correction: add accurate local rustdoc to the confirmed inventory, batching it with bounded implementation corrections and any additional Rust items they change. Document actual workflow/errors/panic invariants; preserve executable behavior, assertions and error handling. No boilerplate promises, suppressions, checker, fixture rewrite or runtime test solely for docs. Existing format/lint plus source review closes this finding. The two OTLP indexing helpers and their callers require panic docs even though they have no explicit panic macro; Signal::rejected calls the documented panic-capable HttpEncoding::decode.

### FIND-009-4 — REJECTED under superseding approved authority

CONC-001 was accurate against revision 15: Python `_session`/factories (`python/wyrd/otel.py:226–240,346–406`) and TypeScript `config`/factories (`wyrd/src/otel.ts:25–59`) accept a public client and optional root UID while configured state retains its environment-bearing derived client privately. This source is unchanged. A UID-only rebind still does not copy private state environment.

Revision 16 explicitly permits application-managed OpenTelemetry resources and standard authenticated OTLP without WyrdState, state startup, or an exporter accepting a state. Wyrd does not copy private state configuration into an application provider. The absent state-to-exporter producer therefore no longer violates acceptance. Delete the earlier proposed public exporter input decision and its two-configured-state external-exporter proof. This is rejection because approved behavior changed, not evidence of a landed fix. Preserve existing factories, authentication, optional validated UID context and application provider ownership.

The two setup paths are a **human-directed acceptance change**, not a retroactive independent FIND ID. Wyrd-managed startup supplies frozen state Service/version/environment and retains typed incompatible-provider refusal. Application-managed setup supplies standard resources and authenticated OTLP, optionally with validated Card context. Resource names/versions confer no ownership. Accepted environment wins; absent it, existing typed stable-key-first resource fallback promotes environment for owned and unowned telemetry; absent environment remains null and raw attributes survive. Permissions and signal/schema/layout contracts remain unchanged. No public state client accessor, exporter identity setting, provider pipeline, state configuration copy or additional exporter input is required.

TASK-009 owns configure/managed startup/provider refusal within existing owners. TASK-003 owns final resource promotion/fallback, canonical projection and authenticated three-signal ingest/readback, including owned-resource environment fallback. TASK-008 documents both paths. Missing TASK-003 projection is not a new TASK-002/009 defect and must not be added to FIND-009-2. Accepted-work context retention remains required for inputs/results. Focused startup/refusal proof belongs at startup; final two-path canonical readback belongs to TASK-003/change review.

## Reconciled acceptance and recommendation

| Acceptance area | Revision-16 result |
| --- | --- |
| Stable accounts, restrictions/credentials, signed authority, federation/delegation/activity and accepted principal snapshot | Prior bounded acceptance retained; no additional validated finding. |
| Direct scoped/root-required verification | FAIL: FIND-002-1. |
| Existing User bearer native state IO | FAIL: FIND-002-2. |
| Immutable configure, Service-only roots, provider-request adapters and plain adapter preservation | Prior focused source/evidence acceptance retained; final canonical projection remains deferred. |
| Exact selected occurrence | FAIL: FIND-009-1. |
| Accepted context at direct/queued inputs/result rows | FAIL: FIND-009-2. |
| Changed tenant capability and mandatory rustdoc | FAIL: FIND-009-3 and FIND-002-3. |
| Public state-bound external exporter producer | Excluded by new approval; FIND-009-4 rejected. |
| Two telemetry setup paths | Approved scope, using existing startup/auth/resource owners. Final projection/readback belongs to TASK-003 and documentation to TASK-008; no new execution proof claimed here. |

Package one remediation task for the six retained findings with the correction boundaries and focused proofs above. No material unapproved semantic ambiguity prevents those corrections. A different occurrence identity/activation model, altered scheduling/group semantics, changed permissions or new exporter contract would exceed the recommendation; none is required to close this ledger.

The prior five focused tests and diff-check remain recorded in [verification.md](../TASK-002-and-009-b1ff72dc3/verification.md). They do not prove the missing direct/native and correlation row-retention paths. Preserve the prior execution-evidence caveat: the late TASK-009 diagnostician-waiver suffix was not accepted authority, and no independent diagnostician proof is invented for those journey repairs. This evidence gap establishes no separate unproven runtime defect.

## Remediation packet boundary check

Read `TASK-002-R1-execution-context.md` in this directory. Its six diagnoses, selected shared-owner corrections, explicit-only occurrence constraint, native fail-closed composition, accepted correlation row retention, TenantConn boundary, rustdoc inventory, focused proofs and TASK-003/008 exclusions agree with this validated ledger. Planned test names are explicitly marked planned rather than falsely reported as executed. The packet adds no exporter API or obsolete FIND-009-4 requirement. No correction to its recommendation is required.

## Rustdoc item-by-item source validation

The following inventory is reproduced from the completed prior independent source validation against this same unchanged cumulative base/candidate. It was not re-audited item by item in this authority-only continuation. Every row is changed/new in this range; none is an unrelated untouched-item cleanup. D = no rustdoc; E = no Errors section on a fallible contract; P = no Panics section despite an explicit or reachable panic path. Every row is CONFIRMED. Locations are candidate line numbers. The table is an inventory of one FIND-002-3 correction, not separate findings.

| Item | Cumulative changed status / validation | Missing contract and evidence |
| --- | --- | --- |
| `crates/shared/wyrd-auth-oidc/src/registry.rs:206` `WorkloadBindingResolver::binding` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/shared/wyrd-client/src/auth.rs:1169` `exchange` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/shared/wyrd-client/src/auth.rs:1187` `exchange_workload` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/wyrd/wyrd-auth/src/jwt_bearer.rs:153` `resolve_workload_binding` | CHANGED / CONFIRMED | D/E: no item rustdoc; fallible result, no Errors section |
| `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:230` `PgWorkloadBindingResolver::binding` | CHANGED / CONFIRMED | D/E: no item rustdoc; fallible result, no Errors section |
| `crates/wyrd/wyrd-server/src/boot/mod.rs:2024` `seed_federation` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/wyrd/wyrd-server/src/boot/mod.rs:2122` `build_workload_binding` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/wyrd/wyrd-server/src/components/verification/service.rs:472` `run_direct` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/wyrd/wyrd-server/src/verification/cache.rs:88` `Target::resolved` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/wyrd/wyrd-testing/src/server.rs:3662` `bootstrap_machine` | CHANGED / CONFIRMED | D/E: no item rustdoc; fallible result, no Errors section |
| `crates/wyrd/wyrd-testing/src/verification.rs:176` `release_with` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/shared/wyrd-client/src/auth.rs:2063` `spawn_workload_mock` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/shared/wyrd-runtime/src/request_context.rs:268` `selector_never_rebinds_a_signed_root` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/gate/mod.rs:2609` `rootless_family_account_needs_a_root_to_attribute_cards` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:3857` `context_for_leasing` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:3233` `native_run_id_is_replaced_by_one_authoritative_nullable_field` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:3518` `scoped_service_principal` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:3697` `member_uids` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/scribe/ingress.rs:1298` `oversized_decoded_request_is_rejected_before_wal_append` | CHANGED / CONFIRMED | P: panic expression `assert!` |
| `crates/vala/vala-bifrost-redux/src/scribe/replay.rs:2100` `nested_accepted_subset_replays_one_fence_and_digest` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd-spec/src/auth/workload.rs:352` `workload_uid_selector_accepts_one_card_uid` | CHANGED / CONFIRMED | P: panic expression `assert_eq!` |
| `crates/wyrd-spec/src/error.rs:4515` `api_key_invalid_remediation_names_credential_issue` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert_eq!` |
| `crates/wyrd-spec/src/vala/audit_detail.rs:1331` `audit_detail_golden_vectors_cover_all_variants_and_nested_values` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/card_scope.rs:416` `issue_scope_error_passes_through_other_errors` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-auth/src/card_scope.rs:514` `uid_of` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/card_scope.rs:527` `resolve_single_service_card_produces_own_scope` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/card_scope.rs:548` `resolve_refuses_foreign_and_unknown_roots_identically` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/card_scope.rs:586` `resolve_scope_populates_every_member_uid` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:719` `unregistered_uid` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1068` `api_key_exchange_issues_no_refresh_token_or_row` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1185` `seed_suspended_account_key` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1349` `cross_tenant_key_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1373` `revoked_key_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1408` `hash_mismatch_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/issuance.rs:1366` `issue_signs_current_grants_and_follows_grant_changes` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/issuance.rs:1477` `unregistered_uid` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:863` `stored_human_connection_requires_the_sub_subject_claim` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:992` `binding_resolver_prefers_audience_then_falls_back` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/refresh.rs:1365` `a_machine_refresh_row_cannot_rotate` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/revoke.rs:251` `a_service_account_is_revoked_under_the_service_kind` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/revoke.rs:310` `a_mismatched_kind_is_refused_as_not_found` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/revoke.rs:338` `a_failed_revocation_write_keeps_its_cause_server_side` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/service_accounts.rs:36` `principal_with_permissions` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-cli/src/auth/workload_binding.rs:226` `add_parses_required_args` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-cli/src/auth/workload_binding.rs:243` `add_requires_principal` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-cli/src/auth/workload_binding.rs:258` `add_requires_subject` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-cli/src/auth/workload_binding.rs:273` `add_accepts_optional_audience` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-cli/src/principal/assignment.rs:236` `list_accepts_only_assignable_kinds` | CHANGED / CONFIRMED | P: panic expression `assert!` |
| `crates/wyrd/wyrd-cli/src/principal/revoke.rs:106` `revoke_parses_all_kinds` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query/investigation.rs:370` `refuse_unauthorized_or_oversized_reads` | CHANGED / CONFIRMED | P: panic expression `assert_eq!` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:171` `workload_jwt_bearer_mints_service_token` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:237` `workload_jwt_bearer_falls_back_to_tenant_slug_with_service_root` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:291` `workload_jwt_bearer_refuses_an_agent_default_root` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:341` `wrong_audience_is_rejected_and_audited` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:398` `expired_assertion_is_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:440` `untrusted_issuer_is_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:475` `server_owned_binding_denial_returns_principal_not_found` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:512` `cloud_workload_subject_shape_uses_server_owned_binding` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:560` `same_issuer_subject_is_tenant_scoped` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:612` `stable_account_mapping_selects_only_allowed_context` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:762` `api_key_grant_still_mints` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:810` `test_state_with_external` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:892` `bootstrap_principal` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:1201` `binding` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:1274` `card_ref` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/boot/mod.rs:2461` `sample_binding_entry` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/boot/mod.rs:2486` `workload_binding_rejects_a_non_https_issuer` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect_err(` |
| `crates/wyrd/wyrd-server/src/components/admin/routes.rs:1474` `delete_issuer_with_live_binding_conflicts_then_cascades` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/components/admin/routes.rs:1525` `workload_binding_crud_round_trip` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/components/admin/routes.rs:1614` `create_binding_for_unknown_issuer_is_not_found` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/components/auth/principal_extractor.rs:206` `extension_hit_returns_stored_principal_without_invoking_verifier` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/components/auth/routes.rs:570` `mounts_token_routes` | CHANGED / CONFIRMED | D: no item rustdoc |
| `crates/wyrd/wyrd-server/src/components/auth/routes.rs:575` `extracts_tenant_from_unverified_access_token` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/config.rs:5385` `workload_binding_toml_round_trip` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/config.rs:5435` `workload_binding_entry_unknown_field_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect_err(` |
| `crates/wyrd/wyrd-server/src/config.rs:5578` `binding_entry_empty_issuer_invalid` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/config.rs:5596` `binding_entry_empty_subject_invalid` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/config.rs:5614` `binding_entry_malformed_principal_id_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect_err(` |
| `crates/wyrd/wyrd-server/src/config.rs:5670` `bindings_without_tenant_slug_invalid` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/verification/cache.rs:524` `targets_serve_only_while_listening_and_until_invalidated` | CHANGED / CONFIRMED | P: panic expression `assert!` |
| `crates/wyrd/wyrd-server/src/verification/cache.rs:595` `effective_environment_follows_the_selector_rules` | CHANGED / CONFIRMED | P: panic expression `assert_eq!` |
| `crates/wyrd/wyrd-server/tests/integration/identity_e2e.rs:94` `binding_card_ref` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/tests/integration/identity_e2e.rs:505` `workload_jwt_bearer_unbound_subject_returns_404_keycloak` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/src/queries/auth/principal_directory.rs:151` `directory_excludes_unassignable_principals` | CHANGED / CONFIRMED | P: panic expression `assert_eq!` |
| `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:538` `restriction_statements_stay_tenant_implicit` | CHANGED / CONFIRMED | P: panic expression `assert!` |
| `crates/wyrd/wyrd-sql/src/queries/auth/workload_bindings.rs:299` `upsert_targets_composite_key_and_overwrites_columns` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_admin_principals.rs:575` `tenant_admin_principal_persists_without_a_card` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_admin_principals.rs:611` `tenant_admin_principal_cannot_carry_a_workload_restriction` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:711` `human_connection_upgrade_preflight` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:1681` `cloud_issuer_fk_rejects_delete_with_live_binding` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:1745` `cloud_issuer_binding_maps_a_stable_account` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:1889` `cloud_issuer_read_path_roundtrips_and_rls` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:2039` `cloud_issuer_crud_write_path_conflict_and_cascade` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_executions.rs:499` `request` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs:214` `card_attributed_resource_spans` | CHANGED / CONFIRMED | P: indexes fixture resource/scope vectors at `[0]` |
| `crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs:227` `card_attributed_resource_logs` | CHANGED / CONFIRMED | P: indexes fixture resource/scope vectors at `[0]` |
| `crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs:961` `Signal::body` | CHANGED / CONFIRMED | P: calls the indexing/panic-capable signal fixture helpers |
| `crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs:979` `Signal::rejected` | CHANGED / CONFIRMED | P: calls `HttpEncoding::decode`, whose response decode uses expect |

Skipped repeated unchanged discovery, new runtime tests, live services, generators and broad aggregates. Risk: the six retained corrections have not landed; final two-path resource projection/readback still requires its owning task/change-review proof.
