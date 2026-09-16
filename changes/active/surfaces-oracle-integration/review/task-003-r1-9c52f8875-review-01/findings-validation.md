# Wave 2 Structured Ponytail Validation

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Cumulative base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `9c52f887595d4e8a04764d3d2836926e5df70950`
- Candidate tree: `e2731ff2d23fc1a0f4b356ccdd79caf3f6c7b93e`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 9 (candidate originally carried revision 8)
- Reviewed tasks: `TASK-002`, `TASK-004`, `TASK-003`, and remediation `TASK-003-R1`

The commit and tree resolve exactly. The checkout is later and dirty with
unrelated work, so all implementation source and diffs were read from the
immutable Git objects. The repository has no `.codegraph/` directory.

## Human-directed amendment (2026-09-16)

The owner confirmed that no storage configuration has been deployed and chose
the tested `WYRD_STORAGE_URL` contract as the intended simpler configuration.
This decision supersedes the earlier DRIFT judgment on storage configuration;
it does not change the immutable candidate or the historical Wave 1 reports.
`FIND-TASK-003-R1-1` is removed. A small follow-up below keeps GitHub Actions
and Wyrd documentation aligned with the retained contract.

The owner also directs real-cloud Actions to run on merges to `main`, not on
a weekly cadence, and directs candidate cloud proof through the existing local
`mise.local.toml` tasks. These changes supersede the proposed `R1-5` finding
and revise `R1-7`. The owner's explicit override is now recorded as approved
specification revision 9; local cloud proof is sufficient and no further
approval is required for this decision.

Independent owner-supplied deadline analysis was checked against the immutable
candidate's client transport, protected HTTP edge, query service, Oracle
forwarder, and existing test seams. It adds `FIND-TASK-003-R1-8` and
`FIND-TASK-003-R1-9` below. This is a direct addendum, not a retroactive Wave 1
review report.

## Wave 1 proposal validation

| Wave 1 proposal | Result | Final disposition |
|---|---|---|
| `TASK-REV-R1-01` | **REVISED** | Retained as `FIND-TASK-003-R1-2`. Both tests are unreachable, but the Azure dispatch assertion is not wholly redundant; fold it into the existing Azurite-owned test before deleting its extra test binary. |
| `TASK-REV-R1-02` | **SUPERSEDED BY OWNER DECISION** | The owner retains the new storage contract; the proposed reversion is removed. |
| `TASK-REV-R1-03` | **CONFIRMED** | Retained as `FIND-TASK-003-R1-6`. The task expressly makes same-candidate proof an acceptance obligation, so this is a valid verification finding rather than optional evidence. |
| `TASK-REV-R1-04` | **REVISED BY OWNER DIRECTION** | Retained as `FIND-TASK-003-R1-7`, using local real-cloud tasks for candidate proof under approved revision 9. |
| `STD-001` | **REJECTED** | `TASK-003-R1` explicitly excludes a stale-authority documentation sweep. The contradictory Oracle WAL/relay prose predates this remediation and is a final-change-review limit, not authority to expand this bounded task. |
| `STD-002` | **SUPERSEDED BY OWNER DIRECTION** | Cloud Actions run on merges to `main`; do not enable the weekly cron. Specification and testing authority now record this in revision 9. |
| `STD-003` | **REVISED** | Retained as `FIND-TASK-003-R1-4`. `AGENTS.md` §16 expressly applies rustdoc to materially modified tests and private helpers, including those retained with the chosen storage configuration. |
| `PERSIST-R1-01` | **SUPERSEDED BY OWNER DECISION** | Same storage-contract proposal; no retained finding. |
| `RI-R1-01` | **SUPERSEDED BY OWNER DECISION** | Same storage-contract proposal; no retained finding. |
| `RI-R1-02` | **REVISED** | Duplicate evidence for `FIND-TASK-003-R1-2`, with the correction narrowed as above. |
| `RI-R1-03` | **CONFIRMED** | Retained as `FIND-TASK-003-R1-3`. |
| `RI-R1-04` | **CONFIRMED** | Duplicate evidence for `FIND-TASK-003-R1-6`. |
| `RI-R1-05` | **REVISED BY OWNER DIRECTION** | Duplicate evidence for `FIND-TASK-003-R1-7`; use existing local real-cloud tasks and retain post-merge Actions. |
| `STREAM-R1-001` | **SUPERSEDED BY OWNER DECISION** | Same storage-contract proposal; no retained finding. |
| Client-contract empty ledger | **CONFIRMED** | Independent source tracing found no retained client, SDK, generated-contract, or MCP finding. |

## Final deduplicated finding ledger

### `FIND-TASK-003-R1-2` — REVISED — MISSING: two surviving storage tests have no owning lane

- **Wave 1 sources:** `TASK-REV-R1-01`, `RI-R1-02`.
- **Violated obligation:** revision-9 `REQ-064`, `INV-025`, `AC-022`, and
  `TASK-003` require every surviving local non-credentialed test to execute
  through an owner with nonzero selection and expressly reject a baseline
  waiver.
- **Location:** `crates/wyrd/wyrd-storage/Cargo.toml:64-82`,
  `crates/wyrd/wyrd-storage/tests/handle_crud.rs:111-119`,
  `crates/wyrd/wyrd-storage/tests/backend_contracts.rs:1-45`, and
  `mise.toml:708-739`.
- **Reachability and evidence:** `handle_crud` requires `emulator`, so the
  default family cannot compile `local_handle_crud`; every explicit handle lane
  filters to one cloud-named test. `backend_contracts` requires `cloud`, and no
  task selects that binary. The candidate's own evidence admits both gaps.
  `backend_contracts` overlaps the Azurite missing-blob test but uniquely calls
  through `BackendSigner -> CloudSigner -> AzureSigner`, so deleting it without
  preserving that dispatch assertion would weaken proof.
- **Observable consequence:** `gate` and all storage workflows can pass while
  two checked-in tests execute zero times.
- **Decision-complete correction:** add `local_handle_crud` as one exact
  selection under the existing emulator-backed handle owner. Move the unique
  `BackendSigner` Azure-abort dispatch assertion into the already-owned
  `integration_azurite` missing-blob test, then delete the redundant
  `backend_contracts` file and manifest target. Add no new task, target, or
  checker.
- **Focused closure proof:** run the exact local handle test with the existing
  emulator feature, run the Azurite test containing the dispatch assertion, and
  run `mise run test:storage:matrix`; record nonzero counts and show the orphan
  target is absent.

### `FIND-TASK-003-R1-3` — CONFIRMED — VIOLATION: the Forge gate fix weakens an expressly excluded durability assertion

- **Wave 1 source:** `RI-R1-03`.
- **Violated obligation:** `TASK-003-R1` excludes rare
  settlement-cancellation remediation and forbids weakening a test to obtain a
  green gate; `REQ-064` and `INV-025` repeat that prohibition.
- **Location:** `crates/vala/vala-bifrost-redux/tests/integration/forge/compaction_admission.rs:2575-2588`
  (`c18f0ce07`).
- **Reachability and evidence:** `acceptance_unknown_recovers_from_durable_state`
  directly calls `unresolved_commit_resets_once_absence_is_provable`, which
  stops the worker and inspects the process-wide returned-error list. The
  candidate broadens the accepted result from checkpoint shutdown only to any
  error containing `refused before commit: Cancelled`. That branch is reachable
  at the production publication-authority check, but changing what this proof
  accepts is precisely the excluded cancellation-settlement work. The
  implementation record acknowledges the scope conflict.
- **Observable consequence:** the journey no longer proves its former contract
  that recovery settles without a publication refusal; a matching refusal is
  accepted by text instead.
- **Decision-complete correction:** restore the original assertion. Reuse the
  existing worker observer/lifecycle barrier to wait for the recovered attempt
  to settle before stopping the worker, so shutdown cannot create the extra
  publication refusal. Do not change production Forge cancellation semantics
  or broaden accepted error strings.
- **Focused closure proof:** run the exact
  `acceptance_unknown_recovers_from_durable_state` test repeatedly and retain
  the original shutdown-only assertion.

### `FIND-TASK-003-R1-4` — REVISED — VIOLATION: materially modified Rust test workflows lack required rustdoc

- **Wave 1 source:** `STD-003`.
- **Violated obligation:** `AGENTS.md` §16 and `architecture/agent-rules.md`
  require rustdoc on every new or materially modified Rust item, including
  private helpers and tests, with `# Panics` and async cancellation/partial
  progress where relevant.
- **Location:** at minimum `crates/wyrd/wyrd-server/tests/storage_e2e.rs:172-255,257-260,348-405,408-435`,
  plus materially rewritten storage helpers/tests in the retained configuration.
- **Reachability and evidence:** `run_client_server_journey` is a materially
  rewritten client→server→cloud workflow with durable upload and cleanup
  effects but has no rustdoc. The local and three cloud tests panic through
  assertions/`expect`; the cloud tests have only summary lines and no
  `# Panics` or interruption/partial-cleanup contract. These items are selected
  by the storage lanes, not dormant code. Rustdoc applies to tests by explicit
  repository rule even where rustdoc tooling does not compile external test
  binaries.
- **Observable consequence:** the candidate violates a named hard blocker and
  leaves maintainers without the interruption/cleanup invariant for durable
  cloud tests.
- **Decision-complete correction:** add concise complete rustdoc to the Rust
  helpers/tests materially changed by this candidate. Document purpose,
  workflow role, panic conditions,
  and cancellation/partial durable effects; add no documentation abstraction
  or new checker.
- **Focused closure proof:** source-audit every retained Rust item in
  `bbfdf35e2..<corrected-candidate>`, then run `mise run fmt`, `mise run lints`,
  and the affected storage lanes.

### `FIND-TASK-003-R1-6` — CONFIRMED — MISSING: broad proof is not tied to one immutable corrected candidate

- **Wave 1 sources:** `TASK-REV-R1-03`, `RI-R1-04`.
- **Violated obligation:** `TASK-003-R1`'s broader-verification section and
  revision-9 `REQ-064`, `INV-025`, and `AC-022` require the broad gate, focused
  capabilities, and gated journeys to be recorded against one immutable final
  candidate.
- **Location:** remediation evidence sections `Broader verification on
  8e266b2b4` and `Storage configuration consolidated onto one variable
  (9c52f8875)`.
- **Reachability and evidence:** the reported `gate` and full journey matrix ran
  at `8e266b2b4` with uncommitted storage content whose tree identity was not
  recorded. The final candidate added 21 storage/workflow/mise/docs paths and
  records only a smaller storage/check subset. A prose assertion of identical
  dirty content cannot identify the tested tree.
- **Observable consequence:** the claimed 6,025-test result cannot prove the
  reviewed candidate and may omit a candidate-only regression.
- **Decision-complete correction:** after all source corrections, commit one
  final candidate and run every command in `TASK-003-R1` broader verification,
  every named focused capability, and every gated `TASK-003` journey on that
  exact tree. Record command, nonzero selection, result, commit, and tree; do
  not create a new aggregate or rerun substitute.
- **Focused closure proof:** the evidence record itself, plus final cumulative
  `git diff --check` against the established base.

### `FIND-TASK-003-R1-7` — REVISED — MISSING: local real-cloud candidate proof is absent

- **Wave 1 sources:** `TASK-REV-R1-04`, `RI-R1-05`.
- **Owner-directed proof rule:** run the existing real-cloud tasks from the
  local ignored `mise.local.toml` against the corrected candidate; the GitHub
  Actions workflow remains merge-to-`main` only. Revision-9 `REQ-064` and
  `AC-022` accept this local proof without a GitHub Actions result on the
  candidate.
- **Location:** `mise.local.toml:4-17`, the three `test:storage:*:cloud` owners
  in `mise.toml`, and remediation `Live-cloud evidence`.
- **Reachability and evidence:** `mise.local.toml` defines
  `storage:s3:dev`, `storage:gcs:dev`, and `storage:azure:dev`; each sets a
  concrete `WYRD_STORAGE_URL` and invokes an existing real-cloud test task.
  The file is local and ignored, so GitHub Actions cannot invoke it. The
  implementation record lacks exact local command, nonzero selection, result,
  and candidate tree for these runs.
- **Observable consequence:** the candidate lacks auditable real-provider
  S3/GCS/Azure proof before merge.
- **Decision-complete correction:** on one clean corrected candidate, run
  `mise run storage:s3:dev`, `mise run storage:gcs:dev`, and
  `mise run storage:azure:dev` locally with the owner's cloud credentials.
  Record command, selected tests, result, commit, and tree. Keep
  `mise.local.toml` ignored; do not add manual dispatch or expose credentials
  to pull requests.
- **Focused closure proof:** three passing local command records with nonzero
  test selection on the corrected immutable candidate.

### `FIND-TASK-003-R1-8` — CONFIRMED — INCORRECT: client transfer helpers inherit an unrelated 30-second total timeout

- **Obligation:** `REQ-056` requires terminal-safe query streaming and dropped
  callers to settle; the storage client journey must permit a healthy slow
  transfer. Neither query execution nor artifact transfer is an ordinary JSON
  control-request deadline.
- **Location:** `crates/shared/wyrd-client/src/transport/http.rs:67-106,195-303,360-397,570-710`,
  `src/transport/config.rs:125-155`, and `tests/transport/http.rs:789-837`.
- **Reachability and evidence:** `HttpTransport::new` builds a total-timeout
  `client` and a connect-only `stream_client`. `request_raw`,
  `request_stream`, and `request_external_stream` all use the total-timeout
  client. Their production callers are LocalFs GET/PUT and S3/GCS/Azure
  presigned transfers. `request_json_stream_inner` alone uses the connect-only
  client. `send_with_retry` serves JSON and idempotent control calls. The
  existing delayed-query test treats `request_raw` as a bounded control call,
  but it is a streaming response helper. `request_arrow` uses
  `send_with_retry`; no production caller of that public method was found, so
  it is not a reason to change or remove the API here.
- **Observable consequence:** a healthy slow download body or upload can fail
  at `HttpConfig.timeout_ms` (30 seconds by default) even while its connection
  is active. The existing terminal query path does not suffer this client
  timeout, but two clients and the mismatched raw helper conceal the defect.
- **Decision-complete correction:** build one `reqwest::Client` inside
  `HttpTransport` using `connect_timeout(config.timeout_ms)` and preserve its
  existing TLS/compression settings. Remove `stream_client` and
  `HttpClientDeadline`; retain `timeout_ms` on the transport and apply
  `RequestBuilder::timeout(Duration::from_millis(config.timeout_ms))` on each attempt inside
  `send_with_retry`, so JSON, Card, Bifrost control, storage-plan/completion,
  and the unchanged public `request_arrow` path retain a finite wait through
  their response bodies. Use the connect-bounded client with no total deadline
  for `request_json_stream_inner`, `request_stream`, `request_raw`, and
  `request_external_stream`. Keep authenticated same-origin enforcement,
  credential-free presigned requests, replay-safe retry and one-shot rules,
  request identity, and drop/cancellation behavior. Update `HttpConfig.timeout_ms`
  rustdoc to state its two uses: connection establishment for all calls and
  total request/response duration only for `send_with_retry` calls. The
  independent `AuthMiddleware` client is outside this work. Do not remove
  `request_arrow` without a separate public-API decision.
- **Focused closure proof:** change the delayed-query test's bounded leg to
  `request_json` and assert its delayed JSON body times out. Prove the query
  stream outlives the same short timeout. Add slow GET and slow PUT cases for
  both authenticated and external streaming helpers; the PUT fixture must
  consume the *entire delayed request body* before responding, and assert the
  sent bytes and expected authentication/credential omission. Run exact named
  tests, `mise run test:shared`, and `mise run codegen:check` because the
  config description changes.

### `FIND-TASK-003-R1-9` — CONFIRMED — INCORRECT: the generic server edge can preempt an Oracle query deadline

- **Obligation:** the accepted public query deadline must govern Oracle
  preparation, first-batch wait, and query completion; protected HTTP ingress
  and pre-Oracle work must remain bounded and default-deny. This does not
  change `BifrostQueryRequest.deadline_ms` or its public validation.
- **Location:** `crates/wyrd/wyrd-server/src/http/router.rs:37-141`,
  `src/http/middleware/body_limit.rs:95-165`, `src/query/routes.rs:230-273`,
  `src/query/service.rs:250-272`, and `src/oracle/forwarding.rs:133-152`.
- **Reachability and evidence:** the protected stack's 30-second
  `TimeoutLayer` wraps body collection, default-deny authentication, the
  query handler, and its awaited `service::stream_query`. The forwarder
  captures the request's Oracle deadline only after validation and capability
  admission; the handler waits for Oracle preparation and the first batch
  before constructing a response. The generic timer can therefore return
  `WYRD_SERVER_504_REQUEST_TIMEOUT` before a valid longer Oracle deadline and
  hide the typed query outcome. Once response headers exist, the current
  Tower timeout does *not* cover the terminal body stream; the conflicting
  interval is the pre-response wait.
- **Observable consequence:** a query with an explicit deadline beyond the
  edge limit can fail at the generic edge while Oracle is still within its
  authorized execution window. Simply exempting `/v1/query` from the edge
  would instead let a slow body or stalled authentication hold a concurrency
  slot without limit.
- **Decision-complete correction:** give only `POST /v1/query` a staged
  timeout path. Keep the existing request-ID, panic, load-shed, concurrency,
  body-size, default-deny authentication, and error-mapping layers. Bound
  body collection, authentication, and route capability admission by the
  existing `state.limits.timeout` while the protected concurrency slot is held; reuse
  the current `WYRD_SERVER_504_REQUEST_TIMEOUT` mapping for that stage. After
  the Oracle forwarder captures the deadline, let Oracle's own deadline and
  cancellation guards own preparation, first-batch wait, and terminal stream;
  do not leave an overlapping generic edge timer. Keep the existing generic
  timeout for every other route, including query lifecycle routes. Implement
  the split at the existing protected/query ownership boundary, without a
  second unprotected query route, global timeout increase, early success
  headers, or a new public deadline parameter. Preserve typed Oracle timeout
  and resource cleanup on caller cancellation.
- **Focused closure proof:** extend the server test fixture, if necessary,
  only to bind the existing request-ID-scoped `OraclePreparationPause` to an
  in-process Oracle; `with_limits_for_test` already sets a short edge limit.
  Send a query whose explicit Oracle deadline exceeds that limit, observe
  the post-pin pause, hold past the edge limit, release before Oracle expiry,
  and require a successful terminal result. In a second query, hold past the
  Oracle deadline and require `WYRD_VALA_504_QUERY_TIMEOUT`. Confirm a slow
  body or stalled pre-Oracle operation still gets the edge timeout, and an
  ordinary non-query route retains it. No 30-second sleep is required. Run
  exact focused tests and `mise run test:bifrost:journey:server`.

## Prior client closure

The original Wave 1 client/SDK/public-contract ledger was empty. The direct
deadline addendum now identifies one client transport issue, not a reopening
of `FIND-TASK-002-19`. That finding, `FIND-TASK-004-1`, and
`FIND-TASK-003-1` through `FIND-TASK-003-3` remain closed by their existing
source corrections and focused evidence.

## Owner-requested storage alignment follow-up

Keep the `WYRD_STORAGE_URL`/`WYRD_STORAGE_ENDPOINT_URL` structure. Scan all
GitHub Actions workflows and local actions for storage configuration usage;
check every storage-selecting workflow against its matching `mise` owner:
backend URL, optional endpoint, provider credentials, feature selection, and
nonzero tests. Correct only mismatches found; keep cloud
credentials out of pull requests. Update Wyrd's self-hosting storage and
configuration documentation to describe the final URL, endpoint, credential,
and merge-to-`main` cloud CI plus local pre-merge proof accurately. The existing pages already use the new variables,
so this is a consistency closeout, not a second storage configuration change.
Proof: exact live-tree search for retired storage variables in workflows and
docs, `mise run check:ci-selection`, `mise run docs:check`, and the owning
emulator/cloud evidence already required below.

## Recommendations and overall implication

Apply the seven retained corrections and the small storage-alignment follow-up
in their existing owners. The delete/reuse/native ladder rejects a new test
target, CI workflow, or verification aggregate. The stale Oracle documentation
conflict remains explicitly outside this remediation.

**Overall implication: `FIX_REQUIRED`.** Seven bounded findings remain under
approved revision 9. Local cloud proof through the existing `mise.local.toml`
tasks can close acceptance; post-merge GitHub Actions results are not a
pre-merge gate.
