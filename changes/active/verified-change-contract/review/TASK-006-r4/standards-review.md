# TASK-006 R4 repository standards review

## Immutable subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`; CodeGraph is not indexed (`.codegraph/` absent).
- Original base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`.
- Candidate: `58cabb529b93da366959db796ac3b596a6c6c1e6` (HEAD at review start); prior candidate: `2fbe90cd880936d8e4f25173839e4cc4fa18f0b4`.
- Scope: complete base-to-candidate file inventory and the R3 delta, with surrounding source for the touched startup, SQL, peer, test, script, and documentation owners. Verification is the committed R3 task evidence, not a fresh rerun.

## Authority coverage

| Changed surface | Governing authority | Inspection and result |
| --- | --- | --- |
| Spec, task, review packet, workflow skills | `AGENTS.md` §§1, 14, 16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`, `implementation-execution.md`, `testing-workflows.md` | Approved revision 41 records local immutable image ID for pre-release acceptance; R3 evidence records named tests and lanes. PASS. |
| Eval, observations, Bifrost Scribe/Oracle/Forge, private wire, SQL | `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`; `AGENTS.md` §§2–6, 9–12, 15–16; reference `architecture/patterns.md`, `domain/vala-architecture.md`, `domain/evaluation.md`, `domain/olap-serving.md`, `domain/iceberg.md`, `domain/datafusion.md`, `domain/analytical-operations-reliability.md`, `languages/rust-core.md`, `languages/testing-workflows.md` | Cumulative diff includes server-owned Eval and Bifrost behavior, receiver-owned peer checks, and SQL migration/readiness. R3 SQL has one explicit handle-boundary failure below; targeted journeys and SQL tests are recorded passing. FAIL. |
| Authentication, peer mTLS, audit, tenant role separation | `architecture/wyrd-security-posture.md`; `architecture/v1/00-foundations/{security,tenancy,permission-model,permission-check,postgres-layout,sql-foundation}.md`; `architecture/agent-rules.md` audit and SQL rules; `AGENTS.md` §§2, 7–10 | R3 adds fixed peer SAN check, receiver-bound audit attribution, exact serving logins, policies/grants and migration lease. Peer admission adds a prohibited local import. FAIL. |
| Server startup, official image, Kubernetes examples, self-hosting docs | `architecture/operations/{README,deployment-and-release,reliability-and-recovery,runbooks}.md`; `architecture/wyrd-design.md`; `architecture/bifrost-design.md`; `AGENTS.md` §§1, 11–12; reference `doctrine/architecture-constraints.md`, `architecture/patterns.md`, `languages/testing-workflows.md` | R3 scripts record local image ID and commit, docs describe local/dev/prod journeys and distinguish published release image; startup/kind/docs lanes recorded passing. PASS subject to the findings. |
| Rust/Python/TypeScript SDK and UI changes in cumulative range | `AGENTS.md` §§2–9, 11–12, 16; reference `languages/{rust-core,pyo3-boundaries,python-api-and-stubs,typescript-guide,errors,testing-workflows}.md`; `architecture/wyrd-design.md` | SDK changes are projections of client/server error and config contracts; Python/TypeScript tests and codegen lanes are recorded in earlier candidate evidence. No new R3 SDK/UI change. PASS on available evidence. |
| Generated artifacts, manifests, shell and CI tasks | `AGENTS.md` §§8, 11–12, 14–16; `architecture/agent-rules.md`; reference `languages/testing-workflows.md`; operations deployment authority | R3 adds image-provenance checks and self-hosting pages; no generated schema/stub modification in R3. Docker/kind and documentation checks recorded passing. PASS. |

## Rule results

| Rule | Evidence | Result |
| --- | --- | --- |
| Use only `TenantConn`/`OperatorPool` in library SQL signatures; keep pool construction at approved boundaries (`agent-rules.md:6`) | New public `MigrationLease::acquire(owner: &PgPool, …)` at `wyrd-sql/src/lib.rs:85` is called from the server, fixture and `SqlStore` and passes a raw pool across the library API. | **FAIL SR-1** |
| Imports at module top (`agent-rules.md:10`) | New `TransportPlane::admits` imports `TcpConnectInfo`/`TlsConnectInfo` inside its method at `wyrd-server/src/grpc/mod.rs:164`. | **FAIL SR-2** |
| Bare imported types in signatures (`agent-rules.md:9`) | New `Bifrost::activate_peer_roles` and two role `activate` methods return `crate::boot::ServerBootError` in signatures at `wyrd-server/src/state.rs:666,1040,1702`. | **FAIL SR-3** |
| Rustdoc for touched items, errors and side effects (`AGENTS.md` §16) | R3 `Tee` writer, lease, schema checks, peer admission and readiness methods carry intent and error docs; no missing R3 declaration docs found. | PASS |
| Server-owned durable behavior, least-privilege serving, RLS and tenant audit (`AGENTS.md` §§2, 3, 9; `agent-rules.md:11–15`) | `WyrdPostgres`/`ValaPostgres` validate exact serving logins and migrated policies; peer refusal chain selection is receiver-bound. Recorded SQL and peer lanes pass. | PASS, except SR-1 API boundary |
| First-class journey and runtime-owned test coverage (`AGENTS.md` §11; `agent-rules.md:18`) | Committed evidence records startup, kind, server, Oracle, SQL and peer journeys; no Python or TypeScript surface changes in R3. | PASS on recorded evidence |
| Generated artifacts and gates (`AGENTS.md` §§8, 11–12; `agent-rules.md:28`) | R3 evidence records `docs:check`, `fmt`, `lints`, SQL, peer, startup, kind, exact named tests and `git diff --check`; no R3 contract/stub change requires codegen. | PASS on recorded evidence |
| Architecture and operations authority (`AGENTS.md` §§1–2; operations README) | Updated security, Bifrost, deployment and runbook prose uses dedicated-CA peer mTLS and removes the normative ticket path; docs use approved local-image acceptance. | PASS |

## Material repository-rule findings

### SR-1 — Raw Postgres pool in the public migration lease API

- **Rule:** `architecture/agent-rules.md:6` permits only typed SQL handles in library signatures and makes pool construction a narrow owner boundary.
- **Location/evidence:** `crates/wyrd/wyrd-sql/src/lib.rs:85` declares `pub async fn MigrationLease::acquire(owner: &PgPool, …)`; `wyrd-server/src/main.rs:207`, the fixture and `SqlStore` call it. The passing `check:from-pools-allowlist` is a narrower construction check and does not establish this signature is allowed.
- **Consequence:** Any caller with a raw pool can enter the privileged migration lease path; the SQL owner API advertises a connection abstraction repository rules forbid, and the rule remains unenforced for this site.
- **Testable correction:** Keep the owner-only migration session, but move lease acquisition to the existing `wyrd-sql` migration owner so callers use a typed owner/DSN boundary rather than pass `&PgPool`; verify the signature is gone and rerun SQL/startup and the pool-boundary check.

### SR-2 — Function-scoped peer transport import

- **Rule:** `architecture/agent-rules.md:10` places all imports at module top, with exceptions only for test modules and narrow `Trait as _` generic use.
- **Location/evidence:** `crates/wyrd/wyrd-server/src/grpc/mod.rs:164` adds `use wyrd_tonic::tonic::transport::server::{TcpConnectInfo, TlsConnectInfo};` inside `TransportPlane::admits`; neither exception applies.
- **Consequence:** The private listener's trust dependency is hidden from the module import list, violating the required dependency-manifest shape.
- **Testable correction:** Move the two type imports to the module's top import group; format and lint.

### SR-3 — Fully qualified error types in new activation signatures

- **Rule:** `architecture/agent-rules.md:9` requires importing types and using bare names in function return types.
- **Location/evidence:** `crates/wyrd/wyrd-server/src/state.rs:666,1040,1702` uses `Result<(), crate::boot::ServerBootError>` for R3 activation methods.
- **Consequence:** The new role activation API is structurally inconsistent with the mandated import discipline; later ownership changes have multiple signature sites to repair.
- **Testable correction:** Import `ServerBootError` once at module top and use the bare name in these signatures; format and lint.

## Overall result

**FAIL.** The recorded verification is credible for behavior, but the candidate violates three explicit repository rules in R3-touched code. These are bounded standards corrections; this report makes no task-acceptance judgment.
