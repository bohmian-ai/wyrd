# Security and Tenancy Domain Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/verify-attribution`
- Base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`
- Approved authority: `changes/active/verify-attribution/spec.md`, revision 3
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`

## Boundary and Authority Coverage

| Boundary | Governing authority | Source and consumer coverage | Result |
|---|---|---|---|
| Inbound W3C context | Spec REQ-005, INV-004; `wyrd-design.md` trace-context contract | `wyrd-telemetry/src/context.rs`; HTTP `request_span`; tonic `request_span`; malformed, repeated, and unsampled contexts | PASS |
| Authentication to tenant route | Spec REQ-007, D6, INV-001; `AGENTS.md` tenant isolation | HTTP `AuthenticatedPrincipal::from_verified`; gRPC `AuthContext::from_verified`; `TenantRoutes::{start,bind,end}`; unauthenticated-request coverage | PASS |
| Same-ID concurrency and route lifecycle | Spec AC-003; no client-selected tenant | Registry is keyed by the server span's `(TraceId, SpanId)`, not the inbound trace alone; entries leave on span end; concurrent same-trace tenant test uses two real tenants and principals | PASS |
| Tenant-safe export | Spec D5, D6, INV-002; `wyrd-design.md` telemetry safety | `TenantExport`, `in_catalog`, `safe_attribute`, `tenant_copy`; status descriptions and event detail are removed; resource is reduced to service name; tenant is not exported as an attribute | PASS |
| Server-internal Scribe path | `AGENTS.md` one-write-path and non-blocking rules; `bifrost-design.md` authenticated-tenant binding | `ServerSpanSink`, `ScribeWrite::ServerSpans`, `ScribeBatch::into_frame`, peer RPC validation, content-derived batch identity, fixed permit pool | PASS |
| Producer links | Spec r3 REQ-006/REQ-007; authenticated ingest boundary | Wire contexts are bounded to 32 consumers, ids and flags are validated, malformed/excess entries do not affect row admission, links carry no attributes or tracestate, and tenant identity still comes only from the verified token | PASS |
| Timeline query | Spec REQ-008, INV-001; Oracle authenticated tenant-scoped read authority | `Bifrost::trace_timeline` uses a typed `TraceId` and bound parameter over the existing `Bifrost::sql` path; Python and TypeScript parse locally; RLS/query-path isolation is exercised by the foreign-tenant journey | PASS |
| External storage exclusion | Spec REQ-006; credential and external-origin rules | Authenticated Wyrd HTTP requests attach context only after same-origin resolution; presigned upload traffic uses the external path and has a focused no-trace-header test | PASS |
| Deferred cause storage | Spec D8; tenant-scoped SQL rules | Existing unshipped verifier-run migration gains paired, length-checked trace/span columns; enqueue and claim remain on `TenantConn`; the stored cause is only a link | PASS |
| Dependencies | Supply-chain review rule | Rust additions reuse workspace-locked crates; TypeScript adds the already-used OpenTelemetry core package/version and keeps it optional for consumers; no new unpinned source or install script entered the range | PASS |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None required for task acceptance.

### Positive Controls

- Tenant and principal attribution are derived only after token verification; neither `traceparent` nor producer links can select a tenant or principal.
- An unauthenticated sampled request can reach the operator export but cannot bind a tenant route or create a tenant copy.
- The per-request sink is carried in the request context, avoiding process-global sink replacement and cross-server routing in shared processes.
- Client-controlled producer contexts can affect only causal links and tenant eligibility inside the already authenticated tenant; malformed entries are skipped and processing is capped at 32 entries.
- Tenant copies retain the operator's ids and parents but remove non-allowlisted span attributes, error descriptions, event detail, and process resource fields. Separately, the operator export removes caller `tracestate`.
- Server-span staging remains on the existing Scribe outbox, is capped at 2,048 outstanding copies, and does not consume permits reserved by audit, gateway capture, or verifier results.
- The timeline query binds the trace id as a parameter and inherits the existing authenticated Oracle limits and tenant authority rather than introducing a new route or query privilege.
- Presigned storage requests remain outside the authenticated Wyrd transport, so Wyrd bearer and trace headers are not disclosed to external storage origins.

## Verification Limits

This review inspected the immutable base-to-candidate source range and the task's recorded green evidence. Per orchestration instruction, it did not rerun Cargo, mise, database, SDK, or journey lanes. No separate penetration test of the peer mTLS listener or external telemetry backend was performed; both retain their existing trust models.

## Material Findings

None.

## Overall Result

**PASS** — the candidate satisfies the task's security and tenant-isolation obligations without a material authentication, authorization, injection, secret-exposure, cross-tenant-routing, or query-isolation defect.
