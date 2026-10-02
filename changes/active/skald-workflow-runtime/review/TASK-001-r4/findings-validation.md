# TASK-001 structured Ponytail validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `a704a8890ef20efe65fee1e116f7d02288f8ec5c`
- Candidate tree: `bf9ee6b314482a8fe028c31f4121eab6b8ac9e92`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 11, including `REQ-053`
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation task:
  `changes/active/skald-workflow-runtime/review/TASK-001-r3/TASK-001-R2-close-round-three-runtime-gaps.md`

The candidate remained at the stated commit throughout validation. No
`.codegraph/` directory exists, so source and caller tracing used Git, `rg`,
and direct file inspection. This pass read every round-four discovery report
and the focused follow-up, then checked each claim against the cumulative
candidate rather than accepting reviewer agreement as proof.

## Validation result

Three findings remain:

| Stable finding | Discovery sources | Validation | Classification |
|---|---|---|---|
| `FIND-TASK-001-27` | `BEH-R4-001`, dependency portion of `STD-R4-001`, `MNT-R4-001`, dependency portion of `FUP-R4-003` | **REVISED, DEDUPLICATED, PRIOR ID PRESERVED** | `INCORRECT` |
| `FIND-TASK-001-28` | `SYS-R4-001`, `CONC-R4-001`, `FUP-R4-001` | **REVISED, DEDUPLICATED** | `INCORRECT` |
| `FIND-TASK-001-29` | `TEL-R4-001`, `FUP-R4-002` | **CONFIRMED, DEDUPLICATED** | `INCORRECT` |

All three corrections fit the approved Revision 11 behavior and existing
owners. None requires a new product, public API, architecture, security,
compatibility, concurrency-semantics, resource-ownership, or persistent-data
decision. The result is bounded remediation, not `SPEC_REVISION_REQUIRED`.

## Proposal-by-proposal disposition

### Documentation proposals

- `BEH-R4-001`: **CONFIRMED and retained as `FIND-TASK-001-27`.** The changed
  Workflow entry says `skald-workflow` depends on `wyrd-spec`, while the same
  page says Skald has no `wyrd-*` dependency and no Skald engine crate depends
  on `wyrd-spec`; the diagram omits the edge. The manifest and doctrine confirm
  the narrow dependency is intentional.
- `MNT-R4-001`: **CONFIRMED and deduplicated into
  `FIND-TASK-001-27`.** This is the same contradictory architecture guidance,
  not a new finding. The maintainer report's proposal to assign a new ID is
  rejected because this is a failed remediation of the same permanent-doc
  obligation and source as the prior finding.
- `STD-R4-001`: **REVISED.** Its dependency-boundary claim is confirmed and
  retained under `FIND-TASK-001-27`. Its PyO3 clause is rejected from the final
  ledger. The statements at `docs/architecture/skald.md:77-87` and the retained
  owner-crate Python features predate the base commit. Round three explicitly
  limited `FIND-TASK-001-27` to the obsolete Workflow descriptions and missing
  example, required preservation of adjacent behavior, and recorded unrelated
  owner-crate PyO3 migration as outside that review. Correcting that older
  documentation debt is useful but not required to accept this task.
- `FUP-R4-003`: **REVISED.** The dependency portion is confirmed under the
  prior stable ID. The proposed PyO3 expansion is rejected for the same
  base-existing, out-of-scope reason. No separate documentation finding or
  wider cleanup is warranted.

### Harness lifecycle proposals

- `SYS-R4-001`, `CONC-R4-001`, and `FUP-R4-001`: **REVISED and deduplicated as
  `FIND-TASK-001-28`.** The shared lifecycle defect is confirmed. Cancellation
  is a wake-up signal, not completion. `shutdown()` drops a bound `JoinHandle`
  on timeout, detaching the serve task, and the in-process path never invokes
  the retained `Bifrost::shutdown`/`abort` owner before fixture destruction.
  Field order cannot join detached Tokio work, and the dedicated runtime drops
  explicitly use `shutdown_background`. The correction is narrowed to the
  existing harness and Bifrost lifecycle owners; no production shutdown design
  change is needed.
- The behavior and invariant reports' harness `PASS` claims are **REJECTED**.
  Their evidence proves that the shared token is cancelled and the fixture
  field is declared last, but not that database-using work has completed or
  been aborted before `DROP DATABASE ... WITH (FORCE)` begins. One green broad
  suite run does not prove this ordering.

### Telemetry proposal

- `TEL-R4-001` and `FUP-R4-002`: **CONFIRMED and deduplicated as
  `FIND-TASK-001-29`.** A public `before_model` callback can replace an OpenAI
  request's model, and the replaced request reaches `dispatch`, but `chat_span`
  still records the original Prompt model. OpenTelemetry defines
  `gen_ai.request.model` as the model to which the request is made, so this is a
  reachable semantic mismatch. The existing `request_model` helper already
  extracts the effective model for the journal, making reuse the first valid
  Ponytail rung; no new telemetry abstraction is justified.

### Empty and rejected ledgers

- The invariant review proposed no findings. Its Workflow execution, deadline,
  panic, route, and result claims are supported; only its harness acceptance
  claim is rejected as described above.
- The network-security review's empty ledger is **VALIDATED**. The bound-secret
  success-reflection correction remains at the existing external-client owner,
  and no separate reachable network or credential-containment defect was found.
- No optional style, refactoring, or speculative hardening proposal is retained.

## Prior-finding closure

| Prior finding | Result | Validation evidence |
|---|---|---|
| `FIND-TASK-001-20` | **CLOSED** | `StepTask::run` fixes the Agent deadline before dispatch and races it after cancellation, total, and step deadlines but before the complete attempt; expiry uses the existing typed Agent timeout. The focused test holds terminal journal settlement pending and proves retry/exhaustion, precedence, span closure, and no live work. |
| `FIND-TASK-001-21` | **CLOSED** | The complete attempt future is unwind-caught while `AttemptSpan` is alive. A panic follows ordinary failed settlement with `WYRD_WORKFLOW_500_INTERNAL`; interrupted attempts alone fall through the span guard's cancelled outcome. |
| `FIND-TASK-001-22` | **CLOSED** | `ExternalGatewayClient::post` owns the sensitive headers and decoded typed success together, refuses retained reflected values before returning them, and returns fixed safe diagnostics. Focused tests cover reflected and clean success plus no-retry/run-projection behavior. |
| `FIND-TASK-001-23` | **CLOSED AS SPECIFIED** | Ordinary OpenAI, Gemini, and Vertex spans now use the approved provider values, carry the resolved model where the request shape lacks it, and omit the scalar finish-reasons field. `FIND-TASK-001-29` is a distinct supported callback-replacement path not diagnosed or proved by the prior finding. |
| `FIND-TASK-001-24` | **CLOSED** | The cited Agent entry points, moved helpers, timeout/telemetry fixtures, Workflow fixtures, fields, and trait methods have substantive error and lifecycle rustdoc. |
| `FIND-TASK-001-25` | **CLOSED** | Stateful run, prompt-loop, journal, tool, and session orchestration is inherent on `Agent`; the remaining free helpers are stateless transformations. |
| `FIND-TASK-001-26` | **CLOSED** | `skald-workflow` has no direct `anyhow` dependency; the resolver refusal uses `std::io::Error` through the existing `SchemaResolverError` boundary. |
| `FIND-TASK-001-27` | **OPEN / REVISED** | Deleted APIs and the absent example entry were removed, but the replacement Workflow architecture text now directly contradicts the page's dependency prose and diagram. The same permanent-doc obligation and source remain open, so the prior stable ID is preserved. |

Earlier `FIND-TASK-001-1` through `FIND-TASK-001-19` were checked as closure
hypotheses through the cumulative diff and round-three evidence; none became
reachable again. Revision 11's Observer deletion remains intact, with no
compatibility alias or replacement telemetry system.

## Final validated finding ledger

### FIND-TASK-001-27 — The remediated Skald architecture page contradicts its required contract dependency

- Discovery sources: `BEH-R4-001`, dependency portion of `STD-R4-001`,
  `MNT-R4-001`, dependency portion of `FUP-R4-003`.
- Status: **REVISED**.
- Classification: `INCORRECT`.
- Violated obligation: the round-three remediation requires the changed
  permanent Workflow architecture surface to accurately describe the
  implemented Revision 11 ownership and dependency boundary.
- Exact location: `docs/architecture/skald.md:3-5,28-38,42-58`, compared with
  `crates/skald/skald-workflow/Cargo.toml:23-31` and
  `architecture/wyrd-doctrine.mdx:182-195`.
- Producer-to-consumer evidence: the crate map correctly produces the rule that
  `skald-workflow` consumes `wyrd-spec` Workflow Card and run contracts. The
  overview and dependency section then tell maintainers that Skald has no
  `wyrd-*` dependency and no engine crate depends on `wyrd-spec`, while the
  diagram omits the live edge. The manifest has the direct dependency and the
  doctrine says each product depends on the shared wire-contract crate.
- Reachability: this is the permanent architecture page and exact Workflow
  entry changed to close the prior finding; maintainers use it to evaluate
  dependency direction and future changes.
- Observable consequence: the architecture authority gives mutually exclusive
  answers about whether the required contract dependency is allowed. A
  maintainer can remove a required edge or reject a correct change as an
  ownership violation.
- Decision-complete minimum correction: edit only the overview,
  dependency-direction paragraph, and diagram so they show the existing narrow
  `skald-workflow -> wyrd-spec` contract edge while preserving that Skald does
  not depend on Wyrd server/application crates or Vala. Keep the correct crate
  map sentence; add no compatibility narrative, new layer, or broader
  dependency allowance. Do not bundle the unrelated pre-existing PyO3 prose.
- Focused closure proof: compare the page directly with the Workflow manifest
  and doctrine, then run `mise run docs:check` and
  `mise run check:client-tier`.

### FIND-TASK-001-28 — Test-server teardown can release Postgres while owned work remains live

- Discovery sources: `SYS-R4-001`, `CONC-R4-001`, `FUP-R4-001`.
- Status: **REVISED**.
- Classification: `INCORRECT`.
- Violated obligation: commit `516d0fbcc` and its recorded gate-failure
  diagnosis require every database-using role or serve task owned by
  `WyrdTestServer` to finish or be explicitly aborted before the fixture
  force-drops its database.
- Exact location: `crates/wyrd/wyrd-testing/src/server.rs:185-241,757-773,`
  `3457-3486,3555-3574,3609-3638`; existing lifecycle owners at
  `crates/wyrd/wyrd-server/src/state.rs:189-205,252-267,494-508,698-745,`
  `897-909,1967-2089`; destructive consumer at
  `crates/shared/wyrd-dev-fixtures/src/pg.rs:426-475`.
- Producer-to-consumer evidence: `shutdown()` and `Drop` cancel the shared
  process token. In-process construction has no serve handle, so teardown does
  not call `Bifrost::shutdown` or `Bifrost::abort`, the owners that join or
  explicitly abort Oracle/Scribe work. Dropping the dedicated runtime owners
  calls `shutdown_background`, which intentionally does not join worker
  threads. In bound mode, the two-second timeout consumes the `JoinHandle`; on
  timeout it is dropped and the serve task is detached. The fixture destructor
  then synchronously issues `DROP DATABASE ... WITH (FORCE)`. Oracle's reader
  epoch supervisor races cancellation against an in-flight renewal, so token
  cancellation alone does not establish completion before that forced
  connection failure.
- Reachability: in-process servers are used broadly without a bound serve task,
  including the suite that produced the recorded SIGABRT. A real bound drain
  may exceed the harness's two-second join budget; the harness also already has
  a cancellation-resistant bound task seam demonstrating the detached-handle
  path. This is not speculative production hardening.
- Observable consequence: teardown can retain the same scheduling-dependent
  Oracle self-fence/process abort or database-after-drop failure that the
  harness patch claims to eliminate. The `Drop` rustdoc overstates the achieved
  ordering.
- Decision-complete minimum correction: keep the fix in `WyrdTestServer` and
  reuse the existing lifecycle owners. In bound mode, retain the serve handle
  through the timeout and abort then join it when graceful drain expires; do
  not detach it. In in-process mode, run the existing Bifrost shutdown path and
  its abort fallback before releasing runtime owners or the fixture. Implicit
  drop must hand the same retained owners and fixture to an off-runtime cleanup
  path that establishes abort/completion before fixture destruction; merely
  cancelling, sleeping, widening the timeout, or weakening Oracle self-fencing
  does not close the defect. No production lifecycle API or second shutdown
  abstraction is required.
- Focused closure proof: use the existing cancellation-resistant bound task
  seam to prove timeout is followed by abort and join before fixture release,
  and add one deterministic in-process teardown case with live role work that
  proves the existing role owner is drained or aborted before fixture-drop
  observation. Use synchronization or retained-task state, not sleeps. Then run
  the exact focused tests and `mise run test:wyrd`.

### FIND-TASK-001-29 — A callback-replaced request records the wrong GenAI model

- Discovery sources: `TEL-R4-001`, `FUP-R4-002`.
- Status: **CONFIRMED**.
- Classification: `INCORRECT`.
- Violated obligation: Revision 11 `REQ-053` and the telemetry authority require
  model-call spans to use OpenTelemetry GenAI semantic attributes consistently;
  `gen_ai.request.model` names the model to which the request is made.
- Exact location: `crates/skald/skald-agent/src/loop_runtime.rs:290-333,`
  `694-711,817-833,903-917`; callback contract at
  `crates/skald/skald-agent/src/callbacks.rs:54-56,81-102`; reachable proof at
  `crates/skald/skald-agent/tests/callbacks.rs:133-160`.
- Producer-to-consumer evidence: `before_model` may replace the complete
  `ProviderRequest`. The callback test changes an OpenAI request to
  `replacement-model`, and that effective request reaches `dispatch`.
  `chat_span(&request, model)` derives the provider from the effective request
  but records the separately retained Prompt model. The adjacent journal owner
  already calls `request_model(request)` on the same effective request. The
  OpenTelemetry registry defines the attribute as the model a request is made
  to: <https://opentelemetry.io/docs/specs/semconv/registry/attributes/gen-ai/>.
- Reachability: callback replacement is a supported public Agent path with an
  existing provider-dispatch test; it is not dormant or test-only. Existing
  telemetry tests cover only unreplaced requests and therefore cannot expose
  the mismatch.
- Observable consequence: traces and downstream model-keyed latency, error, or
  assurance analysis attribute the actual provider call to the wrong model.
- Decision-complete minimum correction: reuse `request_model(request)` at the
  existing `chat_span` owner for effective request variants that carry a model,
  falling back to the resolved Prompt model only for Gemini/Vertex-style
  request variants whose wire shape omits it. Preserve the current provider
  mapping, `chat` operation, payload-free field set, and absent scalar
  finish-reasons field. Add no telemetry abstraction, exporter, or new request
  model owner.
- Focused closure proof: extend the production-shaped capture test with a
  `before_model` callback that replaces an OpenAI request model; assert both the
  provider-captured request and its `chat` span record `replacement-model`.
  Retain the Gemini/Vertex fallback and payload-marker controls, then run the
  exact focused Agent test and `mise run test:skald`.

## Verification assessment

- Discovery reviewers independently reran the focused Workflow deadline,
  panic, ExtGateway, and Agent telemetry tests; all recorded selections passed.
- The candidate records green format, lint, Skald, shared, Python, codegen,
  boundary, docs, examples, and post-`516d0fbcc` Wyrd lanes.
- Those green results close `FIND-TASK-001-20` through
  `FIND-TASK-001-26`, but they do not prove the three retained gaps: semantic
  prose checks do not detect the architecture contradiction, the broad Wyrd
  lane does not assert teardown quiescence, and ordinary telemetry capture does
  not exercise callback replacement.
- `git diff --check` for the immutable range was recorded green by discovery
  reviewers. No candidate source was modified during this validation.

## Structured validation result

**VALIDATED NONEMPTY LEDGER — FIX_REQUIRED.** Retain
`FIND-TASK-001-27`, `FIND-TASK-001-28`, and `FIND-TASK-001-29` only.
