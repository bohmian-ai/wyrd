# Repository standards review

Result: **FAIL**.

Subject: cumulative `036fc48fe26fb87648a277b10099bc475d293b92...b1ff72dc34c344b596c85cf408e6a9217685be35`, TASK-002 plus TASK-009. HEAD remained the candidate during this review. This report reviews repository standards, not task acceptance or optional design preferences. Source and authority were not modified.

## Authority coverage

The starting navigation map was expanded through CodeGraph before source searches, including `OperatorPool`, `TenantConn`, `WyrdState`, and `exchange_rooted`; the complete cumulative diff identified the affected owners. The repository has exactly the root and four nested AGENTS files listed below; all were read. Current approved architecture and task authority were used as frozen by `subject.md`.

| Changed surface | Applicable authorities | Source/consumer coverage |
|---|---|---|
| Shared contracts: auth workload/token/principal DTOs, errors, references, audit detail, Verifier binding and execution | Root AGENTS; `agent_docs/{index,rust,errors,agent-surfaces,maintainer-style}.md`; `architecture/wyrd-design.md` Client model, Runtime identity, Verifier, Error catalog; security Principal and credential lifecycle, Access and refresh tokens | `wyrd-spec` additions/removals and consuming issuance, ingress, SDK declarations, generated wire types/error catalog |
| Authentication: shared OIDC/verify and server auth issuance, exchange, federation, refresh, revocation | Above; security Principal and credential lifecycle, Request principal, Permission model, Postgres tenant binding, Audit integrity and privacy, Cryptography and secret handling | Shared registry, `wyrd-auth` changed owners and PostgreSQL helpers, server grant/extractor/middleware and federation boot callers |
| SQL account/restriction/federation/activity/execution persistence and migrations | `crates/wyrd/wyrd-sql/AGENTS.md`; root and Rust/error guides; security Postgres tenant binding; operations Deployment and release: Database logins, Migration contract | Query signatures and retained connection fields inspected directly; new migrations, associated rows, verifier target reader, acceptance/scheduler/runner consumers, SQL integration tests |
| Verification direct/queued/scheduled authority and environment | Root, Rust/error/maintainer guides; design Runtime identity/Configured Service context/Verifier; security accepted-authority and tenant rules | `components/verification/service.rs`; `verification/{authority,cache,runner,scheduler,submissions}.rs`; queue DTOs and status projections; runtime PostgreSQL tests |
| Gateway, OTLP, Bifrost Gate/Scribe/Oracle | Root and Rust/error guides; security Gateway attribution, Request principal, Audit integrity; `architecture/bifrost-design.md` Ingest, Read audit and terminal contract, Public surface; operations readiness and database boundaries | Gateway credential cache/admission/capture changes, HTTP and gRPC OTLP, Gate selector/attribution/error handling, Scribe scope consumers and changed replay/persistence tests, Oracle principal fixture consumers |
| Shared client/state/HTTP/gRPC/observation/verification and Rust SDK OTEL | Root and Rust/error guides; design Client model, Configured Service context, Stock provider SDK context, API key → JWT flow | Middleware derivation/cache and context propagation, state configuration/pinning, transport headers, producer scope fingerprint, Run verification request, Rust exporter and SDK journeys/support |
| Python/PyO3 package, OTEL and HTTP adapters, authored stubs, tests | `sdks/wyrd-sdk-python/AGENTS.md`; root; Rust/error/maintainer guides; design Client model and Stock provider SDK context | Native client/state/observe/principal/verify/testing changes; public package registration/re-export context; changed `.pyi` signatures; pure-Python adapters and top-level unit/integration tests |
| TypeScript/N-API package, declarations, context adapters, tests | `sdks/wyrd-sdk-ts/AGENTS.md`; root; Rust/error/maintainer/agent-surface guides; design Client model and Stock provider SDK context | Native client/cards/principals/testing changes, handwritten public wrappers/OTEL, generated `.d.ts`/`.d.cts` and wire types/error codes, unit/journey/support changes |
| CLI/MCP principal/restriction/federation and retired issue-key surface | Root; Rust/error/agent-surface guides; design Principal model; security Permission model | CLI dispatch/options, MCP descriptor/catalog/handler, shared HTTP operation, generated error docs and surface tests |
| Fixtures, shared test server/verification/capacity helpers and journey changes | Root Testing; `TESTING.md`; Rust/maintainer guides; SQL/Python/TS nested rules where applicable | Explicit account fixture setup, rooted token helpers, verification fixture release/activity, fixture YAMLs, Rust/Python/TS and MCP/HTTP journeys, OTLP negative helpers |
| Documentation and tooling | `docs/AGENTS.md`, `docs/{CONTRIBUTING,PRODUCT}.md`; root; agent-surfaces; `architecture/README.md`; `changes/spec-driven-development.md`; operations Migration contract | Generated errors/llms text, MCP reference entry, fixtures/TESTING changes, authority edits, Postgres compose-project naming |

## Rule results

| Rule | Result | Evidence |
|---|---|---|
| Tenant SQL functions/fields use only `TenantConn` or `OperatorPool`; tenant binding is owned by `TenantConn` | **FAIL** | The extended verification-target workflow retains `PgConnection` in `VerificationTargets` and `Db::Conn`, obtains `postgres.conn()` for misses, and the new SQL function binds its own tenant. STD-002009-01. Account/restriction/activity writes otherwise use `TenantConn`; no new production `PgPool` owner was found. |
| Every changed Rust item has meaningful rustdoc; fallible items have `# Errors`, potentially panicking items have `# Panics`, including tests/helpers | **FAIL** | STD-002009-02 and its item inventory below. Green format/clippy cannot prove private/test documentation compliance. |
| Durable contracts belong in IO/async/PyO3-free `wyrd-spec`; SDKs remain projections of `wyrd-client`; no new heavy/client-server dependency leak | PASS | Contract/DTO changes remain in `wyrd-spec`; client state/auth stays in shared client; PyO3 additions stay in Python SDK and N-API additions in TS native. No manifest/dependency/feature additions occur in the cumulative diff. |
| Stable domain ids, typed wire bodies and derive-backed error projection | PASS | Selected roots use `CardUid`; stable account inputs use `PrincipalId`; `WorkloadRestriction` is closed and serde-backed; new `SdkStateConfigFrozen` carries derive metadata, and generated error projections include it. OAuth retains its architecture-authorized OAuth envelope. |
| Secrets use redacted wrappers, scrubbed tracing; no credential command-line addition | PASS | Exchange/issuer flows retain `SecretString`/`SecretBearer`; selector/registration fields are non-secret; new restriction route instrumentation skips state/caller/body; CLI additions name account/root/family, not credentials. |
| Atomic writes, caller-owned tenant transaction, SQLSTATE error mapping, PostgreSQL coordination clock | PASS outside STD-002009-01 | Restriction replacement/account creation execute on the caller's `TenantConn`; conflict mapping uses `SqlError::UniqueViolation` and constraint identity; activity uses `statement_timestamp()` and `GREATEST`; new table has composite tenant foreign keys and forced RLS policies. New migrations are appended rather than editing applied migration files. |
| One audit path, permission checks precede protected work, MCP writes enforce scopes/domain checks | PASS | Restriction HTTP/MCP reuse `set_workload_restriction_for`; it requires/audits wildcard permission and uses the existing Scribe audit stage. MCP listing gates the new write descriptor and execution rechecks the same operation. No alternate audit sink was introduced. |
| Shared Rust transport/auth lifecycle; foreign-runtime adapters own only runtime context/foreign boundaries | PASS | Python/TS adapters obtain tokens from the shared native client, snapshot their own active Run, and project configured headers. Rust HTTP/gRPC/exporters use middleware context. No replacement durable registry or auth store is added. |
| Python public/native signature and authored-stub/export parity | PASS | New client workload constructor keyword/getters/`for_workload`, state configure/environment/http_auth, principal restriction operation, verification environment and testing removals agree across native wrappers and authored stubs. Existing registered classes expose the new methods; package exports retain those classes. Python tests remain top-level and exercise public modules. |
| TypeScript N-API declaration and public wrapper parity | PASS | `connectWyrdClient` fourth workload argument, native client getters/`forWorkload`, configure options/state methods and principal setter are present in both declaration formats. Handwritten wrappers/types export the new public surface; testing issue-key removals reach all declaration/loading projections. Claimed generation checks were not rerun. |
| Runtime MCP description and schema agreement | PASS with non-blocking note | Schema is generated from Service-only `WorkloadKind`; one descriptor sentence still says Service or Agent. See note below. |
| Tests at their owning tier; PostgreSQL/server tests excluded from fast lanes without silently skipping unavailable credentials | PASS | New shared pure/context tests are inline; PostgreSQL cases use `pg_tests`/`pg_*`; SDK and MCP journeys use the repository's established ignored journey lane, explicitly run with `--run-ignored=all` in evidence. This is not a newly weakened existing test or a silent missing-credential return. |
| Generated contracts/docs updated through owning source; no committed JSON schema or compatibility alias | PASS | Generated error/wire and declaration diffs correspond to native contract changes; evidence records `codegen:check` and `ts:napi:check`. Old issue-key route/CLI/API fixtures are removed rather than aliased. No committed schema or new compatibility route appears. |
| Portable implementation and repository tooling scope | PASS | No platform-gated runtime implementation, forbidden host isolation primitive, new task/check, or dependency was introduced. Compose project naming keeps existing sanitization/truncation and puts process ids before the worktree name. |

## Material findings

### STD-002009-01 — Extended tenant target reads bypass the required SQL capability

Classification: VIOLATION. Governing rule: `crates/wyrd/wyrd-sql/AGENTS.md`, Connections: “ALWAYS take `&mut TenantConn<'_>` for tenant-scoped work and `&OperatorPool` for cross-tenant work. These are the only connection types allowed in function signatures and struct fields”; security Postgres tenant binding reserves binding/control to `TenantConn`.

Locations: `crates/wyrd/wyrd-sql/src/queries/verification_target.rs:78` (`VerificationTargets`, raw field and constructor at lines 81/91, materially extended `read` at line 113); `crates/wyrd/wyrd-server/src/verification/cache.rs:217` (`Db::Conn`, miss path at lines 319–328); new migration `20261026000000_verification_deployment_environment.sql:46` (independent tenant validation/binding).

The candidate extends the target read with the caller's Service owner and uses it for environment selection before direct/queued verification. A cache miss still obtains a plain app connection via `postgres.conn()` and passes a separately supplied tenant id to a raw-connection reader. The new function independently calls `set_config('app.current_tenant', ...)`. Transaction-backed siblings pass `conn.transaction()` as the same raw capability. Thus the new owner/context resolution depends on manually carrying and binding tenant identity rather than the mandatory type that establishes it. SQL's current matching check does not restore the compile-time capability or make this an authorized exception.

Reachable consumers: direct resolution and enqueue admission in `components/verification/service.rs:694,864`; queue acceptance in `verification/submissions.rs:175`; worker target resolution in `verification/runner.rs:321`. The query owner and SQL function are materially changed in this range; this is not a request to sweep unrelated old raw SQL owners.

Consequence: the extended tenant read accepts an unbound connection and independent tenant authority, allowing callers to bypass the repository's single binding boundary. No externally exploitable cross-tenant request is claimed here; the explicit tenancy capability invariant itself is absent on the new resolution path.

Smallest correction: keep the existing target reader/cache workflow, but retain/pass `TenantConn` for tenant reads, acquire it through `WyrdPostgres::tenant_conn` on cache misses, preserve the caller's existing transaction for acceptance/worker reads, and stop rebinding the tenant inside the replacement SQL function. Preserve cache behavior, readiness, one target read, owner environment reconciliation, and accepted snapshots. Do not add a second connection abstraction or widen a tenant query to an operator pool. Closure: inspect production signatures/fields and run the narrow target/verification tenant and environment regressions; a green directory-limited tenant-isolation check alone is insufficient.

### STD-002009-02 — Changed Rust owners and helpers do not meet the rustdoc gate

Classification: VIOLATION. Governing rules: root AGENTS, Code (“Every new or changed Rust item has rustdoc … `# Errors` on fallible items, `# Panics` when it can panic. Missing rustdoc blocks merge”); `agent_docs/rust.md`, Rustdoc explicitly includes private helpers, test helpers, and tests. This finding groups the correction once across the cumulative write set.

Fallible owner/helper omissions:

| File | Item/line | Missing contract |
|---|---|---|
| `crates/shared/wyrd-auth-oidc/src/registry.rs` | `WorkloadBindingResolver::binding:206` | `# Errors` for resolver failure |
| `crates/shared/wyrd-client/src/auth.rs` | `exchange:1169`, `exchange_workload:1187` | `# Errors` for shared grant transport/refusal/configuration failures |
| `crates/wyrd/wyrd-auth/src/jwt_bearer.rs` | `resolve_workload_binding:153` | Rustdoc and `# Errors` for binding-store/unbound-subject failure |
| `crates/wyrd/wyrd-auth/src/pg_resolvers.rs` | `PgWorkloadBindingResolver::binding:230` | Implementation rustdoc/`# Errors`, including new root decode failure |
| `crates/wyrd/wyrd-server/src/boot/mod.rs` | `seed_federation:2024`, `build_workload_binding:2122` | `# Errors` |
| `crates/wyrd/wyrd-server/src/components/verification/service.rs` | `run_direct:472` | `# Errors` for direct acceptance/execution, including selector disagreement |
| `crates/wyrd/wyrd-server/src/verification/cache.rs` | `Target::resolved:88` | `# Errors` for unavailable target/refusal |
| `crates/wyrd/wyrd-testing/src/server.rs` | `bootstrap_machine:3662` | Rustdoc and `# Errors` |
| `crates/wyrd/wyrd-testing/src/verification.rs` | new `release_with:176` | `# Errors`, including write/commit and malformed Card failure; its single link is not an Errors section |

The same source pass includes the following changed tests/helpers. `D` means no item rustdoc; `P` means assertions, explicit panic, unwrap or expect can panic but no `# Panics` is documented. These are one bounded documentation correction, not separate stylistic findings. Pure identity conversion helpers whose generated-id expectation is a named invariant still need the mandated panic documentation.

| File | Items (line: omissions) |
|---|---|
| `crates/shared/wyrd-client/src/auth.rs` | `spawn_workload_mock` (2063: P) |
| `crates/shared/wyrd-runtime/src/request_context.rs` | `selector_never_rebinds_a_signed_root` (268: P) |
| `crates/vala/vala-bifrost-redux/src/gate/mod.rs` | `rootless_family_account_needs_a_root_to_attribute_cards` (2609: P) |
| `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs` | `context_for_leasing` (3857: P) |
| `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs` | `native_run_id_is_replaced_by_one_authoritative_nullable_field` (3233: P), `scoped_service_principal` (3518: P), new `member_uids` (3697: P) |
| `crates/vala/vala-bifrost-redux/src/scribe/ingress.rs` | `oversized_decoded_request_is_rejected_before_wal_append` (1298: P) |
| `crates/vala/vala-bifrost-redux/src/scribe/replay.rs` | `nested_accepted_subset_replays_one_fence_and_digest` (2100: P) |
| `crates/wyrd-spec/src/auth/workload.rs` | `workload_uid_selector_accepts_one_card_uid` (352: P) |
| `crates/wyrd-spec/src/error.rs` | `api_key_invalid_remediation_names_credential_issue` (4515: D/P) |
| `crates/wyrd-spec/src/vala/audit_detail.rs` | `audit_detail_golden_vectors_cover_all_variants_and_nested_values` (1331: D/P) |
| `crates/wyrd/wyrd-auth/src/card_scope.rs` | `issue_scope_error_passes_through_other_errors` (416: D/P), new `uid_of` (514: P), `resolve_single_service_card_produces_own_scope` (527: D/P), `resolve_refuses_foreign_and_unknown_roots_identically` (548: P), `resolve_scope_populates_every_member_uid` (586: P) |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs` | new `unregistered_uid` (719: P), `api_key_exchange_issues_no_refresh_token_or_row` (1068: P), `seed_suspended_account_key` (1185: P), `cross_tenant_key_rejected` (1349: D/P), `revoked_key_rejected` (1373: D/P), `hash_mismatch_rejected` (1408: D/P) |
| `crates/wyrd/wyrd-auth/src/issuance.rs` | `issue_signs_current_grants_and_follows_grant_changes` (1366: P), new `unregistered_uid` (1477: P) |
| `crates/wyrd/wyrd-auth/src/pg_resolvers.rs` | `stored_human_connection_requires_the_sub_subject_claim` (863: P), `binding_resolver_prefers_audience_then_falls_back` (992: D/P) |
| `crates/wyrd/wyrd-auth/src/refresh.rs` | `a_machine_refresh_row_cannot_rotate` (1365: P) |
| `crates/wyrd/wyrd-auth/src/revoke.rs` | `a_service_account_is_revoked_under_the_service_kind` (251: P), `a_mismatched_kind_is_refused_as_not_found` (310: P), `a_failed_revocation_write_keeps_its_cause_server_side` (338: P) |
| `crates/wyrd/wyrd-auth/src/service_accounts.rs` | `principal_with_permissions` (36: D/P) |
| `crates/wyrd/wyrd-cli/src/auth/workload_binding.rs` | `add_parses_required_args` (226: D/P), `add_requires_principal` (243: D/P), `add_requires_subject` (258: D/P), `add_accepts_optional_audience` (273: D/P) |
| `crates/wyrd/wyrd-cli/src/principal/assignment.rs` | `list_accepts_only_assignable_kinds` (236: P) |
| `crates/wyrd/wyrd-cli/src/principal/revoke.rs` | `revoke_parses_all_kinds` (106: D/P) |
| `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query/investigation.rs` | `refuse_unauthorized_or_oversized_reads` (370: P) |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs` | `workload_jwt_bearer_mints_service_token` (171: D/P), `workload_jwt_bearer_falls_back_to_tenant_slug_with_service_root` (237: P), `workload_jwt_bearer_refuses_an_agent_default_root` (291: P), `wrong_audience_is_rejected_and_audited` (341: D/P), `expired_assertion_is_rejected` (398: D/P), `untrusted_issuer_is_rejected` (440: D/P), `server_owned_binding_denial_returns_principal_not_found` (475: D/P), `cloud_workload_subject_shape_uses_server_owned_binding` (512: D/P), `same_issuer_subject_is_tenant_scoped` (560: D/P), `stable_account_mapping_selects_only_allowed_context` (612: P), `api_key_grant_still_mints` (762: D/P), `test_state_with_external` (810: D/P), `bootstrap_principal` (892: P), `binding` (1201: P), new `card_ref` (1274: D/P) |
| `crates/wyrd/wyrd-server/src/boot/mod.rs` | `sample_binding_entry` (2461: D/P), `workload_binding_rejects_a_non_https_issuer` (2486: D/P) |
| `crates/wyrd/wyrd-server/src/components/admin/routes.rs` | `delete_issuer_with_live_binding_conflicts_then_cascades` (1474: D/P), `workload_binding_crud_round_trip` (1525: D/P), `create_binding_for_unknown_issuer_is_not_found` (1614: D/P) |
| `crates/wyrd/wyrd-server/src/components/auth/principal_extractor.rs` | `extension_hit_returns_stored_principal_without_invoking_verifier` (206: D/P) |
| `crates/wyrd/wyrd-server/src/components/auth/routes.rs` | `mounts_token_routes` (570: D), `extracts_tenant_from_unverified_access_token` (575: D/P) |
| `crates/wyrd/wyrd-server/src/config.rs` | `workload_binding_toml_round_trip` (5385: D/P), `workload_binding_entry_unknown_field_rejected` (5435: D/P), `binding_entry_empty_issuer_invalid` (5578: D/P), `binding_entry_empty_subject_invalid` (5596: D/P), `binding_entry_malformed_principal_id_rejected` (5614: D/P), `bindings_without_tenant_slug_invalid` (5670: D/P) |
| `crates/wyrd/wyrd-server/src/verification/cache.rs` | `targets_serve_only_while_listening_and_until_invalidated` (524: P), `effective_environment_follows_the_selector_rules` (595: P) |
| `crates/wyrd/wyrd-server/tests/integration/identity_e2e.rs` | `binding_card_ref` (94: P), `workload_jwt_bearer_unbound_subject_returns_404_keycloak` (505: P) |
| `crates/wyrd/wyrd-sql/src/queries/auth/principal_directory.rs` | `directory_excludes_unassignable_principals` (151: P) |
| `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs` | `restriction_statements_stay_tenant_implicit` (538: P) |
| `crates/wyrd/wyrd-sql/src/queries/auth/workload_bindings.rs` | `upsert_targets_composite_key_and_overwrites_columns` (299: D/P) |
| `crates/wyrd/wyrd-sql/tests/integration/pg_admin_principals.rs` | `tenant_admin_principal_persists_without_a_card` (575: P), `tenant_admin_principal_cannot_carry_a_workload_restriction` (611: P) |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs` | `human_connection_upgrade_preflight` (711: P), `cloud_issuer_fk_rejects_delete_with_live_binding` (1681: D/P), `cloud_issuer_binding_maps_a_stable_account` (1745: D/P), `cloud_issuer_read_path_roundtrips_and_rls` (1889: D/P), `cloud_issuer_crud_write_path_conflict_and_cascade` (2039: D/P) |
| `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_executions.rs` | `request` (499: P) |
| `crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs` | new `card_attributed_resource_spans` (214: P), `card_attributed_resource_logs` (227: P), `Signal::body` (961: P through these helpers), `Signal::rejected` (979: P from response decoding expectations) |

Consequence: the explicit merge-blocking documentation obligation is not met, including new helper workflows and failure contracts invisible to public-item linting. The correction is documentation beside these changed items, preserving all code and assertions. Describe actual failures/invariants; do not add placeholder boilerplate, suppressions, tests, or a new checker. Fold this into the implementation remediation for the material issue. Closure is a source review of the changed Rust items plus the existing relevant format/lint lane; no new runtime proof is needed solely for prose.

## Non-blocking note

`crates/wyrd/wyrd-server/src/mcp/principals.rs:175` describes a family kind as `Service|Agent`, but `WorkloadKind` and the generated input schema accept Service only. Remove `|Agent` from this new descriptor text when making the bounded correction. Runtime/schema enforcement is correct; this wording observation does not independently determine the verdict.

## Verification and limits

Reviewed existing implementation evidence in both tasks and review-time `verification.md`. The review-time checks selected and passed two client tests, one Python adapter test, one TS adapter test, and the shared claim-verifier test. Existing task evidence reports the broader owner/journey, codegen, declaration, format, lint and boundary checks; these are supporting evidence, not substitutes for the source-local failures above. No tests, generators, formatters, migrations, dependency changes, or build-output checks were run by this reviewer. Source inspection included a read-only changed-function rustdoc inventory; no inspection script was added to the repository.

Skipped independent runtime/Postgres/IdP/Vault/UI reruns. Risk: claimed green checks do not enforce the raw SQL capability or private/test rustdoc rules identified here.
