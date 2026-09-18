# Repository-standards audit — TASK-002

Subject: `git diff e551d5d..f3923cc` in `/home/user/wyrd`
(`ee0cb94`, `cf52a3b`, `7c1d403`, `d4185ef`, `91442c5`, `f3923cc`).

## Independence limitation

The skill requires this audit from a specialist independent of both the
implementer and the acceptance reviewer. This container exposes no in-session
subagent tool. A remote session was spawned as a substitute; it resolved its
own clone from the GitHub remote rather than the immutable local candidate, ran
commands the brief prohibited, and its report could not be retrieved through any
available tool. Nothing from it is used here.

The audit below was therefore performed by the acceptance reviewer, and is
recorded as **non-independent**. It is sufficient to establish the findings it
reports, and it is **not** sufficient to support a future `PASS`: a genuinely
independent standards audit against the cumulative candidate is a precondition
for that verdict.

## Authority coverage

| Changed surface | Applicable authority |
|---|---|
| `crates/shared/wyrd-runtime/src/principal.rs` (`AuthContext`, `PlatformPrincipal`) | AGENTS.md §3 ownership, §4 Rust core, §5 abstraction + struct-centered style, §16 rustdoc; agent-rules rustdoc + struct-structure bullets |
| `crates/shared/wyrd-runtime/src/permission.rs` (`Resource::Tenants`, `Action::Suspend`/`Recover`, four constructors, `as_str` made public) | AGENTS.md §4, §5, §9 contracts, §16; spec REQ-017; VER-006 |
| `crates/wyrd/wyrd-auth/src/platform_authz.rs` | AGENTS.md §2 audit foundation, §4, §5, §6 async, §11 test tiers, §16; agent-rules connection abstractions, transactional authorization audit, fail-closed, `mod pg_tests` home |
| `crates/wyrd/wyrd-sql/src/queries/platform/audit_authz.rs` | agent-rules connection abstractions + raw-query allowlist; `scripts/check_tenant_isolation.py` platform-module rule; AGENTS.md §15, §16 |
| `crates/wyrd/wyrd-sql/migrations/20260601000021_platform_authz_audit.sql` | AGENTS.md §15 durable Postgres layer; agent-rules tenant boundary (platform-scope table, operator role only) |
| `crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs` | AGENTS.md §2 audit, §9 server/contract rules, §11 test tiers, §16; agent-rules transactional authorization audit, fail-closed, test homes; spec REQ-012a/REQ-013/REQ-014 |
| `crates/wyrd/wyrd-storage/src/{audit,service}.rs`, `components/storage/routes.rs`, `grpc/peer_auth.rs`, `boot/bootstrap.rs`, `wyrd-testing/src/server.rs`, vala test fixtures | TASK-001 residue: AGENTS.md §4, §16 |
| `changes/active/admin-principals/tasks/TASK-002-*.md` | AGENTS.md §14 planning; not code |

## Rule results

| Rule | Result | Evidence |
|---|---|---|
| agent-rules: two connection abstractions; never a `sqlx::Transaction` handed in by a caller | PASS | `scripts/check_tenant_isolation.py:276` *requires* platform public async fns to take `PgPool` or `Transaction`; `OperatorPool::begin()` (`wyrd-sql/src/operator_pool.rs:35`) is documented as the sanctioned caller-owned-transaction boundary; `vala-sql/src/queries/forge_operations.rs:1384-1612` and `forge_tasks.rs:1736` are established precedent. `python3 scripts/check_tenant_isolation.py` → passed. |
| agent-rules: no hand-written tenant filters on a `TenantConn` path | PASS | New table is platform-scope, reached only via `OperatorPool`; no tenant predicate added anywhere in the diff. |
| agent-rules: every decision that evaluates a principal's permission appends one audit row, allowed and denied alike | **FAIL** | `platform_extractor.rs:52-63` `PlatformCaller::authorize` decides a platform permission and returns `Ok(())`/`Err(...)` with no audit append, no transaction, no pool. See `FIND-TASK-002-1`. |
| agent-rules: the audit row commits in the same transaction as the decision; an unrecordable decision fails closed | PASS for `PlatformAuthorization::authorize`; **FAIL** for the second entry point | `platform_authz.rs:90-118` inserts before returning the open transaction, rolls back and refuses on append failure (`pg_tests::an_unrecordable_decision_fails_closed`). The `PlatformCaller::authorize` path has neither. |
| agent-rules / AGENTS.md §11: Postgres-dependent tests live in `mod pg_tests`; in-process tests inline in `src/` | PASS | `platform_authz.rs` → `mod pg_tests`; `platform_extractor.rs` → `mod tests`, no IO. No new external `tests/` file. |
| AGENTS.md §11 / agent-rules: a unit test never substitutes for a required journey | **FAIL** | No real-server or route-level cross-plane test; `wyrd-server/tests/auth_e2e.rs` and `pg_authz_check_route.rs` untouched. See `FIND-TASK-002-3`. |
| AGENTS.md §16 / agent-rules: rustdoc on every new or materially modified item, including private items, fields, variants, helpers, and tests; `# Errors` on fallible fns | PASS | Every new struct, field, enum variant, constant, free function, method, and test in `principal.rs`, `permission.rs`, `platform_authz.rs`, `audit_authz.rs`, `platform_extractor.rs` carries intent-level rustdoc; `# Errors` present on `PlatformCaller::authorize`, `resolve_grant`, `PlatformAuthorization::authorize`, `insert_platform_authz_audit`. The migration carries a header comment explaining scope and `target_tenant_id` semantics. |
| AGENTS.md §4: `thiserror` for library errors, no `unwrap` in non-test code, no wildcard deps | PASS | `PlatformAuthzError` uses `thiserror`; `python3 scripts/check_unwrap_audit.py` → passed; no manifest change in the diff. |
| agent-rules: `#[allow(clippy::...)]` requires a `// justification:` line | PASS | `python3 scripts/check_clippy_allow.py` → passed; the diff adds no `#[allow]`. |
| AGENTS.md §5: no zero-sized utility structs | PASS (precedent) | `PlatformAuthorization` is a unit struct, but mirrors `PlatformCredentials` (`platform_credentials.rs:112`) established and accepted in TASK-001; AGENTS.md §12 requires matching the owning crate's local pattern. Recorded as an observation, not a finding. |
| AGENTS.md §6: `async fn` must await IO | PASS | Both new async fns await Postgres directly. |
| AGENTS.md §9: public handlers return structured `WyrdError`; write paths instrumented | PASS | `platform_extractor.rs` rejections are `WyrdError` variants; `PlatformAuthorization::authorize` carries `#[tracing::instrument(skip(self, pool, context), err)]` with the context skipped. |
| agent-rules: never reference plans, tasks, or agents in code | PASS | No plan/task identifier appears in any changed source file. |
| agent-rules: never hand-edit generated artifacts | PASS | No generated artifact in the diff. The added permission labels (`tenants`, `suspend`, `recover`) appear in no committed schema, stub, or golden file. |
| agent-rules: never weaken, delete, `#[ignore]`, or narrow a test to pass | PASS | Every test edit in the diff is a mechanical adaptation to TASK-001's `Option<CardRef>` and optional expiry; assertion strength is preserved (e.g. `wyrd-auth-verify/src/lib.rs:1338` moves the `Some(..)` from the guard into the pattern with identical meaning). No test removed, ignored, or loosened. |
| AGENTS.md §12: format, lints, targeted checks pass | PASS with one caveat | `cargo fmt --all --check` clean; clippy clean on `wyrd-runtime`, `wyrd-auth`, `wyrd-auth-check`, `wyrd-auth-verify` `--all-targets`; `wyrd-server --lib` clippy emits three warnings, all in files the diff does not touch (`state.rs:17`, `boot/mod.rs:878`, `mcp/mod.rs:134`) and therefore out of scope under `VER-005`. |

## Material standards findings

1. **Unaudited platform authorization entry point** — `PlatformCaller::authorize`,
   `crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs:52-63`.
   Violates the agent-rules transactional-audit and fail-closed bullets and
   AGENTS.md §2. Correction: no reachable platform permission decision may exist
   that does not append its row in the deciding transaction and refuse when it
   cannot. Testable by asserting that every allowed and denied platform decision
   leaves exactly one `platform.audit_authz` row bound to the operation's
   transaction.
2. **Required evidence absent** — no route-level or real-server cross-plane test;
   `wyrd-server/tests/auth_e2e.rs` and `pg_authz_check_route.rs` are untouched.
   Violates AGENTS.md §11 and the agent-rules journey bullet. Testable by adding
   the cross-plane refusal on an existing protected route.
