# TASK-006 R5 repository standards review

## Subject and authority coverage

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`; `.codegraph/` is absent.
- Cumulative base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`; immutable candidate: `6ce9f9bb7e2dea288a1b78081346551d896cb7a8`.
- Reviewed the cumulative changed-file inventory, the R4 delta and its consumers, prior review packet for historical boundaries, and committed verification evidence. This is a standards audit; no test was rerun here.

| Changed surface | Applicable authority | Result and source evidence |
| --- | --- | --- |
| Spec, tasks, review workflow | `AGENTS.md` §§1, 11–16; `architecture/agent-rules.md`; `architecture/references/languages/{spec-driven-development,implementation-execution,testing-workflows}.md` | PASS: revision 43 records user approval for edge-terminated public TLS; R4 records focused and broader lanes, including exact named-test commands. |
| Eval, observations, Bifrost Scribe/Oracle/Forge, private wire | `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/bifrost-design.md`; `AGENTS.md` §§2–6, 9–12, 16; reference router's `architecture/patterns.md`, `domain/{vala-architecture,evaluation,olap-serving,iceberg,datafusion,analytical-operations-reliability}.md`, `languages/{rust-core,testing-workflows}.md` | PASS on standards inspected: server/Vala ownership is preserved; R4's Oracle metric uses the existing `oracle_query_duration_seconds_count` series, and kind evidence records an Oracle 1→2 scale. Domain correctness is reviewed separately. |
| SQL, migrations, RLS, serving roles, fixtures | `architecture/agent-rules.md` SQL-handle and transaction rules; `architecture/v1/00-foundations/{postgres-layout,sql-foundation,tenancy}.md`; `architecture/wyrd-security-posture.md`; `AGENTS.md` §§2–6, 9, 11–12, 16; reference `architecture/patterns.md` and `languages/rust-core.md` | **FAIL SR-R5-1:** the newly added public `From<PgPool> for SqlStore` function accepts raw `PgPool`. The lease itself remains in `SqlStore`, and the owner command retains a bounded detached session. |
| Peer trust, public/peer gRPC, auth and audit | `architecture/wyrd-security-posture.md`; `architecture/operations/deployment-and-release.md`; `architecture/v1/00-foundations/{security,permission-model,permission-check}.md`; `AGENTS.md` §§2, 9–12; reference `architecture/patterns.md`, `languages/errors.md` | PASS for the R4 code change: public gRPC certificate inputs were removed with revision-43 approval; `WYRD_PEER_TLS_DIR` and private peer mTLS remain, and the production peer config test passes. The deployment documentation has a separate failure below. |
| Official image, Kubernetes manifests, scripts, self-hosting docs | `architecture/operations/{deployment-and-release,runbooks}.md`; `architecture/wyrd-design.md`; `architecture/bifrost-design.md`; `AGENTS.md` §§1, 11–12; reference `doctrine/architecture-constraints.md`, `languages/testing-workflows.md` | **FAIL SR-R5-2:** the production guide's public ingress route contradicts the operations authority's authenticated and encrypted gateway-to-server hop. Image startup and kind lanes pass but do not exercise that hop. |
| Rust/Python/TypeScript SDK, UI, generated declarations in the cumulative range | `AGENTS.md` §§2–9, 11–12, 16; `architecture/wyrd-design.md`; reference `languages/{rust-core,pyo3-boundaries,python-api-and-stubs,typescript-guide,errors,testing-workflows}.md` | PASS on available cumulative evidence: SDKs project the shared `wyrd-client` transport; recorded R2 journeys, codegen, typecheck and docs checks passed, and R4 changed no SDK/UI surface. |
| Tests, CI, tooling, shell, generated docs | `AGENTS.md` §§8, 11–12, 14–16; `architecture/agent-rules.md`; reference `languages/testing-workflows.md` | PASS with evidence limit: R4 records `test:sql`, startup, kind, peer, `check:from-pools-allowlist`, `docs:check`, `fmt`, `lints`, `git diff --check` passing. The pool check guards construction sites and does not settle SR-R5-1. |

## Applicable rule results and material findings

| Rule | Source evidence | Result |
| --- | --- | --- |
| Library SQL signatures use typed connection capabilities, and raw pools remain at sanctioned construction/fixture boundaries (`architecture/agent-rules.md`, SQL-handle rule) | `crates/wyrd/wyrd-sql/src/lib.rs:298-303` adds public `impl From<PgPool> for SqlStore`; its body is a raw-pool-to-owner-handle conversion. Callers are `crates/shared/wyrd-dev-fixtures/src/pg.rs:382`, `crates/wyrd/wyrd-sql/tests/{pg_migration,pg_admin_principals}.rs`, and `crates/vala/vala-sql/tests/pg_migration.rs`. The production migration command instead calls `SqlStore::connect_with` at `wyrd-server/src/main.rs:202`. | **FAIL SR-R5-1** |
| Gateway-to-server traffic is authenticated and encrypted; plaintext is confined to loopback development or a mutually authenticated policy-enforced local transport (`architecture/operations/deployment-and-release.md:28-32`) | `docs/src/content/docs/self-hosting/kubernetes-production.svx:365-371` instructs TLS termination at ingress then routes to plaintext image ports 8080/50051; `:355-356` admits those ports from every source, suggesting ingress-only restriction only as an optional narrowing at `:342-343`. The official nginx template uses plaintext listeners, including `grpc_pass grpc://127.0.0.1:50053`. | **FAIL SR-R5-2** |
| Approved edge-only public TLS and separate peer mTLS (`spec.md:1397,2227-2233`; `architecture/wyrd-security-posture.md`) | R4 removes public gRPC certificate inputs from `config.rs` and identity loading from `app/server.rs`, while the private listener remains independently TLS-configured through `load_peer_tls`. | PASS for code boundary |
| Top-level imports and bare signature types (`architecture/agent-rules.md`) | `grpc/mod.rs:34` imports `TcpConnectInfo`/`TlsConnectInfo` at module top; `state.rs:34,666,1040,1702` imports and uses `ServerBootError`. | PASS; prior R4 style findings closed |
| Rustdoc, struct-owned IO, narrow async (`AGENTS.md` §§5–6, 16) | New `SqlStore::migration_lease` documents owner-session lease, bounded waiting, cancellation and errors (`wyrd-sql/src/lib.rs:226-274`); R4 config test has purpose docs. | PASS for R4 touched items |
| Journey and targeted gate integrity (`AGENTS.md` §§11–12) | R4 task evidence records named SQL and config tests with exact selectors, SQL/startup/kind/peer lanes, docs, format, lint and diff checks. The kind run measures Oracle executions and retains mTLS refusal. | PASS on recorded evidence; this read-only audit did not rerun |

### SR-R5-1 — Public raw-pool conversion reopens the SQL owner boundary

- **Rule:** `architecture/agent-rules.md` permits only `TenantConn` and `OperatorPool` as library SQL connection abstractions in signatures and restricts pool construction to named owners and fixtures.
- **Location and evidence:** `crates/wyrd/wyrd-sql/src/lib.rs:298-303` exposes `From<PgPool> for SqlStore`. All five call sites are fixtures or integration tests; the production migrator uses `SqlStore::connect_with` and does not need the conversion.
- **Consequence:** A production library caller can wrap any `PgPool` as the migration owner and call `migration_lease` through a raw-pool entrypoint; the R4 fix has moved the forbidden signature from `MigrationLease::acquire` to a public conversion. Passing `check:from-pools-allowlist` does not detect this signature.
- **Testable correction:** Remove the public conversion. Use the existing owner-DSN `SqlStore::connect_with` path where tests need a migration handle, while keeping raw assertion pools in their fixture-only role. Prove no new raw-pool production library signature remains and rerun the named competing-migrator test and SQL/startup lanes.

### SR-R5-2 — Production guide omits the encrypted ingress-to-pod hop

- **Rule:** `architecture/operations/deployment-and-release.md:28-32` requires authenticated, encrypted gateway-to-server transport.
- **Location and evidence:** `docs/src/content/docs/self-hosting/kubernetes-production.svx:339-371` has unrestricted ingress to plaintext HTTP/gRPC ports and instructs external TLS termination at ingress without securing the subsequent hop; the image's nginx listens without TLS.
- **Consequence:** A deployment following the guide sends client requests from ingress to Wyrd pods unencrypted and admits direct in-cluster connections to the same public ports. Peer mTLS secures only port 50052 and does not cover these requests.
- **Testable correction:** Specify an existing Kubernetes or ingress mechanism that authenticates and encrypts ingress-to-pod transport, with plaintext confined to the same pod's policy-enforced local hop, and make public-port NetworkPolicy source restriction mandatory. Validate the rendered policy and the selected transport path in the guide walkthrough; no new Wyrd certificate option is needed.

## Overall result

**FAIL.** Two material repository-rule findings, SR-R5-1 and SR-R5-2. The cumulative candidate remains at the stated immutable hash during this review.
