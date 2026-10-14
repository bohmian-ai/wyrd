# Tenancy and security-disclosure domain review

## Immutable subject and result

- Wyrd base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Wyrd candidate: `dc353ef5ecf39da13f3dcac5b9b5a31558b6a35e`; HEAD verified during this audit.
- Fork range: `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b` to `f059acf6c58b2c5fd4bc36b3358e8a56ac283476` in the pinned Cargo checkout.
- Approved authority: `changes/active/bifrost-errors/spec.md`, especially REQ-002 and INV-001; no original task exists. Both prior remediation tasks and their committed evidence were inspected.
- Overall domain result: **PASS**.
- Material proposed findings: **none**.

This independent review covers confidentiality, authenticated table/tenant binding, execution-error disclosure, and new aggregate state ownership. General SQL completeness and optimizer performance belong to the separate DataFusion review.

## Authority and source coverage

| Changed boundary | Governing authority | Source evidence | Result |
|---|---|---|---|
| Query and nested relation authority | AGENTS.md tenancy/server rules; agent-rules SQL capabilities; security posture Authorization and policy | `oracle/mod.rs:3233-3276`, `oracle/planner.rs:99-139`, pinned SQL `resolve.rs` RelationVisitor | PASS |
| Actionable planning errors | REQ-002; `references/languages/errors.md`; security posture disclosure boundaries | `oracle/mod.rs:3000-3035,3735-3814`, provider registration `2624-2671`, scoped authorization `3426-3445` | PASS |
| Sensitive payload access | Security posture resolved-object authorization; OLAP serving tenant-safe paths | `oracle/mod.rs:3448-3513` optimized-plan traversal includes subqueries | PASS |
| Snapshot bytes and remote identity | INV-001; security posture Peer identity and distributed Oracle; Bifrost distributed analytical execution | `oracle/live.rs:298-384,668-752`; dispatcher `1934-1979`; receiving Scribe `peer_service.rs:236-359` | PASS |
| Cardinality refusal and aggregate state | Error boundary reference; OLAP serving fail-closed terminal and query-owned execution | fork `single_value.rs` accumulator and registration; scalar rewrite `build_join`; Wyrd `map_datafusion_error`, peer negative journey `live_subquery.rs:233-273` | PASS |
| Observation confidentiality | AGENTS.md test-support boundaries and dependency gate | `wyrd-server/src/state.rs:406-413,923-962`; `peer_service.rs:359,389`; dependency check production graph exclusion | PASS |

References read: reference router, errors, OLAP serving, and architecture constraints; governing security authorization, peer identity, tenant isolation, and Bifrost query/distributed sections. No SQL connection, identity issuer, policy, audit owner, or public Wyrd error catalog changes are present.

## Producer-to-consumer trace

### Authorization precedes diagnostic disclosure

`parse_select_tables` uses DataFusion's AST visitor, which visits nested queries and CTE inputs while keeping CTE aliases in their lexical scope. Actual relations are resolved through the catalog with `context.data_tenant_id`. `read_and_materialize` constructs tenant-qualified canonical bindings and registered UIDs, then calls `authorize_resolved_tables` before metadata materialization and provider registration. One denied relation refuses the whole query. Only admitted cut providers enter the fresh planning session; the changed logical mapper therefore cannot suggest a column of an ungranted table. Optimized payload access continues through `apply_with_subqueries` before physical execution; inserting an aggregate around the subquery does not remove its TableScan or this authorization gate.

`caller_planning_fault` unwraps the dependency's known context/diagnostic/shared/collection wrappers to an actionable typed logical-planning error. It does not traverse arbitrary external error sources. Resource exhaustion wins first. Physical-planning messages remain fixed, and runtime failures remain in the generic mapper. The newly generated `single_value` execution error therefore cannot expose values, table schemas, plans, file paths, or remote context through a logical-planning message.

### Scalar state does not become tenant authority

`SingleValueAccumulator` owns a query-local typed value and row count. Its allocation happens through the ordinary AggregateUDF accumulator constructor; no shared process map, tenant cache, or cross-query result store was added. Each partial state travels as ordinary aggregate Arrow columns, and merges into the receiving query's accumulator. The public default registration exposes the same pure aggregate operation to SQL callers; it performs no catalog lookup, identity resolution, network IO, or privileged read. SQL function visibility alone grants no additional data capability.

The scalar-to-join rewrite retains the authorized logical input. Its value subsequently enters the existing hash-join dynamic-filter machinery or existing scalar snapshot mechanism. `LiveScribeExec::resolved` selects subsets of frozen routes and preserves their binding. `fragment` puts that same binding in the outer descriptor and scan assignment and recomputes the fingerprint over the literal-bearing plan. Dispatcher claims retain the authenticated query tenant and permission digest. The Scribe checks peer context, local fence, exact fingerprint, assignment tenant/table, and assignment-authority digest before preflight, plan decode, and provider IO. No scalar value chooses a tenant, table, peer endpoint, or permission scope.

`Cargo.toml`, lock entries, and `ORACLE_DATAFUSION_REVISION` agree on the new fork revision. The existing native aggregate codec roundtrip test reconstructs the rejecting aggregate from ordinary registry semantics; no separate unbound extension protocol was introduced.

### Refusal and test observations

The fixed native cardinality message is mapped to existing opaque `QueryExecutionFailed`, preserving runtime confidentiality. The new peer journey checks both filter and projection through one authorized client and expects `WYRD_VALA_500_QUERY_EXECUTION_FAILED`; it does not weaken a timeout, authorization check, or assertion to clear the credential-provisioning failure. Whether this caller error should become 4xx is outside the approved planning-error requirement and is not a disclosure defect.

Resolved literals can contain tenant data, but the retained latest-fragment bytes and cumulative row tally are compiled only under `test-support`. Their record call runs after authenticated resolution. Production retains no new plan buffer or observation API; the normal server dependency gate excludes the fixture feature. Security/tenant failures retain terminal handling rather than degraded source omission.

## Proof and limits

- Source assertions were inspected in the Rust unauthorized-table journey, mapper test, remote literal/footer journey, new analytical filter/projection refusal journey, aggregate update/merge tests, and fork native codec roundtrip.
- Committed remediation evidence records passing focused journeys, codec/unit checks, dependency and tenant-isolation checks. It is available verification evidence, not an independently reproduced run in this discovery pass.
- No benchmark, heavy Cargo lane, external-environment test, or synthetic load was run, per the review boundary and owner constraint.
- No new cross-tenant journey was required by this audit: all changed value paths remain downstream of the same authenticated bindings and do not create a new identity authority. Existing public unauthorized-table proof plus source trace protects the changed diagnostic behavior.
- The previously accepted disclosure and tenant paths remain closed by current source evidence. General cardinality corner cases, extension-type metadata preservation, and distributed aggregate correctness are delegated to the separate DataFusion domain audit.

## Finding ledger

Empty: no consequential tenancy or security-disclosure finding identified in the cumulative candidate.
