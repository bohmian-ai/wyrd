# Repository Standards Review: Admin Principals Whole Branch

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `072cf8b30c7135e8cf15f92da3e371a9c999703c`
- Range: complete `base..candidate` range, 204 changed files
- Candidate identity was rechecked during review and remained
  `072cf8b30c7135e8cf15f92da3e371a9c999703c`.

## Overall result

**FAIL.** Eight material repository-standard findings remain. The highest-risk
ones violate the repository's single canonical audit path, mandatory Postgres
capability boundary, public-error redaction rule, and architecture-authority
update requirement. Two candidate-added verification tasks are also
unrunnable, so the branch cannot supply its own required proof.

## Authority coverage

| Changed surface | Applicable authority | Result | Evidence |
|---|---|---:|---|
| Active spec, eight tasks, remediation packets, prior review records | `AGENTS.md` §14; `architecture/references/languages/spec-driven-development.md`; `implementation-execution.md` | **FAIL** | The approved spec itself requires architecture amendments at `spec.md:546-562`, but none of those authorities changed; see `RS-WB-4`. |
| Principal kinds and wire contracts in `wyrd-spec`, generated schemas, error catalog | `AGENTS.md` §§2-4, 9, 12, 15-16; `wyrd-design.md`; doctrine; errors reference | **FAIL** | Typed contracts and generated outputs are present, but the governing design/security documents still define the old closed principal model; see `RS-WB-4`. |
| Runtime principal/auth context and shared auth verify/issue/check crates | `AGENTS.md` §§3-7, 9, 16; Rust core; security posture | **PASS with authority drift elsewhere** | The runtime uses typed principal variants and keeps platform scope separate from tenant `Principal`; static unwrap, Clippy-allow, and tenant checks passed. Public authority still contradicts it (`RS-WB-4`). |
| Platform authentication, authorization, credential, login, and session owners in `wyrd-auth` | `AGENTS.md` §§4-6, 9, 16; agent rules for audit/SQL; security posture; errors reference | **FAIL** | `PlatformAuthorization::authorize` returns a raw `sqlx::Transaction`; platform decisions write a second audit table; public projections expose source errors. See `RS-WB-1` through `RS-WB-3`. |
| Tenant/platform migrations and `wyrd-sql` query modules | `AGENTS.md` §§3, 9, 15-16; agent rules for `TenantConn`, `OperatorPool`, RLS, audit; Rust core Postgres section | **FAIL** | New `platform.audit_authz` violates the one-write-path rule, and six query functions accept caller-provided raw transactions. See `RS-WB-1` and `RS-WB-2`. |
| Server initialization, platform routes, tenant-principal routes, extractors, router/state | `AGENTS.md` §§5-6, 9, 16; architecture patterns; security posture; errors reference | **FAIL** | Struct-centered route owners and instrumentation are present, but `TenantProvisioning`/`TenantRecovery` store raw pools and public failures include SQL/crypto/parser strings. See `RS-WB-2` and `RS-WB-3`. |
| Shared Rust client, Rust SDK re-export, transport header consolidation | `AGENTS.md` §§2-3, 9; architecture patterns client boundary; doctrine | **PASS** | Administrative client behavior stays in `wyrd-client`; `wyrd-sdk-rust` remains a thin re-export; no SQL/server dependency was added to the client tier. |
| CLI authentication/admin commands and client consolidation | `AGENTS.md` §§3, 9, 11, 16; agent-harness; errors reference | **PASS** | Commands route through `wyrd-client`; machine-readable stable errors remain projected rather than reimplemented. |
| MCP principal tools and MCP journey coverage | `AGENTS.md` §§2, 9, 11; agent-harness | **PASS on source inspection** | Typed contracts and dispatch-time scope refusal are covered in `wyrd-mcp/tests/bifrost/mcp/principals.rs`; no parallel durable behavior was added. The MCP lane was not rerun by this reviewer. |
| Real-server, CLI, SQL, auth, and fixture tests | `AGENTS.md` §§11-12, 16; `TESTING.md`; testing-workflows reference | **FAIL** | Journey and integration tests exist, but the new canonical principal integration and platform journey tasks fail before selecting a test because `setup:postgres` does not exist. See `RS-WB-5`. A fixture comment also describes a lookup invariant the shipped query no longer has (`RS-WB-6`). |
| OpenAPI, JSON schemas/goldens, docs, docs generator | `AGENTS.md` §§9, 11-12, 16; doctrine; agent-harness generated-artifact rules | **FAIL** | Generated contract artifacts are present and prior `codegen:check` evidence is available, but required architecture documents were not amended (`RS-WB-4`) and changed public rustdoc emits a candidate-attributable warning (`RS-WB-7`). |
| `mise.toml`, lock/manifests, repository checks | `AGENTS.md` §§1, 4, 11-12; testing-workflows | **FAIL** | Static boundary checks passed, but two new tasks reference an absent dependency and the principal slice tasks use positional `cargo test` filters instead of repository-standard exact nextest selection. See `RS-WB-5`. |
| Vala audit schema projection for new principal kinds | `AGENTS.md` §§2-3; single-audit-path authority; Bifrost audit architecture | **FAIL** | The retained audit projection was extended for `tenant_admin`, but platform authorization bypasses it through a new standalone table. See `RS-WB-1`. |
| Commit metadata over the complete branch range | `AGENTS.md` §13 | **FAIL** | Many commits use `Claude <noreply@anthropic.com>` as author/committer and contain AI co-author/session trailers. See `RS-WB-8`. |

## Applicable-rule results

| Rule | Result | Exact evidence |
|---|---:|---|
| One canonical audit append to `vala.audit_staging`; no other audit table or sink | **FAIL** | `20260601000021_platform_authz_audit.sql:1-40`; `queries/platform/audit_authz.rs:49-74`; `platform_authz.rs:117-143`. |
| Every permission decision is audited transactionally and fails closed | **PARTIAL** | The new platform path does couple decision and operation transactionally, but does so through the prohibited alternate table. Tenant principal routes use the canonical tenant writer. |
| Only `TenantConn` and `OperatorPool` appear as SQL capabilities in library fields/signatures | **FAIL** | `platform/provisioning.rs:89,102`; `platform/recovery.rs:39,52`; `platform_authz.rs:97`; six `wyrd-sql/src/queries/platform/*.rs` transaction parameters. |
| Tenant work uses RLS and does not add manual tenant filters | **PASS** | `mise run check:tenant-isolation` passed; new tenant operations acquire `TenantConn`. |
| Public boundaries do not leak database, storage, provider, filesystem, or cryptography error strings | **FAIL** | `platform/routes.rs:100-103,201-205`; `platform_extractor.rs:105-120,162-169`; `platform/identity.rs:127-131,626-688`; `principals/routes.rs:544-549`. |
| Governing design/security/doctrine stays aligned with changed contracts and behavior | **FAIL** | Current `wyrd-design.md:456-470` and `wyrd-security-posture.md:42-48` retain the old three-kind, Card-bound model; `architecture/v1/00-foundations/service-identity.md:3-16` and tenant OIDC spec `REQ-003` also contradict the candidate. |
| Struct-centered Rust with cohesive service owners | **PASS** | `PlatformSessions`, `PlatformAuthorization`, `TenantProvisioning`, `TenantRecovery`, client `Platform`/`Principals` handles, and route modules own their dependencies/workflows. SQL capability choice remains separately noncompliant (`RS-WB-2`). |
| Async only for awaited IO/composition | **PASS** | Changed async workflows await SQL, HTTP, crypto offload, or compose those operations; pure parsing/validation remains synchronous. |
| Stable derive-backed public errors and one server mapper | **PASS with redaction violation** | `mise run check:single-into-response-impl` passed and catalog metadata is derive-backed. Unsafe `details` construction is `RS-WB-3`. |
| No production `unwrap`; Clippy allows justified; mocks scoped | **PASS** | `mise run check:unwrap-audit`, `check:clippy-allow-audit`, and `check:mocks-scope` passed. |
| Every new user/agent surface has appropriate journey coverage | **PASS on source existence; FAIL on runnable lane** | Platform, CLI, and MCP journeys exist; `test:platform:journey` cannot start (`RS-WB-5`). |
| Generated files derive from source and regenerate cleanly | **PASS on available evidence** | Prior task evidence records clean `codegen:check`; diff inspection shows source and generated OpenAPI/schema changes together. This reviewer did not rerun Cargo-backed codegen while other required Cargo verification was active. |
| Rustdoc is valid, accurate, and maintainer-usable | **FAIL** | `service_accounts.rs:176` links public docs to a private constant, producing `rustdoc::private_intra_doc_links`; fixture prose at `server.rs:2670-2672` is stale. See `RS-WB-6` and `RS-WB-7`. |
| Canonical verification tasks exist and select real tests | **FAIL** | `mise.toml:116-139`; both Postgres tasks depend on missing `setup:postgres`; unit/integration slices use positional `cargo test` filters. |
| Git author/committer identity and no AI trailers | **FAIL** | Complete commit-range inspection; see `RS-WB-8`. |

## Material findings

### RS-WB-1 — Alternate platform audit table violates the single authoritative audit path

- Violated rule: `AGENTS.md` §2 and `architecture/agent-rules.md` require every
  audit event to enter `vala.audit_staging` through the canonical append and
  explicitly prohibit any other audit table, WAL, relay, or sink.
- Location: `crates/wyrd/wyrd-sql/migrations/20260601000021_platform_authz_audit.sql:1-40`;
  `crates/wyrd/wyrd-sql/src/queries/platform/audit_authz.rs:39-74`;
  `crates/wyrd/wyrd-auth/src/platform_authz.rs:112-143`.
- Evidence: the candidate creates, grants, writes, queries, and tests
  `platform.audit_authz` as durable authorization history. It never enters
  `vala.audit_staging`, never reaches the sole `AuditPublisher`, and therefore
  never reaches retained `vala.system.audit_log` through the mandated path.
- Consequence: Wyrd has two incompatible audit authorities. Platform
  administrative decisions are absent from the retained canonical history and
  its publisher, watermark, deduplication, and retention guarantees.
- Testable correction: remove the alternate platform audit table and query
  path and route platform authorization decisions through the existing
  canonical append/publisher while preserving fail-closed behavior and the
  operation/decision transaction contract. If a tenantless platform operation
  cannot satisfy both current rules through an existing `SYSTEM_OWNER`-scoped
  mechanism, this is an unresolved persistent-data/transaction-semantics
  decision and must be returned to the spec owner rather than silently keeping
  the second table. Proof must show allowed and denied platform decisions in
  canonical retained audit history and show an append failure prevents the
  operation.

### RS-WB-2 — Raw pools and caller-provided transactions cross library boundaries

- Violated rule: `architecture/agent-rules.md:6` and Rust-core
  `Postgres And Tenant Boundaries` permit only `TenantConn` and `OperatorPool`
  in library fields/signatures and explicitly prohibit raw `PgPool` and a
  caller-provided `Transaction<'_, Postgres>`.
- Location: `crates/wyrd/wyrd-server/src/components/platform/provisioning.rs:85-103`;
  `crates/wyrd/wyrd-server/src/components/platform/recovery.rs:33-53`;
  `crates/wyrd/wyrd-auth/src/platform_authz.rs:90-97`;
  `crates/wyrd/wyrd-sql/src/queries/platform/{audit_authz.rs:49-51,credentials.rs:105-108,identity.rs:243-246,principal_grants.rs:24-27,principals.rs:55-58,provisioning.rs:27-31}`.
- Evidence: `TenantProvisioning` and `TenantRecovery` store `sqlx::PgPool` and
  expose it in their constructors; `PlatformAuthorization::authorize` returns
  a raw transaction; six SQL query functions accept a raw transaction supplied
  by their caller. The passing tenant-isolation script does not sanction these
  shapes; it simply does not detect them.
- Consequence: tenant and operator capability ownership is no longer enforced
  by the repository's two sanctioned types. Callers can propagate raw SQL
  capabilities, and the transaction boundary becomes an undocumented parallel
  API that future code can misuse.
- Testable correction: keep tenant acquisition behind `WyrdPostgres` and
  `TenantConn`, keep cross-tenant work behind `OperatorPool`, and remove raw pool
  and raw transaction types from every changed library field/signature without
  weakening transactional audit. Add or extend the existing boundary check so
  the exact prohibited candidate shapes fail statically only if the current
  compiler/type boundary cannot already enforce them.

### RS-WB-3 — Public administrative errors expose internal source strings

- Violated rule: `architecture/references/languages/errors.md` requires boundary
  conversion to log internal failures and return stable safe public details;
  raw database, provider, filesystem, and cryptography strings must not cross
  HTTP.
- Location: `crates/wyrd/wyrd-server/src/components/platform/routes.rs:100-103,201-205`;
  `components/auth/platform_extractor.rs:105-120,162-169`;
  `components/platform/identity.rs:127-131,626-688`;
  `components/principals/routes.rs:544-549`.
- Evidence: the candidate places `SqlError`, `sqlx::Error`, session key/store,
  serialization, and other internal `Display` strings directly in
  `WyrdError::{Internal,Validation}.details`. The single response mapper emits
  `as_problem_json()` unchanged, so these values are client-visible. The helper
  at `principals/routes.rs:544` even claims to avoid leaking detail while adding
  `error.to_string()` to the response.
- Consequence: public callers can receive schema/table/constraint, database,
  parser, provider, or cryptographic implementation detail, weakening the
  security boundary and making stable errors depend on internal libraries.
- Testable correction: record source errors through structured server tracing
  at the conversion boundary and return only the stable code/message plus
  deliberately safe typed details. Add focused route tests whose injected
  store/key failures assert the public problem body omits the source string.

### RS-WB-4 — The implementation ships a new identity model without its required architecture amendments

- Violated rule: `AGENTS.md` §§1-2 and §14 make current architecture the
  authority and require lasting contract changes to update it. The approved
  spec independently lists these amendments as part of the change at
  `changes/active/admin-principals/spec.md:546-562`.
- Location: `architecture/wyrd-design.md:452-470`;
  `architecture/wyrd-security-posture.md:40-48,65-82`;
  `architecture/v1/00-foundations/service-identity.md:1-20`;
  `architecture/v1/00-foundations/principal-extraction.md:3-7`;
  `changes/active/tenant-oidc-federation/spec.md:77-97`.
- Evidence: current authority still says the principal kind set is exactly
  `User`, `Service`, `Agent`, every Service is Card-bound, every principal has a
  tenant, every human OIDC connection is tenant-owned, and every revocation
  advances an epoch. The candidate implements `GlobalAdmin`, `TenantAdmin`,
  Card-free Services, a tenantless platform context and OIDC connection, and a
  no-cache platform revocation model. No governing architecture file changed in
  the 204-file range.
- Consequence: future contributors are instructed to reject or overwrite the
  shipped behavior, and generated/public contracts disagree with the declared
  security model.
- Testable correction: amend the exact governing identity, security,
  foundation, deployment, and tenant-OIDC authorities named by the approved
  spec so they describe the shipped two-plane model, principal kinds, optional
  Card binding, platform OIDC ownership, initialization/recovery, and each
  plane's revocation mechanics. Then run the docs and design-sync gates.

### RS-WB-5 — Candidate-added verification lanes are not runnable or selector-safe

- Violated rule: `AGENTS.md` §11-12 and testing-workflows require canonical
  repository-managed setup, exact nextest selection for named Rust tests, and
  prohibit positional filters that can pass after selecting no test.
- Location: `mise.toml:116-139`.
- Evidence: both `test:platform:journey` and
  `test:principals:integration` declare `depends = ["setup:postgres"]`, but
  `mise tasks ls` contains no such task. Direct execution of each candidate task
  fails immediately with `mise ERROR task not found: setup:postgres`.
  `test:principals:unit` and two integration commands additionally use
  positional `cargo test` substring filters instead of the required exact
  nextest expressions or an environment-owning whole-target lane.
- Consequence: the branch's advertised capability gates cannot run in CI or
  locally, and its substring slices can silently stop selecting intended tests
  after a rename.
- Testable correction: reuse the repository's existing
  `scripts/postgres/with-test-postgres.sh` plus migration wrapper used by nearby
  working lanes, and use whole explicit targets or exact nextest expressions.
  Prove both tasks execute nonzero tests and pass from a clean local setup.

### RS-WB-6 — The changed fixture documents an equality lookup that no longer exists

- Violated rule: `AGENTS.md` §15-16 requires maintainers to understand the
  complete path and treats documentation correctness as implementation
  correctness.
- Location: `crates/wyrd/wyrd-testing/src/server.rs:2670-2672` compared with
  `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:10-38`.
- Evidence: the fixture says the principal retains a UID-less `card_ref` "for
  the exact JSONB lookup". The candidate deliberately changed the production
  lookup from equality to JSONB containment (`card_ref @> $3`) precisely because
  exact lookup matched no registered UID-bearing principal.
- Consequence: the fixture states the inverse of the credential-issuance
  predicate and will misdirect the next maintainer debugging cross-space or UID
  matching.
- Testable correction: rewrite the two-line comment to describe the actual
  registration-vs-lookup projection and containment behavior; no fixture logic
  or new test is needed.

### RS-WB-7 — Changed public rustdoc emits a candidate-attributable private-link warning

- Violated rule: `AGENTS.md` §16 and `architecture/agent-rules.md` make valid,
  maintainer-usable rustdoc a hard acceptance criterion.
- Location: `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:174-179`.
- Evidence: public `service_account_by_card_ref` links to private constant
  ``[`SERVICE_ACCOUNT_BY_CARD_REF_SQL`]``. `cargo doc -p wyrd-sql --no-deps`
  emits `rustdoc::private_intra_doc_links` for this changed line. The warning is
  candidate-attributable; the separate pre-existing `[IdError]` broken link is
  not.
- Consequence: the public documentation cannot resolve the claimed link, and
  no current CI rustdoc lane covers `wyrd-sql`.
- Testable correction: use ordinary code formatting instead of a public
  intra-doc link (or otherwise make the public docs self-contained), then show
  the candidate-attributable warning is absent. Expanding the repository
  rustdoc lane is a separate check-cost decision and is not required to close
  this source defect.

### RS-WB-8 — Branch history violates repository Git identity rules

- Violated rule: `AGENTS.md` §13 requires the configured contributor identity
  `Thorrester <sjforrester32@gmail.com>`, forbids signing as anyone else, and
  forbids AI co-author trailers.
- Location: commit metadata throughout
  `c5c20754a167e8f4d74a555a720bd51df6179a6f..072cf8b30c7135e8cf15f92da3e371a9c999703c`.
- Evidence: commits including `c488028d3`, `bcdf1f425`, `fc5d444eb`,
  `eb7488cf1`, and many successors use `Claude <noreply@anthropic.com>` as
  author and committer. Earlier and later commits through `4668d8d33` contain
  `Co-Authored-By: Claude ...`; many also contain `Claude-Session:` trailers.
  Later commits use the required identity, but do not make the complete branch
  compliant.
- Consequence: the delivered branch attributes repository work to a prohibited
  identity and retains explicitly forbidden AI trailers.
- Testable correction: the branch owner must decide whether to rewrite the
  unmerged branch history; an implementation agent must not perform that
  destructive operation implicitly. Closure proof is a complete-range log
  showing the required author/committer identity and no AI co-author trailer on
  every commit.

## Supplied-issue validation

| Supplied issue | Disposition | Evidence |
|---|---|---|
| `auth_e2e::cache_ttl_path_also_flips_verdict` fails with `WYRD_AUTH_503_VERIFY_UNAVAILABLE` | **Confirmed pre-existing; not a candidate regression. It still blocks the user's requested all-repository-green state.** | The test file is absent from the branch diff. Base blame shows `DelegateError::Database(_) -> AuthVerifyUnavailable` predates the branch at the corresponding conversion. Prior preserved evidence records the identical focused failure at the branch base and candidate. This reviewer did not duplicate the Cargo run while other agents were using the shared target. |
| `UNIQUE (data_tenant_id, name)` blocks same-named Cards across spaces | **Confirmed pre-existing architecture defect and spec-owner handoff; not filed as a candidate standards finding.** | The constraint exists at base and candidate in `20260601000001_auth.sql:85`; `CardRef` identity includes optional space and registry registration canonicalizes `(kind, space, name)` (`wyrd-design.md:1378,1450-1468`). The branch's containment lookup now explicitly relies on the stronger constraint at `service_accounts.rs:19-28`. Choosing the replacement durable uniqueness/lookup key changes persistent identity semantics and is not decided by this approved change. |
| Stale fixture comment at `wyrd-testing/src/server.rs:2670-2672` | **Confirmed and filed as `RS-WB-6`.** | The candidate's `@>` containment query contradicts the comment's "exact JSONB lookup" claim. |
| `cargo doc -p wyrd-sql` emits `private_intra_doc_links`; crate absent from rustdoc lane | **Confirmed and filed as source defect `RS-WB-7`; lane widening explicitly excluded.** | The public-to-private link is candidate-attributable. Absence of a CI gate does not make invalid changed rustdoc compliant; it only explains why automation missed it. The check-cost decision is separate. |

## Verification notes

Executed during this review:

| Command | Result |
|---|---|
| `git diff --check base..candidate` | PASS |
| `mise run check:clippy-allow-audit` | PASS |
| `mise run check:tenant-isolation` | PASS |
| `mise run check:unwrap-audit` | PASS |
| `mise run check:mocks-scope` | PASS |
| `mise run check:single-into-response-impl` | PASS |
| `mise run test:platform:journey` | FAIL before test selection: missing `setup:postgres` |
| `mise run test:principals:integration` | FAIL before test selection: missing `setup:postgres` |
| Complete-range author/committer/trailer inspection | FAIL (`RS-WB-8`) |

Cargo-backed compilation, tests, codegen, docs, and rustdoc were not launched by
this reviewer because other required reviewers were already running Cargo
against the shared checkout/target, and testing authority forbids overlapping
Cargo work. Preserved task evidence shows many focused lanes passed, but it does
not close the structural findings above. The supplied `auth_e2e` failure is
independently established at the base and remains a whole-repository green
verification gap even though it is not attributable to this candidate.
