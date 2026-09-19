# Repository standards review — admin principals whole branch, round 2

## Result

`FAIL`

The immutable subject remained
`c5c20754a167e8f4d74a555a720bd51df6179a6f..5293546f33b3a5fd9de529098e23ea70d472c412`
through this review (100 commits, 263 changed files). The candidate closes the
prior alternate-audit-table, public-error disclosure, verification-task,
source-comment, and rustdoc findings. Four material repository-rule violations
remain: one required authority is still contradictory, raw SQLx transactions
still cross library boundaries, prohibited commit metadata remains, and two
unrelated change packets entered the candidate.

## Authority coverage

| Changed surface | Applicable authority inspected | Result |
|---|---|---|
| Approved change packet, eight implementation tasks, prior review and remediation records | `AGENTS.md` §§14-16; `architecture/references/languages/spec-driven-development.md`; `.agents/skills/wyrd-task-review/SKILL.md` | **FAIL** — the cumulative candidate contains unrelated verified-change-contract work (`RS-R2-4`), and committed evidence asserts closure/waiver for still-live rules (`RS-R2-1`, `RS-R2-3`). |
| Principal/auth contracts and generated schemas/OpenAPI | `AGENTS.md` §§2, 8-9, 12; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/languages/errors.md`; `architecture/references/languages/agent-harness.md` | PASS for ownership, typed contracts, stable errors, and generated-artifact shape based on source inspection and recorded focused evidence. No generated file was treated as authority. |
| Shared runtime, auth issue/verify/OIDC, and `wyrd-client` | `AGENTS.md` §§3-6, 9, 16; `architecture/wyrd-security-posture.md`; `architecture/references/languages/rust-core.md`; `architecture/references/architecture/patterns.md` | PASS for crate placement, struct-owned workflows, bounded async IO, credential secrecy, and the screened/pinned OIDC client path. |
| `wyrd-auth`, server boot, platform/principal handlers, HTTP, MCP | `AGENTS.md` §§2-6, 9, 16; `architecture/agent-rules.md` audit/SSRF/error rules; security posture; error and agent-harness references | PASS for the canonical `vala.audit_staging` path, fail-closed authz, single public error mapper, request instrumentation, and MCP scope enforcement. The architecture authority describing these paths still fails (`RS-R2-1`). |
| `wyrd-sql`, Vala audit migration/query changes, tenant lifecycle migrations | `architecture/agent-rules.md` SQL tenancy and audit rules; `AGENTS.md` §§2-6, 9, 15-16; security posture | **FAIL** — candidate-added platform query APIs accept caller-handed `sqlx::Transaction` instead of the two sanctioned connection capabilities (`RS-R2-2`). Canonical audit staging itself passes. |
| CLI and Rust SDK projections | `AGENTS.md` §§2-4, 9, 11-12; client ownership rules; agent-harness and errors references | PASS — administrative HTTP use is centralized in `wyrd-client`; CLI commands consume that client instead of creating a second transport implementation. |
| Journey, integration, unit tests and `mise.toml` | `AGENTS.md` §§11-12; `architecture/agent-rules.md`; `architecture/references/languages/testing-workflows.md` | PASS structurally — Postgres-owning tasks use the repository wrapper; nextest selectors are anchored or select a complete explicit target; user-facing HTTP/CLI/MCP paths have real-server journeys. Runtime results remain the orchestrator's verification responsibility. |
| Architecture/security/operator documentation | `AGENTS.md` §§1-2, 12, 14; design, doctrine, security posture, and approved spec §Required architecture amendments | **FAIL** — security posture still declares the superseded single-plane identity and epoch model (`RS-R2-1`). The stale fixture comment and private intra-doc link from round 1 are corrected. |
| Commit authorship and trailers | `AGENTS.md` §13 | **FAIL** — complete-range metadata still violates the required identity and AI-trailer prohibition (`RS-R2-3`). |
| Concurrent verified-change-contract and tenant-OIDC edits | Approved admin-principals spec non-goals; `AGENTS.md` §14; task-review PASS rule excluding unrelated changes | **FAIL** — unrelated specification/architecture/task work is bundled into this immutable candidate (`RS-R2-4`). |

## Applicable rule results

| Rule | Result | Evidence |
|---|---|---|
| Only `TenantConn` and `OperatorPool` cross SQL library signatures; callers never hand raw SQLx transactions to domain queries | **FAIL** | `operator_pool.rs:28-40` returns a raw transaction; five new platform query functions accept it at `provisioning.rs:27-32`, `principals.rs:56-61`, `credentials.rs:106-113`, `principal_grants.rs:25-29`, and `identity.rs:244-249`. |
| Tenant-scoped operations use `TenantConn`; cross-tenant work uses `OperatorPool`; no manual tenant filter replaces RLS | PASS | Tenant principal/credential queries take `&mut TenantConn<'_>`; platform reads take `&OperatorPool`; no candidate tenant path widens through an operator pool. |
| Every permission evaluation stages one canonical audit row transactionally and fails closed; no alternate audit store | PASS | `wyrd-auth/src/platform_authz.rs:1-15,105-143` appends through `vala_sql::queries::audit_staging::append_audit`; the prior `platform.audit_authz` migration/module are absent; `20260910000026_audit_tenant_admin_principal_kind.sql` only widens canonical staging. |
| Public internal failures do not disclose database/provider/crypto strings | PASS | `wyrd-server/src/http/error.rs:63-77` traces the cause and emits empty details; platform/principal route conversions reuse it. The remaining seed serializer source at `http/error.rs:297-306` predates this candidate and is outside the changed admin paths. |
| User/tenant URLs are resolved, screened, and pinned at effective address | PASS | `wyrd-auth-oidc/src/screening.rs` owns resolution/pinning; candidate OIDC discovery, token, and JWKS paths use `ScreenedHttp`, including each JWKS refresh. |
| Governing architecture must describe lasting identity/security behavior delivered by the approved spec | **FAIL** | `architecture/wyrd-security-posture.md:42-46,63-65,74-89` still excludes both administrative kinds, requires tenant identity on every principal/token, and applies tenant epoch semantics to the platform plane. |
| Stateful workflows have cohesive concrete owners; pure helpers remain free | PASS | `PlatformAuthorization`, `PlatformCredentials`, `PlatformSessions`, `TenantProvisioning`, `TenantRecovery`, and client `Platform`/`Principals` handles own dependencies and orchestration. |
| Async is limited to IO/composition; CPU-heavy hashing leaves the executor | PASS | Database/network handlers await IO; Argon2 hashing uses `spawn_blocking` in the changed initialization and credential paths. |
| Write handlers are instrumented with scrubbed arguments and return typed public errors | PASS | Platform/principal write handlers use `#[tracing::instrument(... skip(...))]`; OpenAPI error rows name `WyrdProblem`; response conversion stays in the single server mapper. |
| Generated OpenAPI/schema goldens are source-derived and current | PASS based on recorded evidence | Round-1 remediation records `mise run codegen:check` green; source and checked-in artifacts agree in the inspected administrative contracts. This reviewer did not run codegen. |
| Changed capability tasks own setup and cannot silently select zero tests | PASS | `mise.toml:116-161` uses repository Postgres setup for DB/journey lanes and anchored nextest expressions or whole explicit targets. |
| Changed Rust documentation is accurate and resolvable | PASS | `service_accounts.rs:10-28,174-188` now states containment and its dependency on the existing uniqueness constraint; `wyrd-testing/src/server.rs:2669-2674` no longer claims equality; the private intra-doc link was removed. |
| Commit identity is `Thorrester <sjforrester32@gmail.com>` and no AI co-author trailer exists | **FAIL** | Complete-range log inspection finds 49 commits with a wrong author and/or committer and 84 commits containing `Co-Authored-By: Claude` or `Claude-Session:`. |
| One task review candidate excludes unrelated work | **FAIL** | Commits `63bc79127` and `5293546f3` add/approve a separate verified-change-contract spec, architecture, and eight-task plan; `5293546f3` also rewrites repository-wide Card doctrine. |

## Material findings

### `FIND-admin-principals-4` — `MISSING` — the normative security model still rejects the implementation

- **Violated authority:** `AGENTS.md` §§1-2 and the approved specification's
  required architecture amendments at
  `changes/active/admin-principals/spec.md:546-562`.
- **Location:** `architecture/wyrd-security-posture.md:42-46,63-65,74-89`,
  compared with `architecture/wyrd-design.md:141-157` and approved
  `REQ-003`, `REQ-004`, `REQ-041` through `REQ-046`, and `REQ-049`.
- **Evidence:** the security authority still says the closed kind set is only
  `User | Service | Agent | System`, every runtime identity has a `tenant_id`,
  Service/Agent always carry a Card, every token carries tenant and epoch, and
  credential revocation always advances the principal epoch. The candidate
  actually adds tenantless platform principals and sessions,
  `GlobalAdmin`/`TenantAdmin`, Card-free machine identities, and a platform
  plane that deliberately re-reads authority rather than caching an epoch.
  The committed remediation record claims this finding passed at
  `whole-branch-01/TASK-001-008-R1-close-whole-branch-findings.md:398`, but the
  cited authority still contains the superseded text.
- **Consequence:** the normative security architecture tells future changes to
  reject or overwrite the shipped two-plane model, and gives operators the
  wrong revocation/cache contract.
- **Testable correction:** amend the principal/API-key/token passages in the
  existing security posture so they accurately distinguish platform and tenant
  principal tenancy, Card binding, session claims, and revocation mechanics.
  Re-run the documentation/design checks and use a focused text assertion only
  if an existing design-sync gate already owns this invariant; do not add a new
  permanent grep check solely for this remediation.

### `FIND-admin-principals-2` — `VIOLATION` — raw SQLx transactions still cross the new platform query boundary

- **Violated authority:** `architecture/agent-rules.md` SQL boundary: exactly
  two connection abstractions are allowed in library fields/signatures,
  `&mut TenantConn<'_>` and `&OperatorPool`; a caller-handed
  `sqlx::Transaction<'_, Postgres>` is explicitly banned. The approved spec
  repeats that invariant at `spec.md:97-99`, and round-1 acceptance required
  every changed signature to expose only those two capabilities.
- **Location:** `crates/wyrd/wyrd-sql/src/operator_pool.rs:28-40`;
  `queries/platform/provisioning.rs:27-32`;
  `principals.rs:56-61`; `credentials.rs:106-113`;
  `principal_grants.rs:25-29`; `identity.rs:244-249`. Reachable callers include
  `wyrd-server/src/boot/init.rs:98-140` and
  `components/platform/identity.rs:373-399`.
- **Evidence:** `OperatorPool::begin` returns a raw SQLx transaction and the
  five exported query functions accept that raw transaction from server-tier
  callers. The existing boundary test at `wyrd-sql/src/lib.rs:342-363` scans
  only `src/queries/auth`, so it cannot detect this platform-plane violation.
- **Consequence:** server callers can bypass the repository's typed connection
  capabilities, and transaction/role/tenant ownership becomes an unenforced
  convention precisely on the BYPASSRLS plane.
- **Testable correction:** keep the same atomic workflows but route their query
  calls through the existing `TenantConn` or `OperatorPool` capability rather
  than exporting `Transaction`; initialization may open the existing
  operator-backed `TenantConn`, while authorized writes should pass the
  `TenantConn` already returned by `PlatformAuthorization`. Extend the existing
  source-boundary test to cover platform query modules so `PgPool`,
  `Transaction`, naked connections, and `.begin()` cannot re-enter exported
  platform query signatures. Do not create a third connection wrapper.

### `FIND-TASK-001-10` — `VIOLATION` — prohibited identity and AI metadata remain in the delivered branch

- **Violated authority:** `AGENTS.md:547-555` requires the configured
  `Thorrester <sjforrester32@gmail.com>` identity and forbids AI co-author
  trailers.
- **Location:** commit metadata throughout
  `c5c20754a167e8f4d74a555a720bd51df6179a6f..5293546f33b3a5fd9de529098e23ea70d472c412`.
- **Evidence:** 49 of the 100 commits have a nonconforming author and/or
  committer (predominantly `Claude <noreply@anthropic.com>`), and 84 contain a
  `Co-Authored-By: Claude` or `Claude-Session:` trailer. Commits `13bf9a369`
  and `528cd576f` merely edited a remediation record to call this a waiver;
  they did not alter the branch history or the governing rule. No explicit
  current user instruction supplied to this independent reviewer overrides
  §13.
- **Consequence:** the delivered branch still carries attribution expressly
  prohibited by repository contribution policy; a Markdown assertion cannot
  make Git metadata compliant.
- **Testable correction:** the branch owner, not an implementation agent,
  performs the already-prepared history rewrite after the source tree is
  final. Closure requires a complete-range log showing the required author and
  committer on every commit and no prohibited trailer. If the human owner
  explicitly waives this rule for this branch, that instruction must be
  presented as review authority; self-authored candidate prose is not proof of
  a waiver.

### `FIND-admin-principals-R2-1` — `DRIFT` — unrelated verification-contract work entered the admin-principals candidate

- **Violated authority:** the immutable task-review PASS condition requires no
  unrelated change in the diff; `AGENTS.md` §14 and spec-driven development
  keep each approved change packet tied to its own implementation; approved
  admin-principals non-goals say Card kinds are unchanged at
  `changes/active/admin-principals/spec.md:142-143`.
- **Location:** commits `63bc791272b3b06d5fa24df65fa36ebb6c840284`
  and `5293546f33b3a5fd9de529098e23ea70d472c412`.
- **Evidence:** `63bc79127` approves revision 30 of
  `changes/active/verified-change-contract/spec.md` and changes five of that
  packet's design files. `5293546f3` changes `AGENTS.md`,
  `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and
  `architecture/wyrd-security-posture.md` from 16 Card kinds with Drift/Eval to
  15 kinds with Verifier, then adds eight tasks and more verified-change
  architecture. None implements or remediates admin principals.
- **Consequence:** this admin-principals candidate cannot be accepted,
  reverted, or reviewed independently; it also violates the approved change's
  explicit unchanged-Card-kind boundary.
- **Testable correction:** move the two unrelated commits onto the owning
  verified-change-contract branch (or otherwise rebase the admin-principals
  candidate so those paths are absent from its base-to-candidate range). The
  corrected cumulative diff must contain only admin-principals implementation,
  its required architecture amendments, and its review evidence. Do not copy
  or revert their content ad hoc inside an implementation remediation commit.

## Prior standards finding closure

| Prior finding | Round-2 disposition |
|---|---|
| `FIND-admin-principals-1` alternate audit store | **CLOSED** — alternate table/module removed; platform authz stages canonically. |
| `FIND-admin-principals-2` raw SQL boundaries | **OPEN** — pool fields were wrapped, but raw transactions still cross five exported platform query signatures. |
| `FIND-admin-principals-3` served internal strings | **CLOSED** for changed administrative paths. |
| `FIND-admin-principals-4` architecture amendments | **OPEN** — design/foundation documents changed, but normative security posture remains contradictory. |
| `FIND-admin-principals-5` unsafe/unrunnable tasks | **CLOSED** structurally. |
| `FIND-admin-principals-6` false comment/private rustdoc link | **CLOSED**. |
| `FIND-TASK-001-10` commit identity/trailers | **OPEN** — a candidate-authored waiver note is not metadata remediation or supplied authority. |

## Verification notes

- This role performed static inspection only, as assigned; it launched no
  Cargo or `mise` command and therefore did not overlap repository verification.
- `git diff --check` reports blank lines at EOF in one prior review report and
  four unrelated verified-change task files. This is recorded as verification
  debt, not promoted to a material admin-principals finding.
- Round-1 remediation evidence records green focused lanes for formatting,
  lints, codegen, docs, client-tier and unwrap checks, `wyrd-sql` rustdoc,
  platform/MCP/principal/CLI journeys. Those results do not prove compliance
  with the remaining source/authority/history/scope violations above.
- The supplied `auth_e2e::cache_ttl_path_also_flips_verdict` failure and Card
  same-name-across-spaces constraint are pre-existing/spec-owner matters, not
  candidate standards findings. The stale fixture comment and rustdoc warning
  are closed without widening the permanent rustdoc lane.
