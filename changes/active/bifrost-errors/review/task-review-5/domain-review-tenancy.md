# Tenant and security domain review — round 5

Overall result: **PASS**. No material proposed findings in this domain.

## Immutable subject and boundary

Reviewed Wyrd `945a1548c47897dc94583169a6ba04163a73a749` →
`8a54d81c346db464b624e81ebe8958c63bb7e228`, including the cumulative task
implementation and the latest remediation since `17697dc8…`. Inspected the
latest native DataFusion delta `b2d37bc…` → `8c3f18e9806ae23e8840a710af775c058dfad66a`.
Both local HEAD identities match the subject at completion.

The reviewed boundary is authenticated caller → complete referenced-table set
→ tenant-bound cut and authorized providers → logical/planning error detail,
and admitted live/published execution → authenticated peer fragment → public
result/refusal. This is an independent tenant/security audit, not a general
approval of all SQL semantics.

## Authority and source coverage

| Authority | Applicable obligation and source examined | Result |
|---|---|---|
| Approved `spec.md` REQ-002, AC-002, INV-001 | Caller suggestions carry no unauthorized table/column information; internal errors remain opaque; tenant isolation holds for joins/subqueries | PASS |
| `AGENTS.md`, `architecture/agent-rules.md`, reference router, doctrine architecture constraints | Verified principal supplies tenancy; existing TenantConn/RLS/catalog authority owns tenant data; clients cannot select authority; no duplicate tenant guard | PASS |
| `architecture/wyrd-security-posture.md` security principles, trust boundaries and peer authority | Context, assignment, tenant, table, digest and fence checks precede worker plan/IO; uncertain security fails the request | PASS |
| `architecture/bifrost-design.md` Oracle and public permission rules, including scoped table authorization and gateway payload exception | Complete resolved set is individually granted before provider registration; stages/fragments carry scoped permission digest; gateway payload reads remain separately gated | PASS |
| `architecture/references/domain/olap-serving.md`, `languages/errors.md`, spec/maintainer references | One server-owned query boundary, structured errors, no raw internal failure projection or new SQL admission path | PASS |

CodeGraph exploration preceded focused source navigation. Source inspected:

- `oracle/mod.rs`: `AuthorizedQueryContext::try_new` (190–215),
  `build_physical_root` (2728 onward), `plan_logical` (3018–3034),
  `parse_select_tables` (3233–3282), `authorize_resolved_tables` (3426–3447),
  `authorize_payload_columns` (3463–3520), `scoped_permission_digest`,
  `live_dispatch` (2339 onward), and the three planning/execution mappers.
- `oracle/planner.rs`: complete `validate_query`, `read_and_materialize` and
  `pin_cut` paths; `oracle/session.rs`: complete session-template owner and
  catalog replacement; `oracle/bindings.rs`: once-bound execution ownership.
- `oracle/live.rs`: complete changed pushdown/resolved/fragment/execute and
  first-poll execution paths; unchanged fatal availability classification and
  tenant-invariant propagation. `oracle/dispatcher.rs`: signed fragment claims,
  typed tenant refusal and changed footer tally accessor.
- Server `oracle/peer_service.rs` complete `ScribeFragmentExecutor::execute`
  preflight/claims/admission portion; `state.rs` changed test observations.
- Changed fixture/writer/peer-cluster helpers, peer scalar/join journey,
  Rust/Python/TypeScript caller-error and unauthorized-table journeys, MCP
  query assertions, and latest fork fixed-expression proof and SQL controls.

## Findings assessment

1. **Logical planner messages remain scoped to admitted tables.** Parsing
   extracts references from the statement, including nested queries/joins;
   `read_and_materialize` resolves each under `context.data_tenant_id`, then
   invokes `authorize_resolved_tables` before materializing or registering a
   provider. One ungranted input returns opaque `QueryForbidden`, so no
   DataFusion schema suggestion is generated for that input. The session
   template replaces its catalog list per query and holds no tenant tables
   process-wide. `map_sql_planning_error` unwraps to the typed caller-fault
   leaf and strips backtraces; wrapper diagnostics and physical-planner
   details are not returned. Internal/execution failures remain opaque.

2. **The existing column authority is preserved.** Ordinary sensitive-column
   metadata is descriptive under Bifrost authority; table permission governs
   those columns. Gateway request/response payload content keeps its separate
   `authorize_payload_columns` gate traversing subqueries. No changed code
   relaxes that gate or creates a projection-only authorization path. Planner
   suggestion visibility into an authorized table's schema is distinct from
   permission to read gateway payload values.

3. **Runtime predicate resolution cannot widen tenant authority.** The live
   leaf deals the originally planned route list to partitions and only narrows
   each share after snapshotting; it retains the same tenant-qualified
   `LiveTableRoutes.binding`. Fragment creation derives assignments from that
   binding and fingerprints the complete encoded predicate. The admitted
   dispatcher carries tenant and scoped permission digest from the authorized
   cut. Worker claims, target fence, tenant/table assignment and assignment
   digest are checked before authenticated preflight and follower execution.
   Snapshotting a scalar/dynamic value changes row selection, not the signed
   tenant/table authority. Security and tenant failures remain terminal rather
   than becoming a degraded source omission.

4. **The latest proof correction is reachable without changing trust.** The
   peer journey uses a real authorized Bifrost outer relation and a grouped
   VALUES scalar input; both projection and filter pass the ordinary SQL floor
   and existing Oracle route. Native `Filter::is_scalar` now admits only
   invocation-fixed expression leaves/compositions; input columns, nested
   plans and opaque forms cannot create that proof. This affects cardinality
   validation, not table resolution, principal selection or peer authority.
   Outer references remain fixed per invocation, preserving established
   correlated lookups without accepting inner-row-dependent values. The
   ordinary `single_value` refusal is projected as the existing opaque
   execution-failure code; it does not disclose row values.

5. **Fixture and observation changes introduce no public authority.** The
   writer adds optional managed `run_id` correlation through its existing
   producer, not tenant identity in row payloads. Peer registration retains
   the cluster tenant. Fragment row counts/last-plan inspection are compiled
   under `test-support` and are read in-process by the journey; they introduce
   no public endpoint or production tenant-plan cache.

## Verification evidence and limits

Rust, Python and TypeScript unauthorized-table journeys read an allowed table,
then join an ungranted verification table while misspelling `verdict`; each
asserts `WYRD_VALA_403_QUERY_FORBIDDEN` and absence of both the misspelling and
suggested column. This is useful cross-boundary evidence in addition to the
source ordering. The latest peer journey adds both count/abs refusal shapes
to the existing real remote live/published test, retaining literal/footer and
join coverage. The R4 remediation evidence records the focused fork and Wyrd
selectors and checks; this audit inspected their source and recorded results,
not independently rerun output.

No builds, tests, benchmarks, synthetic load or broad lanes were run during
this static domain review. It does not independently re-prove all unchanged
peer cryptography, RLS policies or SQL optimizer semantics. No reachable
regression or missing security/tenant obligation in the cumulative task was
identified; there is no remediation proposal from this domain.
