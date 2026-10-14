---
id: SPEC-verify-attribution
revision: 1
status: approved
---

# Verifier attribution and gateway trace linking

## Objective and user value

Every Verifier execution and every gateway model call records who asked and which run and Card it belongs to, and gateway spans join the caller's trace.

## Approval

Approved by the human owner on 2026-10-14 as part of SPEC-example-journey-fixes
r6. Split into its own change by owner direction on 2026-10-14 so each problem
is reviewed and verified on its own. The requirement text is carried over
unchanged.

## Required behavior

### REQ-004 Verifier execution attribution

Owner directive 2026-10-14 (attribution from execution context). Verifiers may
execute at any time, not only within a Run, so attribution derives from the
execution's context, not from whether a Run exists.

- If an invocation carries a `card_uid` and/or a `run_id`, it is recorded;
  the requesting principal is always recorded. `run_id` and `card_uid` are
  independent: either, both, or neither may be present, an absent one is
  recorded as null, and neither is inferred or required to record the other.
- A gateway call in `vala.gateway.calls` records its `caller_principal_id`
  plus whichever of the `wyrd-run-id` and `wyrd-card-uid` headers it carried.
  The gateway's existing card-scope authorization applies to a `card_uid`
  whenever one is present.
- Every gateway model call a Verifier execution makes (for example an
  `llm_judge` call through `ServerWyrdGatewayCaller`) records the
  execution's subject Card as `card_uid` and its `run_id` when present, for
  direct, observation, and scheduled executions alike.
- Every Verifier execution records its actor as
  `requested_by_principal_id` on the verification result and on
  `wyrd.verifier_runs`. Actor = the authenticated caller for a manual run or
  a direct execution; actor = the tenant SYSTEM principal for work Wyrd
  starts internally (a scheduled or observation-triggered run). Origin =
  references only: `run_id`, subject `card_uid`, `source_record_id`, and the
  binding, owner, and Trigger; no second principal (such as the observation
  writer) is copied onto runs or results. An observation-triggered
  execution's span links to the observation's stored trace and span when the
  record has them. The result joins its judge calls on
  `subject_card_uid` = `card_uid`.
- No binding enforcement or new authorization is added; direct-execution
  authorization is unchanged.

### REQ-005 Trace linking

- The gateway honors W3C `traceparent` (and `tracestate`) on inbound
  inference requests: its spans become children of the caller's span, so an
  application span and the gateway's model-call span share a trace ID in
  `vala.traces.spans`. In-process calls made on behalf of a traced request
  (such as judge calls) inherit the active trace context. Malformed headers
  are ignored, never fatal.

## Invariants

- INV-001 Tenant isolation holds on every changed path.
- INV-002 Provider credentials and keys are never logged or returned.
- INV-003 Audit stays non-blocking; no new audit failure mode.

## Acceptance obligations

- AC-001 Each REQ has a user-journey test (Rust, Python, and TypeScript where
  the surface ships in all three; MCP/HTTP where agent-facing) that fails on
  the base and passes on the candidate, booting the server the way users do
  where the defect was harness-masked (REQ-003 uses the official TypeScript
  MCP SDK; REQ-007 drives the CLI).
- AC-002 Negative flows: Jev invalid question type and upstream 400/429/5xx;
  Bifrost unauthorized-table errors leak nothing; malformed `traceparent`.
- AC-003 Format, lints, codegen check, and the owning `mise` lanes pass.

## Open material decisions

None.
