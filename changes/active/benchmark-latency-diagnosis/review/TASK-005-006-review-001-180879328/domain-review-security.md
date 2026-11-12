# Security boundary review — TASK-005 / TASK-006

Result: **FAIL** — one required negative-flow proof is missing. No source-proven permission escalation or tenant disclosure was found in the reviewed paths.

## Subject and scope

Immutable base `0c81715f2`, candidate `180879328ac9dde94d7504855e821980e5db0f18`; full cumulative diff available and inspected for the assigned boundary. First review, no carried-forward acceptance. Source unchanged. Review followed the shared `review-scope.md` map, then expanded into the unchanged authenticated query, audit and result-reading owners.

Reviewed: TASK-005 revision-6 compact/details read boundary, historical descriptive decoding, HTTP/MCP dispatch, shared Rust client and Python/TypeScript projections; changed supplied Google access-token client construction; TASK-006 retention of authentication/authorization/capture boundaries around local admission. Persistent contribution RLS/concurrency is assigned to the data/invariant reviewers and is not claimed here.

## Authorities and inspected owners

| Authority / boundary | Source evidence |
|---|---|
| Spec REQ-023–027, AC-019–022; TASK-005 revision 6 scenarios 10–14 | `changes/active/benchmark-latency-diagnosis/spec.md:303–331`, task scenario 12 at `:1108–1126` |
| Security permission model and decision/audit | `architecture/wyrd-security-posture.md` Authorization and policy / Permission check / Audit integrity and privacy; `src/audit/mod.rs::authorize`, `authorize_recording_denial` |
| Verified caller tenancy and ordinary authenticated Bifrost | security Tenant and data isolation / Postgres tenant binding; server `query/scheduled.rs::ScheduledQueryCaller::{authenticated,dispatch,run_with}` |
| Status/details shared authority | server `components/verification/service.rs::{get_execution,get_execution_details,read,direct_execution}` |
| Public exposure | server `components/verification/routes.rs::{get_execution,get_execution_details}`; `mcp/verification.rs::{descriptors_unscoped,mcp_get_execution_details}` and `mcp/mod.rs` dispatch |
| Retained evidence reader | server `verification/inputs.rs::InputResolver::{result,direct,records,rows,evidence,slots}`; `verification/results.rs::{StoredOutput,ResultPayloadBuilder}` |
| Non-executable history | `wyrd-spec/src/card/agent.rs::{PromptForm,Authored,Compiled,AgentSpec}`; `card/verifier/mod.rs::{AgentCall,Check}`; `verification/execution.rs::{VerificationDetails,OutcomeDetails,RequirementDetails}` |
| SDK behavior | shared client `verify/execution.rs::{details,details_typed,status,wait}`; Python `src/verify/mod.rs::PyExecution`; TS native `verification.rs::NativeExecution` and wrapper `wyrd/src/index.ts::Execution` |
| Adjacent changed credential/admission boundary | providers `auth/google_oauth.rs::{new,token,http,token_from_json,token_from_metadata}`; server gateway `invocation.rs::{admit_call,admit,settle_buffered,settle_stream}` |

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| Details require existing Card and Bifrost authority, not verifier:run | Both projections call `read<O>`; `audit::authorize(Permission::card_read())` precedes lookups; `ScheduledQueryCaller::authenticated` dispatches through public query admission | `pg_verification_routes::details_preserve_visibility_and_read_authority`: viewer allowed, outsider/run-only denied, decisions audited; MCP `execution_details_are_an_explicit_read` | PASS |
| Same tenant/non-disclosure rules | `registry_tenant_conn(caller.data_tenant_id)` and authenticated query context; direct lookup masks only the pre-existing permission-denied path as absent; no caller-controlled tenant selector | HTTP foreign rootless administrator gets exact execution-not-found; shared reader used by MCP | PASS |
| Audit decisions remain non-blocking and attributed | `read` passes operation `verification.execution.details.read` to existing `audit::authorize`; audit owner stages allowed/denied on process Scribe outbox | HTTP allowed/denied audit assertions | PASS |
| Descriptive read does not compile/render/execute Prompt or rejudge | `get_execution_details` selects `OutcomeDetails` default `Authored`; `Authored::from_authored` returns data; `result` decodes retained JSON and evidence through existing owner | spec test `execution_details_preserve_authored_checks_without_compilation` includes an unresolved response format and round-trip fidelity; HTTP repeated read; SDK provider-count checks | PASS |
| Compact judgments retain identity and omit checks/provenance/steps/evidence | Compact read selects `VerificationOutcome`; details reconstruct same envelope and share result reader; explicit `.compact()` | HTTP `details_preserve_retained_execution_evidence`; MCP explicit-read test; SDK separate-read tests | PASS |
| Absent complete detail returns stable 404, errors are not successful empty evidence | `result` joins summary/output/input and returns None if no rows; missing group source returns None; `get_execution_details` maps absence to execution-not-found; query errors propagate except existing table-not-found empty-read rule | Unknown-execution HTTP/MCP 404 tested, but required known incomplete/lost/recording-failure scenarios absent (SEC-001) | FAIL (proof) |
| Direct results do not await details or gain hidden detail IO | `stage_direct` stages in memory and logs build failures; direct execution returns `.compact()`; SDK details is separate explicit request, status/wait do not invoke it | separate-read SDK tests establish no rejudgment; required direct recording-failure proof absent (SEC-001) | FAIL (proof) |
| Supplied Google token avoids unused client without widening trust | Only AccessToken source stores None; token match returns existing secret; network sources all retain client and unchanged refresh/exchange/metadata paths | Existing sensitive-bearer test and new `access_token_source_needs_no_network_client` inspected | PASS |
| Local governance does not replace permission/credential/capture gate | Cached policy still passes `decide`, route and capture reservation before local admit; settlement does not change authorization | Changed invocation path inspected; broad local-governance acceptance belongs to other selected specialists | PASS within security scope |

## Proposed finding

### SEC-001 — MISSING: required incomplete-detail and recording-failure journey proof is absent

- **Task:** TASK-005.
- **Obligation:** spec AC-021 explicitly requires real-server journeys for not-yet-visible/missing detail and a useful completed direct verdict when recording fails; TASK-005 scenario 12 (`:1108–1126`) names missing records and recording failure in the planned `details_preserve_visibility_and_read_authority` proof.
- **Exact location:** `crates/wyrd/wyrd-server/tests/integration/pg_verification_routes.rs:2595–2697` (`details_preserve_visibility_and_read_authority`); task's evidence row at `:1355` claims authority/audit/absence PASS from that test.
- **Observed proof:** the test waits for a visible direct result (`:2610`) before all reads. Its missing-data request uses a new unknown execution ID (`:2617–2630`); remaining requests cover successful viewer read, cards:read denials and foreign tenancy. It never has a known queued execution whose result/input/output is not visible, removes/withholds a required retained record, or fails direct result recording. The other `/details` calls in server runtime tests read successfully published detail. MCP/SDK explicit-read tests cover visible detail/unknown IDs or decoding failures, not these missing publication scenarios.
- **Consequence and materiality:** this is an explicit required acceptance proof gap, not a claimed runtime exploit. The new API distinguishes an unknown execution from a known execution lacking complete historical evidence, and must preserve a completed direct decision when history cannot be written. A random unknown ID does not exercise either changed consumer branch. This leaves the approved absence/lifecycle boundary unproved while evidence labels it passed.
- **Smallest correction:** extend the existing real-server verification route/runtime fixtures with bounded negative scenarios at the shared HTTP service owner: (1) known queued status with incomplete visible retained detail, including a required record unavailable after a completed result, returns the stable details 404 while status retains its existing lifecycle/optional-result behavior; (2) direct execution whose result recording fails still yields the useful compact verdict, with explicit detail read returning the existing absence error. Reuse existing publication/fault controls; no new availability state, retention mechanism, privileged reader or runtime redesign. Correct the evidence row to match actual proof. No additional copy of these scenarios per SDK/MCP is required because they call the same service.
- **Focused closure proof:** the new/extended exact real-server selectors, run via existing repository Postgres wrapper and narrow server integration lane. Assert stable code plus preserved direct result/status, rather than timing sleeps or manufactured empty detail.
- **Correction burden:** small test/evidence-only extension. Runtime implementation appears to satisfy the desired behavior statically; do not change it unless the focused proof exposes a defect. Deleting the obligation or relying solely on source inspection would weaken the explicit journey contract.

## Verification limits

Static review only; no test suite, benchmark, profile, broad build, external provider or new hardware experiment was run. Existing test names above identify inspected source and reported implementation evidence, not fresh execution results. No additional security hardening or stricter-than-approved quota durability is proposed.
