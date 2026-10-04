# TASK-004 Behavior Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5cde1b48aab0d70d8686ee8fb5f2978e26cd7f58`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: `TASK-004-R1-close-accepted-job-gaps.md`,
  `TASK-004-R2-align-revision-and-source-contracts.md`, and
  `TASK-004-R3-close-step-attempt-and-import-gaps.md`
- Prior verdicts and validated ledgers:
  `changes/active/skald-workflow-runtime/review/TASK-004-r1/` through
  `changes/active/skald-workflow-runtime/review/TASK-004-r3/`

The candidate resolved to the requested immutable commit before and after this
review. `.codegraph/` is absent, so the review used immutable Git objects and
repository source. This was a strictly read-only source and recorded-evidence
audit: no build, test, Cargo, or mise command was run.

## Caller-to-result coverage

The complete base-to-candidate range was followed through authenticated
create/get/cancel, canonical request hashing and scoped idempotent admission,
tracked graph preparation, tenant-scoped exact Card pinning, Skald preparation
and execution, governed gateway and built-in tool calls, Oracle query
settlement, whole-snapshot observation, terminalization, retention, and
shutdown. Revision 13 was followed from adjacent-tagged `ProviderRequest`
serialization through stored Prompt bodies, generated schemas, provider
projections, fixtures, SDK-facing authoring, and the server-internal Vertex
journey.

The R3 delta was inspected separately against its two validated findings. It
changes the existing scheduler/ledger source of attempt state and four import
sites only. It adds no owner, protocol, dependency, compatibility path,
setting, option, check, or test harness. The fixed human decisions were treated
as authority: deleted Oracle graph-drain polling and supervisor idle refusal
remain deleted; participant grant-stream close is follower release and the
leader awaits no release acknowledgement; a foreign-tenant journey need not
run a model step because only the fixture tenant has seeded gateway
credentials; and a published `Running` step reserves attempt one while an
interrupted published step settles `Cancelled`.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Earliest validation, exact active registered graph, and refusal of unsupported `Native` routes or unavailable/duplicate server tools (`REQ-014`, `REQ-015`, `REQ-029`, `AC-009`) | `components/cards/resolve.rs::PinnedWorkflowGraph::pin`; `components/workflow/host.rs:308-467` | Recorded `admission_is_audited_and_side_effect_free_on_refusal`, including no-upstream assertions | PASS |
| Bounded execution and complete deterministic snapshots, including partial failure, cancellation, deadline, attempt, and output semantics (`REQ-017`-`REQ-023`, `REQ-045`, `REQ-048`, `INV-023`, `AC-008`, `AC-019`, `AC-020`, `AC-028`) | Server limits in `workflow/host.rs:358-379`; Skald scheduling/settlement in `skald-workflow/src/workflow.rs:245-379`; snapshot accounting in `run.rs:45-336`; whole replacements in `workflow/runs.rs:650-688` | Recorded lifecycle/graph journeys and focused Skald assertions, including `prepared_run_keeps_its_id` and `bounded_attempt_lifecycle` | PASS |
| Every published running step represents begun attempt one; an immediate abort settles it cancelled without erasing timestamps (`FIND-TASK-004-19`; Revision 13 snapshot invariants) | `WorkflowExecutor::drive` stores attempt one before `step_started` and publication (`workflow.rs:273-290`); `RunLedger::step_started` publishes attempt one (`run.rs:125-136`); cancelled joins settle through `step_cancelled` and `finish` rewrites only pending steps (`workflow.rs:344-365`; `run.rs:183-189,221-250`) | `prepared_run_keeps_its_id` checks each observed running step has attempt one (`workflow.rs:759-823`); the corrected pre-poll branch checks `Cancelled`, attempt one, and retained timestamps (`workflow.rs:1459-1510`); recorded focused selectors and `test:skald` passed | PASS |
| The asynchronous HTTP resource returns queued `202`, replay `200`, fresh authenticated/audited create/get/cancel decisions, common scoped 404s, and no run for preparation failure (`REQ-030`, `REQ-032`, `REQ-033`, `AC-004`, `AC-010`) | `components/workflow/routes.rs:24-180`; authorization before admission/lookup in `workflow/host.rs:75-203` | Recorded admission, principals, audit, client, and served OpenAPI evidence | PASS |
| One preparation/run per tenant, principal, and key; conflict/replay behavior, disconnect survival, failure release, waiter isolation, and shutdown wakeup (`REQ-034C`, `INV-018`, `AC-021`) | `WorkflowRuns::admit` and tracker token (`workflow/runs.rs:258-367`); `Reservation::{accept,fail,drop}` (`runs.rs:530-635`) | Recorded `tracked_preparation_replay_and_disconnect` plus owner tests for pre-spawn reservations and abandoned blocking work | PASS |
| Accepted work uses token-free captured authority and a pinned graph without later widening, while fresh HTTP calls authorize normally and gateway governance stays live (`REQ-032A`, `INV-005`, `INV-022`, `AC-027`) | Captured `Caller` and pinned graph in `workflow/host.rs:230-399`; replay returns stored state; gateway caller invokes ordinary admission with `authorized = false` in `gateway/workflow.rs` | Recorded token expiry, role revocation/restoration, root deletion, replay non-widening, and live credential-revocation cases in `accepted_authority_outlives_submission_only` | PASS |
| Built-in `bifrost.query` and `cards.get` retain Agent declarations, exact captured identity, resource authorization/audit, closed argument/result schemas, and redacted non-retryable failures (`REQ-052`, `INV-006`, `INV-008`, `AC-025`) | `workflow/tools.rs:42-333`; authorized `get_card_by_ref_for`; shared `query/collect.rs::BoundedQuery` | Recorded real tool-loop journey covers both successful tools, malformed/bounded/terminal cases, independent permissions, inaccessible objects, tenant separation, and cancellation | PASS |
| A query owner survives waiter/step abort and settles under the original query deadline before Workflow terminal commit, without a second query engine (`INV-019`, `INV-021`, Scenarios 4 and 6) | `QueryTool::invoke` uses the run `TaskTracker` (`tools.rs:171-215`); `BoundedQuery::run` (`query/collect.rs:275-324`); `RunningQueryControls::{open_cancellable,cancel_and_settle}`; `tools.drain()` precedes `AcceptedRun::finish` (`workflow/host.rs:285-287`) | Recorded original-deadline unit proof and forwarded cancel/deadline/pod-loss journey with later sibling serviceability | PASS |
| Governed and external gateways retain dialect, fallback, remaining deadline, cancellation, credential, origin, and direct-egress ownership (`REQ-034`, `REQ-036A`, `REQ-038`-`REQ-043`, `INV-010`-`INV-012`, `INV-020`, `AC-011A`, `AC-014`-`AC-017`) | `components/gateway/workflow.rs`; tenant-qualified binding selection in `workflow/host.rs:333-379`; shared external endpoint policy | Recorded route journey covers OpenAI Chat/Responses, Anthropic, Gemini, internal Vertex, incompatible capability refusal, fallback isolation, deadline/cancellation, and direct bound egress | PASS |
| Process-local lifecycle stays bounded: no active eviction, fixed retention, tenant/global oldest-terminal eviction, restart loss, and shared-budget shutdown (`REQ-034A`, `REQ-034B`, `INV-013`, `AC-018`, `AC-022`) | `RunTable::sweep` (`workflow/runs.rs:117-209`); `AcceptedRun::finish`; `WorkflowRuns::drain`; `BoundServer` shutdown order | Recorded lifecycle journey covers terminal races, complete GET snapshots, both eviction levels, queued/running shutdown, restart loss, and common hidden-run behavior | PASS |
| Real tenant isolation crosses Workflow root, run, Card, and Bifrost boundaries without requiring unavailable foreign-tenant model credentials (`INV-006`, `AC-009`, `AC-018`, `AC-025`) | Credential-derived second-tenant principals and RLS-backed owners; tenant-qualified run keys and binding selection | Recorded admission, idempotency, tool, and lifecycle foreign-tenant cases; the no-foreign-model limitation is the approved harness boundary | PASS |
| Revision 13 provider tags reject mismatched bodies, preserve `RawV1`, update authoring/schema consumers, and keep a stored Vertex request Vertex through internal dispatch | `skald-spec/src/request.rs:18-59,196-337`; generated Prompt-bearing schemas and tagged fixtures/examples; `gateway/workflow.rs::NativeCall::project` | Recorded round-trip, mismatch, RawV1, generated-schema, SDK, and internal Vertex journey evidence | PASS |
| R3 import corrections use ordinary module-scope imports and bare interface types without a new enforcement mechanism (`FIND-TASK-004-20`) | Bare `Value`, `Instant`, and `Bytes` at the four validated interfaces; module-scope gated `PermissionsExt as _` in `pg_workflow_runs.rs:10-12` | Direct source inspection plus recorded existing format/lint and affected owner lanes | PASS |
| Non-goals remain excluded: no durable queue/recovery/lease, Workflow principal, bearer retention, arbitrary server tools, second audit/query engine, compatibility reader, remote Python/TypeScript/MCP run lifecycle, release-ack protocol, or bespoke polling/refusal/check/setting | Process-local `WorkflowRuns`; two fixed read tools; reused Cards/gateway/query owners; tagged-only request enum; grant-stream-close follower release | Cumulative diff and recorded boundary, tenant-isolation, codegen, SDK, server, gateway, and Bifrost evidence | PASS |

## Proposed findings

No findings. The cumulative candidate satisfies the mapped TASK-004 behavior,
the R3 correction closes the two remaining validated gaps without disturbing
the accepted caller-to-result paths, and no unrelated mechanism, check, file,
setting, or option absent both established repository practice and comparable
widely used projects is required or introduced.

## Prior-finding closure

- `FIND-TASK-004-1` through `FIND-TASK-004-18` remain **CLOSED** at their
  previously validated owners: reservation/tracker ownership, tracked blocking
  preparation, original-deadline query opening, forwarded owner-loss recovery,
  tenant separation, exact tool schemas and negative paths, pinned/captured
  authority, complete idempotency/gateway/lifecycle/graph journeys, aligned
  design/security authority, Revision 13 provider-tagged examples, ordinary
  imports, active revision metadata, and grant-stream-close documentation.
- `FIND-TASK-004-19` is **CLOSED**. Attempt one is established in the shared
  counter and run ledger before publishing `Running`; the task consumes that
  reserved first attempt, retries alone advance the count, and all interrupted
  published tasks use the existing cancelled settlement with retained timing.
- `FIND-TASK-004-20` is **CLOSED**. The four named sites use their existing
  module import blocks and bare names. No scanner, lint, allow attribute,
  repository check, setting, or option was added.

No prior finding is reopened. The deleted Oracle graph-drain polling,
supervisor idle refusal, reserved-byte poison, release acknowledgement, and
their mechanism-specific tests remain absent.

## Verification limits

Per the review instruction, no executable verification was run. Recorded
results were treated as claims and checked against current source and the
named assertions. The cumulative evidence records all seven server scenario
selectors, the focused Workflow owner and query tests, the forwarded Oracle
cancel/deadline/pod-loss journey, Rust/Python/TypeScript SDK lanes, server,
gateway and Bifrost lanes, format/lints, boundary and tenancy checks, and code
generation. R3 additionally records both exact focused Skald tests,
`test:skald`, the affected Wyrd server tests, and the Oracle journey after its
bounded source corrections.

## Overall result

**PASS**

The repository at the immutable candidate satisfies the original TASK-004
behavior under approved Revision 13, closes `FIND-TASK-004-1` through
`FIND-TASK-004-20`, and preserves every binding human decision and non-goal.
