# Repository standards review

## Review Findings

### Critical

None.

### Important

- `RS-R6-1` —
  `[architecture/references/languages/testing-workflows.md:165]` and
  `[architecture/references/languages/agent-harness.md:80]` still prescribe
  the removed `wyrd-server --lib http::openapi` test surface. A documented
  green command can therefore select zero OpenAPI contract tests. Update both
  references to the current `mise run test:principals:integration` /
  `pg_openapi_contract` proof.
- `RS-R6-2` — `[crates/wyrd/wyrd-mcp/src/client.rs:66]` and the additional
  locations inventoried below retain candidate-added qualified type paths in
  declarations. This violates the mandatory bare-name rule. Import the types
  at module scope, use bare aliases in every candidate-added declaration, and
  prove closure with a cumulative declaration scan plus format and lint.

### Suggestions

None; optional improvements are outside this review.

## Immutable subject

| Item | Value |
|---|---|
| Repository | `/home/thorrester/Documents/GitHub/wyrd` |
| Base | `c5c20754a167e8f4d74a555a720bd51df6179a6f` |
| Candidate | `2c0408b683f7a548cec6dd08b35698d761d33b31` |
| Reviewed range | Complete cumulative base-to-candidate diff: 360 files, 54,959 insertions, 7,453 deletions |
| Candidate at start | `2c0408b683f7a548cec6dd08b35698d761d33b31` |
| Candidate at completion | `2c0408b683f7a548cec6dd08b35698d761d33b31` |

This is an independent repository-standards audit. It does not decide task
acceptance and does not reopen the branch owner's explicit, full waiver of
`FIND-TASK-001-10` or the owner's approval of the cumulative
`verified-change-contract` content.

## Authority coverage

| Changed surface | Applicable authority read and applied | Coverage result |
|---|---|---|
| Active specification, tasks, remediation packet, review evidence, and repository verification wiring | `AGENTS.md` §§11, 14-16; `architecture/agent-rules.md`; `architecture/references/README.md`; `architecture/references/languages/spec-driven-development.md`; `.agents/skills/wyrd-task-review/SKILL.md` | Complete. The immutable range and current approved packet are identifiable. Verification-command drift fails below as `RS-R6-1`. |
| Principal and credential wire types, permissions, public errors, generated JSON schemas, and Rust SDK projection | `AGENTS.md` §§2-4, 8-9; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/errors.md`; `architecture/references/languages/rust-core.md` | Complete. Server-owned durable behavior, typed contracts, stable errors, and shared-client ownership conform. Rust declaration style fails below as `RS-R6-2`. |
| Platform/tenant authentication, authorization, OIDC screening, sessions, refresh/revocation, recovery, and secret handling | `architecture/wyrd-design.md` doctrine 18; `architecture/wyrd-security-posture.md`; `architecture/agent-rules.md` audit, SQL, and SSRF rules; `architecture/references/architecture/patterns.md`; `architecture/references/languages/errors.md` | Complete. Plane separation, verified tenant derivation, fail-closed decisions, secret redaction, fixed-cost refusal, refresh/revocation anchoring, and screened/pinned provider IO conform. |
| Tenant and platform SQL, RLS/operator access, migrations, transactional authorization audit, and Vala audit staging | `AGENTS.md` §§2-3, 9-12; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/operations/deployment-and-release.md`; `architecture/bifrost-design.md`; `architecture/references/domain/vala-architecture.md`; `architecture/references/domain/olap-serving.md`; `architecture/references/domain/analytical-operations-reliability.md` | Complete. `TenantConn`/`OperatorPool` ownership, canonical staging, effect/decision transaction coupling, retained projection, and forward-only migration behavior conform. |
| Server routes, runtime OpenAPI composition, local storage wildcard regression fix, HTTP errors, and route tests | `AGENTS.md` §§9, 11, 16; `architecture/wyrd-design.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/agent-harness.md`; `architecture/references/languages/errors.md` | Complete. Handler and document co-registration and the local wildcard routing correction conform, but two routed references advertise an obsolete proof command (`RS-R6-1`). |
| Shared HTTP authentication replay, CLI, Rust SDK, and MCP principal tools/catalog | `AGENTS.md` §§2-4, 9, 11; `architecture/wyrd-design.md` client model; `architecture/references/architecture/patterns.md`; `architecture/references/languages/agent-harness.md`; `architecture/references/languages/errors.md`; `architecture/references/languages/testing-workflows.md` | Complete. One shared transport owns credential renewal/replay; MCP retains protocol framing and uses typed shared DTO schemas; CLI and SDK remain projections. |
| Bifrost audit schema/projection, Scribe publication consumers, and analytical integration tests | `AGENTS.md` §§2, 10-11; `architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`; `architecture/references/domain/vala-architecture.md`; `architecture/references/domain/olap-serving.md`; `architecture/references/domain/analytical-operations-reliability.md` | Complete. Nullable credential attribution is carried through the canonical staged-to-retained path without adding a second audit authority or weakening table identity. |
| Documentation, generated artifacts, manifests, lockfile, scripts, and `mise` lanes | `AGENTS.md` §§1, 4, 11-12, 16; `architecture/agent-rules.md`; `architecture/references/languages/agent-harness.md`; `architecture/references/languages/testing-workflows.md`; crate/workspace manifests and `mise.toml` | Complete. Generated schema pairs and lockfile/manifests are coherent; codegen evidence is present. The stale OpenAPI command is the material exception. |
| Python, PyO3, TypeScript, and UI-specific rules | `AGENTS.md` §§7-8 and routed language boundaries | Not applicable: the cumulative candidate adds no Python, PyO3, TypeScript, or UI implementation surface for this capability. |

## Rule-by-rule results

| Repository rule | Result | Evidence |
|---|---|---|
| Durable behavior stays in server/Rust owners; clients project the wire contract | **PASS** | Principal, platform, auth, SQL, and audit behavior remains in `wyrd-auth`, `wyrd-server`, `wyrd-sql`, and Vala owners. `wyrd-client`, CLI, MCP, and the Rust SDK call those server surfaces. |
| Principal vocabulary, two-plane identity, typed permissions, and public error catalog follow current architecture | **PASS** | The closed five principal kinds, tenant/platform separation, typed request/response DTOs, and derive-backed problem codes are consistent across `wyrd-spec`, runtime, server, generated schemas, docs, and consumers. |
| Tenant SQL uses `TenantConn`; privileged cross-tenant work uses `OperatorPool`; callees do not end caller-owned tenant transactions | **PASS** | Changed SQL signatures use the sanctioned capabilities, RLS remains the tenant boundary, and the R5 transaction correction keeps raw SQLx transactions inside their owning platform handle. `check:tenant-isolation` is recorded green. |
| Every authorization decision uses the one canonical audit append and is transactionally coupled to its effect/refusal | **PASS** | Tenant and platform handlers append to `vala.audit_staging`; stable no-effect outcomes commit their evaluated decision; mutation/store failures roll it back; only `AuditPublisher` projects retained history. Focused no-effect and failed-mutation tests are recorded green. |
| Secrets are redacted; provider URLs are screened at the effective address and pinned before use | **PASS** | Secret-bearing connection/session/config types have redacted `Debug`; `ScreenedHttp` is used on discovery and subsequent provider requests; internal/link-local rejection and permissive-local tests are present. |
| Public HTTP/MCP/CLI surfaces use typed contracts, stable errors, explicit write scopes, and one authentication policy owner | **PASS** | MCP principal input/output schemas come from `wyrd-spec`; dispatch retains write-scope enforcement; `HttpTransport::authenticated_replay` owns bounded renewal/replay for normal and MCP calls. |
| Runtime OpenAPI is composed from mounted handler registration and has a repository-native, nonzero proof command in all applicable guidance | **FAIL** | Registration now uses `OpenApiRouter`, and `mise.toml:153` runs `--test pg_openapi_contract`; however, `architecture/references/languages/testing-workflows.md:165` and `architecture/references/languages/agent-harness.md:80` still direct maintainers to the removed `--lib http::openapi` slice. See `RS-R6-1`. |
| Generated schemas/goldens are source-derived and drift-free; runtime OpenAPI is not a checked-in duplicate | **PASS** | Source DTOs and both schema trees changed together, `openapi.yaml` and its generator were removed, and `codegen:check` is recorded clean with no drift. |
| Applied migrations are immutable and schema evolution is forward-only | **PASS** | `20260802000000_vala_audit_staging.sql` is byte-identical to the base; `20260910000027_audit_staging_credential_id.sql` is the forward nullable-column migration; upgrade and row-survival proofs are recorded green. |
| Rust uses cohesive concrete owners, narrow async IO boundaries, top-level imports, and bare type names in declarations | **FAIL** | The owner/async/import-location portions conform, but candidate-added fields, signatures, associated types, bounds, and return types still contain qualified paths. See `RS-R6-2`. |
| New/materially modified Rust items have workflow-level rustdoc, `# Errors`, and relevant panic/cancellation/partial-progress documentation | **PASS** | Inspection of changed owners and tests found the required documentation; the recorded strict rustdoc and cumulative diff audit are green. |
| Tests follow tier and placement rules and user/agent-facing behavior has real client-to-server journeys | **PASS** | SQL/Postgres cases use `pg_*`; external tests drive Postgres, HTTP/MCP, or the compiled CLI; platform, identity, CLI, principal, MCP, SQL, Bifrost, storage, and served-OpenAPI lanes are represented. |
| Verification uses canonical `mise` lanes and exact nonzero selectors without gate circumvention | **PASS with finding noted above** | R5 evidence records focused nonzero selectors and all required broader lanes. No check/test was disabled or ignored. `RS-R6-1` concerns future guidance, not the recorded R5 run, which used the live integration target. |
| Git provenance rule | **WAIVED / not reopened** | The current user/branch owner explicitly waived all of `FIND-TASK-001-10`, including historical identities and AI trailers. User authority outranks repository policy for this branch-scoped review. |

## Material findings

### `RS-R6-1` — VIOLATION — OpenAPI verification references select the removed test surface

- **Violated rule:** `AGENTS.md` §11 requires the repository-native task that
  actually owns an environment-backed test and forbids selectors that can pass
  after selecting no test. `architecture/references/README.md` requires focused
  references to remain aligned with governing repository authority.
- **Location:** `architecture/references/languages/testing-workflows.md:160-166`;
  `architecture/references/languages/agent-harness.md:77-81`.
- **Evidence:** R5 moved the OpenAPI contract proof into
  `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs` and wires it through
  `mise.toml:153` / `mise run test:principals:integration`. The current
  `wyrd-server --lib` test inventory contains no `http::openapi` test, while
  both references still prescribe `cargo test ... --lib http::openapi`.
  `AGENTS.md:453-460` already names the current proof, so the authorities now
  contradict one another.
- **Observable consequence:** A maintainer or agent following either routed
  reference can run a green command that exercises zero OpenAPI contract tests,
  leaving route/schema/error drift unproved despite following the documented
  workflow.
- **Required testable correction:** Update both references to the same current
  environment-owning proof named by `AGENTS.md`—`mise run
  test:principals:integration` and its `pg_openapi_contract` target—and verify
  that the documented command selects and passes the served-document suite.
  Do not reintroduce a duplicate lib suite or OpenAPI snapshot.

### `RS-R6-2` — VIOLATION — candidate-added Rust declarations still use qualified type paths

- **Violated rule:** `architecture/agent-rules.md:9` requires types to be
  imported at module scope and used by bare name in struct fields, function
  parameters and returns, associated types, trait bounds, and `where` clauses.
- **Location:** Representative production violations are
  `crates/wyrd/wyrd-mcp/src/client.rs:66,103,106,123` (`reqwest::Client` /
  `reqwest::Error`), `crates/wyrd/wyrd-sql/src/queries/auth/api_keys.rs:66,96`,
  `queries/auth/revocation.rs:111`, `queries/auth/role_assignments.rs:147`,
  `queries/auth/service_accounts.rs:232`, and
  `queries/platform/tenant_resolver.rs:54` (`sqlx::Error`). Candidate-added
  `fmt::Result` return types also remain in
  `crates/shared/wyrd-client/src/auth.rs:126,280`,
  `eval/handle.rs:40,102`, `platform/handle.rs:38`,
  `principals/handle.rs:30`, `crates/wyrd/wyrd-auth/src/platform_authz.rs:114`,
  `platform_login.rs:99`, `platform_sessions.rs:120`, and server platform
  owners at `components/platform/provisioning.rs:116` and
  `components/platform/recovery.rs:48`.
- **Evidence:** These declaration lines are additions in the cumulative
  base-to-candidate diff. The MCP module already imports
  `reqwest::Error as ReqwestError` but still uses `reqwest::Error` in the new
  return, bound, and associated type, making the missed conversion directly
  observable. The R5 recorded `fmt:check` and Clippy lanes do not enforce this
  repository-specific source rule.
- **Observable consequence:** The candidate does not satisfy the mandatory
  declaration-style boundary, and module dependency ownership remains obscured
  precisely on the new shared transport and SQL APIs the change introduced.
- **Required testable correction:** Extend each affected module-top import
  block with unambiguous aliases where necessary (`ReqwestClient`,
  `ReqwestError`, `SqlxError`, `FmtResult`, or equivalent) and replace every
  candidate-added qualified declaration type with its bare imported name.
  Leave expression paths and untouched baseline declarations unchanged. Prove
  closure with a cumulative base-to-candidate declaration scan plus
  `mise run fmt:check` and `mise run lints`.

## Open Questions

None.

## Verification Notes

- Reviewed the complete cumulative diff, manifests/lockfile, current consumers,
  generated schema pairs, current `mise` wiring, and the R5 implementation
  evidence rather than only the final remediation commits.
- Independently confirmed `git diff --check` is clean, the original Vala audit
  migration matches the base byte-for-byte, and the current `wyrd-server --lib`
  inventory has no `http::openapi` test.
- R5 evidence records green focused tests for all nine remediation roots and
  green broader lanes: formatting, Clippy, boundary checks, shared/principal/
  platform/identity/CLI/MCP/SQL/Bifrost/storage journeys and integrations,
  codegen, examples, docs, and strict rustdoc. Two recorded timing flakes passed
  unchanged reruns.
- This review did not rerun the full Cargo/Postgres suite. The two findings are
  source/authority contradictions not disproved by those green results.

## Overall result

**FAIL** — `RS-R6-1` and `RS-R6-2` are material, bounded repository-rule
violations. The immutable candidate remained
`2c0408b683f7a548cec6dd08b35698d761d33b31` throughout the review.
