# TASK-001-R2 repository-standards review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `8e61c03493f5c0ebcc12cdb4a9998df284f7fc8f`
- Candidate: `0ca4ee7ef7416f870ce3244add406901b57337de`
- Result: **FAIL** (one material finding, SR-1)
- CodeGraph: unavailable (`.codegraph/` absent)

HEAD stayed `0ca4ee7ef` and the worktree stayed clean for the whole review.
This review covers repository standards only. It does not judge task
acceptance or do a Ponytail audit.

## 1. Authority coverage

| Changed surface | Authorities applied |
|---|---|
| `vala-bifrost-redux` catalog (`bifrost_catalog.rs`, `tenant_table.rs`), Scribe ingress, audit tables/projection, Gate test helper | `AGENTS.md` §§4–6, 11, 16; `architecture/agent-rules.md` (bare types, cross-tier re-exports, `TenantConn` commit ownership, audit transactionality, rustdoc); `architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`; `languages/rust-core.md` |
| `wyrd-server` audit (`audit/mod.rs`, `audit/publication.rs`) | `AGENTS.md` §§2 (audit doctrine), 4, 11–12 (no gate circumvention); agent-rules audit and test rules; `architecture/v1/00-foundations/permission-check.md`; `languages/testing-workflows.md` |
| `wyrd-server` Bifrost service, admin/cards/eval routes, `state.rs` | `AGENTS.md` §§5, 9, 16; agent-rules (audit same-transaction, bare types, struct-centered style); permission-check authority; `languages/errors.md` (WyrdError) |
| `wyrd-server` tests (`pg_card_registration_route.rs`, `pg_router_smoke.rs`, `storage_e2e.rs`), `wyrd-storage/tests/pg_sweeper.rs`, `wyrd-testing` fixture and journeys | `AGENTS.md` §11 (tiers, runtime ownership, gated lanes), §16 (test rustdoc); agent-rules test placement, `#[ignore]`/deletion rules; `TESTING.md` |
| `mise.toml`, `scripts/postgres/check-inventory.py` | `AGENTS.md` §11 (mise lanes, environment-owning wrappers), §12 (adding/retiring checks); agent-rules (mise-run tests) |
| `architecture/wyrd-design.md` (dead-letter prose) | `AGENTS.md` §§1–2 (design authority, audit doctrine) |
| `.agents/skills/*`, `.claude/skills/*` | `AGENTS.md` §14 (canonical source + generated mirror, `skills:sync`/`check:skills-sync`, no harness-specific paths) |
| Change packet (R2 task, R1 review artifacts, TASK-002) | `AGENTS.md` §14; spec-driven development; lifecycle vocabulary |

## 2. Rule results

| Rule | Result | Evidence |
|---|---|---|
| Bare types in signatures | **FAIL** | New `append_registration_audit` takes `conn: &mut wyrd_sql::TenantConn<'_>` (`crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1490`). Everywhere else the R1 bare-type drift is fixed: `audit/mod.rs`, `ServerGate` (`state.rs:339`), Eval `require_eval_run`, and the audit-publication journey helpers |
| Cross-tier imports via owning tier re-export | **FAIL** | The same signature names `wyrd_sql::TenantConn` from a Vala crate. The correct path, `vala_sql::TenantConn`, exists (`crates/vala/vala-sql/src/lib.rs:20`) |
| `use` at module top | PASS | New imports (`AuditLogTable`, `PLATFORM_AUDIT_PRINCIPAL`, `Display`, `Permission`, `AppState`, `Gate`, `PostgresGateAudit`, `CardRef`, `AuditEvent`) are all at module scope |
| `TenantConn` callee never commits | PASS | `append_registration_audit` and `audit::append_on` only append. Commits stay with the transaction owner (`register_dataset`, `write_registration`, the delete blocks) |
| Audit: authorization decisions audited transactionally, once, fail closed | PASS | Card register/delete append Allowed on the registry transaction before mutation. Every non-committing exit records it standalone once (`record_unless_committed`, `register_card` flag). A catalog concurrent-winner now appends on its observing transaction. Bifrost pre-commit failures record standalone but leave `AuditUnavailable` fail-closed. Issuer create records standalone before discovery network IO, the documented exception where no transaction can span external IO |
| `PermissionCheck` chokepoint | PASS | `authorize_service_accounts_write` now goes through `authorize_recording_denial` (configured checker), and `configured_checker_denial_governs_workload_binding_create` proves it. Observation O-2 covers the leftover dead helper |
| Engine mechanics not audit | PASS | `wyrd-design.md:1353` and the Card service/test prose now describe the dead letter as reconciliation lineage. No `card.registration.dead_letter` references remain outside `changes/` |
| Tenancy (nil tenant) | PASS | The nil-tenant exception is narrow in all three places: `AuditLogTable::admits_system_owner` (namespace `Audit` + exact name), Scribe (also requires `PLATFORM_AUDIT_PRINCIPAL` and matching authenticated tenant), and Gate (still refuses caller writes to `BifrostNamespace::Audit`, `gate/mod.rs:879`). Unit tests cover both admission and refusal |
| Struct-centered style | PASS (localized) | `record_unless_committed`/`allow_card_write` are free functions inside modules that already use functions, a localized edit under §5. `AuditPublisher` and `Gate` keep their owners. No new god object or single-impl trait |
| Async only at IO | PASS | Every new `async fn` awaits Postgres. `admits_system_owner` and the frame validation stay sync |
| `WyrdError` usage | PASS | Denials keep `PermissionDeniedRbac` with the original public message/details. No hand-written code/status logic |
| Tracing | PASS | `#[tracing::instrument]` skip lists now include the new `allowed` parameter, so audit payloads are not recorded as span fields |
| unwrap/expect | PASS | `mise run check:unwrap-audit` passed. New `expect`s are test-only and name their expectation |
| Rustdoc on new/modified items, `# Errors`/`# Panics` | PASS | Every new or modified item in the diff has intent docs. Fallible items carry `# Errors` (`append_registration_audit`, `register_card`, `write_registration`, both deletes, `record_unless_committed`, `allow_card_write`, `create_trusted_issuer`, `retained_audit_batches`). Tests/helpers carry `# Panics` |
| Test tiers and runtime ownership | PASS | New behavior has journey/integration coverage: `system_owner_security_rejections_retain_once` (bound server), `delete_by_ref_not_found_records_one_decision` (in-process server), admin/Bifrost `pg_tests`. Scribe/binding/projection unit tests are pure. No Python/Node lifetime inside Rust tests |
| No gate circumvention: `#[ignore]` | PASS | The new `#[ignore = "requires the serialized Postgres-backed journey lane"]` follows the file's existing pattern (lines 234, 303, 415). The `server` journey lane runs `-P journey --run-ignored=all` (`mise.toml:330`). §11 sanctions `#[ignore]` for Rust e2e gating |
| No gate circumvention: deleted test (8af4ef85d) | PASS | `bounded_sweep_never_exceeds_its_fixed_concurrency` never called `AuditPublisher`. It drove `futures_util::for_each_concurrent` over a range with the constant, so it proved library behavior and could not fail if the sweep dropped its bound. Removing a disconnected test weakens no production proof. The journey doc was corrected to state honestly that the ceiling is unproven end to end (`audit_publication.rs`, stalled-tenant doc). Not a weakened assertion on reachable behavior |
| No `#[allow]` suppressions | PASS | `mise run check:clippy-allow-audit` passed. The diff adds no `#[allow]` |
| Removed error variant / test | PASS | `AuditProjectionError::NilAuthenticatedTenant` and its unit test were removed together as a deliberate behavior change. The replacement test `projects_system_owner_rows` pins the new behavior, and no references remain |
| mise lanes / environment ownership | PASS | Card, CLI, WyrdState, and Python lanes move from the removed `setup:postgres`/fixed DSN env to `scripts/postgres/with-test-postgres.sh` + `:inner`, matching every other Postgres lane. `python3 scripts/postgres/check-inventory.py` → `PASS`. Adding those lanes to the inventory's `pre` set protects the live isolated-lane invariant (§12). See O-3 |
| Formatting | PASS | `cargo fmt --all -- --check` clean |
| Clippy (touched crates, `--all-targets --all-features -D warnings`) | PASS | Ran against `vala-bifrost-redux`, `wyrd-server`, `wyrd-testing`, `wyrd-storage` |
| Skills: canonical source + mirror | PASS (content) | Committed `.agents/skills/{wyrd-plan,wyrd-task-review}/SKILL.md` are byte-identical to `.claude/skills`. `agents/openai.yaml` exists only in the canonical tree. No harness-specific paths added. See O-1 for the local `check:skills-sync` result |
| Lifecycle vocabulary | PASS | `TASK-001-R2` front matter `status: review` (R1 STD-R1-05 closed) |
| No plan/task references in code | PASS | Added rustdoc and comments name behavior, not tasks/IDs |

## 3. Material findings

### SR-1 — New catalog helper names `wyrd_sql::TenantConn` in its signature

- **Rules violated:** `architecture/agent-rules.md` "Bring types in with `use`
  and use bare names in signatures" and "Cross-tier imports go through the
  owning tier's re-exports … Vala and Bifrost code imports
  `use vala_sql::{TenantConn, …}` — never `use wyrd_sql::TenantConn`".
- **Location:** `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1489-1493`
  (`async fn append_registration_audit(conn: &mut wyrd_sql::TenantConn<'_>, …)`).
  This code is new in `4d3d84808`. It copies the older adjacent
  `acquire_table_advisory_lock` (`:1507`), which is existing drift and not
  precedent.
- **Consequence:** R2 exists partly to close the R1 bare-type finding
  (STD-R1-04 / R1-7), and this reintroduces the same violation class in new
  code. It also makes a Vala crate name the Wyrd tier's type directly, which
  bypasses the re-export surface that lets Vala version its `wyrd-sql`
  dependency on its own.
- **Testable correction:** Add `TenantConn` to the existing
  `use vala_sql::{…}` import at the top of `bifrost_catalog.rs` and change the
  parameter to `conn: &mut TenantConn<'_>`. Verification: `git grep -n
  "wyrd_sql::TenantConn" -- crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs`
  must not match `append_registration_audit`. Also run
  `mise exec -- cargo clippy --locked -p vala-bifrost-redux --all-targets
  --all-features -- -D warnings`. Updating the adjacent pre-existing
  `acquire_table_advisory_lock` in the same edit is optional and costs nothing.

## 4. Non-material observations (no remediation required)

- **O-1 `check:skills-sync` fails locally on mtime only.** The script uses
  `rsync -aicn`, and `-a` compares timestamps. It reported
  `.f..t.... wyrd-plan/SKILL.md` (time only, checksum equal). Committed
  contents are identical, and git does not track mtimes, so this is a
  worktree artifact of a pre-existing script, not a candidate defect. Running
  `mise run skills:sync` locally clears it.
- **O-2 Dead shortcut helper.** `wyrd_auth::service_accounts::require_service_accounts_write`
  (`crates/wyrd/wyrd-auth/src/service_accounts.rs:10`) now has no production
  caller. Its denial message is duplicated at
  `crates/wyrd/wyrd-server/src/audit/mod.rs:279`. `permission-check.md`
  discourages shortcut helpers, so a follow-up could delete it. It is not
  added or used by this diff.
- **O-3 Stale workflow comment.** `.github/workflows/storage-integration-cloud.yml:39`
  still cites `build:postgres → db:setup-roles → db:migrate`. This diff
  removed `build:postgres`, and `db:setup-roles` was already gone. The comment
  is not executable and the inventory check passes.
- **O-4 Journey error detail discarded.** `audit_publication.rs:380,383` use
  `.map_err(|_| "…")`, which drops the underlying peer-audit error from a
  failing journey's output. This is test-only diagnostics.

## 5. Verification run by this reviewer

| Command | Result |
|---|---|
| `mise exec -- cargo fmt --all -- --check` | PASS |
| `mise run check:unwrap-audit` | PASS |
| `mise run check:clippy-allow-audit` | PASS |
| `python3 scripts/postgres/check-inventory.py` | PASS |
| `mise run check:skills-sync` | FAIL (mtime-only, O-1); committed content byte-identical |
| `mise exec -- cargo clippy --locked -p vala-bifrost-redux -p wyrd-server -p wyrd-testing -p wyrd-storage --all-targets --all-features -- -D warnings` | PASS (clean at `0ca4ee7ef`) |

Postgres-backed tests were not run. Their evidence belongs to the task review.

## 6. Overall

**FAIL**, on SR-1 only. It is a one-line, mechanically testable correction.
Every other applicable standard passes, including the audit transaction
coupling, the `PermissionCheck` chokepoint, rustdoc completeness, and gate
integrity, which were the R1 standards failures.
