# TASK-001-R3 — Close round-four review gaps

## Authority and immutable subject

- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Parent remediation:
  `changes/active/skald-workflow-runtime/review/TASK-001-r3/TASK-001-R2-close-round-three-runtime-gaps.md`
- Review verdict:
  `changes/active/skald-workflow-runtime/review/TASK-001-r4/verdict.md`
- Validated ledger:
  `changes/active/skald-workflow-runtime/review/TASK-001-r4/findings-validation.md`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Reviewed candidate: `a704a8890ef20efe65fee1e116f7d02288f8ec5c`
- Findings: `FIND-TASK-001-27`, `FIND-TASK-001-28`,
  `FIND-TASK-001-29`

Implementation skill: `$wyrd-implement`.

## Outcome

Finish TASK-001 by making its changed architecture guidance accurate, making
test-server teardown establish quiescence before forced database removal, and
recording the effective post-callback model on payload-free Agent call spans.
Preserve every already-closed runtime, security, API, and Revision 11 Observer
deletion obligation.

## Diagnoses and required corrections

### FIND-TASK-001-27 — Correct the Skald contract-dependency guidance

- Violated obligation: the round-three remediation requires changed permanent
  Workflow architecture documentation to describe the implemented Revision 11
  ownership and dependency boundary accurately.
- Current behavior: `docs/architecture/skald.md:28-38` correctly says
  `skald-workflow` consumes Workflow Card and run contracts from `wyrd-spec`,
  while the same page at `:3-5,42-58` says Skald has no `wyrd-*` dependency,
  says no engine crate depends on `wyrd-spec`, and omits the live edge from its
  diagram.
- Evidence: `crates/skald/skald-workflow/Cargo.toml:23-31` has the direct
  dependency, and `architecture/wyrd-doctrine.mdx:182-195` permits products to
  consume the foundational shared contract crate without depending on Wyrd
  server/application code.
- Observable consequence: the architecture page gives maintainers mutually
  exclusive rules, inviting removal of a required dependency or rejection of
  a correct contract change.
- Why the reviewed candidate falls short: it removed obsolete Workflow names
  but replaced them with prose that conflicts with the same page and the live
  manifest, so prior `FIND-TASK-001-27` is not closed.

Required correction: edit only the overview, dependency-direction paragraph,
and dependency diagram to show the existing narrow
`skald-workflow -> wyrd-spec` contract edge. Preserve the prohibition on Skald
depending on Wyrd server/application crates or Vala. Do not broaden this task
into the pre-existing optional owner-crate PyO3 prose, compatibility guidance,
or another architecture rewrite.

### FIND-TASK-001-28 — Establish test-server quiescence before fixture drop

- Violated obligation: the `516d0fbcc` gate-failure diagnosis requires every
  database-using role or serve task owned by `WyrdTestServer` to finish or be
  explicitly aborted before `PgFixture` executes
  `DROP DATABASE ... WITH (FORCE)`.
- Current behavior: `WyrdTestServer::shutdown` and `Drop` cancel
  `inner.state.shutdown_token`; the fixture is the last field. In-process mode
  has no serve handle and does not invoke the retained `Bifrost::shutdown` or
  abort owner. Dedicated runtime drops use `shutdown_background`. Bound mode
  consumes and drops the serve `JoinHandle` after a two-second timeout, which
  detaches the task.
- Evidence: harness paths in
  `crates/wyrd/wyrd-testing/src/server.rs:185-241,757-773,3457-3486,3555-3574,3609-3638`;
  existing lifecycle owners in
  `crates/wyrd/wyrd-server/src/state.rs:189-205,252-267,494-508,698-745,897-909,1967-2089`;
  destructive fixture cleanup in
  `crates/shared/wyrd-dev-fixtures/src/pg.rs:426-475`.
- Observable consequence: cancellation can race an in-flight Oracle renewal or
  other owned database work with forced database removal, retaining the
  scheduling-dependent process abort or after-drop database failure that the
  harness patch claims to eliminate.
- Why the reviewed candidate falls short: cancellation is a wake-up signal,
  not completion, and field destruction cannot join detached Tokio work. One
  green broad test run does not prove the required ordering.

Required correction: keep ownership in `WyrdTestServer` and reuse the existing
serve-task and Bifrost lifecycle owners. For a bound server, retain the serve
handle through the graceful timeout; if the timeout expires, abort and join it
before fixture release instead of detaching it. For an in-process server, run
the existing Bifrost shutdown path and its abort fallback before runtime-owner
or fixture release. Implicit drop must transfer the retained lifecycle owners
and fixture to an off-runtime cleanup path that establishes completion or abort
before fixture destruction. Do not replace this with a sleep, a larger timeout,
weaker Oracle self-fencing, a second shutdown abstraction, or production
lifecycle API changes.

### FIND-TASK-001-29 — Trace the effective request model

- Violated obligation: Revision 11 `REQ-053` and the telemetry authority require
  `gen_ai.request.model` to identify the model to which the request is made.
- Current behavior: a public `before_model` callback can replace the complete
  OpenAI `ProviderRequest`, and the replacement reaches dispatch, but
  `chat_span(&request, model)` records the separately retained resolved Prompt
  model.
- Evidence: `crates/skald/skald-agent/src/loop_runtime.rs:290-333,694-711,817-833,903-917`;
  callback contract at `crates/skald/skald-agent/src/callbacks.rs:54-56,81-102`;
  replacement dispatch proof at
  `crates/skald/skald-agent/tests/callbacks.rs:133-160`. The adjacent journal
  path already derives the effective value with `request_model(request)`.
- Observable consequence: traces and downstream model-keyed analysis attribute
  a real provider call to the wrong model.
- Why the reviewed candidate falls short: ordinary OpenAI, Gemini, and Vertex
  capture tests do not exercise supported callback replacement.

Required correction: reuse the existing `request_model(request)` mechanism at
the current call-span owner for effective request variants that carry a model.
Fall back to the resolved Prompt model only for Gemini/Vertex-style request
variants whose wire shape omits it. Preserve provider mapping, the `chat`
operation, payload exclusion, and omission of the scalar finish-reasons field.
Do not add a telemetry abstraction, exporter, callback API, or second model
owner.

## Constraints and preserved behavior

- Preserve the explicit Workflow contracts, deterministic namespaced results,
  retry classifications, deadline precedence, cancellation and drain behavior,
  panic-to-failed settlement, and portable Rust/Python result projection.
- Preserve ExtGateway origin/header/DNS/TLS/no-proxy/no-redirect/body limits and
  successful-response credential containment.
- Preserve complete deletion of the Skald Observer crate, hooks, aliases,
  Python exports, tests, examples, and payload-bearing callback surface.
- Preserve ordinary OpenAI/Gemini/Vertex provider names, the approved `chat`
  operation, span hierarchy, stable failure codes, and payload-free fields.
- Keep the harness correction test-only and on existing lifecycle owners. Do
  not weaken production self-fencing or alter production shutdown semantics.
- Do not add a crate, dependency, feature, public API, compatibility layer,
  migration section, check, retry, sleep, or configurable shutdown framework.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-001-27` | The architecture overview, dependency prose, and diagram agree with the manifest and doctrine on the narrow `skald-workflow -> wyrd-spec` edge while retaining the no-server/no-Vala boundary. |
| `FIND-TASK-001-28` | Bound timeout teardown aborts and joins the serve task before fixture release; in-process teardown completes or aborts existing Bifrost-owned work before runtime owners and the fixture are released; implicit drop provides the same ordering without sleeps. |
| `FIND-TASK-001-29` | A callback-replaced OpenAI request is dispatched and traced with the replacement model; Gemini/Vertex retain the resolved-model fallback and all existing payload/privacy controls. |

## Focused proof and broader verification

1. Use the existing cancellation-resistant bound-task seam to prove a graceful
   timeout is followed by abort and join before fixture release.
2. Add one deterministic in-process teardown case with live role work and an
   observable barrier proving completion or abort precedes fixture drop. Use
   synchronization or retained task state, not sleeps.
3. Extend the production-shaped Agent telemetry capture with a `before_model`
   replacement. Assert that both provider dispatch and the `chat` span record
   the replacement model; retain Gemini/Vertex fallback and payload-marker
   controls.
4. Compare the Skald architecture page directly with the Workflow manifest and
   doctrine.
5. Run every specifically named test with its exact `mise exec -- cargo nextest
   run --locked` selector and record the selected count.
6. Run the narrowest existing broader lanes covering the touched surfaces:

```bash
mise run fmt
mise run lints
mise run test:skald
mise run test:wyrd
mise run docs:check
mise run check:client-tier
git diff --check
```

A red required lane blocks completion. Route this task directly to
`$wyrd-implement`; a later task review must reassess the complete cumulative
base-to-candidate range.
