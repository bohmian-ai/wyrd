# TASK-004 independent invariant review

Overall: **FAIL**.

Subject: base `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0` through candidate
`07adb7907951f343544e550f6cabc5b7aa3aadea`, inclusive of the first implementation
commit. Packet/authority commit: `aa32c821ef74886526b7367bfd77d51c87e0d521`.
Authority: approved specification r7, original TASK-004, tasks/README.md and
the r7 TASK-005 handoff. External subject: frozen
`/tmp/wyrd-task004-external-07adb7907`, with the provenance recorded in
`subject.md`. No prior TASK-004 findings exist.

This is a source audit. I did not run Cargo, code generation, builds, or
integrated journeys. The reported checks are evidence claims; no raw passing
logs were supplied. The explicit task instruction defers real-server execution
until TASK-005, but does not defer writing the required journey assertions.
I did not read other discovery reports or alter implementation.

## Authority and navigation

Read the review skill, AGENTS.md, agent rules, reference router,
spec-driven-development, maintainer-style, testing-workflows and errors
references; relevant doctrine/design/Bifrost and approved spec/task sections.
Used CodeGraph before code navigation. Traced the shared Observe/Target and
Execution paths through their state, HTTP, storage, Python, TypeScript and Rust
consumers; inspected the authored journey families, shared Cards, canonical
support-desk examples, external snapshot and documentation contracts.

The relevant ownership comparisons are:

| Changed mechanism | Existing alternative and missing behavior | Assessment |
| --- | --- | --- |
| Target/RunEvidence | Existing Run/Observe owns subject and application Run; storage owns actual byte transfer | Target is invocation-scoped projection, not a new retained workflow or durable owner. It reuses Observe and storage. |
| File hashing and upload | `cards/saga/hash_artifacts.rs` previously hashed files; `WyrdStorageClient::upload` dispatches transfer | Hashing was extracted into `storage::upload::sha256_file` and reused by both; evidence adapter invokes the existing upload owner without Artifact completion. No duplicate PUT transport. |
| Execution reconnect and polling | Cards/client connection conventions, existing HTTP auth/retry owner | One shared Rust polling implementation is appropriate, but its deadline does not encompass that owner's IO (I-001). |
| Python handle construction | `client::LazyConnection<T>`, PyCards and Principals lazy constructors plus eager connect | Existing machinery covers the required lazy handle; Execution omitted it (I-003). |
| Foreign input conversion | Shared `Inputs::{One, Named}` and Target slot projection | Python preserves both; TypeScript always constructs Named (I-002). |
| Scheduled samples | Existing drift observation conversion and Bifrost lifecycle | Submit uses that owner; no new queue, client fitter, or durable scheduling path. |

The temporary serialized conversion in `WyrdState::bound_verifier` cannot
execute new Verifier Cards until the declared Task005 Card-envelope cutover.
That is an explicit staged boundary in the approved coordination, not a
compatibility workflow or a finding in this task. Likewise dispatch projection,
binding identity, runtime MCP and SQL/lineage cutovers remain TASK-005's scope.

## Acceptance matrix

PASS below means the assigned authored/local obligation is supported by source;
it does not claim deferred server execution passed.

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REQ-001/002: project hydrated named slot types; preserve missing evidence for the server | `verify/mod.rs::Target::project`, `state.rs::bound_verifier` | `values_project_onto_named_slots_by_slot_type` tests JSON, samples, trace and refusal paths | PASS |
| REQ-002: one positional value for a single-slot Verifier in all languages | Rust Inputs::One and Python `observe::inputs`; TS `nativeInputs`/native `inputs` discard the distinction | Rust positional projection covered; TS tests only named slots | FAIL (I-002) |
| REQ-010: lazy File, digest pinning and missing-only upload through existing storage | File::pin, RunEvidence::pin, WyrdStorageClient::upload_evidence | `execute_pins_files_and_uploads_only_missing_digests`, `file_refusals_happen_locally_before_any_request` | PASS |
| REQ-010: ArtifactRef keeps registered identity/path, no new registration lifecycle | `Input::Artifact` produces typed InputDeclaration::Artifact | Pinned execution request assertion includes artifact/path | PASS |
| REQ-011: Run verify returns result; submit stages handle; Window submit uses existing drift path; Window verify refuses | Observe::{verify,submit}, Target::{execute,enqueue} | Target tests and authored three-language Window/Run journeys; public live execution explicitly deferred | PASS |
| REQ-011: immediate verification not replayed after ambiguous execution failure | Execute uses unkeyed POST over existing transport; send_with_retry checks replay_safe | `execute_outlives_the_configured_timeout` verifies one delayed 504 | PASS |
| REQ-012: retained explicit/ambient client and reconnect, Python lazy form | Shared Execution retains WyrdClient; Python only exposes eager connect | Shared and Python explicit-client wait tests exist; lazy Python construction absent | FAIL (I-003) |
| REQ-012: wait returns only actual result, terminal own error or caller timeout | execution.rs::wait | Existing tests cover missing row/completed-no-result/conflict/403; no delayed-request deadline test | FAIL (I-001) |
| REQ-013: stable replay identity over transport retries, no SDK durable queue | Target::enqueue creates key once and existing submit transport reuses it | `enqueue_replays_its_key_and_wait_returns_only_a_real_result` | PASS |
| REQ-014: full fitted Service drift lifecycle, failed-only real Operator delivery | Existing scheduled-drift journeys extended in all languages | Authored fitting, Recorded, identity, statistics, failed/pass/empty windows and quiet-delivery assertions | PASS |
| README required drift matrix: PSI, SPC and Custom through scheduled bindings | Scheduled journeys cover PSI; no SPC/Custom counterpart found | Required authored rows absent | FAIL (I-004) |
| REQ-015: preserve typed result identities, outcome, breakdown and provenance across clients | Rust reexports, PyVerificationResult properties, TS lifecycleValue wire decoding | Result assertions in owner tests and authored journeys | PASS |
| REQ-007/008: declared built-in tool names and no SDK authorization/runtime implementation | Raw declarative Cards and generated BuiltinTool vocabulary; no foreign runtime durable owner added | fs.read action journey authored | PASS for projection; protected-tool proof fails below |
| Scenario3: hosted Card/Bifrost/code tool journey including denied/delegated access | Building-block Agent declares only fs.read; other changed journeys do not invoke the required hosted tool paths | No required authored hosted protected-tool journey | FAIL (I-004) |
| Scenario4/README: every action/check and required negative edge in every language | policy-review contains extract/agent/code actions and assert/trace/agent/code checks; PSI elsewhere | Happy/failed/uncited/malformed/missing/forbidden paths authored, but required timeout/schema/ordering/attribute and invalid Verdict paths absent | FAIL (I-004) |
| REQ-009/016: canonical support policy original citations, deterministic refund assertion, tool-less judge in three SDKs | Shared examples/support-desk/service Cards and three language entry points | Authored support_desk journeys assert failed refund promise and original policy page citation | PASS |
| REQ-016: external demo same policy-check requirements/result meanings | Frozen external policy-check omits canonical deterministic refund assertion | External test explicitly expects only follows-policy | FAIL (I-005) |
| REQ-016: immediate/background/resume flows, real built external SDK rather than symlink | Canonical check/serve/wait helpers; external server.ts/policy.ts/vendor.sh | Authored server journeys; external package tarball setup inspected, not executed | PASS (authored/setup only) |
| REQ-017: extraction schema/null/order/table/locator/visual semantics documented | reference/extraction-output.md tracks locked shapes and examples | Reported docs:check; static comparison with REQ-004 | PASS for semantics; text/html individual examples noted below |
| REQ-017: runtime settings/defaults/units/startup/capacity/security documented honestly | reference/verification-runtime-limits.md includes all approved settings and explicit pending-parity caution | Reported docs:check; startup parity deferred with server composition | PASS |
| REQ-017: built-in catalog, both grant forms, independent gates and delegated/original authority documented | reference/agent-tools-and-authorization.md and permissions guide | Reported docs:check; execution enforcement owned elsewhere | PASS |
| INV-001/002: shared implementation, foreign-runtime projections only | Shared storage/Observe/Execution owners; thin PyO3 and napi methods; Rust module reexports | Reported client Clippy, runtime units and type checks | PASS |
| INV-003/004/006: client request identity/context and honest results/loss | VerificationRequest exact subject/verifier/run; no trusted tenant/principal input; no synthetic result; docs explain 202 staging and loss | Authored tenant reconnect/permission negatives plus local terminal-error tests | PASS subject to I-001 timeout contract |
| INV-008: replace SDK/docs/examples rather than retain old aliases, preserve unrelated record behavior | Removed observe.eval/drift and old authoring package; record/lifecycle retained; server cutover explicitly staged | Export tests, updated examples and reported codegen/dependency checks | PASS within task scope |
| AC-005/006/007/008: author journeys now; run only after all tasks join | Three-language integration files and mise lane discovery inspected | Execution correctly unclaimed; authoring incomplete as above | FAIL (I-004) |
| AC-009/010: source-driven artifacts and local focused verification | gen_schemas.rs/gen_ts_types.py roots, TS generated files, authored Python typing | Reported 28 client, 628 Python, 70 TS tests and scoped checks, no raw logs independently rerun | PASS as reported local evidence; not proof of integrated acceptance |
| Non-goals: no second transport, durable client engine, new Card kind, application framework, UI redesign or alias | Actual changed owners and external app inspected | Source review | PASS |

## Proposed material findings

### I-001 — caller wait deadline excludes status request and transport retries

- Classification: **INCORRECT**, high consequence to bounded caller workflows.
- Obligation: REQ-012 and architecture/wyrd-design.md require delayed visibility
  only within the caller deadline and a timeout when that deadline expires.
- Location: `crates/shared/wyrd-client/src/verify/execution.rs:111-145`;
  status at `:77-88`; existing transport
  `crates/shared/wyrd-client/src/transport/http.rs:991-1090`.
- Producer-to-consumer evidence: wait computes a deadline, then unconditionally
  awaits `self.status()`; that enters request_json and the shared GET retry
  machinery, whose configured per-attempt timeout and credential work are
  independent of wait's deadline. Only after the await completes and after a
  successful result can already have returned does wait inspect elapsed time.
  A 20-ms wait can await a 5-second configured status attempt, its retries,
  authentication and body decoding, and can return a result received after the
  caller deadline. After a sleep consumes the remaining budget the loop also
  starts another request before checking the deadline. Zero timeout likewise
  initiates IO.
- Siblings: every Rust, Python and TS Execution wait delegates here; canonical
  support-desk wait helpers additionally pass a shrinking shared deadline, so
  their total-bound promise also fails. Do not fix wrappers independently or
  disable ordinary transport retry/auth semantics for status().
- Correction: enforce the one absolute deadline around the entire polling
  operation, including status/auth/retries/body reads and sleeps, in shared
  Execution::wait using the existing Tokio deadline facility. On expiry return
  the same SDK wait error and release the pending local read; do not cancel or
  resubmit the server execution. Do not start new polling after expiry.
- Focused closure: existing wiremock owner tests with a status response slower
  than the caller wait, plus exhaustion between polls and a transient retry,
  must return the SDK timeout within the caller budget and never return a late
  result. Keep direct status and normal successful/terminal-error tests.

### I-002 — TypeScript cannot express approved single-slot positional evidence

- Classification: **MISSING**, public-client parity.
- Obligation: REQ-002 single-slot positional input and INV-002 first-class
  equivalent workflows; Scenario3 matching usable language projections.
- Locations: `sdks/wyrd-sdk-ts/wyrd/src/index.ts:2089-2110,2266-2267,2280-2281`;
  `sdks/wyrd-sdk-ts/native/src/verification.rs:60-96`.
- Evidence: TS only accepts `Readonly<Record<string, VerifierInput>>` and
  serializes Object.entries into slot-tagged entries; native inputs always
  constructs Inputs::named. No boundary branch constructs Inputs::One. A
  bare string is rejected by typing and, if forced at runtime, becomes slots
  `0`, `1`, etc.; a File exposes its properties as slots. Rust and Python both
  pass Inputs::One to the shared Target, which already enforces exactly one
  declared slot and projects the value correctly.
- Consequence: documented ordinary single-slot values/files usable in Rust and
  Python cannot be submitted or verified equivalently from TypeScript.
- Correction: carry the positional-versus-named call distinction across the
  legitimate TS/native boundary into existing shared Inputs::One/Named;
  retain all slot-count/type validation in Target rather than implementing
  hydrated-slot semantics again in JavaScript. Update authored/generated
  signatures from their sources and both verify/submit callers together.
- Focused closure: TS runtime and typing proof for positional JSON, File and
  ArtifactRef, plus preservation of named multi-slot calls and shared refusal
  for positional evidence against a multi-slot Verifier. Author the matching
  real-server single-slot assertion; execution stays deferred.

### I-003 — Python execution handle omits its required lazy construction form

- Classification: **MISSING**, public construction/lifecycle behavior.
- Obligation: REQ-012 explicitly says Python retains its lazy form and existing
  optional-client connection conventions; INV-002.
- Location: `sdks/wyrd-sdk-python/src/verify.rs:96-125` and authored
  `python/wyrd/verify/__init__.pyi::Execution`.
- Evidence: PyExecution can only wrap an already-connected shared Execution.
  It exposes eager static connect, immediately calling block_on and shared
  credential resolution, and has no #[new]. The public Python module directly
  reexports that class. Therefore Execution(id, client) cannot be constructed
  lazily at all. Compare PyCards and Principals, which retain optional client
  through existing `client::LazyConnection<T>`, connect on first server
  operation, and also expose eager connect.
- Consequence: Python callers cannot retain/reconstitute an ID-only execution
  before credentials/server access are available as the approved contract
  permits, and offline construction fails instead of deferring IO.
- Correction: reuse the existing LazyConnection pattern for this handle,
  preserving the local ID, optional explicit client, eager connect, and
  already-connected handle returned by submit. Status/wait must resolve once
  through shared Execution connect logic; no separate Python transport/cache
  or polling policy. Add authored constructor typing and doc parity.
- Focused closure: Python runtime tests construct lazy handles with no usable
  ambient credential and perform no exchange, read ID offline, then prove
  first server operation uses the retained explicit client and connection
  reuse; eager connect still performs/refuses required exchange.

### I-004 — required authored journey coverage is substantially incomplete

- Classification: **MISSING**, required public-boundary proof.
- Obligation: TASK-004 Scenario3/4, its explicit acceptance criteria, README
  required action/check matrix and AC-005–007. These require authoring now;
  only execution is deferred.
- Locations: `sdks/wyrd-sdk-rust/tests/integration/verifier_building_blocks.rs`,
  its Python/TS siblings, `verify_in_real_time` and scheduled-drift siblings;
  shared `fixtures/cards/verifier_building_blocks/policy-review.yaml:12-82`.
- Evidence: compared the complete authored tests across all three families
  and searched the SDK journey tree for sibling coverage. The new policy
  graph has a successful Agent action, successful tool-less judge, an uncited
  pass, raising code and forbidden host read; no Agent invalid-schema/timeout
  or invalid Verdict response, code timeout or malformed Verdict assertion.
  The only hosted tool used by that Agent is fs.read. There is no declared
  Card/Bifrost/code.run hosted Agent journey proving tool-invoke/domain gates
  under denied and delegated authority. SDK Bifrost delegation tests call
  Bifrost directly, so they do not cover this path. Trace check coverage only
  tests one name plus absent trace, not required stored ordering/attribute
  checks. Scheduled fixture/method coverage is PSI; no SPC/Custom scheduled
  counterparts are authored. The reported coverage does not close these rows.
- Consequence: final composition has no preauthored SDK assertions for these
  required failures, authority boundaries and reused methods; lower-tier
  engine tests and leaf calls cannot substitute for their SDK/server path.
- Correction: complete the existing journey/fixture families rather than add
  a second harness. Add the missing cases in each first-class language,
  using existing deterministic upstream responders and actual hosted runtime,
  shared spec fixtures, and existing scoped/delegated clients. Preserve the
  real scheduled binding and fitted baseline story; cover SPC/Custom through
  their actual scheduled paths. Record exact discovery/commands and map every
  required row honestly. Do not change runtime behavior or require broad suite
  execution in this remediation.
- Focused closure: compile/typecheck/discover the touched tests, inspect their
  concrete assertions, and run only small local responder/wrapper tests where
  added. The existing approved final integration step runs these journeys
  once after TASK-005; do not claim them passing now.

### I-005 — external policy-check has a different refund verdict contract

- Classification: **INCORRECT**, matching-example behavior.
- Obligation: REQ-016 and Scenario4 require the same support policy scenario,
  deterministic guaranteed-refund assertion and expected result meanings in
  the matched examples/demo.
- Location: frozen external `service/policy-check.yaml:23-32`,
  `src/server.ts:37-42`, `test/ask.test.ts:52-57`; compare repository canonical
  `examples/support-desk/service/policy-check.yaml:23-34`.
- Evidence: canonical policy-check contains no-refund-promise and
  follows-policy; external policy-check contains only follows-policy, while
  deterministic refund checking is placed in a separate reply-guard. External
  server submits the original draft, so a guaranteed-refund draft can receive
  a passed background policy-check if the tool-less model returns passed.
  Canonical policy-check deterministically fails that same answer independent
  of the judge. The external background test explicitly expects only the one
  follows-policy outcome, cementing that divergence.
- Consequence: an execution ID for the advertised same policy review has
  different requirement breakdown and guaranteed-refund meaning in the demo
  versus the three SDK examples.
- Correction: bring the external policy-check requirements into parity with
  the canonical version, reusing its deterministic assertion and tool-less
  judge semantics; retain any separately needed preexisting immediate guard.
  Update background presentation/tests/setup descriptions consistently. No
  new application flow or verification primitive is necessary.
- Focused closure: deterministic local result-presentation/assertion checks
  for both canonical requirement IDs and authored server journey cases where
  a guaranteed-refund answer fails the background policy-check even when the
  judge passes. External end-to-end execution remains deferred as approved.

## Non-blocking notes and verification limits

- Extraction-output reference combines text/Markdown/HTML in one type and
  provides only a Markdown JSON example; the literal per-format-example
  acceptance would also benefit from text and HTML instances. Their wire
  semantics are already unambiguous, so this is not a separate blocking round.
- The Python/TS support-desk deployment preflight uses ordinary runtime errors
  where Rust uses WYRD_GATEWAY_404_MODEL_UNAVAILABLE. These are example-local
  preflight helpers, not duplicated SDK transport error conversion; I did not
  elevate this noted discrepancy into an additional independently proven
  SDK-contract finding.
- External snapshot remains uncommitted in its own repository. Its frozen
  files are reviewable but no external implementation commit or reproduced
  installed-package/live-provider result was available.
- Full journeys and final served-schema/startup parity were intentionally not
  run. No build or test state was changed by this reviewer.
