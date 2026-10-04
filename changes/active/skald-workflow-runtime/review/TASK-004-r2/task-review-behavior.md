# TASK-004 Behavior Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `e86831e5ac784028f8022cc3faeee1c22b12c665`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Prior verdict and validated ledger: `changes/active/skald-workflow-runtime/review/TASK-004-r1/{verdict.md,findings-validation.md}`
- Remediation task: `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md`

The candidate remained at the stated commit throughout this review. This was a
strictly read-only source and recorded-evidence audit: no build, test, Cargo, or
mise command was run.

## Caller-to-result coverage

The cumulative diff was followed through the public create/get/cancel routes,
`WorkflowRunHost`, the `WorkflowRuns` reservation/run table, pinned Cards graph
resolution, Skald preparation/execution, the in-process gateway adapter, both
built-in read tools, Oracle query settlement, terminal snapshots, retention,
and shutdown. Revision 13 was also followed from `ProviderRequest` serde through
Prompt fixtures/builders, generated schemas, provider send sites, and the
server-internal Vertex journey.

The review accepted the round's standing decisions as authority: Oracle graph
drain polling and supervisor idle refusal remain deleted; closing the follower
grant stream is follower release and the leader does not await an acknowledgement;
and foreign-tenant journeys do not execute model steps because the harness
seeds gateway credentials only for the fixture tenant.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Authenticated/audited create, get, and cancel; exact active registered graph; refusal before side effects (`REQ-029`, `REQ-030`, `REQ-032`, `REQ-033`, `AC-004`, `AC-009`, `AC-010`) | `components/workflow/{routes,host}.rs`; `PinnedWorkflowGraph::pin` in `components/cards/resolve.rs`; typed permission/role additions | Recorded S1 `admission_is_audited_and_side_effect_free_on_refusal`; principals unit/integration and `test:wyrd` PASS | PASS |
| One tenant/principal/key-scoped preparation and accepted run; conflict, waiter, disconnect, failure retry, lost-response, and shutdown behavior (`REQ-034C`, `AC-021`) | `WorkflowRuns::admit`, `Reservation::{accept,fail,drop}`, tracked preparation in `WorkflowRunHost::create` | Recorded S2 `tracked_preparation_replay_and_disconnect`; owner tests `drain_waits_for_a_reservation_before_its_task_exists` and `drain_waits_for_blocking_work_its_caller_abandoned` PASS | PASS |
| Accepted execution retains pinned graph and captured authority without bearer retention or later widening, while gateway governance remains live (`REQ-032A`, `INV-022`, `AC-027`) | `Preparation` owns a cloned verified `Caller` without token material; pinned bodies are retained in `PinnedWorkflowGraph`; `ServerWyrdGatewayCaller` uses current gateway admission | Recorded S3 `accepted_authority_outlives_submission_only` PASS | PASS |
| Built-in tools use captured authority, canonical Cards/query owners, per-call audit, bounded complete results, and settlement before Workflow terminalization (`REQ-052`, `INV-008`, `INV-021`, `AC-020`, `AC-025`) | `RunTools`, `QueryTool`, `CardsTool`; shared `BoundedQuery`; `RunningQueryControls::{open_cancellable,cancel_and_settle}`; `tools.drain()` precedes `AcceptedRun::finish` | Recorded S4 `declared_tools_use_captured_scopes_and_owned_services`, focused collector tests, forwarded Oracle journey, and Bifrost lanes PASS | PASS |
| Original absolute query deadline bounds cancel-during-open and owner-loss/pod-loss recovery remains serviceable (`FIND-TASK-004-3`, `FIND-TASK-004-4`) | `RunningQueryControls::open_cancellable` fixes one deadline before open; peer journey waits for observable membership recovery | Recorded `cancel_while_opening_ends_at_the_original_deadline` and `workflow_forwarded_query_settles_before_the_run_ends` PASS | PASS |
| Server WyrdGateway and ExtGateway preserve ownership, dialect, fallback, deadline, cancellation, and credential boundaries (`REQ-034`, `REQ-038`-`REQ-043`, `INV-010`-`INV-012`, `INV-020`, `AC-011A`, `AC-014`-`AC-017`) | `components/gateway/workflow.rs` projects OpenAI Chat/Responses, Anthropic, Gemini, and Vertex without recursive HTTP; ExtGateway bindings remain tenant-qualified | Recorded S5 `server_routes_keep_gateway_and_external_ownership` and gateway journey PASS | PASS |
| Complete terminal snapshots, first-terminal-wins races, active-slot release, fixed retention, tenant/global eviction, restart loss, and shutdown drain (`REQ-018`-`REQ-023`, `REQ-034A`-`REQ-034C`, `INV-013`, `INV-018`, `INV-019`, `AC-008`, `AC-018`, `AC-022`) | `PreparedWorkflowRun::execute`, `AcceptedRun::{observe,finish}`, `RunTable::sweep`, and `WorkflowRuns::drain` | Recorded S6 `lifecycle_races_retention_and_shutdown` and owner tests PASS | PASS |
| Step/edge/resolved-byte/input/step-result/aggregate-run bounds, stack-safe deep graph, terminal reserve, and sibling serviceability (`REQ-017`, `REQ-023`, `INV-023`, `AC-019`, `AC-028`) | Incremental charging in `PinnedWorkflowGraph::pin`; tracked blocking Skald preparation; `RunLedger` terminal reserve and aggregate accounting | Recorded S7 `graph_and_snapshot_limits_preserve_sibling_services` PASS | PASS |
| Real second-tenant boundary across Workflow/run/Card/Bifrost paths (`INV-006`, `AC-009`, `AC-018`, `AC-025`) | Credential-derived `foreign_admin`/`foreign_runner`; tenant-qualified run lookup; RLS-backed Card and Bifrost owners | Recorded S1/S2/S4/S6 second-tenant cases PASS; model-step limitation is the approved harness constraint | PASS |
| Prior `FIND-TASK-004-1` through `FIND-TASK-004-14` are closed without restoring rejected Oracle machinery | Current source contains tracked reservation/blocking ownership, deadline-bounded open cancellation, exact tool schemas/rustdoc, expanded journeys, and aligned design/security authority; deleted drain polling and idle refusal remain absent | Remediation evidence records every finding PASS and all listed focused/broader lanes green | PASS |
| Revision 13 uses adjacent provider tagging, rejects structurally invalid tagged bodies, preserves `RawV1`, updates generated schemas/fixtures/SDK authoring, and sends stored Vertex through the in-process Vertex projection | `skald-spec/src/request.rs:24-59`; regenerated `wyrd-spec` schemas; tagged repository fixtures; `components/gateway/workflow.rs:193-249` | Recorded `provider_requests_roundtrip`, `vertex_request_reads_back_as_vertex`, `raw_v1_request_reads_back_when_body_precedes_tag`, `mismatched_body_is_refused`, S5 Vertex case, `test:skald`, and `codegen:check` PASS | PASS |
| Revision 13 requires every serialized example to use the tagged `ProviderRequest` form | The checked-in example bundle is tagged, but the three normative Prompt examples in `spec.md:519-599` still put `model`/`messages` directly under `request` and omit both `provider` and `body` | No recorded proof parses the Markdown examples; source inspection shows they no longer match the derived adjacent-tag contract | **FAIL** (`BEH-004-R2-001`) |
| Non-goals remain excluded: no durable queue/recovery/lease, Workflow principal, bearer renewal, arbitrary server tool registration, second audit/query engine, compatibility reader, new check, or new third-party dependency | Process-local `WorkflowRuns`; two fixed built-ins; shared Cards/gateway/query owners; removed untagged deserializer; `jsonschema` was already a workspace dependency and is dev-only here | Cumulative diff and recorded boundary/codegen/tenant-isolation evidence | PASS |

## Proposed findings

### BEH-004-R2-001 — MISSING: Revision 13 leaves three normative Prompt examples in the removed untagged form

- **Violated obligation:** Revision 13 states that there is no compatibility
  reader and that every fixture, example, SDK authoring path, generated schema,
  and stub moves to `{"provider": ..., "body": ...}`. Serialized Card examples
  must match the current Prompt schema.
- **Exact location:** `changes/active/skald-workflow-runtime/spec.md:519-599`,
  especially the `request` mappings beginning at lines 531, 555, and 579.
- **Evidence:** `ProviderRequest` now derives adjacent-tagged deserialization at
  `crates/skald/skald-spec/src/request.rs:24-27`. Each cited example instead has
  `request: {model, messages}` with no variant tag or content key. The actual
  checked-in code-review Prompt examples already show the required minimal
  shape at `examples/workflows/code-review/prompts/{security,correctness,final-reviewer}.yaml`.
- **Observable consequence:** a user copying any of the three approved-spec
  examples receives a Prompt deserialization/load error before registration or
  execution. The active specification therefore documents a form the candidate
  deliberately no longer accepts.
- **Required testable correction:** update only those three examples to the
  existing tagged OpenAI Chat shape: `request.provider:
  open_ai_chat_completion` and the unchanged provider-native request under
  `request.body`; revise the immediately preceding “native bodies are unchanged”
  sentence only as needed to make clear that the native body is unchanged
  *inside* the tagged envelope. Reuse the checked-in example bundle; add no
  compatibility reader, parser, option, repository check, or new harness.
  Closure is direct source review that all three snippets match the checked-in
  examples, with the already-recorded example/loader and codegen lanes as
  broader proof.

## Prior-finding closure

Source inspection and the recorded remediation evidence support closure of
`FIND-TASK-004-1` through `FIND-TASK-004-14`. No prior stable finding is
reopened. `BEH-004-R2-001` is new and limited to the Revision 13 serialized
examples; it does not challenge the approved follower-release, deleted Oracle
polling/idle-refusal, or foreign-tenant harness decisions.

## Verification limits

Per the review instruction, no command that builds or executes code was run.
The assessment relies on the cumulative diff, current source at the immutable
candidate, and the command/results recorded in the TASK-004 remediation
evidence. Those results are broad and include the seven server journeys,
focused Workflow/Oracle tests, all Bifrost lanes, Rust/Python/TypeScript SDK
lanes, format/lints, boundary checks, and code generation. The one retained
finding is source-visible and is not dependent on rerunning those lanes.

## Overall result

**FAIL**

The runtime and prior remediation obligations are supported by source and the
recorded evidence, but Revision 13 is not complete while its own three
normative Prompt examples remain in the removed untagged wire form.
