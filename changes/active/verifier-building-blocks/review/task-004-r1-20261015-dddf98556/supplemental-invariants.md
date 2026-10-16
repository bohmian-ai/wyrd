# Independent invariant review — TASK-004 R1

Result: **FAIL** (two proposed incomplete closures, subject to independent validation).

Subject: base `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`, candidate `dddf985563029a004fa768110e36d0c5f12bcded`. Reviewed original TASK-004 and r8 authority, prior verdict/R1 diagnoses, cumulative runtime/SDK/example/docs changes and the R1 diff. Candidate HEAD was unchanged at completion. No peer discovery reports were read. CodeGraph was used before symbol exploration. This report does not implement corrections.

Human decisions are authoritative: external repositories are TASK-005; FIND-004-7 is superseded; fixture validation and PDF attributes are approved; real-server journey execution is deferred; code.run arguments remain provisional. None is challenged here.

## Producer-to-consumer map

- `Run` retains exact subject and application invocation. `WyrdState::bound_verifier` resolves bound metadata name, `Target::project` maps slots, and `Target::request` preserves subject/Verifier/Run identities. Run evidence uses existing HTTP and storage owners; Window evidence uses the existing drift observation writer and Bifrost lifecycle.
- `File` retains local bytes/path without IO; `RunEvidence::pin` hashes, checks format/size and plans only missing tenant digests, then delegates to the existing storage upload owner. `ArtifactRef` stays an exact Card/file declaration. No new registry or PUT transport is introduced.
- `Target::enqueue` mints one key per request and uses shared transport replay; returned Execution retains the shared client. Rust and TS wait enter shared `Execution::wait`; Python's newly lazy constructor introduces a preceding `LazyConnection::get` phase. This is the remaining deadline seam below.
- Shared fixed result types serialize through Python `json_to_pyobject`, while Rust returns owner records and TS projects generated wire types. Python now types most nested records, but still erases the closed Check declaration below.
- Three-language scheduled helpers now select `execution_id` and attribution filters use it; actual result-detail joins retain `result_id`. Shared scheduled fixture registration waits on the real fitter, recorded samples cross the real SDK boundary, and actual scheduler controls/readback/Operator receiver remain in the authored tests.
- The canonical policy example has the same two requirements and cited policy preparation in all three languages. The larger primitive fixture graph and deterministic upstream responders now author missing action/check/tool failure paths. Nine source files are tracked at the exact paths consumed by all language extraction journeys.

## Acceptance matrix

Verification references below include the orchestrator's independently executed `verification.md`. Historical implementation evidence is supporting evidence only. PASS for journeys means authored/compiled/typechecked/discovered, not executed.

| Requirement, criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001/002; exact bound subject, input slot and Run identity; no SDK engine | `observe/mod.rs` Run/Observe/target; `verify/mod.rs` Target/project/request; `state.rs` bound-verifier lookup | Client owner scenarios exist; scoped clippy passes; runtime owner tests cannot bind in sandbox | PASS |
| REQ-010/011 lazy files, original evidence identity, storage reuse, Window Recorded vs Run Queued | `File`, `ArtifactRef`, `RunEvidence::pin`; Observe submit/verify; storage upload existing dispatch extension | Owner negative/transfer tests authored; nine tracked sample paths checked with `git ls-tree`; fixture validation exact test passes | PASS |
| REQ-011/013 immediate non-replay vs stable enqueue retry identity | Target execute/enqueue use shared HTTP owner and one generated enqueue key | Existing replay/status test retained; exact selected tests blocked before assertions by socket denial | PASS |
| REQ-012 wait budget includes all operation IO | Shared wait now uses timeout_at, but Python first-use connects before entering it | New shared test covers delay/retry/zero; no lazy first-wait deadline test; source trace I-R1-001 | FAIL |
| REQ-012 Python lazy/eager construction, explicit/ambient retention and retry | `PyExecution` id + LazyConnection; queued uses ready; handle uses existing connect owners | Python unit construction/first-read/eager tests; py:typecheck passes | PASS |
| INV-002; TS scalar/array/File/Artifact positional parity, named object maps and explicit object disambiguation | index.ts nativeInputs/nativeInput and Observe signatures; native verification.rs inputs maps unslotted singleton to Inputs::One | Added wrapper unit scenarios; generated declarations; local tsc and journey discovery pass | PASS |
| REQ-011/015 precise shared fixed result projections | Python verify stub records; native getters serialize shared result; closed Check remains dict | Type fixture proves most records; fixed Check hole I-R1-002 remains | FAIL |
| Mandatory extract/action/check matrix and negative paths | Three verifier_building_blocks suites, shared policy-review/refund-reader/policy-judge/document-formats fixtures | Rust integration compile/list, Python collection/typecheck, TS list/typecheck pass; real execution deferred | PASS |
| Hosted tools allowed/missing invoke/missing resource/exact scope/delegation | investigator/tool-review, scoped keys, assert_tools and retained sub/act attribution in all three languages | Authored concrete SDK/server assertions; compilation/typechecking/discovery; enforcement execution deferred | PASS |
| Scheduled drift full lifecycle; PSI/SPC/Custom and nonfailure no-dispatch | Existing scheduled and observe-a-run owners; new latency-control/latency-steady and ready SPC story; no seeded fitted state | All three new stories inspect retained statistics; compile/typecheck/list passes | PASS |
| REQ-012/015 execution identity distinct from result row identity | All three shared scheduled readback helpers and attribution filters use execution_id; result_id joins unchanged | Source/SQL inspection plus integration compile/typecheck | PASS |
| REQ-016 examples and REQ-017 docs/architecture reference closure | Canonical support-desk in three languages, extraction/runtime/tool references, architecture diagram, generated type sources | Prior task checks retained; R1 does not change product prose; external example scope removed by r8 | PASS |
| Mandatory new ownership-field rustdoc | PyExecution id/inner and PyVerificationResult tuple field have substantive docs | Source inspection; scoped all-features library clippy passes | PASS |
| Non-goals: no SDK durable queue/authorization/provider implementation, no alternative transport, no compatibility aliases | Shared owner flows and thin language projections; no new production dependency in R1 | Diff/source and manifest inspection; approved extra changes excluded from drift claims | PASS |

## Prior finding closure

| Prior ID | Assessment | Evidence |
|---|---|---|
| FIND-004-1 | Partially closed; proposed retain | Shared polling is bounded, but first lazy Python wait connects outside budget (I-R1-001). |
| FIND-004-2 | Closed | Local constructor parses/stores id and optional client; eager connect invokes same LazyConnection; queued handle uses ready; failure not retained. |
| FIND-004-3 | Closed | Both TS methods project unslotted singleton values into Inputs::One; object maps stay named and positional:true is explicit. |
| FIND-004-4 | Closed | All nine samples appear in candidate git tree under samples; three consumers use these paths. |
| FIND-004-5 | Closed at authored-proof stage | All eight retained missing-row families now have actual three-language assertions and deterministic fixtures/responders. Shared real fitter/scheduler retained. |
| FIND-004-6 | Closed | execution_id projects through readback/reconnect/attribution; genuine result_id detail joins preserved. |
| FIND-004-7 | Superseded | Explicit owner/r8 decision; no external requirement remains in TASK-004. |
| FIND-004-8 | Substantially but incompletely closed; proposed retain | Fixed status/result/citation/evidence/provenance records exist; exact check remains arbitrary JSON map (I-R1-002). |
| FIND-004-9 | Closed | Both ownership fields, including lazy replacement fields, have substantive rustdoc. |

## Proposed findings

### I-R1-001 — FIND-004-1: Python's first lazy wait excludes credential connection from its deadline

Classification: **INCORRECT**. Obligation: REQ-012 caller-supplied wait timeout; R1 explicitly requires the caller deadline to win over auth/status/retry/body IO and preserve the same behavior in every language.

Location: `sdks/wyrd-sdk-python/src/verify.rs:198-202`, especially `self.handle(py)?.wait(timeout)` at 201. Producer/owner: `src/client.rs:137-166` LazyConnection::get; shared deadline starts at `crates/shared/wyrd-client/src/verify/execution.rs:112`.

Reachable path: `Execution(id, WyrdClient(..., api_key)).wait(0.02)` constructs lazily, then `wait` evaluates `self.handle(py)?` synchronously before building the shared wait future. `handle` calls existing LazyConnection::get, which takes its connection mutex and block_on's `Execution::connect_with`; `client.connected()` resolves/exchanges the key. Only after that IO succeeds does Execution::wait compute its deadline. A delayed 300 ms token exchange therefore precedes a fresh 20 ms poll budget; even a zero wait sends the exchange. Concurrent first calls may also wait for this gate outside their caller budgets. A failed exchange can return its auth/transport failure after the caller timeout should already have won.

Sibling paths: Rust requires connect before wait and TS has eager connect/private construction, so their connection operation is separately visible to callers; Python's explicit connect does too. Python queued submissions retain an already connected Execution via ready and bypass the problematic initial path. Python standalone status is allowed to connect without a wait budget. This finding targets only connection implicitly performed by a wait invocation, not explicit connect or global auth policy.

Observable consequence: the advertised local wait bound fails precisely for the newly restored Python lazy form; an unavailable credential service can block far beyond the requested timeout. The shared deadline fix is valid but does not close this new preceding phase.

Smallest correction outcome: start one caller budget before the implicit connection and ensure lazy connection/gate/exchange and polling consume that same remaining budget. Reuse the existing LazyConnection and shared Execution owners, retaining failed-connect retry, explicit eager connect, queued retained clients, standalone status, and server work lifetime. Do not introduce Python polling or a second transport, and do not modify global client timeouts. The implementation may extend the existing connection owner where needed; a guard only around already-connected status cannot solve it.

Focused proof: extend the existing local Python execution-server unit test with a delayed initial token response and a zero wait. A lazy short wait must time out within its budget, zero wait must send no connection/status request, and later normal operations must retain existing retry/reuse semantics. Existing connected shared wait tests remain required. A review-only inline repro was attempted through mise/uv --no-sync; OS socket construction was denied before the test body, so no runtime result is claimed.

### I-R1-002 — FIND-004-8: the fixed check declaration is still erased into arbitrary JSON

Classification: **MISSING**. Obligation: precise fixed-record wire views with intentional JsonValue only at genuinely open leaves (R1 FIND-004-8), typed breakdowns under REQ-011/015, and Python public type precision.

Location: `sdks/wyrd-sdk-python/python/wyrd/verify/__init__.pyi:176` (`RequirementResult.check: dict[str, JsonValue]`); proof fixture `tests/typecheck/verify_contracts.py:63` explicitly accepts a made-up `agent` dictionary. Shared source: `crates/wyrd-spec/src/verification/execution.rs:205` uses Check; `card/blocks/mod.rs:258-281` is the closed tagged assert/trace/agent/code/drift enum with fixed per-kind fields and deny_unknown_fields.

Producer → consumer: requirements getters serialize shared RequirementResult, including this exact Check enum, to dictionaries. Both `VerificationResult.requirements` and nested `Execution.status()['result']['outcome']['requirements']` use this same stub. A reader can assign/check `{'kind': 'potato', 'unknown_field': 3}` with no type error; dispatching on check['kind'] cannot narrow to the actual check shape. The new type fixture even labels `agent: {'anything': ['json', 1]}` as an accepted actual status shape, whereas this is not a valid AgentCall. Rust/TS retain the Check contract. JSON expectation/output/arguments/schema leaves are open, but the entire check declaration is not.

Consequence: a remaining fixed public record permits invalid discriminators/fields and hides declared check fields from Python consumers; changes to check structure cannot expose broken typed consumers. This is the same prior type-erasure issue at the remaining fixed producer, not a request for runtime parsing or server behavior.

Smallest correction outcome: finish the existing authored stub wire projection with a discriminated Check wire union that follows the five existing Rust variants and their fixed fields, reusing matching public aliases where available and leaving only genuine JSON leaves open. Reference it from RequirementResult.check. Preserve ordinary dictionaries, native result wrappers, and all serialization. No Pydantic model, generated schema owner, runtime wrapper, or runtime validation is needed.

Focused proof: update the existing typing fixture to use a real check declaration; reject unknown check kind/key and wrong fixed field types while permitting arbitrary JSON in expected/arguments/schema leaves. Run the existing py:typecheck/stub lint owners. No server is required.

## Verification and limits

The orchestrator independently passed Rust integration compilation/listing, exact fixture validation, scoped all-features library clippy, Python typecheck/collection and TS local typecheck/discovery. Client runtime unit tests compile/select but socket binding is denied in this sandbox; my review-only Python local-server repro encountered the same OS restriction. Full journeys, gate and external repository execution were deliberately not run. No environment values were printed and no production source, existing review artifact, commit or push was changed.

No additional optional improvement or architecture redesign is proposed. Remaining risk is the two concrete incomplete contract closures above; real composed engine/server behavior is still intentionally deferred to merged-change review.
