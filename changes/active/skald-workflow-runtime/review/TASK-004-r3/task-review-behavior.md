# TASK-004 Behavior Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `f17726fb25df1fa513875dca8d92f0073340ee0a`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: `TASK-004-R1-close-accepted-job-gaps.md` and
  `TASK-004-R2-align-revision-and-source-contracts.md`
- Prior verdicts and validated ledgers: `changes/active/skald-workflow-runtime/review/TASK-004-r1/`
  and `changes/active/skald-workflow-runtime/review/TASK-004-r2/`

The candidate was `f17726fb25df1fa513875dca8d92f0073340ee0a` before and
after this review. `.codegraph/` is absent, so the review used immutable Git
objects and repository source. This was a strictly read-only source and
recorded-evidence audit: no build, test, Cargo, or mise command was run.

## Caller-to-result coverage

The cumulative range was followed through authenticated create/get/cancel,
canonical request hashing and idempotent admission, tracked preparation,
tenant-scoped graph pinning, Skald preparation and execution, gateway and
built-in tool calls, Oracle stream opening/settlement, whole-snapshot updates,
terminal compare-and-set, retention, and shutdown. Revision 13 was followed
from the adjacent-tagged `ProviderRequest` through stored Prompt bodies,
generated schemas, provider projections, examples, and the internal Vertex
journey.

The r2 implementation delta was checked separately. Its Rust edits only move
existing interface types into module imports and use their bare names. Its
remaining edits correct active authority, tagged examples, and lifecycle
documentation. It changes no runtime, wire, schema, assertion, test mechanism,
setting, option, dependency, or lifecycle protocol.

The binding human decisions were treated as authority: Oracle graph-drain
polling and supervisor idle refusal remain deleted; closing the participant
grant stream is follower release and the leader does not await a follower
acknowledgement; and foreign-tenant journeys do not execute a model step because
the fixture tenant alone has seeded gateway credentials.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Earliest graph validation, exact active registered root, no unsupported `Native` route or undeclared server tool (`REQ-014`, `REQ-015`, `REQ-029`, `AC-009`) | `components/cards/resolve.rs::PinnedWorkflowGraph::pin`; `components/workflow/host.rs::Preparation::{prepare,external_bindings,check_tools}` | Recorded S1 `admission_is_audited_and_side_effect_free_on_refusal`; no-provider-call assertions | PASS |
| Bounded execution, complete deterministic terminal snapshots, partial failure and cancellation semantics (`REQ-017`-`REQ-023`, `REQ-045`, `REQ-048`, `INV-023`, `AC-008`, `AC-019`, `AC-020`, `AC-028`) | `WorkflowRunOptions` bounds in `host.rs`; Skald `PreparedWorkflowRun`; whole-snapshot `AcceptedRun::{observe,finish}`; `RunLedger` accounting | Recorded S6/S7; focused graph, aggregate-run, terminal-reserve, deep-graph, cancellation, deadline, and sibling-service cases | PASS |
| Accepted HTTP resource uses 202/200 replay, fresh authenticated/audited create/get/cancel, indistinguishable scoped lookup, and no run on preparation failure (`REQ-030`, `REQ-032`, `REQ-033`, `AC-004`, `AC-010`) | `components/workflow/{routes,host}.rs`; `audit::authorize` precedes `WorkflowRuns` lookup/admission | Recorded S1 plus principals and served-route evidence | PASS |
| One preparation and run per tenant/principal/key, exact conflict/replay behavior, disconnect survival, no cached failure, and shutdown wakeup (`REQ-034C`, `INV-018`, `AC-021`) | `WorkflowRuns::admit`; tracker token on `Reservation`; `Reservation::{accept,fail,drop}`; tracked `Preparation::run` | Recorded S2 `tracked_preparation_replay_and_disconnect`; owner races `drain_waits_for_a_reservation_before_its_task_exists` and `drain_waits_for_blocking_work_its_caller_abandoned` | PASS |
| Accepted work captures token-free authority and pinned graph without later widening, while new HTTP requests authorize afresh and gateway governance remains live (`REQ-032A`, `INV-005`, `INV-022`, `AC-027`) | `Preparation` owns the verified `Caller` and `PinnedWorkflowGraph`; replay returns stored state; `ServerWyrdGatewayCaller` enters ordinary gateway admission with `authorized = false` | Recorded S3 covers token expiry, role revocation/restoration, replay non-widening, root deletion, and credential revocation before a later call | PASS |
| Built-in `bifrost.query` and `cards.get` preserve per-Agent declarations, captured identity, object authorization/audit, closed arguments/results, and redacted non-retryable failures (`REQ-052`, `INV-006`, `INV-008`, `AC-025`) | `components/workflow/tools.rs::{RunTools,QueryTool,CardsTool}`; authorized `get_card_by_ref_for`; shared `BoundedQuery` | Recorded S4 covers real tool loop, permission/object/tenant denials, numeric and payload bounds, malformed/partial terminals, and cancellation | PASS |
| Query owner survives waiter/step abort and settles under the original query deadline before Workflow terminalization, without inventing another query engine (`INV-019`, `INV-021`, Scenario 4/6 ordering) | `QueryTool::invoke` spawns on the run owner tracker; `BoundedQuery::run`; `RunningQueryControls::{open_cancellable,cancel_and_settle}`; `tools.drain()` precedes `AcceptedRun::finish` | Recorded absolute-deadline unit proof and forwarded cancel/deadline/pod-loss journey, including later serviceability | PASS |
| WyrdGateway and ExtGateway retain route, dialect, fallback, remaining-deadline, cancellation, credential, origin, and direct-egress ownership (`REQ-034`, `REQ-036A`, `REQ-038`-`REQ-043`, `INV-010`-`INV-012`, `INV-020`, `AC-011A`, `AC-014`-`AC-017`) | `components/gateway/workflow.rs`; tenant-qualified binding selection in `Preparation::prepare`; shared endpoint policy below the adapters | Recorded S5 covers OpenAI Chat/Responses, Anthropic, Gemini, internal Vertex, incompatible capability refusal, fallback isolation, deadline/cancel, and direct external binding | PASS |
| Process-local lifecycle stays bounded: first terminal wins, no active eviction, fixed retention, tenant/global oldest-terminal eviction, restart loss, and shared-budget shutdown (`REQ-034A`, `REQ-034B`, `INV-013`, `AC-018`, `AC-022`) | `RunTable::sweep`; `AcceptedRun::finish`; `WorkflowRuns::drain`; `BoundServer` shutdown ordering | Recorded S6 covers completion races, whole-snapshot reads, per-tenant/global eviction, queued/running shutdown, restart loss, and retained terminal inspection | PASS |
| Real tenant isolation crosses Workflow root, run, Card, and Bifrost boundaries without depending on a foreign model call (`INV-006`, `AC-009`, `AC-018`, `AC-025`) | Credential-derived second-tenant principals in `pg_workflow_runs.rs`; tenant-qualified run keys; RLS-backed Cards/Bifrost owners | Recorded S1/S2/S4/S6 foreign-tenant cases; the no-foreign-model limitation is the approved harness boundary | PASS |
| Revision 13 adjacent provider tagging rejects mismatched bodies, preserves `RawV1`, updates examples/schemas/SDK authoring, and sends stored Vertex through its internal projection | `skald-spec/src/request.rs`; generated Prompt-bearing schemas; tagged fixtures/examples; `gateway/workflow.rs` provider projections | Recorded round-trip/mismatch/RawV1 tests, internal Vertex S5 case, `test:skald`, SDK lanes, and `codegen:check` | PASS |
| R2 closes canonical examples, active revision authority, bare-interface source shape, and follower-release documentation without behavior or bespoke machinery (`FIND-TASK-004-15`-`18`) | Tagged snippets at `spec.md:531-598`; Revision 13 task/remediation metadata; import-only Rust delta; `AnalyticalGraphLifecycle` docs at `analytical.rs:2492-2506` | Direct source comparison plus recorded fmt/lints, `test:wyrd`, Oracle journey, and codegen results | PASS |
| Non-goals remain excluded: no durable run persistence/recovery/lease, Workflow principal, bearer retention, arbitrary server tools, second audit/query engine, compatibility reader, remote Python/TypeScript/MCP lifecycle surface, release-ack protocol, new check, or new third-party dependency | Process-local `WorkflowRuns`; two fixed tools; shared Cards/gateway/query owners; tagged-only request enum; grant-stream-close release | Cumulative diff and recorded boundary, tenant-isolation, unwrap, codegen, language, server, gateway, and Bifrost evidence | PASS |

## Proposed findings

No findings. The reviewed behavior satisfies the mapped TASK-004 obligations,
and the r2 remediation introduces no behavioral regression or unsupported
mechanism. In particular, no mechanism, check, file, setting, or option was
found that is absent both repository precedent and comparable established
practice and is required for acceptance.

## Prior-finding closure

- `FIND-TASK-004-1` through `FIND-TASK-004-14` remain **CLOSED**. Their
  corrections remain at the previously validated owners: reservation/tracker
  ownership, tracked blocking preparation, original-deadline query opening,
  pod-loss recovery, real tenant separation, exact tool declarations and
  schemas, captured/pinned authority, expanded idempotency/tool/gateway/
  lifecycle/graph journeys, and synchronized design/security authority.
- `FIND-TASK-004-15` is **CLOSED**: all three canonical Prompt examples use
  `request.provider: open_ai_chat_completion` and `request.body`.
- `FIND-TASK-004-16` is **CLOSED**: every cited changed interface now uses a
  module-imported bare type; `TokioInstant` is the sole real collision alias.
- `FIND-TASK-004-17` is **CLOSED**: active TASK-004 and R1 remediation metadata
  identify Revision 13, while the R1 historical immutable input remains
  accurately Revision 12.
- `FIND-TASK-004-18` is **CLOSED**: the lifecycle owner now documents held
  grant streams, stream-close release, asynchronous follower settlement, and
  reservation/admission-only transport use, with no release acknowledgement.

No prior finding is reopened. The deleted Oracle graph-drain polling,
supervisor idle refusal, and reserved-byte poison remain absent.

## Verification limits

Per the review instruction, no executable verification was run. This review
relies on the cumulative diff, current candidate source, assertions in the
named tests, and recorded results. The recorded evidence includes all seven
server scenario selectors; focused Workflow owner/query tests; forwarded
Oracle cancel/deadline/pod-loss coverage; `test:wyrd`, `test:shared`, principals,
gateway, and all Bifrost lanes; Rust/Python/TypeScript SDK lanes; format/lints;
boundary and tenant checks; and code generation. R2 additionally records
`test:wyrd` (683 passed), the Oracle journey lane (43 passed), fmt, lints, and
`codegen:check` after its behavior-neutral source corrections.

## Overall result

**PASS**

The complete base-to-candidate repository satisfies the original TASK-004
behavior under approved Revision 13, closes all prior validated findings, and
preserves every binding human decision and non-goal.
