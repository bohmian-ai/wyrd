# Task implementation review — admin principals whole branch, round 4

## Review Findings

### Critical

None.

### Important

- **`TREV-R4-1` — `INCORRECT` — revoking a human principal does not revoke its
  refresh authority.**

  - **Violated obligation:** approved `REQ-005`, `INV-013`, `AC-010`, the
    externally observable principal-revocation behavior, and TASK-002/TASK-005's
    requirement that revocation take effect by the next request.
  - **Exact location:**
    [`wyrd-auth/src/revoke.rs:35`](../../../../../crates/wyrd/wyrd-auth/src/revoke.rs),
    [`wyrd-sql/src/queries/auth/revocation.rs:69`](../../../../../crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs),
    and
    [`wyrd-auth/src/refresh.rs:115`](../../../../../crates/wyrd/wyrd-auth/src/refresh.rs).
  - **Evidence:** the served principal-revoke operation reaches
    `revoke_principal_in_conn`. For `User`, that owner only advances
    `auth_users.tokens_not_before`; it neither revokes the user's active refresh
    family nor marks a status that refresh rotation reads. `RefreshTokens::execute`
    subsequently consumes any still-active refresh row, checks only that its stored
    kind is `user`, reloads roles, and calls `issue_and_record_user_session`. That
    owner issues a successor access token at the current time and inserts another
    refresh row. The epoch verifier rejects only access tokens whose `iat` predates
    the epoch, so presenting the still-active refresh token after that epoch mints
    a token that is accepted. The current user-revocation test at
    `wyrd-auth/src/revoke.rs:110-129` proves only that the epoch write returns
    success; it creates no refresh session and does not attempt renewal after
    revocation.
  - **Observable consequence:** an administrator can revoke a human principal and
    see its current access token stop, while a holder of that principal's refresh
    token immediately restores access and receives a new refresh token. The
    promised next-request revocation is therefore reversible by the revoked
    principal.
  - **Required testable correction:** reuse the existing refresh-family revocation
    mechanism in the same tenant transaction as user-principal revocation, so the
    epoch and active family retire atomically. Do not add a second revocation
    service or status cache. Add a real served journey that logs in a human,
    revokes that `User` principal, proves the old access token is refused, proves
    its refresh token cannot mint a successor, and observes no successor refresh
    row after commit.

- **`FIND-admin-principals-13` — `INCORRECT` — the retained OpenAPI test still
  cannot establish exact reachable error coverage, and served operations omit
  reachable codes.**

  - **Violated obligation:** approved `REQ-036`, `REQ-049`, `AC-014`, `AC-019`,
    and the R3 remediation requirement that every administrative operation
    declare every reachable stable `WyrdError` code.
  - **Exact location:**
    [`components/auth/routes.rs:63`](../../../../../crates/wyrd/wyrd-server/src/components/auth/routes.rs),
    [`components/auth/routes.rs:350`](../../../../../crates/wyrd/wyrd-server/src/components/auth/routes.rs),
    and
    [`http/openapi.rs:357`](../../../../../crates/wyrd/wyrd-server/src/http/openapi.rs).
  - **Evidence:** `POST /auth/token` declares only
    `WYRD_AUTH_503_VERIFY_UNAVAILABLE` for status 503. Its API-key, workload,
    delegation, and refresh success paths append canonical audit through
    `append_auth_audit`; an append failure is a reachable
    `WYRD_AUDIT_503_UNAVAILABLE`, including the direct
    `RefreshError::Wyrd` propagation. Conversely, `POST /auth/issue-key`
    declares only `WYRD_AUDIT_503_UNAVAILABLE` for status 503 while tenant
    connection acquisition and commit use `sql_error`, which returns
    `WYRD_AUTH_503_VERIFY_UNAVAILABLE`. The contract test extracts whatever
    codes happen to be present in each response description and checks only that
    those codes exist and have the matching status. It never compares the
    declared set with the operation's reachable set, so both omissions pass.
  - **Observable consequence:** an independent client generated from
    `/openapi.json` cannot enumerate stable failures that the live handlers
    return and cannot implement the exact contract promised by revision 10.
  - **Required testable correction:** keep the existing `utoipa` owner and route
    annotations; add the omitted stable codes to the affected responses and
    audit every other served operation against its reachable error mappings.
    Tighten the existing route-local contract proof so omission of a reachable
    code fails without introducing a second route or error catalog. Exercise an
    injected auth-audit failure and issue-key store failure through the handlers
    and assert the same codes are present in the runtime document.

- **`FIND-admin-principals-R3-5` — `MISSING` — the required retained-audit
  upgrade closure proof was split into fresh-state tests and never exercises an
  upgrade containing history.**

  - **Violated obligation:** the R3 remediation acceptance criterion and closure
    proof require an exact pre-change physical table, catalog row, staging row,
    chain head, and retained row to survive upgrade, publication of both
    credential shapes, restart/replay, and a continuous read with preserved
    field ids.
  - **Exact location:**
    [`bifrost_catalog.rs:2300`](../../../../../crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs),
    [`audit_publication.rs:810`](../../../../../crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs),
    and the required proof in
    [`TASK-001-008-R3-close-validated-findings.md:286`](../whole-branch-03/TASK-001-008-R3-close-validated-findings.md).
  - **Evidence:** `a_pre_credential_audit_log_upgrades_and_keeps_its_field_ids`
    seeds the legacy registration and empty physical schema, then proves field-id
    preservation and idempotent catalog reconciliation. It does not seed a
    retained row, staging row, chain head, or publication/replay state.
    `retained_history_carries_both_credential_shapes` starts a fresh current
    `WyrdTestServer`, appends two current-format rows, and reads them once. It
    does not start from pre-change retained history, execute the catalog upgrade,
    verify an old hash, restart, or replay publication. The hash unit tests cover
    preimage selection in isolation but do not join these boundaries. The three
    green commands recorded as closure evidence therefore do not run the
    required scenario.
  - **Observable consequence:** the candidate has plausible local fixes for the
    catalog schema and hash preimage, but acceptance still has no evidence that
    a real deployment with pre-change audit history can upgrade, publish, restart,
    replay, and read one uninterrupted chain. This persistent-data boundary
    cannot be accepted from fresh-state tests.
  - **Required testable correction:** extend the existing audit publication or
    catalog journey—without a generic migration harness—to seed the exact legacy
    registration and historical state named by the remediation packet, run the
    existing narrow additive upgrade, verify the old hash and field ids, publish
    null/non-null credential rows, restart/replay, and query the continuous
    history. Run that single proof with an exact selector.

- **`FIND-admin-principals-R3-6` — `MISSING` — the appended evidence still does
  not record exact invocations for every named Rust proof.**

  - **Violated obligation:** approved `VER-002` and the R3 packet's Required
    proof section require every specifically named Rust test to be run and
    recorded with package, target, exact `test(=...)` expression, and the
    repository Postgres wrapper where applicable.
  - **Exact location:** implementation evidence at
    [`TASK-001-008-R3-close-validated-findings.md:405`](../whole-branch-03/TASK-001-008-R3-close-validated-findings.md).
  - **Evidence:** several named proofs are reported only as members of another
    run or as bare test names. Examples include the three `wyrd-auth` tests at
    line 405 and two replay tests at line 406, which merely say they passed
    "within the 12-test focused run"; `identity_e2e::human_oidc_login_journey`,
    `platform_admin_e2e::a_tenant_administrator_renews_by_re_exchanging_its_credential`,
    and
    `platform_admin_e2e::a_failed_provisioning_can_be_retried_with_the_same_slug`
    have no recorded exact command. Aggregate `mise` lane counts later in the
    packet do not satisfy the explicit exact-selector contract. The wrong commit
    named for `FIND-005-1` does not reopen that source fix, but further shows the
    evidence table is not a reliable substitute for exact command records.
  - **Observable consequence:** the repository may have run tests that include
    these cases, but the approved verification record cannot demonstrate that
    each named regression proof was selected, ran nonzero, and used its required
    environment.
  - **Required testable correction:** run and append the exact `mise exec -- cargo
    nextest run --locked` command for every named proof currently represented by
    a bare name or aggregate claim, including package, target, exact expression,
    result count, and Postgres wrapper/environment where required. No product
    code or new test harness is needed.

### Suggestions

None. The Ponytail pass found no additional material abstraction or dependency
that can be deleted without leaving the approved behavior incomplete. Each
recommended correction reuses an existing owner.

## Open Questions

None. All four corrections stay within approved revision-10 behavior and require
no new product, API, architecture, security, concurrency, or persistent-data
decision.

## Immutable Review Subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `96a1bd81b1d028fa81d6e0f385e3cd9b62080e30`
- Complete base-to-candidate diff reviewed: 324 changed paths.
- Approved authority: worktree `changes/active/admin-principals/spec.md`,
  revision 10, status `approved`, SHA-256
  `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`.
- Original task authority: all eight immutable packets under
  `changes/active/admin-principals/tasks/`.
- Remediation authority: prior `whole-branch-03` verdict, Wave 1 reports,
  validated ledger, and
  `TASK-001-008-R3-close-validated-findings.md` with its appended evidence.
- The owner-waived `FIND-TASK-001-10` provenance concern remains waived in full.
  The bundled verified-change-contract work remains explicitly approved scope
  and is not drift.

The worktree's pre-existing dirty `README.md`, approved specification, and prior
verdict were treated as owner state and were not modified. Product source was
reviewed at the immutable candidate; no product file was changed.

## Acceptance Matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `REQ-001`–`REQ-004`, `REQ-006`–`REQ-011`; TASK-001; `INV-001`–`INV-003`, `INV-008`–`INV-010`; `AC-005`; principal/credential separation, tenancy, Card-free machines, verifier-only credentials | Principal, API-key, schema, query, and platform-store owners in `wyrd-spec`, `wyrd-sql`, `wyrd-auth`, and server routes | Principal unit/integration lanes and credential journeys recorded green | **PASS** |
| `REQ-005`, `INV-013`, TASK-002/TASK-005 principal revocation, and the revocation-epoch half of `AC-010` | User revocation only advances `tokens_not_before`; refresh rotation ignores that epoch and consumes the surviving row | Existing revocation test does not combine user revocation with refresh; no served journey covers it | **FAIL — `TREV-R4-1`** |
| `REQ-012`–`REQ-019`; closed authenticated context, verified tenant derivation, two planes, role-derived authorization; `INV-004`, `INV-004a` | Typed platform/tenant contexts and extractors, machine/OIDC entry paths, synchronous permission checks, plane-specific persistence | Auth/context, cross-plane, and platform journey evidence recorded green | **PASS** |
| `REQ-020`–`REQ-024`; TASK-003; `INV-005`; `AC-001` initialization and operator-only root recovery | Shipped `wyrd-server init`/`recover-root`, transactional initialization, one-time stdout, stable uninitialized refusal | Real-binary initialization, concurrency, retry, and recovery journeys recorded green | **PASS** |
| `REQ-025`–`REQ-028`; TASK-004; `INV-006`; `AC-002`, `AC-007`, `AC-008` | Resumable tenant provisioning, failed/ready/suspended states, exact-principal retry, CLI trusted-issuer configuration step | Two 33/33 platform journey runs and real CLI journey recorded green | **PASS** |
| `REQ-029`–`REQ-031`; TASK-005 other than user-revocation renewal; `INV-004b`, `INV-007`; `AC-003`, `AC-004`, `AC-012`, `AC-017` | Tenant-scoped principal/grant/credential/configuration routes use `TenantConn`; escalation boundaries remain closed | Principal and cross-tenant journeys recorded green | **PASS** |
| `REQ-032`, `REQ-033`; TASK-006 recovery obligations; `AC-006` | Tenant-admin recovery reuses the existing principal; root recovery remains operator-only | Platform and process recovery journeys recorded green | **PASS** |
| `REQ-034`, `REQ-035`; tenant human identity; `AC-011` | `(tenant, issuer, subject)` resolution and the shared authenticated-context projection | Human OIDC and coexistence journeys recorded green | **PASS** |
| `REQ-041`–`REQ-046`; TASK-007; `AC-015`–`AC-017` | Platform grants, one platform OIDC connection, pre-registration/pinning, platform-only login and escalation boundary | Platform OIDC, trailing-slash issuer, unknown-subject, failure, and scope-separation evidence recorded green | **PASS** |
| `REQ-047`, `INV-015`, TASK-008 transport/header consolidation | CLI/server tools use `wyrd-client`; canonical `X-Wyrd-Access-Token`; no CLI `reqwest::Client` or Wyrd `Authorization` consumer found | Client-tier, CLI, MCP, and focused header evidence recorded green | **PASS** |
| `REQ-048`, `AC-018` machine re-exchange and human refresh rotation/replay | API-key/workload grants carry no refresh; `wyrd-client` re-exchanges; human refresh rotates and replay commits family revocation | Source and recorded refresh/client/journey tests cover the renewal split and replay | **PASS for renewal/replay; principal-revocation interaction fails separately under `TREV-R4-1`** |
| `REQ-036`, `REQ-049`; `AC-013`, `AC-014`, `AC-019`; exact runtime OpenAPI and generated-contract consistency | One runtime `utoipa` `/openapi.json` owner remains and duplicate YAML machinery is deleted, but reachable stable error sets remain incomplete | Six OpenAPI tests pass but the stable-code test cannot detect omissions | **FAIL — `FIND-admin-principals-13`** |
| `REQ-037`, `AC-009`; one canonical transactionally coupled audit path and credential attribution | Current authorization/effect paths use canonical staging; issue-key is coupled; refresh carries consumed-row id; exact platform resources are passed | Focused source/tests cover current writes; upgrade continuity proof is incomplete | **FAIL verification — `FIND-admin-principals-R3-5`** |
| `REQ-038`–`REQ-040`; bootstrap replacement, retired second platform model, operator/SaaS/rotation/recovery docs | Removed bootstrap surfaces; platform tables are owned or removed; docs cover operator and recovery workflows | CLI/docs/codegen evidence recorded green | **PASS** |
| `INV-011`, `INV-012`; fail-closed, non-enumerating and fixed-cost invalid credentials | Stable refusal mapping and shared real/dummy verifier remain in the candidate | Focused invalid-key and tenant-boundary evidence recorded green | **PASS** |
| `INV-014`; server-owned durable authority | Clients project server contracts and do not own durable identity/tenancy behavior | Boundary checks and source inspection | **PASS** |
| Required architecture amendments | Design, security posture, foundation, tenant-OIDC authority, and route documentation use the approved two-plane/principal model | Docs/codegen checks recorded green | **PASS** |
| Material constraints: only `TenantConn`/`OperatorPool`; no compatibility alias; no weakened isolation, audit, or stable-error owner | Provisioning/recovery now receive narrow capabilities; no third connection abstraction or compatibility surface was added | Tenant-isolation and client-tier checks recorded green | **PASS** |
| Non-goals: no new RBAC engine, Card kinds, customer signup UI, billing/tenant deletion/migration, changed `wyrd apply`, or UI refresh state | Complete diff contains no such implementation; approved verified-change bundle is explicitly in scope | N/A (source/diff inspection) | **PASS** |
| R3 closures `FIND-admin-principals-2`, `-3`, `FIND-005-1`, `FIND-004-5`, `R2-3`, `R2-4`, `R3-1`–`R3-4` | Narrow SQL capabilities; static conflicts; atomic issue-key; CLI config/docs; renewal split/replay; exact resources; canonical issuer; secret wrappers; rustdoc | Focused commands or lanes recorded, subject to the exact-command defect below | **PASS implementation** |
| R3 closure `FIND-admin-principals-R3-5` | Narrow legacy fingerprint evolution and legacy-null hash recipe exist | Separate empty-upgrade, hash-unit, and fresh-current publication tests do not run the mandated historical upgrade/restart scenario | **FAIL — `FIND-admin-principals-R3-5`** |
| `VER-001`, `VER-003`–`VER-006`; focused scope, no broad gate, touched crates/contracts only | Recorded proof follows the approved narrow lanes; no broad gate was substituted | Required minimum lanes and additional Bifrost lanes are recorded nonzero/green | **PASS** |
| `VER-002`; exact selection for every named Rust proof | Tests exist, but the evidence table uses aggregate membership and bare names for several named proofs | No exact command record for all named tests | **FAIL — `FIND-admin-principals-R3-6`** |

## Prior-Finding Closure

| Stable prior ID | Round-4 disposition |
|---|---|
| `FIND-admin-principals-1` | **CLOSED.** One canonical staging/publisher path remains; same-plane allowed effects use the audited transaction. |
| `FIND-admin-principals-2` | **CLOSED.** Live provisioning/recovery owners no longer retain `WyrdPostgres`; acquisition remains at the route/state boundary. |
| `FIND-admin-principals-3` | **CLOSED.** Public conflict messages retain stable 409 codes without physical constraint identifiers. |
| `FIND-admin-principals-4` | **CLOSED.** Current authority and implementation consistently use the approved two planes and principal vocabulary. |
| `FIND-admin-principals-8` | **CLOSED.** Last-admin protection counts usable credentials or pinned identities under serialization. |
| `FIND-admin-principals-13` | **OPEN / NARROWED.** Duplicate OpenAPI machinery is gone and typed shapes/prose improved, but exact reachable stable-error coverage remains incomplete. |
| `FIND-004-3` | **CLOSED.** Provisioning retry retires undisclosed credentials and returns one usable replacement. |
| `FIND-005-1` | **CLOSED in current source.** Issue-key allowance, effect, issuance evidence, and commit share one `TenantConn`; failure proof exists. The evidence table's commit attribution is inaccurate but does not change the source result. |
| `FIND-003-2` | **CLOSED.** Initialization and repeat refusal drive the shipped binary. |
| `FIND-004-5` | **CLOSED.** The CLI journey configures a trusted issuer before restricted-principal creation and operator docs include recovery. |
| `FIND-TASK-001-10` | **WAIVED IN FULL by the owner.** Not reopened. |
| `FIND-admin-principals-R2-2` | **CLOSED.** Stored platform kind reaches session, context, and audit. |
| `FIND-admin-principals-R2-3` | **CLOSED for its specified renewal split and attribution.** Machine grants do not refresh; human rotation names the consumed row. `TREV-R4-1` is the separate principal-revocation interaction. |
| `FIND-admin-principals-R2-4` | **CLOSED.** Replay commits family revocation and the successor becomes unusable. |
| `FIND-admin-principals-R2-5` | **CLOSED.** Tenant admission is resolved outside the epoch cache per request. |
| `FIND-admin-principals-R2-6` | **CLOSED.** Invalid tenant keys converge on one real/dummy verification. |
| `FIND-admin-principals-R3-1` | **CLOSED.** Platform operations pass their exact resource; tenant create uses the requested slug. |
| `FIND-admin-principals-R3-2` | **CLOSED.** The canonical parsed issuer is persisted and trailing-slash first login is covered. |
| `FIND-admin-principals-R3-3` | **CLOSED.** Existing redacted secret types own wire and CLI secret handling. |
| `FIND-admin-principals-R3-4` | **CLOSED.** Named fallible and panicking items have the required rustdoc sections. |
| `FIND-admin-principals-R3-5` | **OPEN / REVISED TO MISSING PROOF.** The source contains the narrow upgrade and hash corrections, but the mandated stateful upgrade/restart proof does not exist. |
| `FIND-admin-principals-R3-6` | **OPEN.** The appended evidence still omits exact commands for multiple specifically named Rust proofs. |

## Verification Notes

- This was a review-only static audit. I did not run Cargo, `mise`, migration,
  Postgres, or Bifrost commands; the immutable candidate and recorded command
  evidence were inspected instead.
- Recorded minimum lanes are green: formatting, lints, client-tier, unwrap,
  clippy-allow, tenant isolation, principal unit/integration, two consecutive
  platform journeys, CLI journey, MCP journey, SQL, codegen, examples, docs,
  and strict `wyrd-sql` rustdoc. Additional Redux/server/SQL Bifrost integration
  lanes are also recorded green.
- Those green lanes do not cover `TREV-R4-1`; no current test combines served
  user-principal revocation with a previously issued human refresh token.
- The OpenAPI contract tests prove route presence, auth scheme, media type, and
  validity of codes that are present. They do not prove completeness of each
  operation's reachable code set.
- The candidate HEAD was verified before report creation as
  `96a1bd81b1d028fa81d6e0f385e3cd9b62080e30`. The approved specification hash
  was independently verified as the authority named above.

## Overall Result

**FAIL**

The cumulative candidate does not yet satisfy the approved task exactly. One
reachable revocation bypass is present, the exact runtime OpenAPI contract
remains incomplete, and two explicit remediation/verification closure
obligations remain unproven.
