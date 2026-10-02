# TASK-001 focused follow-up review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `a704a8890ef20efe65fee1e116f7d02288f8ec5c`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation task: `changes/active/skald-workflow-runtime/review/TASK-001-r3/TASK-001-R2-close-round-three-runtime-gaps.md`
- Discovery inputs: every Markdown report present in `review/TASK-001-r4/` before this report

The candidate remained at the stated commit before and after this trace. No
`.codegraph/` directory exists, so navigation used Git, `rg`, and direct source
and caller inspection.

## 1. `WyrdTestServer` teardown conflict

### Source path inspected

- Harness construction and teardown:
  `crates/wyrd/wyrd-testing/src/server.rs:150-241,745-774,810-870,913-973,3457-3486,3489-3606,3609-3638,4451-4596`
- Bifrost role and runtime ownership:
  `crates/wyrd/wyrd-server/src/state.rs:159-205,223-267,387-435,437-478,494-508,698-745,897-909,1583-1685,1953-2090`
- Production composition and shutdown:
  `crates/wyrd/wyrd-server/src/boot/mod.rs:617-760,1248-1361`
- Fixture cleanup:
  `crates/shared/wyrd-dev-fixtures/src/pg.rs:426-475`
- Discovery claims:
  `task-review-behavior.md`, `task-review-invariants.md`,
  `system-review.md`, and `domain-review-concurrency.md`

### Resolution

The behavior/invariant acceptance claim is not supported by the lifecycle
owners. Commit `516d0fbcc` fixes two necessary conditions, but not the required
quiescence condition:

1. `WyrdTestServer::shutdown` and `Drop` now cancel
   `inner.state.shutdown_token`, which is the token handed into
   `compose_bifrost` and retained by the role graph.
2. `fixture` is declared after `state` and the two dedicated runtime owners, so
   synchronous field destruction reaches the fixture last.

Cancellation is only a signal. In-process construction leaves both
`shutdown_token` and `serve_handle` as `None`
(`server.rs:4559-4595`). Explicit `shutdown()` therefore cancels the state token
and immediately moves the server to `spawn_blocking(drop)`
(`server.rs:757-773`); implicit `Drop` on an active Tokio runtime cancels and
returns (`server.rs:3609-3627`). Neither path invokes the existing
`Bifrost::shutdown` or `Bifrost::abort` owner. Those owners are what await or
abort the retained Oracle heartbeat, snapshot, continuity, query, Scribe, and
storage work (`state.rs:698-745,897-909,1990-2090`). Dropping
`ScribeCoordinationRuntime` and `ForgeCompactionRuntime` uses
`shutdown_background`, explicitly detaching rather than joining their workers
(`state.rs:189-205,252-267`). Field order therefore orders owner-value
destruction, not completion of detached database users. `PgFixture` can then
run synchronous `DROP DATABASE ... WITH (FORCE)` (`pg.rs:426-475`) while such
work is still live.

The bound path has a second, deterministic ownership gap. `shutdown()` passes
the serve `JoinHandle` by value into a two-second timeout
(`server.rs:765-770`). On timeout the handle is dropped and the task is
detached, after which fixture destruction proceeds. The harness already has a
reachable bound task that deliberately ignores cancellation until it is
aborted (`server.rs:3555-3573`), and startup rollback already demonstrates the
correct local pattern: retain the handle across the timeout, then abort and
join it (`server.rs:3457-3486`).

The recorded green `mise run test:wyrd` shows that the original race did not
fire in that run. It does not establish that role work or a timed-out serve task
cannot reach Postgres after fixture destruction begins. The new `Drop` rustdoc
at `server.rs:3610-3614` consequently promises stronger ordering than the code
provides.

### Proposed finding: FUP-R4-001

- Discovery sources: `SYS-R4-001`, `CONC-R4-001`; conflicts with the harness
  PASS claims in the behavior and invariant reports.
- Classification: `INCORRECT`.
- Violated obligation: commit `516d0fbcc` and its recorded diagnosis require
  database-using background work to stop before the fixture force-drops its
  database.
- Observable consequence: in-process teardown and a timed-out bound drain can
  still race `DROP DATABASE ... WITH (FORCE)`, retaining the diagnosed
  scheduling-dependent Oracle self-fence/SIGABRT failure.
- Smallest correction boundary: keep the correction in `WyrdTestServer` and
  reuse existing lifecycle owners. In bound mode, retain the serve handle
  through timeout and abort-and-join it instead of detaching it. In in-process
  mode, drive the existing Bifrost shutdown/abort ownership path and do not
  release the fixture until retained role work is drained or aborted. Implicit
  drop must preserve the same no-live-database-user invariant; cancellation
  alone is insufficient. Focused proof must cover one live in-process role and
  the existing cancellation-resistant bound task, asserting quiescence or
  abort completion before fixture-drop observation without sleeps or a wider
  timeout.

## 2. Effective model telemetry (`TEL-R4-001`)

### Source path inspected

- Callback contract and replacement chain:
  `crates/skald/skald-agent/src/callbacks.rs:13-28,54-56,81-102`
- Agent producer-to-dispatch path:
  `crates/skald/skald-agent/src/loop_runtime.rs:60-163,186-249,262-350,653-671,694-711,800-833,903-930`
- Runtime dispatch:
  `crates/skald/skald-runtime/src/dispatch.rs:14-44,73-89`
- Reachability proof:
  `crates/skald/skald-agent/tests/callbacks.rs:133-160`
- Authority: Revision 11 `REQ-053`
  (`changes/active/skald-workflow-runtime/spec.md:1989-2006`) and
  `architecture/references/domain/telemetry-observations.md:44-54`

### Resolution

`TEL-R4-001` is reachable and correct. `BeforeModelFn` may replace the complete
`ProviderRequest`. `Agent::run_loop` applies that chain before journaling,
constructing the `chat` span, and dispatch (`loop_runtime.rs:290-333`). The
existing callback test changes an OpenAI request model to
`replacement-model` and proves that exact request reaches the provider
(`callbacks.rs:133-160`).

After replacement, `chat_span(&request, model)` derives the provider from the
effective request but records the separately retained Prompt model
unconditionally (`loop_runtime.rs:331-333,824-830`). The provider therefore
receives `replacement-model` while the model-call span records the original
Prompt model. This violates REQ-053's requirement to use OpenTelemetry GenAI
semantics: `gen_ai.request.model` is the model the request is made to, not the
pre-callback default. The journal already demonstrates the correct owner
boundary by reading `request_model(request)` from the effective request
(`loop_runtime.rs:699-709`). Existing telemetry tests cover unreplaced OpenAI
and Prompt-carried Gemini/Vertex models, so they cannot expose the disagreement.

### Proposed finding: FUP-R4-002

- Discovery source: `TEL-R4-001`.
- Classification: `INCORRECT`.
- Violated obligation: Revision 11 `REQ-053` and the repository telemetry
  authority require the standard request-model attribute to describe the
  dispatched model call accurately.
- Observable consequence: traces and downstream model-keyed latency, error, or
  assurance analysis attribute a real call to the wrong model.
- Smallest correction boundary: at the existing `chat_span` owner, use
  `request_model(request)` for effective request variants that carry a model,
  falling back to the resolved Prompt model only for variants such as
  Gemini/Vertex whose request shape does not carry it. Preserve the current
  provider mapping, `chat` operation, payload exclusion, and absent scalar
  finish-reasons field. Extend the production-shaped capture proof so one
  callback-replaced OpenAI request reaches the provider and its `chat` span with
  the same replacement model; retain the Google/Vertex fallback controls.

## 3. Skald architecture documentation claims

### Source path inspected

- Changed page: `docs/architecture/skald.md:3-5,28-38,42-62,77-87`
- Workflow manifest: `crates/skald/skald-workflow/Cargo.toml:15-47`
- Retained owner-crate Python boundaries:
  `crates/skald/skald-agent/Cargo.toml:15-43`,
  `crates/skald/skald-agent/src/agent.rs:166-201`,
  `crates/skald/skald-agent/src/run.rs:11-333`,
  `crates/skald/skald-agent/src/python.rs:1-496`,
  `crates/skald/skald-runtime/src/python.rs:1-69`,
  `crates/skald/skald-tool/src/python.rs:1-249`, and
  `crates/skald/skald-workflow/src/python.rs:1-21`
- Repository authority: `AGENTS.md` sections 2, 3, 7, and 8
- Prior remediation: `FIND-TASK-001-27` in
  `review/TASK-001-r3/findings-validation.md:377-402` and its acceptance
  criterion in `TASK-001-R2-close-round-three-runtime-gaps.md:148-161,181-192`

### Resolution

The dependency claim reported by `BEH-R4-001` and `MNT-R4-001` is confirmed.
The page's new Workflow entry correctly says `skald-workflow` depends on
`wyrd-spec` for Workflow Card and run contracts (`skald.md:28-38`), matching the
manifest (`skald-workflow/Cargo.toml:30`) and Revision 11's placement of pure
Workflow contracts in `wyrd-spec`. The same page says Skald depends only on
neutral infrastructure and Skald crates, has no `wyrd-*` dependency, and that
Skald engine crates do not depend on `wyrd-spec` (`skald.md:3-5,42-46`); its
diagram omits the live `skald-workflow -> wyrd-spec` edge
(`skald.md:48-58`). These statements are mutually exclusive.

The additional PyO3 claim in `STD-R4-001` is also false and must not be lost
when the documentation finding is reconciled. `skald.md:77-87` says only
`skald-prompt` and `wyrd-cards` opt into Python and that `skald-agent`,
`skald-runtime`, `skald-tool`, and `skald-workflow` are source-level PyO3-free.
Their manifests retain optional Python features, and the cited owner modules
contain real PyO3 types, attributes, registration, or the approved orphan-rule
boundary. That is permitted migration state under `AGENTS.md`: existing owner
crates may retain optional Python features, only `wyrd-sdk-python` enables and
aggregates them, and new or materially relocated Python logic belongs in the
SDK. The documentation currently describes a completed consolidation that the
repository authority explicitly says is only the long-term direction.

These are two independently necessary correction clauses, but one material
documentation finding. They share the same violated obligation and source:
the task-modified architecture page used to close `FIND-TASK-001-27` must
accurately describe the implemented and approved boundaries. They do not need
separate product or architecture decisions, separate checks, or separate
remediation abstractions.

### Proposed finding: FUP-R4-003

- Discovery sources: `BEH-R4-001`, `STD-R4-001`, `MNT-R4-001`.
- Classification: `INCORRECT`; prior `FIND-TASK-001-27` remains open rather
  than closed.
- Violated obligation: changed permanent documentation must accurately
  describe the implemented Revision 11 surface and repository-approved
  ownership boundaries.
- Observable consequence: maintainers receive contradictory instructions on
  whether the required `wyrd-spec` edge and retained optional Python owner
  features are sanctioned or must be removed.
- Smallest correction boundary: edit only `docs/architecture/skald.md` to
  (a) show the narrow `skald-workflow -> wyrd-spec` contract edge while keeping
  Skald independent of Wyrd server/application and Vala crates, and (b) state
  the current transitional Python rule: existing named owner crates retain
  optional Python features enabled and aggregated only by `wyrd-sdk-python`,
  while new or materially moved wrappers belong in the SDK. Do not add a
  compatibility narrative, new rule, or architecture layer. Prove the prose by
  direct comparison with the manifests/source and run `mise run docs:check`,
  `mise run check:client-tier`, and `mise run check:pyo3-scope`.

## Follow-up result

**RESOLVED.** All three uncertainties were decided from the immutable
candidate and approved authority. The proposed finding union is
`FUP-R4-001`, `FUP-R4-002`, and `FUP-R4-003`; final confirmation,
deduplication, stable `FIND-*` assignment, and correction wording remain the
independent `ponytail-rev`'s responsibility.
