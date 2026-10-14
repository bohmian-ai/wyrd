# Structured Ponytail finding validation

## Immutable subject

- Base: `ef03f624a1f3a6bcd51bbd17060271f0b5e8cabf`
- Candidate: `c4ba2b0502f8968a78e5e960bcc9eba43e960643`
- Approved specification: `changes/active/example-journey-fixes/spec.md`, revision 4
- Binding decisions: `changes/active/example-journey-fixes/owner-decisions.md`, OD-1 through OD-7
- Reviewed range: the complete base-to-candidate diff and the current source at the candidate

The candidate remained at the stated commit throughout validation. I inspected every discovery report and the focused follow-up, then traced each proposal through its current producer, callers, sibling consumers, durable sink, and focused proof. No production code, tests, documentation, task file, or approved authority was edited.

## Proposal validation

| Proposal | Result | Source validation and minimum boundary |
|---|---|---|
| `BEH-001` | **REVISED** | The path is reachable from the HTTP, MCP, and SDK direct-execution surfaces. `VerificationControl::execute` completes engine work before `stage_result`; `stage_result` returns without staging a row when `DirectTarget.system_principal` is absent (`crates/wyrd/wyrd-server/src/components/verification/service.rs:457-510,523-589`). This is not merely missing attribution: the whole result is lost while a successful judgment is returned. The classification is therefore `INCORRECT`. Refuse before engine work by requiring the existing `system_principal_id` result-writer identity during `resolve_direct`; do not substitute the authenticated caller as the Scribe SYSTEM publisher. Retained as `FIND-example-journey-fixes-1`.
| `BEH-002` | **CONFIRMED** | AC-004 explicitly requires the external example to run (`changes/active/example-journey-fixes/spec.md:192-194`). The task evidence contains no example revision, command, or result; repository-owned component journeys cannot prove the external application's composition or absence of its two forbidden workarounds. This is the same defect as `INV-REV-003`. Retained as `FIND-example-journey-fixes-2`.
| `INV-REV-001` | **CONFIRMED** | OD-3 expressly says no further client-side injection (`owner-decisions.md:50-55`). The new task-local reads ambient Rust context and foreign-runtime context (`crates/shared/wyrd-client/src/transport/trace.rs:18-64`), and the shared authenticated request builder adds it to every authenticated HTTP request (`crates/shared/wyrd-client/src/transport/http.rs:876-904`). Python and Node explicitly feed the path (`sdks/wyrd-sdk-python/src/observe/mod.rs:393-435`; `sdks/wyrd-sdk-ts/native/src/cards.rs:666-710`). The server inbound middleware already owns the approved behavior. This is the same defect as `OBS-001`. Retained as `FIND-example-journey-fixes-3`.
| `INV-REV-002` | **CONFIRMED** | OD-4 allows typed deserialization on the unified Jev route but explicitly forbids per-field validation (`owner-decisions.md:67-75`). `chat_request` deserializes `ChatQuestions` and then calls `checked` (`crates/wyrd/wyrd-gateway/src/adapter/jev.rs:63-95`), which invokes the custom non-empty/count/name bounds in `JevRequest::validate` (`crates/skald/skald-spec/src/wire/jev.rs:168-247`). Native `/v1/systemone` is a separate sibling that keeps those bounds. Retained as `FIND-example-journey-fixes-4`.
| `INV-REV-003` | **CONFIRMED** | Duplicate of `BEH-002`; the obligation and absent evidence have one correction boundary. Consolidated into `FIND-example-journey-fixes-2`.
| `MNT-001` | **CONFIRMED** | AGENTS.md requires `# Errors` on every touched fallible Rust method and `# Panics` where a panic remains (`AGENTS.md:713-727`). `JevClient::send` forwards a fallible operation without `# Errors` (`crates/skald/skald-providers/src/clients/jev.rs:134-139`). The new test provider's `send` can panic and its `send`/`stream` are fallible without the required sections (`crates/vala/vala-eval/tests/integration/orchestrator_judge_skald.rs:440-458`). The skill makes this hard rustdoc gate blocking. Retained as `FIND-example-journey-fixes-5`.
| `DATA-001` | **CONFIRMED** | The old constraint guarantees historical scheduled/observation rows had null requesters, while the new migration only swaps constraints (`crates/wyrd/wyrd-sql/migrations/20261014000001_verifier_run_requester_every_origin.sql:12-30`). The earlier SYSTEM migration provisioned one active SYSTEM principal for every existing ordinary tenant (`crates/wyrd/wyrd-sql/migrations/20260601000028_system_principal.sql:89-107`), so those historical actors are deterministically resolvable. The owner-held no-SYSTEM exception remains intact for the exceptional tenant where no principal can be resolved. Retained as `FIND-example-journey-fixes-6`.
| `OBS-001` | **CONFIRMED** | Duplicate of `INV-REV-001`; all affected runtime bridges feed the same shared-client producer. Consolidated into `FIND-example-journey-fixes-3`.
| `SEC-001` | **REJECTED** | The proposal assumes a malicious provider transforms a credential into an arbitrary reversible representation and then treats the representation as a Wyrd-returned credential. That is not a bounded regression introduced by the Jev change: any provider-controlled successful model output is the same unrestricted covert channel, and no finite scrubber can close it. The approved contract specifically requires relaying the 401/403 body after removing the credential (`owner-decisions.md:74-75`). The shared owner removes every literal occurrence before and after JSON decoding and suppresses bodies containing the standard-base64 forms that Wyrd capture can retain/decode (`crates/wyrd/wyrd-gateway/src/adapter/mod.rs:298-408`); Jev reuses that owner (`crates/wyrd/wyrd-gateway/src/adapter/http.rs:458-480`). Replacing the body with a fixed envelope would contradict OD-4, while adding an encoding blacklist would be speculative and incomplete. No specification revision is forced by this proposal.
| `SEC-002` | **CONFIRMED** | REQ-005 requires W3C `tracestate` handling and malformed headers to be ignored (`spec.md:139-146`). Ingress merely joins visible-ASCII values (`crates/wyrd/wyrd-server/src/http/middleware/trace_context.rs:52-70`), and `TraceContext::parse_traceparent` copies the state without validating it (`crates/wyrd-spec/src/trace.rs:19-44`). The malformed tests cover only `traceparent` (`trace_context.rs:115-129`). A valid parent must survive while invalid state is dropped. Retained as `FIND-example-journey-fixes-7`.
| `BIFROST-1` | **CONFIRMED** | REQ-002 names type mismatches and ambiguous references, across HTTP/SDK/MCP (`spec.md:85-93`). The real journeys cover misspelled columns in single/join/subquery forms in Rust (`sdks/wyrd-sdk-rust/tests/integration/query_bifrost.rs:296-330`), Python (`sdks/wyrd-sdk-python/tests/integration/test_query_bifrost.py:115-131`), and TypeScript (`sdks/wyrd-sdk-ts/wyrd/tests/integration/query-bifrost.test.ts:139-152`); MCP adds unknown table/function (`crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:299-376`). None makes DataFusion plan a real type mismatch or ambiguous reference, so the wrapper-sensitive mapper at `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3739-3797` is unproved for two required caller faults. Retained as `FIND-example-journey-fixes-8`.
| `BIFROST-2` | **CONFIRMED** | REQ-006 specifically names correlation on managed `run_id` (`spec.md:148-152`). Every language/MCP live journey correlates fixture `id` (`query_bifrost.rs:235-292`; `test_query_bifrost.py:82-112`; `query-bifrost.test.ts:114-137`; `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:383-456`), while the analytical workload deliberately uses `run_key` because `run_id` is reserved (`crates/wyrd/wyrd-testing/src/bifrost/support_workload.rs:22-23,394-502`). The generic fix is plausible but does not prove the required managed-column path. Retained as `FIND-example-journey-fixes-9`.

## Final deduplicated ledger

### FIND-example-journey-fixes-1 — REVISED — INCORRECT

- Discovery sources: `BEH-001`; follow-up section B.
- Violated obligation: REQ-004 and OD-1 require every direct execution to retain its authenticated actor on its verification result.
- Location: `crates/wyrd/wyrd-server/src/components/verification/service.rs:417-510,523-638`.
- Producer to consumer: `resolve_direct` produces an optional SYSTEM writer; `DirectExecutor` completes the verifier and any judge calls; `stage_result` drops the result on `None`; HTTP/MCP/SDK still receive `Ok(Judgment)`.
- Observable consequence: a caller can receive a successful execution ID and judgment for which `vala.verification.results` has no row, requester, or joinable judge evidence.
- Decision-complete correction: at the existing `resolve_direct` SYSTEM lookup, require the result-writer principal before returning `DirectTarget`. Return the existing `WyrdError::ServiceUnavailable` with reason `system_principal_missing` before `DirectExecutor` starts. Preserve caller authorization, preserve the caller as `ResultRun.requested_by`, preserve SYSTEM only as `VerifierAttribution.principal`, and do not change the approved nullable actor behavior for internally started work.
- Focused closure proof: use the existing tenant-without-SYSTEM fixture through the public direct endpoint; assert a stable 503, no provider/engine call, no successful judgment, and no result. Retain the existing direct-with-SYSTEM actor/result/judge-join test.

### FIND-example-journey-fixes-2 — CONFIRMED — MISSING

- Discovery sources: `BEH-002`, `INV-REV-003`.
- Violated obligation: AC-004 requires `bohmian-ai/wyrd-example-ts` to run clean without client-side escaping or an MCP tool allow-list.
- Location: `changes/active/example-journey-fixes/spec.md:192-194`; absent from the TASK-001 through TASK-005 evidence ledgers.
- Producer to consumer: the repository journeys prove individual server/SDK boundaries, but no recorded command composes them through the external application's package versions, setup, agent loop, telemetry, gateway, verification, Bifrost, and MCP investigation.
- Observable consequence: the exact user journey that motivated the change remains unproved and may still require either forbidden workaround.
- Decision-complete correction: run an immutable, owner-designated `wyrd-example-ts` revision against the cumulative candidate exactly as documented, without patches, escaping, or a tool allow-list. Record the example commit, commands/configuration, credential-gated limitation if any, and terminal result in the task evidence. If it exposes a product failure, fix only that owning path and rerun its focused lane plus the example.
- Focused closure proof: the pinned external example completes end to end and the evidence shows both workarounds absent.

### FIND-example-journey-fixes-3 — CONFIRMED — VIOLATION

- Discovery sources: `INV-REV-001`, `OBS-001`.
- Violated obligation: OD-3 forbids further client-side trace injection; OD-7 forbids unrequested complexity.
- Location: `crates/shared/wyrd-client/src/transport/trace.rs:1-64`; `crates/shared/wyrd-client/src/transport/http.rs:876-904`; `sdks/wyrd-sdk-python/src/observe/mod.rs:141-167,393-435`; `sdks/wyrd-sdk-ts/native/src/cards.rs:666-710`.
- Producer to consumer: ambient Rust/Python/Node trace state enters `with_caller_trace`/`outbound`; the shared authenticated request builder emits it for every authenticated cards, query, storage, admin, and verification request; the server then accepts it as inbound context.
- Observable consequence: the candidate changes unrelated SDK wire behavior and trace topology beyond the approved server-side inbound contract.
- Decision-complete correction: delete the new outbound trace module/task-local, shared authenticated-builder injection, Python active-trace bridge used for `verify`, Node trace parameters/handoff, and tests whose only purpose is automatic SDK propagation. Preserve the server inbound middleware, the MCP re-scope of already received headers, in-process inheritance, and explicit-header HTTP journeys. Do not add a replacement flag or transport abstraction.
- Focused closure proof: explicit valid `traceparent`/`tracestate` still parents gateway and judge spans; malformed inbound context remains non-fatal; a focused shared-client request made under an ambient span emits no synthesized trace headers.

### FIND-example-journey-fixes-4 — CONFIRMED — VIOLATION

- Discovery source: `INV-REV-002`.
- Violated obligation: OD-4 defines the unified Jev route as typed deserialization with no per-field validation.
- Location: `crates/wyrd/wyrd-gateway/src/adapter/jev.rs:63-95,111-120`; custom bounds at `crates/skald/skald-spec/src/wire/jev.rs:168-247`.
- Producer to consumer: the last user text decodes to the closed `ChatQuestions` shape, then `checked` applies native request bounds and can refuse before the Jev client sees it.
- Observable consequence: Wyrd rejects structurally typed unified-route requests that OD-4 requires it to pass through to the provider.
- Decision-complete correction: construct and return the typed `JevRequest` directly after unified-route deserialization. Keep `checked` on native `/v1/systemone`; keep closed-enum/unknown-field deserialization on the unified route; add no schema mapper or alternate request type.
- Focused closure proof: the unified route prepares a structurally typed request that violates one custom bound and reaches the provider, while malformed JSON and an unknown question discriminator still fail once before provider IO; native-route bound validation remains green.

### FIND-example-journey-fixes-5 — CONFIRMED — VIOLATION

- Discovery source: `MNT-001`; follow-up section A.
- Violated obligation: the hard AGENTS.md §16 rustdoc gate for touched fallible and panicking Rust methods.
- Location: `crates/skald/skald-providers/src/clients/jev.rs:134-139`; `crates/vala/vala-eval/tests/integration/orchestrator_judge_skald.rs:440-458`.
- Evidence and consequence: the concrete Jev provider boundary does not state its forwarded error contract, and the test provider hides deliberate errors and panic invariants; accepting it creates an explicit exception to a repository hard gate.
- Decision-complete correction: add `# Errors` to `JevClient::send`, the test provider `send`, and its `stream`; add `# Panics` to the test `send` for the poisoned capture lock and invalid static typed answer. Change no behavior and add no helper.
- Focused closure proof: `mise run fmt` plus the focused Jev provider and Vala judge tests.

### FIND-example-journey-fixes-6 — CONFIRMED — MISSING

- Discovery source: `DATA-001`; follow-up section C.
- Violated obligation: REQ-004 and OD-1 require SYSTEM actor attribution for every scheduled and observation-triggered verifier run when that tenant has SYSTEM.
- Location: `crates/wyrd/wyrd-sql/migrations/20261014000001_verifier_run_requester_every_origin.sql:1-30`.
- Producer to consumer: the old constraint produced null requesters for every historical internal row; the preceding SYSTEM migration already produced one same-tenant actor; the candidate changes only the constraint, leaving durable history under the old attribution regime for every query/result consumer.
- Observable consequence: one tenant's verifier-run history contains resolvable pre-migration internal actors as null and post-migration actors as SYSTEM.
- Decision-complete correction: in the existing migration, before installing the replacement constraint, update historical `schedule` and `observation` rows whose requester is null from the same tenant's active SYSTEM service account. Do not touch manual requesters and leave null only when no SYSTEM row is resolvable, preserving the explicit owner exception.
- Focused closure proof: a migration test seeds old scheduled/observation/manual rows, applies the migration, and asserts same-tenant SYSTEM backfill, unchanged manual attribution, tenant separation, and null preservation for an intentionally no-SYSTEM tenant.

### FIND-example-journey-fixes-7 — CONFIRMED — INCORRECT

- Discovery source: `SEC-002`; follow-up section E.
- Violated obligation: REQ-005 requires W3C `tracestate` and malformed headers to be ignored without making requests fatal.
- Location: `crates/wyrd/wyrd-server/src/http/middleware/trace_context.rs:52-70,91-129`; `crates/wyrd-spec/src/trace.rs:19-44`.
- Producer to consumer: ingress concatenates every visible-ASCII state; the parser validates only the parent and copies state verbatim; gateway capture retains the invalid state on the span.
- Observable consequence: an authenticated caller can persist non-W3C trace state, and standards-compliant downstream trace processors may reject or interpret the retained evidence inconsistently.
- Decision-complete correction: keep parent parsing in the existing inbound middleware and validate the combined state with the already-installed OpenTelemetry `TraceState` parser. If state parsing fails, keep the valid parent with empty state. Do not reject the request or add a new trace carrier/parser abstraction.
- Focused closure proof: focused middleware cases for invalid key/value grammar, duplicate keys, more than 32 members, member length bounds, and invalid repeated fields all preserve the valid parent but drop state; a valid repeated-header combination retains ordered state; existing malformed-parent cases remain ignored.

### FIND-example-journey-fixes-8 — CONFIRMED — MISSING

- Discovery source: `BIFROST-1`.
- Violated obligation: REQ-002 and AC-001 require real caller type-mismatch and ambiguous-reference planning failures to surface as actionable 4xx errors across shipped HTTP/SDK/MCP surfaces.
- Location: mapper at `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3739-3797`; missing cases in `sdks/wyrd-sdk-rust/tests/integration/query_bifrost.rs:296-330`, `sdks/wyrd-sdk-python/tests/integration/test_query_bifrost.py:115-131`, `sdks/wyrd-sdk-ts/wyrd/tests/integration/query-bifrost.test.ts:139-152`, and `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:299-376`.
- Producer to consumer: real DataFusion planning chooses the error wrappers; `caller_planning_fault` decides 4xx versus opaque 5xx; current journeys never produce either required missing shape.
- Observable consequence: both shapes can regress to a 5xx or lose actionable detail while every current test remains green.
- Decision-complete correction: extend the existing query fixtures and journeys with one planning-time type mismatch and one ambiguous qualified/unqualified reference. Reuse `caller_planning_fault`; if either exposes a missing typed wrapper, correct that shared mapper once. Add no new harness or query-shape guard and keep unauthorized/internal errors opaque.
- Focused closure proof: Rust, Python, TypeScript, and MCP/HTTP assertions receive `WYRD_VALA_400_QUERY_INVALID_SQL` plus the relevant planner detail for both real SQL cases.

### FIND-example-journey-fixes-9 — CONFIRMED — MISSING

- Discovery source: `BIFROST-2`.
- Violated obligation: REQ-006 and AC-001 require a live-table subquery correlated on Wyrd's managed `run_id`, compared with its equivalent join.
- Location: obligation at `changes/active/example-journey-fixes/spec.md:148-152`; substitute key at `crates/wyrd/wyrd-testing/src/bifrost/support_workload.rs:22-23,394-502`; current journey cases at the four paths listed in the proposal table.
- Producer to consumer: public run/observation recording stamps managed `run_id`; live schema/fragment planning projects it; the current tests bypass that path with ordinary `id`/`run_key` columns.
- Observable consequence: managed-column stamping, projection, or live-fragment correlation can fail while the generic expression tests pass.
- Decision-complete correction: reuse the existing public run/observation fixture path to write live rows sharing and differing in actual managed `run_id`; add one correlated scalar, `IN`, or `EXISTS` subquery and its equivalent join. Carry that same behavior through the existing Rust, Python, TypeScript, and MCP journey fixtures. Do not add a test-only SQL path or second query engine.
- Focused closure proof: every shipped surface returns identical expected rows for the managed-`run_id` subquery and join while the rows are still live.

## Recommendation

The validated ledger contains nine bounded findings. `SEC-001` does not require an owner decision and is omitted from the ledger; the retained corrections fit the approved specification and binding decisions. The appropriate verdict class is **FIX_REQUIRED**, not `SPEC_REVISION_REQUIRED` or `BLOCKED`.

Remediation should group corrections by their existing owners, without adding compatibility paths or new abstractions: verification direct readiness; external example evidence; deletion of outbound client propagation; Jev unified preparation; Jev rustdoc; verifier-run migration; inbound trace-state validation; and the two existing Bifrost journey fixtures. The broad task lanes need not be rerun inside remediation beyond the narrow write-set lanes; final change review owns the aggregate journeys.

## Verification limits

- I relied on the immutable task-command records and the two successful focused Bifrost reruns recorded by the Bifrost domain reviewer; I did not repeat Postgres, peer-cluster, Python, TypeScript, or external-provider lanes.
- No external `wyrd-example-ts` checkout or successful run was available; that absence is retained as `FIND-example-journey-fixes-2`.
- Live Jev smoke behavior remains credential-gated. This does not affect the source-proven unified-route violation.
