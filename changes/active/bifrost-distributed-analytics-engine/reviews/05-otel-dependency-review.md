# REMEDIATE

## Subject
- Base: 1461a4f7f
- Candidate: d4fdd6e6d
- Planning snapshot: d9a61c9fc
- Evidence snapshot: d4fdd6e6d

## Verification
- Reused: Primary correctness remediation's focused RED/GREEN evidence; actual trace journey RED and failed GREEN recorded in the OTEL task.
- Rerun: Read-only trace from authenticated HTTP OTLP ingestion through Scribe material planning, ingress, direct trace projection, and exact Arrow/IPC admission.
- Not run: Full change review; Task 05 has an unresolved required journey.

## Findings

### FIND-BIFROST-R5-T05-MCP-7 — MAJOR: Valid small trace exports cannot reach the required MCP debugging journey
- Obligations: AC-009, Journey E, original Task 05 Scenario 3; bounded resource admission remains mandatory.
- Locations: `crates/vala/vala-bifrost-redux/src/scribe/material_plan.rs::OtlpCounts::finish`; `scribe/ingress.rs`; `scribe/direct_traces.rs::validate_material_plan`; `scribe/otlp_managed.rs::OtlpManagedMaterialPlan::admitted_bytes`; `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs::agent_debugs_otel_error_trace_through_mcp`.
- Scenario: An authenticated caller submits two valid JSON OTLP spans with current timestamps, service identity, and one ERROR status to `/v1/traces`.
- Consequence: HTTP 413 blocks ingestion before an MCP agent can discover or query the spans. The returned remediation says to shrink the request even though the observed mismatch is in Scribe's planned material capacity.
- Supporting evidence: The exact real journey returns `WYRD_VALA_413_PAYLOAD_TOO_LARGE`, 8193 required bytes against a 2440-byte admitted limit. The preflight estimate uses request/value/managed-projection bytes; ingress supplies this estimate to direct projection's exact Arrow-plus-IPC check.
- Counterevidence: Exact projection admission correctly refuses to exceed its reservation. Existing generic-table MCP journeys do not exercise this path. This predates the MCP changes.
- Recommendation: Reuse the existing direct OTLP projection sizing and managed IPC owners to establish sufficient pre-admission material accounting in Scribe; preserve all reservation, persistence, and replay bounds. Plan this production dependency explicitly, considering sibling log and metric consumers of the shared estimator. Do not pad requests, bypass admission, or substitute a generic fixture.
- Required outcome: Valid small trace exports within configured bounds ingest durably, and the existing real trace journey discovers/describes/queries the error span while proving result ceilings and cancellation settlement.
- Closure verification: The exact OTEL MCP journey command in `tasks/05-remediation-otel-journey.md`, focused Scribe sizing/admission regression proof, and applicable Bifrost integration and MCP journey lanes.

## Obligation Coverage
- Input errors, terminal integrity, and active cancellation have focused proof. Actual OTEL debugging remains blocked at its production ingest dependency.

## Prior Finding Closure
- FIND-BIFROST-R5-T05-MCP-1 through -5: focused/source closure retained; broad verification recorded separately.
- FIND-BIFROST-R5-T05-MCP-6: actual trace coverage is now present but fails; not closed.

## Routing
- Next skill: $wyrd-plan
- Finding IDs: FIND-BIFROST-R5-T05-MCP-7
