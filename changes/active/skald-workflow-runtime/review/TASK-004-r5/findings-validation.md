# TASK-004 r5 structured Ponytail validation

## Immutable subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `d4d4e2da53abfc677abdb804e71517c3b6849f49`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1 through R4 in their preceding review directories

The candidate resolved to the requested commit before this report was written.
`.codegraph/` is absent, so validation used the immutable cumulative Git diff,
repository search, and direct source inspection. Per the review direction, no
build, test, Cargo, mise, formatter, linter, code-generation, or package-manager
command was run.

I read and validated every proposal in:

- `task-review-behavior.md`;
- `task-review-invariants.md`;
- `standards-review.md`;
- `maintainer-review.md`;
- `system-review.md`;
- `domain-review-concurrency-lifecycle.md`;
- `domain-review-security-tenancy.md`;
- `domain-review-query-settlement.md`;
- `domain-review-provider-contract.md`; and
- `followup-review.md`.

The applicable authority included `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/references/languages/spec-driven-development.md`,
`architecture/references/languages/maintainer-style.md`, the approved spec,
the original task, and all four remediation tasks. The standing human decisions
were treated as authority and not reopened.

## Proposed-finding validation

| Discovery proposal | Validation | Source-backed disposition |
|---|---|---|
| `BEHAVIOR-R5-001` | R4 expressly requires positive proof that omitted and longer query deadlines clip to the prepared-run boundary while a shorter one remains shorter. `tools_use_the_prepared_run_deadline` proves only shared instant identity; the forwarded journey omits `deadline_ms`; the server journey checks only zero rejection. | **REVISED** and deduplicated with `INV-R5-001` as `FIND-TASK-004-22`. The diagnosis is confirmed; the correction is narrowed from an unresolved choice of test seams to the existing forwarded-query journey that can observe the actual `QueryTool` outcome. |
| `INV-R5-001` | The same source trace confirms that `QueryTool::invoke` implements `requested.min(remaining)`, but no cited assertion drives either positive explicit branch. Package-wide success cannot substitute for the focused proof R4 required. | **REVISED** and deduplicated as `FIND-TASK-004-22`. No production change, helper, new seam, or new harness is justified. |
| `DOMAIN-PROVIDER-1` | `OpenAiChatCompatible` owns an `OpenAiChatRequest`, is accepted by the Agent OpenAI Chat loop, survives Prompt persistence/hydration, and is dispatched under its embedded custom provider. `protocol_matches` rejects it for `OpenAiChat`, and `ExternalGatewayClient::send` would reject it again if validation were bypassed. | **CONFIRMED** and retained as `FIND-TASK-004-23`. |
| `FOLLOWUP-PROVIDER-1` | The follow-up correctly resolves reachability and scope. Revision 13 `REQ-038`/`REQ-039` bind an external route to request dialect, not to the built-in provider identity, and the only `openai_chat` protocol represents the same body spoken by the compatible variant. | **CONFIRMED**, deduplicated into `FIND-TASK-004-23`. |

The other discovery reports proposed no material findings. Their PASS results
were not treated as proof of the two proposals above; both were independently
traced through current source and their sibling consumers.

## Producer-to-consumer validation

### Prepared-run deadline projection

1. `WorkflowExecutor::new` fixes the one total absolute deadline at
   `crates/skald/skald-workflow/src/workflow.rs:150-197`.
2. `PreparedWorkflowRun::deadline` exposes that stored instant without
   resampling at
   `crates/skald/skald-workflow/src/workflow_surface.rs:672-698`.
3. Server preparation creates and clones `RunTools` during graph hydration,
   prepares the Skald run, then binds that exact instant before returning the
   accepted run at
   `crates/wyrd/wyrd-server/src/components/workflow/host.rs:374-400`.
4. Every tool clone shares the same `Arc<OnceLock<Instant>>`; `QueryTool::invoke`
   converts its remaining time into the query's relative deadline and applies
   `requested.min(remaining)` at
   `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:43-95,188-223`.
5. `BoundedQuery::run` projects that value into both the public query request
   and `RunningQueryControls::open_cancellable` at
   `crates/wyrd/wyrd-server/src/query/collect.rs:250-323`, so the positive
   explicit value is observable through the existing admitted Oracle journey.
6. The new paused-time test at
   `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:426-471` stops after
   asserting clone/deadline equality. The forwarded journey's tool call at
   `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:134-148` supplies
   only `sql`; its scripted continuation establishes the omitted-deadline
   regression boundary but not shorter or longer precedence. The real-server
   tool journey supplies only `deadline_ms: 0` at
   `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:1942-1993`.

The production source therefore closes the earlier double-sampling defect, but
the candidate cannot satisfy R4 lines 105-125: the remediation explicitly made
the positive shorter case focused acceptance evidence and also named the
omitted/longer/shorter matrix as a closure criterion.

### OpenAI-compatible external route

1. `ProviderRequest::OpenAiChatCompatible` contains an
   `OpenAiChatRequest` and its custom dispatch identity at
   `crates/skald/skald-spec/src/request.rs:18-37,90-103`.
2. Native Prompt construction and checked-in Prompt fixtures create that
   variant; for example
   `tests/fixtures/workflow-loading/team/prompts/correctness.yaml:1-23`.
   Server graph pinning and `SkaldWorkflow::from_card_bodies` preserve the
   stored discriminant before `Workflow::prepare` at
   `crates/wyrd/wyrd-server/src/components/workflow/host.rs:309-400`.
3. The Agent request builder classifies both ordinary and compatible variants
   as the same `PromptLoopSupport::OpenAiChat`, extracts the same message type,
   and preserves the compatible wrapper across iterations at
   `crates/skald/skald-agent/src/request_builder.rs:14-51,81-120,222-241`.
4. `ResolvedGraph::resolve` checks every `ExtGateway` route through
   `protocol_matches` at
   `crates/skald/skald-workflow/src/plan.rs:53-93`. The OpenAI Chat arm at
   `crates/skald/skald-workflow/src/route.rs:430-453` accepts only
   `OpenAiChatCompletion`, so the compatible variant is rejected before
   dispatch even though its body dialect matches.
5. If that guard alone were changed, the attempt registry would register the
   existing external adapter under the Prompt's embedded custom provider and
   forward the unchanged typed request at
   `crates/skald/skald-workflow/src/route.rs:395-427,610-646`.
   `ExternalGatewayClient::send` then rejects it again because its existing
   `/chat/completions` arm at
   `crates/skald/skald-providers/src/clients/external.rs:95-139` also accepts
   only `OpenAiChatCompletion`.
6. The sibling in-process gateway already combines both variants into one
   OpenAI Chat projection and response shape at
   `crates/wyrd/wyrd-server/src/components/gateway/workflow.rs:182-205`.
   The Agent response consumers likewise already consume the ordinary
   `ProviderResponse::OpenAiChatCompletion` shape. No translation or response
   variant is missing.

This path is reachable and required by Revision 13 `REQ-038`/`REQ-039` and the
TASK-004 Scenario 5 server composition. The fact that the two shared rejection
sites predate the reviewed base does not remove the obligation: the cumulative
candidate adds the accepted server host that must execute stored external
routes and expressly carries those requirements.

## Ponytail ladder and correction boundaries

### Deadline proof

1. **Delete:** the production R4 correction cannot be deleted; it removes the
   validated two-sampler defect. The missing proof also cannot be waived
   because R4 made it an explicit acceptance obligation.
2. **Existing repository behavior:** the implementation and the current
   forwarded Oracle journey already supply the necessary path. Reuse them.
3. **Native platform:** no additional clock, timer, channel, hook, or test seam
   is needed.
4. **Installed dependency:** no dependency is needed.
5. **Minimum correction:** extend the existing forwarded-query journey's
   arguments and assertions to distinguish omitted, longer, and shorter
   positive deadlines through the actual `QueryTool`/Oracle path. Do not alter
   production behavior merely to expose an internal value.

### Compatible external route

1. **Delete:** neither the compatible Prompt variant nor external OpenAI Chat
   routing can be deleted; both are approved and have real callers.
2. **Existing repository behavior:** reuse the existing `OpenAiChat` matcher,
   external client's `/chat/completions` post, OpenAI Chat response variant,
   server external-route journey, and custom provider dispatch.
3. **Native platform:** ordinary enum alternatives in the two existing match
   arms are sufficient.
4. **Installed dependency:** no dependency is needed.
5. **Minimum correction:** accept the compatible variant in both existing
   OpenAI Chat dispatch points and send its existing inner request unchanged.

Adding a protocol variant, translation layer, provider registry, compatibility
reader, opt-in, setting, option, checker, dependency, fixture system, or test
harness would be DRIFT under the standing direction and is explicitly excluded.
Both retained corrections follow established typed-enum dispatch and existing
journey patterns used by Wyrd and comparable Rust services.

## Prior-finding closure

`FIND-TASK-004-1` through `FIND-TASK-004-21` remain source-closed. The new
proof finding does not reopen the production diagnosis assigned to
`FIND-TASK-004-21`; it records that R4's independently required closure proof
was not delivered.

| Prior IDs | Current closure seam | Status |
|---|---|---|
| `FIND-TASK-004-1`, `-2` | Reservation-time tracker ownership and tracked blocking preparation in the accepted-run owner | **CLOSED** |
| `FIND-TASK-004-3`, `-4` | Original query-deadline cancellation during open and forwarded pod-loss settlement/recovery | **CLOSED** |
| `FIND-TASK-004-5`, `-8`, `-14` | Authenticated second-tenant boundaries, captured/pinned authority, and aligned design/security authority | **CLOSED** |
| `FIND-TASK-004-6`, `-7`, `-10` | Built-in tool rustdoc, exact schemas/bounds, and negative query/Card/terminal journeys | **CLOSED** |
| `FIND-TASK-004-9`, `-11`, `-12`, `-13` | Idempotency, gateway, lifecycle, graph/snapshot, and sibling-service journeys | **CLOSED** |
| `FIND-TASK-004-15` | Adjacent `provider`/`body` Prompt examples, fixtures, and generated projections | **CLOSED** |
| `FIND-TASK-004-16`, `-20` | Required module-scope imports and bare interface types, with no extra enforcement mechanism | **CLOSED** |
| `FIND-TASK-004-17` | Active task/remediation metadata identifies approved Revision 13 | **CLOSED** |
| `FIND-TASK-004-18` | Follower release remains grant-stream close; no acknowledgement protocol or deleted polling mechanism returned | **CLOSED** |
| `FIND-TASK-004-19` | Attempt one is reserved before `Running`; interrupted published work settles `Cancelled` with timestamps | **CLOSED** |
| `FIND-TASK-004-21` | `PreparedWorkflowRun` owns the one deadline, bound once into every `RunTools` clone before acceptance; the former early sampler is absent | **CLOSED in production source**; R4 proof noncompliance is the distinct `FIND-TASK-004-22` |

The fixed human decisions remain intact: Oracle graph-drain polling and
supervisor idle refusal stay deleted; follower release is stream close and the
leader awaits no release acknowledgement; the foreign-tenant fixture is not
required to execute a model step without seeded credentials; published
`Running` reserves attempt one and interrupted work settles `Cancelled`; and
built-in query tools consume the prepared run's one-time-bound deadline.

## Final deduplicated finding ledger

### FIND-TASK-004-22 — REVISED — MISSING: R4 omits its required positive explicit-deadline proof

- **Discovery sources:** `BEHAVIOR-R5-001`, `INV-R5-001`.
- **Violated obligation:** TASK-004 R4 acceptance criteria and focused-proof
  section require proof that an omitted and explicit longer query deadline are
  clipped to the prepared-run boundary while an explicit shorter positive
  deadline remains shorter.
- **Exact locations:**
  `changes/active/skald-workflow-runtime/review/TASK-004-r4/TASK-004-R4-share-prepared-run-deadline.md:105-125`;
  `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:426-471`;
  `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:134-225`; and
  `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:1942-1993`.
- **Evidence:** the implementation contains the correct
  `requested.min(remaining)` projection, but the focused owner test never
  invokes `QueryTool`; the forwarded journey omits `deadline_ms`; and the
  server journey's only explicit value is zero, which is rejected before a
  query opens. No cited assertion can fail if a later change always overwrites
  a positive shorter request with the run remainder or fails to clip a longer
  one.
- **Observable consequence:** the task's R4 completion claim is not supported
  by its mandatory focused evidence. A regression in positive deadline
  precedence can pass every recorded R4 assertion and let a query exceed its
  caller-declared shorter bound or escape the run-bound clipping rule.
- **Decision-complete minimum correction:** keep production source unchanged
  unless the proof exposes a defect. Extend the existing
  `workflow_forwarded_query_settles_before_the_run_ends` journey, using its
  existing admitted Workflow, forwarded Oracle, held query, scripted post-tool
  continuation, and lifecycle metrics, to drive all three actual tool inputs:
  omitted, positive longer than the run remainder, and positive shorter than
  the run remainder. The omitted and longer cases must reach the shared run
  boundary and remain `TimedOut` with no successful continuation. The shorter
  case must reach its own query timeout first, deliver the existing redacted
  tool failure to the scripted continuation, and let that continuation finish
  before the still-live run boundary. Retain the current prepared-instant
  equality assertion and settlement/recovery checks. Add no helper-only
  substitute, new harness, pause, clock owner, setting, option, or checker.
- **Focused closure proof:** the existing exact Oracle journey selector must
  demonstrate those three distinguishable outcomes through `QueryTool`, plus
  the existing owner test must continue to prove every clone contains
  `prepared.deadline()` after nonzero pre-preparation time.

### FIND-TASK-004-23 — CONFIRMED — INCORRECT: the OpenAI-compatible dialect is rejected by the external OpenAI Chat route

- **Discovery sources:** `DOMAIN-PROVIDER-1`, `FOLLOWUP-PROVIDER-1`.
- **Violated obligation:** Revision 13 `REQ-038` and `REQ-039`, mapped directly
  to TASK-004, require `ExtGateway` to dispatch the Prompt's matching native
  request dialect directly in local and server execution. The compatible
  variant is an OpenAI Chat request dialect, not a separate protocol.
- **Exact locations:**
  `crates/skald/skald-workflow/src/route.rs:430-453` and
  `crates/skald/skald-providers/src/clients/external.rs:95-139`.
- **Evidence:** the public/stored `OpenAiChatCompatible` producer is accepted by
  the Agent's OpenAI Chat loop and reaches resolved route validation unchanged.
  `protocol_matches` accepts only `OpenAiChatCompletion` for `OpenAiChat`, so
  preparation refuses it. If that guard alone were changed,
  `ExternalGatewayClient::send` would still return `VariantMismatch` rather
  than post the inner `OpenAiChatRequest`. The sibling Wyrd gateway already
  proves both variants use the same request and response dialect.
- **Observable consequence:** a valid registered custom OpenAI-compatible
  Prompt cannot execute through the only external protocol matching its native
  body. Server preparation fails before upstream dispatch; a guard-only fix
  would move the same failure downstream into execution.
- **Decision-complete minimum correction:** extend the existing OpenAI Chat arm
  in both `protocol_matches` and `ExternalGatewayClient::send` to accept
  `OpenAiChatCompatible { request, .. }`. Post that existing inner request
  unchanged through the existing `chat/completions` path and decode the
  existing `ProviderResponse::OpenAiChatCompletion`; retain the embedded custom
  provider solely as the existing registry dispatch identity. Do not add a
  protocol, conversion, response type, adapter, registry, setting, compatibility
  path, dependency, or downstream guard.
- **Focused closure proof:** extend the existing
  `server_routes_keep_gateway_and_external_ownership` real-server journey with
  one registered custom OpenAI-compatible Prompt using its stored
  `ExtGateway(protocol = openai_chat)` route. Assert successful pre-dispatch
  validation, exactly one direct request to the existing
  `/chat/completions` path with the compatible Prompt's native body, successful
  consumption of the existing OpenAI Chat response, and no Wyrd-gateway
  ingress. This one existing journey crosses both corrected dispatch points;
  the current mismatch and egress-security assertions remain intact.

## Validated ledger summary

| Finding | Status | Classification |
|---|---|---|
| `FIND-TASK-004-1` through `FIND-TASK-004-21` | **CLOSED** | Prior remediation findings |
| `FIND-TASK-004-22` | **REVISED** | MISSING |
| `FIND-TASK-004-23` | **CONFIRMED** | INCORRECT |

No retained correction requires a new product, public API, architecture,
security, compatibility, cross-service, concurrency-semantics,
resource-ownership, or persistent-data decision. Both are bounded within the
approved behavior and reuse current owners and proof surfaces. The validated
ledger is non-empty.

**COMPLETE — two retained findings: `FIND-TASK-004-22` and
`FIND-TASK-004-23`.**
