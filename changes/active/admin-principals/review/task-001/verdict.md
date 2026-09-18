# Task review verdict — TASK-001 Principal and credential model

## Verdict

**FIX_REQUIRED**

Remediation task: `TASK-001-R1-close-verification-and-model-gaps.md` (same directory).

## Immutable subject

| Item | Value |
|---|---|
| Repository root | `/home/user/wyrd` |
| Branch | `claude/admin-principals-spec-qfsmjc` |
| Base (exclusive) | `a4883bf` |
| Candidate (inclusive) | `adbe971` |
| Approved spec | `changes/active/admin-principals/spec.md` — `SPEC-admin-principals` revision 6, status `approved` |
| Original task | `changes/active/admin-principals/tasks/TASK-001-principal-credential-model.md` |
| Working tree | Clean at `adbe971` at review start and review end; the one probe that wrote files (schema regeneration) was reverted with `git checkout --` and verified clean |
| Standards audit | `standards-review.md` (same directory) |

The candidate range contains seven specification/planning commits
(`be4a4a0`..`ea9e9ec`) and six implementation commits (`5118fa5`..`adbe971`).
The specification and task packet are approved authoring artifacts and were not
audited as implementation drift.

## Scope-vs-authority interpretation

The task's recorded "Implementation decisions" resolve the apparent `REQ-003` /
`REQ-042` tension by reading the principal *type* as fixing **scope** and grants
as fixing **authority**. This is sound and is not reviewer-invented latitude:
`REQ-041` states it verbatim — "Platform authority MUST be a grant held by a
principal, not a property of a principal type." Under that reading `REQ-003`'s
"human principals have exactly one tenant" governs the tenant-scope `User` kind,
and `REQ-042`'s tenant-free human platform principal is a platform-scope
principal holding a grant. No requirement is weakened.

It is also faithfully implemented at the durable layer:

- `platform.principals` has no tenant column at all and
  `CHECK (principal_kind = 'global_admin')`, so tenancy is structurally absent
  rather than nullable-and-checked.
- `wyrd.auth_service_accounts` has
  `CHECK (principal_kind IN ('tenant_admin','service','agent'))` over a
  pre-existing `data_tenant_id UUID NOT NULL REFERENCES platform.tenants`, so
  every tenant-scope kind carries exactly one tenant.
- `wyrd_runtime::Principal` keeps its required `tenant_id`, so a platform
  identity is not representable where a tenant identity is required, and
  `wire_kind_into_principal_kind` rejects `GlobalAdmin` outright.

One forward observation, **not** a finding: `platform.principals`'
`CHECK (principal_kind = 'global_admin')` and the `PrincipalKindTag::GlobalAdmin`
rustdoc ("Platform-scope **administrative** principal") leave `TASK-007`'s human
platform principal without a distinct kind. That is `TASK-007`'s obligation to
resolve; it is not owed by `TASK-001`.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| **AC-a** A principal persists and resolves with no credential; issuing, revoking, expiring credentials never mutates the principal or its grants (`REQ-001`, `INV-008`, `INV-009`) | `queries/platform/principals.rs`, `principal_grants.rs`; grants are a separate table keyed on `principal_id` | Re-run by reviewer: `pg_admin_principals::principal_persists_and_is_authorized_without_any_credential`, `…::credentials_are_independent_of_each_other_and_of_authority` — 9/9 passed under `with-test-postgres.sh` | **PASS** |
| **AC-b** One principal holds two valid credentials; revoking or expiring one leaves the other authenticating and authorization unchanged (`REQ-007`, `REQ-010`) | `platform.credentials` FK to `principals` with no uniqueness on `principal_id`; `revoke_platform_credential` is per-credential | Re-run: `credentials_are_independent_of_each_other_and_of_authority` asserts both live, revoke one, other still usable, grant intact | **PASS** |
| **AC-c** A global-administration principal cannot persist with a tenant (`REQ-003`) | Migration: `platform.principals` has no tenant column | Re-run: `platform_principals_have_no_tenant_column`, `platform_store_rejects_a_tenant_scope_kind` | **PASS** |
| **AC-d** A tenant-scope principal cannot persist without a tenant (`REQ-003`) | Pre-existing `data_tenant_id UUID NOT NULL REFERENCES platform.tenants` (`20260601000001_auth.sql:70`), unchanged | **No test in the candidate exercises this half.** The implementer's matrix maps `platform_principals_have_no_tenant_column` and `platform_store_rejects_a_tenant_scope_kind` to it; neither inserts a tenant-scope principal without a tenant. Reviewer confirmed the constraint by source inspection | **PASS (behaviour), evidence overstated** — recorded as a matrix caveat, not a finding, because the constraint is pre-existing and demonstrably present |
| **AC-e** A machine principal persists and holds a credential with no Card (`REQ-004`) | Migration drops `NOT NULL` on the five Card columns, adds `auth_service_accounts_card_binding_check` and `auth_service_accounts_kind_card_check`; `insert_service_account(card_ref: Option<&CardRef>)` | Re-run: `tenant_admin_principal_persists_without_a_card`, `tenant_admin_principal_cannot_bind_a_card`; reviewer independently dumped `pg_constraint` and confirmed all five constraints, including that the untouched `auth_service_accounts_card_kind_check` passes on NULL | **PASS** |
| **AC-f** An existing Card-bound Service or Agent principal keeps its card identity and resolves as before (`REQ-004`, task constraint "durable identity keys must survive") | `UNIQUE (data_tenant_id, principal_kind, card_kind, card_uid)` retained; `kind_card_check` still requires `card_kind='Agent'` for `agent` | `wyrd-auth-verify::into_verified_*` (11 passed), `wyrd-auth` `card_scope::pg_tests` (17 passed). **But** `ServiceAccountPrincipalRow.card_ref` was left non-optional against the now-nullable column | **FAIL** — FIND-TASK-001-5 |
| **AC-g** Credential creation returns the plaintext once; the stored row holds only the verifier plus non-secret metadata (`REQ-008`, `INV-002`) | **No implementation exists** for the platform store: no secret generation, no Argon2 hashing, no return-plaintext-once operation anywhere for `platform.credentials`. `grep -rn "wyrd_crypt\|Argon2" crates/wyrd/wyrd-sql/src/queries/platform/` returns only doc comments | The cited proof, `listing_returns_metadata_and_never_plaintext`, inserts its own literal `"$argon2id$v=19$stub$listed"` and then asserts that value `!=` a different literal and `starts_with("$argon2id$")`. Both assertions are true by construction of the test's own input | **FAIL** — FIND-TASK-001-4 |
| **AC-h** No read, list, log, trace, or error path returns or records the plaintext (`INV-002`) | `PlatformCredentialMetadataRow` carries no secret field; `list_platform_credentials` selects no secret column | `listing_returns_metadata_and_never_plaintext` (metadata shape half only) | **PASS** |
| **AC-i** Every invalid-credential condition — unknown, expired, revoked, wrong secret — returns one indistinguishable error (`INV-012`, `AC-010`) | Tenant plane: pre-existing `ExchangeApiKey` error contract, unchanged. Platform plane: only `is_usable()` and `Option<Row>`; no error surface exists | `every_invalid_condition_yields_an_unusable_credential` covers unknown/expired/suspended; revoked is covered elsewhere; **wrong secret is not covered at all**. Worse, the three pre-existing tenant-plane tests that *do* prove this (`api_key_invalid_reason_distinct_for_every_variant`, `hash_mismatch_rejected`, `revoked_key_rejected`) now **FAIL** because of this change | **FAIL** — FIND-TASK-001-1 |
| **AC-j** No `platform.users`/`roles`/`user_roles`/`api_keys` object remains that is neither part of the platform store nor removed (`REQ-039`) | Migration drops all four tables; `queries/platform/{users,roles,api_keys}.rs` deleted; `wyrd-sql/src/lib.rs` file registry updated | Re-run: `pg_migration::migrations_apply_and_are_idempotent` passed; its assertions were **strengthened** from one `platform.users` existence check to seven explicit present/absent checks | **PASS** |
| **REQ-002** Durable principal record carries id, type, optional tenant, name, status, created/updated times | `platform.principals` and the unchanged `wyrd.auth_service_accounts` both carry all fields | Schema introspection in `platform_principals_have_no_tenant_column`; reviewer's `pg_constraint` dump | **PASS** |
| **REQ-006** Credentials are principal-generic | `ALTER TABLE wyrd.auth_api_keys RENAME COLUMN sa_id TO principal_id`; `platform.credentials.principal_id` | `pg_admin_principals` suite; SQL-shape unit test in `service_accounts.rs` updated to assert `sa.id = k.principal_id` | **PASS (contract)** / **FAIL (regression)** — FIND-TASK-001-1 |
| **REQ-009** Credential record carries id, owning principal, verifier, prefix, created, optional expiry, optional revocation, last-use; listing returns metadata never the secret | `platform.credentials` columns; `PlatformCredentialMetadataRow`; `ALTER COLUMN expires_at DROP NOT NULL` on `wyrd.auth_api_keys` | `listing_returns_metadata_and_never_plaintext`; `api_key_by_prefix` widened to `(expires_at IS NULL OR expires_at > now())` | **PASS** |
| **REQ-011** Non-secret material resolves the principal and, for tenant-scope, the tenant, before secret verification | `platform.credentials.prefix UNIQUE` joined to the principal in one read; tenant plane keeps the tenant-prefixed key material | `platform_credential_by_prefix` returns `principal_id` + `principal_status` without touching the verifier | **PASS** |
| **INV-001** A credential is never an identity; authorization never attaches to a credential record | `platform.principal_grants` keyed on `principal_id`, not credential | `credentials_are_independent_of_each_other_and_of_authority` asserts the grant survives revocation | **PASS** |
| **INV-003** A credential authenticates only its own principal and establishes only its own scope | FK `credentials.principal_id -> principals.id`; `GlobalAdmin` rejected in `wire_kind_into_principal_kind` and `SqlRevocationCheck::epoch` | `into_verified_rejects_platform_scope_kind` | **PASS** |
| **INV-010** Every privileged operation is attributable to a real principal, never a synthetic identity | `SYSTEM_OPERATOR_ID` and `bootstrap-key` are `REQ-038`/`TASK-004`, correctly untouched here | N/A | **PASS (not owed)** |
| **INV-014** Administrative identity remains server-owned durable state | Nothing added to any Card kind, SDK, CLI, or UI | `PrincipalKindTag` gained variants but no `Principal` Card kind was introduced | **PASS** |
| **Constraint** `wyrd-spec` stays IO-free, async-free, PyO3-free | Only enum variants and two `const fn` added; no manifest change | `cargo test -p wyrd-spec --lib` | **PASS** |
| **Constraint** Platform scope through `&OperatorPool`, tenant scope under `TenantConn` RLS, no third abstraction, no hand-written tenant filters | Every `platform/*` fn takes `&OperatorPool`; every tenant fn takes `&mut TenantConn` | `check_tenant_isolation.py` **exits 1** on the new files for a different rule (raw query without marker) | **FAIL** — FIND-TASK-001-2 |
| **Constraint** Reuse the existing Argon2 hashing, lookup-prefix, expiry, revocation, `last_used_at`, single invalid-credential error, and `wyrd.audit_credential_issuance` seams; no parallel credential implementation | Tenant plane reuses them unchanged. Platform plane reuses *none* of them because no issuance path was built | See AC-g | **FAIL** — FIND-TASK-001-4 |
| **Constraint** `wyrd.auth_refresh_tokens` is the principal-generic precedent | `auth_api_keys.principal_id` now mirrors it | Reviewer confirmed by source comparison | **PASS** |
| **Non-goals** No routes, HTTP contracts, initialization, tenant provisioning, OIDC, role-grant management surfaces; no new RBAC engine, explicit deny, or permission-scope redesign | Diff contains none of these | `git diff --stat` shows no route, handler, CLI, or permission-vocabulary file | **PASS** |
| **Task Verification** Add a focused `mise` lane for this capability and run it | `mise.toml` untouched | No lane exists | **FAIL** — FIND-TASK-001-6 |
| **Task Verification / VER-006** `mise run codegen:check` when generated contracts move | Generated goldens not regenerated | Reviewer regenerated: 8 files drift | **FAIL** — FIND-TASK-001-3 |

## Repository-standards result

See `standards-review.md`. Independence limitation recorded there: no
subagent-spawning tool exists in this session, so the acceptance reviewer
performed the standards pass itself as a separate authority-mapped sweep. The
reviewer authored no commit in the candidate range, so independence from
implementation holds.

Standards result: **FAIL** on §9/§11 (tenant-isolation gate), §11/VER-006
(codegen drift), §11 (missing capability lane), §12 (targeted tests fail), §13
(git identity and forbidden AI co-author trailers), §15 (speculative scaffolding;
root-cause duplication), and §16 (missing `# Errors`; rustdoc asserting an
invariant the API cannot provide). PASS on every other mapped rule, including
the unwrap audit, Clippy, async-boundary, connection-abstraction, and
gate-circumvention rules.

## Verification limits

- `mise` is not installed in this container. Direct `cargo` invocations were
  substituted against the same toolchain, and `cargo test` replaced
  `cargo nextest`, which is unavailable. Postgres ran through
  `scripts/postgres/with-test-postgres.sh` with a locally started Docker daemon.
- The spec's **Verification scope** (`VER-001`..`VER-006`) is honoured. No broad
  aggregate was run or required. Workspace-wide compilation was not attempted
  and its absence is not reported as a gap. `wyrd-server`, `wyrd-testing`,
  `vala-*`, and the SDKs were not compiled.
- Two `wyrd-spec` lib failures (`vala::audit_detail::tests::audit_detail_golden_vectors_cover_all_variants_and_nested_values`,
  `query::tests::display_parse_roundtrip_for_small_queries`) were reproduced at
  the **base** commit `a4883bf` in a scratch worktree and are therefore
  pre-existing and out of scope under `VER-005`.
- Seven `wyrd-auth-verify` external-JWKS failures are the container's missing
  rustls crypto provider, confirmed environmental and out of scope under
  `VER-005`, as the implementer recorded.
- The three `wyrd-auth` `exchange_api_key::pg_tests` failures are **not** covered
  by `VER-005`: they sit squarely on the credential surface named in `VER-001`
  and are caused by this change's column rename.

## Material findings

### FIND-TASK-001-1 — REGRESSION — three existing credential tests broken by the `sa_id` rename

- **Violated obligation**: task acceptance "Every invalid-credential condition —
  unknown, expired, revoked, wrong secret — returns one indistinguishable
  error"; `INV-012`; `AC-010`; `AGENTS.md` §12 ("the targeted tests/checks for
  the touched surface pass").
- **Location**: `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:807`, `:1030`,
  `:1064` — three raw `INSERT INTO wyrd.auth_api_keys (… sa_id …)` statements in
  `mod pg_tests`, against the column the migration renamed to `principal_id`.
- **Evidence**: reviewer ran
  `scripts/postgres/with-test-postgres.sh -- cargo test --locked -p wyrd-auth --lib pg_tests -- --test-threads=1`
  → `64 passed; 3 failed`. Failures:
  `exchange_api_key::pg_tests::api_key_invalid_reason_distinct_for_every_variant`
  (panic at `:819`), `…::hash_mismatch_rejected` (`:1074`),
  `…::revoked_key_rejected` (`:1040`).
- **Consequence**: the repository's only proof that revoked, hash-mismatched, and
  otherwise-invalid credentials collapse to one indistinguishable error no
  longer runs. The task claims this criterion `PARTIAL`; it is in fact
  *regressed*. The recorded evidence ran
  `cargo test --locked -p wyrd-auth --lib -- --skip pg_tests`, which excludes
  exactly the suite this change breaks.
- **Required correction**: those three inserts name the renamed column, and
  `cargo test -p wyrd-auth --lib pg_tests` passes under the Postgres wrapper with
  no assertion removed or relaxed.

### FIND-TASK-001-2 — VIOLATION — `check:tenant-isolation` fails on the three new query modules

- **Violated obligation**: `AGENTS.md` §11 ("Boundary-sensitive change: run the
  matching boundary check … `check:tenant-isolation`"), §12 ("the targeted
  tests/checks for the touched surface pass").
- **Location**: `crates/wyrd/wyrd-sql/src/queries/platform/principals.rs`,
  `credentials.rs`, `principal_grants.rs`.
- **Evidence**: `python scripts/check_tenant_isolation.py` exits **1** with
  `use query macros or document the runtime query exception` for all three files.
  The rule (`scripts/check_tenant_isolation.py:477-483`) fires on `sqlx::query(`
  in `WYRD_QUERIES` without a marker from `RAW_QUERY_ALLOWLIST_MARKERS`
  (`:112-116`).
- **Consequence**: a live repository gate is red on the principal surface, and
  the new SQL is not schema-checked at build time — which is the property the
  gate protects.
- **Required correction**: `python scripts/check_tenant_isolation.py` exits 0 with
  the three files unexcluded, using the check's own sanctioned mechanism (the
  documented raw-query marker) or compile-time query macros. Do not widen the
  checker's globs or allowlist paths.

### FIND-TASK-001-3 — VIOLATION — generated contract goldens not regenerated

- **Violated obligation**: spec `VER-006` ("Contract regeneration
  (`mise run codegen:check`) remains in scope because this change owns the HTTP,
  error-catalog, schema, and stub contracts it alters"); the task's own
  Verification line "`mise run codegen:check` when generated contracts move";
  `AGENTS.md` §12 ("Public contracts regenerate cleanly when touched").
- **Location**: `crates/wyrd-spec/schemas/{auth_principal_kind,
  auth_revoke_principal_request, auth_revoke_principal_response,
  bifrost_audit_event}.json` and the four matching files under
  `crates/wyrd-spec/tests/schemas/`.
- **Evidence**: reviewer ran
  `cargo run --locked -p wyrd-spec --example gen_schemas --features server`;
  `git status --porcelain` then listed all 8 files modified. The committed
  `auth_principal_kind.json` still enumerates only `user`/`service`/`agent` and
  still carries the superseded description, while `PrincipalKindTag` now has
  five variants. The reviewer restored the subject with `git checkout --`.
- **Consequence**: `mise run codegen:check` fails; the published JSON-Schema
  contract for a stable wire enum disagrees with its Rust source. The in-tree
  `generated_schema_goldens_tests::generated_schemas_match_goldens`
  (`crates/wyrd-spec/src/lib.rs:186`) compares the two *committed* directories
  against each other, so it passes while both are stale — it cannot detect this.
- **Required correction**: regenerate and commit the drifted artifacts; a
  regeneration run afterwards leaves the tree clean.

### FIND-TASK-001-4 — MISSING — no platform-scope credential issuance

- **Violated obligation**: task acceptance "Credential creation returns the
  plaintext once; the stored row holds only the verifier plus non-secret
  metadata"; task Approach step 4 ("Move credential issuance, lookup,
  verification, revocation, and rotation onto the principal-generic owner,
  keeping the existing hashing and error contract"); `REQ-008`; spec delivery
  stage 1, which places "credential generation, verification, lookup,
  revocation, and rotation" in this stage.
- **Location**: absent. `crates/wyrd/wyrd-sql/src/queries/platform/credentials.rs`
  provides `insert_platform_credential(…, secret_hash: &str, …)` — a raw slot
  that takes an already-computed verifier. No caller, and no secret generator or
  Argon2 hasher, exists for `platform.credentials` anywhere in the candidate.
- **Evidence**: `grep -rn "wyrd_crypt\|Argon2\|argon2"
  crates/wyrd/wyrd-sql/src/queries/platform/` matches only doc comments;
  `crates/wyrd/wyrd-sql/Cargo.toml` has no crypt dependency; the candidate's only
  `wyrd-auth/src/issue_api_key.rs` change is `expires_at` → `Some(expires_at)`.
  The cited proof is circular: `pg_admin_principals.rs:34-40` defines
  `fn verifier(label) -> format!("$argon2id$v=19$stub${label}")`, and
  `listing_returns_metadata_and_never_plaintext` (`:340-380`) then asserts the
  stored value is not the unrelated literal `"wyrd_global_supersecretvalue"` and
  that it `starts_with("$argon2id$")` — both guaranteed by the test's own input,
  independent of any implementation.
- **Consequence**: the acceptance criterion is unimplemented and its evidence is
  tautological. `TASK-003` names "`crates/wyrd/wyrd-auth/` — credential issuance
  from `TASK-001`" as a dependency it consumes; that dependency does not exist,
  so `TASK-003` would have to invent the deployment root credential's generation
  and hashing itself, outside the seam this task was told to own.
- **Required correction**: a platform-scope credential issuance operation exists
  in the owning crate, generating the secret server-side from a CSPRNG, storing
  only the Argon2 verifier through the existing hashing seam, and returning the
  plaintext exactly once. Its proof must start from the issued plaintext — verify
  it against the stored row, then show that no read or listing surface returns
  it — rather than asserting properties of a literal the test wrote itself.

### FIND-TASK-001-5 — INCORRECT — the tenant-scope principal projection was not widened with its column

- **Violated obligation**: task acceptance "A machine principal persists and
  holds a credential with no Card"; `REQ-004`; `AGENTS.md` §15 ("Fix a root cause
  once at the shared owner instead of patching each symptom"); `INV-011`
  (fail-closed with a typed outcome).
- **Location**: `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:34`
  (`ServiceAccountPrincipalRow.card_ref: Json<CardRef>`) and
  `crates/wyrd/wyrd-auth/src/revoke.rs:33-42`.
- **Evidence**: the migration makes `wyrd.auth_service_accounts.card_ref`
  nullable. In the *same file*, `ApiKeyLookupRow.card_ref` (`:51`) was correctly
  widened to `Option<Json<CardRef>>` while `ServiceAccountPrincipalRow.card_ref`
  (`:34`) was not — evidence of an oversight, not a decision. Both
  `service_account_by_id` and `service_account_by_card_ref` deserialise into that
  struct. Separately, `revoke.rs:37-41` derives the kind with
  `if row.principal_kind == "agent" { Agent } else { Service }`, so a stored
  `tenant_admin` row reports `PrincipalKindTag::Service`, while
  `exchange_api_key.rs:501` (`principal_kind_wire`) is the shared owner that
  already fails closed on an unknown label.
- **Consequence**: reading a Card-free tenant-scope principal produces an
  untyped `sqlx` column-decode error rather than a typed refusal. At
  `revoke.rs:33` that error is swallowed by `.ok().flatten()`, so a Card-free
  principal silently falls through to `WyrdError::PrincipalNotFound` and cannot
  be revoked. A `tenant_admin` row that *is* found is tagged as a service, so
  its revocation NOTIFY payload and epoch cache key disagree with the kind its
  token would carry.
- **Required correction**: the row projection represents the column's real
  nullability, a Card-free tenant-scope principal reads back without a decode
  error, and the stored principal kind is mapped through the single existing
  owner rather than re-derived. Proof: a focused Postgres test that inserts a
  Card-free tenant-scope principal and reads it back through
  `service_account_by_id`, plus a test that a `tenant_admin` row maps to
  `PrincipalKindTag::TenantAdmin`.

### FIND-TASK-001-6 — MISSING — the required focused `mise` lane was not added

- **Violated obligation**: the task's Verification section — "Add a focused
  `mise` lane for this capability following the `test:cards:unit` /
  `test:cards:integration` pattern, and run it."
- **Location**: `mise.toml` — untouched by the candidate.
- **Evidence**: `git diff a4883bf..adbe971 -- mise.toml` is empty; no task name
  matches the capability. `pg_admin_principals` is reached only incidentally by
  the broad `test:sql` lane (`mise.toml:1460`, `cargo nextest run -p wyrd-sql`).
- **Consequence**: there is no narrow lane a contributor or reviewer can run to
  prove this capability, which is the lane the task's Verification section, and
  `AGENTS.md` §11's "narrowest task" rule, both call for.
- **Required correction**: a capability-scoped lane exists in `mise.toml`
  following the `test:cards:*` shape, runs the principal/credential Postgres and
  focused suites through the repository-managed Postgres wrapper, and was run.

### FIND-TASK-001-7 — DRIFT — three additions with no production caller

- **Violated obligation**: `AGENTS.md` §15 ("Do not scaffold for hypothetical
  reuse or future requirements"); Ponytail step 1 — each can be deleted while
  preserving every acceptance criterion.
- **Location**: `crates/wyrd-spec/src/auth/principal_kind.rs:65`
  (`is_platform_scoped`), `:75` (`may_bind_card`);
  `crates/wyrd/wyrd-sql/src/queries/platform/principals.rs:96`
  (`count_platform_principals`).
- **Evidence**: `grep -rn "is_platform_scoped\|may_bind_card"` across the tree
  matches only the definitions and their own unit tests.
  `count_platform_principals` has one caller, the assertion tail of
  `platform_store_rejects_a_tenant_scope_kind`, which
  `platform_principal_by_id(...).is_none()` already covers. Its own rustdoc says
  "Initialization uses this", and initialization is `TASK-003`, which does not
  exist. No acceptance criterion names any of the three; the properties they
  describe are enforced durably by the `CHECK` constraints the migration adds.
- **Consequence**: two public `wyrd-spec` contract methods and one query slot
  carry maintenance and contract-stability cost for behaviour nothing exercises,
  and each advertises a second, weaker place to answer a question the schema
  already answers authoritatively.
- **Required correction**: delete all three, or — for anything a later task truly
  needs — let that task add it with its first real caller. The reduced tree still
  passes the `pg_admin_principals` suite and the `principal_kind` unit tests.

### FIND-TASK-001-8 — INCORRECT — wrong error variant for a platform-scope kind

- **Violated obligation**: `AGENTS.md` §4 (public errors carry accurate,
  registered metadata); `INV-011` (denial is fail-closed *and* intelligible).
- **Location**: `crates/shared/wyrd-auth-issue/src/lib.rs:483` —
  `(PrincipalKindTag::GlobalAdmin, _) => Err(IssueError::InvalidCardRef)`.
- **Evidence**: `IssueError::InvalidPrincipalKind` already exists at `:86`
  ("principal kind does not match token issue helper") and is the accurate
  variant; `InvalidCardRef` at `:89` renders as "principal card_ref is missing or
  mismatched". The function's own rustdoc concedes the mismatch, documenting
  "or when the kind is platform-scoped" under the card-ref error. The sibling
  verifier path already gets this right, returning `AuthError::InvalidToken` for
  the same condition (`wyrd-auth-verify/src/lib.rs:811`).
- **Consequence**: an operator or SDK author debugging a rejected platform-scope
  token request is told the card reference is wrong when the kind is wrong.
- **Required correction**: the platform-scope arm returns the existing
  kind-shaped variant, and a focused `wyrd-auth-issue` test asserts it.

### FIND-TASK-001-9 — VIOLATION — rustdoc omissions and one rustdoc that asserts an unreachable invariant

- **Violated obligation**: `AGENTS.md` §16 — "Every fallible Rust function or
  method MUST include a `# Errors` section naming the error conditions",
  "regardless of visibility"; "Rustdoc MUST explain intent … and relevant
  invariants or side effects"; "Missing or placeholder rustdoc on any touched
  Rust item is a hard blocker."
- **Location (a)**: `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:350` —
  `issue_for_subject` was materially modified to add a new
  `IssueError::InvalidPrincipalKind` return for a Card-free principal and has a
  single-line doc comment with no `# Errors` section.
- **Location (b)**: `crates/wyrd/wyrd-sql/src/queries/platform/principals.rs:41-44`
  — `insert_platform_principal`'s rustdoc states "The caller supplies `id` so the
  principal can be referenced **inside the same transaction** that grants its
  authority and issues its first credential."
- **Evidence (b)**: every function in the three new `platform/*` modules takes
  `&OperatorPool` and executes against `pool.pool()`. No slot accepts a
  transaction executor, so no caller can compose them into one transaction.
  `OperatorPool::begin()` exists (`operator_pool.rs:35`) and its own rustdoc says
  "Query modules use this boundary rather than reaching through to the underlying
  pool", but it has zero callers. `REQ-021` requires initialization to create the
  principal, generate its credential, and grant its authority "in one
  transaction"; `TASK-003` inherits that and consumes these slots.
- **Consequence**: (a) a maintainer reading `issue_for_subject` cannot see its
  failure modes; (b) the documentation promises a composition guarantee the API
  cannot deliver, and `TASK-003` will discover the gap only when it tries to
  satisfy `REQ-021`.
- **Required correction**: `issue_for_subject` documents its error conditions.
  The platform query slots either accept an executor that lets a caller compose
  them in one transaction, or their rustdoc stops claiming they can — and if the
  documentation is what changes, say so explicitly so `TASK-003` plans for it.

### FIND-TASK-001-10 — VIOLATION — commit identity and forbidden AI co-author trailers

- **Violated obligation**: `AGENTS.md` §13 — "Contributor identity for this repo:
  name=`Thorrester`, email=`sjforrester32@gmail.com`"; "Never add AI co-author
  trailers"; "If git config is wrong, stop and surface to the user."
- **Location**: all thirteen commits in `a4883bf..adbe971`.
- **Evidence**: `git log a4883bf..adbe971 --format='%an <%ae>' | sort -u` yields
  exactly `Claude <noreply@anthropic.com>`. `git config user.name` / `user.email`
  return the same. Every commit body ends with
  `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` and `Claude-Session: …`.
- **Consequence**: the branch's history does not carry the repository's declared
  contributor identity and carries trailers the repository prohibits. `AGENTS.md`
  §13 also forbids fixing this by running `git config` or setting identity
  environment variables.
- **Required correction**: this is **not** an implementation fix. It must be
  surfaced to the user, who decides whether to correct the environment's git
  configuration and whether to rewrite the branch's authorship. The remediation
  implementer must not run `git config`, must not set identity environment
  variables, and must surface the same condition for its own commits. This
  finding does not gate the code corrections.

## Prior-finding closure

None. This is the first review of TASK-001; no prior verdict exists in
`changes/active/admin-principals/review/`.

## Summary

| Classification | Count | IDs |
|---|---|---|
| MISSING | 2 | FIND-TASK-001-4, -6 |
| INCORRECT | 2 | FIND-TASK-001-5, -8 |
| DRIFT | 1 | FIND-TASK-001-7 |
| VIOLATION | 4 | FIND-TASK-001-2, -3, -9, -10 |
| REGRESSION | 1 | FIND-TASK-001-1 |
| **Total** | **10** | |

The durable model is largely right: the two-store split is well chosen, tenancy
is structurally absent at platform scope rather than nullable-and-checked, the
Card-binding constraints are correct and were verified against live Postgres by
this review, `REQ-039` is fully resolved, and the replaced verifier test
increased rather than reduced coverage. What fails is the other half — the
credential half is a set of raw SQL slots with no issuance owner, its headline
proof is circular, the rename broke the repository's existing
invalid-credential evidence, and three gates that govern this exact surface
(tenant isolation, codegen, the capability lane) were never run or never added.
