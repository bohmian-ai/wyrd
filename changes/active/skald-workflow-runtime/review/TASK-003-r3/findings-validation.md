# Independent findings validation — TASK-003 r3

## Subject and evidence boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `0732ed92fc2907cb806ac918b09abbf6d44ff04f`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 12, including the human-approved 2026-10-03 addition of
  `pub model: ModelRef` to `WyrdGatewayCall`
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior remediations: `changes/active/skald-workflow-runtime/review/TASK-003-r1/`
  and `changes/active/skald-workflow-runtime/review/TASK-003-r2/`, including
  the human-approved same-spec correction of the native-`401` wording

This fresh validation read the complete base-to-candidate diff, current source,
applicable repository authority, the original and remediation tasks, both prior
validated ledgers, and every r3 discovery and follow-up report. It traced each
proposed failure from producer through live callers and sibling consumers.
Agreement among reviewers was treated as corroboration rather than proof.

The review was strictly source-only. No build, compile, test, Cargo, mise, pnpm,
pytest, formatter, linter, package-manager, test-listing, or other verification
command was run. Implementer evidence is accepted only for the exact source path
it exercises; missing or source-contradicted evidence is retained as a finding.
The candidate identity remained unchanged during validation.

The standing DRIFT direction was applied throughout. No correction below adds a
new mechanism, check, file, setting, option, dependency, compatibility path,
allowlist, cache, watcher, detached worker, or generalized test harness.

## Disposition of every proposed finding

| Discovery source IDs | Disposition | Final ID | Source-backed decision |
|---|---|---|---|
| `STD-R3-001`, `SYS-R3-001`, `SEC-R3-001`, `CONC-R3-001`, `FU-R3-001` | **REVISED** | `FIND-TASK-003-2` | The prior native-`401` invariant reopens: status is known before body collection, but renewal is not attempted until the body completes. The stronger system proposal to add cancellation-safe cache retirement is rejected as an unapproved concurrency mechanism; reordering the existing refresh before body collection is sufficient. |
| `SYS-R3-002`, `FU-R3-002` | **REVISED** | `FIND-TASK-003-10` | Task-added local execution performs synchronous ambient configuration IO on an async executor. Validation extends the trace to the sibling client-less `wyrd_gateway` path, which reaches the same config read and synchronous credential resolution through `WyrdClient::from_global`. |
| `STD-R3-002`, `FU-R3-003` | **CONFIRMED** | `FIND-TASK-003-11` | TASK-003 adds a non-retrying production consumer of `force_refresh`, making the auth owner's unconditional “then retries once” workflow documentation false. |
| `STD-R3-003`, `FU-R3-004` | **CONFIRMED** | `FIND-TASK-003-12` | `Workflow` now owns material client context, so consuming it through `into_skald` discards execution authority/composition state that the unchanged one-line rustdoc does not disclose. |

`task-review-behavior.md`, `task-review-invariants.md`, and
`maintainer-review.md` proposed no findings. Their PASS conclusions do not
override the source-established findings above. No other discovery proposal
survives validation.

## Validated finding ledger

### FIND-TASK-003-2 — attempt native renewal before response-body collection can postpone it

- **Status:** REVISED (prior stable finding reopened)
- **Classification:** INCORRECT
- **Discovery sources:** `STD-R3-001`, `SYS-R3-001`, `SEC-R3-001`,
  `CONC-R3-001`, `FU-R3-001`
- **Violated obligation:** The approved R1 task correction and TASK-003-R2
  require one native model POST and, once its response is known to be `401`, an
  attempt through the existing `AuthMiddleware::force_refresh` owner without
  replay. Renewal failure is authoritative; successful renewal preserves the
  original refusal or the existing body-read failure.
- **Exact locations:**
  `crates/shared/wyrd-client/src/transport/http.rs:398-410`, reached through
  `crates/shared/wyrd-client/src/workflow/gateway.rs:96-116`; incomplete proof
  at `crates/shared/wyrd-client/tests/workflow_transport.rs:1039-1062`.
- **Producer-to-consumer evidence:** `request.send().await` has returned and
  `response.status()` has produced `UNAUTHORIZED` at line 402. Line 403 then
  awaits the complete response body before lines 404-408 begin
  `force_refresh`. A native endpoint can send `401` headers with a declared
  body and leave that body pending. The sole production caller races the whole
  `post_native` future against cancellation and `call.timeout`; either branch
  can drop it while body collection is pending, before refresh is polled. The
  old cached exchanged token remains available to the next `bearer()` call.
  Sibling replay-safe JSON/framed and gRPC transports have separate retry
  owners and are not part of this correction.
- **Evidence assessment:** The recorded R2 raw response closes its connection
  immediately after a truncated body. That makes `response.bytes()` finish
  with an error, after which refresh runs. It does not exercise a response body
  that remains pending after the already-observed `401`, so it cannot support
  the implementation claim that renewal follows every observed `401`.
- **Observable consequence:** A call can receive a native `401`, expire or be
  cancelled while waiting for its body, and leave a known-refused cached token
  available to a later call. The model request is still sent once, but the
  approved preparation of later calls is skipped.
- **Decision-complete minimum correction:** In the existing
  `HttpTransport::post_native` owner, branch on the known `401` status and
  attempt the existing `force_refresh` before response-body collection can
  postpone it. Preserve one model POST, no replay, renewal-error precedence,
  and the original status/body or existing body-read failure after successful
  renewal. Preserve the outer caller's established cancellation boundary; do
  not add cache retirement, a detached refresh task, response provenance,
  retry configuration, a new transport, or another auth owner.
- **Focused closure proof:** Extend the existing focused deterministic
  loopback case so `401` headers arrive while the body remains pending. Record
  that the existing auth renewal path is reached before the body is released
  or closed, exactly one model POST is observed, and the refused call is never
  resent. Retain the complete-body renewal-success/failure and immediate
  cut-off cases, and rerun the existing
  `public_gateway_call_context_and_errors` selector. Extending its existing raw
  HTTP fixture is sufficient; a new harness or check would be DRIFT.

### FIND-TASK-003-10 — keep all task-added ambient configuration reads off the async executor

- **Status:** REVISED
- **Classification:** VIOLATION
- **Discovery sources:** `SYS-R3-002`, `FU-R3-002`
- **Violated obligation:** `AGENTS.md` section 6 prohibits blocking filesystem
  work in async paths without an explicit blocking strategy. TASK-003 requires
  the shared async Workflow owner to prepare selected local dependencies for
  Rust, Python, TypeScript, and CLI callers.
- **Exact locations:** `crates/shared/wyrd-client/src/workflow/mod.rs:127-150`,
  with synchronous IO at `crates/shared/wyrd-client/src/global_config.rs:71-105`,
  indirect client configuration at `crates/shared/wyrd-client/src/client.rs:48-54`
  and `crates/shared/wyrd-client/src/config.rs:91-98`, and synchronous ambient
  credential-file resolution reachable from client assembly.
- **Producer-to-consumer evidence:** Every language surface delegates local
  execution to async `Workflow::run`/`run_with`. A selected `ext_gateway` route
  calls `GlobalConfig::load` directly at `workflow/mod.rs:133-135`, whose
  `load_from` performs `std::fs::read_to_string`. A client-less selected
  `wyrd_gateway` route takes the sibling branch at lines 141-146 and calls
  `WyrdClient::from_global`; that calls `ClientConfig::from_global`, reaches the
  same synchronous config load, and can synchronously resolve file-backed
  ambient credentials while assembling the client. These calls execute when
  the async future is polled, before the dependency-preparation await. The same
  module already uses `tokio::task::spawn_blocking` for authored bundle IO, and
  `workflow/local.rs:115-130` uses it for selected secret files.
- **Observable consequence:** A slow home, mounted configuration directory, or
  credential file blocks a current-thread runtime or occupies a shared Tokio
  worker, delaying cancellation and unrelated SDK work before the Workflow
  reaches its bounded network/runtime operations.
- **Decision-complete minimum correction:** At the existing shared
  `Workflow::run_with` preparation owner, put the complete task-added ambient
  configuration/client assembly that may touch the filesystem on the
  established Tokio blocking pool. Load one ambient `GlobalConfig` snapshot
  when selected routes require it, reuse the existing `GlobalConfig`,
  `ClientConfig`, and `WyrdClient` constructors to derive the workflow config
  and any client-less public-gateway client, and preserve lazy route selection,
  retained Cards clients, current public errors, and selected-secret-only
  behavior. Do not add an async config API, cache, watcher, setting, option,
  dependency, or per-language implementation.
- **Focused closure proof:** Static source review must show that neither the
  selected external-binding config read nor client-less public-gateway ambient
  client assembly can perform filesystem IO on the polling thread. Rerun and
  record the existing selected-local-dependencies proof and the existing
  retained-client/language coverage. No timing test, synthetic slow filesystem,
  new harness, or permanent check is warranted.

### FIND-TASK-003-11 — correct the authentication owner's retry contract for native send-once callers

- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Discovery sources:** `STD-R3-002`, `FU-R3-003`
- **Violated obligation:** `AGENTS.md` section 16 and
  `architecture/agent-rules.md` require materially affected Rust documentation
  to describe workflow role, side effects, and retry behavior accurately.
- **Exact location:** `crates/shared/wyrd-client/src/auth.rs:503-507`; new
  consumer at `crates/shared/wyrd-client/src/transport/http.rs:404-408`.
- **Producer-to-consumer evidence:** `AuthMiddleware::force_refresh` says the
  reactive HTTP/gRPC path calls it after rejection and then retries once.
  Replay-safe HTTP and gRPC consumers do retry. TASK-003 adds the live native
  HTTP consumer `post_native`, which must call `force_refresh` only to prepare a
  later call and must return the current refusal without replaying its
  non-idempotent model POST. The method body need not change for its owner
  documentation to have become materially false when this new consumer was
  added.
- **Observable consequence:** A maintainer following the auth owner's stated
  contract can incorrectly restore replay to the native model path or
  misunderstand why successful native refresh cannot make the current call
  succeed.
- **Decision-complete minimum correction:** Qualify the existing
  `force_refresh` rustdoc: replay-safe HTTP/gRPC operations may retry after
  refresh, while the native model POST refreshes only for later calls and
  returns the original refusal. Change no runtime behavior and add no retry,
  policy type, option, helper, or enforcement mechanism.
- **Focused closure proof:** Static review of the corrected owner documentation
  plus the repository's ordinary documentation/lint evidence. A runtime test or
  bespoke documentation check is unnecessary and would be DRIFT.

### FIND-TASK-003-12 — document the client context discarded by `Workflow::into_skald`

- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Discovery sources:** `STD-R3-003`, `FU-R3-004`
- **Violated obligation:** `AGENTS.md` section 16 requires materially modified
  Rust APIs to document maintainer-relevant workflow roles, invariants, and side
  effects. TASK-003 and its R1 remediation make retained Cards/client context a
  required execution invariant.
- **Exact locations:** `crates/shared/wyrd-client/src/workflow/mod.rs:34-41`
  and `:156-174`; direct callers at
  `crates/shared/wyrd-client/src/workflow/mod.rs:660` and
  `crates/wyrd/wyrd-server/tests/pg_workflow_registration.rs:747-770`.
- **Producer-to-consumer evidence:** Before TASK-003, the facade contained only
  the Skald Workflow, so `into_skald` discarded no additional execution state.
  The candidate adds `client: Option<WyrdClient>` and makes it authoritative
  for automatic public-gateway dependency composition. `into_skald` still says
  only “Take the hydrated Skald Workflow” while consuming the wrapper and
  dropping that client, its endpoint/credential/token-cache/connection-pool
  context, and automatic shared dependency preparation. Current direct callers
  subsequently provide explicit execution dependencies, so no separate runtime
  behavior defect is established; the missing ownership warning is the defect.
- **Observable consequence:** A Rust or SDK maintainer can repeat the R1
  context-loss mistake by consuming a loaded facade and expecting later local
  execution to retain the loading client.
- **Decision-complete minimum correction:** Extend the existing method rustdoc
  to state that conversion intentionally discards retained client context and
  automatic shared dependency composition; callers needing those behaviors
  keep and run the shared `Workflow`, while callers taking the Skald value must
  provide explicit dependencies. Add no wrapper, compatibility path, new API,
  clone, test-only guard, or checker.
- **Focused closure proof:** Static review of the corrected rustdoc and current
  callers plus ordinary documentation/lint evidence. Runtime proof is not
  required for this documentation-only correction.

## Rejected and narrowed correction alternatives

### Cancellation-safe token retirement after a native `401`

`SYS-R3-001` proposed retiring the cached token at a cancellation-safe boundary
and then refreshing it. The approved R1/R2 outcome names the existing
`force_refresh` owner and requires the refresh attempt once `401` status is
known; the public caller separately promises that cancellation drops local IO.
No approved authority requires a cache-invalidating state transition that
survives arbitrary future cancellation. Adding one would change shared auth
concurrency semantics and affect sibling callers. The source defect closes by
moving the existing refresh attempt ahead of the potentially pending body read.
Cache retirement, a detached task, or shielding refresh from cancellation is
therefore rejected as DRIFT, not included as optional advice.

### New async configuration or proof infrastructure

The blocking-IO defect does not justify an async config API, long-lived config
cache, watcher, timing knob, mock filesystem, synthetic slow-filesystem test,
new lane, or checker. Wyrd already uses Tokio's blocking pool for synchronous
filesystem owners in this same Workflow path. Reuse that boundary and the
existing focused behavior evidence.

## Prior-finding closure

| Prior finding | Validation result |
|---|---|
| `FIND-TASK-003-1` | **CLOSED.** Python retains `wyrd_client::Workflow` through registered/authored-ref loading, mutation, and run; recorded evidence distinguishes the loading client from hostile or absent ambient configuration. |
| `FIND-TASK-003-2` | **REOPENED / REVISED.** Send-once behavior, renewal-error precedence, complete-body renewal, and immediate cut-off renewal are implemented, but a pending body can still postpone the required refresh attempt until the outer caller drops it. |
| `FIND-TASK-003-3` | **CLOSED.** Recognized codes use catalog title/remediation and provider-controlled message text is discarded. |
| `FIND-TASK-003-4` | **CLOSED.** The plaintext file-reading stage is private; cross-module consumers receive `SecretString` through `read_secret_ref`. |
| `FIND-TASK-003-5` | **CLOSED.** The r1 function-local imports remain removed. |
| `FIND-TASK-003-6` | **CLOSED.** The r2 changed declarations use module-imported bare or role-specific aliased types. |
| `FIND-TASK-003-7` | **CLOSED.** The r2 panic-capable items carry the required panic contracts. |
| `FIND-TASK-003-8` | **CLOSED.** The r2 durable operations document cancellation and partial progress. `FIND-TASK-003-11` and `-12` concern distinct owner contracts exposed by new consumers/state and therefore receive new IDs. |
| `FIND-TASK-003-9` | **CLOSED.** The recorded focused case cancels after model dispatch and observes no resend. |

## Complexity and correction assessment

The task's concrete handles and values have real owners and callers, and the
approved model amendment is not drift. No additional abstraction, dependency,
feature, configuration surface, compatibility path, or permanent check is
needed. The retained corrections are limited to ordering inside the existing
native transport, reuse of the established blocking boundary for the complete
ambient preparation path, and accurate documentation on two existing owners.

The independently validated ledger therefore contains four bounded findings:
`FIND-TASK-003-2`, `FIND-TASK-003-10`, `FIND-TASK-003-11`, and
`FIND-TASK-003-12`. None requires a new product, public API, architecture,
security, compatibility, cross-service, persistent-data, or concurrency
decision. The candidate requires bounded implementation remediation and
recorded focused evidence before TASK-003 can pass.
