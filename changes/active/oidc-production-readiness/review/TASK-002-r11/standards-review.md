# TASK-002 R11 repository standards review

**Subject:** `3fc085acf5b3a710d5dc80892bd2e664b3db6174..bae424cc647dad4be80e7debec976d0b7b3e4cf8` (cumulative); latest correction `454bdb90ef8eedca4bbd4754e083d41f15d0c4ae..bae424cc647dad4be80e7debec976d0b7b3e4cf8`.

**Result: FAIL.** The touched audit module still propagates raw SQL pools on reachable identity paths, and one materially modified resolver module has an inaccurate rustdoc contract. This is a repository standards review, not a task acceptance verdict.

## Review Findings

### Critical

None.

### Important

- **STD-R11-1 — `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:3–6` — inaccurate rustdoc on a materially changed module.** The module rustdoc says the resolvers live in `wyrd-server` because they hold a `PgPool`; the file is in `wyrd-auth`, and the changed `PgWorkloadBindingResolver` field is `WyrdPostgres` (`:202–209`). The removed `PgPool` import also leaves the intra-doc `[`PgPool`]` link without its former target. `AGENTS.md` §16 requires modified Rust documentation to describe the actual owner and invariants. A maintainer is directed toward the wrong dependency and layer. Update the module rustdoc to name `wyrd-auth`, its `WyrdPostgres` owner, and tenant-connection behavior; preserve executable code and confirm rustdoc/build checks. The same opening block still refers to an old commit marker; remove that ephemeral reference while correcting this block, per `architecture/agent-rules.md`'s prohibition on task/plan references in source.

- **STD-R11-2 — `crates/wyrd/wyrd-server/src/audit/mod.rs:145–176` — raw `PgPool` crosses the production audit boundary.** `record_audit` accepts `&PgPool` and `record_audit_owned` accepts an owned `PgPool`, then creates a `TenantConn` itself. This is reachable from the identity connection refusal path (`components/admin/identity.rs:89–98`), which calls `record_audit(state.postgres.vala_pool(), ...)`, and from shared permission denials (`audit/mod.rs:224–245`). The explicit repository rule permits only `TenantConn` or `OperatorPool` in domain function signatures and fields; the current user direction removes exceptions for raw SQL pools. A caller can pass an arbitrary pool around the tenant/audit capability boundary, and the new tenant-isolation check does not cover this server audit module. Route these standalone audit writes through the existing `ValaPostgres::tenant_conn` owner and canonical append, with transaction lifecycle owned by the audit operation; replace raw-pool arguments at its callers, including the owned transport variant, and extend the boundary check to reject this production signature shape. Prove an identity connection refusal still commits its audit event and that a raw-pool signature fails the check. This is pre-existing code, but the cumulative subject touches this module and the live identity path, and the no-exceptions instruction makes it a current acceptance rule. The unrelated unchanged eval resolver remains outside this finding.

### Suggestions

None.

## Authority coverage

| Changed surface | Applicable authority | Review evidence |
|---|---|---|
| `wyrd-spec` auth contracts, schemas and generator; `wyrd-client` auth projection; CLI and documentation | `AGENTS.md` §§2–4, 8–9, 11, 16; `architecture/wyrd-design.md` client model and runtime identity; `architecture/wyrd-doctrine.mdx` public surfaces; references `architecture/patterns.md`, `languages/rust-core.md`, `languages/testing-workflows.md`, `languages/errors.md` | Typed login/token contracts, retired CLI login, generated schema files and documentation inspected in the cumulative diff. |
| `wyrd-auth-verify`, `wyrd-auth`, `wyrd-server` auth and boot, gateway test | `AGENTS.md` §§4–6, 9, 11, 16; `architecture/agent-rules.md` SQL, audit, structure, SSRF, rustdoc; `architecture/wyrd-security-posture.md` identity, tokens, tenant isolation, cryptography; `architecture/wyrd-design.md` runtime identity; references `architecture/patterns.md`, `languages/rust-core.md`, `languages/testing-workflows.md` | Production owner methods, callback/refresh/revocation path, changed tests and rustdoc inspected; latest correction examined line by line. |
| `wyrd-sql` migration, pool/queries, `wyrd-storage` sweeper, `wyrd-testing` fixture, test SQL helpers | `AGENTS.md` §§3–6, 11, 15–16; `architecture/agent-rules.md` capability signatures, RLS, transaction ownership; `architecture/wyrd-security-posture.md` tenant isolation; references `architecture/patterns.md`, `languages/rust-core.md`, `languages/testing-workflows.md` | Checked changed production SQL signatures and fields separately from construction and test-only raw-pool uses; inspected `WyrdPostgres` and `OperatorPool` use. |
| `mise.toml`, boundary scripts, Docker and IdP fixtures, external Rust tests, docs and change packet | `AGENTS.md` §§11–12, 16; `architecture/agent-rules.md` test placement, gates and generated artifacts; references `languages/spec-driven-development.md`, `languages/implementation-execution.md`, `languages/testing-workflows.md` | Diff and available implementation evidence inspected; journey, integration, codegen and boundary gates mapped to touched surfaces. |

The reference router is `architecture/references/README.md`. No Bifrost, PyO3, Python package, TypeScript package, UI, or analytical source is modified by the production diff, so their specialized authorities do not apply.

## Rule results

| Rule | Result | Source evidence |
|---|---|---|
| Repository ownership and typed wire contracts (`AGENTS.md` §§2–3, 9; architecture patterns) | PASS | Auth request/response types and schemas live in `wyrd-spec`; durable login and SQL state remain server-side; `wyrd-client` projects the wire contract. |
| SQL capability types and transaction ownership (`architecture/agent-rules.md`; `AGENTS.md` §§3, 9) | **FAIL** | The changed auth/boot/SQL-query paths use `TenantConn` or `OperatorPool`, but touched `wyrd-server/src/audit/mod.rs:145–176` still passes raw `PgPool` through production audit signatures; the identity connection path calls it at `components/admin/identity.rs:95`. `PgWorkloadBindingResolver` stores `WyrdPostgres`; `resolve_by_slug` and `expired_uploads_batch` take `&OperatorPool`. Added `fixture.app_pool()` calls are under `pg_tests` and inspect `pg_locks`. No changed `&mut TenantConn` callee commits or rolls back. |
| Security, RLS, audit and secrets (`AGENTS.md` §§2, 9; security posture; agent rules) | PASS | Changed resolver acquires tenant transactions; login-state tenant lookup is the narrow definer bridge on the SQL owner; revocation now locks a User's refresh family before reading it (`revoke.rs:50–68`); activation requires a keyring before changing connection state (`connections.rs:466–472`). |
| Struct-centered Rust and bounded async (`AGENTS.md` §§5–6; rust-core reference) | PASS | New login methods are on `HumanConnections`; SQL resolution methods are on `WyrdPostgres`; async changes await database or HTTP operations. Latest test helpers are fixture-local. |
| Rustdoc accuracy and completeness (`AGENTS.md` §16; `architecture/agent-rules.md` rustdoc rule) | **FAIL** | `pg_resolvers.rs:3–9` still documents a stored `PgPool` and a `wyrd-server` owner after the cumulative change moved the resolver to a `WyrdPostgres` field in `wyrd-auth`. See STD-R11-1. Latest added test helpers and changed revocation documentation have intent and panic/error text. |
| Test tiers, isolation, and gate coverage (`AGENTS.md` §11; agent rules; testing reference) | PASS, with verification limit | Added Postgres tests are in `pg_tests`/`pg_*`; the real-server `identity_e2e` journey covers the public flow. The implementation packet records focused tests, identity journey 27/27, format, lint, codegen, docs, SQL and boundary checks. This reviewer did not rerun those lanes. |
| Generated artifacts and codebase hygiene (`AGENTS.md` §§8, 11–12, 16; agent rules) | PASS | Schema changes have generator/source counterparts; the packet records `codegen:check`. `git diff --check` passed for the cumulative subject. No new dependency, feature, or production clippy suppression appears in the changed source. |

## Open Questions

None.

## Verification Notes

- Read the approved revision-5 spec, original TASK-002 and the R10 remediation packets for surface selection only. Reviewed the cumulative diff, latest fix diff, relevant surrounding implementation and tests, and the available command evidence in the task packets. Other Wave-1 reports were not used.
- `git diff --check 3fc085acf5b3a710d5dc80892bd2e664b3db6174 bae424cc647dad4be80e7debec976d0b7b3e4cf8` passed. No test or build was executed by this read-only reviewer; recorded green lanes are implementation evidence rather than independently reproduced results.
