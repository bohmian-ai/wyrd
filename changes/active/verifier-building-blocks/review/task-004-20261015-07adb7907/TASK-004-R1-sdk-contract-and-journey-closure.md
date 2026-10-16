---
id: TASK-004-R1
kind: remediation
status: ready
spec: SPEC-verifier-building-blocks
spec_revision: 7
parent_task: TASK-004
remediates: [FIND-004-1, FIND-004-2, FIND-004-3, FIND-004-4, FIND-004-5, FIND-004-6, FIND-004-7, FIND-004-8, FIND-004-9]
---

# Close SDK contract and authored journey gaps

## Subject and authority

Route directly to `$wyrd-implement`; this is a bounded remediation under approved r7, not a new design or planning cycle.

- Repository root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-004`.
- Approved specification: `changes/active/verifier-building-blocks/spec.md`, SPEC-verifier-building-blocks r7.
- Original task: `changes/active/verifier-building-blocks/tasks/TASK-004-sdks-examples-and-docs.md`. Its r6 header is preserved history; r7 coordination/handoff remains authoritative.
- Coordination: `changes/active/verifier-building-blocks/tasks/README.md` and `TASK-005-server-integration.md` in that directory.
- Original cumulative base: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`; implementation candidate: `07adb7907951f343544e550f6cabc5b7aa3aadea`; packet/evidence HEAD: `aa32c821ef74886526b7367bfd77d51c87e0d521`.
- Prior verdict: `verdict.md` beside this task, FIX_REQUIRED; source-validated ledger: `findings-validation.md` beside it. Original authority/task snapshots and full cumulative diff are retained here.
- External demo root: `/home/thorrester/Documents/GitHub/wyrd-example-ts`; reviewed base `000ed1f2f587a6629a471c51aadff5e05f0c820b` plus the frozen uncommitted source in `external-source.tar.gz`, `external.diff`, and `external-snapshot.sha256` beside this task. `/tmp/wyrd-task004-external-07adb7907` is the extracted review snapshot. Reconcile current external source with that reviewed input before changing it; preserve unrelated edits and identify the final external candidate independently.

## Intended outcome and ownership

Restore the approved caller deadline, Python construction/typing, TypeScript input parity, execution identity, reproducible fixture assets, matched external policy semantics and complete required authored journeys. Keep the shared Rust client as the implementation owner. This task owns the shared client, necessary native/runtime projections and authored declarations, affected SDK tests/helpers/shared fixtures, the external policy Card/test/presentation consumers, and immediately touched rustdoc. Other agents may be working in these repositories; preserve their edits and coordinate shared files.

The following diagnosis and selected correction for each finding are independently validated. They form this task's required correction boundary; do not rediscover the same defect separately at each caller. Ordinary private structure and new test names remain implementation-owned.

## Issue diagnoses and selected corrections


### FIND-004-1 — wait's caller deadline excludes its status IO

**Sources/status/classification:** BEH-002, I-001, SYS-001, CONC-01; **CONFIRMED / INCORRECT**.

**Obligation:** REQ-012/013, TASK-004 Scenario2, and `Execution::wait`'s own documented timeout-first behavior.

**Location:** `crates/shared/wyrd-client/src/verify/execution.rs:108-145`, particularly `self.status().await` at 113 and successful return at 115.

**Producer → consumer:** enqueue and reconnect construct one Execution retaining WyrdClient. Its complete `wait` body computes a deadline, awaits `status`, returns visible results or terminal errors, and only then tests the clock. `status` at 77-87 enters `HttpTransport::request_json`; the full `send_with_retry` body at `transport/http.rs:991-1090` resolves credentials, applies its own per-attempt timeout, and may retry GET timeouts/5xx with backoff. Response decoding also occurs inside `status`. None receives the wait budget. A 20 ms wait with a completed response arriving after 200 ms returns that late result; sleeping to the deadline initiates another read before inspecting expiry. Every Rust caller, Python `PyExecution::wait`, TS `NativeExecution::wait` and the examples' shrinking-budget wait helpers share this owner. Existing scripted tests at `observe/tests.rs:1010-1149` answer status promptly and do not falsify this path.

**Consequence:** callers cannot bound local waiting during a slow endpoint, auth dependency, retry, or body read, and may receive a verdict after the advertised timeout has already won.

**Selected correction:** enforce the existing absolute caller deadline in shared Execution::wait over the whole awaited status operation and polling lifetime with the installed Tokio time facility. Expiry returns the existing SdkVerificationWaitTimedOut with execution/last-status details, and no read starts after expiry. This owner owns the invariant, so its deadline guard belongs here. Keep standalone status, normal transport/auth/retry behavior, late visibility tolerance, terminal errors before expiry, and the server execution unchanged. Do not change global client settings, cancel/resubmit server work, or add runtime-specific timers.

**Focused closure:** extend the existing wiremock owner tests to prove a delayed successful status cannot beat an earlier caller deadline, transport retry/backoff is bounded by it, and an expired/zero budget starts no extra poll. Retain the existing successful/delayed-visibility/terminal-refusal assertions. Run actual exact new selectors plus the two existing owner tests through `mise exec -- cargo nextest run --locked -p wyrd-client --lib`; record names after authoring, not imaginary passing selectors. No database or full journey suite is needed.

### FIND-004-2 — Python cannot construct an execution lazily

**Sources/status/classification:** BEH-004, I-003; **CONFIRMED / MISSING**.

**Obligation:** explicit REQ-012 Python lazy form, INV-002, TASK-004 Scenario3, Wyrd design Client model at 310-318.

**Location:** `sdks/wyrd-sdk-python/src/verify.rs:96-158`; authored `python/wyrd/verify/__init__.pyi:104-176`.

**Producer → consumer:** the complete PyExecution class has one already-connected Execution tuple field and only static connect/id/status/wait methods. Connect resolves the supplied or ambient shared client immediately. The public Python module directly reexports the native class; neither it nor its stub supplies a constructor. `observe/mod.rs:412` constructs the same class from queued native handles. Thus `Execution(id, client)` cannot retain an ID offline. Existing `client::LazyConnection<T>` at `client.rs:84-143` already retains optional client, serializes first use, caches successful connection and leaves failures retryable. Principals' complete constructor/connect/handle path demonstrates that same policy without new transport/cache ownership.

**Consequence:** Python consumers cannot reconstitute an ID before credentials/network availability, as approved; the eager-only class loses an intentionally supported lifecycle path.

**Selected correction:** give Python Execution its IO-free `Execution(id, client=None)` construction, retaining the parsed local execution ID and optional explicit client through the existing LazyConnection owner. Eager connect initializes that same connection path. Status/wait use the shared Execution connection and operations once connected; ID access remains local. Preserve the connected client/handle returned by submit rather than discarding it and performing a second exchange. Extend the existing lazy owner narrowly if it needs to retain an already-connected handle; do not introduce a competing cache, polling loop or Python credential resolver. Update authored stubs and public runtime tests together.

**Focused closure:** Python-runtime tests establish offline construction/ID access without credential exchange, first operation using the retained explicit client, reuse after a successful connection, retry after a failed first connection, and eager connect preserving eager refusal/exchange behavior. Use current Python local-server fixtures and public imports; run focused tests with the repository Python setup plus `py:lints`/`py:typecheck`. No real-server journey execution is required now.

### FIND-004-3 — TypeScript loses positional versus named input identity

**Source/status/classification:** I-002; **REVISED / MISSING**.

**Obligation:** REQ-002 ordinary named values and one positional value for single-slot Verifiers; INV-002 and TASK-004 Scenario3 first-class projections.

**Location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:2089-2110,2266-2267,2280-2281`; `native/src/verification.rs:35-96`; both native call sites `native/src/cards.rs:759-801`.

**Producer → consumer:** VerifierInputs accepts only a record. The complete nativeInputs function converts Object.entries into slot-tagged records, native `inputs` converts every element and always returns Inputs::named, and both NativeRun methods use it. There is no producer for Inputs::One. Rust Target::project at `verify/mod.rs:325-345` already resolves One against exactly one hydrated slot and owns the refusal otherwise; Python's complete inputs converter at `observe/mod.rs:165-193` preserves One/Named. TS cannot pass a bare string/File/ArtifactRef, and object JSON cannot be distinguished from a named slot map by a simple union or by guessing the Verifier slot name.

**Consequence:** TS cannot perform the approved ordinary single-slot workflow in parity with Rust/Python; forced runtime values are enumerated as slots rather than passed intact.

**Selected correction:** preserve existing named-map calls. Accept bare non-record values (JSON scalars/arrays, File, ArtifactRef) as positional. For JSON objects that overlap the named-map shape, add the same explicit third-argument `{ positional: true }` option to verify and submit; that mode treats the complete second argument as the single value. A record without this option remains the current named map. Carry that call distinction through the native boundary to existing Inputs::One/Named, with no hydrated-slot lookup or slot-name inference in JavaScript. This is a small language-boundary disambiguation of already-approved behaviors, using the package's existing optional-options convention; it changes no durable/public wire semantics. It requires no JSON wrapper or new authoring class. Regenerate native declarations from Rust, and update both public methods/docs/type and boundary tests consistently. Do not add separate verify/submit methods or teach the shared client a second projection.

**Focused closure:** TS typing and runtime tests cover bare scalar, array, File and ArtifactRef; positional object JSON including a key equal to a slot name; unchanged named multi-slot maps; and shared rejection of positional evidence against multi-slot contracts. Author the matching real-server positional case while keeping execution deferred. Run focused TS wrapper/native tests, `ts:typecheck`, `ts:napi:check`, and affected format/lint checks; no server implementation is part of this correction.

### FIND-004-4 — extraction journeys depend on uncommitted sample files

**Sources/status/classification:** BEH-001, STD-003; **CONFIRMED / MISSING**.

**Obligation:** REQ-004/016, TASK-004 Scenario4 and required extract row; reproducible complete fixtures under AGENTS §§10-12.

**Location:** Rust `tests/integration/verifier_building_blocks.rs:616-670`, Python `tests/integration/test_verifier_building_blocks.py:290-298`, TS `tests/integration/verifier-building-blocks.test.ts:287-353`; paths `fixtures/cards/verifier_building_blocks/evidence/sample.{pdf,pptx,docx,xlsx,csv,json,txt,md,html}`.

**Producer → consumer:** candidate `git ls-tree -r --name-only 07adb7907 fixtures/cards/verifier_building_blocks` lists only five YAML Cards. The local sample directory is ignored by `.gitignore:55:**/evidence/`. Fixture path helpers merely resolve paths; PolicyDesk, Python desk/conftest, TS server setup and mise lanes do not generate sample files. `fixtures/README.md:26` calls them shared fixture content. File::path deliberately performs no IO, but File::pin → shared sha256_file opens them before upload. Thus local type/build checks pass while a clean candidate's first PDF call fails locally.

**Consequence:** none of the nine-format SDK journeys can run from the committed subject; ignored local bytes conceal the missing deliverable.

**Selected correction:** retain these deterministic sample bytes once in a normally tracked fixture location and update the three path consumers and fixture documentation together. Preserve their exact expected content. Use ordinary staging in a nonignored fixture path; do not force-stage, weaken the global evidence-output ignore, or introduce a generator/harness/check for nine static files. Their placement is reversible fixture organization, not a new contract.

**Focused closure:** inspect the resulting candidate tree/clean export to prove all nine exact consumer paths resolve to tracked files and expected bytes. Compile/typecheck consumers if paths change. The real extraction journeys execute only at final merged review; do not label them passing from a file-existence inspection.

### FIND-004-5 — the mandatory authored action/check matrix is incomplete

**Sources/status/classification:** BEH-003, I-004, SEC-001, FUP-001; **REVISED / MISSING**.

**Obligation:** TASK-004 Scenarios3/4 and README required action/check table at 119-139 and authority allocation at 208-220; REQ-004-010/014 and AC-003/006/007. These require authoring now, not running all suites before Task005.

**Location:** `fixtures/cards/verifier_building_blocks/policy-review.yaml:12-82`, `refund-reader.yaml:10-22`, its Rust/Python/TS journey files, and scheduled drift fixtures/stories. The same missing authored scenarios affect all three first-class SDKs.

**Producer → consumer:** the shared graph has extract→Agent→code actions, plus assert/name-only-trace/Agent/code checks. The provider responders return a valid reader schema and valid cited/failed/uncited judge output. Existing negative tests deliberately raise the code check or deny host access in the code action, and omit/malformed/oversize a PDF. None exercises the remaining required producer states listed below. Code's valid failed branch exists in the fixture but no journey supplies a non-policy, non-raising answer to assert that branch. The trace producer records one span name; no trace-check graph consumes ordering or attributes. The hosted Agent declares only fs.read; an independent Verifier code action does not call the Agent code.run tool. Direct Bifrost/delegated-client journeys do not traverse accepted verification authority → hosted Agent → tool → domain capability.

**Full retained authoring remainder:**

| Required row | Missing authored behavior to add using the existing story/owners |
| --- | --- |
| Agent action | Provider/invocation failure, declared output-schema refusal, and deadline exhaustion, with the approved failed-action/dependency and requirement outcomes. A successful tool turn does not prove these paths. |
| Code action | Raised exception and deadline exhaustion, with failed step and nonpassing dependent requirements. Existing forbidden-host-access action coverage stays. |
| Trace check | Stored multi-span ordering and attribute checks, including a failed expectation. Existing span-name pass/fail and missing trace coverage stays; trace timeline ordering by itself is not a Verifier trace check. |
| Agent check | Malformed/invalid closed Verdict output and an invalid original-evidence citation cannot yield passed. Existing valid passed/failed and empty-citations cases stay. |
| Code check | Valid failed cited Verdict, malformed/invalid Verdict or invalid citation, forbidden access and deadline exhaustion with the approved outcomes. Existing valid pass and raised-exception case stay. |
| Hosted tools | Real hosted Agent calls to declared Card/Bifrost and code tools; allowed path; missing invoke versus missing underlying domain/resource grant; an exact tool grant failing to cover another tool; narrowed delegated subject authority/attribution and denied access exposing no protected data. |
| Drift | Ready/fitted numeric SPC scoring through an actual scheduled binding, with requirement-specific fitted state, retained result and SPC statistical detail. Existing PSI pass/fail/insufficient and unready-SPC cases stay. |

These are the explicit remaining row behaviors, not a demand for every Cartesian combination of failures and methods. A journey can satisfy several rows. Inspect exact outcomes against approved engine semantics instead of merely asserting absence of an exception or weakening a failure to pass.

**Rejected portion / follow-up resolution:** Custom is already authored: `observed-drift.yaml` defines the Custom target; `observed-service.yaml` binds it on a schedule; Rust `observe_a_run.rs:145-193`, Python `test_observe_a_run.py:54-68`, and TS `observe-a-run.test.ts:50-65` submit latency 30, make the binding due, reconnect/read the failed result and inspect its score. Its scalar target does not prove fitted state. `tier-drift.yaml` deliberately uses text-valued tier with SPC, and `verify_in_real_time.rs:759-766` explicitly tests an unready baseline; this cannot stand in for ready numeric SPC. Follow-up's Custom correction is independently supported. No additional Custom scenario is required.

**Consequence:** even after Task005 composes the implementation, the supplied suites have no assertions for required failure/permission/method paths and can pass while those public boundaries are broken. This does not assert an observed server bypass or require Task004 to implement server enforcement.

**Selected correction:** extend the existing shared fixture/story, deterministic provider responders, scoped/delegated client helpers and real worker/fitter/scheduler controls across the three languages. Keep the actual SDK→server→SDK paths, local provider determinism, original-subject authority, and existing scheduler/fitter/Operator owners. No second harness, grant evaluator, scorer, seeded fitted state or copied runtime is needed. Extend the existing numeric scheduled story for ready SPC; keep Custom intact. Update the fixture/evidence coverage mapping to the cases actually authored and record real exact selectors. This closes one incomplete proof authoring boundary, rather than fixing alleged runtime defects independently at each consumer.

**Focused closure:** author all retained cases, compile/typecheck their sources and confirm existing lane discovery/fixture references. Run focused local boundary/responder tests only where new local executable logic requires them. Exact names/commands are recorded after tests exist. Do not run or prescribe full journey suites, `gate`, or live external tests in task remediation. Their first integrated execution remains the approved merged-change review after Task005.

### FIND-004-6 — readback projects result_id as execution_id

**Source/status/classification:** STD-001; **REVISED / INCORRECT**.

**Obligation:** r7 REQ-012 identity distinction and REQ-015 named published `execution_id` correlation field; AGENTS public contract parity.

**Locations:** Rust integration `support/mod.rs:291-320`, Python integration `support.py:165-194`, TS `tests/support/server.ts:96-101`; execution-attribution readers Rust `verify_in_real_time.rs:198-214`, Python `test_verify_in_real_time.py:61-82`, and TS `verify-in-real-time.test.ts:45-65` (filter at 52).

**Producer → consumer:** each complete scheduled helper selects result_id, names/deserializes it as the execution identity, and passes it to Execution.connect and the executions/{id} route. All scheduled observe-a-run/real-time/Operator stories consume these helpers. All three complete attribution readers filter `r.result_id` with a supplied execution ID. Current ResultPayload stores VerificationResultId (`server/verification/results.rs:108`); the queued runner independently mints it (`runner.rs:740`). Direct stage_result deliberately reuses its execution UUID (`components/verification/service.rs:529-550`), explaining why direct-only proof can conceal the assumption. Task005 is cutting these server identities over; no approved contract grants the tests a result/execution alias, whereas r7 explicitly names the field to read.

**Consequence:** scheduled reconnects can select an unknown/wrong execution or constrain Task005 to preserve an unapproved alias; attribution reads can silently miss the intended row. This is a Task004 consumer defect against the agreed future seam, not a demand for early Task005 server implementation.

**Selected correction:** select/deserialise `execution_id` in the three shared execution-readback helpers, and filter by `r.execution_id` in the three execution-attribution readers. Rename local row fields/documentation to match their actual meaning. Preserve result_id where it is genuinely used for result-detail joins (`query_bifrost` siblings), plus existing tenant/table authorization, flush, scheduler/fitter, due-binding and delivery behavior. Fix the projection at shared helper owners rather than each calling story; do not change server identity generation to satisfy a test assumption.

**Focused closure:** source/SQL projection inspection must show the approved field from selection through reconnect/filter in every affected runtime, and preserve legitimate result_id joins. Use a small existing helper seam with distinct result/execution values if available; do not create a harness solely for this readback. Compile/typecheck changed test consumers; the scheduled journeys execute at final merged review as already assigned.

### FIND-004-7 — external background policy-check lacks its deterministic requirement

**Sources/status/classification:** I-005, FUP-002; **CONFIRMED / INCORRECT**.

**Obligation:** REQ-016 and TASK-004 Scenario4 same policy-check requirements and result meanings.

**Location:** frozen external `service/policy-check.yaml:21-30`, consumed by `src/server.ts:37-40`, `scripts/policy.ts:16-22`, and codified by `test/ask.test.ts:46-50`; canonical `examples/support-desk/service/policy-check.yaml:23-37`.

**Producer → consumer:** canonical policy-check has no-refund-promise plus follows-policy in the same execution; canonical support-desk journeys explicitly prove the refund assertion fails even when the model judge passes. External support-api binds separate reply-guard and policy-check Verifiers. The assertion exists only in reply-guard. The complete server ask body sends the original draft to policy-check even if immediate delivery was blocked; no guard outcome is composed into that execution. Its reconnect script prints only that actual result and outcomes maps only its requirements. The external test expects only follows-policy, so it positively codifies the divergent breakdown. Independently reading these paths confirms the follow-up; behavior's general same-story PASS does not close this requirement.

**Consequence:** a guaranteed-refund draft with a passing model judge can produce a passed external background policy-check with one requirement, whereas canonical policy-check must fail with both. Immediate delivery remains guarded; this finding is about the resumed execution's meaning, not a claim that an unsafe draft reaches the customer.

**Selected correction:** restore canonical no-refund-promise alongside follows-policy in external policy-check and update expected background outcomes/presentation. Reuse the canonical assertion and existing SDK submit/reconnect flow. Preserve the separate reply-guard, disclaimer check, delivery gate and recording. No client-side result merge, extra Verifier invocation, or application redesign is needed. Identify/freeze the resulting external revision alongside the main remediation candidate so closure reviews the actual changed source.

**Focused closure:** deterministic focused proof expects both requirement IDs; a guaranteed-refund draft fails the background assertion even with a valid passing judge, while a clean answer has both passed. Reuse existing local adaptation/presentation tests/provider setup and author the corresponding real-server external assertion. External typecheck and small local tests may run now; its full installed-SDK/server test remains deferred as approved.

### FIND-004-8 — Python's fixed response records are typed as arbitrary dictionaries

**Source/status/classification:** MNT-001; **CONFIRMED / MISSING**.

**Obligation:** REQ-011 typed SDK results; REQ-015 attributable fixed breakdown; TASK-004 Scenario3; precise public signatures under `python-api-and-stubs.md`.

**Location:** `sdks/wyrd-sdk-python/python/wyrd/verify/__init__.pyi:89,94,99,134`; native producers `src/verify.rs:139-141,211-241`.

**Producer → consumer:** shared status returns VerificationExecutionState. The complete PyExecution.status body serializes it to an ordinary Python dictionary; result getters similarly serialize RequirementResult, StepResult and InputDeclaration records from VerificationOutcome. These fixed schemas in `wyrd-spec/src/verification/execution.rs` carry named IDs/statuses/outcomes/citations/provenance/checks, with intentionally open JSON only at specified leaves. The public authored stubs erase every record to dict[str, Any]. Python journey helper return types propagate that erasure. Equivalent TS generated records and Rust types retain the schema. There is no native class at the serialized nested `status()['result']` position, so annotating that as the native wrapper would also be wrong.

**Consequence:** the advertised typed Python API cannot catch misspelled status/requirement/citation fields or invalid lifecycle/outcome values; fixed contract changes cannot expose broken Python consumers through typing.

**Selected correction:** keep the dictionary runtime projection and Rust serializer. Author precise TypedDict/discriminated-union wire views for these fixed records once, expose/reexport them through the existing public package, and use them in status/requirements/steps/evidence return annotations. Model the nested status result as its serialized dictionary, separate from the native VerificationResult class. Preserve field names, optional-versus-absent behavior, literal outcomes and true JsonValue leaves; reuse existing shared JSON/CardRef wire aliases where their representation matches. Existing Python operators TypedDict projections demonstrate the mechanism. Do not introduce Pydantic/runtime response parsing, a second server schema owner or a native wrapper per nested record. Task005 remains responsible for future dispatch wire additions; this finding does not require inventing absent server fields.

**Focused closure:** Python typing fixtures accept the actual status/result/requirement/citation shape and reject unknown fixed keys/types, with arbitrary JSON leaves still usable. Existing public result/export/runtime tests remain green; run `py:lints` and `py:typecheck` and the focused public-export/result tests. No real server or new runtime harness is needed for a declaration correction.

### FIND-004-9 — two new wrapper fields omit mandatory rustdoc

**Sources/status/classification:** STD-002, MNT-002; **CONFIRMED / VIOLATION**.

**Obligation:** AGENTS §16 and agent rules explicitly require substantive rustdoc for every new/changed field regardless of visibility; the review skill preserves this hard gate.

**Location:** `sdks/wyrd-sdk-python/src/verify.rs:98` and `:162`, PyExecution's and PyVerificationResult's tuple fields.

**Evidence/consequence:** the complete declarations have struct-level docs but no field-level docs for the stored shared execution or result. Adjacent PyFile.inner and PyArtifactRef.inner already document their owned values. This misses an explicit changed-item gate even though the compiler accepts the declarations. No behavioral error is inferred from it.

**Selected correction/proof:** document both retained ownership fields in place, or the replacement fields necessarily touched by FIND-004-2, describing their workflow/ownership invariant. Keep current behavior and avoid a shape-only refactor. Inspect all immediately changed items and format the touched boundary. No behavioral test for a comment is needed. Batch this with the same implementation remediation; never create an additional style-only task/round.

## Constraints and preserved behavior

- No new dependency, transport, credential resolver, authorization policy, queue, result store, fixture generator, permanent checker or background polling runtime. Reuse the owners named above.
- Preserve existing named TS calls, Rust/Python positional behavior, authenticated client retention, canonical errors, true JSON leaves, standalone status semantics, retry keys/body identity and server work lifetime.
- Preserve genuine result_id joins, the existing Custom and PSI journeys, real fitter/scheduler/Operator ownership, and the external immediate guard/disclaimer/delivery/recording behavior.
- Author genuine SDK/server paths now; do not substitute mocked executors or seed fitted state. Full integration/journey and external installed-SDK/server execution remain deferred to the original merged-change review after TASK-005.
- TASK-005 retains live MCP cutover, dispatch/binding status fields, Operator identity/templates, SQL producer changes and semantic lineage. Do not implement those seams here or guess multi-check drift-count meanings.
- No authority edit, new material decision, compatibility alias, unrelated cleanup, broad stylistic pass, server implementation or example redesign. Stop for a real spec conflict; none is required by the selected corrections.

## Acceptance mapped to findings

| Finding | Required closure |
| --- | --- |
| FIND-004-1 | Caller deadline wins over delayed status/auth/retry/body IO and no post-expiry poll begins; success/visibility/terminal error semantics retained. |
| FIND-004-2 | Python lazy ID/client construction, eager connect, connected submission retention, first-use/retry/reuse and offline ID behavior match the existing connection owner. |
| FIND-004-3 | Both TS methods retain named maps and preserve positional scalars/arrays/files/artifacts and explicitly disambiguated object JSON through Inputs::One; shared slot-count/type refusal remains authoritative. |
| FIND-004-4 | All nine deterministic samples are normally tracked and every language path resolves from a clean candidate export. |
| FIND-004-5 | Every retained missing row above has concrete real-server journey assertions in each language, compiled/typechecked and discoverable in existing lanes; no new Custom requirement. |
| FIND-004-6 | Execution correlation uses execution_id through all three helper/attribution paths; actual result identity joins remain result_id. |
| FIND-004-7 | External background policy result includes both canonical requirements and deterministically fails a refund promise independent of the judge; immediate guard preserved; external source candidate identified. |
| FIND-004-8 | Python fixed dictionary records have precise authored wire views/export parity, with real JSON leaves and correct serialized nested status result. |
| FIND-004-9 | Both ownership fields, including any necessarily changed lazy-state fields, meet the mandatory rustdoc rule in the same remediation. |

## Focused proof and execution evidence

Use one Behavior / RED / GREEN / REFACTOR scenario at a time for executable changes, recording actual failure cause, exact selector and passing result. For newly authored cases, record their real names after they exist and run exact focused commands for local tests; do not invent a named test or accept a zero-selection run. The authored real-server journeys are compiled/typechecked/discovered now and executed only at the already-approved final integration stage. Static fixture, SQL projection and rustdoc corrections need credible source/candidate-tree proof rather than manufactured behavioral tests.

Existing regression owner tests have these exact commands:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=observe::tests::enqueue_replays_its_key_and_wait_returns_only_a_real_result)'
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=observe::tests::wait_raises_terminal_errors_and_times_out_without_a_result)'
```

For local Python/TS runtime boundary changes, rebuild the touched bindings through existing setup/build owners, then use the current focused test-file commands (add exact selectors for new cases to the evidence):

```bash
mise run py:setup
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py tests/unit/test_public_exports.py
mise run ts:build
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/observe.test.ts
```

Compile the authored Rust integration source without running the server journeys:

```bash
mise exec -- cargo check --locked -p wyrd-ai --test integration
```

Use existing lane/target registration plus runner collection/listing to prove discovery without executing the full journeys; include the owning repository environment wrapper if a selected collection step needs it. Do not widen a skip/filter or add a new discovery checker.

Run only the affected final scoped checks, sequentially for Cargo-backed work:

```bash
mise exec -- cargo clippy --locked -p wyrd-client -p wyrd-ai -p wyrd-sdk-python -p wyrd-sdk-ts --all-features --lib -- -D warnings
mise run fmt
mise run py:format:check
mise run py:lints
mise run py:typecheck
mise run ts:format:check
mise run ts:lints
mise run ts:typecheck
mise run ts:napi:check
```

`ts:napi:check` rebuilds from Rust declaration sources; never hand-edit its generated artifacts. If correcting public exports/types touches a wire/schema generator, run `mise run codegen:check`; otherwise do not add unrelated generated churn. Any affected docs-site or examples edits use their existing checks only if actually changed. No repository gate, full user-journey suite, live external test target or unrelated aggregate belongs to this remediation.

For the external demo, use its existing `npm run typecheck` after refreshing the real built SDK through its documented vendor setup, plus narrowly selected local tests that need no live server. Keep the real server assertion authored for final integration, and preserve the reviewed/resulting external candidate identity and setup instructions. Do not run its current whole live-server test target here.

Completion evidence must map every finding to the actual diff, focused proof and verification limits, including the tracked nine-format assets, remaining authored journey table and exact external source revision/archive. A later `$wyrd-task-review` reviews the original base through the complete remediation candidate, this verdict, and all nine stable finding IDs. Do not treat this task's implementation as review PASS.

## Implementation evidence

Status: `IMPLEMENTED`. Implementation commits on `change/verifier-building-blocks-task-004`: `a74acb518`, `76e7253a5`, `49eb3e262`, `240731ee0`, `eb9e135f0`, `028b2f987`, `b836ee002`, `77b78c760`, `ec1538d03`. Authored journeys are compiled, typechecked and discovered. They were not executed; they run at the merged-change review after TASK-005.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-004-1 | `a74acb518`: `Execution::wait` bounds every status read with `timeout_at(deadline)`, starts no read after expiry, and never sleeps past the deadline | RED, then GREEN: `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=observe::tests::wait_deadline_bounds_slow_reads_and_starts_no_read_after_expiry) \| test(=observe::tests::enqueue_replays_its_key_and_wait_returns_only_a_real_result) \| test(=observe::tests::wait_raises_terminal_errors_and_times_out_without_a_result)'` gives 3 passed. The new test failed at 3.2 s against the old loop. | PASS |
| FIND-004-2 | `76e7253a5`: `PyExecution` holds `LazyConnection<Execution>`; `__new__(id, client=None)` does no IO; `connect` connects eagerly; submissions retain their connected handle through `LazyConnection::ready` | `test_execution_constructs_offline_and_connects_on_first_read` and `test_execution_connect_exchanges_now` in `tests/unit/state/test_observe_surface.py`; `mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py tests/unit/test_public_exports.py` gives 96 passed | PASS |
| FIND-004-3 | `49eb3e262`: native `slot` is optional, so a lone unslotted value becomes `Inputs::One`; TS `verify`/`submit` take `VerifierInput \| VerifierInputs` and `{ positional?: boolean }`; declarations regenerated. Real-server cases in `77b78c760`. | `vitest run tests/unit/observe.test.ts` gives 33 passed; `ts:typecheck` and `ts:napi:check` ok; the three positional real-server titles are listed by vitest | PASS |
| FIND-004-4 | `240731ee0`: the nine samples moved from the ignored `evidence/` to the tracked `samples/`; all language paths and the README updated. `ec1538d03` marks `*.pdf binary` so the xref bytes stay exact. | `git archive HEAD fixtures/cards/verifier_building_blocks/samples` lists all nine files; `git check-ignore` reports nothing; the pdf sha256 is unchanged | PASS |
| FIND-004-5 | `028b2f987` (Rust and fixtures), `b836ee002` (Python), `77b78c760` (TS and matrix). See the authored cases below. | `cargo clippy --locked -p wyrd-ai --test integration --all-features -- -D warnings` clean; `cargo nextest list -p wyrd-ai --test integration --run-ignored=all` lists all 8 new tests; pytest `--collect-only -m 'integration and not identity and not local_dev'` collects 24; `vitest list` lists all 11 new TS titles; `py:typecheck` and `ts:typecheck` ok | PASS (authored; execution deferred) |
| FIND-004-6 | `eb9e135f0`: the scheduled readback selects `execution_id`, and the attribution readers filter `r.execution_id` in all three languages; `result_id` joins kept | `cargo check --locked -p wyrd-ai --test integration`, `py:typecheck` and `ts:typecheck` ok | PASS |
| FIND-004-7 | Superseded by approved spec r8 (`0e9a2d4b6`): the standalone example repositories, including `wyrd-example-ts`, moved to TASK-005, and TASK-004's partial external update is discarded. No external source was changed. | Spec r8 revision history | SUPERSEDED |
| FIND-004-8 | `76e7253a5`: stub-only TypedDict wire views in `python/wyrd/verify/__init__.pyi`, used by `status`/`requirements`/`steps`/`evidence`; the nested status result is `VerificationResultWire` | `tests/typecheck/verify_contracts.py` accepts the real shape and rejects unknown keys and values; `py:lints` and `py:typecheck` ok | PASS |
| FIND-004-9 | `76e7253a5`: rustdoc on `PyExecution.id`, `PyExecution.inner` and the `PyVerificationResult` field | `cargo clippy --locked -p wyrd-client -p wyrd-ai -p wyrd-sdk-python -p wyrd-sdk-ts --all-features --lib -- -D warnings` clean | PASS |

Authored FIND-004-5 cases. The Rust name is canonical; Python adds a `test_` prefix and TS uses the spaced title.

| Row | Fixture | Test |
|---|---|---|
| Agent action: provider failure, schema refusal, deadline | `policy-review.yaml` (`refund-rules`, `timeout_seconds: 5`, reader sees the answer) | `failed_reader_leaves_its_dependents_inconclusive` |
| Code action: raise, deadline | `policy-review.yaml` (`answer-facts`, `timeout_seconds: 5`) | `failed_code_step_leaves_its_assertion_inconclusive` |
| Trace check: order and attributes, with failed expectations | `policy-review.yaml` (`answer-drafted-after-lookup`, `lookup-read-refunds`) | `trace_check_judges_span_order_and_attributes` |
| Agent check: malformed Verdict, invalid citation | `policy-judge.yaml` responders | `invalid_judge_verdicts_are_inconclusive` |
| Code check: failed cited; malformed, foreign cite, forbidden, deadline | `policy-review.yaml` (`names-the-policy`) | `code_check_fails_with_citations_and_is_inconclusive_otherwise` |
| Hosted tools: allowed, missing invoke, missing domain, exact tool | `tool-review.yaml`, `investigator.yaml` | `hosted_tools_need_the_tool_and_domain_grants` |
| Hosted tools: narrowed delegate and attribution | `tool-review.yaml`, `investigator.yaml` | `delegated_tools_keep_the_narrower_authority` |
| Drift: ready numeric SPC on a scheduled binding | `latency-control.yaml`, `latency-steady.yaml` | `ready_spc_baseline_charts_a_scheduled_window` |

Material limits:

- `wyrd plan` cannot validate r7 Verifier Cards on this branch until TASK-005 switches the Card loader to `blocks::VerifierSpec`. Meanwhile `crates/wyrd-spec/tests/verifier_fixtures.rs` decodes every fixture Verifier into `blocks::VerifierSpec`, runs `validate_with` with path-referenced Agents and Prompts resolved from disk, and runs `validate_schedule` on every schedule binding: `mise exec -- cargo nextest run --locked -p wyrd-spec --test verifier_fixtures -E 'test(=fixture_verifiers_satisfy_the_staged_contract)'` passes. Mutating a Prompt placeholder or a binding feature makes it fail.
- Tool arguments reuse the existing contracts: `GetCardByRefArgs{card_ref}` for `cards.get_by_ref` and `{sql, max_rows}` for `bifrost.query`. Only `code.run` has no argument contract yet; `{"code": ...}` is provisional and TASK-005 defines it.
- SPC evidence assertions follow the `vala-drift` `FeatureEvidence` serde shape (`{"Spc": {...}}`).
- No Custom scenario was added; the PSI tests are unchanged in behavior.
- Incidental fixes:
  - Python `test_verifier_building_blocks.py` failed to collect at HEAD (a `NameError` on type-only imports); fixed with `from __future__ import annotations`.
  - The TS oversized-policy test compared against a file-scoped receiver; it now compares only requests made after the call.
- Other checks: `cargo fmt --all --check`, `py:format:check`, `ts:format:check`, `ts:lints` and `git diff --check` are all ok.
