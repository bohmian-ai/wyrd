# TASK-001 behavior review

**Subject:** `c46afdcac` → `437205debc628538ba6aa4ec828601c7c40145b4`; approved `SPEC-verification-closeout` revision 2 and `TASK-001`. **Result: FAIL.** This is a source review of the complete cumulative diff. The task's verification table reports passing lanes; those results were available as claims, not rerun in this review.

## Acceptance matrix

| Requirement, criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001: discover active and suspended assignable principals with exact filters and UUIDv7 keyset paging | `principal_directory.rs` directory SQL; `principals/routes.rs::list_principals`; `PrincipalQuery` | `pg_principal_roles.rs` discovery/paging; reported `test:principals:integration` | FAIL: `after` accepts non-v7 UUIDs (BEH-003) |
| REQ-001 / AC-001: list assignments and idempotently grant/revoke direct roles, resolving kind from id | `principals/routes.rs::{list_principal_roles,change_direct_role}`; `role_assignments.rs` | `pg_principal_roles.rs` assignment and refusal paths; reported principal integration lane | PASS for fresh-schema behavior |
| REQ-001: IdP replacement touches only IdP rows; effective roles union sources; direct revocation leaves IdP | `role_assignments.rs::{REPLACE_IDP_USER_ROLES_SQL,LIST_USER_ROLES_SQL}`; login callback | Server and three SDK principal journeys reported passing | FAIL for existing installations: schema upgrade absent (BEH-001) |
| REQ-001 / INV-001: `service_accounts:write` reads, `*` writes, allowed/denied audit, hidden principals share 404 | `principals/routes.rs` authorization and `require_assignable`; directory excludes hidden kinds and deleted rows | `pg_principal_roles.rs` permission, audit, foreign/deleted/unknown coverage | PASS |
| REQ-002 / AC-001: four built-in roles with specified nested permissions, no aliases; new Card principals get `workload` once | `builtin_roles.rs`; `cards/auth_projection.rs`; old names removed from seed and docs | Built-in role unit tests and SDK principal journeys reported passing | FAIL for existing installations: prior roles remain and permissions do not update (BEH-001) |
| REQ-003: unbound writer attributes registered observation-target Cards; Card-bound stays within signed scope across native Bifrost and OTLP | `Principal::card_attribution`; Gate `CardRegistry`; Scribe and OTLP scope checks | Reported Bifrost OTLP and observe journeys, including registered/unregistered cases | PASS for exercised paths |
| REQ-003: same unbound attribution rule applies to verification | `VerificationControl::{resolve_direct,subject_in_scope}` permits unbound callers by RBAC | Existing verification route tests cover scope refusal, but no non-observation-target subject case | FAIL: registered non-target subject is permitted (BEH-002) |
| REQ-003 / AC-001: shared `Principals` operations project to CLI and Rust/Python/TypeScript SDKs | `wyrd_client::Principals`; CLI assignment commands; Python PyO3, TS N-API, Rust exports | Reported principal journeys, codegen and package checks | PASS for fresh-schema behavior |
| REQ-003: stock gateway/OTLP authentication asks shared client for a token per request, without another refresh owner | Python `GatewayAuth` and exporter session; TS `gatewayFetch` and OTLP factories; Rust OTLP HTTP client | Reported native gateway, OTLP and token-expiry journeys | PASS |
| AC-002: admin key and saved login complete register/invoke/observe/verify/export/query without Card key or test flush | Three SDK local and signed-in journeys; local-development guide | Reported Rust, Python, TS Bifrost and identity lanes | PASS for fresh-schema behavior |
| Non-goals: no role aliases, Card-addressed grant route, direct principal permissions, MCP principal tool, or second refresh path | Obsolete route/CLI removed; adapters call `WyrdClient.access_token()` | Codegen, MCP and dependency checks reported passing | PASS |
| Required checks and regression boundaries | Task evidence lists principal, Bifrost, gateway, codegen, tenant, format, lint and docs lanes | Reported passing; no existing-schema upgrade test or non-target verification test | FAIL: evidence cannot cover BEH-001 or BEH-002 |

## Proposed findings

### BEH-001 — REGRESSION / VIOLATION: an existing tenant cannot upgrade to the principal-role contract

- **Obligation:** REQ-001 IdP/direct provenance without destructive migration; REQ-002 replacement of retired built-ins while custom roles remain tenant-owned; AC-001; task's explicit stop condition on destructive provenance changes.
- **Location:** `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:110-119` changes an already recorded migration's checksum and primary key. The candidate deletes `20261002000100_workload_role.sql` and adds no forward migration. `crates/wyrd/wyrd-auth/src/seed.rs:20-43` retains `ON CONFLICT ... DO NOTHING` for preexisting built-in rows. `crates/wyrd/wyrd-sql/src/lib.rs:141-162` applies and verifies embedded migrations; `schema_check.rs:63-98` rejects checksum drift.
- **Reachable result:** An installation that applied the base migration fails schema verification at serving boot with `MigrateChecksum`, or migration application refuses the edited migration. Even if that check were bypassed, `auth_user_roles` there lacks `source`, so login and direct-role SQL fail; old built-in permissions and names remain because the seed never updates rows. Fresh-database journeys do not exercise this state.
- **Testable correction:** Preserve already applied migration files, add a forward migration that introduces `source` and its new key while retaining existing user grants as the appropriate source, and transition existing tenant built-ins and Card principal grants to the approved four-role model without altering custom roles. Prove an upgrade from the base schema with existing users, assignments, Card principals and custom roles, then verify serving schema and direct/IdP role behavior.

### BEH-002 — INCORRECT: unbound verification accepts a registered Card kind that cannot be an observation target

- **Obligation:** REQ-003 says unbound attribution is limited to registered observation-target Cards, identically across Bifrost, OTLP, and verification.
- **Location:** `crates/wyrd/wyrd-server/src/components/verification/service.rs:461-492` bypasses subject scope for `AnyRegistered` and checks only Card availability; `:566-588` returns `true` for every unbound principal before reading subject kind. The sibling Gate resolver at `crates/vala/vala-bifrost-redux/src/gate/attribution.rs:117-139` explicitly applies `kind.is_observation_target()`. `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:43-76` also resolves direct/binding subjects solely by active status.
- **Reachable result:** An unbound user with `evals:run` can submit a registered active `Verifier`, `Policy`, `Operator`, or other non-target Card as a verification subject and receive a queued run or direct execution, while an OTLP or Bifrost observation against that same Card is refused. The server has already resolved the exact tenant Card; this is an authorization-rule mismatch, not a foreign-tenant lookup.
- **Testable correction:** Apply the existing `CardKind::is_observation_target()` rule at the verification subject resolution boundary for both manual queued and direct execution, while retaining signed-scope checks for Card-bound callers and existing not-found handling. Add focused route tests for an unbound caller with a valid observation target and with an active non-target Card, covering both request forms.

### BEH-003 — INCORRECT: principal discovery accepts a non-UUIDv7 pagination cursor

- **Obligation:** Locked R3 decision 8 and task contract require UUIDv7 keyset paging and existing validation errors for invalid pagination.
- **Location:** `crates/wyrd-spec/src/auth/tenant_principals.rs:270` uses `PrincipalId` for `after`; `auth/principal_id.rs:31-52` accepts every syntactically valid UUID; `crates/wyrd/wyrd-server/src/components/principals/routes.rs:604-640` checks `limit` and `kind` but passes `after` directly to SQL. `pg_principal_roles.rs:477-491` tests only server-issued v7 cursors.
- **Reachable result:** `GET /v1/principals?after=<valid-v4-UUID>` returns a page, possibly empty, instead of the specified validation error. The cursor's ordering no longer has the promised UUIDv7 creation-order meaning.
- **Testable correction:** Validate the cursor's UUID version at the principal discovery query boundary, leaving `PrincipalId` usable for existing sentinel and other principal contracts. Add one HTTP test asserting the stable validation error for a valid non-v7 UUID and retain the normal v7 paging test.

## Review limits

I inspected source and the cumulative diff, including sibling SQL role writers, Gate and verification consumers, the three stock-client adapters, route tests, and the migration/serving path. I did not rerun the reported verification lanes. Other reviewers should independently validate the proposed findings before they become a final ledger.
