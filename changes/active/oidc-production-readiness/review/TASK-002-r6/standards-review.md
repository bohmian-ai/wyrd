# Repository standards review — TASK-002-r6

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `0ca117a744ddcb7414b104c4382027970531b608`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation tasks: TASK-002-R1 through TASK-002-R5
- Reviewed range: the complete cumulative base-to-candidate diff: 93 files,
  9,537 insertions and 1,842 deletions.

`HEAD` matched the candidate before inspection. The repository has no
`.codegraph/` directory, so inspection used Git, `rg`, and direct source reads
as required by `AGENTS.md` §18. This report is the Wave 1 repository-standards
review only: it does not decide task acceptance or perform Wave 2 finding
validation. Lead-directed reuse and test commits recorded in the evidence
tables were treated as authorized while still being audited for repository
compliance.

## Authority coverage

| Changed surface / layer | Applicable authority read and applied | Coverage result |
|---|---|---|
| Active change packet, original task, five remediation rounds, and evidence | `AGENTS.md` §§12, 14–16; `architecture/agent-rules.md`; `architecture/references/README.md`; `languages/spec-driven-development.md`; `languages/implementation-execution.md` | Immutable identities, authority ordering, remediation traceability, and verification records are present. **PASS** |
| Rust auth contracts, schema generator, JSON schemas, and schema goldens | `AGENTS.md` §§2–6, 8–9, 11–12, 16; `wyrd-design.md`; `wyrd-doctrine.mdx`; `architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md`; `languages/testing-workflows.md` | Typed contracts remain in IO-/async-/PyO3-free `wyrd-spec`; source, generator registration, schemas, and goldens align. **PASS** |
| Shared Rust client and CLI retirement | `AGENTS.md` §§2–6, 9, 11–12, 16; `wyrd-design.md` client and identity sections; `architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md` | The client projects server contracts; the obsolete direct authorization-code flow is removed rather than retained behind an alias. **PASS** |
| OIDC/JWKS verification and tenant/platform identity | `AGENTS.md` §§4–6, 9–10, 12, 16; `agent-rules.md`; `wyrd-security-posture.md`; `wyrd-design.md` runtime identity; `patterns.md`; `rust-core.md`; `errors.md` | Existing verifier and screened network owners enforce signature, issuer, audience, algorithm, key, and time/claim policy; human identity remains exact OIDC `sub`. **PASS** |
| Login, callback, connection, refresh, secrets, and canonical audit | `AGENTS.md` §§4–6, 9–12, 15–16; `agent-rules.md`; `wyrd-security-posture.md`; `patterns.md`; `rust-core.md`; `errors.md` | Stateful workflows remain on concrete owners, secrets remain redacted, and durable authorization/session effects use the canonical transaction/audit boundaries. **PASS** |
| HTTP routes, typed errors, runtime OpenAPI, and docs | `AGENTS.md` §§9, 11–12, 16; `wyrd-design.md`; `wyrd-doctrine.mdx`; `wyrd-security-posture.md`; `patterns.md`; `errors.md`; `testing-workflows.md` | Typed request/response contracts, one error projection, scrubbed instrumentation, served OpenAPI proof, and public documentation agree. **PASS** |
| Tenant SQL, RLS, migration, advisory locking, transaction ownership, and concurrency | `AGENTS.md` §§4–6, 9, 11–12, 15–16; `agent-rules.md` SQL/RLS rules; `wyrd-security-posture.md`; `architecture/patterns.md`; `rust-core.md` Postgres rules; `testing-workflows.md` | Tenant operations use `TenantConn`; the family lock is tenant-qualified and transaction-scoped; callees do not commit; lock/classification order and caller-owned commit are accurately documented. **PASS** |
| Tests, identity fixtures, Docker, and mise lanes | `AGENTS.md` §§11–12, 15–16; `agent-rules.md`; `testing-workflows.md`; `implementation-execution.md` | Unit, Postgres integration, OpenAPI, and real-server identity journeys remain in their proper lanes; exact commands and repository-managed setup are recorded. **PASS** |
| Python, TypeScript, PyO3, UI, MCP, and Vala/Bifrost analytical code | Reference router and applicable language/domain boundaries | No implementation in these layers changed. Contract generation found no Python/TypeScript projection drift. **N/A** |

## Applicable rule results

| Applicable repository rule | Source evidence | Result |
|---|---|---|
| Durable authentication and issuance remain server-owned (`AGENTS.md` §§2–3, 9; `patterns.md`) | `wyrd-auth/src/login.rs`, `callback.rs`, and `refresh.rs` own state transitions and issuance; server modules adapt HTTP; `wyrd-client` only projects contracts. | PASS |
| Public contracts are typed and `wyrd-spec` remains foundational (`AGENTS.md` §§2–3, 7–9) | Auth contracts remain under `wyrd-spec/src/auth`; the range adds no IO, SQL, Tokio, server framework, or PyO3 dependency to that crate. | PASS |
| Removed public behavior is deleted, not compatibility-aliased (`AGENTS.md` §§2, 9, 12; doctrine) | The direct authorization-code token variant, direct client/CLI login flow, and legacy route are removed across code, schema, tests, and docs. | PASS |
| Rust workflows use cohesive concrete owners and no speculative abstraction (`AGENTS.md` §5; `agent-rules.md`; `rust-core.md`) | `HumanConnections`, `AuthorizationCodeExchange`, `RefreshTokens`, `TenantTokenIssuer`, `ExternalVerifier`, and `WyrdPostgres` retain the relevant behavior; no new one-implementation trait, factory, dependency, feature, or compatibility layer was added. | PASS |
| Async is confined to IO and IO composition (`AGENTS.md` §6; `rust-core.md`) | New async paths await JWKS/network or Postgres operations; claim checks, hashing, key construction, and conversions remain synchronous. | PASS |
| Imports and signatures follow repository form; no unsanctioned lint escape (`agent-rules.md`) | Changed production imports are module-scoped and signatures use imported bare types. The diff adds no production `#[allow(clippy::...)]`. | PASS |
| New/materially modified Rust items have meaningful rustdoc, error contracts, and durable-ordering semantics (`AGENTS.md` §16; `agent-rules.md`; `rust-core.md`) | The cumulative R2/R3/R4 documentation covers changed owners/helpers. At `refresh_tokens.rs:122-146`, R5 now accurately documents lookup-before-family-lock/classification, test-only lifecycle observation, and `# Errors`; it agrees with the complete `RefreshTokens::execute` body and every caller. | PASS |
| Public failures retain stable Wyrd error projection and do not leak internals (`AGENTS.md` §§4, 9; `errors.md`) | Auth failures map through existing `WyrdError` variants and the single HTTP response mapper; provider/database details are logged or collapsed rather than exposed through a parallel payload. | PASS |
| OIDC federation fails closed on exact configured trust (`wyrd-security-posture.md`) | `ExternalVerifier` requires configured issuer/audience, asymmetric signature/key policy, `exp`, numeric non-future `iat`, and valid present `nbf`; tenant callback separately enforces nonce and authorized-party semantics. | PASS |
| Tenant and effective connection derive from trusted state, not request routing metadata (`AGENTS.md` §9; security posture) | Begin login treats the tenant key as routing context; callback resolves the opaque one-use state owner; refresh routes to RLS then authorizes by the stored token hash. | PASS |
| Tenant SQL uses `TenantConn`, relies on forced RLS, and leaves commit/rollback to callers (`agent-rules.md`; `patterns.md`; `rust-core.md`) | New login-state and refresh-family operations accept `&mut TenantConn<'_>` and do not commit or roll back. The narrow cross-tenant state-owner lookup remains on `WyrdPostgres`, not a propagated raw pool. | PASS |
| Native PostgreSQL coordination protects multi-replica refresh behavior (`AGENTS.md` §15; `rust-core.md`) | `pg_advisory_xact_lock` serializes by tenant plus immutable principal family through the caller's transaction; family precedes classification and connection-slot locking. No process-local mutex or lease was introduced. | PASS |
| Refresh replay containment and canonical evidence share the deciding transaction (`security posture`; audit rules) | `RefreshTokens::execute` looks up, family-locks, classifies, revokes on replay, and appends the canonical denial audit before the route-owned terminal commit. | PASS |
| Secret and outbound-provider handling preserve trust boundaries (`AGENTS.md` §§4, 10; security posture) | PKCE/client secrets use redacted secret types and scrubbed tracing; discovery, token, and JWKS calls retain the existing screened/pinned HTTP owner. | PASS |
| Audit uses one staging path and required decisions fail closed (`AGENTS.md` §§2, 9; `agent-rules.md`) | Login role sync, issuance, and refresh containment append through the existing auth audit helper to `vala.audit_staging`; the diff adds no alternate table, queue, publisher, or sink. | PASS |
| Generated artifacts have owning source/generator changes (`AGENTS.md` §§8, 11–12; `agent-rules.md`) | `wyrd-spec/src/auth/*` and `examples/gen_schemas.rs` accompany the schema and golden changes; recorded `codegen:check` is green. | PASS |
| Test placement and priority follow the three-tier model (`AGENTS.md` §11; `testing-workflows.md`) | Pure verifier cases are inline; Postgres cases are under `pg_tests` or `pg_*`; `identity_e2e.rs` drives the real server, IdP, and repository-managed Postgres. | PASS |
| User-visible success and refusal behavior has real journey proof (`AGENTS.md` §11) | The four required identity journeys plus callback-refusal, same-issuer isolation, connection cutoff, served OpenAPI, and the 27-test full identity lane are recorded green; seam tests supplement rather than replace them. | PASS |
| No check/test weakening, synthetic load, new dependency/feature, or source-tree build output (`AGENTS.md` §§4, 11–12; `agent-rules.md`) | The range adds no manifest dependency/feature, ignored required test, weakened assertion, production lint exemption, load generator, or build-script output. R5 is documentation-only. | PASS |
| R5 is the minimum repository-compliant correction (`AGENTS.md` §15; Ponytail rule) | `0ca117a74` changes only the stale `refresh_by_hash` rustdoc. It reuses the existing helper and accurately distinguishes its one production rotation caller from test observation; executable behavior and proof remain unchanged. | PASS |

## Material repository-rule findings

None. No source-local `STD-R6-*` finding is proposed.

## Prior-finding closure relevant to repository standards

| Finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-1` through `FIND-TASK-002-7` | Exact OIDC `sub`, `azp`, canonical role-sync audit, asymmetric algorithm policy, RLS-only login state, owner-held state lookup, and redacted PKCE remain in current source. | CLOSED |
| `FIND-TASK-002-8` and `FIND-TASK-002-9` | Changed Rust inventory retains substantive documentation and module-top imports. | CLOSED |
| `FIND-TASK-002-10` through `FIND-TASK-002-12` | Algorithm-helper docs match responsibility; ID tokens require binding/time claims; refresh operations serialize by tenant principal family with deterministic overlap proof. | CLOSED |
| `FIND-TASK-002-13` | `refresh_by_hash` no longer describes the retired consume-first order; its current docs match production rotation and test-only lifecycle observation. | CLOSED |

## Verification notes and limits

- Fresh read-only proof: immutable candidate identity, complete changed-file and
  commit inventory, full cumulative diff/caller inspection, current R5-only
  diff inspection, authority/rule scans, and cumulative `git diff --check`.
  All passed.
- Recorded implementation evidence is green for exact verifier and deterministic
  refresh-overlap tests, the four required identity journeys, the full 27-test
  identity lane, principals unit/integration, SQL and migration coverage,
  tenant/client boundary checks, codegen/docs, format, lints, and diff hygiene.
- This bounded review did not rerun costly Cargo, Docker, Postgres, Keycloak,
  Dex, docs, or code-generation lanes. R5 changed only a Rust doc comment and
  records fresh green `mise run fmt`, `mise run lints`, and `git diff --check`;
  the prior cumulative behavioral evidence remains applicable.
- No Python, TypeScript, PyO3, UI, MCP, Bifrost, or Vala analytical source
  changed, so no runtime-specific lane for those layers was required.

## Overall result

**PASS**

The cumulative candidate conforms to the applicable ownership, Rust structure
and documentation, stable-error, federation, secret-handling, tenant/RLS,
transaction/audit, concurrency, generated-contract, testing, tooling, and
documentation rules. The R5 correction closes the only cited stale source
contract without executable or scope drift.
