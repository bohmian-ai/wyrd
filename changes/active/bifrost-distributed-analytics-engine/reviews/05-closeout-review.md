# REMEDIATE

## Subject
- Base: 1461a4f7f
- Candidate: 910cbcd98
- Planning snapshot: 55060bf3e
- Evidence snapshot: 910cbcd98

## Verification
- Reused: Scenario 1–4 focused RED/GREEN evidence in `tasks/05-remediation-mcp-query-correctness.md`; final five focused commands pass at the candidate.
- Rerun: Read-only cumulative source review of the MCP handler, collector, Oracle planning mapper, discovery and query journeys, and shared query fixture.
- Not run: Broader verification is running; no approval is inferred from pending checks.

## Findings

### FIND-BIFROST-R5-T05-MCP-6 — MODERATE: Operational trace journey uses generic data
- Obligations: REQ-010, REQ-012; AC-003, AC-006, AC-009; original Task 05 Scenario 3 and Journey E.
- Locations: `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs::agent_debugs_otel_error_trace_through_mcp`; `crates/wyrd/wyrd-testing/src/bifrost/query_fixture.rs::seed_query_fixture`.
- Scenario: The agent's claimed OTEL debugging journey registers `vala.bifrost.agent_query_*` with `id/value`, then selects `value = 'second'`. It never ingests, discovers, describes, or queries `vala.traces.spans`.
- Consequence: The required operational trace flow can fail while this journey remains green.
- Supporting evidence: The shared fixture defines only Int64 `id` and Utf8 `value`, while the owning built-in trace schema exposes `name`, `status`, trace identity, and timestamps. The production OTLP HTTP route already accepts JSON at `/v1/traces` and Scribe registers built-in trace storage.
- Counterevidence: The existing journey genuinely proves the MCP protocol, Interactive result, ceilings, and deterministic cancellation for generic Bifrost data; those assertions remain valuable.
- Recommendation: Reuse `WyrdTestServer`, bootstrap/token exchange, existing `reqwest`, `/v1/traces`, `flush_bifrost`, and MCP connection in the same test; seed one OK and one ERROR span, discover/describe the actual trace table, and query the ERROR span. Retain ceiling and schema-stall cancellation assertions.
- Required outcome: A real OTLP-to-Bifrost-to-MCP operational debugging journey yields only the expected error span with an Interactive success terminal and settles every resource.
- Closure verification: Exact focused `query::pg_tests::agent_debugs_otel_error_trace_through_mcp` and `mise run test:bifrost:journey:mcp`, formatting, lints.

## Obligation Coverage
- The five original remediation findings have focused passing evidence. The operational OTEL journey obligation remains unproven.

## Prior Finding Closure
- FIND-BIFROST-R5-T05-MCP-1 through -5: source and focused test closure verified; broader verification pending.

## Routing
- Next skill: $wyrd-plan
- Finding IDs: FIND-BIFROST-R5-T05-MCP-6
