# Findings validation — cumulative R9 and R10

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `41e60be61958c92f562fceb5b7a03f40f611bbc8`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 14
- Remediation tasks:
  `whole-branch-09/TASK-001-008-R9-close-validated-findings.md` and
  `whole-branch-10/TASK-001-008-R10-correct-rfc8693-delegation.md`

The candidate was unchanged before and after validation. The approved
five-minute stateless-JWT revocation window was excluded as directed.

## Validation method

The complete cumulative diff, approved specification, original tasks, prior
remediation record, applicable authorities, every Wave 1 report, and current
source were inspected. CodeGraph was used first. For every retained proposal,
the live path and all callers of the correction boundary were traced; dormant,
test-only, speculative, and unrelated debt was rejected. Corrections stop at
the existing owner and add no new production abstraction, store, permission,
policy engine, transport, or audit path.

## Wave 1 proposal disposition

| Wave 1 proposal | Disposition | Validation |
|---|---|---|
| `TREV-WB11-1`, `AUTH-1` — delegation is preview-gated out of production | **REVISED / CONSOLIDATED** | Reachable and task-required. `POST /auth/token` checks `ServerAuth.allow_preview`, while both config validation and `AppState::production_validate` reject that flag for an API-serving production process. The test server defaults it on, hiding the contradiction. The preview switch has no remaining production use other than this exchange branch, so the smallest correction is deletion of that obsolete gate and all of its now-orphaned config/error/docs surface, not another exception flag. Retained as `FIND-admin-principals-R11-1`. |
| `TREV-WB11-2` — delegation audit records the operation as the evaluated permission | **CONFIRMED** | `policy_context` evaluates action `invoke`; both `record_decision` and `exchange_audit_event` call `auth_event`, which initializes `permission` to `auth.token.exchange`, and neither overwrites it. This is the one canonical row for the reachable allow/deny decision, so it must retain the operation while naming the actual evaluated permission. Retained as `FIND-admin-principals-R11-2`. |
| `TREV-WB11-3` — the required directed-policy proof is allow-all | **REVISED** | Production constructs the policy question in the correct A-subject/B-actor order, but the primary journey uses `RecordingPolicyHook`, whose complete implementation records and always allows. No test submits the same two valid tokens in reverse, despite `AC-020` requiring that refusal. Keep the existing `PolicyHook` seam; do not change the generally useful allow-and-record helper or add a production policy type. Retained as `FIND-admin-principals-R11-3`. |
| `TREV-WB11-4`, `SDK-1`, `REPO-R9R10-2` — Python/TypeScript delegated clients are unusable and lack journeys | **REVISED / CONSOLIDATED** | Confirmed end to end. Each foreign `WyrdClient` exposes only construction and another delegation call; the existing foreign `Bifrost` constructors always build a new client from raw options and cannot consume the delegated one. The unit tests prove only validation and transport failures. Reuse the existing Rust `Bifrost` composition from `WyrdClient` and the existing Python/TypeScript integration harnesses; do not add token accessors, exchange logic, transports, or a delegation-only harness. Retained as `FIND-admin-principals-R11-4`. |
| `BIFROST-01` — native-ingest audit drops the actor chain | **REVISED** | Confirmed on the production dynamic-dispatch path: Gate authenticates the delegated bearer into `AuthContext.delegation_chain`, passes the full context to its sole `GateAudit` call, and `PostgresGateAudit::append_write_decision` discards the chain when constructing `AuditEvent`. Existing HTTP and Oracle audit builders already provide the required projection. Retained as `FIND-admin-principals-R11-5`. |
| `TREV-WB11-5` — Bifrost CLI documentation still uses removed `--token` | **REVISED** | Confirmed: the shipped `QueryCommand` has no token argument and resolves the ambient credential chain, while the live guide invokes `wyrd query --token`. Delete the argument from that example and name the existing ambient source. A new permanent docs checker is unearned; the existing parser proof plus docs validation is the smallest closure proof. Retained as `FIND-admin-principals-R11-6`. |
| `CRED-CLI-1` — `IssueKeyResponse.key_id` is still a string | **REVISED; prior ID preserved** | Confirmed as the still-incomplete root of `FIND-admin-principals-R9-2`. `POST /auth/issue-key` creates the same UUID-backed credential model, but `IssueKeyResponse` weakens its id to `String`, issuance stringifies the UUID, and the served-OpenAPI proof omits this component. Reuse installed `Uuid`; no wrapper or compatibility field is justified. Retained as `FIND-admin-principals-R9-2`. |
| `REPO-R9R10-1` — changed Rust items lack required `# Panics` contracts | **REVISED** | Confirmed by candidate-added examples including `seed_actor`, `a_policy_denied_exchange_commits_one_denied_decision`, `issue_access_token_delegated_names_subject_and_outer_actor`, and `into_verified_keeps_subject_and_orders_actors_earliest_first`: each can panic and lacks `# Panics`, contrary to `AGENTS.md` §16. Limit correction to a diff-scoped inventory of R9/R10 items; do not document untouched code or add a permanent audit framework. Retained as `FIND-admin-principals-R11-7`. |
| `REPO-R9R10-3` — TypeScript authority forbids the newly approved shared-client projection | **CONFIRMED** | The focused guide says the N-API layer must not wrap a raw `WyrdClient`, while revision 14 and the implemented thin `NativeWyrdClient` intentionally require that shared auth/delegation owner. Narrowly update the guide without relaxing its bans on a separate `QueryClient`, duplicated transport, or per-call gRPC transport. Retained as `FIND-admin-principals-R11-8`. |
| `REPO-R9R10-4` — run `mise run gate` for the Python tooling change | **REJECTED** | Approved spec `VER-003` explicitly says `mise run gate` must not be run or required and that its absence is not missing evidence; R10 repeats that prohibition. The changed Python tasks were run through their named lanes. Requiring the forbidden aggregate would contradict the governing task authority. |

## Final deduplicated ledger

### `FIND-admin-principals-R11-1` — REVISED — MISSING — production cannot use RFC 8693 delegation

- **Wave 1 sources:** `TREV-WB11-1`, `AUTH-1`.
- **Violated obligation:** specification `REQ-012c`, `REQ-047`, `AC-018`, and
  `AC-020`; R10 outcome, removal constraint, and client-helper acceptance.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:176-185`,
  `crates/wyrd/wyrd-server/src/components/auth/state.rs:14-20`,
  `crates/wyrd/wyrd-server/src/config.rs:1931,2537-2539,2746-2750`,
  `crates/wyrd/wyrd-server/src/state.rs:2392-2444`, and the live preview
  error/config documentation found by those owners.
- **Evidence and reachability:** every token-exchange request reaches the route
  guard before token verification. `allow_preview=true` is required to pass it,
  but both production validators reject that same state. The only production
  consumer of the flag is this route; `WyrdTestServer` defaults it to true and
  always composes a development profile.
- **Observable consequence:** every deployed Rust, Python, or TypeScript
  `on_behalf_of` call is refused; the standard delegation flow works only in
  development/test.
- **Decision-complete correction:** delete the route guard and the orphaned
  `allow_preview` config/env parsing, boot warning, `ServerAuth` field, test
  builder switch, production-validation variant, preview-disabled catalog
  error, generated error artifacts, and live preview-delegation prose. Preserve
  the existing production requirements for a real policy hook, real audit
  writer, signing key, and verifier. Add no replacement flag or compatibility
  behavior.
- **Focused closure proof:** run the existing primary A/B/Bifrost journey with
  no preview setting. Extend the existing production-validation smoke owner to
  compose its existing non-stub recording policy plus
  `RealAuthzAuditWriter`, set the serving state to production, and prove
  validation succeeds while the existing stub-policy and missing-verifier
  refusals remain. Regenerate error/docs artifacts and prove no live
  `allow_preview`, `WYRD_AUTH_ALLOW_PREVIEW`, or preview-disabled route/error
  residue remains.

### `FIND-admin-principals-R11-2` — CONFIRMED — INCORRECT — delegation audit records the wrong permission

- **Wave 1 source:** `TREV-WB11-2`.
- **Violated obligation:** `REQ-012c`, `REQ-037`, `INV-010`, `AC-009`, and
  R10's audit acceptance row.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:334-423` and
  `crates/wyrd/wyrd-auth/src/issuance.rs:557-609`.
- **Evidence and reachability:** the existing policy evaluates
  `DELEGATION_POLICY_ACTION` (`invoke`), but every reachable delegated
  allow/deny/no-effect row keeps `permission=auth.token.exchange` from
  `auth_event`. Current assertions do not select `permission`.
- **Observable consequence:** retained audit cannot truthfully answer which
  permission authorized or denied the A-to-B exchange.
- **Decision-complete correction:** preserve the one canonical row and
  `operation=auth.token.exchange`; overwrite its existing `permission` with
  the existing invoke-policy action for every delegated successful, denied,
  and allowed-then-refused decision. Direct non-delegated token exchanges keep
  their current permission encoding. Add no event or vocabulary.
- **Focused closure proof:** extend the existing successful, policy-denied, and
  allowed-then-refused Postgres selectors to require exactly one row with
  `operation=auth.token.exchange` and `permission=invoke`; retain the existing
  audit-failure no-token proof.

### `FIND-admin-principals-R11-3` — REVISED — MISSING — the directed A-to-B policy is not proved

- **Wave 1 source:** `TREV-WB11-3` and the auth-security verification limit.
- **Violated obligation:** `REQ-012c`, `INV-013a`, `AC-020`, and R10's RFC
  request-semantics and primary-journey requirements.
- **Exact location:**
  `crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs:1165-1169`,
  `crates/shared/wyrd-auth-check/src/hook.rs:37-75`, and
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1629-1687`.
- **Evidence and reachability:** the primary journey's
  `RecordingPolicyHook::evaluate` always returns `Allow`; the malformed-input
  table never swaps two valid subject/actor tokens. The production policy
  context is oriented correctly, but the required evidence would remain green
  if the policy treated both directions as equivalent.
- **Observable consequence:** the candidate has no proof that permission for A
  to invoke B cannot be reused as permission for B to invoke A.
- **Decision-complete correction:** revise the existing primary journey's
  policy seam so it allows exactly its seeded A-subject/B-actor relationship
  and denies the reverse. Do not alter the shared allow-and-record helper and
  do not add a production policy, store, role, or permission. Submit both
  exchanges through the real token endpoint and retain the existing correct
  context construction.
- **Focused closure proof:** the named primary journey proves A/B succeeds,
  then sends valid B as subject and valid A as actor, receives the stable policy
  denial, commits exactly one denied invoke decision under B, and issues no
  token or Bifrost effect.

### `FIND-admin-principals-R11-4` — REVISED — MISSING / VIOLATION — foreign delegated clients cannot perform delegated work

- **Wave 1 sources:** `TREV-WB11-4`, `SDK-1`, `REPO-R9R10-2`.
- **Violated obligation:** `REQ-047`, R10's client-helper acceptance, and
  `AGENTS.md` §11's first-class SDK journey requirement.
- **Exact location:**
  `sdks/wyrd-sdk-python/src/client.rs:16-79`,
  `sdks/wyrd-sdk-python/src/bifrost/mod.rs:263-290`,
  `sdks/wyrd-sdk-ts/native/src/client.rs:15-99`,
  `sdks/wyrd-sdk-ts/wyrd/src/index.ts:672-686,983-1033`, and the two
  unit-only tests cited by Wave 1.
- **Evidence and reachability:** Python and TypeScript successfully return a
  wrapper around the Rust delegated client, but no public operation consumes
  that wrapper. Each Bifrost facade instead constructs another client from
  options, losing the delegated auth state. No foreign-runtime test completes
  an exchange or spends its result.
- **Observable consequence:** the new public helper dead-ends in both SDKs;
  users can obtain a delegated client but cannot use it against Bifrost.
- **Decision-complete correction:** expose the smallest idiomatic way for each
  existing Python/TypeScript Bifrost facade to compose from its language's
  existing `WyrdClient` wrapper, calling the current Rust
  `Bifrost::connect`/`query_only` owner. Keep token exchange, bearer access,
  caching, retry, headers, and transport private in Rust. Do not widen Cards or
  `WyrdState`, add a token getter, duplicate Bifrost, or add a new harness.
- **Focused closure proof:** in each existing Python and TypeScript integration
  harness, seed A-read and B-read/write, obtain A's access token, construct B's
  public `WyrdClient`, call `on_behalf_of`/`onBehalfOf`, pass the result into
  public Bifrost, and prove one real A-authorized read succeeds while a B-only
  write is refused before effect. Run the named tests through
  `mise run py:test:integration` and `mise run ts:test:integration` with
  positive selections, plus the Rust primary journey.

### `FIND-admin-principals-R11-5` — REVISED — INCORRECT — native-ingest audit loses the delegated actor

- **Wave 1 source:** `BIFROST-01`.
- **Violated obligation:** R10 §3 and audit acceptance; repository canonical
  authorization-audit attribution rules.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/bifrost/gate_audit.rs:28-64`, reached from
  `crates/vala/vala-bifrost-redux/src/gate/mod.rs:437-459` after
  `crates/vala/vala-bifrost-redux/src/gate/auth.rs:112-126` preserves the
  verified chain.
- **Evidence and reachability:** the sole production `GateAudit`
  implementation receives `AuthContext.delegation_chain` for both allowed and
  denied record-write decisions but creates an event with no detail. Existing
  HTTP and Oracle owners already project this same chain with
  `AuditDetail::DelegationAttribution` and
  `wyrd_runtime::audit_delegation_chain`.
- **Observable consequence:** retained native-ingest audit says A performed the
  decision but cannot identify B as the acting service.
- **Decision-complete correction:** reuse the existing attribution detail and
  projection in `PostgresGateAudit::append_write_decision`, attaching it only
  for a non-empty chain so direct-call events remain unchanged. Do not alter
  Gate authorization, add a sink, or move audit ownership.
- **Focused closure proof:** extend the existing primary journey rather than
  adding a harness: give B the existing `bifrost_record:write` permission while
  A lacks it, submit a real batch through B's delegated public Bifrost client,
  prove Gate refuses it before effect, prove B's direct client can write the
  same shape, and assert the denied Gate audit is under A with B in its chain.
  This also proves the delegated Bifrost audience reaches native gRPC ingest.

### `FIND-admin-principals-R11-6` — REVISED — REGRESSION — live docs use the removed secret argument

- **Wave 1 source:** `TREV-WB11-5`.
- **Violated obligation:** `INV-002`, R9's CLI-secret correction, and the
  requirement that live operator documentation match shipped commands.
- **Exact location:**
  `docs/src/content/docs/bifrost/reading-data.svx:135-142` versus
  `crates/wyrd/wyrd-cli/src/query/mod.rs:16-31`.
- **Evidence and reachability:** the guide invokes `wyrd query --token`, while
  the root parser no longer accepts that option and `QueryCommand` documents
  the ambient credential chain.
- **Observable consequence:** the documented command fails and tells users to
  place a bearer in shell argv despite the security correction.
- **Decision-complete correction:** remove `--token` from that example and
  state that `WYRD_ACCESS_TOKEN` or the normal ambient credential chain must be
  set. Add no compatibility option and no new permanent docs checker.
- **Focused closure proof:** `mise run docs:check` passes, the existing root
  parser secret-option selector still passes, and a focused source assertion
  confirms the published `wyrd query` example contains no secret-valued
  argument.

### `FIND-admin-principals-R9-2` — REVISED — INCORRECT — one credential issuance contract still discards UUID

- **Wave 1 source:** `CRED-CLI-1`.
- **Violated obligation:** the existing stable R9 finding and its acceptance
  row requiring issued/listed credential IDs and OpenAPI to remain UUID typed.
- **Exact location:** `crates/wyrd-spec/src/auth/issue_key.rs:25-38`,
  `crates/wyrd/wyrd-auth/src/issue_api_key.rs:101-136`, and
  `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:519-540`.
- **Evidence and reachability:** the served `POST /auth/issue-key` creates
  `api_key_id: Uuid`, stringifies it into `IssueKeyResponse.key_id`, and exposes
  an unconstrained string schema. The existing contract test checks only
  `IssuedCredential.id` and `CredentialMetadata.id`.
- **Observable consequence:** generated clients cannot rely on one credential
  identifier contract across issuance, listing, revoke, MCP, and OpenAPI.
- **Decision-complete correction:** change the existing response field to the
  already-installed `Uuid` type and pass `api_key_id` directly. Update current
  consumers and generated schemas through normal serialization. Add no newtype,
  parser, compatibility field, or route.
- **Focused closure proof:** extend
  `credential_ids_publish_their_uuid_contract` to assert
  `IssueKeyResponse.key_id.format == uuid`, and make the existing issue-key
  route/journey compare the returned value directly with the UUID produced by
  issuance. Run code generation and the owning auth/CLI journey.

### `FIND-admin-principals-R11-7` — REVISED — VIOLATION — R9/R10 Rust items have incomplete rustdoc

- **Wave 1 source:** `REPO-R9R10-1`.
- **Violated obligation:** `AGENTS.md` §16 and `architecture/agent-rules.md`
  require substantive rustdoc on every changed Rust item and `# Errors` or
  `# Panics` whenever applicable, including private helpers and tests.
- **Exact location:** confirmed examples at
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1469-1491,1629-1655`,
  `crates/shared/wyrd-auth-issue/src/lib.rs:730-758`, and
  `crates/shared/wyrd-auth-verify/src/lib.rs:1085-1128`; the closure boundary is
  every Rust item added or materially modified by the R9/R10 commit range.
- **Evidence and reachability:** each cited helper/test contains `expect`,
  `panic!`, indexing, or assertions but its rustdoc omits `# Panics`. Strict
  public `cargo doc` cannot inspect private/test items, so the recorded lane
  does not establish this repository-specific rule.
- **Observable consequence:** maintainers are given incomplete contracts for
  the new security workflow, and the candidate misses a hard completion rule.
- **Decision-complete correction:** perform one diff-scoped inventory of R9/R10
  Rust items and add only the missing substantive rustdoc, `# Errors`, and
  `# Panics` sections their real bodies require. Do not document untouched
  code, suppress a lint, or add a permanent audit script.
- **Focused closure proof:** append the inventory with file/item/result to the
  remediation evidence, rerun strict rustdoc for affected crates, and rerun the
  exact focused tests whose contracts were documented.

### `FIND-admin-principals-R11-8` — CONFIRMED — VIOLATION — TypeScript guidance contradicts the shared delegation owner

- **Wave 1 source:** `REPO-R9R10-3`.
- **Violated obligation:** R10 §4, `REQ-047`, and repository authority
  synchronization for a changed first-class SDK surface.
- **Exact location:**
  `architecture/references/languages/typescript-guide.md:171-183` versus
  `sdks/wyrd-sdk-ts/native/src/client.rs:15-99`.
- **Evidence and reachability:** the focused TypeScript guide forbids wrapping
  a raw `WyrdClient`, while the approved architecture requires the thin N-API
  projection of the single shared Rust `WyrdClient` for delegation.
- **Observable consequence:** future implementations and reviews receive two
  contradictory instructions for the required public SDK shape.
- **Decision-complete correction:** narrowly allow the N-API layer to project
  the shared `WyrdClient` for shared authentication/delegation and composition
  with existing facades. Preserve the guide's prohibition on a separate
  `QueryClient`, duplicated transport or auth, and per-call gRPC transport.
- **Focused closure proof:** documentation checks pass and a source review finds
  one consistent rule across `AGENTS.md`, `wyrd-design.md`, and the TypeScript
  guide; existing TypeScript boundary checks and the R11-4 journey remain
  green.

## Validation result

Nine material findings remain: the prior `FIND-admin-principals-R9-2` in
revised form and new `FIND-admin-principals-R11-1` through
`FIND-admin-principals-R11-8`. Every correction is bounded by approved revision
14 and an existing owner; none requires a specification revision.
