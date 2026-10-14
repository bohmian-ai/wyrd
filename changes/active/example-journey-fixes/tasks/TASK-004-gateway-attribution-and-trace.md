# TASK-004 Judge call attribution and trace linking

Spec: `../spec.md` REQ-004, REQ-005, INV-001, AC-001..AC-003.

1. When a Verifier runs in real time for a Run, the gateway calls it makes
   (for example `llm_judge` through `ServerWyrdGatewayCaller`, which today
   builds `GatewayCallRequest { subject: None, .. }`) are recorded in
   `vala.gateway.calls` with the Run's `run_id` and subject `card_uid`.
   Without a Run, leave attribution absent.
2. The gateway honors inbound W3C `traceparent`/`tracestate` on inference
   routes so its spans are children of the caller's span, and in-process
   calls made for a traced request inherit the active context. Malformed
   headers are ignored.

Evidence: journeys in Rust, Python, and TypeScript: a Run with an OTEL span
calls the gateway and a judge Verifier, then Bifrost shows the judge call with
the run_id/card_uid and the gateway span sharing the caller's trace_id;
negative cases for no Run and a malformed traceparent.
