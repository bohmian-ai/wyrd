# Repository Standards Review — TASK-001 r4

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `c2787b37d456cd2eeeee01e04a9f8bbfdf8866e5`
- Candidate tree: `e389f56318a664920b5ebead0aee45d20fda6b23`
- Range reviewed: the complete cumulative base-to-candidate diff, including the
  original implementation and R1, R2, and R3 remediation.
- CodeGraph: unavailable because this checkout has no `.codegraph/` index.

This report reviews repository standards only. It does not assess task
acceptance or perform the structured Ponytail validation.

## Authority coverage

| Changed surface | Applicable authority | Evidence inspected | Coverage |
|---|---|---|---|
| Repository-wide Rust ownership, imports, async, documentation, dependencies, checks, and verification | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/references/languages/{rust-core,spec-driven-development,implementation-execution,testing-workflows}.md` | Complete cumulative diff, manifests/lockfile, `mise.toml`, check scripts, changed Rust owners and tests | COMPLETE |
| Human OIDC identity, authorization, tenancy, credentials, audit, and SSRF | `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/agent-harness.md` | Connection owner, handlers, login/callback, screened provider IO, recovery authorization, sealing and server composition | COMPLETE |
| Public contracts, OpenAPI, stable errors, and generated TypeScript projection | `AGENTS.md` §§3, 8–9, 11; design/doctrine authorities; `architecture/references/languages/{errors,typescript-guide}.md` | `wyrd-spec` connection/error contracts, server route/OpenAPI registration, generated error union and contract tests | COMPLETE |
| Tenant persistence, migration, refresh/session binding, RLS, and operator rewrap | `architecture/agent-rules.md`; security posture tenant boundary; architecture patterns storage rules; `AGENTS.md` §§3, 9, 15 | Migration, `TenantConn` queries, row types, migration/refresh tests, `WyrdPostgres` and `OperatorPool` owners | COMPLETE |
| Secret files and key rotation | Security posture secret handling; Rust-core secret rules; `AGENTS.md` §§4, 9, 15 | `SealingKeyring`, boot rewrap, bounded owner-only secret-file loader, signing/sealing config tests and runbook | COMPLETE |
| Bifrost protobuf dependency boundary touched by the repaired tonic check | `architecture/bifrost-design.md`; `architecture/agent-rules.md` cross-tier import rule; `check:no-tonic-outside-wyrd-tonic` | Vala manifest and mechanical prost imports/derive paths through `wyrd_tonic` | COMPLETE |
| User journeys, Postgres integration, fixtures, OpenAPI proof, docs, and lane selection | `AGENTS.md` §§11–12, 16; testing-workflows reference | `identity_e2e.rs`, `pg_openapi_contract.rs`, `pg_migration.rs`, fixtures, docs, `mise.toml`, recorded R1/R2/R3 evidence | COMPLETE |
| Python, PyO3, UI, and MCP implementation | Governing authorities checked for applicability | No materially changed implementation surface | NOT APPLICABLE |

## Applicable-rule results

| Rule | Result | Exact evidence |
|---|---|---|
| Durable contracts remain in `wyrd-spec`; durable auth/server and SQL behavior remain with their owners | PASS | Connection wire types and derived errors are in `wyrd-spec`; `HumanConnections` owns lifecycle; handlers adapt HTTP; `wyrd-sql` owns persistence and migration. |
| `wyrd-spec` remains IO-free, async-free, SQL-free, and PyO3-free | PASS | `auth/human_connection.rs` performs synchronous decoding and validation only. |
| Stateful workflows use cohesive concrete owners | PASS | `HumanConnections`, `ScreenedHttp`, `SealingKeyring`, and `SealedSecretRewrap` own their dependencies and expose inherent operations. |
| Tenant SQL uses `TenantConn`, relies on RLS, and query callees do not commit or roll back | PASS | Tenant query functions take `&mut TenantConn<'_>`; transaction lifecycle stays in owners; the migration forces RLS and enforces one Active/one Candidate. |
| Cross-tenant secret maintenance uses explicit `OperatorPool` authority | PASS | Sealed-secret inventory and compare-and-swap rewrap stay on the operator path; tenant handlers do not receive that capability. |
| Raw pools do not escape sanctioned production owners | PASS | Connection/login/resolver owners use `WyrdPostgres`; the newly allowlisted `boot/auth.rs` construction is inside `#[cfg(test)]` and creates a lazy, never-connected test pool. External integration tests retain their established fixture/superuser pool boundary. |
| Tenant identity derives from verified credentials, not request data | PASS | Admin handlers use the authenticated caller's tenant; connection request/path shapes cannot choose another tenant. |
| Every evaluated permission is canonically audited at its commit boundary | PASS | Bearer decisions and the independently evaluated recovery-principal decision append through the canonical tenant transaction; failed recovery audit append rolls activation back. Unresolved credentials fabricate no decision. |
| Public routes use versioned typed contracts, structured Wyrd errors, bounded extraction, and scrubbed instrumentation | PASS | `/v1/identity/oidc/*` handlers advertise typed schemas and use the central error mapper; candidate PUT authorizes before decoding bounded bytes through `ConnectionInput`. |
| Public errors use the derive-backed catalog and generated projections | PASS WITH RECORDED EVIDENCE | Error variants originate in `wyrd-spec::error`; the TypeScript union agrees with them. The task records `codegen:check` green; this static review did not regenerate artifacts. |
| Provider IO is scheme-checked, DNS-bounded, screened, pinned, proxy-free, redirect-free, timed, and response-bounded | PASS | `ScreenedHttp` uses the fixed fetch deadline for DNS and requests, screens every resolved address, pins with `resolve_to_addrs`, disables redirects/proxies, and caps decoded response bodies. The live browser destination reuses the production scheme rule. |
| Secret material remains redacted, sealed, and read only from bounded restrictive regular files | PASS | Connection views omit secrets; secret-bearing owners redact `Debug`; signing and sealing key files share `read_secret_file`; rewrap uses versioned ciphertext and CAS updates. |
| Async is limited to IO/composition boundaries | PASS | Added async functions await DNS, HTTP, SQL, process, or fixture IO; decoding, validation, scheme qualification, and crypto transformations remain synchronous. |
| New/materially modified Rust items have substantive rustdoc and required error/panic sections | PASS | The prior missing `# Errors` on `screening::tests::bounded_get` is present; new R3 helpers and tests document intent and applicable errors/panics. |
| Imports stay at module scope and type names in signatures are imported and bare | **FAIL — STD-R4-001** | New R3 signatures spell `std::io::Result<I>` in `screening.rs:247` and `sqlx::PgPool` in `identity_e2e.rs:2434` instead of importing those types into their module dependency blocks. |
| The repaired repository checks preserve or strengthen their live invariants | PASS | `fixtures-no-server.sh` and `no-tonic-outside-wyrd-tonic.sh` now distinguish an absent match from an `rg` error; the missing `python/` root was removed; the tonic rule is narrowly exempted only for the generated workspace-hack manifest. The from-pools expansion names one documented in-source test site. |
| The Bifrost dependency repair uses the owning re-export without changing wire behavior | PASS | `vala-bifrost-redux` drops direct `prost`, imports/derives through `wyrd_tonic::prost`, and makes no message field or transport behavior change. |
| User-facing HTTP behavior has real-server journeys and served-OpenAPI proof | PASS WITH RECORDED EVIDENCE | Identity journeys cover tenant isolation, lifecycle, recovery audit, rotation, replica visibility, and cutoff; the assembled-server OpenAPI test covers the route contract. R3 records those lanes green; they were not rerun here. |
| Test tasks use minimal features and exact selectors | PASS | The identity lane no longer forces `--all-features`; filtered tests require exactly one selected test and unfiltered execution keeps all journeys. |
| No generated artifact was hand-authored as an independent contract | PASS WITH RECORDED EVIDENCE | The changed TypeScript error union is generator-owned and matches the Rust catalog; R3 records `codegen:check` green. |

## Material repository-rule findings

### STD-R4-001 — New signatures bypass the module import manifest

- **Violated rule:** `architecture/agent-rules.md` requires types to be brought
  into the top-of-module `use` block and used as bare names in signatures. The
  rule explicitly applies to function parameters, return types, trait bounds,
  and `where` clauses, including tests.
- **Locations:**
  - `crates/shared/wyrd-auth-oidc/src/screening.rs:247`
  - `crates/wyrd/wyrd-server/tests/identity_e2e.rs:2434`
- **Evidence:** The R3-added `bounded_lookup` declares
  `Future<Output = std::io::Result<I>>`; the R3-added `api_key_id` declares
  `superuser: &sqlx::PgPool`. Both types are written through crate-qualified
  paths in new signatures even though their modules have top-level import
  blocks.
- **Consequence:** The true dependency surface is split between imports and
  signatures, reintroducing the exact repository-shape violation corrected in
  earlier remediation. This is a mandatory style rule even though compilation,
  formatting, and Clippy accept the code.
- **Testable correction:** Import the IO result type (with a non-conflicting
  alias if needed) in `screening.rs` and `PgPool` in the identity test module,
  then use the bare imported names in the two signatures. Run `mise run fmt`,
  `mise run lints`, the focused stalled-DNS test, and the filtered rotation
  journey. No behavioral code or new abstraction is needed.

## Prior standards-finding closure

- The R3 missing-`# Errors` finding on `bounded_get` is closed at
  `screening.rs:518-527`.
- Earlier raw-pool ownership, audit-cardinality, Rust documentation,
  function-local import, secret-file permission, typed-handler, and test-feature
  findings remain closed in the cumulative candidate.
- STD-R4-001 is new in the R3 remediation delta; it is not a reopened behavior
  defect.

## Verification limits

- This was a static inspection of the complete immutable cumulative range and
  surrounding owners. Per assignment, no Cargo-backed or `mise` lane was run.
- `git diff --check base..candidate` completed successfully.
- The R3 implementation record reports green format, lints, diff, codegen,
  docs, principals/OpenAPI integration, SQL and platform journeys, filtered and
  unfiltered identity journeys, tenant/pool/fixture/tonic/client/PyO3/unwrap/
  Clippy-allow boundary checks, the Postgres-wrapped auth suite, and focused
  Vala codec/peer/signal tests. Those results are recorded evidence, not
  independently re-executed proof in this review.
- Controlled-provider qualification against external Okta, Keycloak, and Entra
  accounts was not performed; local repository-managed providers and mocks are
  the available evidence.
- No authority or changed source surface required for this standards judgment
  was unavailable.

## Overall result

**FAIL**

The cumulative candidate conforms to the reviewed ownership, tenancy, audit,
security, secret, contract, testing, Bifrost dependency, and verification-shape
rules. Two new qualified type paths in signatures violate the repository's
mandatory import rule. The correction is mechanical, bounded, and requires no
specification or architecture decision.
