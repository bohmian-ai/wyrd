---
id: SPEC-verify-attribution
revision: 3
status: approved
---

# Verifier attribution and distributed trace continuity

## Objective and user value

Wyrd records and verifies the behavior of AI systems. Every Verifier execution
and every gateway model call records who asked and which Run and Card it
belongs to. A client's trace continues through every server action taken on its
behalf, so one query reconstructs a Service's full client → server → client
timeline: its own spans, its observations and Bifrost writes, gateway model
calls, and real-time verification including the judge calls inside it. The
wyrd-server operator keeps a complete view of the same server spans in the
configured trace backend.

## Approval

Revision 1 was approved by the human owner on 2026-10-14 as part of
SPEC-example-journey-fixes r6 and split into its own change the same day.

Revision 2 is drafted from owner direction on 2026-10-14: close out the
remaining revision 1 work; export server spans to both the operator and the
tenant; make every internal service that continues a client action follow OTel
distributed-tracing conventions, including verifier → judge → gateway;
correct the `Wyrd-Request-Id` wording; ship a standard trace-timeline helper
and an action-timeline user journey that also confirms the operator view; and
make every client SDK inject trace context. The decisions D1–D8 below turn that
direction into contract. Revision 2, including D1–D8, was approved by the
human owner on 2026-10-14: "i approve". SPEC-sub-act-attribution lands after
this change and builds on it.

Revision 3 resolves a conflict between AC-002 and REQ-007 found in review:
queued Bifrost writes reach the server with their producer contexts as
links and no `traceparent`, so their CONSUMER span never reached the tenant.
A linked ingest is now tenant-eligible as its own linked trace. The human
owner approved it on 2026-10-14: "i agree. go with the recommendation".

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

### REQ-005 Server trace continuity

- Every wyrd-server request that carries a valid W3C `traceparent` (HTTP
  header, MCP over HTTP, or gRPC metadata) continues that trace: the server's
  request span is a child of the caller's span with kind SERVER, and every
  span created while serving the request descends from it, across every task
  the request spawns or hands work to while the caller waits.
- This covers at least the verifier runtime (direct and MCP execution, Task
  and Eval verifiers), the Eval `llm_judge` invoker, the in-process gateway
  call and its provider attempts, Bifrost ingest (`insert_batch`, OTLP
  ingest), and Bifrost query. A judge's gateway call is a descendant of its
  `verification.*` span, which is a descendant of the request span.
- Work that runs after the request answered is deferred: a queued or
  observation-triggered verifier run, the Scribe outbox writer, Scribe
  commits, and Forge maintenance. Deferred work starts its own trace and
  carries an OTel span link to the context that caused it when one is known:
  the observation's stored trace and span for an observation-triggered run,
  and the enqueuing request's context for a manually enqueued run.
- A missing, malformed, or repeated `traceparent` starts a new trace and
  never fails the request. A malformed `tracestate` is dropped on its own.

### REQ-006 Client SDK trace propagation

- The Rust, Python, and TypeScript SDKs send the caller's active trace
  context (`traceparent`, and `tracestate` when non-empty) on every request
  they make to wyrd-server: HTTP, MCP, and gRPC. The Rust SDK reads the
  current `tracing` span's OpenTelemetry context; Python and TypeScript read
  the active OpenTelemetry context at the binding entry. Requests to external
  presigned storage URLs carry no Wyrd headers, trace context included.
- Each SDK's `start_telemetry` installs the W3C trace-context propagator
  alongside its tracer provider.
- A queued Bifrost write (`record`, `observe`, table `insert`) records a
  PRODUCER span in the caller's trace at enqueue time. The batch that later
  carries it sends the producer contexts of its rows, and the server's ingest
  span (kind CONSUMER) links to each, following the OTel messaging batch
  convention.
- No trace context means no trace headers; the request is otherwise
  unchanged.

### REQ-007 Dual export of server spans

- Every server span is exported to the operator's configured trace backend
  exactly as before this change, under the server's own sampler. Client
  context never changes what the operator receives or how it is sampled.
- A server span that belongs to a request continuing a sampled client trace
  (REQ-005) is also written to the requesting tenant's `vala.traces.spans`,
  staged on the process Scribe outbox like every other server-internal
  Scribe write. The tenant copy has the same trace id, span id, and parent
  as the operator copy, so the client's spans and the server's spans form one
  tree.
- The tenant copy carries only the behavior-span catalog and safe attributes
  of D5. It never carries SQL text, internal hosts or addresses, storage keys
  or paths, credentials, or another tenant's identifiers.
- The tenant export works whether or not an operator endpoint is configured.
- An authenticated Bifrost ingest whose batch links at least one sampled
  producer context is also tenant-eligible. Its CONSUMER span and the catalog
  spans below it are written to the requesting tenant, linked to the
  producers. Ids, parents, and links are the operator's, as above; the
  consumer's parent is a server span the tenant does not receive. The
  operator copy is unchanged.
- Spans of deferred work, of requests with no client context and no sampled
  producer link, and of requests whose client trace is not sampled go only
  to the operator.
- The gateway's captured attempt spans stay the canonical attempt rows; under
  a continued trace their parent is the exported gateway call span, and no
  attempt is written twice.
- A tenant-export staging failure or drop is logged and counted and never
  fails or slows the request.

### REQ-008 Trace timeline read

- The Rust, Python, and TypeScript SDKs expose one standard read that returns
  a trace's timeline from `vala.traces.spans`: every span of the trace plus
  the spans of traces that link to it, ordered by start time, each with its
  trace id, span id, parent span id, links, name, kind, start and end time,
  status, service name, and attributes.
- The read goes through the existing authenticated Bifrost query path and
  sees only the caller's tenant.

### REQ-009 Documented correlation model

- `architecture/wyrd-design.md` describes `Wyrd-Request-Id` and W3C trace
  context as complementary: the request id identifies one logical request
  for audit and replay; trace context orders behavior causally across
  requests and services. Neither replaces the other.
- `architecture/wyrd-design.md` and `architecture/bifrost-design.md` describe
  the dual export, the behavior-span catalog, and server-span rows in
  `vala.traces.spans`.

## Decisions

- **D1 Trace context is the timeline correlator.** W3C Trace Context is
  adopted for causal ordering. `Wyrd-Request-Id` keeps its role for request
  identity, audit, and replay. The "sole correlator … does not depend on
  `traceparent`" wording is retired.
- **D2 Parent, not link, for synchronous work.** An inbound context is the
  parent of the server request span so the tenant timeline is one tree.
  Deferred work uses span links (REQ-005).
- **D3 Every SDK request propagates.** Supersedes the example-journey-fixes
  OD-3 decision that limited client propagation to `verify`.
- **D4 Queued writes use the messaging convention.** PRODUCER spans at
  enqueue, CONSUMER ingest span links per batch. The ingest request contract
  gains a bounded list of producer trace contexts.
- **D5 Tenant export is an allowlist.** Behavior-span catalog: the request
  span, `verification.*`, `dispatch`, `workflow.gateway.call`, `gateway.*`
  except `gateway.attempt` (covered by the captured attempt rows),
  `bifrost.gate.*`, `bifrost.oracle.query`, and the OTLP ingest spans. Safe
  attributes: `wyrd.*` correlation fields, `gen_ai.*`, `http.request.method`,
  `http.route`, `http.response.status_code`, `rpc.system`, `rpc.service`,
  `rpc.method`, `rpc.grpc.status_code`, `error.type`, span status, and events
  limited to `exception.type`. The operator export is unchanged.
- **D6 Tenant rows are attributed to the requesting principal.** A tenant
  server-span row is stamped with the authenticated caller of the request it
  continues, so a Service's timeline is reachable from its principal. The
  tenant is never a span attribute.
- **D7 Sampling is split.** The operator export follows the server sampler.
  The tenant export follows the client's sampled flag.
- **D8 Deferred-run context is persisted.** A manually enqueued verifier run
  stores its enqueuing request's trace context with the run so its execution
  can link to it. This rides in the change's existing unshipped migration.

## Invariants

- INV-001 Tenant isolation holds on every changed path, including tenant span
  export and the trace timeline read.
- INV-002 Provider credentials and keys are never logged or returned.
- INV-003 Audit stays non-blocking; no new audit failure mode.
- INV-004 Client-supplied trace context never changes the operator export's
  trace content or sampling, and never makes a request fail.
- INV-005 Tenant span export is non-blocking: staging never waits on Scribe,
  and a loss is counted, never surfaced to the request.

## Acceptance obligations

- AC-001 Each REQ has proof at the tier AGENTS.md requires: user journeys in
  Rust, Python, and TypeScript where the surface ships in all three, and
  MCP/HTTP where agent-facing.
- AC-002 The action-timeline journey, in each SDK: a Service instrumented
  with the SDK's telemetry starts a client trace, records client spans,
  writes Run observations and a Bifrost table row, makes a gateway model call,
  and verifies in real time with an Eval `llm_judge` Verifier. The trace
  timeline read returns one connected, time-ordered tree containing the client
  spans, the PRODUCER/CONSUMER-linked Bifrost writes, the gateway call and
  attempt, and verification → judge → gateway. The operator's captured export
  contains the same server spans with the same ids.
- AC-003 Negative and edge flows: malformed `traceparent` and `tracestate`;
  an unsampled client trace exports nothing to the tenant; a second tenant
  cannot read the first tenant's timeline; tenant rows carry no disallowed
  attribute; deferred runs link rather than parent; no context sends no
  header.
- AC-004 The revision 1 closeout: an Eval-kind Verifier's judge call joins the
  caller's trace; an observation-triggered judge call records `card_uid` and
  `run_id`; Eval and observation results join their judge calls on
  `subject_card_uid` = `card_uid`.
- AC-005 Format, lints, codegen check, `check:deps`,
  `check:tenant-isolation`, and the owning `mise` lanes pass.

## Open material decisions

None.

## Revision history

- r1 (2026-10-14): approved; attribution and gateway trace linking.
- r2 (2026-10-14): approved; server trace continuity, SDK propagation, dual
  export, trace timeline read, correlation-model wording, r1 closeout.
- r3 (2026-10-14): approved; a linked ingest's CONSUMER span is
  tenant-eligible as its own linked trace (AC-002 / REQ-007 conflict).
