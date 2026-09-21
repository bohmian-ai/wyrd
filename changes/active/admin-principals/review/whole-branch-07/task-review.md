# Admin principals whole-branch review 07 — task implementation review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Cumulative base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate / reviewed HEAD: `ddd80c7a7b723a2f39a729e5aebe6b6072d2b983`
- Latest remediation range:
  `4d185da9cc2805940786416d0392f4c73bf337c2..ddd80c7a7b723a2f39a729e5aebe6b6072d2b983`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 10,
  status `approved`, SHA-256
  `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`
- Prior verdict and validated ledger:
  `changes/active/admin-principals/review/whole-branch-06/{verdict.md,findings-validation.md}`
- Remediation task:
  `changes/active/admin-principals/review/whole-branch-06/TASK-001-008-R6-close-validated-findings.md`

The complete cumulative base-to-candidate diff, original tasks, R6 packet,
current source, reachable callers, contract tests, and recorded evidence were
inspected. The implementation summary was not accepted as proof. The approved
specification hash matched, `git diff --check` was silent, and HEAD still
matched the candidate immediately before this report was written.

## Overall result

**FAIL**

The R6 implementation closes the stale epoch cache, the named no-effect audit
rollbacks, the MCP UUID schema mismatch, the omitted local route registrations,
and the qualified declaration inventory at their principal code boundaries.
Three acceptance defects remain: service-account credential revocation can
refuse a successor token minted from an unrevoked credential in the same
second; the new local download operation advertises a problem response that its
query extractor bypasses; and the shared revocation contract still explicitly
instructs implementations to use the cache R6 prohibited and removed. No new
product or architecture decision is needed.

## Review findings

### `TREV-WB07-1` — INCORRECT — same-second rotation refuses the surviving credential's successor token

- **Violated obligation:** `REQ-007`, `REQ-010`, `INV-009`, `INV-013`,
  `AC-005`, original TASK-006's overlap-rotation acceptance criterion, and the
  R6 focused proof requiring credential revocation to refuse the predecessor
  while admitting the ordered successor.
- **Exact location:**
  `crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs:192-213`,
  `crates/shared/wyrd-auth-issue/src/lib.rs:491-513`,
  `crates/shared/wyrd-auth-verify/src/lib.rs:501-565`, and
  `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:898-915,1008-1033`.
- **Evidence and reachability:** credential revocation writes a sub-second
  `tokens_not_before = now()`. Tenant access tokens carry a whole-second `iat`,
  and verification rejects when `iat < tokens_not_before`. Consequently a
  still-valid sibling credential exchanged after the commit but within the same
  wall-clock second receives an `iat` below the new epoch and its first
  authenticated request is rejected. The rotation journey asserts only that
  `/auth/token` returned a token; it never spends that token. The replay test
  explicitly documents the same failure and avoids presenting the successor.
  This is not outside the task: R6 lines 146-150 expressly require the ordered
  successor to be admitted, and the original model requires rotation without an
  outage.
- **Observable consequence:** a correct issue-B / revoke-A rotation has a
  reachable sub-second outage even though B was never revoked; an SDK can
  re-exchange B and have the returned token refused immediately.
- **Required testable correction:** reuse the existing explicit token-issuance
  instant/next-whole-second ordering used for same-second user role withdrawal
  so every pre-revocation token is below the committed epoch and a token minted
  afterward from a surviving credential is at or above it. Do not add a sleep,
  cache, listener, blacklist, or credential-specific parallel verifier. Extend
  the production-wired rotation journey to spend B's newly exchanged token on
  the very next request while proving A's already-warm token is refused.

### `TREV-WB07-2` — VIOLATION — the local download route bypasses its advertised problem contract on malformed query input

- **Violated obligation:** `REQ-036`, `REQ-049`, `AC-014`, `AC-019`, stable
  `FIND-admin-principals-13`, and R6's requirement that the co-registered local
  operations publish their reachable problem media and stable errors exactly.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/storage/routes.rs:429-479` and
  `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:164-232`.
- **Evidence and reachability:** the OpenAPI operation requires query parameter
  `path` and declares its `400` as `application/problem+json` carrying
  `WYRD_STORAGE_400_TENANT_PATH_MISMATCH`, but the handler takes
  `Query<LocalDownloadQuery>` directly. A missing or undecodable `path` is
  rejected by Axum before the handler and before `WyrdErrorResponse`; Axum
  0.8.9's `FailedToDeserializeQueryString` returns a plain-text `400`. The new
  contract test only inspects the document and never sends malformed input, so
  it certifies the advertised response without checking the reachable one.
- **Observable consequence:** a generated or independent client following the
  served contract receives neither the promised problem media nor a stable Wyrd
  code for a routine malformed request and cannot handle the error as
  documented.
- **Required testable correction:** map the existing query extractor rejection
  into the canonical `WyrdErrorResponse` path for this operation, preserving the
  current query URL and storage validation; do not add another route, error
  catalog, or parser framework. Add an assembled-server request with a missing
  or malformed `path` and assert the documented status, problem media, and
  stable code alongside the existing served-document assertions.

### `TREV-WB07-3` — VIOLATION — the shared revocation interface still prescribes the deleted stale-cache design

- **Violated obligation:** `FIND-admin-principals-R6-1`'s decision-complete
  prohibition on a replacement authorization-epoch cache and the R6 acceptance
  requirement that obsolete cache machinery be absent.
- **Exact location:**
  `crates/shared/wyrd-auth-verify/src/lib.rs:155-164` and
  `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:3050-3054`.
- **Evidence and reachability:** production now correctly performs the combined
  Postgres admission-and-epoch read on every verification, but the public
  `RevocationCheck` rustdoc still says implementations are expected to use a
  short-TTL in-process cache and that the verifier adds no network IO. The
  suspension journey likewise says the production five-second epoch cache is
  running and calls the admission verdict cached. Both statements are false in
  the candidate and describe precisely the design R6 deleted because it could
  not satisfy next-request revocation.
- **Observable consequence:** the shared implementation contract directs a new
  resolver to recreate the security defect, while the journey's explanation
  falsely states what its passing result proves.
- **Required testable correction:** update the shared interface and journey
  rustdoc to state the actual split: verified tokens remain cached, while the
  revocation implementation performs an authoritative admission-and-epoch read
  on every verification. Add no new behavior or documentation layer; strict
  affected-crate rustdoc and the existing journey are sufficient proof.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK-001 / `REQ-001`–`REQ-011`, `REQ-039`: independent principals, five kinds, valid tenancy, optional Card binding, verifier-only principal-generic credentials | Cumulative runtime/spec types, migrations, SQL constraints, issue/list/revoke owners | Recorded principal unit/integration, shared, SQL and codegen lanes; current source trace | **FAIL only for `REQ-007` through `TREV-WB07-1`; otherwise PASS** |
| TASK-001 constraints: `wyrd-spec` remains foundational; platform and tenant persistence retain `OperatorPool` / `TenantConn`; no second credential model | Cumulative dependency and SQL boundary inspection | Client-tier and tenant-isolation checks recorded green | PASS |
| TASK-002 / `REQ-012`–`REQ-019`, `REQ-031`: one verified request context, typed plane separation, claim-derived tenant, fail-closed authorization | Auth context, tenant/platform extractors, shared verifier and permission owners | Shared, identity, principal and platform journeys recorded green | PASS |
| TASK-002 revocation and audit: changes retire predecessor authority on the next request and each evaluated decision is transactionally audited | Uncached combined admission query; canonical decision append | Warm predecessor, role-withdrawal, suspension and no-effect cases are present | **FAIL only for successor admission through `TREV-WB07-1`; predecessor and audit behavior PASS** |
| TASK-003 / `REQ-020`–`REQ-024`, `AC-001`: one-time initialization, concurrency refusal, failure retryability, no start-time secret | `boot/init.rs` and operator command boundary | Platform journey twice and recorded failure cases | PASS |
| TASK-003 non-goals: no initialization HTTP route, tenant creation, or OIDC behavior in initialization owner | Router and cumulative diff inspection | Source inspection | PASS |
| TASK-004 / `REQ-025`–`REQ-028`, `AC-002`, `AC-007`, `AC-008`: provisioning, admission, retry/concurrency, immediate suspension/resumption | Provisioning service, lifecycle migration, combined per-request admission read | Platform journeys include warm token suspension and restoration | PASS |
| TASK-004 non-goals: no billing/org/signup/deletion product, legacy bootstrap alias, fabricated Card, or synthetic operator | Cumulative diff and final tree | Source/docs inspection | PASS |
| TASK-005 / `REQ-029`–`REQ-031`, `REQ-046`, `AC-004`, `AC-012`: tenant administration, restricted machine principals, RLS isolation, no escalation | Tenant principal routes and `TenantConn` queries | Principal integration, platform/identity journeys, tenant-isolation check | PASS |
| TASK-005 / `REQ-037`, `AC-009`: stable no-effect authorization results retain exactly one decision and no effect; store failures retain neither | R6 commits the existing decision for absent roles/principals/tenants and missing platform connection before logical refusal | `an_authorized_request_that_changes_nothing_still_records_the_decision` and MCP unknown-principal case recorded green | PASS |
| TASK-006 / `AC-005`, `AC-006`: list metadata only, overlap rotation, independent credential revocation, same-principal recovery | Credential routes/client handles and recovery owner | Rotation/replay/recovery journeys present | **FAIL — `TREV-WB07-1` shows overlap rotation is not uninterrupted** |
| TASK-006 non-goals: no credential UI/sharing/delegation redesign or tenant access for global recovery | Final surfaces and diff | Source inspection | PASS |
| TASK-007 / `REQ-034`–`REQ-035`, `REQ-041`–`REQ-046`, `AC-011`, `AC-015`–`AC-017`: platform/tenant humans, pinning, independent scopes, provider independence, redaction | Callback, platform identity/login/session owners | Identity and platform journeys recorded green | PASS |
| Previously closed R4 roots: equal-second OIDC predecessor/successor ordering, stored-kind federated audit, shared MCP transport ownership | Next-second user epoch and explicit successor issuance; stored-kind session audit; shared authenticated replay | Prior exact proofs and current identity/MCP lanes | PASS |
| R6 `FIND-admin-principals-R6-1`: no stale epoch memoization; one authoritative admission-and-epoch round trip; verified-token cache retained; fail closed | `SqlRevocationCheck` is stateless apart from pool; `user_admission` / `service_account_admission` each issue one statement; listener/notify/dependencies removed | Production-wired warm predecessor and lifecycle journeys recorded green | **FAIL only for the stale shared contract in `TREV-WB07-3`; runtime cache removal PASS** |
| R6 `FIND-admin-principals-R5-2`: every named no-effect branch commits its decision without mutation | Platform identity/provisioning and principal routes reuse their decision transactions; roles resolve before principal insert | Real-Postgres aggregate no-effect test and MCP case recorded green | PASS |
| R6 `FIND-admin-principals-R5-5`: MCP schemas and dispatch share typed UUID DTOs | `PrincipalId` / `Uuid` fields are deserialized directly; manual parser removed | MCP journey validates malformed/valid inputs and real outputs with format validation | PASS |
| R6 `FIND-admin-principals-13`: local upload/download are typed, co-registered runtime operations with binary contracts and current guidance | Local operations use `routes!`; query URL producer/client match; both references point to `test:principals:integration` | Served-document test checks operations and declared problem responses; storage/CLI lanes recorded green | **FAIL — reachable malformed download input bypasses the declared contract (`TREV-WB07-2`)** |
| R6 `FIND-admin-principals-R5-1`: no inventoried candidate-added declaration retains qualified type paths | Top-level aliases replace the prior inventory; no contrary site found in the R6 diff | Recorded cumulative scan, format and lints | PASS |
| TASK-008 / `REQ-036`, `REQ-047`–`REQ-049`, `INV-015`, `AC-013`, `AC-014`, `AC-018`, `AC-019`: shared CLI/MCP transport and exact generated contract | Shared client ownership remains; local operations are in runtime OpenAPI | CLI/MCP/client evidence is credible | **FAIL only for `TREV-WB07-2`; other TASK-008 obligations PASS** |
| OpenAPI non-goals: no checked-in snapshot/YAML endpoint/generator/digest/manual catalog or compatibility route | Runtime `utoipa-axum` composition and absent `openapi.yaml` | Codegen and served-contract evidence recorded green | PASS |
| Audit storage/publication, migration immutability, tenant RLS and strict current Bifrost schema | Cumulative SQL/migration/audit owners unchanged by R6 except intended no-effect commits | SQL and all three Bifrost integration lanes recorded green | PASS |
| Global constraints and non-goals: no new RBAC engine, Card kind, audit sink, revocation cache/listener/blacklist, validator layer, second transport, UI, Python/TypeScript admin binding, or permanent scanner | Complete cumulative and R6 diff inventory | Boundary/codegen/docs evidence | PASS |
| Verification scope and evidence precision | R6 records every broader lane and one exact storage selector | The packet names several focused Rust tests only through aggregate lanes despite its lines 164-166 requiring exact nonzero selectors for every named test | **FAIL — verification evidence is incomplete even apart from the source findings** |

## Prior-finding closure

- `FIND-admin-principals-R5-2`, `FIND-admin-principals-R5-5`, and
  `FIND-admin-principals-R5-1` are closed at their validated boundaries.
- `FIND-admin-principals-R6-1` is closed in production runtime behavior but not
  in its shared interface/test contract (`TREV-WB07-3`).
- Stable `FIND-admin-principals-13` is closed for route representability and
  co-registration, but its exact reachable problem contract remains open as
  `TREV-WB07-2`.
- `TREV-WB07-1` is a cumulative TASK-006 acceptance failure exposed by the R6
  ordered-successor requirement; it is not prior behavior that the approved
  task permits preserving.
- Earlier closures, the owner's waiver of `FIND-TASK-001-10`, and the withdrawn
  historical application/Iceberg compatibility proposal are unchanged.

## Verification limits

This review was static and did not rerun the long Docker/Postgres suites. The
recorded broader results cover the correct owners and are credible for their
selections. They do not establish the three failed boundaries: the rotation
tests do not spend the same-second successor token; the OpenAPI test does not
send an invalid local-download query; and no gate can make contradictory
rustdoc accurate. The evidence packet also records only one exact focused
`nextest` selector (`local_download_url_uses_mounted_http_blob_route`) even
though its own focused-proof section names multiple Rust tests and explicitly
requires an exact nonzero selector for each. Those missing focused transcripts
must be supplied after the source corrections; broad lane names alone are not
the required evidence.
