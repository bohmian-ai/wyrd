# Domain review: client concurrency and recovery

Result: **FAIL**

## Immutable subject and scope

Reviewed cumulative `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0..07adb7907951f343544e550f6cabc5b7aa3aadea` in the repository identified by `subject.md`. Observed HEAD remains packet-only commit `aa32c821ef74886526b7367bfd77d51c87e0d521`. Scope is shared-client replay, file preparation/transfer, polling deadlines, cancellation, and retained/ambient client lifetime, including their Python and TypeScript projections. Durable server acceptance, dispatch, queue settlement, and persistence remain TASK-005.

## Authority and source coverage

| Boundary | Governing authority | Source/callers/proof inspected |
| --- | --- | --- |
| Shared client and foreign runtimes | AGENTS §§2–3, 6–8; agent-rules; reference router, Rust core, PyO3 boundaries, TypeScript guide, maintainer style | `observe/mod.rs`, `state.rs`, `verify/{mod.rs,execution.rs}`, Python `src/verify.rs`, Python public verify exports, TS native `verification.rs`, TS `src/index.ts` |
| Replay and ambiguous failures | Spec r7 REQ-011–013; TASK-004 scenario 2 | `Target::{request,execute,enqueue}`, `HttpTransport::{request_json,submit_with_optional_idempotency_key,send_with_retry}`, observe tests |
| Lazy files and partial upload | Spec REQ-010; TASK-004 scenario 1 and existing-owner table | `File::{path,bytes,pin}`, `RunEvidence::pin`, storage `upload_evidence`, `upload::{sha256_file,dispatch}`, `FileSource`, `single_put::upload`, existing artifact `validate_and_stamp/hash_source` |
| Status/wait/reconnect | Spec REQ-012; TASK-004 scenario 2; testing workflows and error reference | Entire `Execution` implementation; shared observe tests for replay/wait; Rust building-block reconnect journey; Python and TS native/public callers |

CodeGraph exploration preceded source navigation. Read the complete cumulative file-change inventory and the relevant full changed owners, existing sibling mechanisms, and consumers. No second production SQL/persistent-state implementation was added within this boundary.

## Assessment

- Enqueue builds one owned `VerificationRequest`, generates one UUIDv7 key outside the shared transport retry loop, and transport serializes the body once before retries. Retries retain the same body bytes, key, request ID, and shared auth owner. Existing transport handles fresh credentials and retries; no language duplicates it.
- Direct execute deliberately has no replay key; shared transport retries only safe failures (connect-before-delivery, or rejected authorization refresh), and does not replay ambiguous POST timeout/5xx. Its timeout floor extends only the capability's client clone and shares auth/pool rather than mutating sibling client defaults.
- Files stay lazy until a verification call. Shared `sha256_file` streams bounded buffers and also serves existing Artifact registration; `upload_evidence` extends the existing dispatch/SinglePut transfer instead of creating a parallel transport or registration lifecycle. Only missing digests returned by evidence planning are transferred. Local path bytes can change between hash and upload, but declared identity remains the original digest and spec explicitly assigns digest/length verification to the server; this does not justify a new client snapshot store in this task. A mismatch must not yield pass (server proof is TASK-005).
- Cancellation drops local preparation/HTTP/poll futures; previously uploaded content or an already staged request may survive, which is documented. No client durable queue or exactly-once promise is introduced. Execution clones retain the same client/auth owner; explicit reconnect resolves via existing `connected`, ambient reconnect via existing `connect_ambient`. Python and TS wrappers delegate status and polling to this owner.
- Completed-without-result remains pollable; terminal conflict is converted through the central stable-code mapper, and nontransient status errors propagate. The deadline boundary, however, does not cover status IO or transport retries.

## Proposed material finding

### CONC-01 — INCORRECT: wait timeout does not bound status requests

**Obligation:** REQ-012 requires delayed visibility tolerance within the caller-supplied timeout and a bounded `wait(timeout)`; TASK-004 scenario 2 requires bounded polling and controlled-time proof. Public rustdoc promises timeout when it passes first (`execution.rs:103–104`).

**Location:** `crates/shared/wyrd-client/src/verify/execution.rs:108–145`, specifically `self.status().await` at line 113 and the result return at line 115 before deadline inspection.

**Reachable producer → consumer:** Rust callers obtain the handle from `Target::enqueue` or `Execution::connect[_with]`; Python `PyExecution::wait` (`src/verify.rs:151–155`) and Node `NativeExecution::wait` (`native/src/verification.rs:193–205`), through TS `verify.Execution.wait` (`src/index.ts:2216–2217`), all reach this same owner. `status` calls `HttpTransport::request_json` → `send_with_retry`, whose GET attempts each use the client's configured request timeout (`transport/http.rs:1012`), may retry timeouts/5xx, perform auth resolution, and sleep between attempts. None receives the remaining wait budget. After a sleep clamped to the deadline, the loop starts another status request before checking whether time has expired.

**Observable consequence:** With a 100 ms caller budget and a status endpoint returning a real completed result after 500 ms, `wait` returns that result at 500 ms instead of the SDK wait-timeout error. With an unavailable endpoint and a 5-second client request timeout, a single poll can consume multiple 5-second attempts plus retry pauses before `wait` checks its shorter deadline. This affects all three SDKs, including a blocking Python wait, and cannot be dismissed as server work still running: the wait deadline only bounds local observation.

**Existing proof gap:** `observe::tests::wait_raises_terminal_errors_and_times_out_without_a_result` checks the timeout code against immediate running responses but does not delay a response beyond the wait deadline or assert that the request/retry is interrupted. Its green result cannot detect this gap.

**Smallest correction boundary:** Enforce the single caller deadline in shared `Execution::wait`, using the already installed Tokio deadline/cancellation facility to bound the entire awaited status operation (including auth/transport retry/body decoding), and check expiry before issuing subsequent polls. Return the existing `SdkVerificationWaitTimedOut` with the existing execution/last-status details. Preserve standalone `status()` behavior, transport retry policy, queued execution lifetime, and thin runtime wrappers; do not add language-specific timers or alter global client timeout policy.

**Focused closure proof:** Extend the existing shared-client wiremock tests with a status response delayed past a much shorter wait budget, asserting `WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT` rather than a late successful result and bounded local return. Also cover a poll whose transport retry/backoff extends beyond the budget, and ensure no extra status read starts after expiry. Use the current shared-client test target and local mock server; no database, provider, journey aggregate, or new harness is necessary. Existing immediate-success, delayed-visibility, and terminal-error cases must remain green.

## Verification limits

This was a static independent domain audit. No Cargo/build/codegen commands or runtime tests were run, per orchestration constraints. The reported 28 client tests and language checks are implementation evidence, not independently reproduced results. Full SDK/server journeys are authored and intentionally deferred to integrated change review after TASK-005; the reconnect journey inspected calls both wait and status. The external demo was frozen without a committed implementation identity, so it has the provenance limitation recorded in `subject.md`; the concurrency defect is established in the immutable shared owner and its runtime callers rather than by mutable external source.

No additional recovery machinery, persistent client state, or server changes are recommended. The one retained finding is a bounded correction to existing caller-deadline behavior.
