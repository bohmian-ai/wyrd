# TASK-004 invariant review

## Review findings

### Critical

None.

### Important

#### `INV-R6-001` — VIOLATION/REGRESSION: the active task authority still identifies Revision 13

- **Violated obligation:**
  `architecture/references/languages/spec-driven-development.md:142-168`
  requires every task and remediation to identify its approved specification
  revision. The approved specification expressly assigns Revision 14 to
  TASK-004 (`spec.md:21-54,2760-2764`). This is the same authority-chain
  obligation previously recorded as `FIND-TASK-004-17` when Revision 13
  superseded Revision 12.
- **Locations:**
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md:6,435`
  and
  `changes/active/skald-workflow-runtime/review/TASK-004-r5/TASK-004-R5-close-compatible-route-and-deadline-proof-gaps.md:6,169-188`.
- **Evidence and reachability:** the original task still declares
  `spec_revision: 13` and links “Approved Revision 13.” R5 also declares
  `spec_revision: 13`, but its current implementation record contains the
  Revision 14 commits and acceptance table. The immutable R5 input at
  `:15-24` correctly describes the historical Revision 13 candidate and is
  not the defect. A later implementer or reviewer following the active
  frontmatter and authority link can treat the one-variant request contract,
  optional `Prompt.provider`, shared Gemini/Vertex schema, and local Vertex
  refusal as outside TASK-004 even though the approved authority says
  TASK-004 carries them.
- **Required testable correction:** update the original TASK-004 frontmatter
  and current authority link to Revision 14. Update R5's current governing
  frontmatter to Revision 14 and, if needed for clarity, state that Revision
  14 extended its current scope after the immutable Revision 13 review input.
  Preserve the R1-R5 verdicts, discovery reports, reviewed-candidate fields,
  and clearly historical immutable-input revision statements. Verify the
  active authority chain by source inspection only. Do not add a repository
  check, compatibility path, file, setting, or option.

### Suggestions

None.

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5f3b521b5005c26277d53e7dfd2458c4f740e8be`
- Approved authority:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1 through R5 in their preceding review directories
- Method: static inspection of the complete cumulative diff, current source,
  value/state producers and sibling consumers, lifecycle and failure paths,
  prior validated ledgers, and recorded evidence. No build, test, formatter,
  code-generation, package-manager, Cargo, or mise command was run.

`.codegraph/` is absent, so immutable Git objects, `rg`, and direct source
inspection were used. The candidate resolved to
`5f3b521b5005c26277d53e7dfd2458c4f740e8be` before this report was written.

## Navigation map

| Producer or owner | State or value | Main consumers and proof seams |
|---|---|---|
| `WorkflowRunHost` / `Preparation` | authenticated caller, request hash, timeout, pinned graph, execution dependencies | `WorkflowRuns::admit`, Cards pinning, `Workflow::prepare`, `Reservation::accept`, server run journeys |
| `WorkflowRuns` / `Reservation` / `AcceptedRun` | scoped key, active capacity, preparation ownership, snapshots, cancellation, retention | create waiters and replay, GET/cancel, shutdown drain, lifecycle journeys |
| `WorkflowExecutor` / `RunLedger` | one prepared deadline, attempt counters, ready/running steps, deterministic terminal snapshot | `PreparedWorkflowRun`, transition observer, terminal commit, executor tests |
| `RunTools` / `QueryTool` | captured caller, query-owner tracker, shared one-time-bound prepared deadline | `BoundedQuery`, `RunningQueryControls`, Oracle, built-in-tool and forwarded-query journeys |
| `Prompt` / `ProviderRequest` | optional dispatch target plus one variant per wire schema | native registry lookup, Workflow routes, server gateway projection, external clients, local client refusal, Python and generated schemas |
| `ServerWyrdGatewayCaller` | provider/model identity, request schema, stored fallback, remaining timeout, cancellation, correlation | `GatewayInvocation`, provider ingress selection, live authorization/admission, gateway journeys |
| Analytical lifecycle | participant grant streams and graph settlement | follower stream-close release, running-query terminal state, forwarded-query recovery |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence reviewed | Result |
|---|---|---|---|
| REQ-057, REQ-059, AC-030, AC-031: Workflow is neither a principal nor a WyrdState root; existing graph/loading/runtime owners remain authoritative | The host captures a verified `Caller`, pins through the server Cards owner, and executes one Skald `PreparedWorkflowRun` (`components/workflow/host.rs:264-400`; `components/cards/resolve.rs`; `skald-workflow/src/workflow_surface.rs`) | Recorded client-tier, shared, Wyrd, tenant-isolation, codegen, Python, and TypeScript evidence | PASS |
| REQ-014, REQ-015, REQ-017, REQ-029, REQ-045, REQ-050, INV-001, INV-005, INV-019, INV-023, AC-004, AC-009, AC-019, AC-028: the exact active graph and input/result/run resources are bounded, pinned, validated, and prepared before acceptance | `Preparation::prepare` pins in one tenant transaction, applies suitability and graph bounds, hydrates on tracked blocking work, and binds explicit execution limits before acceptance (`host.rs:318-400`; `cards/resolve.rs`; `run.rs:56-107`) | Recorded scenarios 1, 3, and 7 and registration journeys | PASS |
| REQ-018 through REQ-023, REQ-048, INV-013, AC-008, AC-020, AC-022: one bounded executor produces deterministic complete snapshots with no terminal pending/running step | One `JoinSet` owns step work, joined outcomes settle the ledger, pending-only work becomes `Unstarted`, and tool owners drain before terminal commit (`skald-workflow/src/workflow.rs:251-385`; `run.rs:221-280`; server `host.rs:244-287`) | Recorded executor evidence and scenarios 6 and 7 | PASS |
| R3 / `FIND-TASK-004-19`: published `Running` reserves attempt one and interrupted published work settles `Cancelled` | Scheduling stores one before publishing `Running`; interrupted or cancelled joins call `step_cancelled`; only never-started pending steps are rewritten (`workflow.rs:279-305,342-380`; `run.rs:125-189`) | Recorded `prepared_run_keeps_its_id`, `bounded_attempt_lifecycle`, and pre-poll interruption proof | PASS |
| REQ-019, REQ-030, REQ-034A, REQ-034B, REQ-034C, INV-018, AC-010, AC-021: admission, idempotency, preparation, execution, retention, restart loss, and shutdown have one bounded process owner | `WorkflowRuns` installs scoped keys, slots, preparation ownership, cancellation, tracker state, accepted runs, and terminal-only bounded retention under one owner (`workflow/runs.rs`; `host.rs:255-400`) | Recorded fan-in/replay, failure, race, eviction, restart, and shutdown scenarios | PASS |
| REQ-032, REQ-033, REQ-032A, REQ-057, INV-022, AC-027: every HTTP operation authenticates/authorizes/audits afresh while accepted authority is token-free, immutable, and cannot widen | The host authorizes before admission or owned lookup; retained execution carries only `Caller`; replay cannot replace it; gateway, Card, and query owners apply their own decisions (`host.rs:75-203,230-400`; `gateway/workflow.rs`; `workflow/tools.rs`) | Recorded principal, expiry/revocation, audit, replay, and live-gateway refusal evidence | PASS |
| REQ-034, REQ-036A, REQ-038, REQ-039, REQ-041 through REQ-043, INV-009 through INV-012, INV-020, AC-011A, AC-012, AC-014 through AC-018: route, credential, fallback, timeout, cancellation, result bound, and live gateway governance remain with their established owners | Per-attempt route adapters keep immutable correlation and deadline state; server gateway calls the existing invocation owner; external routes resolve existing bindings and send only the native body (`skald-workflow/src/route.rs`; server `components/gateway/workflow.rs`) | Recorded route matrix, dependency/capability refusal, fallback, cancellation, credential/redaction, result-bound, and gateway journey evidence | PASS |
| Revision 14: one `ProviderRequest` variant per schema, `Prompt.provider` is the optional dispatch target, and content hashes include it | The enum has one OpenAI Chat and one GenerateContent schema; `Prompt::provider()` selects the optional target or dialect default; the hash projection includes `provider` (`skald-spec/src/request.rs:20-98`; `prompt.rs:28-53,142-173`; `wyrd-spec/src/card/prompt/hash.rs:24-45`) | Recorded focused custom-provider/Vertex agent journeys, Skald family, codegen, Python, and TypeScript evidence | PASS |
| Revision 14: native and Workflow dispatch consume the Prompt target, while gateway routes choose upstream from route/model identity and protocol-match only the body schema | Agent dispatch looks up `self.prompt.native().provider()`; Workflow gateway identity uses `prompt.provider()`; both external Gemini and Vertex protocols accept the shared GenerateContent schema (`skald-agent/src/loop_runtime.rs:331-335`; `skald-workflow/src/route.rs:280-308,429-480`) | Recorded custom-provider dispatch, ext-gateway, server route, and Vertex gateway proofs | PASS |
| Revision 14: Vertex Prompt is a Gemini GenerateContent body dispatched to Vertex; `VertexPredict` remains distinct | `VertexClient` accepts `GeminiGenerateContent`; the server selects Vertex ingress from model provider `vertex`; request enum retains `VertexPredict` (`skald-providers/src/clients/vertex.rs:105-130`; server `gateway/workflow.rs:219-228`; `request.rs:38-44`) | Recorded native Vertex journey, gateway journey, Wyrd family, and Skald family evidence | PASS |
| Fixed local-client decision: a local `wyrd-client` gateway still refuses GenerateContent for a Vertex model | The client projects `GeminiGenerateContent` only when the model provider is not `vertex`, then returns the existing unsupported-dialect error (`shared/wyrd-client/src/workflow/gateway.rs:230-279`) | Recorded `workflow_transport` local-Vertex refusal and shared-family evidence | PASS |
| Revision 14 Python/schema projection: removed Vertex request/response wrappers stay absent; `Prompt.vertex` remains; fixtures, stubs, and schemas use the shared variant plus target | Current wrappers expose the shared GenerateContent request shape and Prompt-level provider destination; R5 records regenerated artifacts rather than compatibility accessors (`skald-prompt/src/prompt.rs`; `sdks/wyrd-sdk-python`; generated schemas/stubs) | Recorded codegen, PyO3-scope, Python unit/type/integration, and TS unit/integration evidence | PASS |
| REQ-052, INV-006, INV-008, AC-025: only declared built-ins execute, and Cards/query reads retain captured tenant/principal/scopes while canonical owners authorize and audit | Suitability rejects unknown/duplicate tools; per-Agent resolution exposes only `bifrost.query` and `cards.get`; calls enter existing Card/query owners (`host.rs:431-468`; `workflow/tools.rs:98-319`) | Recorded malformed/bounded input, SQL floor, object access, Card audit, query, and unchanged foreign-tenant harness evidence | PASS |
| INV-021 and query ownership: waiter loss signals a tracked owner, the query settles within its bound, and Workflow terminalization follows owner drain | `RunTools` owns a `TaskTracker`; waiter drop cancels a child token; `BoundedQuery` owns response settlement; `drain` closes and waits after no further invocation can start (`workflow/tools.rs:45-114,188-230`; `query/collect.rs`; Oracle lifecycle controls) | Recorded query-control/collector tests and forwarded cancel/deadline/pod-loss recovery journey | PASS |
| R4/R5 / `FIND-TASK-004-21` and `-22`: all tool clones consume the exact prepared deadline; omitted/longer values clip to it and a shorter positive `deadline_ms` wins | Host binds `PreparedWorkflowRun::deadline()` once before acceptance; invocation uses omitted-as-remaining and `requested.min(remaining)` (`host.rs:382-400`; `workflow/tools.rs:45-96,188-215`) | R5's existing Oracle journey now drives omitted, longer, and shorter cases; lines 232-270 require run-timeout/no-output for the first two and query-timeout plus successful continuation before the run boundary for the shorter case | PASS |
| R5 / `FIND-TASK-004-23`: an OpenAI Chat body with a custom Prompt destination is accepted by the existing `openai_chat` external route without gateway ingress | `protocol_matches` accepts the sole Chat variant independently of `Prompt.provider`; external client posts the native body (`skald-workflow/src/route.rs:429-454,611-637`) | `pg_workflow_runs.rs:2326-2377` registers the custom-provider Prompt and asserts one `/v1/chat/completions` request, bound secret, unchanged body, no provider wrapper, no gateway ledger, and no gateway decision | PASS |
| Follower graph release follows grant-stream close; supervisor drain polling, idle refusal, and leader acknowledgement stay absent | The leader retains participant grant streams through settlement and drops them as release; follower settlement is asynchronous; no release acknowledgement is awaited (Oracle analytical lifecycle owner and Bifrost design authority) | Recorded forwarded Oracle settlement, recovery, and surviving-baseline evidence | PASS |
| No unauthorized durable jobs, new graph/query/audit engine, arbitrary tool platform, bearer renewal, compatibility reader/alias, bespoke check, setting, option, dependency, or unprecedented enforcement mechanism | The cumulative diff composes existing owners and common mechanisms (`JoinSet`, `TaskTracker`, cancellation tokens, watch channels, `OnceLock`, serde tagging, schema generation, existing journeys). Revision 14 deletes duplicate variants/wrappers rather than adding compatibility machinery. | Complete changed-file inventory and cumulative-diff inspection | PASS |
| Active task packet identifies the current approved authority while preserving immutable historical review subjects | Current task and R5 frontmatter remain Revision 13 although Revision 14 is approved, expressly carried by TASK-004, and recorded in R5 implementation evidence (`TASK-004:6,435`; R5 `:6,169-188`; `spec.md:21-54,2760-2764`) | Direct source inspection | **FAIL** (`INV-R6-001`) |
| R1-R5 preserve closure of prior findings without reopening a shared source | Source and proof corrections for findings 1-16 and 18-23 remain present. The authority-chain obligation previously captured by finding 17 regressed when Revision 14 was added without advancing current task metadata. | Prior validated ledgers, current source, and recorded R5 evidence | **FAIL** (`INV-R6-001`) |

## Invariant traces

- **Prepared deadline:** Skald samples one total deadline during preparation.
  The server binds that exact instant into the `OnceLock` shared by every
  hydrated `RunTools` clone before acceptance. `QueryTool` uses the caller's
  shorter positive deadline or the prepared remainder. R5's real Oracle path
  now distinguishes all three precedence cases and retains settlement and
  recovery assertions.
- **Published step state:** scheduling reserves attempt one before publishing
  `Running`. Cancellation, deadline, and task abort rejoin through settlement
  and become `Cancelled`; only never-scheduled pending entries become
  `Unstarted`.
- **Accepted authority and run ownership:** verified authentication produces a
  token-free `Caller`; create authorizes and audits it; preparation pins the
  graph; admission installs the scoped key, capacity, cancellation, tracker,
  and outcome. Gateway/Card/query calls retain the captured attribution but
  enter their existing live decision owners. Replay cannot replace authority.
- **Provider schema and destination:** persistence and schema generation carry
  the body dialect in `ProviderRequest` and the optional dispatch destination
  in `Prompt.provider`. Native Agent registry lookup consumes the latter;
  gateway routes consume their selected model/upstream and the body schema.
  Vertex therefore sends the shared GenerateContent body through Vertex's
  credentials/endpoint, while the local client retains its existing refusal.
- **Query and analytical ownership:** an Agent wait owns only a cancellation
  guard and completed-value wait. The tracked query owner retains and settles
  the response before Workflow terminal commit. The Oracle leader releases a
  follower graph by dropping grant streams and never waits for an
  acknowledgement.

## Prior-finding closure

- `FIND-TASK-004-1` through `-16` and `-18` through `-23` remain closed at
  their corrected source owners and named proof seams.
- R5 closes `FIND-TASK-004-22`: the existing forwarded-query journey now
  exercises omitted, explicit longer, and explicit shorter positive deadlines
  through the actual Workflow tool and Oracle path.
- R5 and Revision 14 close `FIND-TASK-004-23`: the route accepts the one
  OpenAI Chat schema irrespective of the Prompt dispatch target and the
  existing server journey proves direct external ownership.
- The obligation behind `FIND-TASK-004-17` regressed as `INV-R6-001`: current
  TASK-004/R5 metadata did not advance when approved Revision 14 was appended.

The binding human decisions were treated as authority and not relitigated:
supervisor drain polling and idle refusal stay deleted; grant-stream close is
the follower release and has no leader acknowledgement; the foreign-tenant
harness is unchanged; published `Running` reserves attempt one and interrupted
published work settles `Cancelled`; query tools use the prepared run deadline
through a one-time bind; and Revision 14's schema/destination split and local
Vertex refusal remain as approved.

## Open questions

None affecting correctness or the remediation boundary.

## Verification notes

- This review did not run builds, tests, formatters, code generation, package
  managers, Cargo, or mise, as explicitly required. Recorded evidence was
  treated as a claim and checked against candidate source and named
  assertions.
- R5 records the focused compatible-route and prepared-deadline commands plus
  the final format, lint, Skald, shared, Wyrd-server, gateway, Python,
  TypeScript, codegen, boundary, and Oracle journey lanes as passing.
- The R5 Revision 14 record additionally names focused custom-provider and
  Vertex dispatch tests and reports all broader lanes green on the final tree.
- These recorded results support the runtime and generated-contract rows, but
  tests cannot resolve the stale active task authority; that correction is
  direct source metadata/prose only.

## Overall result

**FAIL**
