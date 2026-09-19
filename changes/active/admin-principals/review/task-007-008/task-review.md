# TASK-007 / TASK-008 — task implementation review

## Immutable subject

| Item | Value |
|---|---|
| Repository root | `/home/user/wyrd` |
| Branch | `claude/admin-principals-spec-qfsmjc` |
| Base | `c5c20754a167e8f4d74a555a720bd51df6179a6f` (`git merge-base HEAD origin/change/surfaces-oracle-integration`) |
| Candidate | `4225069` `feat(client): project principal and credential administration to the SDKs` |
| Working tree | clean at review time (`git status --porcelain` empty) |
| Approved spec | `changes/active/admin-principals/spec.md` revision 6 (status `approved`) |
| Tasks | `tasks/TASK-007-platform-human-administration.md`, `tasks/TASK-008-sdk-and-mcp-projection.md` |
| Verification scope | `VER-001`..`VER-006` (spec §Verification scope) |

## Verification executed

Environment substitutions were required and are recorded as such: `mise` is not
installed, `cargo-nextest` is not available. `rustup run 1.97.1 cargo` and
`cargo test` were used in their place. No broad aggregate was run; `VER-003`
absences are not reported as findings.

| Command | Result |
|---|---|
| `cargo check --locked -p wyrd-auth -p wyrd-client -p wyrd-sdk-rust --all-targets` | pass |
| `cargo clippy --locked -p wyrd-auth-oidc -p wyrd-auth -p wyrd-server -p wyrd-client --all-targets` | pass, no diagnostics |
| `with-test-postgres.sh -- WYRD_AUTH_E2E=1 cargo test -p wyrd-server --test platform_admin_e2e -- --test-threads=1` | 10 passed, 0 failed |
| `with-test-postgres.sh -- WYRD_AUTH_E2E=1 cargo test -p wyrd-sdk-rust --test principals -- --ignored --test-threads=1` | 2 passed, 0 failed |

The green lanes are credible for what they cover. They do not cover the
federated login flow, identity pinning, or any non-Rust surface, because no test
for those exists (FIND-007-1, FIND-007-2, FIND-008-1..3).

## Acceptance matrix — TASK-007

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-041 platform authority is a grant, stored at platform scope | `wyrd-sql/migrations/20260601000020_admin_principals.sql`; `platform_extractor.rs:93-118` resolves grant per request | `platform_admin_e2e.rs` `the_two_control_planes_cannot_reach_each_other` | PASS |
| REQ-042 human platform principals live at platform scope, no tenant | `20260601000023_platform_identity.sql:25-27` adds `user` kind; `platform.principals` has no tenant column | `pg_admin_principals.rs::platform_principals_have_no_tenant_column` | PASS |
| REQ-043 one deployment-owned connection; absence/outage never blocks the credential | `platform.oidc_connection` singleton; `identity.rs:180-240` | `an_operator_configures_and_removes_federated_platform_sign_in` covers *removed*; **provider-failing not covered** | PARTIAL |
| REQ-044 pre-registration, one-time `(issuer, subject)` pin, unknown subject denied | `platform_login.rs:258-300`; `queries/platform/identity.rs::pin_platform_identity` | **none** — no test at any tier | FAIL |
| REQ-044 matching claim is trustworthy | `identity.rs:46-52` maps `email`; `platform_login.rs:282` matches on `claims.email` with no `email_verified` check | none | FAIL (FIND-007-3) |
| REQ-045 entry point selects connection; platform session carries platform scope only | `platform_login.rs:146`, `PLATFORM_TOKEN_SCOPE` check in `verify_platform` and `confirm` | `the_two_control_planes_cannot_reach_each_other` (credential path only) | PARTIAL |
| REQ-046 only platform authority creates a platform principal / grant | `identity.rs:274` `authorize(..., platform_identity_write)`; migration grants `platform.*` to `wyrd_platform_admin` only | `a_tenant_administrator_cannot_configure_platform_sign_in` | PASS |
| REQ-034/REQ-035 tenant human path, same context shape, coexistence | no change to the tenant human path; `identity_e2e.rs` untouched | none added | FAIL (FIND-007-8) |
| REQ-040 documentation | no `docs/` change in the diff | n/a | FAIL (FIND-008-7) |
| INV-004a platform authority confers no tenant data access | `PlatformCaller` holds no tenant (`platform_extractor.rs:39-52`) | `platform_caller_exposes_no_tenant`, `the_two_control_planes_cannot_reach_each_other` | PASS |
| INV-004b platform authority unreachable from the tenant plane | migration grants nothing on `platform.*` to `wyrd_app`; `PlatformAuthzError` `wrong_control_plane` arm | `a_tenant_context_is_refused_on_the_platform_plane`, `automation_cannot_escalate_itself_to_an_administrator` | PASS |
| AC-011 federated human ≡ machine credential context; human tenant admin coexists | not implemented as evidence | none | FAIL |
| AC-015 first login pins and succeeds; unknown subject denied; credential works while provider fails | pin logic exists; provider-failure path exists only as `ProviderUnavailable` | connection-removed only | FAIL |
| AC-016 scope separation for the same human across both planes | no federated-human test exists | none | FAIL |
| AC-017 no tenant-plane operation creates/elevates a platform principal | DB grant + plane separation | `a_tenant_administrator_cannot_configure_platform_sign_in`, `automation_cannot_escalate_itself_to_an_administrator`; **OIDC group mapping path not exercised** | PARTIAL |
| Task constraint: reuse SSRF screening | `configure_connection` (`identity.rs:129-178`) validates nothing and stores caller-supplied `jwks_uri` verbatim | none | FAIL (FIND-007-4) |
| Task constraint: provider secrets in no read/log/trace/error/audit | `PlatformOidcConnectionView` omits it; `#[instrument(skip(request))]`; `IssuerSealError` opaque | e2e asserts the read is secret-free | PASS with FIND-007-9 |
| Task Approach 3: pre-registration, **listing**, **revocation** | only pre-registration (`POST /platform/admins`) | none | FAIL (FIND-007-6) |
| Task Approach 6: amend `changes/active/tenant-oidc-federation/spec.md` | not amended | n/a | FAIL |
| Task Verification: real-provider identity journey is the primary proof | `identity_e2e.rs` untouched; 0 occurrences of `platform` in it | none | FAIL (FIND-007-1) |

## Acceptance matrix — TASK-008

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-036 administrative ops over HTTP with typed bodies and stable errors | `wyrd-spec/src/auth/platform_identity.rs`, `components/principals/routes.rs`, `components/platform/*` | e2e + SDK journey | PASS |
| REQ-036 …and **generated artifacts** | `http/openapi.rs` and `openapi.yaml` unchanged; no new path registered | n/a | FAIL (FIND-008-5) |
| REQ-036 CLI, SDKs, and MCP project the contract | Rust only: `wyrd-client/src/principals/`, `sdks/wyrd-sdk-rust` | Rust journey passes | FAIL (FIND-008-1/2/3) |
| INV-014 no client becomes durable truth | `principals/handle.rs` is transport-only, holds no state | SDK journey | PASS |
| Constraint: `wyrd-client` is the sole SDK-facing Rust surface; SDKs thin over it | `sdks/wyrd-sdk-rust/src/lib.rs` re-exports `wyrd_client::principals` | compiles | PASS |
| Constraint: credential plaintext crosses once, never persisted by a client | `IssuedCredential` returned once; handle writes nothing | `creates_a_principal_and_rotates_its_credential` | PASS |
| AC-013 one identity model; `bootstrap-key` and fabricated identities gone | `cli:bootstrap-key` removed, `boot/mod.rs` no longer declares the module — but `boot/bootstrap.rs` still tracked at HEAD with `SYSTEM_OPERATOR_ID` and `system/bootstrap-admin`; 3 docs pages still document `bootstrap-key` | none | FAIL (FIND-008-6) |
| AC-013 no unreachable second identity model in the schema | `20260601000020_admin_principals.sql:21-24` drops `platform.users/roles/user_roles/api_keys`; query slots deleted | `pg_migration.rs` | PASS |
| AC-014 Rust, Python, TypeScript SDKs perform their operations against a real server | Rust only | Rust journey only | FAIL |
| AC: tenant-scope client cannot invoke a platform-plane operation through any SDK or MCP tool | no platform operation exists on any client surface | none | FAIL (FIND-008-4) |
| AC: MCP write tools scope-gated, read tools open | not implemented | none | FAIL |
| AC: boundary checks (client tier, SDK client tier, PyO3 scope) | `wyrd-client/principals` depends only on `reqwest` + `wyrd-spec` | not run (`mise` unavailable); inspection shows no forbidden dependency | PASS (by inspection) |
| REQ-040 documentation of the three-command journey, SaaS model, rotation, loss recovery | none | n/a | FAIL (FIND-008-7) |

## Overall result

`FAIL`.
