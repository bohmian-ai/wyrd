---
id: TASK-004-R2
kind: remediation
status: complete
spec: SPEC-verifier-building-blocks
spec_revision: 8
parent_task: TASK-004
remediates: [FIND-004-1, FIND-004-8, FIND-004-10]
requirements: [REQ-011, REQ-012, REQ-013, REQ-015, INV-002]
depends_on: []
---

# Close Python wait budgeting and the remaining typed Check record

Implement through `$wyrd-implement` under approved `changes/active/verifier-building-blocks/spec.md` r8. Original task: `changes/active/verifier-building-blocks/tasks/TASK-004-sdks-examples-and-docs.md`. This packet is based on immutable candidate `dddf985563029a004fa768110e36d0c5f12bcded`, cumulative base `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`, and prior R1 candidate `07adb7907951f343544e550f6cabc5b7aa3aadea`. Read adjacent verdict.md and findings-validation.md, plus original review `task-004-20261015-07adb7907/` and its R1 task. A later review remains cumulative against the original task.

## Diagnosis and selected correction

### FIND-004-1: the lazy connection phase escapes Python's wait budget

The shared `Execution::wait` now correctly bounds status, credential refresh, transport retry/body decoding and backoff. But `sdks/wyrd-sdk-python/src/verify.rs:198-203` evaluates `self.handle(py)?` before the shared future starts its timer. `handle` enters `LazyConnection::get` at `src/client.rs:140-163`, waits for its synchronous initialization mutex and completes shared connect/authentication. Only afterward does shared `verify/execution.rs:112` compute its deadline. Slow first credential exchange or initialization contention can therefore outlive the caller budget, then grant a fresh polling budget. Independent public `Execution(id, API-key-client).wait(0)` probes returned `WYRD_CLIENT_503_TRANSPORT_DOWN` instead of the already-expired wait-timeout error. The source proves delayed-success/contended paths too; the review sandbox could not run a local delayed server because port binding is denied.

Begin one monotonic budget at Python wait entry. Reuse the existing LazyConnection initialization owner and shared Execution polling owner; extend their composition only as needed so acquiring the retained initialization authority and awaiting the existing shared connect operation are bounded by that same caller deadline. After connection, shared wait receives only the remaining budget. A zero or expired budget starts no connection/status IO. Merely subtracting elapsed time after an unbounded synchronous connect does not close the gap. Timeout/failure must leave initialization available for a later ordinary retry and preserve once-only successful cache/client retention. Do not introduce a second credential resolver, transport, background connector, Python polling loop or global auth timeout setting.

Affected consumer: Python lazy Execution first wait. Preserve standalone status, explicit eager connect, ready queued handles, Rust/TS ready-handle waits and other ordinary LazyConnection consumers (Cards, Bifrost, Gateway, Principals and OperatorConnections). The existing connection owner owns contention and is the right correction boundary; another downstream status guard cannot bound a preceding phase. Server work continues independently when local waiting expires.

### FIND-004-8: Check is a fixed schema, not open result metadata

`RequirementResult.check` at `sdks/wyrd-sdk-python/python/wyrd/verify/__init__.pyi:176` remains `dict[str, JsonValue]`. Shared `verification/execution.rs:183-211` instead stores the closed Check enum (`card/blocks/mod.rs:255-281`), tagged assert/trace/agent/code/drift with fixed nested fields and deny_unknown_fields. Both native requirements and nested execution-status result serialize that same value unchanged. The current type fixture at `tests/typecheck/verify_contracts.py:63` accepts a made-up Agent object; impossible kind/key combinations are legal under the annotation. This leaves the explicitly requested fixed result typing incomplete despite the corrected surrounding records. Existing scheduled drift consumers also inspect these check fields.

Finish the existing authored wire declaration once with the actual five-variant discriminated Check contract, matching serde field names, nested reference/configuration alternatives and omission/null behavior. Reference it from the existing RequirementResult type so direct and nested consumers close together. Reuse matching serialized aliases where present; native CardRef/Agent objects cannot stand in for dictionaries. Keep genuinely open comparison expected/schema/payload JSON leaves open. Preserve runtime dictionary serialization and existing native VerificationResult wrappers. No runtime parsing, Pydantic, nested native wrappers, generator, normalization or second contract authority is needed. Update the existing positive type fixture to real check data and prove invalid fixed fields are refused. This is completion of an approved schema projection, not a new public behavior decision.

### FIND-004-10: two new fixture items omit mandatory panic documentation

AGENTS §16 expressly includes private helpers/tests and requires Panics whenever a panic remains possible. `crates/wyrd-spec/tests/verifier_fixtures.rs:171-175` (`sibling`) calls parent.expect but documents no panic contract; `:187-207` (`fixture_verifiers_satisfy_the_staged_contract`) omits fixture loading/resolution/validation/assertion failure conditions. Neighboring new fixture helpers already document them. Add the two substantive Panics sections describing the existing invariants/failures. Preserve the approved test, its assertions, its failure model and all validation. Do not add guards, rewrite its loader, remove the test or introduce a checker. Batch this comment correction with the two substantive findings; no style-only round.

## Execution scenarios and proof

1. For wait budgeting, use one Behavior/RED/GREEN/REFACTOR scenario at a time in the existing public Python execution-server unit-test owner. First demonstrate zero-budget/no-auth, then a delayed first exchange losing to a shorter budget without a later status read, contention behind another initializer expiring within the caller's own budget, and successful retry/reuse after timeout. Use the existing shared runtime/client and local test facilities. Record actual failure cause, exact new pytest selector and passing result after each case exists; do not invent selectors or manufacture a different server harness. Preserve existing local/eager/queued cases.
2. For fixed typing, extend existing `verify_contracts.py` with real representatives of all five check discriminators, discriminator-based branch access, rejected unknown kind/key/wrong fixed-field type and accepted arbitrary JSON at a genuine open expected/schema leaf. Keep unused suppressions failing. Source/typecheck proof is the relevant evidence; no runtime parser or server test is required for authored typing.
3. For panic docs, inspect the two complete bodies and add their missing contract sections. Static review and format/owner lint suffice; no manufactured RED test for comments.

Existing exact preservation commands:

```sh
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py::test_execution_constructs_offline_and_connects_on_first_read tests/unit/state/test_observe_surface.py::test_execution_connect_exchanges_now
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=observe::tests::wait_deadline_bounds_slow_reads_and_starts_no_read_after_expiry) | test(=observe::tests::enqueue_replays_its_key_and_wait_returns_only_a_real_result) | test(=observe::tests::wait_raises_terminal_errors_and_times_out_without_a_result)'
```

Build the modified Python boundary with existing `mise run py:setup` before its runtime tests. Run the existing focused file/export tests after the exact new selectors:

```sh
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py tests/unit/test_public_exports.py
```

Final narrow lanes (Cargo-backed work sequential):

```sh
mise exec -- cargo fmt --all --check
mise exec -- cargo clippy --locked -p wyrd-sdk-python --all-features --lib -- -D warnings
mise run py:format:check
mise run py:lints
mise run py:typecheck
```

If shared Rust source changes are genuinely needed, include that touched owner in the same scoped Clippy command and exact owner tests. The two fixture rustdoc sections require source inspection; their existing exact behavior check is:

```sh
mise exec -- cargo nextest run --locked -p wyrd-spec --test verifier_fixtures -E 'test(=fixture_verifiers_satisfy_the_staged_contract)'
```

It already passed independently; a comment-only edit does not require another behavior test. Do not broaden into full journeys, gate, TS/native generator, codegen or docs-site aggregates unless their owning sources actually change. Normal runtime tests need local port binding; if the execution environment denies it, record that exact blocker and do not claim a pass or weaken the test. Never print environment variables.

## Acceptance and preserved boundaries

| Finding | Required closure |
|---|---|
| FIND-004-1 | One deadline bounds implicit initialization authority acquisition, auth and shared wait; zero budget starts no IO; timeout releases initialization for retry; eager/status/ready/cache/error/server-lifetime semantics remain. Exact Python boundary cases pass with recorded selectors. |
| FIND-004-8 | Check is a precise five-variant serialized union throughout direct/nested requirement records; valid fixed shapes narrow, impossible kinds/keys/types fail, actual JSON leaves stay open; Python lint/typecheck/export parity preserved. |
| FIND-004-10 | Both cited items substantively document panic conditions in this same packet without validator/assertion changes. |

No new dependency, transport, durable queue, authorization policy, server engine, runtime parser, permanent check, external example change or unrelated cleanup. Keep all approved extra R1 changes and existing nine tracked assets. Do not reopen superseded FIND-004-7 or provisional code.run arguments. Preserve genuine result_id detail joins and execution_id correlations. Full real-server journey execution remains deferred to merged-change review after TASK-005. No spec revision is required or authorized by this packet.

Completion evidence maps every retained ID to source, exact focused proof, narrow lanes and remaining environment limits. Implementation completion is not review PASS. Route the finished cumulative candidate back to `$wyrd-task-review`.

## Implementation evidence

Status: `IMPLEMENTED`. Commits on `change/verifier-building-blocks-task-004`: `4d98f8f17` (FIND-004-8, FIND-004-10) and `1eb3f6bd3` (FIND-004-1).

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-004-1 | `1eb3f6bd3`. Reuse: the existing `LazyConnection` still owns first-connection contention, and the shared `Execution` still owns polling. Gap: the gate was a blocking mutex taken before the deadline existed. Fix: the gate becomes an async `tokio::sync::Mutex`, using the workspace `tokio` already in the lock. `LazyConnection::get_before(deadline)` checks for expiry before any work and bounds both the gate and `connect` with `timeout_at`. Shared `Execution::wait` now delegates to the new `wait_until(deadline)`, and `Execution::wait_timed_out` is the single timeout-error constructor. `PyExecution::wait` starts one deadline, then connects and polls against it. | RED: all three new tests failed. Zero wait sent `/auth/token`, and both slow cases waited about 3 s. GREEN: `mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py::test_execution_zero_wait_starts_no_connection tests/unit/state/test_observe_surface.py::test_execution_first_wait_bounds_a_slow_exchange_and_retries_after tests/unit/state/test_observe_surface.py::test_execution_wait_behind_another_first_connection_keeps_its_budget` gives 3 passed. Preservation: `test_observe_surface.py` plus `test_public_exports.py` give 99 passed. The three shared `wyrd-client` wait tests give 3 passed. | PASS |
| FIND-004-8 | `4d98f8f17`: `Check` in `wyrd/verify/__init__.pyi` is a union of five kinds (`AssertCheck`, `TraceCheck`, `AgentCheck`, `CodeCheck`, `DriftCheck`). It has the serde field names, references as `CardRefWire`, sibling or path, and `InlineCodeSource \| ArtifactCodeSource`. `RequirementResult.check` uses it. The two drift consumers in `test_scheduled_drift_alerts_operator.py` now narrow on `kind`. The open leaves are `expected`, the operator's parameter object, the drift signal, condition and profile, and the inline Agent spec. | `verify_contracts.py` accepts every kind and narrows on `kind`. It rejects an unknown kind, a foreign key and a wrong field type, with every suppression used. `mise run py:typecheck`, `py:lints` and `py:format:check` are ok. | PASS |
| FIND-004-10 | `4d98f8f17`: `# Panics` sections on `sibling` and `fixture_verifiers_satisfy_the_staged_contract`. | `mise exec -- cargo nextest run --locked -p wyrd-spec --test verifier_fixtures -E 'test(=fixture_verifiers_satisfy_the_staged_contract)'` gives 1 passed. | PASS |

Lanes: `cargo fmt --all --check`, `cargo clippy --locked -p wyrd-client -p wyrd-sdk-python --all-features --lib -- -D warnings`, `cargo clippy --locked -p wyrd-client --all-features --tests -- -D warnings`, `mise run check:deps` (new `tokio` edge) and `git diff --check` are all ok.

Notes:
- The wait-timeout message is now "did not settle before the wait deadline". The previous message included the timeout, which would misstate the remaining budget after a connection; no test or consumer matched the old text.
- The operator stays `str | dict[str, JsonValue]`. Those are its two serde alternatives, and copying its roughly 50 discriminators into the stub would duplicate the Rust enum.
- Non-goals are excluded: no new transport, resolver, background connector, Python poller, runtime parser or external change. Journeys remain deferred.
