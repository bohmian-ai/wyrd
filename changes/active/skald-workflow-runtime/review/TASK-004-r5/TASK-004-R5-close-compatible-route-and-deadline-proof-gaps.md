---
id: TASK-004-R5
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 13
parent_task: TASK-004
remediates: [FIND-TASK-004-22, FIND-TASK-004-23]
---

# Close compatible external-route and deadline-proof gaps

Implementation skill: `$wyrd-implement`.

## Immutable review inputs

- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Prior remediations: TASK-004 R1 through R4 in their preceding review
  directories
- Reviewed base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Reviewed candidate: `d4d4e2da53abfc677abdb804e71517c3b6849f49`
- Validated diagnosis:
  `changes/active/skald-workflow-runtime/review/TASK-004-r5/findings-validation.md`

## Outcome

Complete TASK-004 by preserving the prepared-run deadline implementation while
proving every required query-deadline precedence case through the actual
Workflow tool path, and by allowing the existing custom OpenAI-compatible
Prompt variant to use the existing external OpenAI Chat protocol end to end.

## Diagnoses and required corrections

### Required positive deadline proof (`FIND-TASK-004-22`)

R4 correctly removed the independently sampled tool deadline.
`WorkflowExecutor` fixes the one total deadline, `PreparedWorkflowRun` exposes
it, and the server binds that exact instant once into every shared `RunTools`
clone before acceptance. `QueryTool::invoke` also contains the correct
`requested.min(remaining)` projection.

The R4 task nevertheless made three actual tool-input cases part of completion:
an omitted deadline and an explicit longer deadline must clip to the prepared
run boundary, while an explicit shorter positive deadline must remain shorter.
The candidate's focused owner test only compares the stored instants and never
invokes the tool. The forwarded Oracle journey omits `deadline_ms`, and the
real-server tool journey supplies only zero, which is rejected during argument
validation. A regression that always overwrites a caller's shorter deadline or
fails to clip a longer one can therefore pass every recorded R4 assertion.

Keep the production deadline owner and one-time binding unchanged unless the
new proof exposes a defect. Extend the existing forwarded Workflow-query
journey so its current admitted run, forwarded Oracle query, scripted
post-tool continuation, lifecycle metrics, settlement, and recovery evidence
distinguish all three inputs. The omitted and explicit-longer cases must reach
the one run boundary and remain `TimedOut` without a successful continuation.
The explicit-shorter case must reach its own query timeout first, deliver the
existing redacted tool failure to the scripted continuation, and allow that
continuation to finish while the Workflow's total deadline is still live.

This uses the existing production path and existing journey owner. Do not add
a helper-only substitute, internal-value assertion in place of the user path,
clock service, pause protocol, timer task, channel, setting, option, checker,
or test harness.

### Compatible OpenAI Chat external routing (`FIND-TASK-004-23`)

`ProviderRequest::OpenAiChatCompatible` contains a normal
`OpenAiChatRequest`, retains a custom `ProviderName` for registry dispatch, is
accepted by the Agent OpenAI Chat loop, and survives Prompt persistence and
Workflow hydration. Revision 13 `REQ-038` and `REQ-039` bind an external route
to the Prompt request dialect, not to the built-in OpenAI provider identity.

The candidate rejects that valid combination twice. `protocol_matches`
recognizes only `OpenAiChatCompletion` for
`ExternalGatewayProtocol::OpenAiChat`, so preparation reports a dialect
mismatch. If that guard alone changed, `ExternalGatewayClient::send` would
still return `VariantMismatch` rather than post the compatible variant's inner
request. The in-process gateway already treats both variants as the same
OpenAI Chat request/response dialect.

Extend the existing OpenAI Chat alternative at both owning dispatch points to
accept `OpenAiChatCompatible { request, .. }`. Send that existing inner
request unchanged through the existing `chat/completions` path and decode the
existing `ProviderResponse::OpenAiChatCompletion`. Keep the embedded custom
provider solely as the existing provider-registry dispatch identity. Prove the
complete validation and send path by extending the existing
`server_routes_keep_gateway_and_external_ownership` journey with one registered
custom OpenAI-compatible Prompt on its stored
`ExtGateway(protocol = openai_chat)` route, asserting one direct request with
the native body, the existing response shape, and no Wyrd-gateway ingress.

Do not add a protocol variant, request conversion, response type, adapter,
registry, compatibility reader, downstream guard, dependency, setting,
option, checker, or new fixture system. Those mechanisms are unnecessary and
would be DRIFT from both established typed-enum dispatch and ordinary
OpenAI-compatible gateway practice.

## Preserved behavior and non-goals

- Preserve closure of `FIND-TASK-004-1` through `FIND-TASK-004-21`.
- Preserve the single prepared-run deadline owner and its one-time `RunTools`
  bind, explicit shorter-deadline precedence, query cancellation, tracked
  settlement, terminal compare-and-set, and capacity-release ordering.
- Preserve the adjacent-tagged Revision 13 request wire, embedded custom
  provider identity, native and Wyrd-gateway dispatch, route immutability,
  endpoint screening, credential binding, secret redaction, retry, timeout,
  and response-limit behavior.
- Preserve Cards, authorization, audit, tenancy, snapshots, attempts,
  retention, and shutdown behavior.
- Keep Oracle graph-drain polling and supervisor idle refusal deleted.
  Follower release remains participant grant-stream close and the leader
  awaits no acknowledgement.
- Do not provision foreign-tenant gateway credentials merely to add a model
  step to that journey.
- Published `Running` still reserves attempt one, and interrupted published
  work still settles `Cancelled` with its timestamps.
- No new public API, wire field, compatibility behavior, lifecycle owner,
  query engine, protocol, dependency, test harness, repository check, setting,
  or option.

## Acceptance criteria

| Finding | Closure criterion |
|---|---|
| `FIND-TASK-004-22` | The existing forwarded Workflow-query journey drives omitted, explicit longer, and explicit shorter positive query deadlines through `QueryTool` and Oracle and observes the three required outcomes. |
| `FIND-TASK-004-22` | Omitted and longer values clip to the exact prepared-run boundary; the shorter value terminates the query first while the Workflow remains live for the scripted continuation. |
| `FIND-TASK-004-22` | The prepared-instant equality proof, query settlement/recovery evidence, cancellation behavior, and prior deadline finding closures remain intact without production-only test seams. |
| `FIND-TASK-004-23` | `OpenAiChatCompatible` matches the existing `openai_chat` external protocol during resolved validation and posts its unchanged inner request through the existing `/chat/completions` external client path. |
| `FIND-TASK-004-23` | The existing server external-route journey proves registered compatible-Prompt hydration, pre-dispatch acceptance, exactly one direct native-body request, the existing OpenAI Chat response, and no Wyrd-gateway ingress. |
| `FIND-TASK-004-23` | Ordinary OpenAI Chat and every other route/dialect mismatch, external-egress security, credential, redaction, timeout, retry, and result-bound behavior remain unchanged. |

## Focused and broader proof

Extend the two existing journey tests named above; do not add a new test target
or harness. Run their exact focused commands:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=workflow::workflow_forwarded_query_settles_before_the_run_ends)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_workflow_runs -E "test(=server_routes_keep_gateway_and_external_ownership)"'
```

Retain the existing exact prepared-deadline owner proof:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib -E "test(=components::workflow::tools::tests::tools_use_the_prepared_run_deadline)"'
```

Then run the narrow existing broader lanes for the touched Skald, server, and
Oracle surfaces:

```bash
mise run fmt
mise run lints
mise run test:skald
WYRD_TEST_PACKAGES="wyrd-server" mise run test:wyrd
mise run test:bifrost:journey:oracle
```

The implementation report must identify the two corrected existing dispatch
owners, show the actual omitted/longer/shorter query outcomes, record the exact
focused commands and broader lane results, and confirm that no prohibited
mechanism or unrelated behavior entered the diff.
