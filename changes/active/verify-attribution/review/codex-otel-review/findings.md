Reviewed the implementation after `99944fefb` through `aabccd933bbdeccbf1a627e27900466ec048a3b3`, including the uncommitted gateway changes present during inspection. Six findings are confirmed against current source and the pinned OpenTelemetry 0.31 / tracing-opentelemetry 0.32.1 sources. The principal failure is that Eval fan-out preserves tracing spans but loses the separate task-local gate used by canonical gateway attempt capture. The split sampler, routing registry, operator processor filter, and 2,048-permit cap are explicitly required by TASK-001 and are not reported as invented complexity. Scenarios 9–13 are excluded; Scenario 8 findings concern its present implementation, not unfinished work. This was a static review; no tests or builds were run.

| ID | Severity | Category | Location | Finding |
|---|---|---|---|---|
| F1 | high | otel-conformance | `crates/wyrd/wyrd-server/src/components/gateway/invocation.rs:491` | Eval judge attempts lose their continued trace despite the preserved tracing parent |
| F2 | medium | otel-conformance | `crates/shared/wyrd-telemetry/src/context.rs:48` | Inbound extraction rejects valid future-version traceparents |
| F3 | medium | otel-conformance | `crates/shared/wyrd-telemetry/src/context.rs:46` | `TraceState::from_str` is treated as a complete W3C header validator |
| F4 | medium | otel-conformance | `crates/shared/wyrd-telemetry/src/export.rs:155` | Sampling erases valid vendor state from the live trace context |
| F5 | medium | otel-conformance | `crates/wyrd/wyrd-server/src/http/router.rs:162` | HTTP request metadata uses fields the tenant projection discards |
| F6 | low | duplication | `crates/shared/wyrd-telemetry/src/context.rs:74` | Current-span context extraction duplicates the verification client's helper |

## F1 — Eval judge capture is still gated by task-local state

**Locations:** `crates/wyrd/wyrd-server/src/components/gateway/invocation.rs:491`; `crates/vala/vala-eval/src/executor.rs:424`; `crates/wyrd/wyrd-server/src/http/middleware/trace_context.rs:19`; `crates/wyrd/wyrd-server/src/components/gateway/capture.rs:370`.

**Problem:** Admission obtains the current `gateway.call` context only when `trace_context::active()` succeeds. The Eval executor spawns each task with `.instrument(tracing::Span::current())`, which carries the tracing span but does not carry Tokio's `INBOUND` task-local. A judge running in that spawned task therefore has the correct OTel ancestry but admission stores `trace: None`. Capture then leaves the attempt on its fallback `call_id` trace with no parent.

**Failure:** A sampled, traced direct Eval `llm_judge` produces exported request → verification → judge → gateway spans, but its canonical captured provider attempt is a disconnected root in another trace. The added `eval_judge_gateway_call_descends_from_the_request_span` checks operator tracing spans, not that captured row, so its assertions cannot detect this break. This violates the required synchronous parent chain and the Scenario 8 attempt-parent behavior.

**Smallest fix:** Determine whether the request continued a caller trace from request context that travels with the OTel `Context`/request span, then read the current gateway span through `wyrd_telemetry::current_context`. Remove the task-local prerequisite from capture admission. Preserve the required fallback for genuinely untraced calls; simply using every current server span would change that behavior. Extend the existing Eval journey to inspect its canonical captured attempt's trace and parent.

## F2 — Valid future-version traceparents start unrelated traces

**Locations:** `crates/shared/wyrd-telemetry/src/context.rs:48`; existing parser `crates/wyrd-spec/src/trace.rs:26`; callers `crates/wyrd/wyrd-server/src/http/router.rs:166` and `crates/wyrd/wyrd-tonic/src/server/mod.rs:234`.

**Problem:** The newly shared inbound path delegates to a parser requiring exactly four fields and version `00`. It rejects `01-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01`, and rejects additive future fields even when the known prefix is valid. W3C specifies forward-compatible parsing of the known prefix. [W3C versioning rules](https://www.w3.org/TR/trace-context/#versioning-of-traceparent).

**Failure:** A compliant upstream using a newer version silently loses continuity on HTTP, MCP, and gRPC; its request also becomes ineligible for the continued-trace tenant export.

**Smallest fix:** Use the installed SDK's `TraceContextPropagator` for edge extraction, retaining the task's duplicate-traceparent rejection and combined-tracestate carrier handling. Project the resulting `SpanContext` into the existing wire type where needed. Do not implement another version parser in telemetry.

## F3 — Tracestate validation rejects valid whitespace and accepts malformed lists

**Locations:** `crates/shared/wyrd-telemetry/src/context.rs:40` and `:99`. Dependency evidence: `opentelemetry-0.31.0/src/trace/span_context.rs`, `TraceState::from_str` and `TraceState::from_key_value`.

**Problem:** The pinned parser splits on commas without normalizing member whitespace. Thus valid `a=1, b=2` is dropped. Conversely, `a=1,a=2`, `a==1`, and lists exceeding 32 members pass: construction checks individual keys/values, not duplicate keys or member count, and parsing removes every leading `=` from the value. W3C permits surrounding whitespace and requires unique keys with at most 32 members. [W3C tracestate grammar](https://www.w3.org/TR/trace-context/#tracestate-header).

**Failure/cost:** Legitimate vendor state disappears at the edge, while malformed state is accepted as valid `TraceContext`. Current tests cover an uppercase key and missing separator but do not exercise these cases. The SDK propagator uses this same parser for tracestate, so adopting it alone does not close this finding.

**Smallest fix:** Normalize allowed whitespace/empty members and validate uniqueness, member count, and separators once in the shared inbound owner before constructing `TraceState`. Preserve the valid traceparent independently. Add the examples above to its existing parser tests; avoid repeating validation in HTTP, MCP, or gRPC adapters.

## F4 — Export policy is implemented by deleting live propagation state

**Locations:** `crates/shared/wyrd-telemetry/src/export.rs:155`; context projection `crates/shared/wyrd-telemetry/src/context.rs:82`; captured attempt propagation `crates/wyrd/wyrd-server/src/components/gateway/capture.rs:380`.

**Problem:** Every `SamplingResult` returns empty `TraceState`, including valid continued requests. The SDK uses that result as the new span's state, so `current_context()` and the newly reparented capture path see empty state. This sanitizes propagation itself to keep the operator export independent of client fields. The OTel sampler contract recommends preserving incoming state unless deliberately changing it, and W3C discourages mutating other vendors' state. [OTel sampler result contract](https://opentelemetry.io/docs/specs/otel/trace/sdk/#shouldsample), [W3C tracestate privacy rules](https://www.w3.org/TR/trace-context/#privacy-of-tracestate-field).

**Failure:** A request with `tracestate: wyrd=journey` retains its IDs but loses vendor state in all descendant span contexts and canonical attempts. The uncommitted gateway journey removes its former `trace_state` assertion rather than proving preservation.

**Smallest fix:** Return the parent's valid state from the sampler. Enforce the operator-content restriction on the operator export copy in the existing `OperatorExport` processor, without clearing the live context. Keep tenant projection governed by D5. Retain a valid-state assertion alongside the new gateway-parent assertions.

## F5 — Tenant HTTP SERVER spans lose their request metadata

**Locations:** `crates/wyrd/wyrd-server/src/http/router.rs:158`; `crates/shared/wyrd-telemetry/src/export.rs:410` (`safe_attribute`).

**Problem:** The changed SERVER span records `method`, `path`, and `version`. The tenant allowlist keeps the standard `http.request.method`, `http.route`, and `http.response.status_code`; none is recorded by this span or by a response callback on the installed `TraceLayer`.

**Failure/cost:** Tenant HTTP and MCP request rows named `request` cannot identify their HTTP method, matched operation, or response outcome from these attributes. Standard HTTP trace tooling also cannot consume the custom field names. [OTel HTTP server conventions](https://opentelemetry.io/docs/specs/semconv/http/http-spans/#http-server).

**Smallest fix:** Add `http.request.method`, the available matched route template as `http.route`, and an initially empty `http.response.status_code` recorded by the existing layer's response callback. Use the matched template, not the raw path. Preserve existing operator fields if required by its unchanged-content obligation; do not expand the tenant allowlist to custom/raw-path fields.

## F6 — Two identical current-span extraction implementations

**Every location:** new `crates/shared/wyrd-telemetry/src/context.rs:74` (`current_context`) and existing `crates/shared/wyrd-client/src/observe/verify.rs:171` (`current_trace_context`). Both obtain the current tracing span's OTel context, test validity, and construct the same four `TraceContext` fields with identical conversions.

**Cost:** Fixes to flag handling, state handling, or context projection must be applied twice; verification propagation and server continuation can diverge. This is duplicated implementation, not independent test evidence or a missing Scenario 9 capability.

**Smallest fix / owner:** Keep one shared projection implementation, owned by the shared telemetry context module, and make both call sites reuse it. Expose that context-only capability without pulling the server exporter/routing dependency cone into the client. No second parser, carrier type, or general context abstraction is needed.

No executable verification was run. The working tree was changing during the review; the listed implementation sites were rechecked immediately before writing this report.
