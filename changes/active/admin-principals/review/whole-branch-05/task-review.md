# Admin principals whole-branch review 05 — task implementation review

## Immutable subject

- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `c9e1092bbdb4df3781eb91b0eb33150e00df7623`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 10,
  status `approved`
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`
- Prior verdict and validation:
  `changes/active/admin-principals/review/whole-branch-04/{verdict.md,findings-validation.md}`
- Remediation task:
  `changes/active/admin-principals/review/whole-branch-04/TASK-001-008-R4-close-cumulative-findings.md`

The review inspected the complete cumulative base-to-candidate diff, not only
the R4 commits or their completion narrative. The candidate remained pinned at
the commit above while this report was written.

## Overall result

**FAIL**

Most R4 roots are materially closed, but four acceptance failures remain. One
lets authority removed by an OIDC role change survive when the old and new
tokens share an `iat` second; one records every federated platform-human grant
as `global_admin`; and two preserve the duplicate MCP/OpenAPI ownership that
the approved specification and R4 packet explicitly required deleting. These
are bounded implementation corrections under the approved behavior; no
specification revision is required.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `REQ-001`–`REQ-011`, `INV-001`–`INV-003`, `INV-008`–`INV-010`, `INV-012`, `INV-014`: principals and credentials are separate; five kinds, plane-valid tenancy, optional Service binding, principal-generic verifier-only credentials, overlap rotation, indistinguishable rejection | Principal/wire/runtime types, platform and tenant migrations, credential owners, and SQL constraints in the cumulative diff | `test:principals:unit`, `test:principals:integration`, `test:shared`, `test:sql`, and codegen evidence recorded in R4 | PASS |
| `REQ-012`–`REQ-019`, `REQ-031`, `INV-004`, `INV-004a`, `INV-011`: one verified request context, typed plane separation, permission checks, fail-closed auth | `AuthContext`, tenant and platform extractors, verifier and platform authorization services | Shared/principal/platform journey lanes and boundary checks recorded in R4 | PASS |
| `REQ-020`–`REQ-024`, `INV-005`, `AC-001`: explicit one-time initialization, output before commit, failure retryability, no server-start secret | `wyrd-server/src/boot/init.rs`, command dispatch, writer-failure journey | Platform journey twice plus writer-failure cases recorded in R4 | PASS |
| `REQ-025`–`REQ-028`, `INV-006`, `AC-002`, `AC-007`, `AC-008`: provisioning, readiness/admission, retry/concurrency, suspension | Platform provisioning service and lifecycle migrations; every durable-stage failure is parameterized in `platform_admin_e2e.rs` | Two consecutive `test:platform:journey` runs, 35/35 each | PASS |
| `REQ-029`–`REQ-033`, `AC-004`–`AC-006`, `AC-012`: tenant administration, restricted machines, credential lifecycle and recovery | Tenant principal/credential routes and shared client handles; recovery remains a separately authorized platform operation | Principal integration and platform/CLI journey evidence recorded in R4 | PASS |
| `REQ-034`, `REQ-035`, `REQ-041`–`REQ-046`, `AC-011`, `AC-015`–`AC-017`: tenant and platform human identity, preregistration, scope separation, provider independence | Tenant callback and platform identity/session flows | `test:identity:journey` recorded 20/20 | FAIL — the platform federated grant audit records the wrong stored principal kind (`TREV-WB05-2`) |
| `REQ-005`, `INV-013`, `AC-010`, R4-1/R4-2: revoked or unverifiable tenant authority stops on the next request | Cache-hit and cache-miss verifier branches invalidate/refuse on resolver errors; User revocation retires epoch and refresh family in one `TenantConn` | New unit/integration cases and served identity revocation journey recorded in R4 | PASS |
| R4-3: a changed OIDC role set advances the User epoch, invalidates every old token, and admits the reduced successor; unchanged login is stable | Callback replaces roles, writes `date_trunc('second', now())`, then issues a whole-second `iat`; verifier rejects only `iat < epoch` | Unit proof checks that the epoch moved; served journey does not force old issuance and epoch advancement into the same second | FAIL — equality admits a reachable stale token (`TREV-WB05-1`) |
| `REQ-037`, `AC-009`, R4-4/R4-5: canonical same-transaction audit names the real principal, stored kind, credential where applicable, operation, resource, tenant and outcome | Tenant exchange and replay paths append canonically; platform `record_grant` appends in the grant transaction | R4 tests assert row count and credential id, but seed `GlobalAdmin` even for the federated case and never assert `principal_kind` | FAIL — platform-human grants are misclassified (`TREV-WB05-2`) |
| R4-6: failed root disclosure leaves initialization absent and retryable | Fallible writer is flushed before commit | Writer-failure and retry journey recorded in R4 | PASS |
| R4-7 and withdrawn R3-5: unshipped audit compatibility is deleted; current schema remains strict | Legacy fingerprint/evolution/reconciliation paths and additive migration removed; `credential_id` folded into the original migration | Bifrost Redux/SQL/server lanes and `test:sql` recorded green | PASS |
| `REQ-049`, `AC-014`, `AC-019`, R4 `FIND-admin-principals-13`: one exact runtime `utoipa` contract, no parallel route catalog, every served operation and error covered | All current handlers are annotated and listed, `/openapi.json` is served, YAML is absent; however `WyrdApiDoc` keeps a second manual path list and the closure test source-parses method-agnostic paths | `pg_openapi_contract` requests the assembled server, but its path-only comparison can pass when an additional method on an existing path is undocumented | FAIL — prohibited duplicate route knowledge and incomplete exactness proof remain (`TREV-WB05-4`) |
| `REQ-047`, `REQ-048`, `INV-015`, R4-9: every Wyrd-owned caller obtains HTTP/auth behavior from `wyrd-client`; MCP has one shared 401 re-exchange/replay and no independent Wyrd header/raw-client construction | CLI uses `WyrdClient`; MCP receives a `WyrdClient` but clones its raw `reqwest::Client`, declares the header names, renders/inserts bearer headers, classifies 401, and implements the replay loop itself | MCP unit and journey tests prove the duplicate path currently behaves like the shared one | FAIL — the behavior is copied rather than owned by `wyrd-client` (`TREV-WB05-3`) |
| `REQ-036`, `REQ-038`–`REQ-040`, `AC-013`, R4-10/R4-11: public surfaces/docs describe and exercise one model; removed bootstrap identities remain absent; operator uses returned credential through the CLI | Architecture/public docs updated; `bootstrap-key`, fabricated bootstrap Card and `SYSTEM_OPERATOR_ID` absent outside historical change records; operator journey passes returned credential to CLI | Docs, codegen, CLI and platform journey evidence recorded in R4 | PASS |
| R4-12: every cumulative new/materially modified Rust item has required Rustdoc and fallible contracts | R4 documentation commits cover the cumulative touched Rust items inspected for this review | Recorded diff audit, strict `wyrd-sql` rustdoc, and lints are green; no contrary source-local gap found | PASS |
| R4-13 and `VER-002`: diff hygiene and reproducible exact named-test evidence | Packet records literal exact selectors and nonzero results; cumulative files have no whitespace errors | Independent `git diff --check c5c20754..c9e1092b` was silent | PASS |
| Verification constraint `VER-003`–`VER-006`: use the specified focused/broader lanes, not `mise run gate`, and preserve generated/boundary checks | R4 packet records the required lane set and explicitly excludes `gate` | All required lanes are recorded green; their gaps are called out in the four failed rows rather than treating green checks as acceptance | PASS |
| Non-goals: no UI, new RBAC engine, third SQL boundary, audit sink, compatibility route, historical audit shim, Python/TypeScript admin binding, OpenAPI YAML/snapshot/generator, migration/failure-injection framework, or permanent doc scanner | Cumulative diff and final tree preserve these exclusions | Source/diff inspection; boundary and generation evidence recorded in R4 | PASS |
| R4 non-goal: do not add another route catalog, transport, or auth middleware | OpenAPI and MCP changes add no dependency, but retain local route/header/retry ownership | Direct source inspection | FAIL — `TREV-WB05-3` and `TREV-WB05-4` |

## Material proposed findings

### `TREV-WB05-1` — INCORRECT — same-second OIDC role withdrawal leaves the old token authorized

- **Violated obligation:** `REQ-005`, `INV-013`, `AC-010`, R4
  `FIND-admin-principals-R4-3`, and its acceptance requirement that a changed
  role set invalidates the old token no later than the next request.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/callback.rs:219-243`,
  `crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs:87-119`,
  `crates/shared/wyrd-auth-issue/src/lib.rs:519-533`, and
  `crates/shared/wyrd-auth-verify/src/lib.rs:500-563`.
- **Evidence:** the callback writes
  `tokens_not_before = date_trunc('second', now())` and immediately issues the
  successor. Access-token `iat` is also `Utc::now().timestamp()`, in whole
  seconds. Verification rejects only when `iat < epoch`. Therefore an old token
  issued earlier in the same wall-clock second has `iat == epoch` and is
  accepted from both the cache-hit and cache-miss branches after its role was
  removed. The focused test in `callback.rs` asserts only that the epoch is
  second-truncated; it does not present an equal-`iat` stale token.
- **Observable consequence:** a withdrawn administrative role remains usable on
  the next request for a reachable timing window, contrary to the immediate
  authorization-epoch contract.
- **Required testable correction:** make the existing epoch/token issuance
  owner establish an ordering that distinguishes every pre-change token from
  the successor without adding another revocation mechanism. Add a focused
  same-second regression proof in which the old token has the epoch's timestamp:
  the old token must be refused, the reduced successor must be accepted, and an
  unchanged login must leave existing sessions accepted.

### `TREV-WB05-2` — INCORRECT — platform federated grants are audited as `global_admin`

- **Violated obligation:** `REQ-037`, `AC-009`, the security authority's rule
  that audit records the deciding principal's stored kind, and R4
  `FIND-admin-principals-R4-5`'s requirement that federated issuance name the
  resolved principal.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/platform_sessions.rs:195-259`; the covering fixture
  at `crates/wyrd/wyrd-auth/src/platform_sessions.rs:399-410,475-490`.
- **Evidence:** `issue_federated` re-reads a `PlatformPrincipalRow`, which
  contains `principal_kind`, but passes only the id to `record_grant`.
  `record_grant` hard-codes `PrincipalKindTag::GlobalAdmin`. Platform human
  principals are inserted as `PrincipalKindTag::User` by the served identity
  flow. The R4 test seeds `GlobalAdmin` even for its “federated” case and reads
  only `credential_id`, so it cannot detect the wrong kind.
- **Observable consequence:** retained audit history attributes a human
  platform administrator's session grant to the deployment-root kind, making
  principal-kind investigations and authorization attribution false.
- **Required testable correction:** carry the already-read stored platform
  principal kind into the canonical grant event for both credential and
  federated paths; do not infer it from entry path. Prove a pre-registered
  platform `User` federated grant stages exactly one row with kind `user`, no
  credential id, and the real principal id; retain the existing GlobalAdmin
  credential case.

### `TREV-WB05-3` — VIOLATION — MCP still owns a second Wyrd auth/header/retry path

- **Violated obligation:** `REQ-047`, `REQ-048`, R4
  `FIND-admin-principals-R4-9`, and the remediation constraint to leave no
  independent Wyrd header/raw-client construction in `wyrd-mcp`.
- **Exact location:** `crates/wyrd/wyrd-mcp/src/client.rs:31-60,68-175` and the
  five `StreamableHttpClient` method implementations beginning at line 178.
- **Evidence:** the adapter clones a raw `reqwest::Client`, declares
  `HEADER_WYRD_ACCESS_TOKEN`, renders `Bearer ...`, inserts the Wyrd and request
  headers, interprets two `rmcp` error shapes as 401, calls `force_refresh`, and
  implements its own one-replay loop. Receiving the pool and middleware from a
  `WyrdClient` avoids a second cache, but it does not satisfy the explicit rule
  that first-party surfaces neither assemble their auth header nor own status
  classification/retry policy.
- **Observable consequence:** MCP's authentication behavior can diverge from
  the shared HTTP path whenever the canonical header, request correlation,
  refusal classification, or replay policy changes; the new tests certify the
  duplicate implementation rather than preventing that drift.
- **Required testable correction:** keep `rmcp` protocol framing in
  `wyrd-mcp`, but move the Wyrd-specific header decoration and bounded
  authentication-refusal refresh decision to the existing `wyrd-client`
  owner, then have the adapter consume that capability. Delete the MCP-local
  header constants, bearer rendering, and Wyrd 401 policy. Preserve proofs of
  one replay after the first refusal, terminal behavior after the second, MCP
  headers/session semantics, and one shared connection pool.

### `TREV-WB05-4` — VIOLATION — OpenAPI exactness still depends on a parallel route catalog and path-only source parser

- **Violated obligation:** `REQ-049`, `AC-014`, `AC-019`, stable prior
  `FIND-admin-principals-13`, and the R4 prohibition on another route catalog.
- **Exact location:** `crates/wyrd/wyrd-server/src/http/openapi.rs:107-177` and
  `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:70-205`.
- **Evidence:** Axum registrations remain one declaration while
  `WyrdApiDoc` maintains a second manual list of every handler. The new closure
  test then parses Rust source using `.route(`, `.merge(`, function-body, and
  filesystem heuristics. Its own contract says the comparison is
  “method-agnostic”: it compares only path strings. Adding a new served method
  to an existing path therefore leaves `every_mounted_public_route_is_documented`
  green even if that operation is absent from `/openapi.json`. This is the same
  independent route knowledge the remediation required deleting, moved into a
  larger test rather than co-owned with registration.
- **Observable consequence:** the runtime contract can silently omit a served
  operation while the acceptance test passes, so an independent client still
  cannot rely on `/openapi.json` as the exact server contract.
- **Required testable correction:** co-own Axum registration and `utoipa`
  operation registration in the existing route modules, compose the runtime
  document from those owners, and delete the central duplicate handler list and
  source parser. The closure proof must compare served **method + normalized
  path** operations from the same declarations, continue requesting the
  assembled server's `/openapi.json`, and preserve the runtime error/security
  and no-YAML proofs. Do not add a snapshot, generator, or third catalog.

## Prior-finding closure

R4-1, R4-2, R4-4, R4-6 through R4-8, R4-10 through R4-13, and R3-6 are closed
for their named boundaries. R4-3 and R4-5 are only partially closed in the
forms described by `TREV-WB05-1` and `TREV-WB05-2`. R4-9 and stable
`FIND-admin-principals-13` remain open in the narrowed forms described by
`TREV-WB05-3` and `TREV-WB05-4`. The owner-withdrawn historical-upgrade finding
remains withdrawn; no compatibility work is requested.

## Verification limits

The R4 packet records all required focused and broader lanes green, including
two platform journey runs, identity, CLI, MCP, SQL/Bifrost, codegen, docs,
rustdoc, boundary checks, and exact named selectors. This review independently
confirmed cumulative diff hygiene and inspected the source and tests behind
each R4 root. It did not rerun the long environment-owning suites. The green
results are credible for their selections, but the existing tests do not cover
same-second epoch equality or platform-human audit kind and intentionally test
the duplicate MCP/OpenAPI implementations rather than eliminating them.
