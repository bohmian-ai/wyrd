# TASK-002 R10 Structured Ponytail Findings Validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `6e21d8ed00d5159ec71e3f2e2414e80fd16f76af`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation authority: TASK-002 R1 through R9 and their prior verdicts and
  validated finding ledgers

The candidate remained exactly
`6e21d8ed00d5159ec71e3f2e2414e80fd16f76af` throughout validation. The
repository has no `.codegraph/` directory, so this review used Git, `rg`, and
direct source and caller inspection. All four required Wave 1 reports were
present. This validator read the complete bodies and callers named below and
did not rely on an intended verdict.

## Validation result

**VALIDATED WITH FINDINGS — FIX_REQUIRED.** Both Wave 1 standards findings are
reachable, distinct violations of the repository's hard Rust documentation
rule. They require only two local comment corrections. No runtime behavior,
test body, route composition, public contract, dependency, or architecture
decision needs to change, so no specification revision is required.

## Wave 1 disposition

| Wave 1 proposal | Disposition | Stable finding | Validation summary |
|---|---|---|---|
| `REPO-TASK-002-1` | **CONFIRMED** | `FIND-TASK-002-23` | The test body was materially narrowed when the public authorization-code grant was retired, remains reachable in the ordinary `wyrd-spec` unit-test target, and has no item rustdoc. |
| `REPO-TASK-002-2` | **CONFIRMED** | `FIND-TASK-002-24` | The module now owns and mounts login initiation, but its module contract still lists only the other three auth surfaces. |
| Task review | Empty ledger validated for task acceptance | — | No separate task-behavior defect was found. |
| Security review | Empty ledger validated | — | No separate security finding was found. |
| Tenancy/data review | Empty ledger validated | — | No separate tenancy, persistence, concurrency, durability, or migration finding was found. |

## Independent validation

### `REPO-TASK-002-1` — materially changed token-contract test

The complete body of
`new_grant_variants_reject_unknown_fields` constructs the surviving
`jwt-bearer` and `refresh_token` request variants with an extra field and
asserts that both fail deserialization. The base-to-candidate diff deletes the
authorization-code input and assertion from this function because TASK-002
retires that grant, so the item is materially modified rather than untouched
legacy code. It has no direct Rust caller, as expected for a `#[test]`
function; the standard test harness discovers it in the `wyrd-spec` library
test target. The path is therefore reachable and directly protects the changed
wire contract.

`AGENTS.md` section 16, `architecture/agent-rules.md`, and the Rust-core
documentation authority explicitly include materially modified test functions
in the rustdoc requirement. The surrounding `serde_grant_type_discriminator`
and `authorization_code_grant_is_retired` tests do not replace this item's
contract: one proves discriminators and round trips, and the other proves the
retired grant is absent; neither documents or proves why the two surviving
newer grants reject unknown fields.

Ponytail outcome: deletion would violate the hard documentation rule, while a
new test, helper, lint, or harness would add no proof. Add one concise rustdoc
block to the existing function explaining that the surviving `jwt-bearer` and
`refresh_token` variants deny unknown fields after authorization-code
retirement. Preserve the assertions and all adjacent serde behavior.

### `REPO-TASK-002-2` — stale auth-routes module contract

The module's complete production composition is visible in `auth_router`: it
mounts `POST /auth/login`, `GET /auth/callback`, `POST /auth/token`, and
`POST /auth/issue-key` under one shared governor. The full body has two callers:
`http::router::build_router`, which merges it into the served HTTP/OpenAPI
router, and `tests::mounts_token_and_issue_key_routes`, which constructs it.
`build_router` is the production server composition, so this is not a dormant
or test-only module.

The module-level rustdoc still says the module owns only token exchange, the
OIDC callback, and Card-bound API-key issuance. The cumulative diff adds the
login handler to this module's imports and router composition, while the
already-correct `auth_router` item rustdoc lists all four surfaces. The stale
module contract therefore omits the public login-initiation boundary required
by TASK-002 and contradicts its own router owner.

Ponytail outcome: the existing router documentation is not module
documentation and cannot satisfy the explicit module-item rule. Update only
the two module-rustdoc lines to add tenant human login initiation. Reuse the
existing wording for the other three surfaces; do not move documentation,
change routing, rename the module, or add a documentation checker.

## Final deduplicated finding ledger

### FIND-TASK-002-23 — Materially changed token-contract test lacks mandatory rustdoc

- **Wave 1 source ID:** `REPO-TASK-002-1`
- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Violated obligation:** `AGENTS.md` section 16,
  `architecture/agent-rules.md`, and
  `architecture/references/languages/rust-core.md` require substantive rustdoc
  for every new or materially modified Rust item, explicitly including test
  functions; missing documentation is a hard blocker.
- **Exact location:**
  `crates/wyrd-spec/src/auth/token.rs:225-241`
  (`new_grant_variants_reject_unknown_fields`).
- **Evidence:** the cumulative diff removes the retired authorization-code
  case from this test while retaining the `jwt-bearer` and `refresh_token`
  unknown-field contract. The function is immediately preceded by `#[test]`
  and has no rustdoc. It is discovered by the ordinary Rust test harness; no
  production caller is expected for a test item.
- **Observable consequence:** the candidate violates a hard repository
  acceptance rule, and the narrowed test no longer states whether its remaining
  two-variant scope is intentional after grant retirement.
- **Decision-complete correction:** add concise rustdoc immediately above
  `#[test]` explaining that the surviving `jwt-bearer` and `refresh_token`
  request variants reject unknown fields after authorization-code retirement.
  Keep the test name and body unchanged; add no helper, test, lint, or harness.
- **Focused closure proof:** direct source inspection confirms the item has the
  stated contract and the body is unchanged; run `git diff --check`, then the
  repository-required `mise run fmt` and `mise run lints` broader checks.

### FIND-TASK-002-24 — Auth-routes module documentation omits login initiation

- **Wave 1 source ID:** `REPO-TASK-002-2`
- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Violated obligation:** `AGENTS.md` section 16,
  `architecture/agent-rules.md`, and the Rust-core documentation authority
  require every materially modified Rust module to document its workflow role
  accurately and completely.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:1-2`.
- **Evidence:** `auth_router` mounts four surfaces, including the newly added
  `POST /auth/login`, and is called by the production
  `http::router::build_router`. The module rustdoc lists only token exchange,
  callback, and Card-bound API-key issuance, while the router's own rustdoc
  correctly lists all four.
- **Observable consequence:** the required module contract gives an incomplete
  ownership map at the exact public auth boundary TASK-002 changed and fails a
  hard source-documentation rule.
- **Decision-complete correction:** update only the module-level rustdoc to
  include tenant human login initiation alongside token exchange, the OIDC
  callback, and Card-bound API-key issuance. Preserve all imports, handlers,
  router composition, governor behavior, and item-level documentation.
- **Focused closure proof:** direct source inspection confirms the module
  contract names all four mounted surfaces and `git diff` confirms no code
  changed; run `git diff --check`, then `mise run fmt` and `mise run lints` as
  the broader repository checks.

## Prior-finding closure

The current candidate was independently checked at each previously corrected
owner. No prior stable finding is reopened.

| Prior finding | Current-candidate closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-1` | `ConnectionInput::validate` and stored-row decode require exact human `sub`; callback identity remains `(issuer, sub)`. | CLOSED |
| `FIND-TASK-002-2` | `verify_authorized_party` enforces configured-client `azp` before identity or issuance. | CLOSED |
| `FIND-TASK-002-3` | Changed mapped roles append the canonical `auth.user.roles.sync` event in the callback transaction; unchanged sets do not. | CLOSED |
| `FIND-TASK-002-4` | Fresh advertised-algorithm membership is checked before the shared verifier's asymmetric signature/JWKS validation. | CLOSED |
| `FIND-TASK-002-5` | Login-state purge, consume, complete, and redeem rely on forced RLS without manual tenant predicates. | CLOSED |
| `FIND-TASK-002-6` | State-owner lookup remains a narrow inherent `WyrdPostgres::login_state_tenant` operation over the private app pool. | CLOSED |
| `FIND-TASK-002-7` | Durable login state stores the PKCE verifier as `SecretString` and wraps the decoded string immediately. | CLOSED |
| `FIND-TASK-002-8` | The exact trait projections, SQL constants, signing-helper panic contract, and test alias from R2 remain documented. | CLOSED |
| `FIND-TASK-002-9` | The cited SHA-256, Utoipa, and Wiremock imports remain module-scoped. | CLOSED |
| `FIND-TASK-002-10` | `verify_id_token_algorithm` accurately documents advertised membership and the shared verifier's symmetric-algorithm ownership. | CLOSED |
| `FIND-TASK-002-11` | Generic verification requires `exp`, `iss`, and `aud`; OIDC verification additionally requires valid `iat` while workload assertions retain their separate generic contract. | CLOSED |
| `FIND-TASK-002-12` | Refresh resolves the family, takes `lock_refresh_family` before lifecycle classification, and holds it through rotation or replay containment. | CLOSED |
| `FIND-TASK-002-13` | `refresh_by_hash` documentation matches lookup-before-lock-before-classification and test-only lifecycle observation. | CLOSED |
| `FIND-TASK-002-14` | Administrative User revocation takes the same family lock before suspension and family retirement. | CLOSED |
| `FIND-TASK-002-15` | `issue_human_session` takes the family lock before connection and principal reads and refresh insertion. | CLOSED |
| `FIND-TASK-002-16` | `print_tokens` documents only its token-output role; the stale argument-parsing sentence remains absent. | CLOSED |
| `FIND-TASK-002-17` | OIDC-specific verification rejects missing, non-string, empty, non-ASCII, and over-255-byte subjects while the generic workload path stays separate. | CLOSED |
| `FIND-TASK-002-18` | The callback takes the User family lock immediately after canonical identity resolution and before role replacement, audit, issuance, completion, and commit. | CLOSED |
| `FIND-TASK-002-19` | `TokenResponse::refresh_token` and generated descriptions identify human-only refresh issuance; machine grants remain credential re-exchange. | CLOSED |
| `FIND-TASK-002-20` | `POST /auth/token` carries `#[tracing::instrument(level = "debug", skip_all)]`. | CLOSED |
| `FIND-TASK-002-21` | `auth_router` has substantive four-surface/shared-governor rustdoc and the required static-configuration `# Panics` contract. | CLOSED |
| `FIND-TASK-002-22` | `HumanConnections::begin_login` passes only `TenantSlug` to `WyrdPostgres::resolve_tenant_slug`; app-pool selection stays inside the database owner. | CLOSED |

## Recommendation and verification limits

Route `FIND-TASK-002-23` and `FIND-TASK-002-24` together as one bounded
documentation-only remediation. The two corrections are independent one-site
edits and should not share a helper or abstraction. Preserve every executable
path and all prior-finding owners.

- This Wave 2 pass did not run Cargo-backed commands, Postgres, Docker, or IdP
  lanes, as directed. Wave 1 records fresh focused verifier and callback
  concurrency checks plus static/codegen checks, and the immutable packet
  records the broader green lanes.
- Fresh cumulative `git diff --check` passed.
- Compilation and behavioral tests do not enforce complete private test/module
  rustdoc, so the two static defects remain despite the recorded green lanes.
- TASK-003 BFF completion, TASK-004 CLI handoff/persistence, and live
  Okta/Entra qualification remain downstream or change-level work, not gaps in
  this bounded task.
