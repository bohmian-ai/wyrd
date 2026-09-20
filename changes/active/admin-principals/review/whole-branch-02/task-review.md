# Admin principals re-review — task implementation

## Immutable subject

| Item | Value |
|---|---|
| Repository | `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces` |
| Base | `c5c20754a167e8f4d74a555a720bd51df6179a6f` |
| Candidate | `5293546f33b3a5fd9de529098e23ea70d472c412` |
| Approved authority | `changes/active/admin-principals/spec.md`, revision 7 |
| Original tasks | `TASK-001` through `TASK-008` |
| Prior review | `changes/active/admin-principals/review/whole-branch-01/` |
| Reviewed range | Complete cumulative base-to-candidate range, 100 commits |

The candidate remained unchanged during this review. I reviewed the approved
specification, all eight original tasks, the prior whole-branch reports and
remediation packet, current source and callers, the complete cumulative diff,
and commit metadata. I did not run Cargo or `mise`; the orchestrator owns
sequential verification.

## Overall result

**FAIL**

Most of the prior behavioral findings are closed: initialization fault and
concurrency proof, tenant lifecycle operations, resumable provisioning,
non-active recovery refusal, platform credential lifecycle, platform-human
grants, last-administrator protection, SSRF pinning, constant-work credential
rejection, CLI/MCP journeys, typed administrative errors, and source
documentation are now present.

The cumulative candidate still does not satisfy the original change. Four
prior findings remain open or only partially closed, two original acceptance
obligations were missed by both the implementation and prior review, and an
unrelated approved change was added to the reviewed branch.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001..011, INV-001..003, INV-008..010, INV-012 — durable principals and principal-generic verifier-only credentials | Principal/credential migrations and tenant/platform credential owners remain present; status is checked on exchange | Prior focused principal lanes; no new execution in this review | PASS |
| REQ-012..019, especially REQ-013 — one closed two-plane context carrying identity, principal type, and scope | `AuthContext::Tenant` carries `Principal.kind`; `AuthContext::Platform` carries only id and permissions | Platform journey proves plane separation, but no test can prove a field that is absent | **FAIL — `FIND-admin-principals-R2-2`** |
| REQ-020..024, AC-001 — initialization, concurrency, staged failure, retry, and uninitialized server behavior | `boot/init.rs`; new real-server cases in `platform_admin_e2e.rs` | Remediation reports 28/28 platform journey | PASS |
| REQ-025..028, AC-007..008 — provisioning convergence and tenant lifecycle administration | Stale claim adoption, failure propagation, list/inspect/suspend/resume routes, client and CLI projections | Real-server interrupted/racing provisioning and suspend/resume cases exist | PASS for behavior; audit coupling and SQL capability ownership still fail below |
| REQ-029..031, AC-004, AC-012 — tenant administration, isolation, restricted machine principals | Tenant principal routes and RLS queries; cross-tenant guard shared by credential operations | Cross-tenant and restricted-principal journeys exist | PASS |
| REQ-032..033, AC-006 — same-principal recovery only for active tenants | `TenantRecovery::recover` checks the platform directory before tenant acquisition | Non-active state recovery journey exists | PASS for behavior; SQL capability ownership still fails below |
| REQ-034..035, REQ-041..046, AC-011, AC-015..017 — human identity and platform human administration | Human registration now installs the fixed grant; status protection and screened OIDC paths exist | Platform journeys cover registration, login, operation, and last-admin protection | PASS except actual platform principal kind is discarded (`FIND-admin-principals-R2-2`) |
| REQ-036, REQ-038..040, REQ-047, AC-002, AC-013..014 — HTTP/OpenAPI, shared client, CLI, MCP, documentation, and removal of bootstrap path | Tenant/platform CLI commands, typed errors, MCP write observation, generated contract, and self-hosting workflow are present | Reported codegen/docs/CLI/MCP lanes are green | PASS |
| REQ-037, AC-009 — every decision uses canonical audit, is coupled to same-plane effect, and records the authenticating credential | Canonical `vala.audit_staging` is now used | Source contradicts completion: several effects run after the allowance commits, and the audit model has no credential field | **FAIL — `FIND-admin-principals-1`, `FIND-admin-principals-R2-3`** |
| Required architecture amendments | `wyrd-design`, foundations, tenant-OIDC spec, and admin route prose were amended | `docs:check` reported green | **FAIL — security posture still publishes the old closed kind set (`FIND-admin-principals-4`)** |
| Only `TenantConn` and `OperatorPool` cross library SQL fields/signatures | Raw `PgPool` fields were removed | Replacement stores `WyrdPostgres`; raw SQLx transactions still cross query signatures | **FAIL — `FIND-admin-principals-2`** |
| Non-goals and branch scope remain excluded | Admin implementation itself does not add a new RBAC engine, Card kind, UI, or compatibility alias | Complete diff inspection | **FAIL — the cumulative range includes the unrelated verified-change-contract revision (`FIND-admin-principals-R2-1`)** |
| AGENTS.md section 13 provenance | Later commits use the configured contributor identity | Complete range still contains prohibited Claude authors/committers and trailers | **FAIL — `FIND-TASK-001-10`** |
| VER-002 exact named-test evidence | Tests exist and focused capability lanes were reported green | The committed remediation evidence records aggregate `mise` lanes, not every named exact nextest expression; orchestrator verification may supplement this | LIMIT — not a separate finding because source defects already require remediation |

## Proposed findings

### FIND-admin-principals-1 — VIOLATION — canonical platform audit is still separated from same-plane mutations

- **Prior status:** revised but still open. The unsupported
  `platform.audit_authz` table was deleted and decisions now stage through the
  canonical Vala append. That closes only the first half of the finding.
- **Violated obligation:** REQ-037, AC-009, AGENTS.md, and
  `architecture/agent-rules.md` require a same-plane authorization decision and
  its effect to commit or roll back together.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/platform/provisioning.rs:287-309`;
  `crates/wyrd/wyrd-server/src/components/platform/identity.rs:114-129,151-213,287-304,488-511`.
- **Evidence and reachability:** `TenantProvisioning::set_suspended` commits the
  audited allowance at lines 302-305, then calls `set_tenant_suspended` through
  a new operator transaction at lines 307-309. The shared identity `authorize`
  helper likewise commits before `configure_connection`, `remove_connection`,
  and `set_admin_status` perform their platform-table mutations. All are served
  routes. A store failure after authorization therefore leaves a durable
  `allowed` row for an effect that did not commit.
- **Observable consequence:** retained audit can state that a privileged
  platform operation was allowed even though its same-plane mutation failed,
  defeating the transactional attribution guarantee.
- **Required testable correction:** keep the canonical staging path, but pass
  the returned operator-owned transaction into every same-plane mutation and
  commit once. Reads and deliberately cross-plane tenant work retain their
  truthful existing boundaries; do not introduce distributed transactions or
  another audit store. Inject a same-plane write failure for tenant suspension,
  OIDC connection mutation, and administrator status mutation and prove neither
  effect nor allowance commits.

### FIND-admin-principals-2 — VIOLATION — the SQL capability boundary was changed, not closed

- **Prior status:** open.
- **Violated obligation:** `architecture/agent-rules.md` permits exactly
  `TenantConn` and `OperatorPool` in library struct fields and signatures and
  explicitly forbids caller-handed raw SQLx transactions.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/platform/provisioning.rs:88-115`;
  `crates/wyrd/wyrd-server/src/components/platform/recovery.rs:34-58`;
  `crates/wyrd/wyrd-sql/src/operator_pool.rs:38`;
  `crates/wyrd/wyrd-sql/src/queries/platform/{credentials.rs:107,identity.rs:245,principals.rs:57,principal_grants.rs:26,provisioning.rs:28}`.
- **Evidence and reachability:** remediation replaced `PgPool` fields with
  `WyrdPostgres`, but that is still a third connection-bearing capability in
  the two live service structs and constructors. `OperatorPool::begin` exports
  a raw SQLx transaction, and the live `*_tx` query functions accept it across
  module signatures.
- **Observable consequence:** the type boundary no longer guarantees that
  tenant work enters through a caller-owned `TenantConn` or cross-tenant work
  stays behind an `OperatorPool`; future methods can use broader pool authority
  without changing their dependency shape.
- **Required testable correction:** acquire tenant transactions at the existing
  server composition boundary and pass `&mut TenantConn<'_>` into the tenant
  operation. Keep bounded operator transactions behind `OperatorPool`-owned
  methods rather than exporting raw transaction parameters. Add no wrapper or
  third connection type. A static source check plus focused initialization,
  registration, provisioning, and recovery tests closes the finding.

### FIND-admin-principals-4 — MISSING — the security authority still rejects the shipped principal model

- **Prior status:** partially closed.
- **Violated obligation:** the approved spec's Required architecture amendments
  explicitly names `architecture/wyrd-security-posture.md`.
- **Exact location:** `architecture/wyrd-security-posture.md:40-50`.
- **Evidence:** the file still states that `PrincipalKind` is only `User`,
  `Service`, `Agent`, and `System`, and describes Card-bound provisioning as
  the principal model. The implementation and `wyrd-design.md` ship
  `PlatformAdmin` and `TenantAdmin`, platform principals without tenants,
  Card-free tenant automation, and human platform principals. Later paragraphs
  mention platform credentials and OIDC but never repair the governing closed
  set.
- **Observable consequence:** the active security authority contradicts the
  code and permits future security work to reject or erase the administrative
  identities this change requires.
- **Required testable correction:** replace the stale lifecycle opening with the
  revision-7 two-plane principal and credential model, including grant-held
  platform authority and optional Card binding. Reuse the existing design
  wording rather than creating a parallel explanation; run docs checks and a
  targeted stale-model search.

### FIND-TASK-001-10 — VIOLATION — prohibited AI commit metadata remains in the immutable range

- **Prior status:** open. A committed review note calls it waived, but AGENTS.md
  section 13 remains unchanged and the reviewed conversation contains no
  instruction overriding that repository rule for this review.
- **Violated obligation:** AGENTS.md section 13.
- **Exact location:** commit objects in
  `c5c20754a167e8f4d74a555a720bd51df6179a6f..5293546f33b3a5fd9de529098e23ea70d472c412`.
- **Evidence:** 85 commits in the current range contain `Claude` in author,
  committer, message trailers, or session metadata. The branch also committed a
  document saying those prohibited identities and trailers remain.
- **Observable consequence:** the cumulative candidate fails the repository's
  provenance rule regardless of working-tree correctness.
- **Required testable correction:** the branch owner rewrites only the unmerged
  offending commits using the already-configured contributor identity and
  removes AI co-author/session trailers, without changing the cumulative tree.
  Do not use `git config`, identity environment variables, or replacement
  trailers. Prove the rewritten tree is identical and inspect every commit.

### FIND-admin-principals-R2-1 — DRIFT — an unrelated change packet entered the cumulative candidate

- **Violated obligation:** `$wyrd-task-review` permits PASS only when no
  unrelated change entered the complete base-to-candidate diff; the approved
  admin-principals scope does not alter the Verifier contract.
- **Exact location:** `AGENTS.md:67-75`;
  `architecture/wyrd-doctrine.mdx:76-136`;
  `changes/active/verified-change-contract/spec.md:1-1045` and its eight new
  task files and architecture artifacts.
- **Evidence:** commits `63bc79127` and `5293546f3` approve and plan
  `SPEC-verified-change-contract`, replace Drift/Eval Card kinds with Verifier,
  add Operator-connection decisions, and change repository-wide doctrine. None
  is needed to implement global or tenant administrative principals.
- **Observable consequence:** the reviewed admin-principals candidate also
  changes a separate approved product contract, so acceptance, rollback, and
  remediation cannot be scoped to the requested change.
- **Required testable correction:** split the verified-change-contract commits
  onto their owning branch and present an admin-principals candidate whose
  base-to-candidate range contains only this approved change and its review
  artifacts. Preserve the other change's tree on its own branch; do not delete
  or reinterpret its approved decisions.

### FIND-admin-principals-R2-2 — INCORRECT — platform authentication drops the principal type and audits every human as GlobalAdmin

- **Violated obligation:** REQ-013 requires the authenticated context to carry
  server-verified principal identity, principal type, and control-plane scope;
  REQ-037 and AC-009 require accurate principal attribution.
- **Exact location:** `crates/shared/wyrd-runtime/src/principal.rs:362-383`;
  `crates/wyrd/wyrd-auth/src/platform_sessions.rs:45-58,229-284`;
  `crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs:165-177`;
  `crates/wyrd/wyrd-auth/src/platform_authz.rs:153-186`.
- **Evidence and reachability:** `PlatformPrincipal` contains only id and
  permissions. Both credential and federated session confirmation discard the
  `platform.principals.principal_kind` row value. The extractor therefore
  constructs a kind-less context, and `decision_event` hard-codes
  `PrincipalKindTag::GlobalAdmin`. A pre-registered human platform principal is
  stored as `User`, but every protected route audits it as `global_admin`.
- **Observable consequence:** downstream authorization cannot satisfy the
  required typed-context contract, and retained audit falsely identifies human
  platform administrators as bootstrap global administrators.
- **Required testable correction:** preserve the stored platform principal kind
  through session confirmation into `PlatformPrincipal`, validate that it is a
  platform-eligible kind, and use that verified kind in canonical audit events.
  Prove credential-root and federated-human sessions produce distinct correct
  kinds while retaining the same platform scope and permissions.

### FIND-admin-principals-R2-3 — MISSING — authorization audit never records the authenticating credential

- **Violated obligation:** REQ-037 and AC-009 explicitly require each covered
  decision to name the credential that authenticated the request, with no
  secret material.
- **Exact location:** `crates/wyrd-spec/src/vala/api.rs:2708-2733`;
  `crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs:44-55`;
  `crates/wyrd/wyrd-server/src/components/auth/caller_extractor.rs:11-31`;
  `crates/wyrd/wyrd-auth/src/platform_authz.rs:153-186`;
  `crates/vala/vala-sql/src/queries/audit_staging.rs:78-104`.
- **Evidence and reachability:** platform extraction correctly resolves
  `credential_id`, and its rustdoc claims the value is carried into audit, but
  `PlatformAuthorization::authorize` accepts only `AuthContext` and never reads
  `PlatformCaller.credential_id`. `AuditEvent` and `vala.audit_staging` have no
  credential-id field. Tenant access-token claims already carry optional `cid`,
  but `VerifiedToken`/`AuthenticatedPrincipal`/`Caller` discard it before audit.
- **Observable consequence:** incident responders can identify the principal
  but cannot determine which of several concurrent credentials performed a
  privileged action or distinguish credential-backed platform sessions from
  human OIDC sessions as the approved audit contract requires.
- **Required testable correction:** preserve optional `cid` from verified token
  claims through the existing authenticated caller types and add one optional,
  non-secret credential id to the canonical audit event/staging/publication
  path. Platform credential sessions populate it; tenant credential sessions
  populate it; federated sessions leave it empty. Do not add a second audit
  detail or store. Prove two credentials for one principal produce attributable
  rows and that no plaintext enters audit.

## Prior-finding closure table

| Prior finding | Re-review status | Evidence |
|---|---|---|
| `FIND-admin-principals-1` | **OPEN / REVISED** | Canonical store fixed; same-plane effects still commit separately |
| `FIND-admin-principals-2` | **OPEN** | `WyrdPostgres` fields and raw transaction signatures remain |
| `FIND-admin-principals-3` | CLOSED | `internal_failure` traces source and serves stable empty details |
| `FIND-admin-principals-4` | **OPEN / PARTIAL** | security posture closed-set statement remains stale |
| `FIND-admin-principals-5` | CLOSED | principal lanes own Postgres setup and select nonzero targets/slices |
| `FIND-admin-principals-6` | CLOSED | containment comment and private rustdoc link corrected |
| `FIND-admin-principals-7` | CLOSED | registered administrator receives fixed platform grant transactionally |
| `FIND-admin-principals-8` | CLOSED | usable-authority guard and serialized status change added |
| `FIND-admin-principals-9` | CLOSED | platform issue/list/revoke HTTP, client, CLI, and journey added |
| `FIND-admin-principals-10` | CLOSED | shared screened/pinned client reaches runtime discovery/token/JWKS paths |
| `FIND-admin-principals-11` | CLOSED | every invalid platform credential executes one Argon2 verification |
| `FIND-admin-principals-12` | CLOSED | authorized MCP revoke and observed retirement added |
| `FIND-admin-principals-13` | CLOSED | typed problems and required revoke body/reason implemented |
| `FIND-admin-principals-14` | CLOSED | exact MCP catalogs include principal tools; reported lane 9/9 |
| `FIND-TASK-001-10` | **OPEN** | prohibited metadata remains; committed waiver does not alter AGENTS.md |
| `FIND-003-2` | CLOSED | concurrency, staged failure/retry, and uninitialized journeys added |
| `FIND-003-3` | CLOSED | dead variant and bootstrap vocabulary removed outside history |
| `FIND-004-2` | CLOSED | list/inspect/suspend/resume shipped through route, client, and CLI |
| `FIND-004-3` | CLOSED | stale provisioning claims are adoptable; mark-failed errors surface |
| `FIND-004-4` | CLOSED | real failure/cancellation/race journey exists |
| `FIND-004-5` | CLOSED | actual CLI binary drives the operator workflow; docs match |
| `FIND-005-2` | CLOSED | real cross-tenant principal/credential refusal journey exists |
| `FIND-006-3` | CLOSED | recovery gates on active directory state |
| `FIND-006-4` | CLOSED | canonical append failure leaves tenant mutation absent |

## Verification evidence assessment

- The prior remediation report records green `fmt`, lints, codegen, docs,
  client-tier, unwrap, `wyrd-sql` rustdoc, platform (28/28), MCP (9/9),
  principal integration (5/5), principal unit (4/4), and CLI (24 pass, 5
  ignored) lanes. Those results are useful for covered paths but cannot refute
  the source-proven failures above.
- The report does not record every named test through the exact command form
  required by VER-002. The orchestrator may add current exact evidence, but no
  test can close the absent context/audit fields or separated transactions.
- The supplied `auth_e2e::cache_ttl_path_also_flips_verdict` failure remains a
  base-reproduced verification limit, not a candidate regression under VER-005.
- The same-name Card constraint remains a separate persistent-identity handoff.
  The fixture comment is corrected. Widening the rustdoc CI lane remains an
  unrelated permanent-cost decision.

## Review findings summary

### Critical

- None.

### Important

- `FIND-admin-principals-1` — canonical platform allowances still separate from same-plane effects.
- `FIND-admin-principals-2` — the required SQL capability boundary remains violated.
- `FIND-admin-principals-4` — the security authority still publishes the old principal model.
- `FIND-TASK-001-10` — prohibited branch metadata remains.
- `FIND-admin-principals-R2-1` — unrelated verified-change-contract work entered the candidate.
- `FIND-admin-principals-R2-2` — platform context drops kind and audits humans as GlobalAdmin.
- `FIND-admin-principals-R2-3` — canonical audit cannot attribute decisions to credentials.

### Suggestions

- None. Every retained item is required by the approved task or repository rules.
