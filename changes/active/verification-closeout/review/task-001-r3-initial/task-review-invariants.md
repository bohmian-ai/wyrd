# TASK-001 R3 invariant review

Subject: base `c46afdcac`, candidate `437205debc628538ba6aa4ec828601c7c40145b4`; approved `SPEC-verification-closeout` revision 2; original `TASK-001-r3-principal-roles-and-local-flow.md`. Reviewed the complete base-to-candidate diff. The task's verification table is treated as reported evidence; this review did not rerun its lanes. No `.codegraph/` index exists.

## Acceptance matrix

| Requirement, criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: tenant principal discovery and idempotent principal-id role reads, grant, revoke | `principal_directory.rs`; `principals/routes.rs` list and role routes; `tenant_principals.rs` wire types | Reported `test:principals:integration`, `pg_principal_roles` | PASS |
| REQ-001: hidden principals yield one not-found and reads/writes retain their distinct permission gates | `principal_directory.rs` excludes deleted and reserved kinds; `require_assignable`; `change_direct_role` uses wildcard while reads use `service_accounts:write` | Reported `pg_principal_roles` negative and audit tests | PASS |
| REQ-001: user source provenance, IdP-only replacement, effective-role union | `auth_user_roles.source` in bootstrap SQL; `role_assignments.rs`; callback replacement; issuance reads distinct names | Reported principal integration and three SDK coexistence journeys | PASS for fresh databases; existing databases fail below |
| REQ-002: exactly four built-ins with nested permissions; Card principal first projection gets workload; omitted issuer default viewer | `builtin_roles.rs`, auth projection, trusted issuer request default | Reported builtin tests and principal journeys | PASS for fresh databases; existing databases fail below |
| REQ-003: unbound registered observation-target attribution and bounded Card scope across native Bifrost and OTLP | `Principal::card_attribution`; Gate `CardRegistry`; Scribe `attributed_cards`; OTLP projector and Scribe scope checks | Reported OTLP and observe journeys | PASS |
| REQ-003: same unbound rule for verification observations | `VerificationControl::resolve_direct` and `subject_in_scope` | Reported verification route tests; no wrong-kind subject proof identified | FAIL (INV-003) |
| Principal HTTP contract: exact filters, 1–200 limit, UUIDv7 `after` cursor | `PrincipalQuery` and `list_principals`; SQL keyset by id | Reported paging test covers returned cursors, not a non-v7 UUID | FAIL (INV-002) |
| Principal clients and CLI share typed list/roles/grant/revoke; no legacy Card grant route | `wyrd_client::Principals`, CLI principal command, Python and TS adapters; old route removed | Reported SDK principal journeys, codegen, CLI tests | PASS |
| Stock gateway and OTLP adapters refresh through the shared client on each request | Rust `otel.rs`; Python `GatewayAuth` and exporter session; TS `gatewayFetch` and exporter header callback | Reported signed-in journeys, unit tests, package check | PASS |
| AC-002: setup key and saved login complete local workflows without issuing a Card key or publication flush | Three SDK local and signed-in journeys and local-development guide | Reported Rust/Python/TS journeys and identity lanes | PASS |
| Preserve live installation and existing tenant role assignments | Changed bootstrap migration, removed later workload migration, `seed_builtin_roles_for_tenant` inserts without updating | Fresh fixture lanes only; no existing-ledger upgrade proof | FAIL (INV-001) |
| No role aliases, principal direct permissions, MCP role surface, compatibility route, second refresh path, or Rust gateway adapter | Complete diff of auth route removal, role constants, CLI/SDK surfaces | N/A | PASS |

## Findings

### INV-001 — REGRESSION: deployed databases cannot accept the changed migration ledger

**Obligation.** REQ-001/002 role provenance and exact four built-ins must work for the existing server and tenants; the task prohibits destructive assignment migration.

**Source.** `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql` changes the already-versioned bootstrap migration to add `source` and a new primary key; candidate deletes `20261002000100_workload_role.sql`. `wyrd_sql::MIGRATOR` embeds migration checksums and `OperatorPool::verify_migrations` in `schema_check.rs` rejects a changed applied checksum. `verify_schema` runs before serving readiness. A database that applied the base migration retains the old three-column role table and old role rows. `seed_builtin_roles_for_tenant` only inserts missing names with `ON CONFLICT DO NOTHING`; it cannot update old built-in permissions or retire names.

**Consequence.** An existing installation fails migration/checksum verification and cannot serve. Even bypassing that check leaves `grant_role_to_user` and IdP replacement referencing a missing `source` column, and retained old built-ins violate the exact-role contract. Fresh fixture tests cannot detect this lifecycle path.

**Testable correction.** Preserve applied migration files and provide a forward, nondestructive tenant schema/data transition for the source key and built-in role replacement, including existing direct grants and Card principals under the approved semantics. Prove migration and serving schema verification from a database with the base ledger and representative existing assignments. If preserving the exact required assignment provenance for existing rows needs a product decision, return that decision for spec revision instead of silently assigning a source.

### INV-002 — INCORRECT: the public `after` cursor accepts any UUID version

**Obligation.** The locked HTTP contract specifies a UUIDv7 keyset cursor and invalid pagination must return the existing validation error.

**Source.** `crates/wyrd-spec/src/auth/tenant_principals.rs:272` types `after` as `PrincipalId`; `principal_id.rs::FromStr` accepts any parseable UUID. `principals/routes.rs:638-647` forwards its UUID without a version check to `list_assignable_principals`, whose SQL applies `id > $4`. The integration paging test passes only server-produced UUIDv7 cursors.

**Consequence.** `after=<valid UUIDv4>` returns 200 and an arbitrary partial/empty page rather than the specified 400 validation response. The server's claimed creation-order cursor contract is therefore not enforced at its trust boundary.

**Testable correction.** Validate the effective cursor as UUIDv7 before querying while preserving the existing `PrincipalId` wire type and stable validation error; add a focused HTTP test with a valid UUID of another version.

### INV-003 — INCORRECT: unbound direct verification accepts a registered non-observation Card as subject

**Obligation.** REQ-003 grants unbound principals attribution only to registered **observation-target** Cards in the same tenant, consistently for Bifrost, OTLP, and verification observations.

**Source.** `crates/shared/wyrd-runtime/src/principal.rs::card_attribution` produces `AnyRegistered`. Gate `CardRegistry::resolve` filters `card.kind.is_observation_target()` before it supplies a scope. In contrast, `crates/wyrd/wyrd-server/src/components/verification/service.rs:472-495` retrieves any `subject_card_uid` through `get_card_by_uid`, skips scope when the principal is `AnyRegistered`, then accepts any available subject with an active Verifier. `subject_in_scope` at line 574 also returns true immediately for this principal before inspecting its target.

**Consequence.** A user or tenant administrator with `evals:run` can directly judge and attribute a result to a registered control-plane Card (for example `Policy` or `Verifier`) that the same principal cannot name as an observation target in Gate. This breaks the common attribution rule and produces a verdict for an excluded subject kind.

**Testable correction.** Apply the existing observation-target kind predicate to the resolved verification subject at the verification service's target boundary, preserving the current tenant registry lookup and Card-bound scope rule. Cover a direct call with an active non-observation subject and an allowed observation-target subject; check queued target resolution if it can receive the same kind through an existing binding.

## Result

**FAIL.** The migration lifecycle failure is decisive. The cursor and direct-verification failures are independently reachable public-contract gaps. Findings are proposals for the independent validation pass; no source was modified.
