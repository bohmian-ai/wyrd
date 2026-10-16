# Independent behavior review — FAIL

Subject: TASK-004, base `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`, candidate `dddf985563029a004fa768110e36d0c5f12bcded`. This is a caller-to-result review of the cumulative task, using the prior candidate `07adb7907951f343544e550f6cabc5b7aa3aadea` to locate remediation. No peer report was read. Source was not modified.

Authority: approved specification r8, original TASK-004 and R1 packet, AGENTS.md, agent-rules, spec-driven-development and maintainer-style references, Wyrd design/doctrine. Human decisions in subject.md govern extra changes, external-example reassignment, provisional code.run arguments, and deferred real-server execution.

## Acceptance matrix

PASS for a journey row means authored boundary behavior with reported compile/typecheck/discovery evidence, not executed server behavior. The orchestrator owns Cargo verification.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001/002 generic hydrated slots; preserve missing evidence | Shared `verify/mod.rs::Target::project`, `state.rs::bound_verifier`, Rust Inputs; Python observe conversion; TS native conversion | Shared observe projection tests and three-language named-input journeys | PASS |
| Single-slot positional values and named map compatibility | TS `src/index.ts::nativeInputs/nativeInput`, native `verification.rs::inputs`; lone unslotted input becomes Inputs::One; object JSON explicitly disambiguated | TS observe unit tests and authored positional scalar/object/file/Artifact journeys | PASS |
| REQ-010 local lazy File and shared transfer reuse | File path/bytes declaration and RunEvidence::pin; sha256_file and WyrdStorageClient::upload_evidence reuse storage dispatch | File/oversize/missing-upload owner cases and extraction journeys | PASS |
| REQ-011 Run verify vs queued submit vs Recorded Window; Window verify refusal | Observe routes through Target and existing drift_value, rather than language engines | Shared routing/refusal tests; real-time, observed-run and scheduled journeys | PASS |
| REQ-012 retained client, ID, eager/ambient/explicit reconnect | Shared Execution; Python `LazyConnection::ready/new/get`; TS Execution delegates native | Shared reconnect tests; Python offline-first-use/eager tests; authored status/wait reconnect paths | PASS |
| REQ-012/013 caller timeout includes connection and read IO | Shared wait now bounds status and backoff; Python first lazy `wait` connects before entering that owner | Exact zero-budget Python reproduction below returns transport error instead of timeout | FAIL — BEH-R1-001 / retained FIND-004-1 |
| Honest status/wait: tolerate missing rows/results; expose terminal conflict/error, failed verdict as data | `verify/execution.rs::wait` retries visibility/5xx; no completion-only fabricated result; code_to_wyrd_error preserves stable error | Supporting scripted owner tests and authored result reconnect assertions | PASS apart from deadline row |
| REQ-013 one retry key/body; no silent direct replay or SDK durable queue | Target::enqueue mints once, shared submit_with_idempotency_key; execute has no key and uses shared transport | Existing enqueue replay and direct-timeout tests; static transport ownership | PASS |
| Required all action/check kind positives and failure rows in every SDK | `verifier_building_blocks` counterparts, shared policy-review/policy-judge/refund-reader fixtures; reader provider/schema/deadline, code raise/deadline, invalid Verdict/citations, trace order/attributes, sandbox refusals | Actual Rust/Python/TS case bodies inspected; R1 records compile/typecheck/discovery, execution deferred | PASS for authorship |
| Hosted Card/Bifrost/code tool allowed, denied, exact-scope and delegated calls | `script_investigator`/provider scripts request three tools; `assert_tools` checks returned bytes/digest/denial; granted principals and delegation narrow tools; SQL readback asserts subject/actor | Three counterpart authored tool tests inspected; code.run argument remains approved provisional seam | PASS for authorship |
| Required drift methods including ready numeric SPC | Existing Custom and PSI kept; latency-control + latency-steady, real fitter via wait_for_baseline, submitted observations, retained SPC chart assertion | Three `ready_spc_baseline_charts_a_scheduled_window` counterparts; reported compile/typecheck/discovery | PASS for authorship |
| REQ-014 scheduled Service fitting, submit/drain, failed-only Operator delivery | Watch lifecycle retains real fitter/scheduler/HTTP receiver; failed/pass/empty cases | Existing scheduled_drift_alerts_operator counterparts and helper source | PASS for authorship |
| Nine extraction formats runnable from clean candidate | `fixtures/cards/verifier_building_blocks/samples` and all three extraction consumers; locked expected outputs | `git ls-tree -r --name-only HEAD .../samples` confirms all nine assets tracked | PASS |
| REQ-012/015 execution correlation and analytical attribution | Three scheduled helpers select execution_id; three real-time readers filter r.execution_id; delegated result reader uses same identity | Source inspection across all language callers; legitimate result joins unchanged | PASS |
| REQ-015 typed fixed Python result dictionaries and runtime shape | verify authored TypedDicts reflect serialized inputs, citations, provenance, steps, outcome, nested result and state; native properties still serialize shared records | Typecheck fixture accepts nested wire record and rejects key/status/field mistakes; reported py:lints/typecheck | PASS |
| REQ-016 canonical matched example | Shared policy-check includes refund assertion and cited tool-less judge; all language serve/check/wait paths use same policy PDF/Card | Existing authored support-desk counterparts; reported example checks; external repository explicitly TASK-005 | PASS for TASK-004 authorship |
| REQ-017 extraction output/runtime/tool docs and architecture cutover | Existing guide replacement plus extraction-output, verification-runtime-limits, agent-tools-and-authorization; runtime parity caution is explicit | Reported docs/codegen/examples checks; no inference that pending startup/runtime seam shipped | PASS task-local |
| No alternate transport, SDK authorization, durable queue/engine, compatibility alias, new app framework | Shared owner implementations reused; wrappers only project values and foreign-runtime lifetime | Cumulative changed-owner and caller inspection | PASS |
| Changed wrapper ownership rustdoc | PyExecution id/inner and PyVerificationResult field now documented | Source inspection | PASS |

## Proposed material finding

### BEH-R1-001 — INCORRECT — Python lazy wait starts its timeout after first-use authentication

Retain stable **FIND-004-1**: its shared Rust status-read hole is fixed, but the new lazy first-use path introduced in R1 still excludes part of the caller operation from the budget. This is one caller-deadline invariant, not a separate policy.

**Obligation.** REQ-012/013 and TASK-004 Scenario 2 require bounded caller waiting; R1 explicitly requires the caller deadline to win over auth/status IO and zero-budget calls to start no read. Python's public `Execution.wait(timeout)` claims timeout-first behavior and matches this shared operation.

**Exact location.** `sdks/wyrd-sdk-python/src/verify.rs:198-203`, particularly `self.handle(py)?.wait(timeout)` at 201. `handle` at 121-128 calls `LazyConnection::get`; `sdks/wyrd-sdk-python/src/client.rs:146-167` takes the initialization mutex and `runtime().block_on(...)` around connect before returning. Shared `Execution::wait` computes `Instant::now() + timeout` only after that, at `crates/shared/wyrd-client/src/verify/execution.rs:111-112`.

**Producer → consumer and siblings.** `Execution(id, client)` stores a locally parsed ID and disconnected LazyConnection. Its first wait invokes handle; get may wait for another initializer and then connects with retained or ambient client, including credential exchange. Only after it completes does shared wait start its timer. An unavailable token endpoint therefore yields the connection error before the zero-budget wait can refuse, and a slow token endpoint can exceed any short budget and still grant the full budget again afterward. `Execution.connect` deliberately connects eagerly before wait; `PyExecution::queued` stores a ready shared handle, so neither needs its factory lifetime folded into subsequent waits. `status()` intentionally has no wait budget. TS and Rust reconnect factories remain eager and their ready handles use shared wait. Existing other LazyConnection users need their current unbounded non-wait operations preserved. No SDK transport, credential implementation, or server cancellation policy needs replacement.

**Observed consequence and focused proof.** With the installed Python boundary, the following exact narrow command returned `{'code': 'WYRD_CLIENT_503_TRANSPORT_DOWN'}`. A zero timeout is already expired and must return `WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT` without attempting authentication. Source shows the same ordering for delayed successful or refused authentication.

```sh
mise exec -- sdks/wyrd-sdk-python/.venv/bin/python - <<'PY'
from wyrd import WyrdClient, WyrdError
from wyrd.verify import Execution
execution = Execution('0190b2a4-0000-7000-8000-000000000000',
                      WyrdClient('http://127.0.0.1:1', 'wyrd_sk_t_v_s'))
try:
    execution.wait(0)
except WyrdError as error:
    print({'code': error.code})
PY
```

The attempted delayed-local-server reproduction could not bind a socket under this sandbox (`PermissionError: Operation not permitted`); this does not limit the source-proven ordering or zero-budget reproduction. The initial uv invocation could not write its global cache; direct existing venv execution under mise succeeded. No environment values were printed.

**Smallest testable correction.** Start one deadline at the Python wait boundary and include lazy initialization/serialization contention in its budget, reusing the existing LazyConnection and shared Execution wait. Preserve once-only retained connection, retry after failed initialization, eager connect, ready submissions, error metadata and independent server work. Pass only the remaining budget into shared waiting after connection. Do not merely subtract elapsed time after an unbounded blocking connect: authentication and initialization waiting must actually stop waiting at the caller deadline. Add Python-runtime focused cases for zero budget/no auth, delayed first exchange, and concurrent first initialization; retain successful first use, failed-connect retry and ready-handle reuse. A small deadline-aware connection capability on the existing owner is appropriate; a second connection resolver or per-language transport/polling policy is not.

## Prior finding closure

| Prior ID | Source-backed disposition |
|---|---|
| FIND-004-1 | **PARTIAL / OPEN**. Shared Rust timeout_at fixes status/auth refresh/retry/body reads and no-post-expiry polling on ready handles. Lazy Python first-use connect now precedes the timer; BEH-R1-001 above. |
| FIND-004-2 | **CLOSED**. Python local constructor, eager connect, ready submission and existing LazyConnection reuse are present; owner runtime tests exercise offline ID, first read, failure retry and reuse. Deadline consequence is charged once to FIND-004-1. |
| FIND-004-3 | **CLOSED**. Both TS methods retain named maps; unslotted native input reaches Inputs::One; object JSON can be explicit positional. |
| FIND-004-4 | **CLOSED**. Nine normally tracked sample assets and language path closure; binary PDF rule approved. |
| FIND-004-5 | **CLOSED for authorship**. All retained missing negative/tool/trace/ready-SPC cases now have concrete assertions across Rust/Python/TS. Runtime proof remains explicitly deferred. |
| FIND-004-6 | **CLOSED**. Scheduled/readback and attribution identity is execution_id across all languages; legitimate result joins retained. |
| FIND-004-7 | **SUPERSEDED** by authoritative r8/human scope decision; external repositories belong to TASK-005. |
| FIND-004-8 | **CLOSED**. Precise authored wire views replace Any records without altering serialization; correct nested result mapping and typing fixture present. |
| FIND-004-9 | **CLOSED**. Both changed ownership declarations have substantive field documentation. |

No optional improvement, approved-scope drift complaint, or new product decision is proposed. In particular, pending dispatch/binding/loader/server seams and provisional code.run arguments remain TASK-005, not TASK-004 defects.

Overall **FAIL** because the caller deadline still does not bound the required Python lazy wait path. Skipped full server journeys, gate and Cargo commands as instructed; real runtime journey success remains unproven until merged-change review.
