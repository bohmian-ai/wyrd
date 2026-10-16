# Concurrency, recovery, and client lifecycle review

Result: **FAIL**. One proposed finding: **CONC-R1-01**, a remaining instance of **FIND-004-1** introduced by the interaction with the lazy-construction remediation.

## Immutable subject and boundary

Reviewed base `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0` through candidate `dddf985563029a004fa768110e36d0c5f12bcded`, using prior candidate `07adb7907951f343544e550f6cabc5b7aa3aadea` to locate remediation changes. HEAD remained the candidate on final inspection. Scope was polling deadlines, lazy/connected client retention, concurrent first use, submission replay, cancellation and upload lifetimes. No peer discovery report was read.

Authority: approved spec r8 REQ-011–013, INV-006 and AC-005/006; original TASK-004 and R1 remediation; `AGENTS.md` §§2, 4–7, 11–12; `architecture/agent-rules.md`; spec-driven-development and maintainer-style references; PyO3/Rust references; the client-model section of `architecture/wyrd-design.md`, verification doctrine, and Bifrost execution/evidence ownership. Human decisions in `subject.md` govern: no spec revision, approved extra changes, provisional `code.run`, and no real-server journey execution in this review.

## Source and owner coverage

| Boundary | Source traced | Assessment |
|---|---|---|
| Whole status-read deadline | Shared `verify/execution.rs::Execution::{wait,status}` → `transport/http.rs::{request_json,request_json_with_headers,send_with_retry}` → authentication/body decoding | `timeout_at` bounds the connected handle's status future, including retries/refresh/body awaits. Zero budget starts no shared status read. Late results are cancelled instead of returned. |
| Python first-use connection | `src/verify.rs::PyExecution::{__new__,handle,connect,queued,status,wait}` → existing `client.rs::LazyConnection::{new,ready,get}` → shared `Execution::{connect,connect_with}` → `WyrdClient::{connect_ambient,connected}` | Existing connection owner is correctly reused; ID reads are local, failures are not cached, successful initialization is serialized and retained, and queued handles preserve their original client. **First lazy wait's connection occurs outside the deadline.** |
| Sibling foreign-runtime consumers | Python Observe submit projection and existing Bifrost LazyConnection usage; TS `NativeExecution::{status,wait}`, `NativeWyrdClient::connect_execution` and public `Execution` wrapper | Returned Python executions use `ready`; TS reconnect is explicitly eager and waits delegate to shared `Execution`. No second polling runtime or replacement transport. |
| Submission replay and interruption | `verify/mod.rs::Target::{execute,enqueue,request}`, `RunEvidence::pin`, shared HTTP submit/retry owner | Enqueue mints one key after evidence pinning and reuses the serialized body/key across retry attempts. Direct execute is a non-idempotent POST, so ambiguous timeouts/5xx do not silently replay model work. Cancellation can leave a staged submission whose ID is unknown; source documents this limit rather than promising exactly-once work. |
| File/upload resource lifetime | `File::pin`, `RunEvidence::pin`, `storage::WyrdStorageClient::upload_evidence`, existing upload dispatch/SinglePut/LocalFs/FileSource, shared `sha256_file` and Artifact hash consumer | Reuses the storage executor. Partial upload failure prevents the execution request. File handles/stream futures are owned by the invocation; cancellation abandons transfer without creating an Artifact completion lifecycle. File reopening leaves digest validation server-owned; no new client durability owner was introduced. |
| Window writer lifetime | Observe projection, `state.rs`, `observe/lifecycle.rs::{BifrostLifecycle,StartClaim}` | Removing Eval's destination preserves the existing shared writer/start-claim/shutdown owner. Describe failure/cancellation releases the claim, and shutdown prevents an in-flight start from installing a new writer. No separate Window writer or retained queue was introduced. |

## Proposed finding

### CONC-R1-01 — first lazy Python wait connects before its timeout starts

- **Classification:** INCORRECT; partial closure of **FIND-004-1**, at its interaction with FIND-004-2.
- **Obligation:** REQ-012's caller-supplied bounded wait and R1 FIND-004-1 closure criterion: the caller deadline must win over delayed credential/status/retry/body IO. Preserve Python's required lazy constructor and eager `connect` semantics.
- **Location:** `sdks/wyrd-sdk-python/src/verify.rs:198–203`, especially `self.handle(py)?.wait(timeout)` at line 201; upstream `PyExecution::handle` at 122–132 and `src/client.rs::LazyConnection::get` at 140–158. Shared deadline begins only at `crates/shared/wyrd-client/src/verify/execution.rs:112`.
- **Producer → consumer:** Python evaluates `self.handle(py)?` before constructing the shared wait future. On a fresh `Execution(id, client)` this enters `LazyConnection::get`, releases the GIL, acquires its synchronous initialization gate, then synchronously `block_on`s `Execution::connect_with` or ambient `Execution::connect`. Both complete credential exchange through `WyrdClient::connected` before returning. Only after all that succeeds does shared `Execution::wait(timeout)` create its deadline. A slow or refused API-key/workload/delegated exchange therefore consumes arbitrary time relative to the caller budget and can return an auth/transport error even for a zero budget. A concurrent first operation can also hold the initialization gate outside that budget. Ready submission handles and already-connected handles avoid this gap; eager `connect` intentionally has its own separate lifetime.
- **Concrete proof:** an existing compiled Python boundary was loaded through the repository toolchain, with no server or source edits:

  ```bash
  WYRD_LOG=info mise exec -- sdks/wyrd-sdk-python/.venv/bin/python - <<'PY'
  import time
  from wyrd import WyrdClient
  from wyrd.verify import Execution
  execution = Execution('0190b2a4-0000-7000-8000-000000000000', WyrdClient('http://127.0.0.1:9', 'wyrd_sk_t_v_s'))
  started = time.monotonic()
  try:
      execution.wait(0)
  except Exception as error:
      print(type(error).__name__, getattr(error, 'code', None), 'elapsed_seconds=', round(time.monotonic() - started, 3))
  PY
  ```

  Actual output: `WyrdError WYRD_CLIENT_503_TRANSPORT_DOWN elapsed_seconds= 0.029`. The zero-budget operation attempts credential resolution instead of immediately returning `WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT`. The credential is a local dummy value matching the existing test fixture, not an environment value.
- **Observable consequence:** the advertised Python caller timeout does not bound a first lazy wait during authentication outage/slow exchange; Python also differs from a connected Rust/TS wait for the zero-budget path.
- **Smallest correction boundary:** retain `LazyConnection` as the single connection/cache owner and `Execution::wait` as the polling owner, but make a lazy Python wait's one caller budget cover required first-use initialization and the subsequent shared wait. Do not merely subtract initialization time after an unbounded blocking connection has already completed. Keep successful initialized-handle reuse, retry after failed initialization, connected submission retention, eager `connect`, ordinary standalone `status`, and server execution lifetime unchanged. Do not add a Python polling loop, second resolver, background connection task, or timeout policy for unrelated handles.
- **Focused closure:** add one Python-runtime regression using the existing local execution server: a delayed first API-key exchange must lose to a short wait budget, zero budget must send no exchange/status request, and a later normal operation must still initialize/reuse the same handle. Keep existing eager-connect/refused-first-use tests and shared wait/replay tests green. No full real-server journey is needed for this boundary regression.

## Prior-finding closure within this domain

| Prior finding | Assessment |
|---|---|
| FIND-004-1 | Shared connected-handle defect is corrected by `timeout_at`; complete first-use Python path remains incorrect as above. Existing new Rust test covers late success, repeated 503 and zero read, but uses an already-connected bearer and cannot expose lazy initialization. |
| FIND-004-2 | Lazy constructor/eager connect/ready queued handle/state reuse obligations are implemented through the existing owner. Existing Python tests cover refusal then retry/reuse and eager connection; the newly exposed deadline interaction belongs with FIND-004-1. |
| FIND-004-5 | Added deadline/failure journeys are authored as required, with actual execution deferred. This domain review does not claim their server outcomes have run or establish coverage of the entire action/check matrix. |
| Other prior findings | Outside this domain's closure determination; no conclusion inferred from another report. |

## Verification limits

No Cargo commands, full journeys, repository gate, production edits, commits, pushes, environment dumps, or generated artifacts. Existing implementation evidence was treated as a claim; the only executable boundary proof here is the local zero-budget call above. A delayed-auth local-server reproduction was attempted with tracing enabled but socket creation was denied by the sandbox (`PermissionError: Operation not permitted`), so its timing path is source-proven, not claimed as executed. The standard `uv run` entry initially could not write its global cache; the existing package `.venv` interpreter was then invoked through `mise exec --`. Orchestrator-owned verification supplies crate/owner lanes separately.

No other material concurrency, replay, cancellation, or upload-lifetime findings were established.
