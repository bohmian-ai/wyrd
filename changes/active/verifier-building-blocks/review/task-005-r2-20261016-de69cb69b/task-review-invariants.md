# TASK-005 invariant review

Result: **FAIL**. Prior FIND-TASK-005-1 through FIND-TASK-005-9 are closed on the inspected candidate. One newly identified shared input-validation gap remains.

## Immutable subject and method

- Source: detached checkout `/tmp/wyrd-task005-review-de69cb69b`, HEAD `de69cb69b68eced72f7d760681d27253e013f8a1`; clean at final inspection.
- Cumulative range: `f2a726c90edbf8956c7b91c843379175ee20eac0...de69cb69b68eced72f7d760681d27253e013f8a1`. Recent remediation range `99882cf82...de69cb69b` was navigation, not the acceptance boundary.
- Authority: frozen AGENTS.md, agent rules, reference router, spec-driven-development and maintainer-style references; approved verifier-building-blocks spec r9; original TASK-005; prior verdict/validation/R1/S1 and their recorded verification evidence; applicable Wyrd design, doctrine, Bifrost durability/identity, security/authority, evaluation and testing rules.
- The checkout has no `.codegraph/`; CodeGraph was skipped. No peer discovery reports from this round were read. No source, task, configuration or generated artifact was changed. Only this assigned report was written.
- Reviewed the cumulative changed-surface inventory and actual source/diffs, expanding the supplied map through producers, consumers, sibling consumers, authorization, queue acceptance, claim/reclaim, result publication, settlement, scheduled scoring, and trace routing. External examples were inspected through the recorded commit objects (`5c79080`, `1e27f68`, `669d130`), not their working trees.
- No migration, deployed-history compatibility, or old pending-delivery upgrade requirement is introduced. Nothing shipped. The draft agent-runtime-composition spec is not implementation authority. S1's open Jev/non-conversation capability and unproven provider cases are not treated as approved new requirements here.

## Acceptance matrix

PASS means no blocking invariant defect found in the inspected implementation and recorded evidence, not a fresh test execution. The full integrated verification obligation remains subject to final change review.

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: one generic Verifier, no implementation discriminator, globally unique block identity | Existing VerifierSpec/validate/plan; registration cutover in components/cards/resolve.rs and service.rs | Recorded contract, registration and codegen lanes; direct no-step fixture | PASS |
| REQ-002: named slots, wrong kinds/unknown names/schema/size refusal, honest missing evidence | Slot shapes exist, but HTTP/MCP input is not checked against them; Scope::resolve reads supplied values directly | Existing route negatives test malformed request and extra top-level fields, not an extra named input or violated JSON slot schema | **FAIL — INVREV-01** |
| REQ-003: one dependency graph, per-step checks before dependents, independent overlap | VerificationEngine::drive reuses plan/stage walk and Scope/RunLedger | Recorded Vala block tests and three-language building-block journeys | PASS |
| REQ-004: extract/Agent/code preparation, no external mutation | HostedBlocks routes to TenantEvidence, CodeExecutor and AgentExecutor; old judge removed | Recorded eight-cell three-language matrix and sandbox lane | PASS |
| REQ-005/006: typed checks, inconclusive infrastructure failure, skips and aggregation | Engine::judge/attempt, existing comparison/scenario executors; Busy is execution-wide | Recorded engine and scenario tests; missing required input remains inconclusive | PASS |
| REQ-007: Prompt/route/identity reuse, hosted gateway, exact provenance | HostedBlocks, AgentExecutor, TenantRegistry, ServerWyrdGatewayCaller; schema compiler/provider lowering | Recorded building-block journeys and supplemental live-provider results, with stated limits | PASS |
| REQ-008: built-in tool invocation and independent resource permissions, original subject/delegation | HostedTools dispatches through audit::authorize and existing Card/Bifrost/workspace/sandbox owners; statement_caller retains authority with fresh query identity | Recorded delegated/tool-negative journeys and hosted integration tests | PASS |
| REQ-009: original evidence citations, tool-less judge | TenantEvidence::cited/located and AgentExecutor checks; workspace source mappings | Recorded citation/invalid-citation journeys | PASS |
| REQ-010: file digest/length/sniffing, local-only upload, tenant object namespace | Local upload uses caller tenant and atomic verified write; TenantEvidence::stored/read verifies each read | Recorded local storage/route/served OpenAPI tests | PASS for upload integrity; input-slot and aggregate-size enforcement fails under INVREV-01 |
| REQ-011/012: direct vs queued, identity, status/wait/reconnect, dispatch visibility | Shared client Target and Execution; VerificationControl; queue state/dispatch projections | Recorded three-language immediate/submitted/reconnect/tenant journeys | PASS |
| REQ-013: one ACK→batched acceptance→lease lifecycle, async replay/conflict, accepted authority, no request PG write | ScribeSink::write only hands acknowledged non-lost inputs to SubmissionOutbox; accept_submissions owns uniqueness; runner claims/fences/settles | Recorded SQL/runtime replay, fencing, loss and reclaim tests | PASS |
| REQ-014: explicit slots/windows, requirement baselines, failed-only Operators | Scheduler + queue preserve owner and mapping; DriftEngine::checks filters mapped slots; baseline fitter keyed by requirement; complete freezes dispatch context | Recorded scheduled drift and optional-slot proof; all methods retain existing scorers | PASS |
| REQ-015: canonical attributable breakdown, correlation/time fields, winning result identity | ResultPayloadBuilder assigns one new result ID per built payload; SQL stores winning pointer; read_result selects pointer; drift feature rows carry same ID and requirement | Recorded reclaimed-attempt fault test plus result mapping and Bifrost-read journeys | PASS |
| REQ-016: three matched and standalone examples | Recorded example commits own Cards and use current Run/observe/verify/submit/reconnect paths; inspected primary application sources | Recorded external automated tests; live README quickstarts not rerun | PASS within recorded scope |
| REQ-017: HTTP/MCP/SDK/CLI/UI/docs cutover | Shared control adapters; CLI scenario path preserves assertions/trace/agent; removed judge and Eval observation activation; docs/generated consumers updated | Recorded OpenAPI/MCP/codegen/docs/export/typecheck lanes | PASS |
| INV-001/002: Rust server core, shared client, no duplicated orchestrator | HostedBlocks composes Vala stage owner and Skald Agent runtime; clients use wyrd-client | Recorded dependency checks; source owner/caller trace | PASS |
| INV-003/004: tenant and exact scope; blocking permission/nonblocking audit; no extra durable outbox | authorize_run uses exact Verifier and queued signed subject scope; evidence operation-level check; tenant transactions; one Scribe write route | Recorded scope/delegation/tenant/audit checks; source tracing | PASS |
| INV-005: no general application runtime/network writes | Closed action/check/tool enums; code through sandbox; external reactions owned by Operator worker | Recorded isolation/tool denial tests; no additional host execution path found | PASS |
| INV-006: no success from invalid evidence | Missing/omitted scheduled evidence is corrected, but invalid JSON evidence can satisfy an assertion because slot schema is not checked | Source path in INVREV-01; green client-oriented paths do not prove trust-boundary validation | **FAIL — INVREV-01** |
| INV-007 / deployment limits: startup configuration, bounded resources/cancellation | Once-at-startup parsing and existing pool/workspace owners are used; Busy category reaches runner; combined input-file limit has no execution owner | Recorded configuration/capacity/cancellation tests do not cover separately uploaded over-total evidence | **FAIL — INVREV-01** |
| INV-008: complete replacement, unrelated scenarios/observations preserved | Old direct/Eval/observations/judge modules removed; assertions/trace/agent scenario owners remain; historical table data not relabelled as new results | Recorded search/codegen/CLI and scenario tests | PASS |
| AC-001–004: contracts, engine, composed blocks/isolation | Relevant registration and executor owners inspected; no new runtime isolation mechanism added by this task | Recorded contract/Vala/hosted/sandbox lanes; deployment aggregate gap above | **FAIL for aggregate input admission; otherwise supported** |
| AC-005/007/008: three-language Run and scheduled journeys/examples | Shared client + current server lifecycle + recorded example sources | Recorded integrated gate and remediation-focused evidence; no fresh broad rerun | PASS within evidence limits |
| AC-006: cross-boundary schema/size negatives | Existing wrong-output/provider cases exercise provider response schema; they do not exercise authored input JSON schema or over-total execution files | Missing focused raw HTTP/MCP regression identified below | **FAIL — INVREV-01** |
| AC-009/010: consumer parity and verification | Updated runtime MCP/served contract and generated declarations | Original integrated gate predates remediation; R1 records focused runtime/SQL/three-language trace and required lint/docs/boundary checks | PASS as task evidence, pending final integrated qualification |
| Non-goals: no second queue/store/scheduler/recovery service, no new Card kind, no arbitrary plugins/runtime, no new drift algorithm/rendering | Existing queue/outbox/claim/scheduler/scorers reused; worker/tool runtime remains closed | Static owner/caller audit | PASS |

## Proposed finding

### INVREV-01 — Supplied evidence has no authoritative validation against its Verifier or execution byte budget

**Classification:** MISSING / VIOLATION. **Obligations:** REQ-002; INV-001 (server owns validation), INV-006 (honest evidence), INV-007 and Deployment `WYRD_VERIFICATION_FILES_MAX_MIB`; TASK-005 Scenarios 1, 2, 5 and 6; AC-006.

**Changed locations:**

- `crates/wyrd/wyrd-server/src/components/verification/service.rs:445-496` (`execute`) and `:554-618` (`enqueue`).
- Shared composition: `crates/wyrd/wyrd-server/src/verification/hosted/blocks.rs:105-146`.
- Actual consumers: `crates/vala/vala-eval/src/blocks/engine.rs:149-177` and `scope.rs:198-231`.
- Size checks that currently sit at insufficient boundaries: service `:325-345`, `hosted/evidence.rs:236-269`, `hosted/workspace.rs:73-89`.

**Producer → consumer proof:**

1. HTTP `decode_body` and MCP deserialize a typed VerificationRequest. That validates the envelope and typed identities, not the relationship between `inputs` and the selected VerifierSpec.
2. Direct execution resolves exact Cards and passes `request.inputs` into HostedBlocks and VerificationEngine unchanged. Enqueue stages the same declarations; queue `accept` checks target readiness/availability/replay, then `insert_submission` stores them. Runner dispatch restores them and reaches the same HostedBlocks/engine. None checks unknown named inputs, InputSlot/InputDeclaration compatibility, input JSON schema, accepted media formats or slot max_bytes against the resolved Verifier.
3. VerificationEngine::run plans the authored graph, but does not validate supplied evidence. Scope::resolve directly returns supplied JSON, or a non-JSON declaration with an empty reference path. It does not consult the corresponding slot schema/type for present evidence.
4. The existing client projection (`wyrd-client/src/verify/mod.rs:314-393`) checks names/kinds and its File pinning checks formats/max_bytes, but it is bypassed by primary HTTP/MCP surfaces. It also does not evaluate an InputSlot JSON schema, so that failure is reachable through the SDK too.
5. For total bytes, only evidence upload planning sums the files in that particular plan request. Individual local PUTs need no plan/session. Even normal separate plan calls can upload files legally and later combine them. TenantEvidence enforces only each file's individual limit, and Workspaces enforces a sum per materialized workspace. Extract actions and native media reads do not necessarily create a workspace; several workspaces in one execution also have separate totals. Artifact declarations bypass upload planning altogether.

**Concrete reachable consequences:**

- Register an ordinary no-step Verifier with a JSON slot `x` whose schema is `{"type":"integer"}` and an `is_not_null` requirement on `inputs.x`. Supply `{"kind":"json","value":"text"}`. The graph accepts this declaration and the assertion passes, despite REQ-002 requiring a structured refusal for a schema violation. This is not missing evidence or a provider failure.
- Add an undeclared named input to an otherwise valid request: it is retained in `outcome.evidence` rather than refused. A wrong declared kind likewise reaches consumption instead of required input validation.
- Configure tiny limits (for example 4 bytes/file and 5 bytes/execution), upload two valid 4-byte text files separately, then execute or submit a two-extract Verifier over both. Each read succeeds; the execution consumes 8 input bytes and can pass. The same path scales past the default 100 MiB bound. Existing per-upload/per-workspace checks do not enforce the declared per-execution invariant.

**Existing owner and minimal correction boundary:** Add the missing authoritative validation to the existing execution admission/composition path, shared by direct and queued executions, before actions/checks can run. Resolve validation against the exact VerifierSpec already loaded there, rather than adding schema/scope discovery or another workflow. Reuse InputSlot/InputDeclaration, current structured input errors, and the installed JSON Schema implementation already used for Agent output validation. Keep server-specific deployment byte limits with HostedBlocks/TenantEvidence, rather than putting HostedLimits in a foundational contract. A shared pure slot validator may also serve client feedback; independent parallel SDK validators are not the fix.

Enforce known declaration byte bounds before staging/execute where possible; enforce the actual execution total for resolved Artifact/file evidence at the execution-owned evidence boundary so extraction, native media, citations and workspace siblings cannot bypass it. Preserve per-file digest/length/sniffing, Artifact authorization and per-workspace restrictions. Do not count repeated internal reads as new independent input declarations or reset the aggregate for each step/workspace. Use overflow-safe accounting. Do not add per-request Postgres writes, a durable reservation, another queue, or model work merely to validate input. Queued schema/Artifact rejection can use the existing asynchronous terminal execution-error surface after its exact spec is resolved; it must not become a subject pass/failure or spend provider/action retry budgets.

**Focused testable closure:** Extend the existing real route/runtime fixtures using lowered hosted limits and raw typed HTTP/MCP requests so client preprocessing cannot hide the defect. Cover schema violation, unknown named slot, wrong declaration kind and accepted media/slot byte constraints on both execution modes; retain missing-required inconclusive and omitted-optional-null behavior. Upload two individually legal small files in separate calls and prove their over-total execution is refused before block work, with no passing result or Operator dispatch. Include the Artifact and multi-workspace/extract/native-media siblings through their existing hosted fixtures, and a below-limit successful execution. Run only exact new regression selections through the existing Postgres wrapper and the narrow affected modules; no new test harness or broad gate is required for this correction.

No runtime reproduction was executed in this static review. The source path is complete and reachable without credentials, races, legacy data or unsupported providers. Existing route `unknown` coverage adds an unknown top-level request field, not an unknown entry inside `inputs`; provider/output-schema negatives test a different boundary.

## Independent prior-finding closure

| Stable finding | Source evidence on this candidate | Closure |
|---|---|---|
| FIND-TASK-005-1 | enqueue passes subject to authorize_run; Scoped(scope).uids() checked before staging; audit outcome follows the combined decision; direct exception deliberately preserved | CLOSED |
| FIND-TASK-005-2 | Sandbox admission/checkout yields Busy; Workspaces uses try_acquire_owned; tools preserve Busy as typed cause; RecordedTool raises refusal; AgentExecutor checks it after loop; engine propagates Busy without judging/retrying; runner defers via fenced refunded release | CLOSED for the prior diagnosed capacity category |
| FIND-TASK-005-3 | runner passes frozen Scheduled.slots; DriftEngine::checks scores only supplied slot names; no report falls through to DriftChecks missing evidence | CLOSED |
| FIND-TASK-005-4 | authorize_evidence uses covers_operation(Verifier, Run), audits both outcomes; execute/enqueue retain exact selected-Verifier permission | CLOSED |
| FIND-TASK-005-5 | build mints VerificationResultId::new_v7 once per payload; features and summary share it; staged payload retries retain it; fresh claim builds another ID; SQL complete and status choose winning ID; frozen dispatch references same pointer | CLOSED |
| FIND-TASK-005-6 | r9 explicitly approves local-only PUT; route conditional, caller-derived tenant, bounded body, digest check before write_atomically, served contract | CLOSED under r9 |
| FIND-TASK-005-7 | Remediation diff supplies substantive rustdoc on outbox fmt, submissions items/errors/name, scenario/trace/assertion/agent/executor test helpers, parity drive and authority description | CLOSED for the prior inventory; repo reviewer owns exhaustive standards coverage |
| FIND-TASK-005-8 | Bifrost prose now says outcome Variant and recorded_at; telemetry labels match owner; security description separates scheduled SYSTEM reads from submitted caller trace reads | CLOSED |
| FIND-TASK-005-9 | Source search for AWAIT_TRACE_SQL, TraceWaitOutcome and await_trace under crates/sdks returns no matches; active release/retry/fencing paths remain | CLOSED |

The new finding is not a reappearance of one downstream capacity mapping, subject scope, optional scheduled evidence inference, or result-identity alias. Its common source is missing validation of the evidence set entering execution. Correcting only the SDK, upload planning or one extractor would leave sibling paths reachable.

## Verification and limits

This review ran static git/source navigation only. Prior recorded proof includes the original integrated gate, R1 focused red/green and module checks, runtime 40/40, SQL 22/22, real worker capacity checks, three-language caller-trace journeys, lint/docs/dependency/tenant checks, and S1 live-provider results. Those records are evidence, not fresh reproduction on this review host. The supplemental sweep honestly remains 42 passed/1 failed and explicitly lists unproven paths; no draft capability was demanded to clear it.

Skipped fresh tests, broad gates, live providers, cloud storage and external rebuilds. The material risk is accepting schema-invalid or over-budget supplied evidence as a successful verification; INVREV-01 requires closure before acceptance.
