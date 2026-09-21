# Admin principals whole-branch review 07 — structured Ponytail findings validation

## Immutable subject

| Item | Value |
|---|---|
| Repository | `/home/thorrester/Documents/GitHub/wyrd` |
| Cumulative base | `c5c20754a167e8f4d74a555a720bd51df6179a6f` |
| Candidate / reviewed HEAD | `ddd80c7a7b723a2f39a729e5aebe6b6072d2b983` |
| Latest remediation range | `4d185da9cc2805940786416d0392f4c73bf337c2..ddd80c7a7b723a2f39a729e5aebe6b6072d2b983` |
| Approved authority | `changes/active/admin-principals/spec.md`, revision 10, status `approved` |
| Approved spec SHA-256 | `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837` |
| Original task authority | `TASK-001` through `TASK-008` under `changes/active/admin-principals/tasks/` |
| Prior review/remediation | Whole-branch-06 verdict, validation, and `TASK-001-008-R6-close-validated-findings.md` |
| Wave-1 inputs | `task-review.md`, `standards-review.md`, `domain-review-security.md`, `domain-review-data.md`, and `domain-review-contract.md` in this directory |

The candidate and approved-spec checksum matched the values above before this
report was written. CodeGraph was used before direct source inspection. The
complete cumulative diff, the R6 remediation diff, all five complete Wave-1
reports, applicable authorities, current function bodies, and callers behind
every proposal were inspected. This validation changes only this report.

## Validation outcome

**FIX_REQUIRED.** Eight material roots survive independent validation. Three
preserve earlier stable IDs in revised form:
`FIND-admin-principals-13`, `FIND-admin-principals-R5-2`, and
`FIND-admin-principals-R6-1`. Five are new:
`FIND-admin-principals-R7-1` through `FIND-admin-principals-R7-5`.

The minimum corrections reuse the current combined admission read,
`WyrdPostgres::tenant_conn`, Postgres RLS, existing authorization transaction,
Axum's fallible extractors, the canonical `WyrdErrorResponse`, existing token
issuance-at-an-instant behavior, and the existing real-server suites. No cache,
listener, blacklist, middleware layer, parallel error type, pool wrapper,
tenant predicate, audit writer, or test harness is justified.

**SPEC_REVISION_REQUIRED: none.** Revision 10 already requires uninterrupted
overlap rotation, status-gated authentication, next-request revocation,
transactional audit, exact public error contracts, RLS as the tenant boundary,
and exact focused proof. The corrections below make the implementation and its
evidence satisfy those decisions; none introduces a new product, public API,
security, concurrency, ownership, compatibility, or persistent-data decision.

## Wave-1 proposal disposition

| Wave-1 proposal | Validation | Disposition |
|---|---|---|
| `TREV-WB07-1`, `REPO-R7-1`, `SEC-R7-2`, `DATA-R7-1` — same-second successor refusal | **CONFIRMED / CONSOLIDATED** | New `FIND-admin-principals-R7-1`. Whole-second JWT `iat` is strictly below the sub-second epoch even when the surviving credential exchanges after commit. |
| `SEC-R7-1` — inactive or missing tenant principal remains admitted | **CONFIRMED** | New `FIND-admin-principals-R7-2`. The one authoritative read returns no status/admission bit, so Card deletion is invisible to warm and fresh token verification. |
| `TREV-WB07-2`, `REPO-R7-2`, `CONTRACT-07-01` — local download extractor bypasses problem JSON | **REVISED / CONSOLIDATED** | Stable `FIND-admin-principals-13`. Download is confirmed; the upload path extractor has the same reachable pre-handler rejection class and belongs in the same exact-contract correction. |
| `TREV-WB07-3` — shared revocation contract still prescribes a cache | **CONFIRMED** | Stable `FIND-admin-principals-R6-1`. Runtime memoization is gone, but public trait and journey prose still direct readers to or claim the deleted design. |
| `SEC-R7-3` — principal-revoke misses roll back audit and SQL failures become not-found | **CONFIRMED / CONSOLIDATED** | Stable `FIND-admin-principals-R5-2`. This is another reachable stable no-effect branch of the same previously retained audit root; `.ok().flatten()` is the separate fail-closed half. |
| `REPO-R7-3` — revocation owner stores raw `PgPool` | **CONFIRMED** | New `FIND-admin-principals-R7-3`. The R6 rewrite materially changed this owner but retained the connection shape explicitly banned by repository authority. |
| `REPO-R7-4` — combined admission queries add manual tenant predicates | **CONFIRMED** | New `FIND-admin-principals-R7-4`. Both predicates were added on `TenantConn` paths despite forced RLS and explicit `AC-010`. |
| Task/contract verification note — named Rust tests lack exact selector records | **CONFIRMED / CONSOLIDATED** | New `FIND-admin-principals-R7-5`. The aggregate lanes are credible but do not satisfy the packet's explicit exact-nonzero-selector requirement for every named test. |
| Data-domain report beyond `DATA-R7-1` | **VALIDATED EMPTY** | Tenant transaction ownership, migration durability, audit publication, and concurrency boundaries have no additional retained finding. |

### Rejected or narrowed alternatives

- **REJECTED — sleep until the next second or weaken predecessor revocation.**
  Waiting adds avoidable latency and flakiness; keeping a current-second epoch
  leaves same-second predecessors live. The existing ordered user-token path
  already proves that an epoch and successor can share one explicit instant.
- **REJECTED — another status query, cache, listener, or blacklist.** The
  authoritative admission statement already runs on every verification and can
  return principal admission together with tenant admission and epoch.
- **REJECTED — global rejection middleware or hand-parsing query strings.**
  Axum already permits a route to receive extractor rejection as `Result`; the
  existing Wyrd mapper owns the response.
- **REJECTED — retain a raw pool because it predates R6.** The resolver was
  materially rewritten, its constructor and every assembly caller changed, and
  `architecture/agent-rules.md` permits only `TenantConn` and `OperatorPool` in
  library fields/signatures. `WyrdPostgres::tenant_conn` is the existing owner.
- **REJECTED — keep explicit tenant filters as defense in depth.** Repository
  authority and `AC-010` deliberately make RLS the sole tenant predicate on a
  `TenantConn` path; duplicate scope is drift, not defense.
- **REJECTED — treat aggregate lane names as exact focused evidence.** The R6
  packet explicitly required recorded nonzero `mise exec -- cargo nextest`
  selectors and AGENTS.md repeats that requirement for named Rust tests.

## Deduplicated retained ledger

### `FIND-admin-principals-R7-1` — CONFIRMED — INCORRECT — same-second overlap rotation returns a successor token that verification refuses

- **Wave-1 sources:** `TREV-WB07-1`, `REPO-R7-1`, `SEC-R7-2`,
  `DATA-R7-1`.
- **Violated obligation:** `REQ-007`, `REQ-010`, `INV-009`, `INV-013`,
  `AC-005`, original TASK-006's uninterrupted overlap rotation, and R6's
  focused requirement that the predecessor be refused while its ordered
  successor remains admitted.
- **Exact location:**
  `crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs:192-213`,
  `crates/shared/wyrd-auth-issue/src/lib.rs:491-513,581-594`,
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:220-279,382-526`,
  `crates/shared/wyrd-auth-verify/src/lib.rs:501-565`, and
  `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:840-1033,1050-1173`.
- **Caller trace and evidence:** Credential revocation calls
  `revoke_service_account_principal`, which stores sub-second `now()`. API-key
  exchange through `ExchangeApiKey::execute` issues through
  `issue_for_subject`; the issuing library serializes `iat` as whole Unix
  seconds. Both verifier paths call the authoritative resolver and reject when
  `iat < tokens_not_before`. Therefore B can exchange successfully after A's
  revocation commits but within the same wall-clock second, then be refused on
  its first protected request. The rotation and replay journeys stop after
  token exchange and explicitly avoid spending that token, so the path is
  reachable and unproved.
- **Observable consequence:** `issue B -> verify B -> revoke A -> re-exchange
  B` has a deterministic sub-second outage even though B was never revoked.
- **Decision-complete minimum correction:** Reuse the existing ordered
  user-token model. Store a service-account revocation epoch at the next
  representable whole-second boundary, carry that durable epoch through the
  already-required active-principal resolution during machine credential
  exchange, and issue the successor at `max(current issuance instant, stored
  epoch)`. Apply the same ordering to Card-bound and Card-free machine tokens;
  do not sleep, lower precision, change the verifier comparison, or add a
  credential-specific verifier.
- **Focused closure proof:** In the production-wired real-server rotation
  journey, warm A's token, revoke A, exchange B without a sleep, use B's exact
  returned token immediately on a protected route and require success, then
  require A's warm predecessor to fail on its next request. Retain replay proof
  that a second revoke neither moves the epoch nor invalidates B.

### `FIND-admin-principals-R7-2` — CONFIRMED — INCORRECT — the authoritative per-request read does not gate live tokens on principal status

- **Wave-1 source:** `SEC-R7-1`.
- **Violated obligation:** `REQ-005`, `REQ-012`, `INV-011`, `INV-013`,
  `AC-008`, and `AC-010` require suspended, deleted, missing, or unverifiable
  principals to stop authenticating no later than the next request.
- **Exact location:**
  `crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs:39-127`,
  `crates/wyrd/wyrd-auth/src/revocation_resolver.rs:47-93`,
  `crates/shared/wyrd-auth-verify/src/lib.rs:501-565`, and
  `crates/wyrd/wyrd-sql/src/queries/cards/delete.rs:148-169`.
- **Caller trace and evidence:** Served Card deletion marks the backing Service
  or Agent row `deleted` but does not change its epoch. Every protected tenant
  route enters `TokenVerifier::verify`; both cache-hit and cache-miss paths ask
  `SqlRevocationCheck`, but `service_account_admission` selects only
  `tokens_not_before`. A deleted or missing row therefore becomes `None`, which
  the verifier interprets as never revoked. Credential exchange correctly
  filters inactive rows, but that does not affect an already-issued bearer.
- **Observable consequence:** A bearer already held by a deleted Card-bound
  Service or Agent continues authorizing until token expiry, including through
  Bifrost routes, after the containment action commits.
- **Decision-complete minimum correction:** Extend the existing one-statement
  admission result and the existing `RevocationCheck` result to carry an
  explicit principal-admitted verdict in addition to the epoch. Derive that
  verdict from row presence and `status = 'active'` under RLS, and make the
  verifier unconditionally refuse missing, suspended, or deleted principals
  before epoch comparison on both cache paths. Preserve the one database round
  trip, tenant admission, verified-token cache, and fail-closed read errors;
  do not encode an inactive principal as `None` or add another lookup.
- **Focused closure proof:** Warm a Service or Agent token through the
  production verifier, delete its backing Card through the served route, and
  require the very next protected request with that bearer to fail. Also prove
  an active principal with no epoch remains admitted and a resolver read error
  still returns verify-unavailable.

### `FIND-admin-principals-13` — REVISED — VIOLATION — local transfer extractor failures still bypass the operation's published Wyrd problem contract

- **Wave-1 sources:** `TREV-WB07-2`, `REPO-R7-2`, `CONTRACT-07-01`.
- **Violated obligation:** `REQ-036`, `REQ-049`, `AC-014`, `AC-019`, stable
  `FIND-admin-principals-13`, AGENTS.md section 9, and the HTTP error authority
  require reachable public failures to match served problem media and stable
  codes.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/storage/routes.rs:312-379,429-490`
  and
  `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:158-225`.
- **Caller trace and evidence:** Both operations are mounted once through the
  protected storage router and consumed by server-produced URLs and the shared
  client. A missing or undecodable download `path` fails Axum's
  `Query<LocalDownloadQuery>` before `download_local_blob` can return
  `WyrdErrorResponse`; Axum serves plain-text 400 while OpenAPI promises
  `application/problem+json`. Upload's `Path<String>` normally reaches
  `parse_upload_id`, but invalid percent-decoded UTF-8 has the same pre-handler
  `PathRejection`, despite the operation promising a stable invalid-ID problem.
  The current test inspects only `/openapi.json`, so neither runtime rejection
  is exercised.
- **Observable consequence:** Independent and shared clients can receive an
  undocumented plain-text 400 with no stable Wyrd code from operations whose
  document promises a problem object.
- **Decision-complete minimum correction:** Receive the existing Axum query
  and path extractor rejections at these two route boundaries and map them
  through the existing `WyrdErrorResponse` owner. Use an existing validation or
  invalid-upload code and list the actually reachable code in the operation;
  preserve query-based download URLs, nested paths, binary bodies, and storage
  validation. Add no middleware, parser, route alias, or error catalog.
- **Focused closure proof:** Against the assembled authenticated router, send a
  download with no `path`, an undecodable download query, and an undecodable
  upload path; assert documented status, `application/problem+json`, standard
  problem shape, and an operation-listed stable code. Retain served-document,
  nested-path storage, shared-client, and CLI journeys.

### `FIND-admin-principals-R6-1` — REVISED — VIOLATION — the shared revocation contract and journey still prescribe the deleted epoch cache

- **Wave-1 source:** `TREV-WB07-3`.
- **Violated obligation:** Stable `FIND-admin-principals-R6-1` required direct
  authoritative reads and deletion of obsolete cache machinery and its
  contract; repository documentation must describe actual behavior.
- **Exact location:**
  `crates/shared/wyrd-auth-verify/src/lib.rs:155-164` and
  `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:3047-3057`.
- **Caller trace and evidence:** `TokenVerifier::verify` invokes
  `RevocationCheck::epoch` on both verified-token cache paths, and production
  now correctly reaches Postgres each time. The public trait rustdoc still says
  implementations should use a short-TTL cache and add no network IO. The
  tenant-suspension journey still claims it primes a production five-second
  epoch/admission cache. These are the exact design R6 removed.
- **Observable consequence:** The shared implementation contract directs a
  future resolver to recreate the closed security defect, and the journey
  falsely states what its passing assertion proves.
- **Decision-complete minimum correction:** Update only the trait and journey
  prose to state the real split: verified token/signature work remains cached,
  while authoritative tenant/principal admission and epoch state is read on
  every verification. Add no documentation layer or behavior.
- **Focused closure proof:** Strict rustdoc for the affected crates and the
  existing production-wired suspension journey pass, and a cumulative search
  finds no claim that production revocation epochs or admission are cached.

### `FIND-admin-principals-R5-2` — REVISED — INCORRECT — principal-revoke stable misses discard the decision and store failures masquerade as not-found

- **Wave-1 source:** `SEC-R7-3`.
- **Violated obligation:** `REQ-037`, `INV-011`, `AC-009`, the canonical audit
  rule, and stable `FIND-admin-principals-R5-2` require every evaluated
  permission decision to commit for a stable no-effect result while store
  failures roll back and fail closed.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/revoke.rs:38-78` and
  `crates/wyrd/wyrd-server/src/auth/revoke.rs:72-107`.
- **Caller trace and evidence:** The served revoke route appends its allowed
  decision on a `TenantConn`, calls `revoke_principal_in_conn`, and commits only
  after success. Unknown and wrong-kind principals return the stable
  `PrincipalNotFound`, so `?` drops the transaction and its decision. Both user
  and service-account reads use `.await.ok().flatten()`, collapsing a SQL error
  into the same not-found result and hiding an unhealthy security store. No
  other caller commits this transaction.
- **Observable consequence:** Authorized enumeration-resistant misses leave no
  durable evidence, while a database outage is falsely reported as a caller
  error and is indistinguishable operationally from absence.
- **Decision-complete minimum correction:** Propagate lookup errors through the
  existing internal failure mapping so they roll back decision and effects.
  At the route owner, distinguish the existing stable `PrincipalNotFound`,
  commit the already-appended decision-only transaction, then return that same
  problem; keep all other domain/store failures uncommitted. Add no second
  transaction or audit helper.
- **Focused closure proof:** Real-Postgres unknown-id and wrong-kind revocations
  each return the stable not-found response, append exactly one allowed
  decision, and change no principal/epoch/refresh state. An injected lookup
  failure returns internal failure and commits neither decision nor effect.

### `FIND-admin-principals-R7-3` — CONFIRMED — VIOLATION — the materially rewritten revocation owner retains a raw application pool

- **Wave-1 source:** `REPO-R7-3`.
- **Violated obligation:** `architecture/agent-rules.md` permits only
  `&mut TenantConn<'_>` and `&OperatorPool` in library fields and signatures,
  makes pool acquisition an owning-handle boundary, and requires materially
  changed Rust to comply; the approved specification likewise fixes exactly
  two connection abstractions.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/revocation_resolver.rs:8-15,27-44,59-69`, with
  construction callers in
  `crates/wyrd/wyrd-server/src/boot/auth.rs:55-85`,
  `crates/wyrd/wyrd-testing/src/server.rs:3808-3826`, and
  `crates/wyrd/wyrd-server/src/http/middleware/authenticate.rs:251-263`.
- **Caller trace and evidence:** R6 rewrote `SqlRevocationCheck`, its
  constructor, and all production/test assembly, but the struct still stores
  `Arc<PgPool>`, accepts it publicly, and calls `TenantConn::acquire` directly.
  `WyrdPostgres::tenant_conn` already owns acquisition lifecycle and telemetry;
  this resolver bypasses it. There is one production implementation and no
  need for a new connection abstraction.
- **Observable consequence:** The security hot path has a second acquisition
  path outside the repository's owner and silently omits its lifecycle
  telemetry; future connection policy has two places to change.
- **Decision-complete minimum correction:** Store the existing cloneable
  `WyrdPostgres` owner in `SqlRevocationCheck`, accept that owner at assembly,
  and acquire through `WyrdPostgres::tenant_conn`. Update the existing
  production and fixture callers; use the sanctioned test-only `from_pools`
  seam for the unreadable-store test. Add no wrapper, trait, or factory.
- **Focused closure proof:** A cumulative declaration scan finds no raw pool in
  the resolver's field or constructor, the production constructor still drives
  the warm-token and fail-closed tests, and the existing pool-acquisition
  telemetry path is reached.

### `FIND-admin-principals-R7-4` — CONFIRMED — VIOLATION — the new admission statements duplicate RLS with hand-written tenant predicates

- **Wave-1 source:** `REPO-R7-4`.
- **Violated obligation:** `AC-010`, `architecture/agent-rules.md`, and the
  storage pattern require `TenantConn` queries to rely on forced Postgres RLS
  and prohibit parallel tenant predicates.
- **Exact location:**
  `crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs:69-75,96-102`; the forced
  RLS policies are in
  `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:35-38,97-100`.
- **Caller trace and evidence:** `user_admission` and
  `service_account_admission` are called only after the resolver acquires a
  tenant-bound `TenantConn`. Their new subqueries nevertheless repeat
  `data_tenant_id = wyrd.current_tenant()`. RLS already applies the same scope;
  the separate `platform.tenant_admits_credentials($1)` argument is the tenant
  lifecycle lookup and is not the predicate at issue.
- **Observable consequence:** The hot path maintains two expressions of tenant
  scope which can drift, directly failing an explicit acceptance obligation
  even though they happen to agree today.
- **Decision-complete minimum correction:** Remove only the two manual tenant
  predicates from the auth-table subqueries and rely on the existing
  RLS-bound `TenantConn`; keep the tenant argument used by the platform
  lifecycle function and keep the single statement/round trip.
- **Focused closure proof:** Focused user and service admission cases prove
  same-tenant results and cross-tenant invisibility under `TenantConn`, followed
  by `mise run check:tenant-isolation` and the owning SQL lane.

### `FIND-admin-principals-R7-5` — CONFIRMED — VIOLATION — the completion evidence omits required exact nonzero selectors for named Rust tests

- **Wave-1 sources:** task-review verification matrix and contract-review
  verification limits.
- **Violated obligation:** AGENTS.md section 11 and R6 remediation lines
  164-166 require every specifically named Rust test to be recorded and run via
  an exact nonzero `mise exec -- cargo nextest run --locked` selector with its
  owning environment setup.
- **Exact location:**
  `changes/active/admin-principals/review/whole-branch-06/TASK-001-008-R6-close-validated-findings.md:209-225`.
- **Evidence:** The packet records aggregate lanes for
  `platform_admin_e2e::an_authorized_request_that_changes_nothing_still_records_the_decision`,
  `pg_openapi_contract::local_transfer_operations_publish_their_binary_contract`,
  and the named warm-token test, but records an exact selector only for
  `wyrd-storage::service::tests::local_download_url_uses_mounted_http_blob_route`.
  Aggregate lanes credibly show broad suites passed; they do not prove the
  mandated focused commands were selected and run.
- **Observable consequence:** The completion packet cannot establish its own
  focused-proof acceptance condition and allows a renamed, filtered, or gated
  test to be credited without a nonzero selector transcript.
- **Decision-complete minimum correction:** Confirm every named Rust test with
  `cargo nextest list`, run each exact package/target/test expression through
  `mise exec --` with the repository-managed environment wrapper where needed,
  and append the exact command, nonzero count, and result to the next
  remediation evidence table. Do not add a scanner or another test lane.
- **Focused closure proof:** The final evidence contains exact nonzero
  selectors for every old and newly named Rust closure test, including the
  rotation successor, principal-status, extractor-rejection, principal-revoke
  audit, and production-resolver cases, plus their owning aggregate lanes.

## Prior-finding closure

- `FIND-admin-principals-R5-1` and `FIND-admin-principals-R5-5` are closed at
  their validated source and contract boundaries.
- `FIND-admin-principals-R5-2` remains open only for the newly traced
  principal-revoke stable miss and swallowed read failure.
- `FIND-admin-principals-13` is closed for co-registration, locator
  representability, binary schemas, and proof guidance, but remains open for
  the reachable extractor error shape.
- `FIND-admin-principals-R6-1` is closed for runtime cache/listener deletion,
  one-round-trip authoritative reads, and next-request warm-token lookup, but
  remains open for its contradictory shared contract and journey prose.
- Earlier findings recorded closed by whole-branch-06 remain closed. The
  owner's waiver of `FIND-TASK-001-10` and withdrawn historical
  application/Iceberg compatibility proposal are unchanged.

## Verification limits

This was a static Wave-2 validation. It did not rerun the long Docker/Postgres
suites. The recorded broader results are credible for their selected lanes,
but none spends the same-second successor token, checks a warm bearer after
Card/principal deletion, exercises local extractor rejection through the
assembled router, proves principal-revoke miss audit durability and SQL-error
propagation, or supplies all mandated exact focused selectors. Source-shape
rules independently establish the raw-pool, manual-predicate, and stale-rustdoc
violations.

The candidate remained
`ddd80c7a7b723a2f39a729e5aebe6b6072d2b983` at the final integrity check.
