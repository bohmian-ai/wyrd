# Admin principals whole-branch review 03 — structured findings validation

## Immutable subject

| Item | Value |
|---|---|
| Base | `c5c20754a167e8f4d74a555a720bd51df6179a6f` |
| Candidate | `a9a7c9c1e8502ccf3befa74c4283c78e5d70c132` |
| Approved authority | `changes/active/admin-principals/spec.md`, revision 7; TASK-001 through TASK-008 |
| Prior review authority | `whole-branch-01` and `whole-branch-02`, including both remediation packets and the R2 implementation record |
| Wave-1 inputs | `task-review.md`, `standards-review.md`, `domain-review-security.md`, `domain-review-data.md`, `domain-review-contract.md` in this directory |

The candidate remained the exact commit above throughout this validation. This
review wrote only this report. The owner's two explicit decisions are applied
without qualification: all of `FIND-TASK-001-10` is waived, including trailers
and the historical Claude author/committer identities; the bundled
verified-change-contract work is approved cumulative scope and is not drift.

## Validation result

The validated ledger is non-empty. Thirteen material roots remain: seven reopen
or narrow established IDs and six are newly identified candidate defects. All are reachable from
served or upgrade paths except the mandatory Rust-documentation and exact-proof
gaps, which are explicit repository completion rules.

The Ponytail result is deletion/reuse-first throughout: no new audit sink,
credential model, error catalog, transport, schema framework, secret wrapper,
or test harness is justified. Each bounded correction below reuses its current
owner. The retained audit-table upgrade is the only substantial data repair;
the existing Iceberg authority already selects additive evolution, so the
narrow recognized `audit_log` upgrade is preferable to inventing a general
schema-migration framework or changing the approved credential-attribution
contract.

## Wave-1 finding disposition

| Wave-1 proposal | Validation | Final disposition |
|---|---|---|
| `TREV-R3-1` — platform audit resources are generic or wrong | **CONFIRMED** | New `FIND-admin-principals-R3-1`. The final target is not represented reliably. |
| Task/standards `FIND-admin-principals-R2-3` — refresh loses credential attribution | **CONFIRMED / NARROWED** | Existing `FIND-admin-principals-R2-3`. Use the consumed refresh row id; federated sessions remain `None`. |
| Task/standards/contract `FIND-admin-principals-13` — incomplete generated contract | **CONFIRMED / CONSOLIDATED** | Existing `FIND-admin-principals-13`, including stable errors, unconstrained response fields, and stale generated revocation prose. |
| `TREV-R3-2` — exact-test evidence is not the evidence recorded | **CONFIRMED** | New `FIND-admin-principals-R3-6`; aggregate lanes do not satisfy approved `VER-002`. |
| Standards/data `FIND-admin-principals-2` / `DATA-R3-3` — retained `WyrdPostgres` fields | **CONFIRMED** | Existing `FIND-admin-principals-2`. The raw transaction half is closed; the broad dependency-owner half is not. |
| Standards `FIND-admin-principals-1` / `FIND-005-1` — pre-write discovery failures should retain an allowance | **REJECTED** | The governing same-transaction rule requires an allowance and effect to commit or roll back together. `audit/mod.rs:250-258` and the accepted failure journey state the same contract. Committing an allowance for an operation that never happened would recreate the prior defect. |
| Standards/data `RS-R3-1` / `DATA-R3-2` — old audit hashes cannot be reproduced | **CONFIRMED / CONSOLIDATED** | New `FIND-admin-principals-R3-5`, together with the same upgrade's physical/catalog schema failure. |
| Data `DATA-R3-1` — an existing retained audit table rejects the changed fingerprint | **CONFIRMED / CONSOLIDATED** | New `FIND-admin-principals-R3-5`. |
| Standards `RS-R3-2` — mandatory Rust documentation is incomplete | **CONFIRMED** | New `FIND-admin-principals-R3-4`. |
| Contract `CONTRACT-R3-2` — `/auth/issue-key` commits authorization separately | **CONFIRMED** | Existing `FIND-005-1`. |
| Contract `CONTRACT-R3-3` — tenant issuer secrets use plain `String` and derived `Debug` | **CONFIRMED** | New `FIND-admin-principals-R3-3`. |
| Contract `CONTRACT-R3-4` — CLI/operator workflow omissions | **REVISED** | Existing `FIND-004-5` only for the required tenant-configuration step and stale recovery synopsis. The proposal to add every platform-human identity operation to the CLI is rejected: TASK-007 requires the HTTP contract and journey, while revision-7 TASK-008 explicitly accepts narrower CLI/MCP administrative subsets. |
| Security `SEC-R3-1` — refresh reuse rolls back family revocation | **CONFIRMED** | Existing `FIND-admin-principals-R2-4`, reopened for the still-broken replay containment promised by its proof. |
| Security `SEC-R3-2` — platform OIDC persists a noncanonical issuer | **CONFIRMED** | New `FIND-admin-principals-R3-2`. |
| Security `SEC-R3-3` — constraint names remain public | **CONFIRMED** | Existing `FIND-admin-principals-3`, narrowed to the two explicit conflict mappers. |

## Final deduplicated finding ledger

### FIND-admin-principals-2 — CONFIRMED — VIOLATION — SQL capability ownership remains broader than allowed

- **Sources:** standards review, `DATA-R3-3`.
- **Obligation:** `architecture/agent-rules.md:6` permits only
  `TenantConn` and `OperatorPool` in library function signatures and struct
  fields; the R2 packet explicitly required the broad owners to be removed.
- **Exact evidence:** `components/platform/provisioning.rs:93-116` stores and
  accepts `WyrdPostgres`; `components/platform/recovery.rs:36-58` does the same.
  Live construction is at `components/platform/routes.rs:141,185,335`, and both
  owners use the handle to open tenant transactions.
- **Reachability/consequence:** these are the served provisioning and recovery
  workflows, not construction-only or test seams. Each keeps authority to open
  arbitrary tenant transactions beyond the two reviewed capabilities, so the
  type boundary claimed by the rule and R2 acceptance evidence is false.
- **Minimum correction:** keep pool construction and tenant acquisition at the
  existing `ServerPostgres`/route composition boundary. Pass only the acquired
  `TenantConn` into tenant work and keep platform work on `OperatorPool`; do not
  add a third pool wrapper, raw pool, provider trait, or closure abstraction.
  Extend the existing tenant-boundary source check to these two fields and
  constructors.
- **Closure proof:** the source check rejects `WyrdPostgres`, raw pool, and raw
  transaction propagation in these owners, and the focused provisioning and
  recovery journeys remain green.

### FIND-admin-principals-3 — REOPENED / NARROWED — VIOLATION — conflict responses still expose physical constraint names

- **Source:** `SEC-R3-3`.
- **Obligation:** public errors use stable safe messages while source diagnostics
  remain server-side; `INV-002` excludes internal/secret material from errors.
- **Exact evidence:** `components/admin/routes.rs:699-705` and `:721-734`
  interpolate `SqlError::{UniqueViolation,FkViolation}.constraint` into both the
  public message and problem details. Both helpers serve authenticated
  `/v1/admin/*` create/delete conflict paths.
- **Consequence:** a duplicate issuer/binding or referenced issuer delete
  reveals PostgreSQL schema identifiers and makes the public contract change
  when a physical constraint is renamed.
- **Minimum correction:** retain the useful existing `409` variants, but return
  operation-specific static messages/details and trace the constraint only on
  the server. Reuse the current mappers; add no error variant or mapper layer.
- **Closure proof:** duplicate and referenced-delete route tests inject known
  physical names and prove neither response contains them while preserving the
  stable `WYRD_AUTH_409_ADMIN_CONFLICT` code.

### FIND-admin-principals-13 — REOPENED / REVISED — INCORRECT — generated administration contract is still incomplete

- **Sources:** task, standards, and `CONTRACT-R3-1`.
- **Obligation:** REQ-036 and AC-014 require every administrative path to expose
  typed bodies, reachable stable errors, problem media, and authentication in
  generated OpenAPI.
- **Exact evidence:**
  - `http/openapi.rs:314-365` checks stable codes only for `Auth` and `Admin`,
    explicitly skipping `Platform` and `Principals`, and accepts any one
    `_<status>_` substring rather than the reachable code set.
  - `/auth/token` at `components/auth/routes.rs:63-75` declares only
    `WYRD_AUTH_401_API_KEY_INVALID` for 401 and no 400/404, although the served
    match returns `WYRD_AUTH_401_REFRESH_REUSED`,
    `WYRD_AUTH_401_REFRESH_REVOKED`,
    `WYRD_AUTH_400_DELEGATION_DEPTH_EXCEEDED`, and
    `WYRD_AUTH_404_PRINCIPAL_NOT_FOUND` (`wyrd-spec/src/error.rs:599-635,739-751`).
  - `wyrd-spec/src/auth/admin.rs:100-121,143-152` exposes response-side
    `serde_json::Value` fields despite existing concrete `ClaimMappingPayload`,
    map/vector, and `CardRef` types; generated clients receive no usable shape.
  - `auth/revoke.rs:19-24` says the allowance commits before revocation, while
    lines `70-93` now append, mutate, and commit together; that false prose is
    copied into `openapi.yaml`.
- **Consequence:** an independent client cannot type the issuer/binding
  responses or enumerate actual administrative error branches, and the
  generated revocation contract states the opposite durability behavior.
- **Minimum correction:** keep the existing DTO and `utoipa` owners. Replace the
  response `Value`s with the already-existing concrete types, correct the
  revocation rustdoc, and enumerate every reachable stable code in each
  operation's existing response metadata across all four administrative tags.
  Strengthen the one current generator test to compare exact per-operation code
  sets and problem media; do not create a second error catalog or checker.
- **Closure proof:** regenerated OpenAPI carries constrained schemas, correct
  revocation prose, authentication, `application/problem+json`, and exact
  stable-code sets for representative multi-grant and Platform/Principals
  operations; `codegen:check` remains clean.

### FIND-005-1 — REOPENED / NARROWED — VIOLATION — API-key issuance authorization and effect use different transactions

- **Source:** `CONTRACT-R3-2`.
- **Obligation:** REQ-037, AC-009, and `agent-rules.md:14` require an allowed
  same-plane decision and its effect to commit or roll back together.
- **Exact evidence:** `components/auth/routes.rs:365-378` authorizes and commits
  the allowance through standalone `record_audit`; only afterward do lines
  `379-393` acquire a `TenantConn`, issue the key, append issuance evidence, and
  commit. `auth_router` serves this as `POST /auth/issue-key`.
- **Consequence:** connection, issuance, issuance-audit, or commit failure leaves
  a durable allowance for a key that was never issued.
- **Minimum correction:** use the existing `TenantConn` plus `audit::append_on`
  pattern already used by principal writes: acquire once, append the returned
  allowance, issue/audit, and commit once. Preserve independently durable
  denials. Add no audit abstraction or second event.
- **Closure proof:** inject failure after authorization but before key issuance
  commit and prove neither a key nor its allowed decision remains; preserve a
  durable denied decision.

### FIND-004-5 — REOPENED / NARROWED — MISSING — the required operator configuration journey is still absent

- **Source:** revised `CONTRACT-R3-4`.
- **Obligation:** AC-002 requires the real-server SDK/CLI journey to create a
  tenant, configure it, and create a restricted machine principal; TASK-004
  requires the matching executable self-hosting journey.
- **Exact evidence:** `wyrd-cli/tests/operator_journey.rs:63-117` creates and
  inspects a tenant, then jumps directly to principal creation. It never invokes
  the existing `auth trusted-issuer` or workload-binding tenant-configuration
  command. `docs/.../running-the-server.svx:81-110` likewise skips
  configuration. Lines `11,15-25` say only `init` exists and omit the already
  shipped `recover-root` synopsis, contradicting lines `44,112-154`.
- **Consequence:** the green CLI lane does not prove the approved three-command
  path and the operator page does not provide one internally consistent process.
- **Minimum correction:** extend the existing operator journey and existing page
  to configure tenant OIDC through the already-shipped CLI before principal
  creation, and add `recover-root` to the current command synopsis. Do not add
  platform-identity CLI commands, another page, transport, or client owner.
- **Closure proof:** the real-server CLI journey completes tenant create ->
  tenant OIDC configuration -> restricted principal creation and use, and
  `docs:check` passes.

### FIND-admin-principals-R2-3 — REOPENED / REVISED — INCORRECT — refresh-authenticated decisions erase their credential

- **Sources:** task/standards refresh finding.
- **Obligation:** REQ-037 and AC-009 require the non-secret credential that
  authenticated each covered decision; a refresh token is a stored one-way
  credential, unlike a federated session.
- **Exact evidence:** `wyrd-auth/src/refresh.rs:118-167` has the consumed durable
  row as `active.id`, but the rotation audit passes `None` at line `158` and
  `rotate_stored_principal` mints the successor access token with
  `credential_id: None` at lines `261-274`. The active security-posture sentence
  blessing that omission conflicts with the still-approved REQ-037 and cannot
  narrow it.
- **Consequence:** the refresh rotation itself and every privileged call made by
  its successor token are indistinguishable from credential-free federation.
- **Minimum correction:** reuse `active.id` as the credential id in the rotation
  event and successor token context. Preserve `None` for actual federated and
  delegated sessions; add no second identifier or detail field.
- **Closure proof:** a real TenantAdmin refresh produces a protected decision
  naming the consumed refresh-row id, while a federated decision remains null.

### FIND-admin-principals-R2-4 — REOPENED / REVISED — INCORRECT — refresh replay reports revocation but rolls it back

- **Source:** `SEC-R3-1`.
- **Obligation:** `wyrd-security-posture.md:124-126` requires reuse of a rotated
  token to revoke its family and durably audit that response; the R2 proof
  promised replay containment for the newly supported TenantAdmin path.
- **Exact evidence:** `wyrd-auth/src/refresh.rs:179-218` revokes the family and
  appends `RefreshFamilyRevocation` on the caller's transaction, then returns
  `Err(RefreshError::Reused)`. `components/auth/routes.rs:209-243` commits only
  `Ok`; every error returns at lines `228-241`, dropping and rolling back both
  writes. Existing unit tests inspect the still-open transaction, and
  `platform_admin_e2e.rs:3744+` checks only the replay response.
- **Consequence:** after a stolen token wins rotation, replay of the original
  tells the legitimate holder it was detected while leaving the attacker's
  successor refresh token valid.
- **Minimum correction:** special-case the existing typed `Reused` outcome at
  the route boundary: commit its already-staged family revocation and canonical
  audit before rendering `WYRD_AUTH_401_REFRESH_REUSED`; all other errors still
  roll back and use the existing failure path. Do not add a refresh service or
  second audit event.
- **Closure proof:** through `/auth/token`, rotate once, replay the original,
  then prove the successor refresh token is refused and exactly one committed
  family-revocation event is visible from a separate transaction.

### FIND-admin-principals-R3-1 — CONFIRMED — INCORRECT — platform authorization records the wrong or no target resource

- **Source:** `TREV-R3-1`.
- **Obligation:** REQ-037 and AC-009 require every decision to name the resource
  acted upon.
- **Exact evidence:** `platform_authz.rs:104-110,152-172` accepts only an
  optional tenant and otherwise writes literal `platform`. The shared identity
  helper always passes `None` (`components/platform/identity.rs:118-126`), so
  connection, principal, status, credential issue, and credential revoke
  decisions do not identify their targets. Provisioning creates a fresh id and
  audits it at `provisioning.rs:139-150`, but `insert_provisioning_tenant` may
  return a resumed tenant and replace that id only at lines `156-165`; the
  committed audit row then names a discarded tenant.
- **Consequence:** retained evidence cannot answer which connection, principal,
  or credential was acted on, and a provisioning retry falsely attributes the
  allowance to a tenant that was never used.
- **Minimum correction:** replace the tenant-only parameter with the exact
  resource string already known by each caller. Use stable typed target
  spellings for connection/principal/credential/status operations and the
  requested tenant slug for create (known before the claim and truthful for
  both fresh and resumed attempts). Keep `target_tenant_id` separate only where
  a final existing tenant is already known. Do not mutate an appended hash-chain
  row or add resource metadata elsewhere.
- **Closure proof:** assert exact resources for identity and credential paths,
  fresh provisioning, and resumed provisioning; the resumed record must never
  contain the discarded proposal id.

### FIND-admin-principals-R3-2 — CONFIRMED — INCORRECT — platform OIDC stores a noncanonical issuer

- **Source:** `SEC-R3-2`.
- **Obligation:** REQ-043/044 require a configured connection and preregistered
  identity to form a working first-login path; `IssuerUrl` defines the canonical
  equality form.
- **Exact evidence:** `components/platform/identity.rs:206-215` parses and
  normalizes the input, but lines `229-246` persist and return the raw
  `request.issuer_url`. Registration copies that raw row at lines `388-414`.
  `pg_resolvers.rs:460-480` reparses the connection to a normalized `IssuerUrl`,
  and `platform_login.rs:277-304` queries/pins with that normalized string.
- **Consequence:** an accepted issuer such as `https://idp.example/` stores the
  identity with a slash and searches without it, so first login always refuses
  the preregistered administrator.
- **Minimum correction:** persist and return the already-parsed
  `issuer.as_str()` and use that one value for both connection and identity
  writes. Add no second normalizer.
- **Closure proof:** a served journey configures a trailing-slash issuer,
  preregisters the administrator, and completes first-login pinning.

### FIND-admin-principals-R3-3 — CONFIRMED — VIOLATION — tenant issuer secrets are debug-visible plain strings

- **Source:** `CONTRACT-R3-3`.
- **Obligation:** AGENTS.md section 4 requires `SecretString` for secrets and a
  redacted `Debug`; INV-002 excludes raw credential material from diagnostics.
- **Exact evidence:** `wyrd-spec/src/auth/admin.rs:65-80` derives `Debug` while
  holding `client_secret: Option<String>`. The shipped CLI repeats it in
  `wyrd-cli/src/auth/trusted_issuer.rs:28-50`; its resolver returns another
  `Option<String>` at lines `252-278`. `wyrd-spec/src/auth/secret_bearer.rs:9-95`
  already owns the string-compatible, write-only, redacted wire type.
- **Consequence:** formatting either public request or parsed CLI arguments can
  expose a provider secret, and generated schema fails to mark the field as a
  write-only password.
- **Minimum correction:** reuse `SecretBearer` on the wire and `SecretString`
  after CLI resolution; give the CLI argument holder redacted debug behavior.
  Do not add a secret wrapper.
- **Closure proof:** request and CLI debug tests exclude a sentinel secret, the
  JSON wire remains a string, and OpenAPI marks it write-only/password.

### FIND-admin-principals-R3-4 — CONFIRMED — VIOLATION — new fallible and panicking items omit mandatory contracts

- **Source:** `RS-R3-2`.
- **Obligation:** `architecture/agent-rules.md:35` and AGENTS.md section 16 make
  complete rustdoc, including `# Errors` and `# Panics`, a hard merge criterion
  for new/materially changed Rust items, including private tests.
- **Exact evidence:** `wyrd-server/src/main.rs:90-105` adds fallible
  `recover_root` without `# Errors`. New helpers in
  `platform_admin_e2e.rs:172-187,3455-3503` panic through `expect` but
  `operator_command`, `fail_writes_to`, `stop_failing_writes`, and
  `staged_platform_decisions` omit `# Panics`.
- **Consequence:** the candidate does not satisfy the repository's explicit
  Rust completion gate and hides setup/process failure behavior from callers.
- **Minimum correction:** complete the existing rustdoc with the actual errors
  and panics. No helper, abstraction, lint suppression, or test is needed.
- **Closure proof:** source review plus the existing lint/doc checks.

### FIND-admin-principals-R3-5 — CONFIRMED — REGRESSION — credential attribution breaks retained-audit upgrades

- **Sources:** `DATA-R3-1`, `DATA-R3-2`, `RS-R3-1`.
- **Obligation:** R2's credential-attribution correction must reach the one
  retained audit table without breaking existing history; Iceberg guidance
  requires explicit compatible additive evolution and preserved field identity.
- **Exact evidence:**
  - Base `AuditLogTable::arrow_fields` has 13 content columns; current
    `audit_log.rs:44-60` inserts nullable `credential_id`, changing the registered
    user-schema fingerprint.
  - Every publication calls `ensure_builtin`
    (`scribe/ingress.rs:183-195`), which computes the new fingerprint
    (`catalog/bifrost_catalog.rs:901-923,956-963`) and rejects an existing old
    registration at lines `977-980`. Migration 27 changes only Postgres staging;
    it does not evolve the tenant catalog or Iceberg table.
  - Base `audit_staging::entry_hash` places `permission` immediately after
    `principal_kind`; current `audit_staging.rs:357-373` always inserts
    `push_opt(credential_id)`. For every migrated null row this adds a zero byte,
    although migration `20260910000027_audit_credential_id.sql:11-14` claims old
    entries remain reproducible.
- **Consequence:** an upgraded deployment rejects its first retained audit
  publication and staging accumulates. Independently, recomputing any historical
  null-credential row with current code yields a different digest, making valid
  history appear tampered.
- **Minimum correction:** keep the one table and publisher. In the existing
  catalog owner, recognize only the exact prior `audit_log` fingerprint and
  perform one idempotent additive upgrade: add nullable `credential_id` to the
  Iceberg schema while preserving all old field IDs, then advance the control
  fingerprint; a retry must reconcile “physical evolved/control old” without
  widening arbitrary built-in evolution. For hashing, preserve the legacy
  preimage whenever `credential_id` is absent and append the credential segment
  only when present; this avoids a hash-version column and keeps both old and new
  null rows reproducible. Do not rewrite retained history or bypass generic
  fingerprint conflicts.
- **Closure proof:** seed a pre-change registered physical `audit_log`, catalog
  row, staging row, chain head, and retained row; upgrade; verify the historical
  hash, publish null and non-null credential rows, restart/replay, and read the
  uninterrupted history with preserved old field IDs.

### FIND-admin-principals-R3-6 — CONFIRMED — MISSING — approved exact-test evidence was not recorded

- **Source:** `TREV-R3-2`.
- **Obligation:** approved `VER-002` requires every named Rust proof to run via
  exact package/target/`test(=...)` selection under repository setup.
- **Exact evidence:** the R2 record names nonexistent selectors
  `a_failed_mutation_discards_its_own_allowance` and
  `a_decision_records_the_kind_it_was_made_by`; current source names
  `a_failed_platform_mutation_leaves_no_allowance`
  (`platform_admin_e2e.rs:3520`) and
  `platform_authz::pg_tests::a_decision_records_the_stored_principal_kind`
  (`platform_authz.rs:356`). Refresh, tenant-admission cache, and verifier-cost
  claims are recorded only through aggregate lanes.
- **Consequence:** the implementation record does not satisfy the approved
  proof contract even though the aggregate lanes ran nonzero tests.
- **Minimum correction:** run and record the existing exact tests—no code or new
  harness—including the two corrected names,
  `a_tenant_administrator_refreshes_and_cannot_replay`,
  `an_operator_suspends_and_resumes_a_tenant_through_the_platform_plane`, and
  `exchange_api_key::pg_tests::every_invalid_api_key_costs_exactly_one_verification`.
- **Closure proof:** successful exact `mise exec -- cargo nextest run --locked`
  commands with package, target, exact expression, and repository Postgres
  wrapper where required.

## Prior whole-branch-02 finding closure

| Prior stable ID | Independent disposition at candidate |
|---|---|
| `FIND-admin-principals-1` | **CLOSED.** The alternate table is gone and inspected platform same-plane effects use the returned audited transaction. The Wave-1 proposal to commit allowances for failed pre-write work is rejected. |
| `FIND-admin-principals-2` | **OPEN / NARROWED.** Raw SQLx transactions are confined; the two live `WyrdPostgres` owner fields remain. |
| `FIND-admin-principals-3` | **OPEN / NARROWED.** Generic source failures are redacted; explicit physical constraint names remain public. |
| `FIND-admin-principals-4` | **CLOSED.** Current design and security authorities describe the two planes, closed kinds, Card-free administration, and canonical audit. |
| `FIND-admin-principals-8` | **CLOSED.** `USABLE_ADMINISTRATORS_SQL` requires the fixed grant plus a live credential or pinned identity on the current connection under the transaction advisory lock (`queries/platform/principals.rs:200-315`). |
| `FIND-admin-principals-13` | **OPEN / REVISED.** Paths, auth scheme, and media type landed; typed response shapes, complete stable codes, and one generated operation description remain wrong. |
| `FIND-004-3` | **CLOSED.** A resumed provisioning attempt reuses the principal and retires its pre-existing credentials before issuing the sole disclosed replacement (`provisioning.rs:373-447`). |
| `FIND-005-1` | **OPEN / NARROWED.** Issuer, binding, and principal revoke are coupled; `/auth/issue-key` retains the same separate-commit defect. |
| `FIND-003-2` | **CLOSED.** The real `wyrd-server init` process and repeat refusal are exercised. |
| `FIND-004-5` | **OPEN / NARROWED.** `recover-root` is shipped and process-tested; the required tenant-configuration CLI step and consistent command synopsis are not. |
| `FIND-TASK-001-10` | **WAIVED IN FULL.** It is not reopened and requires no history change. |
| `FIND-admin-principals-R2-2` | **CLOSED.** Stored platform principal kind reaches the session/context and canonical event (`platform_authz.rs:165-176`). |
| `FIND-admin-principals-R2-3` | **OPEN / REVISED.** Fresh API-key decisions carry the id, but refresh drops it. The separate upgrade regression is `FIND-admin-principals-R3-5`. |
| `FIND-admin-principals-R2-4` | **OPEN / REVISED.** TenantAdmin rotates and replay returns 401, but the required family revocation and audit roll back. |
| `FIND-admin-principals-R2-5` | **CLOSED.** Tenant admission is resolved per request outside the principal epoch cache, and the nonzero-TTL suspension/resume journey exists. |
| `FIND-admin-principals-R2-6` | **CLOSED.** Invalid tenant keys converge on the shared real/dummy verifier with one-call instrumentation. |

Whole-branch-01 findings not carried into the sixteen-root R2 remediation remain
closed unless explicitly reopened above. In particular, platform credential
surface, SSRF pinning, MCP catalog/action proof, provisioning retry, root
recovery, principal-kind attribution, architecture alignment, and the original
source-error paths were rechecked through their current owners rather than
accepted from the implementation summary.

## Validated exclusions and verification context

- The base-reproduced `auth_e2e::cache_ttl_path_also_flips_verdict` failure is
  not a candidate regression. The same-named Card constraint and stale
  `wyrd-testing` comment remain spec-owner handoffs. Widening a permanent
  rustdoc lane remains a separate decision. None is in this ledger.
- The approved verified-change-contract commits are not drift. Blank-line EOF
  notices in their task/review Markdown do not become findings.
- Fresh orchestrator evidence is green for `fmt:check`, client-tier,
  unwrap/clippy-allow/tenant-isolation checks, workspace lints, strict
  `wyrd-sql` rustdoc, principal unit/integration, two consecutive 32/32 platform
  journeys, MCP journey, CLI journey, detached-worktree codegen/docs checks, and
  `test:sql` (122 wyrd-sql, 4 fixtures, 113 vala-sql, 2 storage). These results
  establish nonzero execution but do not disprove the source paths above, the
  upgrade path they never construct, or the exact-selector requirement.
- No broad gate was required or treated as evidence; approved VER-003 excludes
  it. The fresh scoped runs do not establish that every test in the entire
  repository is green.
