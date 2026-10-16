# TASK-004 independent finding validation

Result: **FIX_REQUIRED**. Nine deduplicated bounded findings remain. No retained correction needs a specification revision.

## Subject, inputs, and method

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-004`.
Immutable cumulative subject: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0..07adb7907951f343544e550f6cabc5b7aa3aadea`.
Packet-only HEAD remains `aa32c821ef74886526b7367bfd77d51c87e0d521`; `git status --short` contains only this review directory. No implementation source was changed.

Read all seven discovery reports, `claims-comparison.md`, and the fresh `followup-review.md`. Used the complete cumulative diff and inventory, original task, approved r7 specification, coordination README and Task005 handoff, and frozen external diff/source. The external input is `/tmp/wyrd-task004-external-07adb7907`, preserved by this directory's archive and SHA-256 manifest, against external base `000ed1f2f587a6629a471c51aadff5e05f0c820b`; it is not an external implementation commit.

Authorities applied: AGENTS.md, agent rules, reference router, spec-driven development, maintainer style, Python API/stubs and TypeScript guidance, applicable Wyrd Client/Verifier and Bifrost contracts, and the task's explicit distinction between authored proof now and merged execution after Task005. CodeGraph was consulted before code navigation. Validation reread the affected owner bodies, boundary converters, shared transport retry/auth path, existing lazy connection owner, wire result types, test helpers and their sibling callers, shared Cards, authored journey families, and external result producers/consumers. Matching discovery claims were hypotheses, not proof.

No Cargo, codegen, build, external-app runtime, or full journey command was run by this validator. Root's retained `client-check.log` independently establishes 28 shared-client tests passed; it does not exercise the newly identified deadline case. Other green checks are implementation-reported. The final journey execution deferral is valid and is not a finding.

## Disposition of every discovery proposal

| Source claim | Disposition | Independent resolution |
| --- | --- | --- |
| BEH-001 | CONFIRMED | FIND-004-4: consumer paths exist but all nine sample assets are absent from the candidate tree. |
| STD-003 | CONFIRMED, duplicate | Same delivery cause and correction as FIND-004-4; no separate fixture finding. |
| BEH-002 | CONFIRMED | FIND-004-1: deadline is tested only after unbounded status IO and after a result can return. |
| I-001 | CONFIRMED, duplicate | FIND-004-1. |
| SYS-001 | CONFIRMED, duplicate | FIND-004-1; request-local cancellation is sufficient, with no server lifecycle change. |
| CONC-01 | CONFIRMED, duplicate | FIND-004-1. |
| BEH-004 | CONFIRMED | FIND-004-2: Python exposes only eager connect, contrary to explicit lazy construction authority. |
| I-003 | CONFIRMED, duplicate | FIND-004-2; reuse existing LazyConnection rather than introduce another connection workflow. |
| I-002 | REVISED | FIND-004-3: preserve unambiguous named calls and select an explicit object-positional disambiguation; a bare JSON-value/map union alone is insufficient. |
| BEH-003 | REVISED | FIND-004-5: required authoring gaps remain; the full row-level remainder is specified below, rather than only the timeout shorthand. |
| I-004 | REVISED in part, REJECTED in part | FIND-004-5 retains independently located negative/tool/trace/ready-SPC gaps. The claim that Custom scheduled journeys are absent is false and rejected. |
| SEC-001 | CONFIRMED, duplicate | Hosted tool authority authorship is one part of FIND-004-5, not a demonstrated SDK bypass and not another authorization implementation. |
| FUP-001 | CONFIRMED, duplicate | Ready numeric SPC remains in FIND-004-5; preserve authored Custom and PSI coverage. |
| I-005 | CONFIRMED | FIND-004-7: the deterministic assertion belongs to another external Verifier and cannot affect the resumed policy-check result. |
| FUP-002 | CONFIRMED, duplicate | FIND-004-7. Behavior review's broad external REQ-016 PASS must be narrowed accordingly. |
| STD-001 | REVISED | FIND-004-6 confirms the shared cause and includes the TypeScript attribution reader sibling, in addition to the named Rust/Python readers. Actual result-identity joins are excluded. |
| MNT-001 | CONFIRMED | FIND-004-8: fixed metadata is untyped; arbitrary action output is a different concern and remains JSON. |
| STD-002 | CONFIRMED | FIND-004-9: mandatory field-level rustdoc is absent. |
| MNT-002 | CONFIRMED, duplicate | FIND-004-9, batched with the implementation remediation; no style-only round. |

The discovery nonblocking import-style, stale prose link, per-format example wording, and example-local preflight exception notes are not promoted into findings or remediation requirements. The supplied Python `LookupError` observation does not establish a missing SDK error conversion: that example's local deployment preflight is distinct from a returned server refusal. No rejected claim is retained as optional advice.

## Final validated ledger

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

## Scope and correction sufficiency

The ladder selects existing owners throughout: Execution's installed Tokio deadline; existing Python LazyConnection; shared Inputs::One/Named; existing tracked fixture practice; current SDK stories/provider/runtime/fitter/security helpers; published SQL execution correlation; canonical policy-check; existing Python TypedDict/JSON projection; field docs. Deleting any retained requirement would leave an approved workflow or mandatory proof incomplete. None needs a new dependency, transport, durable receipt, server queue, grant evaluator, client authorization policy, result store, broad permanent check, or generalized framework. Slot-count/type and server authorization validation remain where they already belong.

The r7 server-owned MCP tool cutover, dispatch projection/binding identity, Operator template adaptation, published SQL producer fields and semantic lineage stay Task005 handoffs. They do not excuse incorrect Task004 consumers, and their absent live implementation is not charged to Task004. No multi-check drift-count policy is selected. Existing Custom coverage stays closed on source evidence; no other prior TASK-004 findings exist. The external uncommitted archive remains a provenance and runtime-proof limit rather than a fabricated commit identity.

Remediation should be one self-contained TASK-004-R1 packet covering these bounded findings. Its proof is exact focused owner/runtime tests, affected typing/declaration/format/lint checks and authored-case discovery/source closure. Do not prescribe the repository gate or full journey suites. Final all-task integration, real extraction/hosted tools/ready SPC/runtime permissions and the external installed-SDK workflow still need their approved merged execution.
