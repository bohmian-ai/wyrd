# TASK-004 behavior review — r6

## Immutable subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5f3b521b5005c26277d53e7dfd2458c4f740e8be`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediation inputs: TASK-004 R1 through R5 in their named review directories
- Review mode: cumulative source/diff and recorded evidence only. No build, test,
  formatter, linter, code-generation, package-manager, or source-edit command was
  run.

`.codegraph/` is absent, so navigation used Git and direct source inspection. The
candidate resolved to the requested commit before this report was written.

The fixed human decisions were treated as authority: deleted Oracle graph-drain
polling and supervisor idle refusal were not reopened; follower release remains
grant-stream close without a leader acknowledgement; the foreign-tenant harness
was not expanded; published `Running` reserves attempt one and interruption
settles it as `Cancelled`; tools receive the prepared run deadline through the
one-time bind; and Revision 14's single-schema request model controls over the
older Revision 13 remediation wording.

## Navigation and caller-to-result coverage

The cumulative range was traced through these behavior owners and proofs:

- accepted-run admission, preparation, execution, cancellation, retention, and
  shutdown in `components/workflow/{host,routes,runs,tools}.rs`, server
  configuration/state/boot, and `app/server.rs`;
- exact graph pinning and authorized Card reads in
  `components/cards/{resolve,routes}.rs`;
- in-process governed gateway projection in
  `components/gateway/workflow.rs`, including native dialect decoding;
- bounded query ownership in `query/collect.rs`,
  `oracle/lifecycle_controls.rs`, and the Bifrost Analytical lifecycle;
- Skald preparation/execution and route dispatch in
  `skald-workflow/{workflow,workflow_surface,run,route,bodies}.rs`;
- Revision 14 producers and consumers in `skald-spec::{Prompt,
  ProviderRequest,ProviderResponse}`, `skald-agent`, `skald-runtime`, provider
  clients, Prompt authoring/Python projections, Wyrd Prompt Card persistence,
  generated schemas, fixtures, and gateway callers; and
- the seven real-server scenarios in `pg_workflow_runs.rs`, the forwarded
  Oracle journey, provider/runtime tests, Python tests, and the recorded broad
  lanes.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-014/017/029-034/050; S1 admission is authenticated, audited, bounded, and side-effect free before acceptance | `workflow/routes.rs`, `host.rs`, `runs.rs`, `config.rs`; canonical `audit::authorize`; exact graph owner in `cards/resolve.rs`; typed `workflows:run` permission and builtin grants | Recorded `admission_is_audited_and_side_effect_free_on_refusal`, configuration unit coverage, principals integration, tenant-isolation and codegen lanes | PASS |
| REQ-030/034C, INV-018, AC-021; one tracked preparation and accepted run per tenant/principal/key, with disconnect/replay/shutdown closure | `WorkflowRuns` owns reservation, task registration, promotion, deduplication, waiter outcome, and capacity; preparation blocking work remains tracked | Recorded `tracked_preparation_replay_and_disconnect` plus owner-level shutdown/preparation tests from R1 | PASS |
| REQ-032A, AC-027; accepted authority outlives only the submission token and neither widens nor bypasses live resource admission | captured token-free `Caller` in `host.rs`; per-call Cards, Bifrost, and gateway owners; governing design/security authority now documents the exception | Recorded `accepted_authority_outlives_submission_only` and gateway live-refusal cases | PASS |
| REQ-052, INV-021, AC-025; both built-in reads retain object authorization, exact closed arguments, trustworthy terminal collection, and tracked settlement | `workflow/tools.rs`, `query/collect.rs`, `cards/routes.rs::get_card_by_ref_for`, `RunningQueryControls::cancel_and_settle` | Recorded real-server tool journey, collector units, MCP query journeys, and forwarded Oracle cancel/deadline/pod-loss journey | PASS |
| REQ-034/036A/038/039/041-043 and AC-011A/014-017; Wyrd and external gateway routes retain their owners, secrets, fallback, cancellation, and protocol shapes | `skald-workflow/route.rs`; `skald-providers/clients/external.rs`; `components/gateway/workflow.rs`; shared client gateway projection | Recorded `server_routes_keep_gateway_and_external_ownership`, gateway journey, shared-client transport test, and Skald route tests | PASS |
| REQ-018-023/030/034A-C/048/050, INV-018-023, AC-018/022; races, complete terminal snapshots, eviction, and shared-deadline shutdown | terminal CAS, bounded snapshots, active/retained accounting, and task drain in `runs.rs`/`host.rs`; server shutdown orders Workflow before dependent services | Recorded `lifecycle_races_retention_and_shutdown`, forwarded Oracle journey, and focused owner tests from R1-R4 | PASS |
| REQ-017/045/050, INV-023, AC-028; graph and snapshot preparation remain bounded and sibling services remain available | bounded `PinnedWorkflowGraph` preparation; step/edge/body/input/result/run accounting and terminal reserve | Recorded `graph_and_snapshot_limits_preserve_sibling_services` and focused bound tests | PASS |
| FIND-TASK-004-22 / R4-R5 deadline closure; omitted and longer tool deadlines use the prepared run boundary, while a shorter positive deadline wins | `PreparedWorkflowRun::deadline`; one-time `RunTools` bind; `QueryTool::invoke` applies `requested.min(remaining)`; `BoundedQuery` carries the result to Oracle | `workflow_forwarded_query_settles_before_the_run_ends` now runs omitted, `120000 ms`, and `10000 ms` cases: the first two time out at the run bound with no output; the shorter case returns the redacted query timeout and lets the run succeed before its bound | PASS |
| Revision 14: one `ProviderRequest` variant per wire schema; no OpenAI-compatible request variant or Vertex GenerateContent wrapper/response | `skald-spec/request.rs` has one `OpenAiChatCompletion` and one `GeminiGenerateContent`; Vertex Predict remains; deleted Vertex wrapper/module/snapshot and removed match arms | Recorded Skald tests, codegen check, Python unit/typecheck, and repository search evidence in R5 | PASS |
| Revision 14: `Prompt.provider` is the optional native destination and survives persistence/hash; native dispatch uses it, otherwise the dialect default | `skald-spec/prompt.rs::Prompt::provider`; `skald-runtime::dispatch`; Agent loop; Prompt Card hash/persistence; Python custom-provider builder | Recorded `custom_provider_prompt_round_trips_and_dispatches_to_its_client`, Prompt Card/schema checks, Python unit/integration, and TypeScript fixture journeys | PASS |
| Revision 14: Vertex is a Google GenerateContent body dispatched to Vertex; server in-process gateway selects Vertex ingress; local Wyrd client refusal is unchanged | Vertex builder/draft assigns `Prompt.provider = Vertex`; Agent dispatch uses effective Prompt provider; `VertexClient` accepts `GeminiGenerateContent`; server gateway selects Vertex by model provider; local projection retains its explicit refusal | Recorded `vertex_prompt_round_trips_and_dispatches_to_the_vertex_client`, Vertex provider path test, server route journey, gateway journey, and shared `workflow_transport` test | PASS |
| Revision 14: gateway routes accept the request schema without reviving a compatibility variant, migration, alias, option, or second adapter | external `openai_chat` matches and sends the sole `OpenAiChatCompletion`; the existing route binding chooses external upstream; server gateway projects the same body; no legacy variant remains | `server_routes_keep_gateway_and_external_ownership` registers a custom-provider Prompt, observes one direct native-body `/v1/chat/completions` call, and observes no added governed-gateway call | PASS |
| Prior FIND-TASK-004-1 through -21 remain closed without restoring prohibited Oracle machinery or changing the fixed foreign-tenant/step-attempt decisions | cumulative source preserves tracker ownership, original query deadline, typed tool schemas, exact attempts, import style, accepted authority, and stream-close release | Recorded focused and broad evidence from R1-R5; current Revision 14 delta does not change those owners except the approved provider representation | PASS |
| No durable Workflow queue/principal, client graph loader, arbitrary server tool platform, second audit/query engine, bearer retention, compatibility route, bespoke graph-drain poll, or unrelated mechanism | cumulative diff retains one process-local host, existing graph/runtime/query/gateway owners, and standard task tracking/configuration mechanisms | Source review and recorded boundary checks | PASS |
| Active task/remediation authority identifies the approved revision and contains no live instruction contradicted by stronger authority | Original TASK-004 still declares `spec_revision: 13`; R5 still declares and names Revision 13 as its approved input and retains required `OpenAiChatCompatible` implementation/acceptance text, while its appended evidence and candidate implement Revision 14 | Direct active-packet source inspection | **FAIL — BEHAVIOR-R6-001** |

## Proposed findings

### BEHAVIOR-R6-001 — VIOLATION: the active TASK-004/R5 contract still claims Revision 13 and requires behavior Revision 14 removed

- **Violated obligation:** The spec-driven authority chain requires a ready task
  and its active remediation to identify the approved specification revision
  and not prescribe behavior contradicted by that stronger authority. The user
  explicitly requested this acceptance review against approved Revision 14.
- **Exact locations:**
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md:6`;
  `changes/active/skald-workflow-runtime/review/TASK-004-r5/TASK-004-R5-close-compatible-route-and-deadline-proof-gaps.md:6,15-22,48-75,127-145`.
- **Evidence:** The original task says `spec_revision: 13`. R5 also says
  `spec_revision: 13`, names Revision 13 as the approved specification input,
  and makes restoration and proof of `ProviderRequest::OpenAiChatCompatible`
  a required outcome. Revision 14 explicitly deletes that variant and every
  path that exists only for it. The same R5 file then appends a “Revision 14
  evidence” section documenting the opposite result. The candidate correctly
  follows Revision 14, so it cannot satisfy the still-live R5 acceptance rows
  as written.
- **Observable consequence:** The active packet presents two incompatible
  acceptance contracts for the same candidate. A future implementer or review
  cannot determine from task metadata whether deleting or restoring the
  compatible variant completes TASK-004, and the original task does not record
  the revision actually being accepted in this review.
- **Required testable correction:** Route this invalid private-task instruction
  through the existing `$wyrd-plan` task-correction path under already-approved
  Revision 14. Align the active original TASK-004 and the R5 remediation's
  current authority/acceptance text with Revision 14, explicitly preserving the
  R5 deadline-proof obligation while marking the Revision 13 compatible-variant
  correction as superseded by the single OpenAI Chat schema and optional
  `Prompt.provider`. Preserve R1-R4 and prior review reports as historical
  records. Do not restore a request variant, compatibility path, migration,
  alias, option, checker, or test harness.
- **Closure proof:** Source inspection must show the active task metadata names
  Revision 14 and no active acceptance row requires
  `OpenAiChatCompatible`; the existing Revision 14 source/tests and the three
  deadline outcomes remain the implementation evidence. No runtime test or new
  repository check is required for this artifact-only correction.

## Verification assessment

The recorded command ledger in TASK-004 and R5 covers the focused server,
Oracle, Skald, shared-client, gateway, Python, TypeScript, codegen, boundary,
format, and lint surfaces. This review did not rerun those commands. Assertions
were accepted only after their current source was read: in particular, the
deadline journey now distinguishes all three R5 cases through the real tool and
forwarded Oracle path, and the server route journey crosses persistence,
hydration, route validation, external dispatch, and response consumption for a
custom-provider OpenAI Chat Prompt.

No implementation behavior finding remains from the inspected caller-to-result
paths. The one proposed finding is an active authority/task conflict, not a
request to change the conforming Revision 14 implementation.

## Overall result

**FAIL**

`BEHAVIOR-R6-001` must be independently validated. If retained, TASK-004 cannot
PASS until the active task packet unambiguously targets approved Revision 14.
