# Independent structured Ponytail validation

Validation result: **bounded corrections remain**. The final deduplicated ledger contains **FIND-004-1, FIND-004-8, and FIND-004-10**. No specification revision is required. This report supplies source validation; the orchestrator owns the verdict and remediation packet.

## Subject, authority, and method

Immutable base `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`; candidate `dddf985563029a004fa768110e36d0c5f12bcded`; prior candidate `07adb7907951f343544e550f6cabc5b7aa3aadea`. HEAD was independently checked and matches. Inputs include the cumulative and remediation diffs, original TASK-004, approved spec r8, prior verdict/FIND-004-1..9 and R1 diagnosis/evidence, all seven required discovery reports, supplemental-invariants.md, claims-comparison.md, followup-review.md, and verification.md. The replacement invariant report is the required independent pass; the supplemental report contributes proposals only.

Applied AGENTS.md, agent-rules, the reference router, spec-driven-development, maintainer-style and Python API/stub rules, under Wyrd's client/server and Verifier authority. CodeGraph preceded source navigation. Inspected actual complete corrected owners and immediate consumers, including shared Execution, PyExecution, LazyConnection, shared credential connection methods, result serializers and Check producer types, TypeScript input conversion, fixture loading/resolution/validation helpers, typing fixtures, and journey/readback paths. Findings are decided from these sources, not reviewer counts or implementation PASS labels.

Human authority is accepted: extra remediation edits and fixture gate/PDF rule are approved; FIND-004-7 is superseded by r8; external apps and provisional code.run arguments belong to TASK-005; live journey execution is deferred. No approved addition is reclassified as drift.

## Every proposal and conflicting closure claim

| Discovery proposal or disputed claim | Independent disposition | Source reason / final boundary |
|---|---|---|
| BEH-R1-001 | **CONFIRMED**, FIND-004-1 | Python evaluates handle acquisition before the shared wait future and deadline exist. |
| INV-R1-01 (required replacement invariant pass) | **CONFIRMED**, FIND-004-1 | Same preceding connection/gate lifetime; no separate finding. |
| MNT-R1-001 | **CONFIRMED**, FIND-004-1 | Same public caller budget defect, not a request for a new polling owner. |
| SYS-001 | **CONFIRMED**, FIND-004-1 | Slow credential dependency can outlive Python caller budget; request-local correction, no shared-service policy change. |
| CONC-R1-01 | **CONFIRMED**, FIND-004-1 | The synchronous initialization gate as well as authentication precedes the deadline. |
| Supplemental I-R1-001 | **CONFIRMED**, FIND-004-1 | Independently traced despite the supplemental report not qualifying as the required invariant pass. |
| Supplemental I-R1-002 | **CONFIRMED**, FIND-004-8 | `check: Check` is a closed schema, not arbitrary metadata. |
| followup-review.md Check proposal | **CONFIRMED**, FIND-004-8 | The follow-up's source reading is correct; correction is an authored wire declaration, not runtime validation. |
| Behavior, replacement invariant, standards and maintainer claims that FIND-004-8 is fully closed / Check is an intentional open payload leaf | **REJECTED** | Actual/output/tool argument JSON leaves are open; the entire Check declaration is not. Passing the present type fixture does not prove this missing negative case. |
| STD-R1-001 | **CONFIRMED**, new FIND-004-10 | Exact AGENTS §16 applies to new helpers and test functions and expressly requires Panics whenever a panic remains possible. Both cited items omit it. Batch with substantive corrections. |
| Security report's empty finding set; other reports' no-additional-finding statements | Validated within their reviewed boundaries | No unreviewed security policy change is necessary for these corrections. Hosted-tool execution remains deferred, rather than proved green. |

There are no further unique material proposals to discard. Non-blocking import/qualified-path preferences in standards-review.md do not enter the ledger: they have no established public-contract or behavioral consequence. No optional refactor, new dependency, checker, schema generator, transport, runtime wrapper, or speculative hardening is accepted as remediation.

## Final stable finding ledger

### FIND-004-1 — CONFIRMED / INCORRECT — implicit Python connection precedes the wait deadline

**Sources:** BEH-R1-001, INV-R1-01, MNT-R1-001, SYS-001, CONC-R1-01, supplemental I-R1-001. Prior ID retained because the governing invariant remains the caller's bounded wait; FIND-004-2's new lifecycle exposes another phase of that same operation.

**Obligation:** spec REQ-012's caller-supplied timeout and Python lazy form; original TASK-004 Scenario 2; R1 FIND-004-1's explicit deadline-over-auth/status/retry/body requirement and zero-budget behavior.

**Exact location:** `sdks/wyrd-sdk-python/src/verify.rs:198-203`, especially line 201; upstream `PyExecution::handle:121-128` and `sdks/wyrd-sdk-python/src/client.rs::LazyConnection::get:140-163`; shared deadline begins at `crates/shared/wyrd-client/src/verify/execution.rs:112`.

**Producer → consumer proof:** `PyExecution::__new__` parses and retains the ID with a disconnected LazyConnection. Python `wait` parses its duration, then synchronously evaluates `self.handle(py)?` as an argument to the shared future. `handle` calls `get`, which releases the GIL, waits on the retained initialization mutex, and block_on's shared `Execution::connect_with` or ambient `connect`. Shared `connect_with` awaits `client.connected`; connected awaits `access_token`, which delegates to the existing AuthMiddleware bearer owner. Only after all of this returns is the shared `Execution::wait(timeout)` future polled and its deadline computed. A delayed initial exchange, refusal or another initializer therefore wins outside the budget. Successful initial connection also incorrectly grants the caller a fresh full polling budget afterward.

**Reachability and independent executable corroboration:** ran the installed public Python boundary through `WYRD_LOG=info mise exec -- sdks/wyrd-sdk-python/.venv/bin/python`, constructing the same synthetic ID as the existing test with an explicit dummy API-key client at `http://127.0.0.1:9`, then `wait(0)`. Actual output was `WYRD_CLIENT_503_TRANSPORT_DOWN`, not `WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT`. This corroborates attempted pre-deadline credential IO without requiring a listener. The complete source ordering establishes the slow-success and contention cases; no timed delayed-server result is claimed.

**Sibling owners and preservation:** `queued` uses `LazyConnection::ready`; successful first use caches the same shared Execution; eager `connect` intentionally completes connection as its own separate operation; `status` has no wait budget. Rust and TS reconnect factories are explicit eager operations; their subsequent ready waits enter the shared deadline owner directly. Other LazyConnection consumers include Cards/SharedCards, Bifrost, Gateway, Principals and OperatorConnections. Their ordinary no-deadline operations, cached client selection and retry-after-failure must stay intact. There is no durable authority writer in Python: shared client/auth remain the credential owners, server work remains server-owned, and shared Execution remains the sole polling owner.

**Consequence:** a Python caller's selected wait timeout fails to bound first-use authentication or contention, and even an expired zero-budget operation can produce a transport/authentication error. This is a reachable required public path, not an unrelated global auth timeout concern.

**Smallest safe correction:** begin one monotonic caller budget at Python wait entry. Extend the existing LazyConnection initialization boundary only as needed to allow that budget to bound both acquisition of its existing once-only initialization authority and the shared connection future, then give shared Execution waiting only the remaining time. Reuse its retained client, one-success cache and shared auth/transport; do not duplicate connection resolution, polling, or retries. Do not accept subtracting elapsed time after unbounded initialization: it still violates the operation bound. A zero/expired budget must start no connection/status request. A timed-out or failed initialization must not leave the owner locked or permanently poisoned, and later ordinary first use must remain possible. Already-ready/eager handles, standalone status, queued client retention, terminal errors, and server execution lifetime remain unchanged. This boundary owns initialization serialization and therefore legitimately enforces the caller deadline there; another guard only around the downstream status future cannot close the defect.

**Focused closure proof:** use the existing Python execution-server unit-test owner and public SDK imports. Cover zero budget with no exchange/status requests; delayed first exchange losing to a shorter wait budget with no subsequent status read after expiry; a wait blocked behind another initializer expiring within its own budget; and subsequent successful initialization/reuse after timeout. Preserve existing offline ID, refused-first-use retry, eager connection and returned-ready behavior. Record exact pytest selectors after the new cases exist; do not invent test names. Existing exact preservation selectors:

```sh
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py::test_execution_constructs_offline_and_connects_on_first_read tests/unit/state/test_observe_surface.py::test_execution_connect_exchanges_now
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=observe::tests::wait_deadline_bounds_slow_reads_and_starts_no_read_after_expiry) | test(=observe::tests::enqueue_replays_its_key_and_wait_returns_only_a_real_result) | test(=observe::tests::wait_raises_terminal_errors_and_times_out_without_a_result)'
```

No full server journey or new harness is needed. Normal focused execution requires the local listener capability that this review sandbox denies.

### FIND-004-8 — CONFIRMED / MISSING — the fixed Check wire schema remains erased

**Sources:** supplemental I-R1-002 and followup-review.md; prior ID retained. Most original response-record erasure is corrected; this remaining fixed field is the same root contract gap.

**Obligation:** original typed result/handle task, REQ-015's exact check configuration in the breakdown, R1 FIND-004-8's precise fixed-record TypedDict/discriminated-union correction with JSON only at true open leaves, Python Stub Rules and maintainer public-type guidance.

**Exact location:** `sdks/wyrd-sdk-python/python/wyrd/verify/__init__.pyi:176` (`RequirementResult.check: dict[str, JsonValue]`), with false positive shape in `tests/typecheck/verify_contracts.py:63`. Producers: `crates/wyrd-spec/src/verification/execution.rs:183-211` and `crates/wyrd-spec/src/card/blocks/mod.rs:255-281`.

**Producer → consumer proof:** shared RequirementResult's `check` is a `Check`, the tagged enum of assert/trace/agent/code/drift, with deny_unknown_fields. Assert has declared reference/operator fields and an open expected value; trace has declared on/selector/operator and expected; AgentCall has an InlineableRef<AgentSpec> plus declared reference maps; CodeCall has runtime/source/entrypoint/reference inputs; DriftCheck flattens the existing declared DriftSpec beside on. Ref and InlineableRef serialize declared alternatives (exact CardRef, sibling, path, inline where permitted), not arbitrary metadata. PyVerificationResult::requirements serializes those exact shared records unchanged through serde/json_to_pyobject. PyExecution::status uses the same serialization nested under result.outcome. The one stub alias is used by both consumers, so one declaration correction covers them.

The current annotation permits an unknown kind/unknown fields as long as their values are JSON. Its positive fixture even accepts `{"kind": "agent", "agent": {"anything": ["json", 1]}}`, which is not the declared Agent reference or inline spec shape. A Python caller cannot narrow by the closed discriminator or receive useful validation of fixed Check fields. The scheduled drift journey already reads `check["method"]`; these are actual result fields consumed by the task. Rust retains Check and generated TS retains the five variants, confirming shared ownership. Existing JSON fields elsewhere do not make Check an open leaf.

**Consequence:** the advertised typed fixed Python result contract still accepts impossible kinds/configuration and hides declared branch fields. Runtime serialization is faithful; this is a static public-contract gap, not a need for runtime parsing.

**Smallest safe correction:** finish the existing authored wire views at this field with the existing five-variant discriminated Check wire contract, following actual serde names, fixed nested configuration and optional omission behavior. Reuse a matching existing wire alias where available; native Python CardRef/Agent classes cannot stand in for serialized dictionaries. Keep genuinely JSON-owned leaves open, including comparison expected values and JSON schema/configuration leaves that are actually open in their producer. Source/reference/configuration maps that have fixed structure must retain that structure. Do not add a runtime response model, Pydantic, native nested wrapper, schema generator, normalization, or server/client serializer change. Update the existing positive fixture to a valid check and retain ordinary runtime dictionaries. This is declaration closure under the already-approved result schema, not a new product/API decision.

**Focused closure proof:** extend the existing `verify_contracts.py` fixture with a valid representative of each discriminator, unknown discriminator/key and wrong fixed-field rejection, discriminator-based access to a valid branch, and accepted arbitrary JSON at a real expected/schema leaf. Keep unused suppressions treated as failures. Use `mise run py:typecheck` and `mise run py:lints`, plus `mise run py:format:check` for edited Python. Existing runtime/export tests need remain green when the boundary is rebuilt for FIND-004-1; no runtime parser or real server is required to prove this typing correction. Avoid requiring a generated-contract lane unless its owning source is actually changed.

### FIND-004-10 — CONFIRMED / VIOLATION — omitted panic contracts in the approved fixture gate

**Source:** STD-R1-001. This is new R1 code and receives the next unused stable ID; FIND-004-9's wrapper-field documentation is closed.

**Exact authority:** AGENTS §16 lines 723-726 includes private helpers and test functions. Lines 731-734 say to add `# Panics` whenever a panic remains possible; lines 735-737 make missing touched-item rustdoc a hard blocker. The task-review skill explicitly preserves this documentation threshold. The rule is not limited to public production methods or Result-returning functions. Approval of the new fixture test is scope approval, not exemption from its documentation rule.

**Location and complete-body proof:** `crates/wyrd-spec/tests/verifier_fixtures.rs:171-175` (`sibling`) and `:187-207` (`fixture_verifiers_satisfy_the_staged_contract`). The former documents only path resolution and calls parent.expect; its callers are collect_bindings and resolve for Agent/Prompt sibling references. The test documents its intended successful validation but omits its panic conditions: load/read/decode/reference failures, absent collections, invalid Verifier or schedule bindings and unknown bound Verifier. Neighboring load/add/collect_bindings/resolve/decode/canonical helpers already document their panic contracts. No additional production behavior or alternate fixture resolver is needed.

**Consequence and selected correction:** the changed items fail the explicit documentation acceptance gate. Add substantive Panics sections describing their existing invariant/failure conditions, without changing validation/assertions or broadening documentation edits to untouched items. The parent-path invariant is already the expect message, not a request for a new runtime guard. Do not delete the approved test, change its failure model, or build a checker to satisfy this rule. Batch the two small documentation corrections with FIND-004-1/8; never create a style-only remediation round.

**Focused proof:** inspect both item docs against their full bodies and run formatting/owning lint for the touched Rust write set. The exact fixture test is already independently green:

```sh
mise exec -- cargo nextest run --locked -p wyrd-spec --test verifier_fixtures -E 'test(=fixture_verifiers_satisfy_the_staged_contract)'
```

A comment-only correction needs no new behavioral test or repeated journeys.

## Prior finding dispositions

| Prior ID | Final disposition | Source-backed closure or remainder |
|---|---|---|
| FIND-004-1 | **OPEN, partial closure** | Shared wait wraps complete status IO in timeout_at, clamps sleeps and skips expired reads; Python implicit initial connection/gate still precedes that deadline. |
| FIND-004-2 | **CLOSED** | PyExecution has local ID + existing LazyConnection; __new__ is local, connect initializes eagerly, queued uses ready, success is retained and failed initialization is retryable. Deadline interaction counted once under FIND-004-1. |
| FIND-004-3 | **CLOSED** | TS nativeInputs preserves named maps and explicit positional object disambiguation; native optional slot maps one unslotted value to shared Inputs::One. Both public methods use it; shared slot/type validation remains owner. |
| FIND-004-4 | **CLOSED** | Independent candidate git-tree inspection lists all nine samples (csv/docx/html/json/md/pdf/pptx/txt/xlsx); all language extraction paths point at samples. Approved PDF binary rule retained. |
| FIND-004-5 | **CLOSED at approved authored-proof stage** | Concrete three-language reader/code failures, malformed/foreign Verdict citations, sandbox/deadline paths, trace order/attributes, hosted-tool two-gate/exact/delegated assertions and real-fitter scheduled SPC story are authored. Source assertions inspect actual outcomes and provenance; verification.md supplies compilation/typechecking/discovery. Existing PSI/Custom retained. No execution is inferred. |
| FIND-004-6 | **CLOSED** | All three scheduled readers select execution_id and reconnect with it; attribution readers filter that identity. Actual result-detail identity joins are preserved. |
| FIND-004-7 | **SUPERSEDED** | Approved r8/human decision moves external repositories to TASK-005. No external correction enters this ledger. |
| FIND-004-8 | **OPEN, substantial partial closure** | Fixed state/result/outcome/citation/evidence/provenance views now exist and serialized nested status result is correctly distinct from the native wrapper; Check still erased as above. |
| FIND-004-9 | **CLOSED** | PyExecution id/inner and PyVerificationResult's tuple field now have substantive ownership documentation. New fixture docs are separately FIND-004-10. |

## Correction scope and verification limits

One bounded R2 packet can cover all three retained findings. Its implementation boundary is the existing Python lazy connection/wait composition, authored Python result wire views/type fixture, existing local Python boundary tests, and the two fixture rustdoc sections. No server worker, durable queue, authorization policy, alternate SDK polling engine, storage pipeline or external example change is needed. Review immediate changed-item docs in that same packet to avoid another documentation round. Follow the narrow existing binding build, fmt, affected all-feature library Clippy, Python lint/format/typecheck and exact runtime tests; do not prescribe a full journey suite or gate.

Independent verification.md/logs show integration Rust compilation/listing, exact fixture test, scoped library Clippy, Python typing/collection/lint/format, TS local typing/discovery/lint/format and source whitespace proof. Mock client tests compiled and were selected but local socket binding was denied before assertions, including the traced rerun. This limitation is not a green runtime test or a diagnosed production regression. My own zero-budget public Python probe above independently reproduced the error-contract defect; no additional Cargo lane was run. Historical review-diff whitespace is provenance and should not be rewritten.

No production code, existing report, environment values, commit, push, full journey suite or gate was changed/exposed/run. Remaining risks are the retained deadline and typing gaps and the expressly deferred composed server/journey proof after TASK-005.
