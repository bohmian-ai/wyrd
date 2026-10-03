# TASK-003-r3 focused follow-up review

## Subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `0732ed92fc2907cb806ac918b09abbf6d44ff04f`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 12, including the human-approved 2026-10-03 addition of
  `pub model: ModelRef` to `WyrdGatewayCall`
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior remediations:
  `changes/active/skald-workflow-runtime/review/TASK-003-r1/TASK-003-R1-preserve-client-context-and-native-call-safety.md`
  and
  `changes/active/skald-workflow-runtime/review/TASK-003-r2/TASK-003-R2-close-renewal-proof-and-source-contracts.md`

This follow-up investigated only four conflicts or previously untraced paths.
It used the r3 discovery reports as claims to test, not as conclusions. The
candidate remained unchanged during the source inspection.

## Source and authority inspected

- `AGENTS.md`, especially the async rules at lines 270-284 and rustdoc rules at
  lines 712-730
- `architecture/agent-rules.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/maintainer-style.md`
- Revision 12 requirements `REQ-038`, `REQ-043`, `REQ-058`, `INV-007`,
  `INV-020`, and `AC-031`
- The complete base-to-candidate diff and commit history
- `crates/shared/wyrd-client/src/transport/http.rs`
- `crates/shared/wyrd-client/src/auth.rs`
- `crates/shared/wyrd-client/src/workflow/{mod,gateway,local}.rs`
- `crates/shared/wyrd-client/src/{global_config,config}.rs`
- `crates/shared/wyrd-client/tests/workflow_transport.rs`
- `sdks/wyrd-sdk-python/src/workflow.rs`
- `sdks/wyrd-sdk-ts/native/src/workflow.rs`
- The current callers of `Workflow::into_skald`
- The r1/r2 verdicts, validated ledgers, remediation tasks, and recorded
  implementation evidence

## Claim resolutions

### 1. Native `401` renewal can remain behind a pending response body

**Resolution: confirmed and narrowed.**

`HttpTransport::post_native` receives the response and records its status at
`crates/shared/wyrd-client/src/transport/http.rs:398-402`. It then awaits
`response.bytes()` at line 403 and does not enter the `401` branch or poll
`AuthMiddleware::force_refresh` until lines 404-408. The sole production caller
at `crates/shared/wyrd-client/src/workflow/gateway.rs:96-116` races that complete
future against the Workflow cancellation token and the call timeout. A native
endpoint can therefore send `401` headers with an unfinished body and keep
body collection pending until cancellation or timeout drops `post_native`.
Renewal has not started, and the cached refused credential remains available
to a later call.

This is a reachable production path. `reqwest::send` completes after the
response head is available; body collection is a later asynchronous operation.
`post_native` intentionally has no total deadline of its own, while the public
caller supplies the outer deadline and cancellation boundary.

The approved R1 wording says that on a native `401` the adapter renews the
refused credential through `AuthMiddleware::force_refresh`, never resends the
model call, propagates renewal failure, and otherwise returns the original
refusal. R2 makes the ordering explicit at
`TASK-003-R2-close-renewal-proof-and-source-contracts.md:30-46`: once status is
known to be `401`, renewal must be attempted regardless of whether body
collection succeeds. The current source contradicts that obligation and its
own rustdoc claim at `http.rs:357-364` that renewal follows every observed
`401` even when the body cannot be fully read.

The recorded R2 cut-off-body case at
`crates/shared/wyrd-client/tests/workflow_transport.rs:1039-1062` closes the
socket immediately after a partial body. That makes `response.bytes()` finish
with an error, after which renewal runs. It does not cover headers followed by
a body that remains pending until the outer caller cancels or times out.
Accordingly, the recorded evidence does not close this path.

The minimum correction is inside the existing `HttpTransport::post_native`
owner: after status is known to be `401`, attempt the existing
`AuthMiddleware::force_refresh` path before body collection can postpone it.
Keep one model POST, no replay, and renewal-error precedence, then return the
original refusal body or the existing body-read failure. Focused closure should
extend the existing raw-loopback selector with a `401` whose headers arrive
while its body remains pending, establish that renewal is reached before the
body is released or closed, and prove one model POST. No detached worker,
cache-retirement API, retry/configuration option, response marker, transport,
or new harness is required. Whether an already-started credential exchange
must survive arbitrary caller cancellation is a broader concurrency semantic
not stated by the approved R2 correction and is not added by this finding.

**Proposed finding `FU-R3-001` — INCORRECT / MISSING PROOF.** The source still
permits an already-observed native `401` to be cancelled before the required
renewal attempt, and the recorded abrupt-cutoff proof does not exercise that
state.

### 2. Selected ExtGateway execution blocks the async executor on config IO

**Resolution: confirmed.**

The path is direct:

1. Rust, Python, and TypeScript local execution call the shared async
   `Workflow::run`/`run_with` owner (`workflow/mod.rs:97-153`, Python
   `workflow.rs:537-562`, TypeScript `workflow.rs:134-159`).
2. `SelectedRoutes::of` records every selected `LlmRoute::ExtGateway`, and
   `needs_config` is true whenever that set is nonempty
   (`workflow/local.rs:39-70`).
3. Before the first await in `Workflow::run_with`, the selected route enters
   `GlobalConfig::load` synchronously (`workflow/mod.rs:132-137`).
4. `GlobalConfig::load` calls `load_from`, which executes
   `std::fs::read_to_string` (`global_config.rs:71-104`).

The current task introduced this selected-route configuration call path in
commit `80abe8483`; it is not unrelated pre-existing behavior. It is reachable
for every local run containing at least one selected `ExtGateway` route.
Depending on the embedding runtime, a slow home or mounted configuration
filesystem blocks the current-thread runtime or one shared Tokio worker before
secret resolution, network cancellation, or the Skald executor can progress.

This is material under the explicit repository rule at `AGENTS.md:282-283`:
blocking work may not run inside an async request path without a blocking
strategy. The same changed Workflow module already places authored bundle IO
on `tokio::task::spawn_blocking` (`workflow/mod.rs:56-86`), and selected secret
file reads use that established boundary (`workflow/local.rs:107-130`). The
minimum correction is therefore to move only the existing synchronous
`GlobalConfig::load` call onto the established blocking pool while retaining
lazy route selection, `GlobalConfig` ownership, and current public errors. No
async config API, cache, watcher, setting, option, dependency, checker, or new
test harness is justified.

**Proposed finding `FU-R3-002` — VIOLATION.** The task-added selected
ExtGateway preparation performs synchronous filesystem IO on the async executor
and can stall the shared SDK runtime.

### 3. `AuthMiddleware::force_refresh` documents retry behavior the native caller forbids

**Resolution: confirmed as a documentation-only contract violation.**

`AuthMiddleware::force_refresh` at
`crates/shared/wyrd-client/src/auth.rs:503-507` says the HTTP/gRPC reactive
`401` path calls it and then retries once. That text predates TASK-003, and the
method implementation is unchanged in the cumulative diff. TASK-003, however,
adds `HttpTransport::post_native`, a production HTTP transport consumer that
calls `force_refresh` specifically to prepare a later call and is expressly
forbidden from retrying the refused non-idempotent model request
(`transport/http.rs:351-410`; original task lines 77-87). Adding this caller
materially changes the auth owner's documented consumer contract even though
the method body itself did not change.

The concrete maintenance effect is that the auth owner now tells a maintainer
that every reactive HTTP caller retries once, while the approved native path
must not. Following that documentation can reintroduce model-call replay or
obscure why renewal cannot make the current native call succeed. This is not an
optional prose refinement: `AGENTS.md:716-730` and
`architecture/agent-rules.md` make accurate workflow role, side effects, and
retry documentation part of implementation correctness.

The minimum correction is to qualify the existing auth-owner rustdoc:
replay-safe HTTP/gRPC helpers may retry once, while native model POSTs refresh
only for later calls and return the original refusal. This changes no runtime
behavior and requires no test, check, option, or mechanism; ordinary source
review and the repository's normal documentation/lint evidence are sufficient.

**Proposed finding `FU-R3-003` — VIOLATION.** TASK-003's new non-retrying
consumer makes the authentication owner's unqualified retry contract false.

### 4. `Workflow::into_skald` omits its newly material client-context loss

**Resolution: confirmed as a task-relevant public-contract gap.**

`Workflow::into_skald` existed before TASK-003 with the same one-line rustdoc,
when the shared wrapper contained only `inner`. The candidate adds
`client: Option<WyrdClient>` to the shared `Workflow` and makes that retained
client the authority for automatic public-gateway dependency composition
(`workflow/mod.rs:34-41,106-153`). Consuming the wrapper through
`into_skald` at lines 170-174 now discards meaningful connection, credential,
token-cache, and pool context, together with the facade's automatic local
dependency preparation. The conversion's behavior is therefore materially
changed even though its body is textually unchanged.

This side effect is directly relevant to TASK-003. The original task requires a
Cards-loaded Workflow to retain its Cards connection context, and the R1
remediation exists because converting away the shared owner at the Python
boundary caused execution to use ambient configuration instead. Current direct
callers use `into_skald` only where they subsequently provide explicit
`WorkflowExecutionDependencies` or reconstruct a test-only facade, so no
additional runtime defect is established. The public conversion nevertheless
fails the hard rustdoc rule at `AGENTS.md:720-723`: its maintainer-relevant side
effect is no longer inferable from “Take the hydrated Skald Workflow.”

The minimum correction is documentation on the existing method: consuming the
shared wrapper intentionally discards retained client context and automatic
shared dependency composition; callers that need that behavior must keep and
run the shared `Workflow`, while callers taking the Skald value must supply
explicit dependencies. No wrapper, compatibility path, new API, check, test,
setting, or option is warranted.

**Proposed finding `FU-R3-004` — VIOLATION.** The materially changed public
conversion omits a task-critical ownership side effect already demonstrated by
the R1 context-loss defect.

## Proposed finding summary

| Source ID | Classification | Disposition | Smallest correction boundary |
|---|---|---|---|
| `FU-R3-001` | INCORRECT / MISSING PROOF | Confirmed | Reorder the existing native `401` refresh ahead of potentially pending body collection; extend the existing focused raw-loopback case |
| `FU-R3-002` | VIOLATION | Confirmed | Put the existing synchronous global-config load on the established Tokio blocking boundary |
| `FU-R3-003` | VIOLATION | Confirmed | Qualify existing `force_refresh` rustdoc for replay-safe versus native send-once callers |
| `FU-R3-004` | VIOLATION | Confirmed | Document client-context and automatic-composition loss on the existing consuming conversion |

All four corrections reuse existing owners and ordinary repository mechanisms.
None requires a new product or public API decision, dependency, abstraction,
configuration surface, checker, allowlist, compatibility path, test harness,
setting, or option. Requiring any such addition would be DRIFT.

## Verification limits

This review was strictly source-only. It ran no build, compile, test, Cargo,
mise, pnpm, pytest, formatter, linter, test-listing, package-manager, or other
verification command. The implementer's recorded evidence was assessed only
for the exact paths it describes. It does not cover a native `401` body that
remains pending after headers, and source contradicts the claimed universal
renewal ordering for that case. The other three findings are established by
the current source and repository documentation/async rules; no reviewer-run
command could override those source facts.

## Result

**RESOLVED**

The four disputed claims are source-resolvable. Four bounded proposed findings
remain for independent Ponytail validation; there is no unresolved authority,
caller-path, or evidence conflict in this follow-up.
