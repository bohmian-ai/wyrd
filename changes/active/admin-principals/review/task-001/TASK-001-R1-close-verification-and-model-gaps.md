---
task: TASK-001-R1
title: Close the TASK-001 credential-issuance, regression, and gate gaps
spec: SPEC-admin-principals
spec_revision: 6
original_task: changes/active/admin-principals/tasks/TASK-001-principal-credential-model.md
review: changes/active/admin-principals/review/task-001/verdict.md
candidate_base: a4883bf
candidate_head: adbe971
depends_on: []
skill: $wyrd-implement
---

## Subject

- Approved specification: `changes/active/admin-principals/spec.md`
  (`SPEC-admin-principals`, revision 6, status `approved`). Its **Verification
  scope** section (`VER-001`..`VER-006`) is authoritative and still binds this
  remediation: no broad aggregate, no workspace-wide compilation, and failures
  outside the principal/credential surfaces are out of scope.
- Original task:
  `changes/active/admin-principals/tasks/TASK-001-principal-credential-model.md`.
  Its Objective, Constraints, Non-goals, and Acceptance Criteria are unchanged
  and still govern.
- Reviewed candidate: `a4883bf` (exclusive) .. `adbe971` (inclusive) on
  `claude/admin-principals-spec-qfsmjc`.
- Verdict: `changes/active/admin-principals/review/task-001/verdict.md`;
  standards audit: `.../standards-review.md`.

Environment notes carried forward: `mise` is not installed in this container, so
`mise exec -- cargo …` is substituted with direct `cargo` invocations and
`cargo test` replaces the unavailable `cargo nextest`. Postgres runs through
`scripts/postgres/with-test-postgres.sh`; start the Docker daemon first if it is
not up.

## Issue diagnosis

### D1 — The `sa_id` rename broke three existing credential tests (FIND-TASK-001-1, REGRESSION)

The migration renames `wyrd.auth_api_keys.sa_id` to `principal_id`. Three raw
`INSERT` statements inside `crates/wyrd/wyrd-auth/src/exchange_api_key.rs`'s
`mod pg_tests` still name `sa_id` — at `:807` (`insert_lifecycle_key`), `:1030`
(`revoked_key_rejected`), and `:1064` (`hash_mismatch_rejected`). Running
`scripts/postgres/with-test-postgres.sh -- cargo test --locked -p wyrd-auth --lib pg_tests -- --test-threads=1`
gives `64 passed; 3 failed`, panicking at `:819`, `:1040`, and `:1074`.

Those three tests are the repository's proof of the task's own acceptance
criterion "Every invalid-credential condition — unknown, expired, revoked, wrong
secret — returns one indistinguishable error" (`INV-012`, `AC-010`). The
candidate's recorded evidence ran `cargo test -p wyrd-auth --lib -- --skip pg_tests`,
which excludes exactly the suite the change breaks, so the regression was never
observed. `VER-005` does not excuse it: this is the credential surface named in
`VER-001`, and the breakage is caused by this change.

### D2 — No platform-scope credential issuance exists (FIND-TASK-001-4, MISSING)

The task's Approach step 4 requires moving "credential issuance, lookup,
verification, revocation, and rotation onto the principal-generic owner, keeping
the existing hashing and error contract", and its acceptance criterion requires
"Credential creation returns the plaintext once; the stored row holds only the
verifier plus non-secret metadata". The spec's delivery stage 1 places
"credential generation, verification" here too.

What shipped for `platform.credentials` is
`crates/wyrd/wyrd-sql/src/queries/platform/credentials.rs::insert_platform_credential`,
which takes an already-computed `secret_hash: &str`. Nothing generates a secret,
hashes it, or returns a plaintext once. `grep -rn "wyrd_crypt\|Argon2"
crates/wyrd/wyrd-sql/src/queries/platform/` matches only doc comments, and
`crates/wyrd/wyrd-sql/Cargo.toml` has no crypt dependency.

The cited proof is circular. `crates/wyrd/wyrd-sql/tests/pg_admin_principals.rs:34-40`
defines `fn verifier(label) -> format!("$argon2id$v=19$stub${label}")`, and
`listing_returns_metadata_and_never_plaintext` (`:340-380`) inserts that literal
and then asserts it differs from an unrelated literal and begins with
`$argon2id$`. Both assertions hold by construction of the test's own input and
would still hold if no implementation existed.

`TASK-003` names "`crates/wyrd/wyrd-auth/` — credential issuance from `TASK-001`"
as a dependency it consumes to satisfy `REQ-021`. That dependency is absent.

### D3 — The tenant-scope principal projection was not widened with its column (FIND-TASK-001-5, INCORRECT)

The migration makes `wyrd.auth_service_accounts.card_ref` nullable. In
`crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs`, `ApiKeyLookupRow.card_ref`
(`:51`) was widened to `Option<Json<CardRef>>` but `ServiceAccountPrincipalRow.card_ref`
(`:34`) was left as `Json<CardRef>`. Both `service_account_by_id` and
`service_account_by_card_ref` deserialise into the latter, so a Card-free
tenant-scope principal produces an untyped `sqlx` column-decode error instead of
a typed refusal. At `crates/wyrd/wyrd-auth/src/revoke.rs:33` that error is
swallowed by `.ok().flatten()`, so such a principal silently becomes
`WyrdError::PrincipalNotFound` and cannot be revoked.

The same function, at `:37-41`, derives the kind with
`if row.principal_kind == "agent" { Agent } else { Service }`. The migration now
admits `principal_kind = 'tenant_admin'`, which that expression reports as
`PrincipalKindTag::Service`, so the revocation NOTIFY payload and epoch cache key
disagree with the kind a token for that principal would carry.
`crates/wyrd/wyrd-auth/src/exchange_api_key.rs:501` (`principal_kind_wire`) is the
shared owner that already maps stored labels and fails closed on an unknown one;
`revoke.rs` duplicates that mapping instead of using it, contrary to `AGENTS.md`
§15.

### D4 — `check:tenant-isolation` is red on the three new query modules (FIND-TASK-001-2, VIOLATION)

`python scripts/check_tenant_isolation.py` exits **1**:

```
crates/wyrd/wyrd-sql/src/queries/platform/credentials.rs: use query macros or document the runtime query exception
crates/wyrd/wyrd-sql/src/queries/platform/principal_grants.rs: use query macros or document the runtime query exception
crates/wyrd/wyrd-sql/src/queries/platform/principals.rs: use query macros or document the runtime query exception
```

The rule is `scripts/check_tenant_isolation.py:477-483`: `sqlx::query(` inside
`WYRD_QUERIES` requires either a compile-time query macro or one of the
documented markers in `RAW_QUERY_ALLOWLIST_MARKERS` (`:112-116`). `AGENTS.md` §11
requires running the matching boundary check for a boundary-sensitive change;
it was not run.

### D5 — Generated schema goldens were not regenerated (FIND-TASK-001-3, VIOLATION)

`PrincipalKindTag` gained `GlobalAdmin` and `TenantAdmin` and a new doc comment,
but the committed goldens were not regenerated. Running
`cargo run --locked -p wyrd-spec --example gen_schemas --features server`
rewrites eight files: `crates/wyrd-spec/schemas/{auth_principal_kind,
auth_revoke_principal_request, auth_revoke_principal_response,
bifrost_audit_event}.json` and the four matching files under
`crates/wyrd-spec/tests/schemas/`. The committed `auth_principal_kind.json`
still enumerates only `user`/`service`/`agent`.

`VER-006` explicitly keeps contract regeneration in scope, and the original
task's Verification names `mise run codegen:check`. The in-tree
`generated_schema_goldens_tests::generated_schemas_match_goldens`
(`crates/wyrd-spec/src/lib.rs:186`) only compares the two committed directories
to each other, so it passes while both are stale and cannot catch this.

### D6 — The required focused `mise` lane was never added (FIND-TASK-001-6, MISSING)

The original task's Verification section says: "Add a focused `mise` lane for
this capability following the `test:cards:unit` / `test:cards:integration`
pattern, and run it." `git diff a4883bf..adbe971 -- mise.toml` is empty.
`pg_admin_principals` is reached only incidentally by the broad `test:sql` lane
(`mise.toml:1460`).

### D7 — Three additions have no production caller (FIND-TASK-001-7, DRIFT)

`crates/wyrd-spec/src/auth/principal_kind.rs:65` (`is_platform_scoped`) and `:75`
(`may_bind_card`) are matched across the whole tree only by their definitions and
their own unit tests. `crates/wyrd/wyrd-sql/src/queries/platform/principals.rs:96`
(`count_platform_principals`) has one caller, the assertion tail of
`platform_store_rejects_a_tenant_scope_kind`, which
`platform_principal_by_id(...).is_none()` already covers; its rustdoc says
"Initialization uses this", and initialization is `TASK-003`, which does not
exist. No acceptance criterion names any of the three, and the properties they
describe are enforced durably by the `CHECK` constraints the migration adds.

### D8 — Wrong error variant for a platform-scope kind (FIND-TASK-001-8, INCORRECT)

`crates/shared/wyrd-auth-issue/src/lib.rs:483` returns
`IssueError::InvalidCardRef` ("principal card_ref is missing or mismatched") when
the *kind* is platform-scoped. `IssueError::InvalidPrincipalKind` already exists
at `:86` and is the accurate variant. The function's own rustdoc documents the
kind condition under the card-ref error, and the sibling verifier path already
gets this right (`wyrd-auth-verify/src/lib.rs:811` returns
`AuthError::InvalidToken`).

### D9 — Rustdoc omission and a rustdoc asserting an unreachable invariant (FIND-TASK-001-9, VIOLATION)

`crates/wyrd/wyrd-auth/src/exchange_api_key.rs:350` — `issue_for_subject` gained a
new `IssueError::InvalidPrincipalKind` return and has a one-line doc with no
`# Errors` section, against `AGENTS.md` §16 which applies "regardless of
visibility".

`crates/wyrd/wyrd-sql/src/queries/platform/principals.rs:41-44` —
`insert_platform_principal`'s rustdoc states the caller supplies `id` "so the
principal can be referenced inside the same transaction that grants its authority
and issues its first credential". Every function in the three new `platform/*`
modules takes `&OperatorPool` and executes against `pool.pool()`; none accepts a
transaction executor, so no caller can compose them in one transaction.
`OperatorPool::begin()` (`operator_pool.rs:35`) exists for exactly this and has
zero callers. `REQ-021` requires initialization to do all of it "in one
transaction", and `TASK-003` consumes these slots.

## Intended correction outcome

The credential half of TASK-001 becomes real and proven: a platform-scope
credential can be created by the server, with the plaintext leaving exactly once
and only a verifier persisted; the tenant-scope credential surface this change
renamed is green again on its existing invalid-credential evidence; the durable
read path represents the shape the migration now permits; every gate that governs
this surface passes; and nothing remains in the diff that no acceptance criterion
asked for.

## Decision-complete recommendation

### R1 closes D1

Update the three `sa_id` occurrences in
`crates/wyrd/wyrd-auth/src/exchange_api_key.rs`'s `pg_tests` to `principal_id`.
This is a column-name correction only. Do not change any assertion, do not
relax any `matches!` pattern, and do not `#[ignore]` or delete a test — `VER-005`
explicitly prohibits weakening a test to produce a passing result.

### R2 closes D2

Add the platform-scope credential issuance operation to `crates/wyrd/wyrd-auth`,
which is the crate `TASK-003` already expects to consume and which owns
`IssueApiKey`, the existing tenant-plane precedent. Reuse the existing seams
rather than building a parallel implementation, exactly as the original task's
constraints require:

- the existing secret generator and tenant-prefixed key-material shape
  (`WyrdApiKey::generate` and its siblings in `crates/shared/wyrd-crypt`) —
  adapted for a platform prefix, since a platform credential has no tenant;
- the existing Argon2 hashing seam used by `IssueApiKey`, at the same pinned
  parameters proven by
  `wyrd-auth-issue::tests::hash_api_key_verifies_with_pinned_argon2_params`;
- `insert_platform_credential` as the durable write, unchanged.

The operation returns the plaintext exactly once to its caller and persists only
the verifier. Follow `IssueApiKey`'s shape for where the plaintext lives and how
it is returned. Do not log, trace, or place it in an error payload.

Do **not** build routes, a CLI subcommand, initialization, or provisioning —
those remain `TASK-003`/`TASK-004` non-goals of the original task. Deliver the
Rust-native operation only.

Alternative resolved: do not satisfy this by adding hashing inside `wyrd-sql`.
`wyrd-sql` is the durable Postgres layer (`AGENTS.md` §15) and has no crypt
dependency; adding one there would move a specialized dependency into a broadly
consumed crate, which §4 forbids.

### R3 closes D3

Widen `ServiceAccountPrincipalRow.card_ref` to match its column's real
nullability, mirroring the change already made to `ApiKeyLookupRow.card_ref` in
the same file, and adjust the two consumers that dereference it
(`exchange_api_key.rs::DelegateToken` and `jwt_bearer.rs::issue_and_audit`) so a
Card-free row produces a typed, fail-closed refusal rather than a decode error or
a silent `PrincipalNotFound`.

In `revoke.rs`, replace the inline `if agent { Agent } else { Service }` with the
existing shared owner `principal_kind_wire`, so an unknown or unmapped stored
label fails closed instead of defaulting to `Service`. That is the root-cause fix
`AGENTS.md` §15 requires; do not add a second mapping anywhere.

Alternative resolved: do **not** close this by narrowing the migration back to
forbid Card-free rows. `REQ-004` requires them, and the durable constraints are
correct; it is the Rust projection that is behind.

### R4 closes D4

Use the check's own sanctioned mechanism: add the documented raw-query marker
(one of `RAW_QUERY_ALLOWLIST_MARKERS`, e.g. `raw-query grep allowlist`) as a
comment in each of the three new `platform/*` modules, with a sentence saying why
these are runtime queries. Compile-time `query!` macros would be the stronger
option but require an `sqlx` offline bundle for the new tables, which is a
larger, separately-scoped change; the marker is the in-pattern precedent used by
the sibling `queries/cards/lifecycle.rs`.

Do **not** widen the checker's globs, add paths to its exclusion lists, or edit
`scripts/check_tenant_isolation.py` — `AGENTS.md` §12 forbids broadening a
boundary to hide a violation.

### R5 closes D5

Regenerate and commit the drifted artifacts. In this container that is
`cargo run --locked -p wyrd-spec --example gen_schemas --features server`
(the `wyrd-spec` leg of `mise run codegen:regen`); run the `wyrd-client` and
`vala-core` legs too if they move. Do not hand-edit a generated file.

### R6 closes D6

Add the capability lane to `mise.toml` following the `test:cards:unit` /
`test:cards:integration` shape: a focused unit leaf for the `wyrd-spec` /
`wyrd-runtime` / `wyrd-auth-issue` principal tests, and an integration leaf that
runs `pg_admin_principals` (and the now-repaired `wyrd-auth`
`exchange_api_key::pg_tests`) through
`scripts/postgres/with-test-postgres.sh`. Register it wherever `test:cards:*` is
registered so `check:test-coverage` stays satisfied. Run it.

### R7 closes D7

Delete `PrincipalKindTag::is_platform_scoped`, `PrincipalKindTag::may_bind_card`,
`count_platform_principals`, and their dedicated unit tests. Rewrite the tail of
`platform_store_rejects_a_tenant_scope_kind` to assert the same property with
`platform_principal_by_id(...).is_none()`, which already exists.

If R2's issuance path genuinely needs one of these, keep only that one, with its
real caller. Do not keep any of them on the strength of a future task.

### R8 closes D8

Return `IssueError::InvalidPrincipalKind` from the `GlobalAdmin` arm of
`validate_principal_ref`, and correct the function's `# Errors` section so the
platform-scope condition is documented under the variant that actually carries
it.

### R9 closes D9

Give `issue_for_subject` an `# Errors` section naming its conditions, including
the new Card-free refusal.

For `insert_platform_principal`: R2 determines which way this resolves. If the
issuance operation needs the principal, grant, and credential in one transaction
— which `REQ-021` will require of `TASK-003` — change the platform query slots to
accept an executor the caller can hold across all three, using the existing
`OperatorPool::begin()` boundary its own rustdoc already designates for this.
If you instead leave the slots pool-scoped, remove the same-transaction claim
from the rustdoc and state plainly that each call is independent, so `TASK-003`
plans for it rather than discovering it. Either is acceptable; silently leaving
the false claim is not.

## Constraints and preserved behavior

- The approved specification is unchanged. This remediation introduces no new
  product, public API, architecture, security, compatibility, cross-service,
  concurrency-semantics, or persistent-data decision.
- Every original TASK-001 constraint still binds: `wyrd-spec` stays IO-free,
  async-free, PyO3-free; platform rows reach only `&OperatorPool` and tenant rows
  only `TenantConn` RLS with no third connection abstraction and no hand-written
  tenant filters; existing Card-bound Service and Agent principals keep
  `card_ref`, `card_ref_scope`, `wyrd apply` provisioning, emit-scope behaviour,
  and their durable identity keys.
- Preserve everything the review found correct: the two-store split, the absence
  of a tenant column on `platform.principals`, all five
  `wyrd.auth_service_accounts` check constraints, the `REQ-039` retirement of the
  dormant `platform.*` objects, the strengthened `pg_migration` assertions, and
  the replacement verifier tests
  (`into_verified_accepts_card_free_service_with_empty_scope`,
  `into_verified_rejects_card_free_service_claiming_scope`,
  `into_verified_rejects_platform_scope_kind`).
- Do not weaken, disable, `#[ignore]`, delete, or narrow the assertion of any
  test to produce a passing result. Do not add `#[allow]`, and do not broaden a
  boundary glob or a checker allowlist.
- `AGENTS.md` §13 binds your own commits. The environment's git identity is
  `Claude <noreply@anthropic.com>` with forbidden `Co-Authored-By` /
  `Claude-Session` trailers, against the repository's declared
  `Thorrester <sjforrester32@gmail.com>` and its "never add AI co-author
  trailers" rule. Do not run `git config` and do not set identity environment
  variables. Surface this to the user (FIND-TASK-001-10); it does not gate the
  code corrections.
- Non-goals, unchanged from TASK-001: routes, HTTP contracts, initialization,
  tenant provisioning, OIDC, role-grant management surfaces; a new RBAC engine,
  explicit deny, or permission-scope redesign. Additionally: do not add a
  platform-scope authentication pipeline or public error surface — that is
  `TASK-002` under `REQ-012`.

## Acceptance criteria

| # | Criterion | Closes |
|---|---|---|
| A1 | `cargo test --locked -p wyrd-auth --lib pg_tests -- --test-threads=1` under the Postgres wrapper passes with zero failures, with no assertion removed, relaxed, ignored, or deleted relative to `adbe971` | D1 |
| A2 | A platform-scope credential issuance operation exists in `crates/wyrd/wyrd-auth`, generates the secret server-side from the existing CSPRNG seam, stores only the Argon2 verifier through the existing hashing seam, and returns the plaintext exactly once | D2 |
| A3 | A Postgres-backed test starts from an *issued* plaintext: it verifies that plaintext against the stored row, asserts the stored `secret_hash` is neither the plaintext nor derivable from it, and asserts that neither `list_platform_credentials` nor `platform_credential_by_prefix` returns the plaintext. The test must not assert properties of a literal it wrote itself | D2 |
| A4 | A Postgres-backed test inserts a Card-free tenant-scope principal and reads it back through `service_account_by_id` without error; a test asserts a stored `tenant_admin` row maps to `PrincipalKindTag::TenantAdmin`, not `Service` | D3 |
| A5 | `python scripts/check_tenant_isolation.py` exits 0, with the three `platform/*` files still checked (no path excluded, no checker edit) | D4 |
| A6 | Re-running the schema generator leaves `git status --porcelain` clean for `crates/wyrd-spec/schemas` and `crates/wyrd-spec/tests/schemas` | D5 |
| A7 | A capability-scoped lane exists in `mise.toml` in the `test:cards:*` shape, covers the focused and Postgres principal/credential suites, is registered where `check:test-coverage` expects, and was run | D6 |
| A8 | `grep -rn "is_platform_scoped\|may_bind_card\|count_platform_principals"` returns only symbols that have a real production caller; `pg_admin_principals` still proves that a tenant-scope kind is refused at platform scope | D7 |
| A9 | A `wyrd-auth-issue` unit test asserts that a `GlobalAdmin` `TokenPrincipalRef` yields `IssueError::InvalidPrincipalKind`, and the rustdoc names that variant for that condition | D8 |
| A10 | `issue_for_subject` has an `# Errors` section naming its conditions; `insert_platform_principal`'s rustdoc either describes an API that can actually compose in one transaction, or states plainly that each call is independent | D9 |
| A11 | No non-goal entered the diff: no route, HTTP contract, initialization, tenant-provisioning, OIDC, RBAC-engine, or platform authentication-pipeline change | all |

## Proof

Focused proof, each aimed at one diagnosed gap:

```bash
# A1 — the regression
scripts/postgres/with-test-postgres.sh -- \
  cargo test --locked -p wyrd-auth --lib pg_tests -- --test-threads=1

# A3, A4 — issuance and the widened projection
scripts/postgres/with-test-postgres.sh -- \
  cargo test --locked -p wyrd-sql --test pg_admin_principals -- --test-threads=1

# A9 — the error variant
cargo test --locked -p wyrd-auth-issue --lib

# A5 — the boundary gate
python scripts/check_tenant_isolation.py; echo "exit=$?"

# A6 — generated contracts
cargo run --locked -p wyrd-spec --example gen_schemas --features server
git status --porcelain crates/wyrd-spec/schemas crates/wyrd-spec/tests/schemas
```

Broader verification, scoped by `VER-001`..`VER-006`:

```bash
cargo fmt --all
cargo clippy --locked -p wyrd-spec -p wyrd-runtime -p wyrd-auth-verify \
  -p wyrd-auth-issue -p wyrd-sql -p wyrd-auth --all-targets
cargo test --locked -p wyrd-spec --lib
cargo test --locked -p wyrd-runtime --lib
cargo test --locked -p wyrd-sql --lib
cargo test --locked -p wyrd-auth-verify --lib
python scripts/check_unwrap_audit.py
scripts/postgres/with-test-postgres.sh -- \
  cargo test --locked -p wyrd-sql --test pg_migration migrations_apply_and_are_idempotent
# then the new capability lane from A7
```

Known-out-of-scope failures you are **not** required to fix, each confirmed
pre-existing or environmental by the review:

- `wyrd-spec` `vala::audit_detail::tests::audit_detail_golden_vectors_cover_all_variants_and_nested_values`
  and `query::tests::display_parse_roundtrip_for_small_queries` — reproduced at
  base commit `a4883bf`.
- The seven `wyrd-auth-verify` external-JWKS tests failing with "No rustls crypto
  provider is configured" — container environment.
- Any compilation failure in `wyrd-server`, `wyrd-testing`, `vala-*`, or the
  SDKs — excluded by `VER-004`.

Record acceptance and verification evidence in the original task file's
"Verification evidence" section, mapping each criterion A1..A11 to the exact
command and its result. A later review reassesses the complete cumulative
candidate `a4883bf`..HEAD against the original TASK-001.
