# Repository-standards audit — TASK-001

Subject: `changes/active/admin-principals/tasks/TASK-001-principal-credential-model.md`
Candidate: `a4883bf` (exclusive) .. `adbe971` (inclusive), branch `claude/admin-principals-spec-qfsmjc`
Working tree verified clean at `adbe971` before and after every probe.

## Independence limitation

`wyrd-task-review` requires this audit to be delegated to a fresh specialist
independent of both the implementer and the acceptance reviewer. This session
has no subagent-spawning tool (no `Task`/`Agent` tool is available; only
`Claude_Code_Remote__create_session`, which cannot see this branch's working
tree). The acceptance reviewer therefore performed this standards pass itself,
as a separate authority-mapped sweep conducted before the acceptance matrix was
written. The reviewer is fresh relative to implementation — it did not author
any commit in the candidate range — so implementation independence holds, but
reviewer-vs-specialist independence does not. This deviation is recorded rather
than hidden; it does not change any finding below, every one of which is backed
by a command this session executed.

## Authority coverage

| Changed surface | Governing authority |
|---|---|
| `crates/wyrd-spec/src/auth/principal_kind.rs` | AGENTS §3 (wyrd-spec ownership), §4 (Rust core), §9 (contract rules), §15 (minimum code), §16 (rustdoc); spec VER-006 |
| `crates/wyrd-spec/schemas/*`, `tests/schemas/*` (generated, untouched) | AGENTS §11 (codegen:check), §12 (public contracts regenerate cleanly); spec VER-006 |
| `crates/shared/wyrd-runtime/src/principal.rs`, `request_context.rs` | AGENTS §4, §5 (abstraction), §16 |
| `crates/shared/wyrd-auth-issue`, `wyrd-auth-verify` | AGENTS §4 (`WyrdError`/typed errors), §10, §16 |
| `crates/wyrd/wyrd-sql/migrations/20260601000020_admin_principals.sql` | AGENTS §15 (`wyrd-sql` is the durable Postgres layer); `architecture/agent-rules.md` `TenantConn`/`OperatorPool` boundary; spec REQ-003/004/039 |
| `crates/wyrd/wyrd-sql/src/queries/platform/*` (new) | AGENTS §4, §5 (struct-centred style), §6 (async at IO only), §16; `scripts/check_tenant_isolation.py` |
| `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs` | AGENTS §4 (`expect` only for invariants), §16; tenant-isolation check |
| `crates/wyrd/wyrd-auth/src/*` | AGENTS §9 (server durable behaviour), §16; AGENTS §11 test taxonomy |
| `crates/wyrd/wyrd-sql/tests/pg_admin_principals.rs` | AGENTS §11 (test tiers, no credential requirement in fast lane), §16 (rustdoc on test fns) |
| `mise.toml` (NOT changed) | AGENTS §11 (narrowest lane), TASK-001 Verification (explicit lane instruction) |
| `changes/active/admin-principals/spec.md`, `tasks/*` | AGENTS §14 (planning artifacts) |

Authorities read: `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/references/languages/spec-driven-development.md`,
`architecture/references/README.md` router entries for SQL/auth, and the
approved `spec.md` **Verification scope** (VER-001..VER-006), which narrows
§11/§12 for this change.

No `.codegraph/` directory exists at the repository root, so CodeGraph was
correctly skipped.

## Rule-by-rule result

| Rule | Result | Evidence |
|---|---|---|
| §3 `wyrd-spec` is IO-free, async-free, PyO3-free | PASS | Only `PrincipalKindTag` variants and two `const fn` added; no new deps in `crates/wyrd-spec/Cargo.toml` |
| §4 `thiserror` for library errors, no hand-written problem-json | PASS | No new error enum; existing `IssueError`/`AuthError` reused |
| §4 no `unwrap()` outside tests; `expect()` only for named invariants | PASS | `python scripts/check_unwrap_audit.py` → `unwrap/expect audit passed`, exit 0. New `expect("invariant: card-bound principal CardRef has resolved space")` names its invariant |
| §4 no wildcard deps / per-crate profiles | PASS | No manifest changes in the candidate |
| §4 Clippy clean on touched crates | PASS | `cargo clippy --locked -p wyrd-spec -p wyrd-runtime -p wyrd-auth-verify -p wyrd-auth-issue -p wyrd-sql -p wyrd-auth --all-targets` → exit 0; the single warning (`unused async`, `refresh.rs:401`) is pre-existing untouched test code |
| §5 abstraction: concrete types, no speculative traits | PASS | No new trait; concrete row structs |
| §5 struct-centred style for new symbols | PASS (in-pattern) | New `platform/*.rs` use free functions over `&OperatorPool`, matching the established `queries/cards/lifecycle.rs` and `queries/auth/service_accounts.rs` precedent for this tier. §16 "follow existing code style" governs here |
| §6 async only at IO boundaries | PASS | Every new `async fn` awaits sqlx directly; `is_usable`/`is_active` are sync |
| §7 PyO3 boundary | N/A | No PyO3 surface touched |
| §9 tenant isolation preserved on every path | **FAIL** | `python scripts/check_tenant_isolation.py` exits **1**: the three new `queries/platform/*.rs` files use `sqlx::query(` without the sanctioned raw-query marker. See FIND-TASK-001-2 |
| §11 run the matching boundary check for a boundary-sensitive change | **FAIL** | The tenant-isolation check was never run; its failure is reproducible today |
| §11 / VER-006 generated contracts regenerate cleanly | **FAIL** | `cargo run --locked -p wyrd-spec --example gen_schemas --features server` rewrites 8 committed goldens. See FIND-TASK-001-3 |
| §11 no test weakened, disabled, deleted, or narrowed to pass | PARTIAL | `pg_migration.rs` assertions were *strengthened*; `into_verified_rejects_non_user_without_card_ref` was intentionally replaced by two tests (net coverage up). **However** the recorded evidence ran `cargo test -p wyrd-auth --lib -- --skip pg_tests`, skipping exactly the suite this change breaks. See FIND-TASK-001-1 |
| §11 focused `mise` lane for the new capability | **FAIL** | `mise.toml` untouched; `grep -n "admin.principals" mise.toml` returns nothing. See FIND-TASK-001-6 |
| §12 targeted tests for the touched surface pass | **FAIL** | `cargo test -p wyrd-auth --lib pg_tests` under `scripts/postgres/with-test-postgres.sh` → 64 passed, **3 failed** |
| §12 no gate circumvented | PASS | No `#[allow]`, `#[ignore]`, deleted test, or widened boundary glob added |
| §15 stop at the first correct option; no scaffolding for future requirements | **FAIL** | `is_platform_scoped`, `may_bind_card`, `count_platform_principals` have no production caller. See FIND-TASK-001-7 |
| §15 fix the root cause at the shared owner | **FAIL** | `revoke.rs:37` re-derives a principal kind with `if agent {Agent} else {Service}` while `principal_kind_wire` is the shared owner. See FIND-TASK-001-5 |
| §16 rustdoc on every new/materially modified item | **FAIL** | `issue_for_subject` (`exchange_api_key.rs:350`) gained a new error return and carries no `# Errors` section. See FIND-TASK-001-9 |
| §16 rustdoc states real invariants | **FAIL** | `insert_platform_principal` documents same-transaction composition its `&OperatorPool` signature cannot provide. See FIND-TASK-001-9 |
| §16 single responsibility, no new paradigms | PASS | New modules mirror the existing query-slot shape |
| §13 git identity; never add AI co-author trailers | **FAIL** | `git config user.name/user.email` is `Claude <noreply@anthropic.com>`, not the declared contributor identity `Thorrester <sjforrester32@gmail.com>`; every commit in the range carries `Co-Authored-By: Claude Opus 5` and `Claude-Session:` trailers that §13 forbids. §13 also forbids fixing this with `git config` and requires surfacing it. See FIND-TASK-001-10 |
| agent-rules: two connection abstractions only | PASS | Platform rows reach only `&OperatorPool`; tenant rows only `TenantConn`; no third abstraction, no hand-written tenant filter added |
| agent-rules: audit of authorization decisions | N/A | No authorization decision is made in this task's surface |

## Material standards findings

Carried into `verdict.md` as FIND-TASK-001-2, -3, -5, -6, -7, -9, -10, plus the
verification-integrity component of FIND-TASK-001-1.

## Commands executed by this audit

```bash
python scripts/check_unwrap_audit.py                                   # pass, exit 0
python scripts/check_tenant_isolation.py                               # FAIL, exit 1
cargo clippy --locked -p wyrd-spec -p wyrd-runtime -p wyrd-auth-verify \
  -p wyrd-auth-issue -p wyrd-sql -p wyrd-auth --all-targets            # exit 0
cargo run --locked -p wyrd-spec --example gen_schemas --features server # 8 files drift
git checkout -- crates/wyrd-spec/schemas crates/wyrd-spec/tests/schemas # subject restored
cargo test --locked -p wyrd-sql --lib                                  # 74 passed
cargo test --locked -p wyrd-runtime --lib                              # 43 passed
cargo test --locked -p wyrd-auth-issue --lib                           # 25 passed
cargo test --locked -p wyrd-auth-verify --lib into_verified            # 11 passed
scripts/postgres/with-test-postgres.sh -- \
  cargo test --locked -p wyrd-sql --test pg_admin_principals -- --test-threads=1   # 9 passed
scripts/postgres/with-test-postgres.sh -- \
  cargo test --locked -p wyrd-sql --test pg_migration migrations_apply_and_are_idempotent # 1 passed
scripts/postgres/with-test-postgres.sh -- \
  cargo test --locked -p wyrd-auth --lib pg_tests -- --test-threads=1   # 64 passed, 3 FAILED
```

Environment substitutions, matching the implementer's recorded limits: `mise` is
not installed in this container, so `mise exec -- cargo …` was replaced by direct
`cargo` invocations against the same toolchain, and `cargo test` was used where
`cargo nextest` is unavailable. Postgres ran through the repository-managed
wrapper `scripts/postgres/with-test-postgres.sh` with the Docker daemon started
locally.
