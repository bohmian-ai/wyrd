# Independent system-resilience review

Result: **FAIL**. One remaining bounded-wait defect is proposed below. This is
the system-resilience discovery report, not the final validated verdict.

## Subject and authority

- Base: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`.
- Candidate: `dddf985563029a004fa768110e36d0c5f12bcded`; HEAD was rechecked at
  completion and matches. Cumulative and remediation ranges were inspected.
- Original TASK-004, approved spec r8, prior verdict and FIND-004-1..9, and
  R1 task/evidence are inputs. Owner-approved scope additions, superseded
  FIND-004-7, deferred journey execution, and provisional `code.run` arguments
  are accepted without qualification.
- Governing evidence: AGENTS.md; agent rules; design sections on clients,
  verification/outboxes and execution identities; doctrine; Bifrost verification
  handoff; reference router; spec-driven development; maintainer style;
  architecture constraints; PyO3 boundaries; evaluation; telemetry;
  drift and analytical operations/reliability references. REQ-010..015 and
  TASK-004 scenarios 1..3 govern the runtime paths below.

## Deployment and changed-path coverage

The deployment remains one logical `wyrd-server` surface, potentially spread
over replicas and selected-subsystem pods. The cumulative candidate changes
client projection, wrappers, fixtures, journeys, examples and documentation;
it does not change production server, worker, SQL, Scribe, or runner code.
No new server component, health check, process-fatal initialization rule, queue,
or recovery service is introduced. TASK-005 owns the staged server cutover.

| Path and owner | Actual producer-to-consumer/source evidence | Availability and recovery consequence |
|---|---|---|
| Local named/positional projection | `Observe::verify/submit` in `observe/mod.rs` -> `Target::project` in `verify/mod.rs`; Python `observe::inputs`; TS `nativeInputs` -> native `verification::inputs` | Wrong names, slot counts and kinds are refused at the invocation boundary. TS positional additions reuse `Inputs::One`; no separate execution owner or retry path appears. |
| File declaration and transfer | `File::pin` -> shared `storage::upload::sha256_file`; `RunEvidence::pin` -> evidence plans -> `WyrdStorageClient::upload_evidence` -> existing upload dispatcher | File IO/storage failures fail that invocation before execute/enqueue. Files stream for hashing and uploading; retained uploads are tenant evidence, not Artifact registration. Cancellation can leave deduplicated uploaded evidence, as documented; no shared server shutdown follows. |
| Immediate verification | `Target::execute` -> existing HTTP `request_json` POST without replay key; `HttpTransport::send_with_retry` | Ambiguous timeout/5xx after sending is surfaced rather than replayed; connection failure before processing and refused authentication retain existing transport behavior. The execution deadline plus response grace uses the shared transport. No SDK provider engine or durable result store exists. |
| Background submission | `Target::enqueue` -> one generated key -> shared `submit_with_idempotency_key` -> `Execution::new` | Transport retries reuse request body/key. A 202 is staging only. Abrupt server death before Scribe acknowledgement can leave no visible row; clients poll and time out instead of silently resubmitting. Durable acceptance, restart fencing and eventual settlement remain existing server owners and deferred integration proof. |
| Status and connected wait | `Execution::status/wait` -> `request_json_with_headers` -> `send_with_retry` -> auth/body read | The R1 `timeout_at(deadline, self.status())` bounds the full read future, including credential renewal, retry sleeps and response decoding. Zero timeout starts no read; sleeps are deadline-clamped. 404 and 5xx can recover under capped polling; authorization refusal is returned. Completion without a result remains pollable. Dropping the Rust future drops polling, leaving server work unaffected. |
| Python lazy/eager handles | `PyExecution` -> retained `LazyConnection<Execution>`; `queued` -> `LazyConnection::ready`; `handle` -> existing synchronized `get` | Offline construction and ID reads do not connect; failed connection retains no handle and next use retries. Successful first use retains the supplied principal/client; already queued executions stay connected. The first lazy **wait** has the deadline defect below. |
| Node handle lifetime | `NativeExecution::{status,wait}` clones retained shared execution into `trace::spawn`; `NativeRun::{verify,submit}` clones Run before asynchronous work | Binding work owns its Rust dependencies instead of borrowing a JS object across asynchronous completion. All language paths share the same transport, projection, execution ID and result semantics. |
| Window samples and shutdown | `Observe::submit` Window branch -> `drift_value`; state-owned `BifrostLifecycle` and `StartedBifrost` | The existing writer, queue refusals, start claim and shutdown/drain remain owners. Eval-table startup/emits are removed under the approved contract; samples do not create execution handles or a second writer. |
| Result/evidence readback | Execution result serializers, Python wire TypedDicts, TS generated types; scheduled helpers select/filter `execution_id` while retaining real result joins | Readback correlation is corrected without changing persistence or authority. Typed wrapper projections do not manufacture outcomes. SDK/example reconnect paths retain the distinction between execution completion and retained evidence. |

The surrounding file-transfer, auth, transport and lifecycle owners were read,
not inferred from wrapper signatures. The client tier changes do not prescribe
crashing a shared server on component errors. Worker crash isolation, Scribe
loss accounting, queue lease recovery, tool execution and scheduled dispatch
are server-side obligations whose implementation is outside TASK-004; the
candidate authors their public journeys rather than substituting a client
engine. Their actual execution is expressly deferred by the owner.

## Failure and proof assessment

| Failure/recovery path | Evidence and assessment |
|---|---|
| Slow status/response, retrying service outage, expired budget | R1 wraps the entire shared status future in a monotonic deadline. New exact Rust test covers delayed completed response, 503 and zero read budget. Source supports closure for connected handles. Independent reruns compiled/selected the tests but sandbox listener denial prevented behavior execution; see `verification.md`. |
| Lost staged submission, delayed queue/result visibility | `wait` retries 404/5xx, and retains completed-without-result polling; real results and terminal errors remain the only successful/terminal returns. Existing scripted owner tests express these branches; no durability guarantee beyond staged acceptance is added. |
| Delayed asynchronous replay conflict | Shared wait maps a terminal execution error through the existing error catalog. Enqueue owns one key per invocation and transport retries preserve it. No extra client queue or silent retry after wait timeout exists. |
| Auth exchange failure then retry | Python existing lazy owner serializes initialization, stores only success and releases the gate after failure. New Python tests exercise refusal/retry/reuse via status and eager connect. They do not exercise deadline-bearing lazy wait. |
| Concurrent initialization during outage | Existing LazyConnection gate serializes first connection; no GIL is held during that blocking work. For a lazy `wait`, waiting for this gate is also outside the caller timeout, the same defect as the initial exchange. |
| Client/process restart or abandonment | Execution ID is independent of the client Run and reconnectable in all three surfaces. Clients retain no durable queue/result cache. Poll cancellation does not cancel server work. Reconnect after server data loss eventually times out instead of claiming success. |
| Uploaded path changes or storage interruption | File is hashed before declaration and existing streaming upload machinery sends the bytes. Server digest/length/format enforcement is an explicit retained authority, not duplicated by the client. A changed local file cannot legitimately produce a pass; the server cutover/integrity journeys remain deferred. |

Shared independent checks reported in `verification.md`: Rust integration source
compiles and is listed; exact fixture validation passes; affected library
Clippy passes; Python typing and collection pass; local TypeScript typing and
journey discovery pass. These checks support authored/compiled/discovered
closure, not executed system recovery. No Cargo command was launched by this
reviewer and no full journey suite or gate was run.

## Proposed finding SYS-001 — first Python lazy wait bypasses its deadline

**Prior finding:** remaining portion of FIND-004-1, exposed by the R1
FIND-004-2 lazy-handle implementation. **Classification:** INCORRECT.

**Obligation:** REQ-012 and TASK-004 scenario 2 require bounded waiting under
the caller-supplied timeout. R1 explicitly requires the deadline to win over
authentication/status/retry/body IO, including a zero budget that starts no
read.

**Location:** `sdks/wyrd-sdk-python/src/verify.rs:198-203`, specifically
`crate::trace::block_on(py, self.handle(py)?.wait(timeout))` at line 201;
`handle` at lines 121-128; existing `LazyConnection::get` at
`sdks/wyrd-sdk-python/src/client.rs:140-163`.

**Producer and reachable failing state:** `Execution(id, explicit_api_key_client)`
retains no connected handle. The first `wait(timeout)` evaluates
`self.handle(py)?` before creating/polling shared `Execution::wait`. That runs
`LazyConnection::get`, waits for its synchronous initialization gate and calls
the shared runtime's `block_on(Execution::connect_with(...))`.
`Execution::connect_with` calls `WyrdClient::connected`, which awaits
`access_token`. Only after this succeeds does shared `wait` create its
`Instant` deadline. Initial exchange duration, retries, refusal and competing
first-use gate wait are therefore outside the caller budget. A slow auth
dependency can delay this caller past its timeout; even an already expired
zero budget attempts authentication and can return an auth/transport error.

**Observed public-boundary probe:** ran through the existing installed Python
SDK with `mise exec -- uv run --no-sync --directory sdks/wyrd-sdk-python python`
and a temporary writable `UV_CACHE_DIR`, using only synthetic credentials and
`http://127.0.0.1:9`:

```python
Execution("0190b2a4-0000-7000-8000-000000000000",
          WyrdClient("http://127.0.0.1:9", "wyrd_sk_t_v_s")).wait(0)
```

It returns `WYRD_CLIENT_503_TRANSPORT_DOWN`, whereas the same zero-budget call
with a synthetic bearer returns
`WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT`. Repeated with `WYRD_LOG=info` and got
the same two outcomes; no environment values were printed. An attempted local
delayed-auth listener was refused by the sandbox at socket creation, so no
delayed-exchange timing result is claimed. The wrong zero-budget error plus
the complete synchronous-before-deadline call path establishes reachability.

**Consequence and boundary:** this is a Python caller's wait availability and
error-contract defect, not lost server work or a shared-server crash. It can
block request/worker threads beyond their selected budgets on an unavailable
auth dependency. Submitted/eagerly connected handles and the Rust/Node shared
wait path already apply the fixed polling deadline.

**Smallest testable correction:** retain existing LazyConnection state,
credential owner, successful-handle reuse and eager `connect` semantics. Make
the one deadline of Python `wait` cover first-use acquisition/connection as
well as the shared polling interval, with no reset after connecting and no
blocking initialization-gate acquisition outside it. Reuse existing connection
and auth machinery; do not add a transport, background polling runtime,
per-language retry policy or server cancellation. A successful/failed initial
connection must preserve the existing reuse/retry rules; an expired zero
budget must start neither connection nor status IO. This is the existing
bounded-wait requirement, not a new cross-service policy.

**Focused closure proof:** extend the owning public Python execution tests
with lazy `wait(0)` on an unconnected key client (timeout and zero requests),
and a controlled delayed initial exchange with a shorter wait budget (timeout
within budget, no post-expiry status request). Cover waiting behind a concurrent
first connection using the same deadline, and retry/reuse after a failed or
timed-out connection. Retain existing Rust deadline/replay tests and Python
offline-ID, status retry and eager-connect tests. No full journey suite or new
harness is needed; use the existing local execution server when permitted.

## Prior-finding closure within this lens

- FIND-004-1: shared connected wait fixed; Python lazy first-use deadline remains
  SYS-001.
- FIND-004-2: offline constructor, existing lazy owner reuse, eager connect and
  queued client retention are present; interaction with bounded wait is SYS-001.
- FIND-004-3: positional TS values enter the same shared projection; no new
  failure/retry owner.
- FIND-004-4: nine candidate-tracked samples and approved PDF handling are
  present; not a process-recovery change.
- FIND-004-5: negative/recovery journeys are authored; executed proof remains
  explicitly deferred. Supporting compile/typecheck/discovery is credible.
- FIND-004-6: execution correlation fixes are present with result joins kept.
- FIND-004-7: superseded by r8/owner decision.
- FIND-004-8: dictionary wire views preserve runtime serializers and add no
  new parsing/state owner.
- FIND-004-9: retained ownership fields now have rustdoc.

No other material system-resilience finding is proposed. Production source,
prior review files, environment variables, commits and branches were untouched.
