# Concurrency, async, cancellation, and lifecycle domain review

**Immutable subject:** base `58d07d7260df1f022a721e720a28ea48e5096e35`, candidate `5084b0e5b30fe79fdb2468ad7d0fa3a9d2106069`.

**Overall result: PASS.** The candidate satisfies the concurrency and async obligations in original TASK-003 and R1-R4. R4 closes the remaining mixed-route configuration race at its run-start producer: selected external bindings and a client-less public-gateway client now derive from one `GlobalConfig` value. The cumulative source continues to preserve immutable per-call context, one native model POST, renewal-before-body ordering on `401`, local cancellation/drop semantics, shared auth ownership, retained client lifetime, and blocking-pool isolation for synchronous run-start reads. I found no material defect in this domain and no qualifying DRIFT.

## Reviewed boundary and authority

I reviewed the complete base-to-candidate diff and current source against:

- `AGENTS.md` sections 2, 5, 6, 9, 11, and 12, especially the narrow async-boundary, dependency-owner, cancellation, and evidence rules;
- `architecture/agent-rules.md`, `architecture/references/languages/spec-driven-development.md`, `architecture/references/languages/maintainer-style.md`, and the applicable client/runtime ownership rules in `architecture/references/architecture/patterns.md`;
- approved `changes/active/skald-workflow-runtime/spec.md` Revision 12;
- original `TASK-003-remote-client-and-public-gateway.md` and remediation tasks R1, R2, R3, and R4;
- the prior TASK-003 findings ledgers and the implementation evidence recorded in each task.

The human-approved `WyrdGatewayCall.model: ModelRef` amendment, the R1 native-`401` wording correction, and the approved use of `spawn_blocking` for run-start configuration/client/secret reads were treated as authority. The review did not reinterpret the approved run-start boundary as drift: Workflow loading must continue to resolve no execution secrets.

The end-to-end trace covered:

- route selection and attempt-local model, fallback, absolute deadline, cancellation token, and run/step/attempt correlation;
- concurrent public calls through `WyrdGatewayProvider`, `PublicWyrdGatewayCaller`, `HttpTransport::post_native`, and the shared auth owner;
- pre-dispatch cancellation, post-dispatch cancellation, caller timeout, dropped futures, complete/truncated/pending `401` bodies, renewal failure precedence, and no resend;
- auth cache and renewable-mint locking/lifetime behavior;
- `Workflows::{create,get,cancel,wait}`, stable submission idempotency, foreground polling, terminal snapshots, and dropped-wait behavior;
- `Workflow::run_with`, selected-route discovery, the run-start blocking task, one configuration snapshot, selected secret reads, retained Cards clients, and explicit native dependencies;
- the Rust focused fixtures and public Python retained-client journey named in the recorded evidence.

No build, compilation, test, lane, Cargo, mise, pnpm, pytest, formatter, linter, or other verification command was run. Conclusions are based only on source, the cumulative diff, and recorded evidence.

## Boundary and source coverage

| Boundary | Source and evidence | Result |
|---|---|---|
| Immutable concurrent call context | `crates/skald/skald-workflow/src/route.rs:395-426,543-590` copies the resolved model and fallback into an attempt-private adapter, recalculates remaining time from the attempt's absolute deadline for every model call, and creates a fresh `WyrdGatewayCall` with cloned correlation. `crates/shared/wyrd-client/src/workflow/gateway.rs:77-123` keeps the native projection and fallback header vector local to that invocation. No call metadata enters shared provider, registry, or header state. The recorded concurrent fixture distinguishes models and fallback headers on one shared caller. | PASS |
| Deadline and cancellation propagation | `workflow.rs:430-518,525-568` fixes step/Agent/run deadlines per attempt, races cancellation and total/attempt/Agent deadlines with explicit precedence, and supplies the same run cancellation token to the route adapter. `workflow/gateway.rs:96-116` races the complete authenticated native exchange against cancellation and the call's remaining timeout with cancellation precedence. A sent request may continue at the gateway, but the local future neither retries nor mutates a later terminal snapshot. | PASS |
| Native `401` ordering and send-once | `crates/shared/wyrd-client/src/transport/http.rs:351-410` constructs and sends exactly one native model POST. Once the response status is known to be `401`, it awaits `AuthMiddleware::force_refresh` before reading the body, gives renewal failure precedence, and never rebuilds or resends the request. The recorded raw fixture covers complete, truncated, and held-open bodies and observes one model POST plus renewal while the held body remains pending. Prior `FIND-TASK-003-2` stays closed. | PASS |
| Shared auth ownership and locking | `crates/shared/wyrd-client/src/auth.rs:436-619` retains one middleware-owned cache gate. Ordinary bearer acquisition rechecks freshness under that gate; force-refresh uses the same owner; renewable synchronous minting runs on the blocking pool and leaves one pending join handle for a later waiter if the current future is dropped. TASK-003 introduces no second cache, detached renewal owner, or credential mutation path. | PASS |
| Remote lifecycle and dropped futures | `crates/shared/wyrd-client/src/workflow/remote.rs:41-116` reuses shared idempotent submission for create, performs direct get/cancel calls, and implements wait as foreground one-second polling. Dropping create/cancel can leave server-side progress already accepted, as documented; dropping wait destroys only its polling future and issues no cancel or resubmission. Terminal failed/cancelled/timed-out snapshots remain values. | PASS |
| Run-start blocking boundary | `crates/shared/wyrd-client/src/workflow/mod.rs:78-96,131-149` moves authored bundle/client construction and selected local setup to Tokio's blocking pool. `crates/shared/wyrd-client/src/workflow/local.rs:83-138` resolves only selected secret references and performs each synchronous secret read through `spawn_blocking`. Already-started read-only work may finish after caller drop, but it performs no dispatch or durable mutation. This is the explicitly approved boundary. | PASS |
| One run-start configuration snapshot | `workflow/mod.rs:179-239` calls `GlobalConfig::load` at most once when selected bindings or a client-less selected Wyrd gateway need ambient state. `local_setup_from` consumes that single value, moves its Workflow bindings, and builds the gateway client with `ClientConfig::from_global_with_env(&global)` plus `WyrdClient::with_config`; it no longer calls the independently reloading `WyrdClient::from_global`. The ordinary client-less mixed `ext_gateway` plus `wyrd_gateway` path therefore cannot combine two config-file versions. `mixed_routes_use_one_config_snapshot` exercises both consumers from one parsed value, and R4 records the focused selector result. Prior `FIND-TASK-003-10` is closed. | PASS |
| Route selection and retained-client precedence | `workflow/local.rs:39-70` derives needs solely from each step's resolved route. `workflow/mod.rs:136-149,215-239,303-348` passes a Cards-loading client into setup; when present it remains authoritative for public gateway calls and ambient client assembly is skipped. A purely native run has neither need flag and performs no ambient configuration read. Python retains and runs the complete shared facade, and the recorded public integration verifies that later hostile or absent ambient configuration does not redirect registered or authored-external-ref runs. | PASS |
| Source-contract and ownership transitions | `auth.rs:503-514` accurately distinguishes replay-safe transport retry from native send-once renewal. `workflow/mod.rs:152-176` states that `into_skald` discards retained client/automatic dependency composition while borrows preserve it. No hidden background lifecycle or mutable global per-call context is implied by either API. | PASS |

## Findings

No findings.

The R4 correction uses existing `GlobalConfig`, `ClientConfig::from_global_with_env`, and `WyrdClient::with_config` owners. It adds no cache, watcher, reload protocol, timing fixture, configuration option, dependency, permanent checker, compatibility path, or language-specific executor. Under the standing DRIFT rule, there is no unsupported mechanism to remove and no such mechanism is required for remediation.

## Failure, cancellation, and recovery assessment

The local public-gateway caller owns only caller-side IO. Cancellation or deadline drops that IO future promptly; it cannot promise rollback after the gateway has accepted the request. Skald's executor owns its step tasks and aborts/drains them on run cancellation or total deadline, while gateway settlement remains gateway-owned and cannot mutate the completed Workflow snapshot. This matches the approved cross-owner lifecycle boundary.

On a native `401`, refresh begins after headers establish the refusal and before response-body collection can stall it. Successful renewal affects later calls only; failed renewal is authoritative; the refused model operation is never replayed. Concurrent callers share only the established connection/auth owner, while their model, fallback, deadline, cancellation, correlation, native body, and headers remain call-local.

Run-start `spawn_blocking` work can outlive a dropped awaiting future, but the approved operations are bounded reads and client assembly before dispatch. Selected secret reads remain at run start rather than loading/apply, so no execution secret is read early. No new cancellation worker or cache-retirement mechanism is warranted.

## Verification evidence and limits

The candidate records, for R4, a five-test focused shared-client selector covering Workflow module tests, public gateway context/errors, and the shared remote client contract; shared-client Clippy and formatting; Python setup; and the exact public Python retained-client integration with one passing test. Source matches the recorded single-snapshot, retained-client, pending-`401`, send-once, cancellation, and remote-lifecycle assertions. Earlier remediation records supply the focused negative and raw-transport cases retained in the cumulative candidate.

This review independently ran none of those commands, per the human read-only direction. That is a review-method limit, not missing implementation evidence: the required evidence is recorded with exact commands and selected counts, is represented by source fixtures in the candidate, and is not contradicted by source. Candidate identity remained `5084b0e5b30fe79fdb2468ad7d0fa3a9d2106069` at final inspection.
