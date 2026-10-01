# Repository standards review — TASK-002-r5

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `b57d43d501c136591125b98fe78352e657b093b6`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Reviewed range: the complete cumulative base-to-candidate diff: 86 files,
  8,646 insertions and 1,839 deletions.

`HEAD` matched the candidate before inspection and immediately before this
report was written. The worktree was clean before the review directory was
created. The repository has no `.codegraph/` directory, so source inspection
used Git, `rg`, and direct file reads as required by `AGENTS.md` §18.

This is the Wave 1 repository-standards review only. It does not judge task
acceptance and does not perform Wave 2 Ponytail consolidation. Lead-directed
reuse and test commits recorded in the task evidence are treated as authorized
and were still checked for repository-rule compliance.

## Authority coverage

| Changed surface / language / layer | Complete applicable authority read and applied | Coverage result |
|---|---|---|
| Approved spec, task, four prior review/remediation rounds, and implementation evidence | `AGENTS.md` §§12, 14–16; `architecture/agent-rules.md`; `architecture/references/README.md`; `languages/spec-driven-development.md`; `languages/implementation-execution.md` | Immutable identities, authority hierarchy, prior findings, remediation boundaries, and recorded proof are present and internally traceable. **PASS** |
| Rust auth contracts and generated JSON schemas | `AGENTS.md` §§2–6, 8–9, 11–12, 16; `wyrd-design.md`; `wyrd-doctrine.mdx`; `doctrine/architecture-constraints.md`; `architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md`; `languages/testing-workflows.md` | Typed contracts remain in IO-/async-/PyO3-free `wyrd-spec`; source definitions, generator registration, generated schemas, and schema goldens remain aligned. **PASS** |
| Shared Rust client and CLI retirement | `AGENTS.md` §§2–6, 9, 11–12, 16; `wyrd-design.md` client/runtime-auth sections; `wyrd-doctrine.mdx`; `architecture-constraints.md`; `patterns.md`; `rust-core.md`; `errors.md` | `wyrd-client` projects the server contract and the obsolete direct authorization-code flow is deleted rather than preserved as a compatibility surface. **PASS** |
| Shared external token verification, JWKS, tenant and platform OIDC | `AGENTS.md` §§4–6, 9–10, 12, 16; `agent-rules.md`; `wyrd-security-posture.md` §§Security principles, Delegation and federation, Signing keys; `wyrd-design.md` runtime identity; `patterns.md`; `rust-core.md`; `errors.md` | One existing `ExternalVerifier` owns signature/JWKS/issuer/audience/time verification; tenant callback and platform login share the ID-token entry while workload assertions remain on the generic entry. **PASS** |
| Tenant login/callback/connection/refresh services and canonical audit | `AGENTS.md` §§4–6, 9–12, 15–16; `agent-rules.md`; `wyrd-security-posture.md` §§Credential lifecycle, Tenant isolation, Audit integrity, Secrets; `patterns.md`; `rust-core.md`; `errors.md` | Stateful workflows remain on cohesive concrete owners, secrets are redacted, and successful issuance/replay containment retain the canonical same-transaction audit path. **PASS** |
| HTTP handlers, route registration, stable error projection, and runtime OpenAPI | `AGENTS.md` §§9, 11–12, 16; `wyrd-design.md`; `wyrd-security-posture.md`; `patterns.md`; `errors.md`; `testing-workflows.md` | Typed request/response contracts, one error mapper, scrubbed instrumentation, fixed callback response, and served-document OpenAPI proof remain aligned. **PASS** |
| Tenant SQL, RLS, migration, advisory locking, transaction ownership, and concurrency | `AGENTS.md` §§4–6, 9, 11–12, 15–16; `agent-rules.md` SQL/RLS/transaction rules; `wyrd-security-posture.md` §§Access and refresh tokens, Tenant isolation, Audit; `architecture-constraints.md`; `patterns.md`; `rust-core.md` Postgres section; `testing-workflows.md` | Tenant work uses `TenantConn`; the new transaction-scoped family lock is tenant-qualified from RLS state, precedes classification and the connection lock, and remains held through the caller-owned terminal transaction. **PASS** |
| Migration, identity-provider fixtures, Docker, mise lanes, and test placement | `AGENTS.md` §§11–12, 15–16; `agent-rules.md`; `testing-workflows.md`; `implementation-execution.md`; `wyrd-security-posture.md` | Postgres work stays in `pg_tests`/`pg_*`; real-server identity tests remain gated; repository-managed setup and exact focused commands are recorded; no gate was weakened. **PASS** |
| Documentation and surface alignment | `AGENTS.md` §§2–3, 9, 11–12; `wyrd-design.md`; `wyrd-doctrine.mdx`; `wyrd-security-posture.md`; `architecture-constraints.md`; `patterns.md` | Docs match body-routed login, fixed callback completion, refresh provenance/cutoff, and the retired CLI/grant surface. **PASS** |
| Python, TypeScript, PyO3, UI, MCP, Vala/Bifrost analytical layers | Router-selected language/domain authorities | No implementation in these layers changed. Shared contract/schema checks cover their projected wire input where applicable; no layer-specific implementation rule was activated. **N/A** |

## Applicable rule results

| Applicable rule / authority | Exact source evidence | Result |
|---|---|---|
| Server owns durable authentication, identity, tenancy, issuance, and audit (`AGENTS.md` §§2–3, 9; `wyrd-design.md`; `patterns.md`) | `wyrd-auth/src/login.rs` owns begin state; `wyrd-auth/src/callback.rs:201-272` owns verified callback completion; `wyrd-auth/src/refresh.rs:84-229` owns rotation/containment; server files remain adapters and clients remain projections. | PASS |
| Public contracts are typed and language-agnostic; `wyrd-spec` remains foundational and IO-/async-/PyO3-free (`AGENTS.md` §§2–3, 7–9; `architecture-constraints.md`) | Contract changes remain under `wyrd-spec/src/auth/*`; the cumulative range adds no runtime, SQL, network, Tokio, or PyO3 dependency to `wyrd-spec`. | PASS |
| Surface contracts agree; retired behavior is removed, not aliased (`AGENTS.md` §§2, 9, 12; `wyrd-doctrine.mdx`) | `wyrd-client/src/auth.rs`, server routes, generated schemas, docs, and deletion of `wyrd-cli/src/auth/login.rs` agree on begin/callback/completion; no legacy grant or compatibility route remains. | PASS |
| Stateful workflows have cohesive concrete owners; no speculative trait/factory/helper is introduced (`AGENTS.md` §5 and §15; `agent-rules.md`; `rust-core.md`) | Existing `ExternalVerifier`, `HumanConnections`, `AuthorizationCodeExchange`, `RefreshTokens`, `TenantTokenIssuer`, and `WyrdPostgres` own their dependencies. R4 adds one method and one SQL lock operation to existing owners, with no new type, trait, dependency, configuration knob, or persistence model. | PASS |
| Async is used only for IO or IO composition (`AGENTS.md` §6; `agent-rules.md`; `rust-core.md`) | New `verify_id_token_against` awaits JWKS-backed verification; `lock_refresh_family` awaits Postgres; pure `iat` comparison and lock-key construction remain synchronous/native. | PASS |
| Types are imported at module scope and signatures use bare names; no new production lint escape (`agent-rules.md`) | R4 signatures use imported `IssuerVerification`, `ExternalClaims`, `TenantConn`, and `Uuid`; the R4 diff adds no production function-scoped `use` or `#[allow(clippy::...)]`. | PASS |
| New/materially changed Rust items have substantive rustdoc, `# Errors`, and relevant transaction/lock semantics (`AGENTS.md` §16; `agent-rules.md`; `rust-core.md`) | `ExternalVerifier::verify_external_against` and `verify_id_token_against` at `wyrd-auth-verify/src/lib.rs:484-602`, `LOCK_REFRESH_FAMILY_SQL` and `lock_refresh_family` at `wyrd-sql/.../refresh_tokens.rs:27-30,96-120`, and `RefreshTokens::execute` at `wyrd-auth/src/refresh.rs:85-229` document intent, errors, ordering, ownership, and commit behavior. Added tests carry intent/panic documentation where applicable. | PASS |
| Public failures retain stable Wyrd error projection; internal/provider details are redacted (`AGENTS.md` §§4, 9; `errors.md`) | Verifier additions reuse `AuthError`, callback maps through `auth_error_to_wyrd`, platform login renders `NotAccepted`, and server HTTP still uses the existing `WyrdError` response mapper. No parallel code/status/problem serializer is added. | PASS |
| Federation requires configured issuer/audience/algorithm/claims and fails closed (`wyrd-security-posture.md` §§Delegation and federation, Signing keys) | `wyrd-auth-verify/src/lib.rs:540-556` requires `exp`/`iss`/`aud`, pins configured issuer/audience, validates present `nbf`, and retains asymmetric-key/JWKS validation; `:585-601` requires numeric non-future `iat`. Tenant and platform callers use it at `callback.rs:212-217` and `platform_login.rs:276-283`. | PASS |
| Tenant identity comes from trusted state/credentials, not headers or request-chosen context (`AGENTS.md` §§2, 9; `wyrd-security-posture.md`) | Callback resolves opaque one-use state to its owning tenant and opens `TenantConn`; refresh routing is re-authorized by the RLS hash lookup. Header-hostility and cross-tenant journey coverage remain recorded. | PASS |
| Tenant SQL uses `TenantConn`, forced RLS, and caller-owned transactions; no raw pool is propagated (`agent-rules.md`; `architecture-constraints.md`; `rust-core.md`) | New `lock_refresh_family(conn: &mut TenantConn<'_>, ...)` at `refresh_tokens.rs:109-120` neither commits nor rolls back. The lock key reads `wyrd.current_tenant()` at `:29-30`; service code never receives `PgPool`. The prior narrow cross-tenant state lookup remains an inherent `WyrdPostgres` capability. | PASS |
| PostgreSQL owns coordination timing and native database coordination is preferred (`AGENTS.md` §15) | `pg_advisory_xact_lock` provides transaction-lifetime serialization; existing statement/database timestamps remain the refresh eligibility and expiry clock. No process mutex, lease, retry protocol, or isolation-level change is introduced. | PASS |
| Refresh replay revokes the family and records canonical evidence (`wyrd-security-posture.md` §Access and refresh tokens; `AGENTS.md` audit rules) | `RefreshTokens::execute` resolves the stored family, locks it at `refresh.rs:121-128`, revokes active rows at `:128-140`, appends canonical audit at `:142-164`, and relies on route-owned commit for both state and evidence. | PASS |
| Lock order and multi-replica behavior are explicit (`AGENTS.md` §15; `rust-core.md`) | `lock_refresh_family` documents family-before-connection ordering at `refresh_tokens.rs:96-105`; `execute` acquires the family lock before `TenantTokenIssuer::issue_human_session`, whose connection-slot lock follows. PostgreSQL, rather than process memory, coordinates replicas. | PASS |
| Secrets stay redacted and provider IO remains screened (`AGENTS.md` §§4, 10; `wyrd-security-posture.md`) | Login state keeps PKCE in `SecretString`; bearer/provider secrets are skipped in tracing; provider discovery, token exchange, and JWKS retain the existing screened HTTP/JWKS owners. R4 logs only non-secret principal/row metadata. | PASS |
| Authorization/audit writes use the single canonical path and fail closed where required (`AGENTS.md` §§2, 9; `agent-rules.md`; `patterns.md`) | Callback role sync/issuance and refresh containment append through the existing auth audit helper into `vala.audit_staging` within the deciding tenant transaction. No second audit table, queue, log sink, or best-effort substitution appears. | PASS |
| Generated files have owning source/generator changes and are checked rather than hand-maintained (`AGENTS.md` §§8, 11–12; `agent-rules.md`) | Cumulative source changes in `wyrd-spec/src/auth/*` and `examples/gen_schemas.rs` accompany both generated schema trees. R4 changes no contract/generated artifact. Recorded `codegen:check` is green. | PASS |
| Tests follow the three-tier/runtime placement rules (`AGENTS.md` §11; `agent-rules.md`; `testing-workflows.md`) | Pure verifier cases are inline in `wyrd-auth-verify`; Postgres concurrency is inline under `refresh::pg_tests`; `identity_e2e.rs` drives the real server/IdP/Postgres and remains in the gated identity lane. | PASS |
| New user-visible negative behavior has real journey proof; seam tests do not substitute for it (`AGENTS.md` §11; `wyrd-design.md` doctrine 20) | `identity_e2e.rs:3153-3185` includes signed missing-`iat` refusal and persisted-state assertions; verifier unit coverage exercises the complete claim matrix; the four cumulative identity journeys remain recorded green. | PASS |
| Concurrency proof observes a real lock condition rather than relying on timing alone (`AGENTS.md` §11; `implementation-execution.md`) | `refresh.rs:983-1088` holds rotation open, observes the replay backend waiting through `pg_locks`, then commits and reads final state from fresh transactions; its 5 ms polling delay only yields between authoritative lock-state checks. | PASS |
| No test/gate weakening, synthetic host load, new feature, dependency, or source-tree build output (`AGENTS.md` §§4, 11–12; `agent-rules.md`) | Cumulative and R4 diffs add no Cargo manifest/feature/dependency change, production lint allowance, disabled required test, weakened assertion, load generator, or build-script output. R4 strengthens tests and changes no `mise.toml` lane. | PASS |
| Public docs, CLI docs, fixtures, Docker, and mise configuration stay scoped to the shipped behavior (`AGENTS.md` §§11–12, 15) | Cumulative docs and fixtures support the approved login journeys; recorded docs/codegen/identity checks are green. R4 adds only remediation/evidence records and auth tests/source. | PASS |

## Material repository-rule findings

None. No source-local `STD-R5-*` finding is proposed.

## Prior-finding closure

| Prior finding | Repository-standards closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-1` exact human `sub` identity | Identity resolution remains keyed by verified issuer/subject; email is profile/match data, not tenant identity selection. | CLOSED |
| `FIND-TASK-002-2` OIDC `azp` | Tenant callback still validates authorized party after shared verification; cumulative journey cases cover missing/wrong `azp`. | CLOSED |
| `FIND-TASK-002-3` provider role-change audit | Current role synchronization and token issuance retain the canonical transaction and injected-audit-failure rollback proof. | CLOSED |
| `FIND-TASK-002-4` advertised asymmetric algorithm | Discovery membership is checked before the shared verifier; symmetric algorithms remain rejected by the verifier. | CLOSED |
| `FIND-TASK-002-5` duplicate tenant predicates | Login-state transitions rely on forced RLS and no longer assert a second manual tenant boundary. | CLOSED |
| `FIND-TASK-002-6` raw-pool state lookup | State-to-tenant lookup remains a narrow inherent `WyrdPostgres` operation; domain APIs do not propagate `PgPool`. | CLOSED |
| `FIND-TASK-002-7` printable PKCE verifier | PKCE state remains `SecretString` with redaction proof. | CLOSED |
| `FIND-TASK-002-8` incomplete Rust documentation | The R2 inventory remains documented; R4 items add substantive contract/lock/error documentation. | CLOSED |
| `FIND-TASK-002-9` function-scoped imports | The cited changed SHA-256, Utoipa, and Wiremock imports remain module-scoped; R4 adds no production function-scoped import. | CLOSED |
| `FIND-TASK-002-10` false algorithm-helper rustdoc | Current helper documentation continues to describe advertised-set membership and assigns HMAC rejection to the shared verifier. | CLOSED |
| `FIND-TASK-002-11` missing OIDC binding/time claims | `wyrd-auth-verify/src/lib.rs:540-601` now requires issuer/audience/expiry, validates present `nbf`, and adds the shared numeric non-future `iat` contract used by both ID-token callers; focused and journey proof is recorded. | CLOSED |
| `FIND-TASK-002-12` refresh-family serialization | `refresh.rs:121-164` and `refresh_tokens.rs:27-30,96-120` serialize classification, rotation, containment, and audit per tenant principal family through the caller-owned commit; the deterministic Postgres overlap proof is recorded. | CLOSED |

## Verification notes and limits

- Fresh read-only checks in this review: candidate identity, clean initial
  worktree, complete changed-file inventory, commit inventory, source/caller
  inspection, authority/rule scans, and
  `git diff --check 3fc085acf5b3a710d5dc80892bd2e664b3db6174..b57d43d501c136591125b98fe78352e657b093b6`.
  All passed.
- I reviewed the complete cumulative diff and all prior `TASK-002-r1` through
  `TASK-002-r4` reports/remediation records. The R4 implementation record lists
  green exact tests for `oidc_id_token_requires_binding_and_time_claims`,
  `ancestor_replay_overlapping_rotation_revokes_successor`, and the filtered
  callback-refusal journey, followed by green `test:principals:unit`,
  `test:principals:integration`, `test:sql`, `check:tenant-isolation`, the full
  27-test identity journey lane, `fmt`, `lints`, and `git diff --check`.
- Earlier cumulative evidence records green code generation, documentation,
  OpenAPI, SQL/migration, client/pool/tenant boundary, gateway, and broad Wyrd
  lanes. I relied on that credible recorded evidence rather than rerunning
  costly Cargo, Docker, Postgres, Keycloak, Dex, or broad documentation lanes
  during this bounded review.
- The callback journey carries the missing-`iat` claim case end to end; the
  shared verifier unit test carries missing `iss`/`aud`, non-numeric/future
  `iat`, and future `nbf`. The implementation record explains that adding all
  cases to the same journey exceeded the route governor burst. This is an
  explicit verification limit, not a repository-rule failure: the stateful
  negative boundary is proved end to end and the remaining claim branches are
  proved at their single shared owner.
- No Python, TypeScript, PyO3, UI, MCP, Bifrost, or Vala analytical source was
  changed, so no runtime-specific lane for those layers is required by this
  standards review.

## Overall result

**PASS**

The cumulative candidate conforms to the applicable ownership, Rust structure
and documentation, stable-error, federation, secret-handling, tenant/RLS,
transaction/audit, concurrency, generated-contract, test-tier, tooling, and
documentation rules. All twelve prior findings are closed on current source,
and the R4 remediation introduces no material repository-standard violation.
