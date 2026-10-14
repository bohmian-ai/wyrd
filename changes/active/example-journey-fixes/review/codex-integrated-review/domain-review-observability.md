# Observability attribution and W3C trace propagation review

## Subject

- Base: `ef03f624a1f3a6bcd51bbd17060271f0b5e8cabf`
- Candidate: `c4ba2b0502f8968a78e5e960bcc9eba43e960643`
- Specification: `changes/active/example-journey-fixes/spec.md`, revision 4
- Task evidence: `changes/active/example-journey-fixes/tasks/TASK-004-gateway-attribution-and-trace.md`
- Binding decisions: `changes/active/example-journey-fixes/owner-decisions.md`, OD-1 through OD-7

The candidate remained at the stated commit throughout this review.

## Reviewed boundary and authority

I reviewed the complete cumulative diff, then traced these paths through their current owners and callers:

- public gateway attribution headers through `requested_subject`, card-scope admission, `CallFacts`, and the `vala.gateway.calls` Arrow projection;
- direct, manual, scheduled, and observation-triggered Verifier actor/origin state through `wyrd.verifier_runs`, `ClaimedRun`, `ResultRun`, and `vala.verification.results`;
- direct and observation judge calls through `DirectExecutor` / `EvalEngine`, `ServerWyrdGatewayCaller`, gateway admission, and capture;
- observation trace/span identity through the committed observation read and the `verification.engine` OpenTelemetry span link;
- inbound HTTP and MCP `traceparent`/`tracestate`, task-local request context, in-process gateway calls, and captured gateway spans;
- Rust, Python, and TypeScript outbound SDK context additions and the affected shared HTTP transport.

Applicable authority was `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`, `architecture/wyrd-security-posture.md`, spec REQ-004/REQ-005 and invariants, OD-1/OD-2/OD-3/OD-7, and the W3C Trace Context Recommendation. OD-4, OD-5, and OD-6 were checked against the changed-domain map and have no observability-attribution obligation beyond the shared gateway and Bifrost rules already covered here.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| OD-1: manual/direct actor is the authenticated caller; scheduled/observation actor is tenant SYSTEM; origin remains references, not a copied writer principal | `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:87-107`, `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:159-187`, `crates/wyrd/wyrd-server/src/verification/results.rs:165-179`, `crates/wyrd/wyrd-server/src/components/verification/service.rs:563-589` | TASK-004 records the Postgres verifier-run lane as 46/46 and Bifrost server integration as 86/86; source tests assert observation SYSTEM at `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_runs.rs:763-770` and scheduled SYSTEM in the same file at 1027-1029 | PASS |
| OD-1: observation-triggered execution links, rather than parents, its engine span to stored observation trace/span | `crates/wyrd/wyrd-server/src/verification/eval.rs:169-185`; link construction at `crates/shared/wyrd-telemetry/src/lib.rs:264-284` | `crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:1683-1703`, `1766-1785`; TASK-004 records the focused server journey 9/9 | PASS |
| Known OD-1 open decision: observation judge authority remains `writer_caller` | `crates/wyrd/wyrd-server/src/verification/eval.rs:204-225` and `253-263` preserve the writer authority while separately attaching subject/Run attribution | Source inspection; no decision was made or implemented | PASS |
| OD-2 / REQ-004: principal-only, Run-only, Card-only, and Run+Card gateway calls retain independent nullable correlation; a caller-provided Card is scope-checked | `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:251-299`, `crates/wyrd/wyrd-server/src/components/gateway/invocation.rs:482-513`, `722-752`, `crates/wyrd/wyrd-server/src/components/gateway/capture.rs:284-346` | Unit coverage at `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:645-713`; Rust/Python/TypeScript journeys listed in TASK-004 | PASS |
| REQ-004: every judge call receives the execution subject Card and optional application Run, including a direct execution without a Run and an observation execution | Direct path: `crates/wyrd/wyrd-server/src/components/verification/service.rs:457-476`; observation path: `crates/wyrd/wyrd-server/src/verification/eval.rs:253-263`; common call owner: `crates/wyrd/wyrd-server/src/components/gateway/workflow.rs:112-134` | Rust direct journeys at `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:477-568`; Python and TypeScript counterparts recorded passing in TASK-004 | PASS |
| REQ-004: result actor and subject join fields reach durable results; binding, owner, Trigger, and source record remain the queued run's frozen references | `crates/wyrd/wyrd-server/src/verification/results.rs:136-179`, `306-340`; runner uses that view at `crates/wyrd/wyrd-server/src/verification/runner.rs:725-785` | Result/judge join journey at `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:497-525`; continuous-eval source/result coverage recorded in TASK-004 | PASS |
| REQ-005: inbound inference `traceparent`/`tracestate` controls captured gateway trace/parent/state; malformed `traceparent` is ignored | `crates/wyrd/wyrd-server/src/http/middleware/trace_context.rs:28-70`, capture at `crates/wyrd/wyrd-server/src/components/gateway/invocation.rs:482-513` and `crates/wyrd/wyrd-server/src/components/gateway/capture.rs:349-395`, `861-875` | Rust journey `sdks/wyrd-sdk-rust/tests/integration/gateway_inference.rs:314-367`; Python and TypeScript counterparts recorded passing in TASK-004 | PASS |
| REQ-005: an in-process judge call made for traced HTTP or MCP verification inherits the inbound context | HTTP task-local context above; MCP task re-scope at `crates/wyrd/wyrd-server/src/mcp/mod.rs:235-255`; gateway admission snapshots it at `crates/wyrd/wyrd-server/src/components/gateway/invocation.rs:489-513` | Rust judge journey `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:530-568`; MCP journey `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/connectivity.rs:327-383`; TASK-004 records both focused lanes passing | PASS |
| OD-3: add no further client-side trace injection; trace context remains observability-only | Server-side use is observability-only, but the candidate adds automatic injection to every authenticated shared HTTP request and foreign-runtime bridges | Candidate source at `crates/shared/wyrd-client/src/transport/http.rs:876-904`, `crates/shared/wyrd-client/src/transport/trace.rs:18-64`, `sdks/wyrd-sdk-python/src/observe/mod.rs:429-434`, and `sdks/wyrd-sdk-ts/wyrd/src/index.ts:2640-2653` contradicts `owner-decisions.md:50-60` | FAIL |
| OD-4 Jev, OD-5 sealing, OD-6 analytical oracle bar | No additional actor/origin or trace-context rule in this domain; Jev uses the same reviewed gateway ingress/capture path | Diff/domain-map inspection | PASS |
| OD-7: no unrequested complexity or downstream guards | The server reuses one task-local inbound owner and existing gateway capture, but the client additions create a second, broader propagation mechanism expressly excluded by OD-3 | Same evidence as OBS-001 | FAIL |
| INV-001/INV-003 and OD-3 authorization separation | Tenant comes from verified `Caller`; trace context is read only after authentication/attribution decisions and is not consulted by permission checks; no audit write behavior changed | Source trace through gateway admission and verification control; task records `check:tenant-isolation` passing | PASS |

## Material finding

### OBS-001 — VIOLATION: the candidate adds client-side trace injection that OD-3 expressly excludes

- **Violated obligation:** Binding OD-3 says the existing Run/OpenTelemetry stamping is sufficient and that “No further client-side injection is added” (`changes/active/example-journey-fixes/owner-decisions.md:50-55`). OD-7 also forbids complexity the owner did not request (`owner-decisions.md:97-100`). The required change is server-side: when a request already carries `traceparent`, server work and in-process judge calls join it (`owner-decisions.md:56-60`; spec REQ-005 at `spec.md:139-146`).
- **Exact location:** `crates/shared/wyrd-client/src/transport/http.rs:876-904` now injects an active context into every authenticated Wyrd HTTP request. Its new owner and fallback machinery live at `crates/shared/wyrd-client/src/transport/trace.rs:18-64`. Python adds an OpenTelemetry lookup and scopes `verify` at `sdks/wyrd-sdk-python/src/observe/mod.rs:141-168` and `429-434`; TypeScript does the same at `sdks/wyrd-sdk-ts/wyrd/src/index.ts:2484-2510`, `2640-2653` and `sdks/wyrd-sdk-ts/native/src/cards.rs:666-710`.
- **Evidence:** `HttpTransport::authenticated` is the shared constructor for authenticated HTTP helpers, not a gateway- or verification-only seam (`http.rs:882-904`). Consequently an active application span is automatically exported as W3C headers on cards, query, storage, admin, and other authenticated HTTP traffic as well as verification. The Python and Node bridges exist only to manufacture that new implicit transport behavior across their runtime boundaries. This is not needed by the approved server-side solution: explicit inbound headers are already parsed by `http/middleware/trace_context.rs:28-70`, MCP re-scopes its original inbound headers at `mcp/mod.rs:235-255`, and the gateway snapshots that context for capture at `components/gateway/invocation.rs:482-513`.
- **Observable consequence:** callers who did not elect to send distributed-tracing headers now send their active trace identity and vendor `tracestate` to Wyrd on unrelated SDK operations. That changes the shared client wire behavior and trace topology beyond REQ-005, and it directly contradicts the binding owner decision even though the added traced-SDK journeys are green.
- **Required testable correction:** remove the automatic shared-client injection and the Python/TypeScript foreign-runtime handoff added solely to feed it. Preserve the server-owned inbound parser, MCP task re-scope, in-process propagation, and gateway capture projection. Prove closure with the existing explicit-header gateway and raw-HTTP verification journeys (valid `traceparent`/`tracestate`, malformed `traceparent`, and in-process judge inheritance), plus a focused shared-client request assertion that the client does not synthesize trace headers from an active local span. Do not add a replacement propagation option or another transport abstraction.

## Verification limits

- I reviewed the task's recorded passing evidence and inspected the named test bodies. I attempted the two focused `wyrd-client` trace unit tests through `mise`; the first attempt could not create mise's trust symlink on the read-only home state, and the isolated-state retry could not install the pinned Rust toolchain because network/tool installation is unavailable in this review environment. No test was re-run successfully here.
- I did not rerun the Postgres journeys; TASK-004 records their commands and results, and their source assertions were inspected.
- The owner-held absence of an old-table migration for `requested_by_principal_id`, null internal requester when no active SYSTEM principal exists, and the open observation-judge authority decision were not treated as findings.

## Overall result

**FAIL**

REQ-004's actor/origin and judge-call attribution is implemented coherently, and the server-side REQ-005 path correctly links explicit inbound context. The candidate nevertheless violates binding OD-3 by adding implicit client-side trace injection across the shared HTTP transport and foreign-language verify bridges.
