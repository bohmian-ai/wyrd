# Tenancy and security disclosure review — resumed round 4

## Subject and scope

Independent domain review of cumulative Wyrd task base after integration
`945a1548c47897dc94583169a6ba04163a73a749` to candidate
`17697dc8c2740213565e5f913cde1edfb0cb8ce5`, retaining original task base
`526355fb8c58dae6d67d516f98dddcd5c02b0e79` and pre-merge review source
`7fe69545d0d8a2ab2a1e3033f52d5091e80d5b0e` as historical context, and DataFusion
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b` to pinned
`b2d37bc182b9d3e576650a414b364f3d9b5aefe3`. The latest fork delta was read
separately to locate the row-bound correction; conclusions use cumulative
production source and actual tests, not the implementation message.

Authority: approved spec REQ-002/REQ-006/INV-001; AGENTS and agent-rules;
reference router, spec-driven development and maintainer style; security
posture trust boundaries and tenant/data isolation; Bifrost design's query
permission boundary and descriptive sensitive-column metadata; OLAP serving,
DataFusion and public-error references. CodeGraph was used before source
navigation. This review is limited to security/tenant/disclosure consequences;
the independent DataFusion review owns complete relational semantics.

## Boundary coverage and evidence

| Boundary | Actual source/producer-to-consumer evidence | Result |
|---|---|---|
| Nested and CTE table discovery | `oracle/mod.rs::parse_select_tables` uses DataFusion `resolve_table_references(statement, true)`; canonical relations enter `OraclePlanner::pin_cut`; its `read_and_materialize` derives every identity from the authenticated tenant and checks the complete table set before materialization. `query_floor_extracts_joined_and_cte_tables` pins CTE and outer-join inputs. | PASS |
| Authorization before SQL diagnostics | `planner.rs::read_and_materialize` invokes `authorize_resolved_tables` before provider registration; `build_physical_root` registers only cut providers before `plan_logical` can expose a message. One uncovered canonical table denies the whole query. Rust `unauthorized_table_refusal_reveals_no_columns` checks an ungranted joined table with a misspelled column returns 403 without either spelling. | PASS |
| Error disclosure classification | `map_sql_planning_error` exposes only SQL/Plan/Schema/NotImplemented leaves through known wrappers; resources take precedence; external/internal/execution leaves remain opaque. `map_query_planning_error` always uses a fixed physical-planning repair message. `map_datafusion_error` keeps runtime scalar refusal opaque. The changed mapper test separately checks SQL detail, physical redaction, wrapper redaction, internal failure and exhaustion. | PASS |
| Payload read authority | `authorize_payload_columns` traverses logical subqueries and refuses gateway payload projection/filter without its additional permission; the ordinary table authorization still runs first. Bifrost design explicitly describes sensitive metadata as descriptive, with the additional gateway permission controlling payload reads, rather than concealing its schema. No changed source opens payload IO before this gate. | PASS |
| Per-query scalar and aggregate state | `ScalarSubqueryExpr` retains its execution-owned `ScalarSubqueryResults`; snapshots create a literal with the field metadata. `single_value` constructs a fresh accumulator per aggregate execution/group and exchanges value/count through native aggregate state; it introduces no process-global tenant result cache or shared cross-query store. Its second-row error contains no value, table, path or credential. | PASS |
| Native cardinality refusal across workers | `ScalarSubqueryToJoin::build_join` places `single_value` around uncorrelated inputs whose native `max_rows` proof is unknown or greater than one; accumulator merge counts partial rows, rather than trusting a single partition. Latest FULL/grouping bounds conservatively become unknown. The negative peer journey uses one client, one registered same-tenant table, published/live input and FULL/grouping scalar expansions, requiring the opaque execution-failure code. | PASS |
| Lazy live fragment snapshot and assignments | `LiveScribeExec::resolved` retains the original binding, assigns routes from the planned list by partition before narrowing, and snapshots only predicates. `fragment` takes the same binding, schema fingerprint, projection closure, writer epoch and partition into each follower assignment; serialized predicate changes are included in the native plan fingerprint. No predicate can replace tenant identity. | PASS |
| Signed remote receiving boundary | `live_dispatch` derives tenant from `AuthorizedQueryContext`, plus the scoped permission digest and admitted identity/fence/deadline. `ScribeFragmentExecutor::execute` verifies ticket before decode/IO, compares tenant/binding/fences/fragment identity, recomputes assignment-authority digest, and runs `authenticated_preflight`. Cumulative changes do not bypass or weaken any of those checks. | PASS |
| Test observations and production reachability | New footer-row and last-plan retained state/accessors in server `state.rs` are `cfg(feature = "test-support")`; production calls retain only existing execution/footer counters. The native tally's row getter exposes a count, not data or credentials. The dependency check forbids test tooling in production packages; no new public endpoint exposes stored fragment bytes. | PASS |
| Cross-language/user surfaces | Cumulative Rust/Python/TypeScript/MCP journey additions use the shared Bifrost transport/error projections; they do not establish independent tenant identity or client authorization. Production SDK transport and wire contracts are unchanged. | PASS |

## Findings and prior closure

### Merge seam revalidation

The resumed task diff retains the same Bifrost production owners and test
coverage. The actual pre-merge-to-current diff is empty for the entire Oracle
module, server Scribe peer service, server MCP Bifrost handler, MCP query
journeys and peer-cluster Bifrost harness. Consequently the authority and
producer/consumer evidence in the table remains current rather than relying
on unchanged-looking summaries.

The relevant merged changes were inspected explicitly:

- `AppState` removes gateway-specific managed-key fields and uses the shared
  authentication sealing owner. Its Bifrost runtime, query authority and
  test-support fragment observations are not relocated or widened.
- `WyrdTestServerBuilder` still creates the Bifrost runtime before `AppState`,
  installs issuing/verifying authentication plus `RbacCheck`, and preserves
  the fixture's authenticated tenant. Gateway key resolution now consumes
  `state.auth.sealing_key`; the gateway endpoint helper adds Jev only. Neither
  supplies query tenant authority or alters peer ticket/fragment ownership.
- MCP catalog changes narrow gateway write discovery by caller permission
  and make caller-dependent list metadata private with zero TTL. Bifrost
  descriptors remain in the read catalog, and Bifrost dispatch still derives
  the caller from verified request context before list/describe/query.
- Updated security posture expands deployment sealing-key coverage to
  gateway managed secrets with tenant/name/provider-bound envelopes. This
  does not change Bifrost tenant binding or diagnostic rules. No unrelated
  upstream gateway or MCP feature is audited as a new Bifrost finding.

No merge conflict changes these security conclusions; no new domain finding
is proposed on the integration delta. Fork/cardinality state source and
native codec are unchanged at the same immutable pin.

No material tenancy, RBAC, signed-fragment, or diagnostic-disclosure finding is
proposed. The latest native proof correction does not introduce a Wyrd
query-shape guard, tenant predicate, protocol, or alternate authority. It
preserves the existing evaluated-scalar runtime refusal boundary. The
additional permission/error surfaces remain subject to the same authorization
and opaque runtime mapping after the fork pin changes.

The scalar second-row refusal remains an opaque 500. That is explicitly
preserved by the remediation, applies to execution rather than the spec's
caller planning-error requirement, and leaks no sensitive content. Changing
that mapping is not a required security correction in this round.

## Verification limits

This reviewer inspected the source and actual assertions and read the
committed R3 command/evidence table; it did not independently rerun Cargo,
database journeys, benchmarks or load generation. Full integrated capability
verification belongs to change review. Cumulative runtime state/codec
inspection is sufficient for this domain's ownership and isolation
conclusion, not a claim of exhaustive SQL correctness or proof against future
unsound row-bound changes.

## Overall result

**PASS** — validated domain finding union is empty.
