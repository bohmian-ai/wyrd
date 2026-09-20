# Repository standards review — admin principals whole branch, round 3

## Result

`FAIL`

The immutable subject remained
`c5c20754a167e8f4d74a555a720bd51df6179a6f..a9a7c9c1e8502ccf3befa74c4283c78e5d70c132`
through this static review. The candidate closes most of the sixteen round-2
findings, including the architecture vocabulary, public source-error leaks,
last-administrator guard, provisioning retry, shipped init/recovery processes,
tenant admission cache, TenantAdmin refresh behavior, and constant-work API-key
refusals. Six material standards defects remain.

Per the current user's explicit authority, all of `FIND-TASK-001-10` is waived:
AI attribution trailers and historical Claude author/committer identities are
not findings in this review. The verified-change-contract work in the cumulative
candidate is also explicitly approved and is not drift.

## Authority coverage

| Changed surface | Applicable authority inspected | Result |
|---|---|---|
| Approved spec, TASK-001..008, both prior whole-branch reviews, and R1/R2 remediation records | `AGENTS.md` §§1-14; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; current user instructions | **FAIL** — implementation evidence overstates closure of six source-proven rules below. The history and verified-change scope exceptions are accepted exactly as instructed. |
| Principal, credential, token, and authenticated-context contracts | `AGENTS.md` §§2-4, 9; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; approved REQ-001..019, REQ-037 | **FAIL** — ordinary API-key and platform sessions now preserve credential identity, but refresh-authenticated access deliberately erases it (`FIND-admin-principals-R2-3`). |
| Platform and tenant authorization/audit workflows | `AGENTS.md` §§2, 9; `architecture/agent-rules.md` audit rules; `architecture/references/architecture/patterns.md` Audit Pattern; approved REQ-037/AC-009 | **FAIL** — allowed decisions can be made and then lost when pre-write discovery/conversion refuses (`FIND-admin-principals-1`, `FIND-005-1`). The one canonical staging/publisher path itself passes. |
| `wyrd-sql`, platform SQL, tenant provisioning/recovery, and connection ownership | `architecture/agent-rules.md` SQL boundary; approved spec Constraints/INV-001; Rust ownership reference | **FAIL** — raw `Transaction` APIs are removed, but two live server owners still store and accept `WyrdPostgres`, contrary to the rule's exhaustive two-capability list (`FIND-admin-principals-2`). |
| Canonical audit staging migration, hash chain, publisher projection, and retained audit schema | `AGENTS.md` audit decisions; security-posture audit integrity; architecture patterns; persistent-data compatibility | **FAIL** — the new nullable column unconditionally changes the canonical hash preimage for old `NULL` rows without a version/compatibility rule (`RS-R3-1`). Column projection is by name/declared order and otherwise consistent. |
| HTTP routes, stable Wyrd errors, OpenAPI source, generated `openapi.yaml`, docs | `AGENTS.md` §§2, 9, 11-12; errors and agent-harness references; approved REQ-036/AC-014 | **FAIL** — route registration and problem media are now present, but the generated `/auth/token` operation still omits reachable stable error families (`FIND-admin-principals-13`). |
| Rust structure, async boundaries, errors, secrecy, and documentation | `AGENTS.md` §§4-7; `architecture/agent-rules.md`; `architecture/references/languages/rust-core.md` | **FAIL** — struct owners and async/error/secrecy shape generally pass; new fallible/panicking helpers have incomplete required rustdoc (`RS-R3-2`). |
| Server initialization and deployment recovery | Approved REQ-020..024, REQ-033; server/client ownership; testing workflow | PASS — the real binary is spawned for init and recovery, one-time stdout behavior is asserted, and recovery reuses the existing root/issuer. |
| Last-administrator concurrency and platform OIDC identity | Approved REQ-041..046; security posture; SQL concurrency rules | PASS — usable administrators require a live credential or pinned identity on the active connection, under the existing transaction advisory lock. |
| Tenant provisioning, retry, suspension, and recovery | Approved REQ-025..033; tenancy/persistence authority | PASS except the owner-field boundary above — retry revokes prior undisclosed credentials, admission is evaluated per request, and cross-plane work remains explicit/resumable. |
| Rust client, CLI, MCP, generated schemas, docs | `AGENTS.md` §§2-3, 8-12; agent-harness/errors/testing references | PASS by source inspection and recorded focused evidence — shared client ownership is preserved, MCP/CLI project it, and generated artifacts have an owning source. |
| Tests and `mise` tasks | `AGENTS.md` §11; testing-workflows reference; approved VER-001..006 | **LIMITED** — required focused lanes are recorded green, including two 32/32 platform-journey runs. This role was instructed not to run Cargo/mise. The newly added assertions do not cover the six defects below. |
| Git provenance and concurrent verified-change-contract work | Current user instructions, which outrank repository/task authority | **WAIVED / APPROVED** — do not reopen `FIND-TASK-001-10`; do not classify verified-change-contract inclusion as drift. |

## Applicable rule results

| Rule | Result | Exact evidence |
|---|---|---|
| Only `TenantConn` and `OperatorPool` may appear as SQL connection capabilities in fields/signatures | **FAIL** | `components/platform/provisioning.rs:93-102,115` and `components/platform/recovery.rs:36-44,57` retain `WyrdPostgres`; live construction is at `components/platform/routes.rs:141,185,335`. `architecture/agent-rules.md:6` explicitly limits fields/signatures to the two named capabilities and limits `WyrdPostgres` to pool construction. |
| Every permission evaluation writes one canonical row, allowed or denied, before the operation proceeds or refuses | **FAIL** | `audit/mod.rs:270-286` evaluates and returns an allowed event. `admin/routes.rs:254-295` then performs OIDC discovery and secret sealing before append; either refusal leaves no allowed decision. The platform equivalent appends into an uncommitted transaction at `platform_authz.rs:104-142`, then `identity.rs:188-227` can refuse discovery/sealing and drop that row. This conflicts with `agent-rules.md:12-14` and `patterns.md:245-257`. |
| Canonical audit remains one staging path and one publisher | PASS | `PlatformAuthorization` and tenant `append_on` both call `vala_sql::queries::audit_staging::append_audit`; no second table, WAL, publisher, or retained authority was added. |
| Durable hash-chain encoding remains reproducible across additive schema migration | **FAIL** | The pre-R2 hash placed `permission` immediately after `principal_kind`; current `audit_staging.rs:357-373` always inserts `push_opt(credential_id)`, including a `0` byte for migrated `NULL`. Migration `20260910000027_audit_credential_id.sql:11-14` claims old rows are unaffected, but it neither versions nor rewrites their stored hashes. |
| Audit names the credential that authenticated every covered decision | **FAIL** | `refresh.rs:118-167` consumes stored refresh row `active.id` and audits the rotation, but `refresh.rs:261-274` mints the successor access token with `credential_id: None`. The active authority at `wyrd-security-posture.md:78-83` was amended to bless that omission, contradicting approved REQ-037 rather than satisfying it. |
| Generated HTTP contract includes served auth/admin operations, real problem media, and their stable route errors | **FAIL** | Registration and media conversion pass, but `/auth/token` declares only one 401 code and no 400/404 at `components/auth/routes.rs:63-75`, while the same reachable handler maps refresh replay/revocation to `WYRD_AUTH_401_REFRESH_REUSED`/`...REFRESH_REVOKED`, delegation depth to `WYRD_AUTH_400_DELEGATION_DEPTH_EXCEEDED`, and a missing delegated principal to `WYRD_AUTH_404_PRINCIPAL_NOT_FOUND` (`wyrd-spec/src/error.rs:600-635,747+`). The new test at `http/openapi.rs:329-363` checks only that a description contains *some* code with the same status, so it cannot detect omitted codes. |
| Public internal failures keep source strings server-side | PASS for the reviewed paths | Revocation and admin conversion now trace sources and return stable static messages through `internal_failure`; the previous three reachable leaks are closed. |
| Active architecture accurately describes the two planes and shipped principal names | PASS | `wyrd-design.md:140-157,452+` and `wyrd-security-posture.md:42-83` consistently use `GlobalAdmin`, `TenantAdmin`, `User`, `Service`, and `Agent`, with tenantless platform scope and Card-free administrative principals. The refresh credential exception remains a separate REQ-037 contradiction above. |
| Last usable administrator cannot be removed under races | PASS | `queries/platform/principals.rs:228-313` holds the transaction advisory lock and counts only granted active principals with a live credential or pinned identity on the active OIDC connection; focused integration cases cover unpinned and removed-connection identities plus concurrent suspension. |
| Tenant suspension/resume is effective on the next request | PASS | Tenant admission moved out of the epoch cache; recorded platform journeys ran twice with nonzero TTL. |
| Invalid API-key refusals perform one expensive verification and return one problem | PASS | Both malformed-route and parsed exchange paths converge on `credential_verify::verify_presented`; focused instrumentation asserts one call. See suggestion about production-only instrumentation below. |
| Every new/materially modified Rust item documents errors, panics, cancellation/partial progress where applicable | **FAIL** | New `main.rs:95` returns `Result` without `# Errors`; new journey helpers `platform_admin_e2e.rs:177-181,3462-3503` panic through `expect` without `# Panics`. `agent-rules.md:35` and `rust-core.md:752-770` call incomplete rustdoc `BLOCK_BEFORE_MERGE`. |
| Generated artifacts are source-owned rather than hand-maintained | PASS with recorded evidence | `ProblemMediaAddon` and the registered `utoipa` operations own `openapi.yaml`; prior evidence records `codegen:check` green. Contract completeness, not regeneration, is the failure above. |

## Material findings

### `FIND-admin-principals-2` — `VIOLATION` — SQL capability ownership remains broader than the mandatory boundary

- **Violated authority:** `architecture/agent-rules.md:6` and the approved
  spec's `TenantConn`/`OperatorPool` constraint.
- **Location:** `crates/wyrd/wyrd-server/src/components/platform/provisioning.rs:93-102,115`;
  `components/platform/recovery.rs:36-44,57`; callers in
  `components/platform/routes.rs:141,185,335`.
- **Evidence:** the raw SQLx transaction leak is closed, but both live workflow
  structs store `WyrdPostgres` and accept it in their constructors. The rule is
  exhaustive: `WyrdPostgres` may construct pools but is not a third connection
  capability for domain fields/signatures. The implementation record's
  “Material limits” explicitly acknowledges retaining these fields; that prose
  cannot waive the repository boundary.
- **Consequence:** privileged workflow owners retain access to every tenant pool
  operation rather than only the reviewed operator/tenant capabilities, so the
  tenant boundary is broader than the source check and task claim establish.
- **Testable correction:** compose tenant acquisition at the server/state
  boundary and pass only the existing `TenantConn`/`OperatorPool` capabilities
  into the operation that needs them. Do not add a wrapper or expose `PgPool`.
  Extend the existing boundary source check to cover these server owner fields
  and constructor signatures.

### `FIND-admin-principals-1` / `FIND-005-1` — `VIOLATION` — pre-write refusals erase already-made authorization decisions

- **Violated authority:** approved REQ-037/AC-009;
  `architecture/agent-rules.md:12-14`; architecture Audit Pattern.
- **Location:** `wyrd-server/src/audit/mod.rs:270-286`;
  `components/admin/routes.rs:254-295,455-481`;
  `wyrd-auth/src/platform_authz.rs:104-142`;
  `components/platform/identity.rs:188-227`.
- **Evidence:** both plane owners evaluate permission first. Tenant issuer
  creation then performs screened network discovery and sealing before it ever
  appends the allowed event. Platform connection configuration has appended the
  event only inside an open transaction, then drops it when parsing, discovery,
  or sealing refuses. The new tests intentionally assert no row after discovery
  failure. This is not an engine transition: a principal permission was
  evaluated and allowed, so the rule requires one row before the operation
  proceeds or refuses.
- **Consequence:** repeated authorized attempts that fail during discovery,
  validation, or sealing are absent from the security record, defeating the
  specified one-row-per-decision cardinality and incident attribution.
- **Testable correction:** preserve the current single canonical writer, but
  order each permission evaluation and its append so it is recorded before any
  authorized network/crypto/store work. If the workflow deliberately evaluates
  permission again at the mutation boundary, audit both evaluations as the rule
  requires. Add failure-path proof for discovery/sealing/conversion in both
  planes; do not create an operation log or second audit store.

### `FIND-admin-principals-13` — `INCORRECT` — OpenAPI publishes paths but not the stable errors those paths return

- **Violated authority:** approved REQ-036 and AC-014; `AGENTS.md` server/error
  contract rules; `architecture/references/languages/errors.md`.
- **Location:** `wyrd-server/src/components/auth/routes.rs:63-75` and
  `http/openapi.rs:314-363`; corresponding live variants at
  `wyrd-spec/src/error.rs:600-635,747-829`.
- **Evidence:** `/auth/token` serves four grant shapes. Its OpenAPI operation
  advertises only `WYRD_AUTH_401_API_KEY_INVALID` for 401 and omits reachable
  400 and 404 responses, despite refresh replay/revocation, delegation-depth,
  and missing-subject variants. The closure test only searches each response
  description for any `_<status>_` substring; it cannot compare the route's
  reachable catalog entries or notice that multiple same-status codes were
  collapsed into one prose line.
- **Consequence:** an independently generated client cannot discover or branch
  on the stable errors the actual endpoint returns, so the generated artifact
  still does not implement the route contract.
- **Testable correction:** project every reachable stable error for each grant
  through the existing `utoipa` owner (without a second catalog), and strengthen
  the source-of-truth assertion to compare operation-declared codes with the
  route's expected derive-backed code set, not merely status-shaped text.

### `FIND-admin-principals-R2-3` — `INCORRECT` — refresh-authenticated decisions lose credential attribution

- **Violated authority:** approved REQ-037/AC-009 and TASK-006's requirement
  that every decision name its authenticating credential.
- **Location:** `wyrd-auth/src/refresh.rs:118-167,261-274` and
  `architecture/wyrd-security-posture.md:78-83`.
- **Evidence:** successful rotation has the durable refresh-token row id as
  `active.id`/`rotated_from`, yet the audit event and successor access token are
  built with no credential id. The architecture edit stating that a refresh
  rotation names no credential narrows the still-approved requirement; it is
  not an approved spec revision. Existing proof covers two API keys and a
  federated `NULL`, not a refresh-authenticated operation.
- **Consequence:** once a tenant administrator rotates its returned refresh
  token, subsequent privileged decisions are indistinguishable from a
  credential-free federated session, and the rotation decision itself cannot
  identify the stored secret that authenticated it.
- **Testable correction:** reuse the consumed refresh row's existing non-secret
  id as the authenticating credential id in the rotation audit and successor
  access-token context. Prove API-key, refresh-token, and federated attribution
  separately; do not add a second identifier or free-form detail.

### `RS-R3-1` — `REGRESSION` — the additive audit column silently changes the hash encoding of every historical row

- **Violated authority:** audit-integrity requirements in
  `architecture/wyrd-security-posture.md`; the module's own canonical,
  reproducible hash contract; durable migration compatibility.
- **Location:** `vala-sql/migrations/20260910000027_audit_credential_id.sql:11-15`
  and `vala-sql/src/queries/audit_staging.rs:345-374`.
- **Evidence:** old hashes were computed with `permission` immediately after
  `principal_kind`. The new function always inserts `push_opt(None)`, adding a
  zero byte even for every migrated historical row. The migration does not
  store a hash-format version or rewrite/rechain existing entries, while its
  comment claims existing entries remain reproducible and unaffected.
- **Consequence:** recomputing a pre-migration row from its stored columns with
  the current canonical encoder produces a different digest, so legitimate
  history appears tampered and the claimed durable chain cannot be verified
  across upgrade.
- **Testable correction:** preserve the legacy preimage for legacy/absent
  credential rows or introduce an explicit versioned encoding and migration
  rule that can verify both formats without rewriting retained history. Add an
  upgrade fixture containing an old hashed row, migrate, and prove its hash plus
  a new credential-bearing successor both verify.

### `RS-R3-2` — `VIOLATION` — newly added fallible and panicking Rust helpers have incomplete mandatory rustdoc

- **Violated authority:** `architecture/agent-rules.md:35` and
  `architecture/references/languages/rust-core.md:752-770`, which classify this
  as `BLOCK_BEFORE_MERGE` even for private test helpers.
- **Location:** `wyrd-server/src/main.rs:90-105` (`recover_root` lacks
  `# Errors`); `wyrd-server/tests/platform_admin_e2e.rs:172-187,3455-3503`
  (`operator_command`, `fail_writes_to`, `stop_failing_writes`, and
  `staged_platform_decisions` panic through `expect` without `# Panics`).
- **Evidence:** these items were introduced in the R2 remediation and their docs
  describe intent but omit required error/panic contracts.
- **Consequence:** the candidate does not meet the repository's explicit Rust
  completion gate; maintainers cannot see partial-process/setup failure
  behavior from the owning item documentation.
- **Testable correction:** complete the existing rustdoc with the real error,
  panic, cancellation, and partial-progress behavior. No new abstraction or
  test is needed.

## Prior-finding disposition

| Round-2 finding | Round-3 disposition |
|---|---|
| `FIND-admin-principals-1` | **OPEN / REVISED** — same-plane writes are coupled, but allowed platform decisions disappear on pre-write refusal. |
| `FIND-admin-principals-2` | **OPEN / NARROWED** — raw transaction signatures are closed; `WyrdPostgres` remains in two domain-owner fields/signatures. |
| `FIND-admin-principals-3` | **CLOSED** — reviewed source failures now remain server-side. |
| `FIND-admin-principals-4` | **CLOSED** — both active authorities consistently describe the two planes and stable kind names, apart from the separate refresh-attribution conflict. |
| `FIND-admin-principals-8` | **CLOSED** — last-admin guard counts only usable, granted identities/credentials under serialization. |
| `FIND-admin-principals-13` | **OPEN / NARROWED** — paths and problem media are present; route-stable error coverage is incomplete. |
| `FIND-004-3` | **CLOSED** — retry retires prior undisclosed credentials and journey asserts one usable returned credential. |
| `FIND-005-1` | **OPEN / REVISED** — effects are coupled to appended rows, but earlier allowed decisions can remain unaudited. |
| `FIND-003-2` | **CLOSED** — shipped binary init is exercised. |
| `FIND-004-5` | **CLOSED** — operator-only root recovery exists and is journey-tested. |
| `FIND-TASK-001-10` | **WAIVED IN FULL BY USER** — trailers and historical author/committer identities are not reopened. |
| `FIND-admin-principals-R2-2` | **CLOSED** — stored platform kind flows through sessions/context/audit. |
| `FIND-admin-principals-R2-3` | **OPEN / REVISED** — API-key and platform-session attribution are fixed; refresh credential attribution is not. |
| `FIND-admin-principals-R2-4` | **CLOSED** — TenantAdmin refresh rotates, authenticates, and rejects replay. |
| `FIND-admin-principals-R2-5` | **CLOSED** — tenant admission is outside the epoch cache and nonzero-TTL journeys are recorded green twice. |
| `FIND-admin-principals-R2-6` | **CLOSED** — invalid API-key paths use one shared dummy/real verification and one public problem. |

## Suggestions

- `wyrd-auth/src/credential_verify.rs:27-33,61-70` keeps a public, process-wide
  `AtomicU64` counter and increments it on every production authentication solely
  for an in-crate test. Gate the counter/accessor to tests (the only callers are
  in `exchange_api_key.rs`'s test module), so the proof adds no production API or
  hot-path atomic operation.
- `wyrd-server/src/http/openapi.rs:211-214` repeats `name = "Admin"` in one tag
  declaration. Delete the duplicate token when touching the contract source.

## Verification notes

- This reviewer ran no Cargo or `mise` command, as assigned.
- Recorded R2 evidence reports `fmt:check`, lints, client-tier/unwrap checks,
  strict `wyrd-sql` rustdoc, codegen/docs checks, principal unit/integration,
  MCP, CLI, and two consecutive 32/32 platform journeys green.
- The recorded `mise run test:sql` aggregate had two Vala Forge failures that
  passed directly and do not intersect this remediation's write set. They remain
  a repository-green verification limit, not an admin-principals finding.
- A static `git diff --check` on the immutable base-to-candidate range reports
  pre-existing blank-line-at-EOF warnings in review/task Markdown. No source
  finding is based on them.
- The candidate remained at
  `a9a7c9c1e8502ccf3befa74c4283c78e5d70c132` throughout this review.
