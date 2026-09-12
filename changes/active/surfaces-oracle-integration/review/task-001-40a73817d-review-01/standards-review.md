# TASK-001 repository-standards review

## Immutable subject

- Base: `089f626c7681f4c8bf8abdaddedb61e4a35a26d5`
- Candidate: `40a73817d415e9a1626e6ec7a91edda083e344d3`
- Result: **FAIL**
- CodeGraph: unavailable (`.codegraph/` is absent)

## Authority coverage

| Changed surface | Applicable authority | Result |
|---|---|---|
| Active specification and tasks | `AGENTS.md` §§14–16; spec-driven development; implementation execution | FAIL |
| Redux Gate, Scribe, Forge, and tables | Bifrost design; security posture; Rust, Vala, OLAP, Iceberg, DataFusion, and reliability references | FAIL |
| Audit staging and publication state | Agent SQL/audit rules; security posture; recovery authorities | FAIL |
| Server publisher, query, and Oracle WAL | `AGENTS.md` ownership/server/Rust rules; Bifrost and security authorities | FAIL |
| Contracts, schemas, and OpenAPI | Wyrd design/doctrine; errors and agent-harness references; generated-artifact rules | PASS |
| MCP and public documentation | Agent-surface, documentation, and verification rules | FAIL |
| Tests and fixtures | `AGENTS.md` §11; testing workflows | FAIL: placement passes; required proof is incomplete |
| Dependencies, PyO3, and client tier | Ownership and dependency rules | PASS |
| Git identity | `AGENTS.md` §13 | PASS |

## Material standards findings

### STD-001 — Raw pool crosses into library state

`crates/wyrd/wyrd-server/src/audit/publication.rs:4,59-62,92` materially rewrites `AuditPublisher` while storing `sqlx::PgPool`. This violates `architecture/agent-rules.md`'s rule that library fields and signatures use only `TenantConn` or `OperatorPool` connection capabilities. Retain the existing Postgres owner and acquire tenant connections through it. `check:from-pools-allowlist` does not cover raw-pool propagation.

### STD-002 — TenantConn queries duplicate the RLS tenant boundary

New freeze, range-read, and settlement SQL in `crates/vala/vala-sql/src/queries/audit_staging.rs:234,244,267,296,336,346,350` manually adds `data_tenant_id = wyrd.current_tenant()` while accepting `TenantConn`. Agent rules prohibit this second tenant mechanism. Remove the manual tenant predicates and preserve the sequence/range predicates and RLS-backed tests.

### STD-003 — Rustdoc hard blockers

The candidate deletes the module rustdoc from `wyrd-server/src/audit/publication.rs`; its fallible `freeze` and `read_range` lack `# Errors`; the changed Gate test seam and test lack required documentation; materially changed card-audit helpers are not brought to §16 compliance; and `append_managed_columns` still documents unconditional correlation fields although the implementation is conditional. `AGENTS.md` §16 classifies this as `BLOCK_BEFORE_MERGE`.

### STD-004 — Fully-qualified types remain in signatures

`gate/mod.rs:1287,1299,1313` and `audit/publication.rs:240` use fully-qualified durable types in fields/signatures instead of top-level imports and bare names, contrary to `architecture/agent-rules.md`.

### STD-005 — Governing authority and public docs contradict the audit boundary

`architecture/bifrost-design.md:454-480` defines authorization-only audit, but `:685-688` still requires every durable transition and Forge operation to append audit. The same obsolete model remains in `references/domain/vala-architecture.md:77-79`, `references/languages/rust-core.md:601-605`, `references/domain/olap-serving.md:38-39`, and Bifrost public docs including `forge.svx:164-184,206-211`, `architecture.svx:46,175-193,334`, and `data-plane-internals.svx:41-44,84,101`. Align these with authorization decisions, operational lineage, and frozen-range publication.

### STD-006 — Required verification is incomplete

The task explicitly records that `mise run verify:bifrost` was not run. A runtime MCP query test and files under `docs/` changed without recorded MCP runtime coverage or `mise run docs:check`. Leaf lanes do not substitute for the capability gate required for this broad cross-owner change.

### STD-007 — Task lifecycle metadata is invalid

`TASK-001` uses unsupported status `implemented`; `TASK-005` and `TASK-006` remain `ready` despite completion evidence. The permitted lifecycle requires `review` before a verdict and `approved` only after PASS.

## Passing rule areas

Durable behavior remains in Rust server/Vala owners; Oracle remains WAL-first; the local Scribe path does not call Gate; async additions await IO; the greenfield migration edit is explicitly authorized by revision 7; generated artifacts are synchronized; test binaries/harnesses were reused; no new dependency, feature, PyO3, or client-tier violation was found; and commit identity matches repository policy.

## Verification limits

The specialist ran `git diff --check` and `mise run check:from-pools-allowlist`; both passed. Other results are the candidate's recorded evidence. `verify:bifrost`, the owning MCP runtime proof, and `docs:check` are unavailable.
