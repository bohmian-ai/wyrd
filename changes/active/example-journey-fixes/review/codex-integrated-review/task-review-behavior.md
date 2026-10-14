# Independent behavior review

## Immutable subject

- Base: `ef03f624a1f3a6bcd51bbd17060271f0b5e8cabf`
- Candidate: `c4ba2b0502f8968a78e5e960bcc9eba43e960643`
- Candidate observed at review start and finish: `c4ba2b0502f8968a78e5e960bcc9eba43e960643`
- Approved authority: `changes/active/example-journey-fixes/spec.md` revision 4, with `owner-decisions.md` OD-1 through OD-7 binding on conflict
- Reviewed range: complete `ef03f624a..c4ba2b050` diff (153 files), not task summaries alone

## Navigation and caller map

| Changed behavior | Owning path | Reachable callers and consumers | Main proof inspected |
|---|---|---|---|
| Typed Jev | `skald-spec` Jev wire/request/response and Prompt; `skald-providers::JevClient`; `wyrd-gateway::adapter::jev`; `vala-eval::SkaldJudgeInvoker` | Native `/v1/systemone`, unified `/v1/chat/completions`, provider registry, direct and queued LLM judges, generated Card schemas/types | Typed wire/adapter tests plus Rust, Python, and TypeScript gateway journeys recorded in TASK-001 |
| Bifrost errors and live subqueries | `vala-bifrost-redux::oracle::{map_sql_planning_error, LiveScribeExec}` | HTTP query, shared Bifrost clients, Rust/Python/TypeScript SDKs, MCP query, local and peer Scribe | Error journeys and shared DataFusion-oracle workload in TASK-002 |
| MCP compatibility | `wyrd-server::mcp` descriptors/dispatch; nested `operators::CreateArgs` | Full administrator tool catalog, official MCP SDK, OpenAI Agents conversion, read-tool invocation | Rust whole-catalog schema test and TypeScript framework journey in TASK-003 |
| Attribution and traces | gateway ingress/invocation/capture; verification control/runner/eval/results; verifier-run SQL; client transport | Public inference, SDK verify, MCP verify, direct/manual/scheduled/observation verification, Scribe result and trace tables | Rust/Python/TypeScript/MCP journeys and SQL/runtime tests in TASK-004 |
| Managed-secret sealing | `wyrd-gateway::{managed,credential}`; auth sealing rewrap; server boot; CLI dev | Admin credential writes, gateway invocation, rotation/startup, fresh and repeated `wyrd server dev` | Binding, rotation, gateway, and local-dev CLI journeys in TASK-005 |

The review expanded the supplied map through the concrete callers above. No duplicate production graph store, loader, transport, keyring, or alternate authorization owner was introduced. The shared analytical workload is test-only and compares Bifrost with plain DataFusion rather than becoming a production query path.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: Jev is typed end to end, including native Prompt rendering, native and unified routes, typed response/usage, stable errors, no chat translation, and LLM-judge assertions | `crates/skald/skald-spec/src/wire/jev.rs:23`; `crates/wyrd/wyrd-gateway/src/adapter/jev.rs:27`; `crates/vala/vala-eval/src/orchestrator/judge.rs:214` | TASK-001 matrix and commands at `TASK-001-jev-typed-provider.md:44-72`; Rust/Python/TS journeys cover native, stock OpenAI pass-through, invalid form, refusals, and judge pass/fail | PASS |
| REQ-002: caller SQL planning faults are actionable 4xx across shapes/surfaces; internal errors stay opaque; unauthorized schema is not disclosed | `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3746-3800` unwraps only caller-planning wrappers after authorization | TASK-002 evidence at `TASK-002-bifrost-errors-and-subqueries.md:50-64` covers SDKs, MCP, unauthorized table, internal/exhaustion mapping | PASS |
| REQ-003: 2026-07-28 MCP catalog is consumable by common clients without weakening validation | `crates/wyrd/wyrd-server/src/mcp/operators.rs:56`; `crates/wyrd/wyrd-server/src/mcp/principals.rs:100`; `crates/wyrd/wyrd-server/src/mcp/mod.rs:454` | Full 35-tool Rust catalog and official MCP/OpenAI Agents TypeScript journey at `TASK-003-mcp-compatibility.md:37-55` | PASS |
| REQ-004: independent optional gateway Run/Card correlation; judge calls carry subject and optional Run; manual/direct caller and internal SYSTEM actor are recorded; origin remains references; no new direct binding authorization | Header parsing and card check in `gateway/ingress.rs:263` and `gateway/invocation.rs:731`; judge subject in `verification/eval.rs:252`; requester projection in `verification/results.rs:147`; SQL SYSTEM actor in `wyrd-sql/src/queries/verifier_runs.rs:92` | Normal provisioned-tenant paths pass across Rust/Python/TS and SQL/runtime tests (`TASK-004-gateway-attribution-and-trace.md:42-47,70-72,101-103`) | **FAIL**: BEH-001 leaves one reachable successful direct execution with no result or requester evidence |
| REQ-005: inbound `traceparent`/`tracestate` links captured gateway spans; malformed parent is ignored; in-process judge calls inherit | `http/middleware/trace_context.rs:31-70`; `components/gateway/invocation.rs:489`; `components/gateway/capture.rs:360-382`; client propagation in `wyrd-client/src/transport/trace.rs:25-63` | Rust/Python/TS gateway and verify journeys plus MCP rescoping at `TASK-004-gateway-attribution-and-trace.md:46,70-82` | PASS |
| REQ-006: scalar, IN, EXISTS, correlated, and representative analytical subqueries agree over live, published, mixed, and remote state | Scalar-subquery predicate remains on leader at `oracle/live.rs:395-410`; shared oracle workload in `wyrd-testing/src/bifrost/support_workload.rs` | Ten-query DataFusion comparison and hand counts across all required states at `TASK-002-bifrost-errors-and-subqueries.md:69-135` | PASS |
| REQ-007: managed secrets use the deployment sealing keyring with binding and rotation; rejected keyring is removed; local dev provisions/reuses a private key before boot without restart or disclosure | `wyrd-gateway/src/managed.rs:25-139`; `wyrd-gateway/src/credential.rs:58-153`; `wyrd-cli/src/server/dev.rs:522-573`; migration `20261014000000_gateway_managed_secret_sealing_keyring.sql:1` | Binding, cross-tenant, rewrap, keyless boot, CLI rerun and no-leak evidence at `TASK-005-local-keyring.md:45-63` | PASS |
| INV-001: tenant isolation on every changed path | TenantConn/RLS remains the storage boundary; gateway Card attribution is tenant-resolved; managed envelopes bind tenant; analytical workload covers tenant-qualified live/published paths | `check:tenant-isolation` and cross-tenant journeys recorded in TASK-002, TASK-004, and TASK-005 | PASS |
| INV-002: credentials and keys are never logged or returned | Secret wrappers/redacted debug; provider refusal scrubbing; key creation prints no value | Secret-canary, refusal, local CLI output and rotation evidence in TASK-001/TASK-005 | PASS |
| INV-003: audit remains non-blocking | Changed permission decisions still stage through the existing `ScribeOutbox`; no audit write or error path was added | Existing gateway/verification journey evidence and diff audit | PASS |
| INV-004: no removed Jev shim or alias; only approved typed unified pass-through remains | Closed typed Jev request plus `chat_request` decoding of `{state,questions}`; Agent loop rejects Jev chat turns | TASK-001 typed adapter, fallback-isolation, stock-client, and Agent-loop tests | PASS |
| AC-001: every REQ has the required journey class and base/candidate discrimination | Journey files exist across SDK, MCP/HTTP, CLI, and Bifrost topology surfaces | TASK-001..TASK-005 record the focused commands and, where required, base reproduction | PASS |
| AC-002: invalid Jev type and 400/429/5xx, unauthorized Bifrost table, malformed traceparent | Negative branches are implemented at typed decode, planner authorization, and trace parsing boundaries | Recorded journeys at TASK-001:52,74-77; TASK-002:50-53; TASK-004:46 | PASS |
| AC-003: format, lints, codegen and owning mise lanes pass | N/A | Each task records green scoped gates. This reviewer attempted an additional focused `mise exec` run; after redirecting read-only mise state to `/tmp`, it remained blocked on the shared Cargo build lock and was cancelled. No contrary result was observed. | PASS (recorded evidence) |
| AC-004: `wyrd-example-ts` itself runs clean with no escaping or tool allow-list | No candidate file or task evidence names an execution of the external example | The only `wyrd-example-ts` occurrence in the candidate is the objective/AC text at `spec.md:11,192`; task command ledgers stop at repository journeys | **FAIL**: BEH-002 |
| Non-goal: no DeepSeek or other provider | Provider changes are confined to Jev and existing provider abstractions | Complete diff audit | PASS |
| Non-goal: no new Verifier semantics | Changes add typed Jev dispatch and attribution/evidence only; scoring semantics remain owned by existing executors | Complete diff and existing pass/fail journeys | PASS |
| Non-goal: no public release | No release/tag/publication machinery changed | Complete diff audit | PASS |
| OD-1: manual/direct actor is caller; internal actor is SYSTEM; origin is references; observation span links; observation judge authority remains unchanged/open | SQL resolves SYSTEM for scheduled/observation; result copies frozen requester; direct sets caller; record trace is linked; writer caller remains judge authority | TASK-004 follow-up evidence at lines 88-120 | **FAIL** only for BEH-001's direct/no-SYSTEM path; the three owner-held exceptions are otherwise unchanged |
| OD-2: gateway Run/Card are optional and independent; judge gets subject plus optional Run; no binding enforcement | Optional header parsing and `GatewayCallSubject` options; server-resolved judge attribution; direct authorization remains `verifier:run` | Four-way gateway attribution and direct-without-Run journeys in TASK-004 | PASS |
| OD-3: no extra Run-context injection; server joins inbound trace; trace does not authorize or attribute | Trace context is a separate task-local carrier read only by capture; attribution comes from explicit headers/execution context | Trace and malformed-header journeys in TASK-004 | PASS |
| OD-4: approved Jev r2 behavior on both routes, pass-through, 401 relay, typed Agent-loop refusal | Typed Jev adapter/client/Judge owners noted above | TASK-001 matrix lines 44-52 and journey commands | PASS |
| OD-5: deployment keyring replaces old TASK-005; no separate config/restart/stale code | Existing `SealingKeyring` and rewrap inventory extended; local dev supplies the ordinary env file before boot; old columns/config removed | TASK-005 matrix lines 45-63 | PASS |
| OD-6: analytical correctness uses nulls/ties/duplicates/orphans, plain DataFusion plus hand answers, and published/live/mixed/remote states | Shared deterministic workload and remote peer test | TASK-002 analytical evidence lines 69-135 | PASS |
| OD-7: root causes, minimum complexity, independent integrated review | Planner/live-leaf, descriptor schema, execution-context, shared keyring, and typed provider owners are corrected in place; this report is the independent behavior pass | Complete diff and this review | PASS |

## Proposed findings

### BEH-001 — MISSING — a successful direct execution can omit its required caller attribution entirely

- Violated obligation: REQ-004 and OD-1 require the authenticated caller to be recorded for a direct execution, including on the verification result.
- Location: `crates/wyrd/wyrd-server/src/components/verification/service.rs:535-538`.
- Evidence: the direct engine has already completed and its judgment will be returned, but `stage_result` returns without staging any result when `resolve_direct` found no active SYSTEM principal. The caller is not placed into the result until `service.rs:563-589`, which is unreachable on this branch. TASK-004 itself establishes that tenants without an active SYSTEM principal are representable (`TASK-004-gateway-attribution-and-trace.md:107-112`); the owner-held exception permits a null actor only for **internal** work, not direct authenticated executions.
- Observable consequence: the caller receives a successful direct judgment and execution ID, but querying `vala.verification.results` finds no row, so neither the requester nor the judgment is durable evidence and the result cannot join its judge calls.
- Required testable correction: preserve the existing SYSTEM-scoped Scribe result-writer invariant, but make an active SYSTEM principal a pre-execution prerequisite for a result-producing direct execution instead of silently succeeding without evidence. Return the repository's stable verification/service refusal before the engine produces a judgment, and add a focused direct-execution test using the existing tenant-without-SYSTEM fixture proving that no successful response can lack its result. Do not record the caller as the Scribe publisher and do not alter the known internal-run null-requester decision.

### BEH-002 — MISSING — the required external example journey has no execution proof

- Violated obligation: AC-004 requires `bohmian-ai/wyrd-example-ts` itself to run clean against the candidate without client-side escaping or a tool allow-list.
- Location: `changes/active/example-journey-fixes/spec.md:192-194`.
- Evidence: a full candidate search finds `wyrd-example-ts` only in the objective and AC-004 (`spec.md:11,192`). None of the TASK-001..TASK-005 verification ledgers records cloning/running the example, its command, its revision, or its result; they prove repository-owned component journeys instead.
- Observable consequence: the acceptance audit cannot establish that the exact user journey which exposed the combined defects works without the forbidden workarounds. Individually green component journeys do not prove the external example's composition, configuration, dependency versions, or absence of those workarounds.
- Required testable correction: run the owner-designated `wyrd-example-ts` revision against this cumulative candidate using its documented setup, remove/disable any client-side escaping and MCP tool allow-list for the run, and record the exact immutable example revision, command, and successful end-to-end result in the change packet. If the example cannot run because its repository/revision or credentials are unavailable, record that as a blocker rather than substituting repository tests.

## Non-blocking notes

None. The review found no optional refactor or style preference that belongs in acceptance.

## Verification limits

- I relied on the detailed immutable task evidence for the expensive Postgres, SDK, MCP, local-dev, and full Bifrost lanes after checking the cited production paths and test sources.
- An additional focused Bifrost unit rerun initially failed because mise tried to write its trusted-config state under a read-only home. With `XDG_STATE_HOME` redirected to `/tmp`, Cargo waited on the shared build lock held by concurrent reviewers; I cancelled the wait without a test result. This is not contrary evidence, but no new independent test result was obtained.
- Live Jev smoke evidence remains credential-gated as recorded by TASK-001.

## Overall result

**FAIL**

The candidate closes the component defects and binding decisions on its normal provisioned paths, but it does not satisfy the original task exactly: one reachable direct-execution path returns success without the required result/requester evidence, and the mandatory external example run is unproved.
