# TASK-002 task review — verdict

## Immutable subject

- Repository root: `/home/user/wyrd`, branch `claude/admin-principals-spec-qfsmjc`.
- Base: `e551d5d` (end of TASK-001). Candidate: `f3923cc` (HEAD).
- Range: `ee0cb94`, `cf52a3b`, `7c1d403`, `d4185ef`, `91442c5`, `f3923cc`
  (29 files, +1180/-31).
- Approved spec: `changes/active/admin-principals/spec.md`,
  `SPEC-admin-principals` revision 6, approved 2026-09-18.
- Task: `changes/active/admin-principals/tasks/TASK-002-auth-context-and-planes.md`.
- The candidate did not change during review.

## Verdict

**FIX_REQUIRED**

Three material findings: `FIND-TASK-002-1` (VIOLATION), `FIND-TASK-002-2`
(VIOLATION), `FIND-TASK-002-3` (MISSING).

Remediation task:
`changes/active/admin-principals/review/task-002/TASK-002-R1-audited-single-plane-entry.md`.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-013 — closed two-variant authenticated context carrying identity, type, and exactly one scope | `wyrd-runtime/src/principal.rs` `AuthContext`, `PlatformPrincipal` | `principal::auth_context_tests::*` (4/4 pass) | PASS |
| REQ-013 — a platform identity is not representable where a tenant identity is required | `AuthContext::Platform(PlatformPrincipal)` carries no tenant field; `PlatformCaller` and `Caller` are distinct extractors | `auth_context_tests::platform_context_carries_no_tenant`, `platform_extractor::tests::platform_caller_exposes_no_tenant` | PASS |
| REQ-017 — vocabulary gains platform administrative operations (create, read, suspend, recover) | `permission.rs` `Resource::Tenants`, `Action::Suspend`, `Action::Recover`, four constructors | `platform_extractor::tests::authorization_follows_the_grant_and_never_reaches_a_tenant`, `…::empty_grant_authorizes_nothing` | PASS |
| REQ-018 / INV-004a — platform authority never confers tenant data access | `PlatformPrincipal` has no tenant; `Permission::tenant_recover_admin()` is the separately named capability | `platform_extractor::tests::authorization_follows_the_grant_and_never_reaches_a_tenant`; `platform_authz::pg_tests::a_tenant_context_is_refused_on_the_platform_plane` | PASS |
| REQ-019 — capability follows from the grant, never from the type | `resolve_grant` reads `platform.principal_grants`; absence resolves to an empty set | `platform_extractor::tests::empty_grant_authorizes_nothing` | PASS |
| REQ-014 — tenant identity derives only from verified claims | `token_extract.rs::verify_authenticated_principal` (unchanged) derives tenant from the token; the platform variant has no tenant to supply | `auth_context_tests::tenant_context_carries_exactly_its_tenant` | PASS |
| REQ-031 — tenant access stays under `TenantConn` RLS, no hand-written filters, no widened query | new table is platform-scope via `OperatorPool`; no tenant predicate added | `python3 scripts/check_tenant_isolation.py` → passed | PASS |
| AC — audit row appends in the deciding transaction for allowed and denied alike; unrecordable audit refuses and commits nothing | `platform_authz.rs::PlatformAuthorization::authorize`, migration `20260601000021`, `queries/platform/audit_authz.rs` | `platform_authz::pg_tests::{an_allowance_commits_with_the_operation, a_rolled_back_operation_leaves_no_allowance, a_denial_is_recorded_and_refuses, an_unrecordable_decision_fails_closed}` (4/4 pass) | **FAIL** — holds only for `PlatformAuthorization::authorize`; `PlatformCaller::authorize` is a second, unaudited decision point (`FIND-TASK-002-1`) |
| REQ-012 — machine credential exchange mints a Wyrd token carrying the principal representation | no platform exchange exists; the platform plane mints no token | none | **FAIL** (`FIND-TASK-002-2`) |
| REQ-012a — the per-request path constructs the context only from verified Wyrd token claims; no surface authorizes from credential material, a lookup prefix, or a credential record | `platform_extractor.rs::from_request_parts` authenticates the raw credential against `platform.credentials` on every request | `platform_extractor::tests::bearer_credential_is_extracted` demonstrates the forbidden shape | **FAIL** (`FIND-TASK-002-2`) |
| INV-013 — revocation advances the applicable authorization epoch transactionally; no token outlives it | none; the platform plane issues no token, so the epoch obligation was declared inapplicable | `platform_credentials::pg_tests::revocation_takes_effect_on_the_next_request` | **FAIL** — the claim is a consequence of the `REQ-012a` deviation, not a satisfaction of `INV-013` (`FIND-TASK-002-2`) |
| REQ-015 / REQ-016 — all authenticated handlers receive their principal through the context; no authorization keyed on credential material remains | no handler changed; tenant handlers already decide against `Caller.principal` | grep sweep of `wyrd-server/src/components/` for `api_key`/`credential`-keyed authorization → no matches (reproduced) | PASS on the letter of "no credential-keyed authorization"; see `FIND-TASK-002-2` for the platform plane, which newly *introduces* one |
| AC-003 / INV-004 — cross-plane negative evidence, both directions, with stable errors and no enumeration | platform direction: `extract_platform_credential` + `PlatformCredential::prefix_of`; reverse direction: nothing in this candidate | `platform_extractor::tests::{a_tenant_access_token_is_not_a_platform_credential, a_tenant_api_key_is_not_a_platform_credential, malformed_headers_yield_no_credential}` | **FAIL** — reverse direction and the on-route proof are absent (`FIND-TASK-002-3`) |
| INV-011 — fail-closed on unknown, unverifiable, or unauditable conditions | one indistinguishable `unauthenticated()` rejection; `AuditUnavailable` rolls back and refuses | `platform_credentials::pg_tests::every_rejection_is_the_same_error`, `platform_authz::pg_tests::an_unrecordable_decision_fails_closed` | PASS for the paths that exist; the unaudited `PlatformCaller::authorize` has no unauditable condition to fail closed on (`FIND-TASK-002-1`) |
| Non-goal — no platform OIDC, human platform principals, initialization, provisioning, or credential-management routes | none added | n/a | PASS |
| Non-goal — no customizable roles, no second grant store, cache, or checker | one grant read, one `PermissionSet::contains` | n/a | PASS |

## Repository-standards result

See `standards-review.md`. Two standards failures, both already carried as
`FIND-TASK-002-1` and `FIND-TASK-002-3`. All other applicable rules pass,
including the two-connection-abstraction rule: `&mut Transaction` in
`queries/platform/audit_authz.rs` is what `scripts/check_tenant_isolation.py:276`
requires of a platform query module, `OperatorPool::begin()` is the documented
caller-owned-transaction boundary, and `vala-sql` `forge_operations.rs` /
`forge_tasks.rs` are in-repo precedent.

**The standards audit was not independent** — this container exposes no
in-session subagent tool, and the remote substitute resolved its own GitHub
clone instead of the immutable candidate and could not return a retrievable
report. A genuinely independent standards audit against the cumulative candidate
is a precondition for any future `PASS`.

## Verification limits

Run by this review, in `/home/user/wyrd`, with `rustup run 1.97.1` substituted
for the absent `mise` toolchain shim (the container default fails to compile
`wyrd-server`):

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `python3 scripts/check_tenant_isolation.py` | passed |
| `python3 scripts/check_unwrap_audit.py` | passed |
| `python3 scripts/check_clippy_allow.py` | passed |
| `python3 scripts/check_test_contracts.py` | passed (13 contracts) |
| `cargo clippy --locked -p wyrd-runtime -p wyrd-auth -p wyrd-auth-check -p wyrd-auth-verify --all-targets` | clean |
| `cargo clippy --locked -p wyrd-server --lib` | 3 warnings, all in files this diff does not touch (`state.rs:17`, `boot/mod.rs:878`, `mcp/mod.rs:134`); out of scope under `VER-005` |
| `cargo test --locked -p wyrd-runtime --lib` | 47 passed |
| `cargo test --locked -p wyrd-auth-check --lib` | 18 passed |
| `cargo test --locked -p wyrd-auth-verify --lib` | 34 passed, **7 failed** |
| `cargo test --locked -p wyrd-server --lib platform_extractor` | 7 passed |
| `scripts/postgres/with-test-postgres.sh -- … cargo test --locked -p wyrd-auth --lib platform_ -- --test-threads=1` | 13 passed |

Every test the task packet names was re-run and passes. The seven
`wyrd-auth-verify` failures are all `tests::verify_external_*`, each panicking in
`reqwest` with `No rustls crypto provider is configured`; they are environmental,
touch no surface in `VER-001`, and are out of scope under `VER-005`. The task
packet records these as "nine"; the actual count is seven — a bookkeeping error
in the evidence section, not a finding.

Not run, with reasons: `mise run codegen:check` (`mise` absent; the added
permission labels `tenants`, `suspend`, `recover` appear in no committed schema,
stub, or golden artifact, so `VER-006` has no drift surface here). Broad
aggregates were deliberately not run per `VER-003`.

## Material findings

### `FIND-TASK-002-1` — VIOLATION — a second, unaudited platform authorization entry point

- **Obligation**: agent-rules — "Every decision that evaluates a principal's
  permission appends one audit row — allowed and denied alike"; "Every audited
  authorization decision appends its row in the same transaction as the
  decision… A decision that cannot be recorded fails closed." AGENTS.md §2, same.
  TASK-002 acceptance criterion: "An authorization decision appends its audit row
  in the deciding transaction for allowed and denied outcomes alike; an
  unrecordable audit refuses the operation and commits nothing." REQ-016, INV-011.
- **Location**: `crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs:52-63`
  (`PlatformCaller::authorize`).
- **Evidence**: the method evaluates
  `self.context.effective_permissions().contains(required)` and returns
  `Ok(())` or `WyrdError::PermissionDeniedRbac`. It takes no pool, opens no
  transaction, and appends nothing. It is `pub`, it hangs off the axum extractor
  that every platform route must take, and the task's own evidence table cites
  its two unit tests (`authorization_follows_the_grant_and_never_reaches_a_tenant`,
  `empty_grant_authorizes_nothing`) as proof of REQ-017 and REQ-018. The audited
  `PlatformAuthorization::authorize` in `crates/wyrd/wyrd-auth/src/platform_authz.rs`
  has no production caller (`grep -rn PlatformAuthorization` returns only its own
  definition and its `pg_tests`).
- **Consequence**: this is the concrete path by which a platform authorization
  decision commits with no audit row and with no fail-closed behavior. It is the
  more discoverable of the two APIs, so the first platform route is more likely
  to reach for it than for the audited one.
- **Compounding**: the audited path's coupling guarantee is currently unusable.
  `PlatformAuthorization::authorize` returns `Transaction<'a, Postgres>`, but
  every existing `crates/wyrd/wyrd-sql/src/queries/platform/*` slot —
  `insert_platform_principal`, `set_platform_grant`,
  `platform_grant_for_principal`, and the credential slots — takes
  `&OperatorPool` and executes on a connection drawn from the pool, not on the
  caller's transaction. No existing platform write can be composed into the
  returned transaction, so a caller that does hold it must either add
  transaction-taking slots or commit its operation outside the audited
  transaction, which silently reintroduces exactly the decoupling the design
  exists to prevent.
- **Required correction**: exactly one reachable platform-plane authorization
  entry point, and it must be the one that appends its decision row inside the
  transaction the operation itself runs in and refuses when that append fails.
  Platform writes must be composable into that transaction.

### `FIND-TASK-002-2` — VIOLATION — the platform per-request path authorizes from credential material

- **Obligation**: REQ-012a — "The per-request path MUST construct the
  authenticated context only from verified Wyrd token claims, subject to the
  existing authorization epoch. No served surface MAY authorize from credential
  material, a lookup prefix, a credential record, or a provider token directly."
  TASK-002 Constraints restate it: "The request path reads only verified token
  claims… Never a credential record, lookup prefix, provider token, request
  header, path, hostname, or body." Also REQ-012 (credential exchange mints a
  Wyrd token), REQ-013, REQ-005, INV-013.
- **Location**: `crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs:76-82`
  and `:130-176`.
- **Evidence**: `from_request_parts` reads the `Authorization` request header,
  strips `Bearer `, and passes the raw secret to
  `PlatformCredentials::authenticate(&pool, &presented)`, which resolves the
  lookup prefix and verifies against the `platform.credentials` record — on every
  request — then reads the grant and builds `AuthContext::Platform` from that
  credential record. All four of the named forbidden inputs (request header,
  credential material, lookup prefix, credential record) are read on the request
  path. No platform token is minted anywhere in the candidate.
- **Consequence**: the platform plane has no entry-path / request-path
  separation. The credential secret travels on every request instead of once;
  every platform request performs an Argon2 verification plus two operator-pool
  round trips; and INV-013's epoch coupling is declared inapplicable rather than
  satisfied. The task's claim that "the platform plane issues no access tokens,
  so it has no epoch to advance" is internally consistent but is a *consequence*
  of the deviation, not evidence that the requirement does not apply — REQ-012a
  names the epoch explicitly for the per-request path. It also leaves TASK-007
  without a foundation: REQ-045 requires that "a platform session MUST carry
  platform scope only", and
  `crates/shared/wyrd-auth-verify/src/lib.rs:811` currently rejects
  `PrincipalKindTag::GlobalAdmin` outright, so no platform-scope token can verify
  into any context at all.
- **Required correction**: platform credential verification becomes an entry path
  that mints a platform-scope Wyrd token; the per-request platform extractor
  builds `AuthContext::Platform` from verified token claims and the principal's
  grant alone, touching no credential record; and revocation of a platform
  credential, principal, or grant advances a platform authorization epoch
  transactionally, so a token issued before it stops verifying.

### `FIND-TASK-002-3` — MISSING — cross-plane proof on a real route, and the reverse direction

- **Obligation**: AC-003 — "Cross-plane negative evidence proves a tenant
  credential cannot invoke any platform operation **and a global credential
  cannot invoke ordinary tenant operations**, with stable errors and no
  enumeration." TASK-002 approach step 6 — "Prove plane separation and
  tenant-boundary enforcement on an existing protected route before any new
  administrative route exists." AGENTS.md §11 and the agent-rules journey bullet.
- **Location**: absent. `crates/wyrd/wyrd-server/tests/auth_e2e.rs` and
  `crates/wyrd/wyrd-server/tests/pg_authz_check_route.rs` — both named in the
  task's *Relevant Surface* — are untouched by the candidate.
- **Evidence**: the only cross-plane evidence is four in-process unit tests over
  header-shape and lookup-prefix helpers in `platform_extractor.rs::tests`. No
  test presents a platform credential to a tenant-plane extractor or route, and
  no test drives either direction through a real server.
- **Consequence**: this is a proof gap rather than a live hole — the tenant plane
  requires `X-Wyrd-Access-Token` carrying a compact Wyrd JWT
  (`components/auth/token_extract.rs:17-47`), so a `wyrd_global_*` secret on
  `Authorization` cannot authenticate there today. But the obligation the task
  accepted was to demonstrate it on an existing protected route, and a unit test
  over a helper does not stand in for that.
- **Required correction**: real-server evidence, on an already-protected route,
  that a platform credential is refused on the tenant plane and a tenant access
  token is refused on the platform plane, each with its stable error and with no
  response that distinguishes an existing tenant or principal from an absent one.

## Non-blocking observations

Recorded for the record; none is a finding and none requires action in
remediation.

- `wyrd_runtime::AuthContext` collides by name with the pre-existing
  `vala_bifrost_redux::gate::AuthContext`, and `wyrd-server` now imports both
  (`http/otlp.rs:49` and `components/auth/platform_extractor.rs:19`).
- `AuthContext::Tenant`, `From<Principal>`, `tenant()`, and `tenant_id()` have no
  production constructor; the tenant plane still carries `Principal` through
  `Caller`. REQ-013 requires the two-variant type to exist, so this is not drift,
  but the tenant arm remains unexercised outside tests.
- `PlatformAuthorization` is a zero-sized struct, which AGENTS.md §5 discourages.
  It mirrors `PlatformCredentials` (`platform_credentials.rs:112`), established
  and accepted in TASK-001, so consistency with the owning crate's pattern wins.
- Commit `ee0cb94` carries TASK-001 residue — optional `card_ref`, optional
  credential expiry — into `vala-bifrost-redux`, `wyrd-server`, `wyrd-storage`,
  and `wyrd-testing`. Mechanical and required to compile the range; every edit
  preserves assertion strength.

## Prior-finding closure

None. This is the first review of TASK-002.

## Candidate integrity note

While this review was being written, a concurrent process in the same working
tree left uncommitted modifications to three candidate files —
`crates/wyrd/wyrd-auth/src/platform_authz.rs`,
`crates/wyrd/wyrd-auth/src/platform_credentials.rs`, and
`crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs` (file mtimes
2026-09-18 21:20:23Z). They appear to be remediation already in progress —
`PlatformAuthorization` is being reshaped to own an `OperatorPool`.

The reviewed subject is unaffected: `e551d5d..f3923cc` is committed history and
is intact (`git diff --stat e551d5d..f3923cc` still reports 29 files,
+1180/-31). Every command in **Verification limits** ran before those
modifications appeared, against the pristine candidate. This review commit stages
only `changes/active/admin-principals/review/task-002/`; the working-tree
modifications are left untouched and uncommitted.

The findings below are scoped to `f3923cc`. Whatever the concurrent work turns
out to be, it is a later candidate and must be reviewed as the cumulative
candidate on its own pass.
