# Tenancy and security-disclosure review

## Subject and result

- Base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Candidate: `7d6a1ff95b3dc09315e3af236ab9906b305e11d0` (HEAD confirmed during review).
- Specification: `changes/active/bifrost-errors/spec.md`, REQ-002, REQ-006 and INV-001.
- Original task: none; specification and owner directions are authoritative.
- Remediation: `review/task-review-1/bifrost-errors-R1-materialize-live-subquery-predicates.md`, including its committed verification table.
- Overall domain result: **PASS**.
- Proposed material findings: **none**.

This is an independent cumulative source audit of authorization, tenant bindings, diagnostic disclosure, private fragment authority and the newly retained test observations. It is not an acceptance judgment on general SQL cardinality semantics or optimizer performance, which belong to the separate DataFusion domain review.

## Authority and coverage

| Boundary | Authority | Source evidence | Result |
|---|---|---|---|
| Tenant identity and complete query-table authorization | AGENTS.md §§2,9; agent-rules tenant and permission boundaries; security posture Authorization and Tenant/data isolation; architecture-constraints Identity/tenant isolation | `oracle/mod.rs:1685`, `oracle/planner.rs:99-139`, `oracle/mod.rs:3426-3445` | PASS |
| Nested and CTE relation discovery | REQ-006, INV-001; OLAP serving Oracle paths | `oracle/mod.rs:3228-3276`; pinned DataFusion `datafusion/sql/src/resolve.rs:50-134,227-239`; `query_floor_extracts_joined_and_cte_tables` | PASS |
| Safe actionable SQL errors | REQ-002; reference `languages/errors.md` boundary conversion; security posture disclosure principles | `oracle/mod.rs:3000-3035,3735-3814`; mapper unit test; Rust/Python unauthorized-table journeys | PASS |
| Sensitive-column denial | Security posture authoritative object decision; OLAP serving sensitive columns | `oracle/mod.rs:3448-3513` traverses optimized plan with subqueries | PASS |
| Runtime snapshot and remote fragment authorization | Security posture Peer identity/distributed Oracle; OLAP serving stage authority | `oracle/live.rs:298-384,536-559,668-752`; `oracle/dispatcher.rs:846-876,1876-1979`; `wyrd-server/src/oracle/peer_service.rs:234-359` | PASS |
| Test observation confidentiality and production reachability | AGENTS.md test seams and production boundaries; agent-rules production test-support exclusion | `wyrd-server/src/state.rs:406-413,923-962`; server manifest feature definitions; `scripts/checks/deps.sh:111-120` | PASS |

The reference router, agent rules, errors, OLAP serving and architecture-constraints references were read. Governing security sections were traced back to the security posture; design/doctrine anchors were inspected for the applicable tenant/query boundaries. No new SQL connection capability, policy surface, identity authority or public diagnostic contract is introduced by this candidate.

## End-to-end source trace

### Public query to authorized planning

`prepare_query_attempt` validates the SELECT and collects its referenced tables before `pin_cut`. The table collector uses DataFusion's AST relation visitor, whose CTE scope stack excludes CTE aliases while still visiting each CTE's real input relations and nested queries. It does not accept caller-provided tenant identity or turn CTE names into physical tenants.

`read_and_materialize` reads the catalog under `context.data_tenant_id`, resolves `(tenant, TableRef)` into the canonical table binding and registered UID, then calls `authorize_resolved_tables` before metadata materialization. One denied table rejects the whole query with the fixed `QueryForbidden` catalog variant. `register_cut_providers` installs only these authorized cuts into a fresh planning session (`oracle/mod.rs:2624-2671`). Thus a typo in a denied table's column never reaches the changed diagnostic mapper.

After successful logical optimization, `authorize_payload_columns` still traverses all subqueries. Projection and pushed-filter access to gateway payload columns is denied without payload-read authority; the second optimizer pass does not remove this authorization gate. It runs before executable physical planning and remote reads.

### Planning diagnostics to public error

Logical planning now unwraps only Context, Diagnostic, Shared and the first Collection element to typed SQL/Plan/Schema/NotImplemented causes. It strips backtraces and exposes the actionable cause rather than wrapper context. Resource exhaustion retains its catalog identity; Internal, Execution and other causes stay generic failures. Physical planning retains a fixed public repair action rather than exposing implementation operators. Neither the scalar snapshot errors nor private fragment failures are routed through the logical-planning diagnostic mapper.

The mapper's assertion that available schema is already authorized is supported by provider registration order, not just by its comment. Existing table-not-found handling remains a typed public catalog failure; this candidate introduces no session-wide catalog discovery or cross-tenant schema registration.

### Runtime scalar to authenticated Scribe fragment

`LiveScribeExec::resolved` snapshots only its physical predicate and selects a subset of already frozen routes. It clones the existing route binding; it does not derive a tenant from the scalar value. Each serialized fragment carries the same binding in both its outer descriptor and its scan assignment. The fingerprint is recomputed over the new literal-bearing plan before dispatch.

`LiveFragmentRead` obtains the query-owned execution lock, checks the live grant and dispatches with its existing context. The dispatcher mints claims containing that context's tenant, permission digest, the snapshotted plan fingerprint, assignment-authority digest, target fence and deadline. The receiving Scribe verifies the peer ticket and requires every assignment's tenant and table to match its verified claims. Assignment digest and authenticated preflight are checked before follower execution. This closes the changed scalar/literal bytes over the existing authority mechanism; no bypass or alternate authorization path was added.

Security and tenant failures remain terminal (`is_availability_loss`, `live_error`); predicate snapshotting does not turn them into degraded omissions. No remote endpoint is derived from the SQL or scalar value.

### New fragment observations

The newly retained encoded plan can contain resolved scalar data, but its field and accessor compile only under `test-support`. It is recorded after authenticated follower resolution. Normal builds discard the borrowed byte argument and row observation argument; they retain no new plan buffer or public accessor. `check:deps` checks normal/cloud server graphs for test feature leakage. These observations are in-process fixture APIs, not public HTTP/MCP routes. A cluster fixture may inspect its own last plan without weakening production isolation.

## Verification credibility and limits

- The committed evidence reports the focused mapper, live snapshot/reset test, remote scalar and join journeys, common-query corpus and the nine Rust query journeys passing, plus the Oracle lane 57/57 and dependency/tenant checks.
- Source inspection confirms the Rust and Python unauthorized-table journeys use a schema-restricted reader, successfully read the permitted table, then assert a fixed 403 without the misspelled or suggested denied-column name. This directly protects the changed disclosure behavior.
- The common-query and remote scalar fixtures are not cross-tenant attack tests; their success is not treated as proof of peer authorization. The preserved verified binding, fingerprint and assignment checks provide the source evidence for that boundary.
- I did not rerun heavy lanes, benchmarks or synthetic load, consistent with the review constraints. I inspected the committed evidence rather than claiming fresh execution results.
- The previously observed query-memory release failure is a resilience verification limit, not evidence of a tenant leak. Scalar multiple-row semantics also require the DataFusion review; no tenancy finding is inferred from that adjacent semantic risk.

## Finding ledger

Validated proposal input: empty. No material tenancy/security-disclosure regression was identified in the cumulative candidate, and no remediation is proposed by this domain review.
