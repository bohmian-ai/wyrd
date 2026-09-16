---
id: TASK-003-R2
kind: remediation
status: ready
spec: SPEC-surfaces-oracle-integration
spec_revision: 9
requirements: [REQ-003, REQ-033, REQ-034, REQ-035, REQ-035A, REQ-064, INV-025, AC-008, AC-022]
depends_on: [TASK-003-R1]
parent_task: TASK-003
remediates: [FIND-TASK-003-R1-2, FIND-TASK-003-R1-3, FIND-TASK-003-R1-4, FIND-TASK-003-R1-6, FIND-TASK-003-R1-7, FIND-TASK-003-R1-8, FIND-TASK-003-R1-9]
---

# Close TASK-003-R1 review findings

Implementation route: `$wyrd-implement`.

## Authority and candidate

- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 9
- Original tasks:
  - `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
  - `changes/active/surfaces-oracle-integration/tasks/TASK-004-unify-bifrost-data-root.md`
  - `changes/active/surfaces-oracle-integration/tasks/TASK-003-close-repository-integration.md`
- Parent remediation:
  `changes/active/surfaces-oracle-integration/review/tasks-002-r3-004-003-bbfdf35e-review-01/TASK-003-R1-close-cumulative-review-findings.md`
- Review verdict and validated ledger:
  `changes/active/surfaces-oracle-integration/review/task-003-r1-9c52f8875-review-01/{verdict,findings-validation}.md`
- Cumulative base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Reviewed candidate: `9c52f887595d4e8a04764d3d2836926e5df70950`
- Reviewed candidate tree: `e2731ff2d23fc1a0f4b356ccdd79caf3f6c7b93e`

Owner decision, 2026-09-16: no storage configuration has been deployed; keep
the tested `WYRD_STORAGE_URL` structure. `FIND-TASK-003-R1-1` is removed from
the final ledger. The review's historical Wave 1 reports are not rewritten.
The owner also directs real-cloud GitHub Actions to run on merges to `main`,
not weekly, and directs candidate cloud proof through the local ignored
`mise.local.toml`. Approved revision 9 records the owner's explicit override:
passing those local S3, GCS, and Azure tasks on the corrected candidate is
sufficient pre-merge cloud proof. Post-merge GitHub Actions results are not an
acceptance gate. This task is ready for implementation.

## Outcome

Keep the new storage configuration and align GitHub Actions and Wyrd
documentation with it. Make every surviving storage test reachable, preserve
the original Forge recovery assertion, satisfy Rust documentation rules, and
produce both local and live-cloud evidence against one immutable corrected
candidate. Separately correct the client transfer deadline and server query
edge timeout without changing the public query deadline contract.

## Issue diagnoses and required corrections

### `FIND-TASK-003-R1-2` — two storage tests are unreachable

`local_handle_crud` requires the `emulator` feature, but every emulator-backed
handle lane filters to a cloud-named test. The `cloud`-gated
`backend_contracts` binary is selected by no mise task. Its Azure abort case
overlaps the owned Azurite test but uniquely proves dispatch through
`BackendSigner`, so simply deleting it would lose coverage.

Add `local_handle_crud` as an exact selection under the existing
emulator-backed handle owner. Preserve the unique `BackendSigner` Azure-abort
dispatch assertion in the existing Azurite missing-blob test, then delete the
redundant `backend_contracts` file and manifest target. Add no new task, test
binary, checker, feature, or dependency.

### `FIND-TASK-003-R1-3` — Forge recovery proof was weakened

`acceptance_unknown_recovers_from_durable_state` formerly accepted only the
checkpoint shutdown outcome. The candidate also accepts any returned error
containing `refused before commit: Cancelled`, even though rare
settlement-cancellation work is an explicit non-goal and tests may not be
weakened to obtain a green gate. The publication-authority refusal is reachable
when worker stop races the recovered attempt.

Restore the original shutdown-only assertion. Reuse the existing Forge worker
observer/lifecycle barrier so the recovered attempt settles before worker stop.
Do not change production cancellation or publication semantics and do not
broaden accepted error text.

### `FIND-TASK-003-R1-4` — retained Rust test workflows lack required rustdoc

The materially rewritten storage server journey and retained storage
helpers/tests are executable lane owners but omit documentation required by
`AGENTS.md` and `architecture/agent-rules.md`, including panic and
interruption/partial-durable-effect behavior. This is a hard repository rule,
not optional polish.

With the chosen storage configuration retained, add concise complete rustdoc
to Rust helpers and tests materially changed by this remediation.
Document intent, workflow role, panic conditions, and async cancellation or
partial durable effects where applicable. Add no documentation framework or
checker.

### `FIND-TASK-003-R1-6` — local proof is not tied to one immutable candidate

The recorded broad gate and full journey matrix ran at `8e266b2b4` while the
storage changes were uncommitted and had no recorded tree identity. The final
candidate then changed storage, workflows, mise, and docs but records only a
smaller subset. A prose assertion that dirty content was identical cannot prove
the immutable reviewed tree.

After every source correction, commit one final candidate and run the complete
TASK-003-R1 broader-verification matrix, every named focused capability, and
every gated TASK-003 journey on that exact commit and tree. Record command,
nonzero selection, result, commit, and tree. Reuse the existing tasks; add no
aggregate or substitute run.

### `FIND-TASK-003-R1-7` — local real-cloud candidate proof is absent

The owner directs pre-merge cloud proof through the existing ignored
`mise.local.toml`, not candidate-SHA GitHub Actions jobs. Its
`storage:s3:dev`, `storage:gcs:dev`, and `storage:azure:dev` tasks already set
concrete `WYRD_STORAGE_URL` values and invoke the owning real-cloud test lanes.
The implementation record has an owner assertion but no exact local commands,
nonzero test selections, results, or candidate tree. Emulator results cannot
substitute for real-provider behavior.

On one clean corrected candidate, run `mise run storage:s3:dev`,
`mise run storage:gcs:dev`, and `mise run storage:azure:dev` locally with the
owner's cloud credentials. Record each command, selected test count, result,
commit, and tree. Keep `mise.local.toml` ignored; do not add manual dispatch,
another cloud workflow, or a PR-secret path. The existing cloud Actions jobs
remain merge-to-`main` checks, not a weekly cadence or pre-merge proof.

### `FIND-TASK-003-R1-8` — client transport total timeout cuts off transfers

`HttpTransport` currently builds two Reqwest clients: the ordinary client has
`HttpConfig.timeout_ms` as a total request deadline and the terminal query
client bounds only connection establishment. Authenticated LocalFs GET/PUT and
external S3/GCS/Azure GET/PUT use the ordinary client, so a healthy transfer
can fail at 30 seconds. `request_raw` is a streaming response helper, not a
bounded JSON control operation.

Build one client in `HttpTransport` with
`connect_timeout(config.timeout_ms)`, preserving TLS and compression. Remove
`stream_client` and `HttpClientDeadline`; store the configured duration and
apply `RequestBuilder::timeout(Duration::from_millis(config.timeout_ms))` only on each
`send_with_retry` request. Its JSON and idempotent control callers—including
Card, Bifrost control, and storage plan/completion—must retain a finite
header-and-body wait. `request_arrow` currently has no production caller; it
also uses `send_with_retry`, but leave its public API unchanged and do not use
the obsolete raw-Arrow path as the justification for this control timeout.
Let `request_json_stream_inner`, `request_stream`, `request_raw`, and
`request_external_stream` use the one connect-bounded client without a total
transfer deadline. Keep same-origin authentication, no credentials on external
presigned requests, retry and one-shot semantics, request IDs, and
cancellation behavior. `AuthMiddleware` owns a separate client and is not
part of the one-client requirement. Update `HttpConfig.timeout_ms` rustdoc to
name its two uses accurately; do not change its serialized field or default.

In `tests/transport/http.rs`, switch the delayed-query test's bounded leg
from `request_raw` to `request_json` and assert the delayed JSON body times
out. Add slow GET and PUT cases for both authenticated and external streaming
helpers. Make each PUT server fixture consume the entire delayed request body
before responding; assert bytes received, credentials present only on the
same-origin path, and successful transfer beyond the configured short
duration. Query streaming must also outlive that duration. Keep this a
client-only correction; do not add another transport abstraction or timeout
configuration.

### `FIND-TASK-003-R1-9` — generic server timeout preempts Oracle preparation

The protected edge's `TimeoutLayer` covers the `/v1/query` handler, which
awaits `service::stream_query` through Oracle preparation and first-batch
availability before returning response headers. The forwarder captures the
requested Oracle deadline after route admission. Thus a valid query with a
longer deadline can receive generic `WYRD_SERVER_504_REQUEST_TIMEOUT` while
Oracle is still within its execution window. Tower's timer does not cover
the body after headers; this correction targets the pre-response wait.

Give only `POST /v1/query` a staged timeout path at the protected edge/query
boundary. Preserve request-ID propagation, default-deny authentication, panic
handling, load shedding, concurrency admission, body-size enforcement, and
existing error mapping. While the protected concurrency slot is held, bound
body collection, authentication, and query capability admission by the
existing `state.limits.timeout`, using the same generic request-timeout
problem on expiry. After `ReadyOracleForwarder` captures the query deadline, the generic
edge timer must no longer race Oracle preparation, first-batch wait, or the
terminal stream. Oracle remains responsible for typed timeout and
cancellation/resource cleanup. Keep the existing timeout unchanged for all
other paths, including query lifecycle routes. Do not globally raise the
limit, bypass any protective layer, publish success headers before existing
pre-stream decisions, or introduce another public deadline.

Use `with_limits_for_test` to set an edge timeout shorter than an explicit
query deadline. Reuse the request-ID-scoped `OraclePreparationPause`; expose
only a narrow test-support binding method from the in-process fixture if
needed. Hold preparation beyond the edge limit, release before Oracle expiry,
and require a successful terminal result. Hold a second query beyond Oracle
expiry and require typed `WYRD_VALA_504_QUERY_TIMEOUT`. Assert a slow request
body or stalled pre-Oracle operation still receives the generic edge timeout,
and an ordinary non-query route keeps its edge timeout. Use short controlled
durations, not a real 30-second sleep.

## Small storage-alignment follow-up

The owner keeps `WYRD_STORAGE_URL` and `WYRD_STORAGE_ENDPOINT_URL`; the
implementation already passes these values in the storage emulator and
credentialed-cloud workflows and describes them in self-hosting pages. Before
closeout, scan every GitHub Actions workflow and local action for storage
configuration usage, then check every storage-selecting path against its
`mise` task and the final configuration: backend URL, optional endpoint,
provider credentials, feature selection, and nonzero tests. Correct any
mismatch in the existing workflow or task owner, without adding a parallel
workflow, runner, or configuration shape. Update Wyrd's self-hosting storage
and configuration documentation to describe the final URL, endpoint,
credential, and CI behavior accurately. In particular, remove stale claims
about cloud workflow triggers or backend setup: document merge-to-`main`
Actions and local pre-merge cloud commands. This is a documentation and workflow consistency
pass, not authority to redesign storage again.

## Constraints and preserved behavior

- Preserve all source corrections that closed `FIND-TASK-002-19`,
  `FIND-TASK-004-1`, and `FIND-TASK-003-1` through `FIND-TASK-003-3`.
- Preserve `wyrd-client::Bifrost` as the sole client owner, the three thin SDKs,
  exactly three runtime MCP tools, the single locked data root, bounded Oracle
  audit work, canonical `vala.audit_staging`, and tenant isolation.
- Preserve generic Rust PR ownership in the existing Linux `ci` job and the
  seven-entry nightly matrix.
- Preserve the selected `WYRD_STORAGE_URL`/`WYRD_STORAGE_ENDPOINT_URL`
  configuration and its passing emulator/cloud behavior; do not add legacy
  aliases or another replacement contract.
- Preserve Forge production cancellation, publication, and recovery behavior.
- Preserve the public query deadline schema, validation, and terminal-frame
  contract; do not remove `request_arrow` in this remediation.
- Do not claim these deadline corrections solve every storage-duration limit:
  Azure's single SAS may expire during later blocks, and the LocalFs server
  PUT still has separate buffering, body-size, and route-timeout constraints.
- Do not weaken, skip, ignore, or allowlist around any test or gate.
- Do not expand into the expressly excluded Oracle audit-WAL/relay authority
  documentation sweep.
- Do not merge, push to `main`, release, deploy, or perform final change review.

## Acceptance criteria

| Criterion | Finding |
|---|---|
| The local handle test executes under an existing owner; the unique Azure signer-dispatch assertion executes in the Azurite owner; no orphan `backend_contracts` target remains. | `FIND-TASK-003-R1-2` |
| The Forge recovery journey retains its original shutdown-only assertion and passes after the recovered attempt settles before stop, with no production semantic change. | `FIND-TASK-003-R1-3` |
| Every retained materially changed Rust helper/test has complete required rustdoc, including panic and interruption/partial-effect notes where applicable. | `FIND-TASK-003-R1-4` |
| The existing live-cloud workflow runs after merges to `main` without a weekly cron or manual dispatch, preserving its three jobs and OIDC permissions. | Approved revision-9 cloud workflow outcome |
| One immutable corrected commit/tree has passing, nonzero results for the full prescribed local matrix and all three local `mise.local.toml` real-cloud tasks. | `FIND-TASK-003-R1-6`, `FIND-TASK-003-R1-7` |
| One `HttpTransport` client bounds connects; only `send_with_retry` applies the configured total timeout. JSON bodies time out, but query and authenticated/external slow GET/PUT transfers outlive it without changing auth or retry behavior. | `FIND-TASK-003-R1-8` |
| Query ingress, authentication, and capability admission remain edge-bounded; Oracle preparation can exceed the edge limit but not its own deadline; non-query routes retain the generic timeout and typed query errors survive. | `FIND-TASK-003-R1-9` |

The owner-requested follow-up is complete when every GitHub Actions
storage path uses the final URL/endpoint contract through its owning `mise`
task, Wyrd's self-hosting storage/configuration docs match that contract and
the actual merge-to-`main` cloud workflow trigger and local cloud commands, and neither workflows nor docs retain
the retired storage variables.

## Focused proof

1. Run settings tests for every URL-selected backend, each existing storage
   emulator/handle lane with the selected variables, the exact local handle
   test, the Azurite missing-blob test carrying the signer-dispatch assertion,
   and `mise run test:storage:matrix`. Record nonzero counts and prove the
   orphan target is absent. Search live workflows and docs for retired storage
   variables and check their task/trigger descriptions against actual owners.
2. Run the exact Forge
   `acceptance_unknown_recovers_from_durable_state` test repeatedly with its
   original assertion.
3. Run `mise run fmt`, `mise run lints`, `mise run docs:check`, and
   `mise run check:ci-selection`.
4. Run exact named client transport tests and server deadline tests through
   `mise exec -- cargo nextest run --locked` with explicit package, target,
   and exact expressions; run `mise run test:shared` and
   `mise run test:bifrost:journey:server`, and `mise run codegen:check` for
   the changed config description. Record nonzero selection.
5. On the final immutable commit/tree, run every focused capability, broader
   check, `gate`, and gated journey named by TASK-003-R1. Record exact commands,
   selections, and results.
6. On that final SHA, run `mise run storage:s3:dev`,
   `mise run storage:gcs:dev`, and `mise run storage:azure:dev` locally. Record
   each command, nonzero selection, result, commit, and tree.

## Broader verification

Run the complete verification set required by TASK-003-R1 and `AGENTS.md` for
the touched Rust, storage, documentation, CI, generated-contract, Bifrost, and
journey surfaces. Finish with
`git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..<corrected-candidate>`.
All evidence must identify the same final commit and tree.
