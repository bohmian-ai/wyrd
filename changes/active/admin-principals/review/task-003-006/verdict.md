# Task review verdict — TASK-003, TASK-004, TASK-005, TASK-006

**Verdict: `FIX_REQUIRED`**

## Immutable subject

| | |
|---|---|
| Repository root | `/home/user/wyrd` |
| Branch | `claude/admin-principals-spec-qfsmjc` |
| Base | `40a73817d415e9a1626e6ec7a91edda083e344d3` (merge-base with `origin/change/surfaces-oracle-integration`; `origin/main` is not fetched in this checkout) |
| Candidate | `9bc53a6` — *test(server): prove credential rotation has no gap* |
| Range | 34 commits, 96 files, +8794 / −433 |
| Approved spec | `changes/active/admin-principals/spec.md`, revision 6, approved 2026-09-18 |
| Tasks under review | `tasks/TASK-003-deployment-initialization.md`, `TASK-004-tenant-provisioning.md`, `TASK-005-tenant-administration.md`, `TASK-006-credential-lifecycle-and-recovery.md` |
| Standards audit | `standards-review.md` (same directory) |

**Subject stability**: the reviewed range `40a7381..9bc53a6` did not change during
review. While the review was in progress the branch advanced to `b9597dc`
*feat(auth): one external-token verification for both control planes*, with
further uncommitted work in `wyrd-auth` (`platform_login.rs`, `login.rs`,
`callback.rs`, `pg_resolvers.rs`, `platform_sessions.rs`). That is TASK-007
platform OIDC work — a task not under review — and it added no commit to and
rewrote no commit in the reviewed range. Every finding below cites a commit that
remains in history. This is recorded rather than returned as `BLOCKED`, because
the immutable subject was obtainable and was audited in full; a re-review of
TASK-007 must nonetheless confirm that its work did not disturb the seams named
in FIND-004-1 and FIND-006-1, both of which sit in `wyrd-auth`.

## Verification performed and its limits

Scope is `VER-001` … `VER-006`. Broad aggregates were not run and their absence
is not treated as missing verification (`VER-003`). `mise` is unavailable in this
environment, so lanes were reproduced with `rustup run 1.97.1 cargo` and
`scripts/postgres/with-test-postgres.sh`, which is what the `mise` lanes wrap.

| Command | Result |
|---|---|
| `cargo clippy --locked -p wyrd-server -p wyrd-sql -p wyrd-auth --all-targets` | clean |
| `with-test-postgres.sh -- env WYRD_AUTH_E2E=1 cargo test -p wyrd-server --test platform_admin_e2e -- --test-threads=1` | 5 passed, 0 failed |
| `with-test-postgres.sh -- cargo test -p wyrd-sql --test pg_admin_principals -- --test-threads=1` | 9 passed, 0 failed |

Limits worth stating, because they bound what green means here:

- The five journeys return early unless `WYRD_AUTH_E2E` is set. They are real
  proof in the gated lane and silent no-ops outside it.
- `mise run codegen:check` and `mise run docs:check` (named by TASK-004/005/006)
  could not be run. They would pass trivially: no generated artifact and no doc
  was touched, which is itself finding FIND-004-6 / FIND-004-5.
- `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:506` asserts
  `contains()` against a SQL literal re-declared inside the test body rather than
  against the production query. It is not evidence for the nullable-expiry
  change. Pre-existing pattern, updated but not introduced here.
- The standards audit was not produced by an independent specialist; see
  `standards-review.md`.

## What the candidate does well

Stated so the findings are read against the real state of the work, not as a
verdict on the whole change.

- The two-plane separation is genuinely type-level. `PlatformCaller` is the only
  producer of `AuthContext::Platform`, `Caller` the only producer of
  `AuthContext::Tenant`, and the platform routes sit outside the `/v1` nest so a
  tenant token is refused before a handler sees it. The journey proves both
  directions.
- Platform-plane authorization is correctly coupled to its audit record:
  `PlatformAuthorization::authorize` appends inside the deciding transaction,
  hands the open transaction to the caller so decision and effect commit
  together, rolls back and refuses when the append fails, and commits denials
  durably (`wyrd-auth/src/platform_authz.rs:110-137`).
- Single initialization is carried by a durable `UNIQUE` on
  `platform.principals.name`, not by a read-then-write check. That part of
  TASK-003's approach step 2 is exactly right.
- The migrations are careful and correct: name-agnostic constraint discovery
  with a load-bearing predicate, superset replacements that validate existing
  rows, a column rename that carries its FK, index and RLS policy, and a
  nullable-expiry change paired with the matching lookup predicate.
- Identity/credential separation is real and proven: `pg_admin_principals` shows
  a principal authorized with no credential at all, and the rotation journey
  shows two live credentials on one principal with the principal untouched.
- Recovery restores the *same* principal id, proven end to end.
- The retired `platform.users/roles/user_roles/api_keys` model is genuinely gone
  and `pg_migration` asserts its absence.

## Acceptance matrix

### TASK-003 — Deployment initialization

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Creates exactly one global principal holding platform authority, one credential, prints plaintext once with non-retrievable guidance | `boot/init.rs:76-101`; `main.rs:79-86` | `platform_admin_e2e.rs:90-97` (journey 1) | PASS |
| A second invocation refuses, creates nothing, re-exposes nothing | `boot/init.rs:87-89` on `UNIQUE(name)` | `platform_admin_e2e.rs:144-162` | PASS |
| Concurrent invocations converge on one principal and one credential; the loser refuses | `platform.principals.name UNIQUE` (migration …20:35) | none | **FAIL** — FIND-003-2 |
| An injected failure at each stage leaves the deployment uninitialized and a later invocation succeeds cleanly | none — three unrelated statements, `init.rs:78`, `:95`, `:97` | none | **FAIL** — FIND-003-1, FIND-003-2 |
| Server start emits no credential material to stdout, logs or traces under every profile | `main.rs:42` — `Command::Init` is the only credential-emitting arm | none | **FAIL** — FIND-003-2 (asserted by construction, never tested) |
| An uninitialized server serves ordinary routes and refuses platform routes with a stable error | `platform_extractor.rs:76-81`, `routes.rs:81-86` | none | **FAIL** — FIND-003-2 |
| The resulting credential authenticates and yields a platform-scope context | `platform_sessions.rs`; `routes.rs:62-92` | `platform_admin_e2e.rs:56-76, 97` | PASS |
| Constraint: initialization is one transaction | violated — `init.rs:78-99` | — | **FAIL** — FIND-003-1 |
| Constraint: plaintext to stdout only, never logged/traced/audited | `init.rs:75` `skip(pool)`; `SecretString` return | inspection | PASS |
| Constraint: local development stays one command | `mise.toml:1112` `cli:init`; `dev bootstrap` untouched | — | PASS |

### TASK-004 — Tenant provisioning and bootstrap-key removal

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| One authorized creation yields a ready tenant, admin principal, seeded roles, returned credential usable with no further platform call | `provisioning.rs:120-247` | `platform_admin_e2e.rs:81-140` | PASS |
| An injected failure at each stage leaves no usable tenant, is observable as failed, and a retry converges on one correct tenant | partial — `mark_tenant_failed` at `provisioning.rs:172`; no resume path | none | **FAIL** — FIND-004-3, FIND-004-4, FIND-004-7 |
| Concurrent creation for the same identity yields exactly one tenant and one admin principal | `platform.tenants.slug UNIQUE`; `slug_or_store` `provisioning.rs:256` | none | **FAIL** — FIND-004-4 (mechanism plausible, unproven) |
| A non-ready tenant is invisible to the live-tenant directory read **and refuses authenticated tenant operations** | directory half only — `queries/platform/tenants.rs:28-36`, `resolve_tenant_by_slug` | none | **FAIL** — FIND-004-1 |
| Suspending stops principals authenticating and stops live tokens at the epoch; resuming restores access | absent — `set_tenant_suspended` has no caller | none | **FAIL** — FIND-004-2 |
| A tenant-scope credential invoking any platform operation is refused without revealing the directory | `platform_extractor.rs`; router placement outside `/v1` | `platform_admin_e2e.rs:178-206` | PASS |
| No reference to `bootstrap-key`, `bootstrap-admin`, `SYSTEM_OPERATOR_ID` remains | `boot/bootstrap.rs`, `tests/pg_bootstrap_key.rs`, `cli:bootstrap-key` all deleted | repo-wide grep: one explanatory mention at `boot/init.rs:4` | PASS (see FIND-004-8, cosmetic) |
| Documentation describes install → initialize → create tenant → configure | none — `docs/` untouched | none | **FAIL** — FIND-004-5 |
| REQ-036 typed HTTP contract with stable errors and **generated artifacts** | typed bodies `wyrd-spec/src/auth/tenant_admin.rs`; no OpenAPI paths | `openapi.yaml` has no `/platform` path | **FAIL** — FIND-004-6 |
| AC-002 three-command journey through the shipped SDK **and CLI** | no CLI; journey calls the Rust fn and raw HTTP | `platform_admin_e2e.rs:90` | **FAIL** — FIND-004-5 |
| Tenant directory writes on the operator boundary, tenant-scope under RLS | `provisioning.rs:86-88, 128-149, 192-241` | `pg_admin_principals` | PASS |

### TASK-005 — Tenant administration and service principals

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| A tenant admin credential configures its tenant, creates a machine principal, grants a narrower role set, with no platform call | `principals/routes.rs:132-183` | `platform_admin_e2e.rs:364-386` | PASS |
| The restricted principal performs its granted operations and is refused tenant administration | `require_principal_admin` `routes.rs:57-69` | `platform_admin_e2e.rs:388-406` | PASS |
| Tenant A cannot read, mutate or authenticate against tenant B's principals, roles or configuration; denial reveals nothing | RLS + composite FK `(data_tenant_id, principal_id)`; `tenant_conn` derives tenancy from the verified caller only | none for this surface | **FAIL** — FIND-005-2 (mechanism sound, unproven for these routes) |
| No tenant-plane operation can create a platform principal or confer platform authority | structural — `platform.principals` is reachable only from `OperatorPool`, never from `TenantConn` | none | PASS (structural), FIND-005-2 for proof |
| Each covered decision appends its audit row in the deciding transaction, allowed and denied; an injected audit-append failure refuses and commits nothing | **absent** — no audit anywhere in `components/principals/` | none | **FAIL** — FIND-005-1 |
| The module's recorded no-audit stance is corrected | `components/admin/routes.rs:15-17` unchanged | — | **FAIL** — FIND-005-1 |
| A suspended tenant or principal is refused on every operation on this surface | principal status is checked at exchange (`exchange_api_key.rs:228`); tenant status is checked nowhere | none | **FAIL** — FIND-004-1 |
| Project the operations through the CLI | none — `wyrd-cli` untouched | none | **FAIL** — FIND-005-3 |
| Roles fail closed when unknown | `routes.rs:162-170` → `WyrdError::Validation` | — | PASS |

### TASK-006 — Credential lifecycle and administrative recovery

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Credentials listable as metadata; no path returns an existing plaintext | `queries/auth/api_keys.rs:62-81`; `routes.rs:212-227` | `platform_admin_e2e.rs:439-463`; `pg_admin_principals::listing_returns_metadata_and_never_plaintext` | PASS |
| Rotation — issue B, verify B, revoke A — uninterrupted, stops A immediately, authorization unchanged | `routes.rs:194-204`, `:236-256` | `platform_admin_e2e.rs:408-493` | PASS |
| Revoking one credential leaves the principal's others working | `REVOKE_API_KEY_SQL` targets one id | `platform_admin_e2e.rs:490-493` | PASS |
| A tenant admin with zero usable credentials is restored against the same principal id, grants unchanged, no second principal | `recovery.rs:69-130`; `tenant_admin_principal_id` | `platform_admin_e2e.rs:230-289` | PASS |
| Recovery appears in audit as a distinct named capability attributable to the global principal and its credential | `recovery.rs:75-83` with `Permission::tenant_recover_admin()` and `Some(caller.credential_id)` | `platform_authz.rs` pg_tests (generic); none naming recovery | PASS (implementation), weak proof |
| Recovery grants the global principal no further access | `init.rs:51-58` grant excludes every tenant permission | `platform_admin_e2e.rs:208-224`; `boot/init.rs:110-133` | PASS |
| **A revoked credential's live tokens stop verifying at the epoch** | **absent** — `revoke_api_key` does not touch `auth_service_accounts.tokens_not_before` | test proves only that the revoked credential cannot mint a *new* token | **FAIL** — FIND-006-1 |
| An injected audit-append failure refuses the operation and commits nothing | absent on this surface | none | **FAIL** — FIND-005-1 |
| Project the operations through the CLI; document rotation and credential-loss recovery | none | none | **FAIL** — FIND-005-3, FIND-004-5 |

### Non-goals and invariants — all held

| | |
|---|---|
| No `Principal`/`Credential`/`Tenant` Card kind | PASS |
| No billing, plans, quotas, `Organization`, tenant deletion, signup UI | PASS |
| No compatibility route, alias, legacy name or migration shim | PASS |
| No human identity / OIDC work leaked in from TASK-007 | PASS |
| INV-001 authorization never attached to a credential record | PASS — grants live in `platform.principal_grants` / `auth_service_account_roles` |
| INV-002 verifier-only persistence, no plaintext in any durable or diagnostic surface | PASS |
| INV-003 credential establishes only its own scope | PASS — tenant derives from the verified principal at `routes.rs:82` |
| INV-004 / INV-004a planes never collapse | PASS |
| INV-010 attributable to a real principal | PASS — `SYSTEM_OPERATOR_ID` gone |
| INV-013 revocation advances the epoch | **FAIL** — FIND-006-1 |
| No unrelated change entered the diff | PASS |

## Material findings

### FIND-003-1 — `INCORRECT` — initialization is not one transaction and can permanently brick a deployment

**Violated obligation**: REQ-021 ("Initialization MUST, in one transaction:
create the global administrative principal, generate its initial credential,
persist only the verifier, grant its administrative authorization…"), REQ-023
("A failed initialization MUST leave the deployment uninitialized and retryable,
never a global administrative principal with no usable credential"), INV-005,
TASK-003 constraint "Initialization is one transaction" and its acceptance
criterion "An injected failure at each stage leaves the deployment uninitialized
and a later invocation succeeds cleanly".

**Location**: `crates/wyrd/wyrd-server/src/boot/init.rs:76-101`.

**Evidence**: three independent statements are issued against `&OperatorPool`
with no transaction: `insert_platform_principal(pool, …)` at line 78,
`set_platform_grant(pool, …)` at line 95, and
`PlatformCredentials::new(pool.clone()).issue(…)` at line 97. The query slot's
own rustdoc discloses this —
`crates/wyrd/wyrd-sql/src/queries/platform/principals.rs:44-47`: "Each of those
is a separate statement on the operator pool; the single-initialization
invariant is carried by the durable uniqueness of `name`, not by this call."

**Falsifying scenario**: an operator runs `wyrd-server init`. The principal row
commits. The credential insert then fails — a dropped connection, a pool
timeout, statement timeout, `SIGINT` on the operator's terminal, or the
container being evicted. The deployment now holds a `platform-admin` principal
with no credential, and `platform.principals.name` is taken. Every retry takes
the `SqlError::UniqueViolation` arm at line 87 and returns
`InitError::AlreadyInitialized`. There is no application-level path to a
credential — `/auth/platform/token` needs one to exist — and there is no route
to create one, because platform credential issuance is reachable only from
`initialize_platform_root`. The deployment is permanently unusable and the only
remedy is the out-of-band SQL this entire change exists to abolish. The same
window exists between the principal insert and the grant write, producing a root
that authenticates and is authorized for nothing.

**Required correction**: the principal row, its grant, and its credential must
commit or roll back together, so a failed initialization leaves no
`platform.principals` row and the next invocation starts clean. Proof must
inject a failure at each of the three stages and then show a subsequent
invocation succeeding.

---

### FIND-003-2 — `MISSING` — three TASK-003 acceptance criteria have no test at any tier

**Violated obligation**: TASK-003 acceptance criteria and AC-001.

**Location**: `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs`.

**Evidence**: the file has one initialization negative,
`initialization_happens_at_most_once` (line 144), which calls
`initialize_platform_root` twice sequentially. Absent entirely:

1. **Concurrency.** AC-001 and the task both require that concurrent invocation
   converge on one principal with the loser refusing. The sequential test does
   not exercise the race; two `now_v7` ids are generated before either insert, so
   the property being claimed is a Postgres unique-index race, which only a
   concurrent test observes.
2. **Staged failure and retryability.** No test injects a failure. This is the
   criterion that would have caught FIND-003-1.
3. **No credential material on server start.** No test captures server output
   across a boot, before or after initialization, under any profile.

Additionally, AC-002's "through the shipped SDK and CLI" is not met for
initialization: the journey calls `initialize_platform_root` directly as a Rust
function (line 90), so the `wyrd-server init` subcommand — the operator-facing
surface REQ-020 names — is never executed by any test.

**Consequence**: three acceptance criteria are asserted by construction only, and
one of them is false.

**Required correction**: cover concurrent initialization, per-stage failure
followed by successful retry, and the absence of credential material in captured
server output. Drive at least one path through the `init` subcommand rather than
the library function.

---

### FIND-003-3 — `DRIFT` — `InitError::NotConfigured` is unconstructible dead code

**Violated obligation**: Ponytail step 1 — deletable while preserving the task.

**Location**: `crates/wyrd/wyrd-server/src/boot/init.rs:35-37`.

**Evidence**: repo-wide grep finds the identifier only at its own declaration.
`main.rs:75-79` handles the unconfigured case by constructing a
`std::io::Error::other` carrying a hand-copied duplicate of the variant's
`#[error]` string, so the message now exists twice with no shared owner.

**Required correction**: delete the variant, or construct it at the one site
that needs it and drop the duplicated literal.

---

### FIND-004-1 — `INCORRECT` — tenant lifecycle status is never enforced on the authentication or authorization path

**Violated obligation**: REQ-026 ("A tenant MUST NOT be reachable by any
authenticated tenant operation, background sweeper, or directory consumer
servicing live tenants until it is ready. A tenant whose provisioning failed MUST
be observable as failed and MUST NOT be usable"), REQ-028 ("A suspended tenant
MUST refuse authentication and authorization for its principals"), INV-006,
TASK-004 acceptance criterion "A non-ready tenant is invisible to the live-tenant
directory read **and refuses authenticated tenant operations**", TASK-005
acceptance criterion "A suspended tenant or principal is refused on every
operation on this surface".

**Location**: `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:222-230`;
`crates/wyrd/wyrd-sql/src/tenant_conn.rs` `TenantConn::acquire`.

**Evidence**: the credential exchange checks the *principal's* status (`if
row.status != "active"` at line 228, reading
`ApiKeyLookupRow.status`, which is `wyrd.auth_service_accounts.status`) and the
credential's own lifecycle. It never reads `platform.tenants.status`.
`TenantConn::acquire` sets `app.current_tenant` and returns; it performs no
directory lookup. No middleware, extractor, or handler on the tenant plane
consults the tenant's lifecycle state. The change added three new tenant states
and wired none of them to a refusal.

The migration's own comment asserts the opposite —
`migrations/20260601000022_tenant_lifecycle.sql:52-54`: "The live-tenant index
already excludes everything that is not active, so a provisioning or failed
tenant is invisible to sweepers and directory reads without any consumer
change." That is true of `list_active_tenant_ids` and
`platform.resolve_tenant_by_slug`, and only of those. Neither is on the
credential-exchange path, because a Wyrd key carries its tenant id in the prefix
and never resolves by slug.

**Falsifying scenario A (suspension, REQ-028)**: provision tenant `acme`; take
its admin credential; then set `platform.tenants.status = 'suspended'` — which
is what any suspension route would do, and what `set_tenant_suspended` already
implements. `POST /auth/token` with that credential still returns 200 and the
resulting token authorizes every tenant operation. Suspension freezes nothing.

**Falsifying scenario B (failed provisioning, REQ-026/INV-006)**: provisioning
reaches `establish_tenant_administration` and commits the tenant-scope
transaction — admin principal, roles, grant, credential all durable — and then
`mark_tenant_active` at `provisioning.rs:158` fails on a transient operator-pool
error. The handler returns `ProvisionError::Store`, `mark_tenant_failed` flips the
row to `failed`, and the caller sees a 500. The tenant is now observable as
`failed`, yet every row needed to use it exists. Any holder of a credential minted
inside that tenant — including one recovered later through
`/platform/tenants/admin/credentials`, which also performs no status check —
authenticates into a tenant the directory reports as failed. "MUST NOT be usable"
does not hold.

**Consequence**: the lifecycle states added by this change are advisory
decoration on the directory row. The invariant `INV-006` claims is not enforced
anywhere.

**Required correction**: a tenant that is not `active` must refuse credential
exchange and must refuse to yield an authenticated tenant context, fail-closed,
with the single indistinguishable invalid-credential error (INV-012) rather than
a state-revealing one. Prove it for `provisioning`, `failed`, and `suspended`,
and prove that resuming restores access with grants intact.

---

### FIND-004-2 — `MISSING` — no tenant listing, inspection, suspension, or resumption

**Violated obligation**: REQ-028 ("A global administrative principal MUST be able
to list tenants, inspect one tenant's state, and suspend and resume a tenant"),
TASK-004 approach step 5 and its acceptance criterion on suspension, AC-008.

**Location**: `crates/wyrd/wyrd-server/src/components/platform/routes.rs:34-41`.

**Evidence**: the platform router exposes exactly two routes —
`POST /platform/tenants` and `POST /platform/tenants/admin/credentials`. There is
no `GET /platform/tenants`, no per-tenant read, and no suspend/resume entry.
`set_tenant_suspended`
(`crates/wyrd/wyrd-sql/src/queries/platform/provisioning.rs:105-127`) is fully
written, correct, and has **zero callers** — grep finds only its own definition.
`Permission::tenant_read()` and `Permission::tenant_suspend()` are granted to the
root at `boot/init.rs:54-55` and `boot/init.rs:110-133` asserts they are granted,
but no route ever requires either. The permissions and the query exist; the
capability does not.

**Falsifying scenario**: an operator holding the root credential wants to freeze a
compromised tenant. There is no surface to do it. A test that asserts the root
*holds* `tenant_suspend` therefore proves a grant, not a capability — which is
the distinction REQ-019 draws.

**Required correction**: expose tenant listing, single-tenant inspection, and
suspend/resume on the platform plane, authorized on the permissions already
granted and audited through `PlatformAuthorization` like the two existing routes.
Prove suspension end to end against FIND-004-1's refusal, including that resume
restores access with state and grants intact.

---

### FIND-004-3 — `INCORRECT` — a failed provisioning permanently burns the slug; retry never converges

**Violated obligation**: REQ-027 ("Provisioning MUST be transactional where one
transaction suffices and otherwise **resumable to the same outcome**. Retried or
concurrent creation for the same requested identity MUST converge on one tenant
with one tenant administrative principal… Retry behavior MUST be explicit in the
contract and tested"), TASK-004 constraint and acceptance criterion "a retry
converges on exactly one correct tenant", AC-007.

**Location**: `crates/wyrd/wyrd-server/src/components/platform/provisioning.rs:139-177`;
`crates/wyrd/wyrd-sql/src/queries/platform/provisioning.rs:27-44`.

**Evidence**: `insert_provisioning_tenant` is a bare `INSERT` with no
`ON CONFLICT`. `platform.tenants.slug` is `TEXT UNIQUE NOT NULL`
(`migrations/20260601000000_platform.sql:54`). `provision` has no branch that
recognises an existing `provisioning` or `failed` row for the requested slug and
continues from it. The rustdoc claims the opposite —
`queries/platform/provisioning.rs:73-75`: "Leaves the tenant visibly incomplete
with the reason attached, so an operator can see why it never became usable **and
a retry knows it is resuming rather than starting fresh**." Nothing resumes.

**Falsifying scenario**: `POST /platform/tenants {"slug":"acme"}`. The directory
row commits. `establish_tenant_administration` fails — Postgres restart, Argon2
task panic, RLS pool exhaustion. `mark_tenant_failed` flips the row to `failed`
and the caller gets a 500. The operator retries the identical request. The insert
hits the unique constraint, `slug_or_store` maps it to `ProvisionError::SlugTaken`
(`provisioning.rs:256-260`), and the caller receives
`WyrdError::Conflict {"field":"slug"}` — forever. The tenant `acme` can never be
created on this deployment. Convergence "on exactly one correct tenant" is
falsified: the system converges on one permanently broken tenant and a
permanently unusable slug, with no application-level remedy.

Note that the two halves of REQ-027 are separable and the *concurrent* half is
fine: a race genuinely converges, because the loser's `SlugTaken` is correct when
the winner produced a working tenant. Only the retry-after-failure half is
broken.

**Required correction**: a retry for the same requested identity against a
`provisioning` or `failed` row must resume to the same outcome — one tenant, one
administrative principal, no duplicate and no orphaned credential — rather than
returning a conflict. The retry contract must be explicit in the HTTP contract and
tested at each failure stage.

---

### FIND-004-4 — `MISSING` — no provisioning failure, retry, or concurrency test

**Violated obligation**: AC-007 ("Provisioning-failure and retry evidence proves
an injected failure at each stage produces no usable tenant, and that retry and
concurrent creation converge on exactly one tenant and one administrative
principal"); TASK-004 verification ("Provisioning failure, retry, and concurrency
coverage is Postgres-backed integration proof").

**Location**: `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs`;
`crates/wyrd/wyrd-sql/tests/pg_admin_principals.rs`.

**Evidence**: neither file contains a failure-injection, retry, or concurrency
test for provisioning. The nine `pg_admin_principals` tests cover the durable
principal/credential/grant model; none touches `platform.tenants` lifecycle. The
five journeys cover only the happy path plus two cross-plane negatives.

**Consequence**: the task's single highest-risk property — what a partially
failed two-boundary operation leaves behind — is entirely unproven. FIND-004-1
and FIND-004-3 are both defects this coverage would have caught.

**Required correction**: inject a failure at each provisioning stage (directory
commit, role seeding, principal insert, grant, credential insert, promotion) and
assert the resulting state is not usable and that a retry converges; drive
concurrent creation for one slug and assert exactly one tenant and one
administrative principal.

---

### FIND-004-5 — `MISSING` — no CLI projection and no documentation

**Violated obligation**: REQ-040, REQ-036, AC-002; TASK-004 approach steps 4 and
7 and its acceptance criterion "Documentation describes install → initialize →
create tenant → configure, with no step requiring database access"; TASK-005
approach step 5; TASK-006 approach steps 5 and 6.

**Location**: `crates/wyrd/wyrd-cli/` and `docs/src/content/docs/self-hosting/`
are untouched by the entire 96-file diff.

**Evidence**: `git diff --name-only $BASE..HEAD -- docs/ crates/wyrd/wyrd-cli/`
returns nothing. `crates/wyrd/wyrd-cli/src/` has no tenant module and
`wyrd-cli/src/principal/` gained no administrative commands. TASK-004 names
`crates/wyrd/wyrd-cli/` as relevant surface for "the tenant creation command" and
TASK-005 names `crates/wyrd/wyrd-cli/src/principal/`. TASK-004 and TASK-006
verification both name `mise run docs:check`, which currently has nothing to
check.

The spec's headline promise is the three-command operator journey — `start the
server`, `wyrd-server init`, `wyrd tenant create --name acme`. The second command
exists; the third does not, and neither is documented.

**Scope note**: TASK-008 owns SDK, MCP, and cross-surface documentation
reconciliation. It does **not** own the CLI, which TASK-004/005/006 each name in
their own relevant surface, approach and acceptance, nor TASK-004's self-hosting
rewrite. This is not deferred work.

**Required correction**: project tenant creation and tenant principal/credential
administration through the CLI, and rewrite the self-hosting documentation to the
three-command journey including rotation and credential-loss recovery.

---

### FIND-004-6 — `MISSING` — the new HTTP routes are absent from the generated contract

**Violated obligation**: REQ-036 ("available headlessly over the
language-agnostic HTTP contract with typed bodies, stable `WyrdError` codes, and
**generated artifacts**"); AGENTS.md §9.

**Location**: `crates/wyrd/wyrd-server/src/http/openapi.rs:14-41`.

**Evidence**: `WyrdApiDoc` registers fifteen paths, none of them new. No handler
in `components/platform/` or `components/principals/` carries `#[utoipa::path]`
(grep returns nothing). `openapi.yaml` contains zero occurrences of `platform` or
`principals`. The wire types in `wyrd-spec/src/auth/{tenant_admin,
tenant_principals}.rs` do derive `utoipa::ToSchema`, so the schemas exist and are
simply never referenced.

**Consequence**: a language-agnostic client cannot discover the administrative
contract from the generated document — the exact property REQ-036 exists to
guarantee. `mise run codegen:check` passes vacuously, so the gap is silent rather
than caught.

**Required correction**: annotate the six handlers, register their paths and
component schemas, and regenerate so `codegen:check` is meaningful for this
surface.

---

### FIND-004-7 — `INCORRECT` — `mark_tenant_failed` is best-effort and unreachable under cancellation

**Violated obligation**: REQ-026 ("A tenant whose provisioning failed MUST be
observable as failed"); TASK-004 acceptance criterion "is observable as failed".

**Location**: `crates/wyrd/wyrd-server/src/components/platform/provisioning.rs:171-175`.

**Evidence**: the failure arm is `let _ = mark_tenant_failed(...).await;` — the
result is discarded. It is also an ordinary `.await` inside the axum handler
future with no `spawn`, no cancellation guard, and no `Drop` fallback.

**Falsifying scenario**: the client disconnects while `establish_tenant_administration`
is running — a CLI `^C`, an ingress read timeout, an SDK deadline. Axum drops the
handler future, so neither the `Err` arm nor `mark_tenant_failed` ever runs. The
row stays `provisioning` forever. A second path reaches the same state: the
operator pool is the very thing that just failed, so `mark_tenant_failed` is
likely to fail too, and its error is swallowed. Either way the tenant is
observable as `provisioning`, not `failed`, with no reason recorded — and, by
FIND-004-3, its slug is burned.

**Required correction**: a provisioning attempt that does not complete must
become observable as failed, including when the request is cancelled. Resolving
FIND-004-3 with a resumable retry may subsume this — a stale `provisioning` row
that a retry adopts is no longer a stuck state — so correct the two together
rather than adding a separate guard.

---

### FIND-004-8 — `DRIFT` (cosmetic) — a removed name survives in a doc comment

**Violated obligation**: TASK-004 acceptance criterion "No reference to
`bootstrap-key`, `bootstrap-admin`, or `SYSTEM_OPERATOR_ID` remains in code,
tasks, tests, or documentation"; AGENTS.md preamble on legacy names.

**Location**: `crates/wyrd/wyrd-server/src/boot/init.rs:4`.

**Evidence**: repo-wide grep over `.rs`, `.toml`, `.md`, `.mdx`, `.sql`, `.ts`,
`.py`, excluding `changes/`, returns exactly one hit: "It replaces the removed
`bootstrap-key` path, which…". Everything else — the module, the test, the
`mise` task, the fabricated CardRef, `SYSTEM_OPERATOR_ID` — is genuinely gone.

**Required correction**: state what initialization does without naming the
removed command; the historical note belongs in the completed-change record.

---

### FIND-005-1 — `MISSING` — the tenant principal and credential surface writes no audit at all

**Violated obligation**: REQ-037 ("Every authorization decision made by these
operations MUST append its audit row in the same transaction as the decision, for
allowed and denied alike, naming the principal, the credential that authenticated
the request, the permission, the resource, the tenant where applicable, and the
outcome. A decision whose audit cannot be recorded MUST fail closed"); AC-009;
INV-011; TASK-005 constraint, approach step 3 and acceptance criterion; TASK-006
constraint and acceptance criterion; AGENTS.md §2 and
`architecture/agent-rules.md`.

**Location**: `crates/wyrd/wyrd-server/src/components/principals/routes.rs` —
`require_principal_admin` at lines 57-69, called at 137, 199, 217, 241;
`crates/wyrd/wyrd-server/src/components/admin/routes.rs:15-17`.

**Evidence**: grep for `audit` across `components/principals/` returns nothing.
`require_principal_admin` is a pure in-memory `PermissionSet::contains` with an
early return on failure; no transaction is opened for the decision, no row is
appended for an allowance or a denial, and no path fails closed on an
unrecordable audit. The four decisions govern the creation of durable principals,
the minting of credentials, the reading of credential metadata, and revocation —
precisely the operations AC-009 says audit must be able to attribute.

`components/admin/routes.rs:15-17` still reads: "No audit row is written here,
and that is deliberate: admin-mutation audit is deferred to the audit→Vala/Iceberg
consolidation…". TASK-005 approach step 3 required correcting exactly this
stance and the spec lists it under *Required architecture amendments*. It is
unchanged.

The platform plane got this right. `wyrd-auth/src/platform_authz.rs:110-137`
begins a transaction, appends the decision, rolls back and returns
`AuditUnavailable` when the append fails, commits denials durably, and hands the
open transaction to the caller so decision and effect commit together. That is
the owner and the pattern this surface should have reused; `PlatformAuthorization`
is bound to `OperatorPool`, so the tenant-plane equivalent needs the same shape
over `TenantConn`.

**Falsifying scenario**: a tenant administrator creates a principal granted
`admin`, mints it a credential, and that credential is later used to exfiltrate
the tenant's Cards. Audit can name the *Card reads*, but nothing records who
created the principal, which credential authenticated that request, or that the
decision was permitted. AC-009's requirement that audit "state which principal,
using which credential, performed which operation, against which tenant, at what
time" cannot be satisfied for the four operations that establish tenant
authority. Separately, a denied `require_principal_admin` — the escalation
attempt in the rotation journey at `platform_admin_e2e.rs:391-406` — leaves no
record at all, so the deny half of REQ-037 is unmet too.

**Required correction**: every authorization decision on this surface appends its
row in the transaction that performs the operation, for allowed and denied alike,
naming principal, authenticating credential, permission, resource, tenant, and
outcome; an unrecordable audit refuses and commits nothing. Retire the recorded
no-audit stance in `components/admin/routes.rs`. Prove both outcomes and the
injected-audit-failure refusal.

---

### FIND-005-2 — `MISSING` — no cross-tenant or escalation negative coverage for this surface

**Violated obligation**: AC-004, AC-017, INV-007, INV-004b; TASK-005 acceptance
criteria on cross-tenant denial and platform-escalation, and its verification
("Cross-tenant and escalation negative coverage… run as Postgres-backed
integration proof").

**Location**: `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs`.

**Evidence**: the only cross-boundary negatives in the suite are the two
cross-*plane* assertions at lines 178-224. No test drives tenant A's credential
at tenant B's principals, credentials, or roles. No test attempts to create a
platform principal or confer platform authority from the tenant plane.

The underlying mechanisms are sound — RLS with `FORCE`, the composite FK
`(data_tenant_id, principal_id) → wyrd.auth_service_accounts`, and `tenant_conn`
deriving tenancy from `caller.data_tenant_id` only, never from the request — and
`platform.principals` is structurally unreachable from `TenantConn`. But TASK-005
names this coverage explicitly, and these are exactly the properties AGENTS.md
§11 says a journey must cover rather than leave to structural argument.

**Required correction**: prove that a tenant A credential is refused against
tenant B's principals and credentials with a denial that reveals nothing about
tenant B, and that no tenant-plane operation — principal creation or role grant —
can create a platform principal or confer platform authority.

---

### FIND-005-3 — `MISSING` — tenant administration is not projected through the CLI

Folded into **FIND-004-5**; recorded here so TASK-005's own approach step 5 and
relevant surface (`crates/wyrd/wyrd-cli/src/principal/`) are not lost.

---

### FIND-005-4 — `INCORRECT` — a duplicate principal name returns 500 instead of a conflict

**Violated obligation**: REQ-036 (stable `WyrdError` codes); AGENTS.md §9.

**Location**: `crates/wyrd/wyrd-server/src/components/principals/routes.rs:149-159`,
mapping through `internal()` at line 271.

**Evidence**: `wyrd.auth_service_accounts` carries `UNIQUE (data_tenant_id,
name)` (`migrations/20260601000001_auth.sql:82`), and
`CreateServicePrincipalRequest.name` is documented as "unique within the tenant"
(`wyrd-spec/src/auth/tenant_principals.rs:17`). A second
`POST /v1/principals {"name":"ci-runner"}` therefore raises a unique violation
that `insert_service_account` surfaces and `internal()` renders as
`WyrdError::Internal`, HTTP 500, with the raw database error string in `details`.

**Falsifying scenario**: a CI pipeline re-runs its bootstrap step. It receives a
500 with a Postgres constraint message instead of a caller-correctable conflict,
and cannot distinguish "name taken" from a real outage. The platform plane
already does this correctly: `slug_or_store`
(`provisioning.rs:256-260`) classifies the same class of violation as
`ProvisionError::SlugTaken` → `WyrdError::Conflict`.

**Required correction**: classify the unique violation on this surface the way the
platform plane already does, reusing the existing `SqlError::UniqueViolation`
discrimination rather than adding a second mechanism.

---

### FIND-006-1 — `INCORRECT` — revoking a credential does not stop the tokens it minted

**Violated obligation**: INV-013 ("Revoking a credential, principal, or role grant
advances the applicable authorization epoch transactionally; no token or cached
permission set outlives the earlier of its expiry or that epoch"); REQ-010;
AC-005; AC-010; TASK-006 constraint "Revocation advances the applicable
authorization epoch transactionally" and its acceptance criterion "A revoked
credential's live tokens stop verifying at the epoch".

**Location**: `crates/wyrd/wyrd-sql/src/queries/auth/api_keys.rs:11-15`
(`REVOKE_API_KEY_SQL`); `crates/wyrd/wyrd-server/src/components/principals/routes.rs:236-256`.

**Evidence**: `REVOKE_API_KEY_SQL` sets `wyrd.auth_api_keys.revoked_at` and
nothing else. The authorization epoch the verifier consults is
`wyrd.auth_service_accounts.tokens_not_before`, read by
`service_account_revocation_epoch`
(`crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs:44-60`) and routed through
`SqlRevocationCheck` (`wyrd-auth/src/revocation_resolver.rs:113-133`). The revoke
path never writes `tokens_not_before`, and `revoke_credential` performs no other
write. The epoch machinery exists and is correctly wired for *principal*
revocation; credential revocation is simply not connected to it.

**Falsifying scenario**: an operator discovers credential A has leaked. They call
`DELETE /v1/principals/{id}/credentials/{a}` and the route returns 204. The
attacker already exchanged A for an access token — default TTL, unexpired. Every
request bearing that token continues to authorize normally until natural expiry,
because verification consults `tokens_not_before`, which was never advanced. The
operator believes revocation was immediate; it was not.

**Why the existing proof does not cover this**: the rotation journey at
`platform_admin_e2e.rs:485-489` asserts
`tenant_token(&srv, &first).await.unwrap_err() == UNAUTHORIZED` — that the
revoked credential can no longer *mint a new token*. That is a strictly weaker
property than the one the acceptance criterion states, and the test's message
("the retired credential stops working") reads as though the stronger property
were established. A token minted from `first` before the revocation is never
constructed, so the failing case is never exercised.

**Required correction**: revoking a credential must advance the owning
principal's authorization epoch in the same transaction as the revocation, so a
token minted from the revoked credential stops verifying immediately. Prove it by
minting a token from A, revoking A, and asserting that the *already-held* token is
refused — not merely that A cannot mint another. Reuse the existing
`tokens_not_before` epoch and `revocation_resolver` machinery; do not add a second
revocation mechanism.

---

### FIND-006-2 — `INCORRECT` — revocation ignores the principal in its own path

**Violated obligation**: REQ-031 ("Every tenant-scoped operation MUST verify that
the target resource belongs to the authenticated principal's tenant") read
together with the route's own contract; INV-011 fail-closed on ambiguity.

**Location**: `crates/wyrd/wyrd-server/src/components/principals/routes.rs:239-245`.

**Evidence**: the handler destructures `Path((_principal_id, credential_id))` and
discards the first element; `revoke_api_key` matches on `id` alone. The route is
`/principals/{principal_id}/credentials/{credential_id}`, so the URL asserts a
relationship the handler never checks.

**Falsifying scenario**: a tenant administrator, or automation holding
`service_accounts:write`, issues
`DELETE /v1/principals/{ci-runner}/credentials/{cred-of-payments-service}`. The
credential belongs to a different principal in the same tenant. It is revoked and
204 is returned, while the audit trail — once FIND-005-1 is fixed — would record
the wrong principal as the target. Tenant isolation holds (RLS bounds the blast
radius to one tenant), so this is a correctness and attribution defect rather than
a tenancy breach.

**Required correction**: revocation resolves the credential within both the
authenticated tenant and the named principal, and a mismatch is a not-found
rather than a silent success.

---

### FIND-006-3 — `MISSING` — recovery does not require a usable target tenant

**Violated obligation**: REQ-026/REQ-028 read with REQ-032; INV-011 fail-closed.

**Location**: `crates/wyrd/wyrd-server/src/components/platform/recovery.rs:69-101`.

**Evidence**: `recover` authorizes `tenant_recover_admin`, commits the decision,
then opens `TenantConn::acquire(&self.app, tenant_id)` on the caller-supplied
`tenant_id` and mints a credential. It never reads `platform.tenants.status`. A
`suspended` tenant yields a working credential — and, by FIND-004-1, that
credential then authenticates. A `provisioning` or `failed` tenant with a
committed admin principal does too.

Taking `tenant_id` from the request body is correct here — this is REQ-018's
explicitly named platform-plane exception, not a tenant-scoped operation — so the
defect is the missing lifecycle gate, not the parameter.

**Required correction**: recovery refuses a tenant that is not usable, with a
stable error that reveals nothing about tenants the caller cannot otherwise see.

---

### FIND-006-4 — `MISSING` — no proof that an unrecordable audit refuses on the credential surface

Direct consequence of **FIND-005-1**: with no audit on
`components/principals/routes.rs`, TASK-006's acceptance criterion "An injected
audit-append failure refuses the operation and commits nothing" has nothing to
exercise. The platform plane's equivalent *is* proven, in
`wyrd-auth/src/platform_authz.rs` pg_tests. Recorded separately so TASK-006's own
criterion is not closed by TASK-005's fix without its own proof.

## Prior-finding closure

TASK-001 and TASK-002 review verdicts
(`changes/active/admin-principals/review/task-001/`, `task-002/`) are outside this
review's scope. Their remediation commits (`e551d5d`, `c3ef4b4`, `999cf1e`,
`91442c5`) are present in the range and none of their findings recurs in the
surfaces reviewed here.

## Verdict

`FIX_REQUIRED`.

Two findings are severe enough to block on their own. **FIND-003-1** makes a
crashed `wyrd-server init` permanently brick a deployment, which is the precise
outcome REQ-023 forbids and the one the change exists to eliminate.
**FIND-005-1** leaves the entire tenant principal and credential surface
unaudited, against an explicit repository rule, an explicit spec requirement, and
a task approach step that named the file to correct.

Three more are load-bearing: **FIND-004-1** makes the new tenant lifecycle states
advisory rather than enforced, so INV-006 does not hold; **FIND-004-3** means a
failed provisioning burns its slug forever; **FIND-006-1** means credential
revocation does not stop the tokens that credential minted, with an existing test
whose message suggests otherwise.

The remainder are bounded gaps — the absent suspend/list/inspect routes, the CLI
and documentation, the OpenAPI registration, and the missing failure, retry,
concurrency and cross-tenant coverage.

None of this requires changing approved behavior or an expensive-to-reverse
decision; every correction lies inside spec revision 6 and reuses an owner the
repository already has. Remediation task:
`TASK-003-006-R1-lifecycle-audit-and-atomicity.md` in this directory.

Findings: FIND-003-1, FIND-003-2, FIND-003-3, FIND-004-1, FIND-004-2,
FIND-004-3, FIND-004-4, FIND-004-5, FIND-004-6, FIND-004-7, FIND-004-8,
FIND-005-1, FIND-005-2, FIND-005-3, FIND-005-4, FIND-006-1, FIND-006-2,
FIND-006-3, FIND-006-4.
