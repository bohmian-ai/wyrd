# TASK-002 R7 Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `e1ce3c847c14d306c69a704cd15ce6686db9e62e`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation tasks: TASK-002-R1 through TASK-002-R6
- Reviewed range: complete cumulative base-to-candidate diff, 101 files,
  10,841 insertions and 1,850 deletions

`HEAD` matched the candidate at the beginning and end of inspection. The
repository has no `.codegraph/` directory, so inspection used Git, `rg`, and
direct source reads as permitted by `AGENTS.md` §18. This is the Wave 1
repository-standards review only; it does not decide task acceptance. The
lead-directed reuse and test commits identified in the evidence tables were
treated as authorized rather than scope drift and were still audited for
repository compliance.

## Authority coverage

| Changed surface / layer | Applicable authority read and applied | Coverage result |
|---|---|---|
| Active change packet, original task, R1–R6 remediation tasks, review records, and implementation evidence | `AGENTS.md` §§12–16, 18; `architecture/agent-rules.md`; `architecture/references/README.md`; `languages/spec-driven-development.md`; `languages/implementation-execution.md` | Immutable identities, remediation lineage, focused commands, completion evidence, and authorized follow-up test evidence are present. **PASS** |
| Rust auth contracts, schema generator, JSON schemas, and goldens | `AGENTS.md` §§2–6, 8–9, 11–12, 15–16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md`; `languages/testing-workflows.md` | Typed contracts remain in IO-/async-/PyO3-free `wyrd-spec`; source and generated projections remain paired. **PASS** |
| Shared Rust client and CLI retirement | `AGENTS.md` §§2–6, 9, 11–12, 16; `wyrd-design.md` client and runtime-identity sections; `patterns.md`; `rust-core.md`; `errors.md` | The client projects server contracts; the obsolete authorization-code client/CLI path is deleted without a compatibility alias. **PASS** |
| OIDC/JWKS verification and tenant/platform identity | `AGENTS.md` §§4–6, 9–10, 12, 16; `agent-rules.md`; `wyrd-security-posture.md`; `wyrd-design.md` runtime identity; `patterns.md`; `rust-core.md`; `errors.md` | Verification, exact subject identity, secrets, provider screening, and authority-plane separation remain within their existing owners. **PASS** |
| Login, callback, connection lifecycle, refresh rotation, principal revocation, and canonical audit | `AGENTS.md` §§4–6, 9–12, 15–16; `agent-rules.md`; `wyrd-security-posture.md`; `patterns.md`; `rust-core.md`; `errors.md` | Stateful workflows remain on concrete owners; R6 reuses the existing tenant-qualified family lock and preserves route-owned commit and audit boundaries. **PASS** |
| HTTP routes, typed errors, runtime OpenAPI, and docs | `AGENTS.md` §§9, 11–12, 16; `wyrd-design.md`; `wyrd-doctrine.mdx`; `wyrd-security-posture.md`; `patterns.md`; `errors.md`; `testing-workflows.md` | Typed public contracts, one error projection, served OpenAPI proof, and public documentation remain aligned. **PASS** |
| Tenant SQL, RLS, migration, advisory locking, transaction ownership, and concurrency | `AGENTS.md` §§4–6, 9, 11–12, 15–16; `agent-rules.md`; `wyrd-security-posture.md`; `patterns.md`; `rust-core.md`; `testing-workflows.md` | Tenant work uses `TenantConn`; callees do not commit; family-before-connection ordering and transaction-scoped native PostgreSQL locks cover rotation, administrative User revocation, and connection cutoff. **PASS** |
| Inline Rust tests, external journeys, identity fixtures, Docker, and mise lanes | `AGENTS.md` §§11–12, 15–16; `agent-rules.md`; `testing-workflows.md`; `implementation-execution.md` | Pure and Postgres tests remain in their owning modules/`pg_tests`; real-server identity journeys remain the primary user-facing proof. R6 adds deterministic lock-state integration proofs without a new harness. **PASS** |
| Python, TypeScript, PyO3, UI, MCP, and Vala/Bifrost analytical code | Reference router and applicable language/domain boundaries | No implementation in these layers changed; recorded contract generation produced no Python or TypeScript projection diff. **N/A** |

## Applicable rule results

| Applicable repository rule | Source evidence | Result |
|---|---|---|
| Durable authentication, refresh, and revocation behavior remains server-owned (`AGENTS.md` §§2–3, 9; `patterns.md`) | `wyrd-auth/src/login.rs`, `callback.rs`, `refresh.rs`, `revoke.rs`, and `connections.rs` own the workflows; HTTP modules remain adapters and `wyrd-client` only projects contracts. | PASS |
| Public contracts are typed and `wyrd-spec` remains foundational (`AGENTS.md` §§2–3, 7–9) | Auth contracts remain under `wyrd-spec/src/auth`; the range adds no IO, SQL, Tokio, server framework, or PyO3 dependency to that crate. | PASS |
| Removed public behavior is deleted rather than compatibility-aliased (`AGENTS.md` §§2, 9, 12; doctrine) | The direct authorization-code token variant, old client/CLI login flow, and old route are removed across source, schemas, tests, and docs. | PASS |
| Rust workflows use cohesive concrete owners and no speculative abstraction (`AGENTS.md` §5; `agent-rules.md`; `rust-core.md`) | `HumanConnections`, `AuthorizationCodeExchange`, `RefreshTokens`, `TenantTokenIssuer`, and existing SQL capabilities remain the owners. R6 adds one call to `lock_refresh_family`; it adds no type, trait, lock service, feature, dependency, or compatibility layer. | PASS |
| Async is confined to IO and intentional IO composition (`AGENTS.md` §6; `rust-core.md`) | Production async paths await Postgres or provider IO. R6's async helpers observe real Postgres lock state; no pure transformation was made async. | PASS |
| Imports and signatures follow repository form; no unsanctioned lint escape (`agent-rules.md`) | Changed imports are module-scoped, signatures use imported bare types, and the cumulative diff adds no production `#[allow(clippy::...)]`. | PASS |
| New/materially modified Rust items have substantive rustdoc, fallible contracts, and durable-ordering semantics (`AGENTS.md` §16; `agent-rules.md`; `rust-core.md`) | `revoke_principal_in_conn` documents the family lock, ordering, caller-owned release, and errors (`revoke.rs:16–41`). New R6 test helpers and tests document intent, synchronization, terminal state, and panics (`revoke.rs:496–572`; `refresh.rs:1094–1150`). Prior R2–R5 corrections remain present. | PASS |
| Public failures use the stable Wyrd error boundary and redact internals (`AGENTS.md` §§4, 9; `errors.md`) | R6 maps lock/store failure through the existing logged `WyrdError::Internal` path; it adds no new code, payload, or raw SQL/provider error exposure. | PASS |
| Federation and secret handling continue to fail closed (`wyrd-security-posture.md`; `agent-rules.md`) | Existing screened/pinned provider IO, exact issuer/audience/algorithm/claim checks, asymmetric verification, and redacted secret types are unchanged by R6. | PASS |
| Tenant SQL uses `TenantConn`, relies on RLS, and leaves commit/rollback to callers (`agent-rules.md`; `patterns.md`; `rust-core.md`) | `revoke_principal_in_conn` receives `&mut TenantConn<'_>` and acquires the family lock without committing (`revoke.rs:42–67`). The served route retains transaction ownership. Test-only direct queries run through fixture tenant transactions or read lock metadata through the fixture pool. | PASS |
| Native PostgreSQL coordination protects multi-replica mutation ordering (`AGENTS.md` §15; `rust-core.md`) | The existing `pg_advisory_xact_lock` owner keys by tenant, principal kind, and principal id (`refresh_tokens.rs:96–120`). Administrative User revocation takes it after existence is established and before suspension/family mutation (`revoke.rs:50–66`), preserving family-before-connection ordering. | PASS |
| Canonical audit and transaction coupling remain unchanged (`AGENTS.md` §§2, 9; `agent-rules.md`; security posture) | R6 changes neither the audit writer nor route transaction. User suspension and family retirement remain in the deciding transaction; connection deactivation continues through `HumanConnections::begin_locked`. | PASS |
| Generated artifacts retain owning source/generator changes (`AGENTS.md` §§8, 11–12; `agent-rules.md`) | Cumulative schema and golden changes accompany auth source and generator registration; recorded `codegen:check` is green. R6 changes no generated artifact. | PASS |
| Test placement and tiering follow repository policy (`AGENTS.md` §11; `agent-rules.md`; `testing-workflows.md`) | R6 Postgres concurrency tests are inline under `pg_tests` in `wyrd-auth`; the existing four real-server identity journeys remain the capability-level proof. | PASS |
| Concurrency proofs use observed lock state rather than timing as the assertion (`implementation-execution.md`; test integrity rules) | `wait_for_advisory_lock_wait` observes the exact revoking backend's ungranted advisory lock (`revoke.rs:530–556`); `wait_for_connection_slot_waiter` derives the tenant slot key and observes an ungranted matching lock (`refresh.rs:1094–1132`). The 5 ms delay only bounds polling load; the lock predicate is the synchronization condition. | PASS |
| No gate weakening, synthetic host load, new dependency/feature, or source-tree build output (`AGENTS.md` §§4, 11–12; `agent-rules.md`) | The range adds no manifest dependency/feature, ignored required test, weakened assertion, production lint allowance, load generator, or build-script output. The R6 helper extraction responds to Clippy without suppressing it. | PASS |
| R6 follows the minimum root-cause correction (`AGENTS.md` §15; Ponytail ladder) | One omitted production caller now reuses the existing shared family lock. The focused overlap test and authorized connection-lifecycle proof reuse existing owners and fixtures; no alternate synchronization mechanism was added. | PASS |

## Material repository-rule findings

None. No source-local `STD-R7-*` finding is proposed.

## Prior standards closure

| Finding set | Current closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-1` through `FIND-TASK-002-13` | Exact OIDC subject identity, `azp`, canonical role-sync audit, asymmetric algorithm policy, RLS-only state, owner-held state lookup, secret redaction, Rust documentation/import corrections, mandatory ID-token claims, refresh-family serialization, and corrected lookup rustdoc remain in the cumulative candidate. | CLOSED |
| `FIND-TASK-002-14` | Administrative User revocation now participates in the existing tenant-qualified refresh-family lock before suspension and family mutation; the focused Postgres overlap proof observes the lock wait and terminal successor retirement. | CLOSED |

## Verification notes and limits

- Fresh read-only proof covered candidate identity, complete changed-file and
  commit inventory, the cumulative diff, every R6 production/test change and
  relevant caller, authority/rule scans, and cumulative `git diff --check`.
  These checks passed.
- Recorded candidate evidence is green for the focused User-revocation overlap
  test, connection-deactivation overlap test, principals unit/integration,
  SQL, all 27 identity journeys, tenant isolation, format, lints, and diff
  hygiene. Earlier cumulative evidence also records codegen/docs, served
  OpenAPI, and client-tier checks green.
- This bounded review did not rerun Cargo, Docker, Postgres, Keycloak/Dex,
  migration, code-generation, docs, or broad workspace lanes. It independently
  inspected their current source and recorded evidence; runtime results remain
  a verification limit rather than a missing repository-standard artifact.
- No Python, TypeScript, PyO3, UI, MCP, Bifrost, or Vala analytical source
  changed, so no runtime-specific lane for those layers was applicable.

## Overall result

**PASS**

The cumulative candidate conforms to the applicable ownership, Rust structure
and documentation, stable-error, federation, secret-handling, tenant/RLS,
transaction/audit, concurrency, generated-contract, testing, tooling, and
documentation rules. R6 closes the omitted family-lock caller by reusing the
existing SQL capability and adds focused, repository-native concurrency proof
without introducing a new abstraction or weakening any gate.
