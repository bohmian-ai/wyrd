# TASK-001 invariant review

**Subject:** `d1ec13200d332745af2fed8069a21d5b5c39cb47..f9115fbbf6b6f116cf5ec5fe5582a9543107955a` (HEAD matched candidate at review). Approved authority: `changes/active/bifrost-scribe-live-reads/spec.md` revision 3 and `tasks/TASK-001-unified-scribe-live-query.md`. The `e15c610af` skill edit in the range is an approved review-skill edit, outside TASK-001. **Result: FAIL.**

I followed the producer-to-sink path through the Scribe listing RPC, discovery, physical follower resolver, Scribe frame stream, Oracle live leaf, terminal accumulator, and scheduled Drift consumer. The findings below are reachable loss-classification errors; a green failure matrix with injected availability/capacity faults does not exercise these classes.

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REQ-001 / AC-001: one request and validated Success/Degraded/Failed terminal | `wyrd-spec/src/vala/api.rs`, client and SDK projections, `oracle/query_stream.rs` | Contract, SDK, CLI/MCP journeys listed in task evidence | PASS |
| REQ-002 / AC-002: authenticated discovery and selected owners | `oracle/mod.rs::discover_live_routes`, `oracle/live.rs::LiveTableRoutes::select` | `distributed::live_query_routes_only_relevant_scribes` | FAIL: listing faults misclassified (INV-01) |
| REQ-003 / AC-003: one plan, Scribe-local streaming, distributed published work | `oracle/live.rs::LiveScribeExec`, `LiveUnionBoundary`, Scribe follower | `distributed::published_workers_and_live_scribes_share_one_plan` | PASS |
| REQ-004 / AC-005: only pre-row availability degrades; schema, security, resource and protocol faults fail | `oracle/live.rs::LiveFragmentRead`, `wyrd-server/oracle/peer_service.rs::execute_scribe_fragment` | `distributed::live_query_terminal_failure_matrix`; its injected cases omit resolver-schema and producer-error paths | FAIL: INV-01, INV-02 |
| REQ-005 / AC-004: bounded, query-owned live resources and >30 s life | `scribe/tail_rpc.rs::LiveTailBatches`, `oracle/live.rs::LiveFragmentRead` | `distributed::live_stream_backpressure_and_query_owned_lifetime` | PASS |
| REQ-006: publication overlap remains best effort | `architecture/bifrost-design.md`, docs, staged source lease | Source and journey evidence | PASS |
| REQ-007 / AC-006: ordinary query service for Drift, terminal before verdict | `query/scheduled.rs::consume_to_terminal`, `verification/drift.rs::DriftReader::fold` | Drift journey reads live rows; scheduled unit rejects Failed; no server Drift failure journey | PASS for inspected control flow; end-to-end negative proof remains limited |
| AC-007: retire tail fence, keep authenticated listing | Deletion of `oracle/tail_fence.rs` and tail RPC methods; listing retained | Source sweep, Oracle journeys | PASS |
| AC-008: required gates | Task evidence records fmt, lints, codegen, docs, verify:bifrost and gate | Recorded pass, not rerun in this read-only review | PASS as available evidence |
| INV-001: ACK/WAL/publication unchanged | Ingest path unchanged; staged lease retained | Scribe and Forge journeys | PASS |
| INV-002: tenant, ticket, projection, schema boundaries | Peer authority and follower preflight remain | Security matrix; fault classification below | FAIL: security/schema refusal may become Degraded |
| INV-003: automatic class, one physical root | `Oracle::build_physical_root` and retained root | Distributed journey | PASS |
| INV-004: no foreign local files, only terminal-backed client result | Scribe-local staged reader; SDK decoders and scheduled consumer | Query journeys and unit | PASS |
| INV-005: bounded ownership | Query and Scribe stream/drop owners | Lifetime journey | PASS |
| Non-goals: no Flight, owner index, replacement lease, second planner/scheduler, Scribe stage role, pushdown, public selectors, verification-only route, exact ACK promise, persisted state or changed ACK timing | Base-to-candidate source/diff inspection | N/A | PASS |

## Proposed findings

### INV-01 — INCORRECT: authenticated listing refusals become best-effort loss

**Violated:** REQ-002, REQ-004, INV-002, AC-005. **Location:** `crates/vala/vala-bifrost-redux/src/scribe/tail_rpc.rs:328-331`; `crates/wyrd/wyrd-server/src/oracle/tail_discovery.rs:164-178`; `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3520-3533`.

`ScribeTailGrpc::list_active_streams` returns `PermissionDenied` for ticket or tenant refusal and `InvalidArgument` for binding refusal. `tonic_error` converts every gRPC status to `TailReadError::State`; `RegistryTailStreamDiscovery::discover` converts that again to a generic `State`. `Oracle::discover_live_routes` treats every non-deadline error as a known live-source loss, omits the table's live routes, and later emits Degraded with `LiveTailUnavailable`. The same path covers credential and ticket-mint faults. Thus an actual trust-boundary refusal can return published-only rows with a valid Degraded result instead of failing. Preserve a typed security/binding fault across discovery and fail the query for that class; degrade only ready-source availability. A focused real-server listing refusal should assert Failed and client rejection, alongside an unavailable-listing Degraded case.

### INV-02 — INCORRECT: Scribe schema and producer errors become availability

**Violated:** REQ-004, INV-002, AC-005. **Location:** `crates/wyrd/wyrd-server/src/oracle/peer_service.rs:324-335,356-370`; `crates/vala/vala-bifrost-redux/src/oracle/follower.rs:1003-1011,1465-1489`; `crates/vala/vala-bifrost-redux/src/oracle/live.rs:490-507,529-543`.

The Scribe resolver rejects a schema fingerprint or signed projection mismatch as `PhysicalPlanFollowerError::Resolution`. `execute_scribe_fragment` maps *every* `Resolution` to `EligibleSourceLoss`, including that schema refusal, and maps every pre-stream `Execution` error to `Unavailable`. Its streaming branch maps every non-stale, non-tenant DataFusion error to `Unavailable`; this includes staged-run decode, projection and predicate failures and resource errors before the first row. The Oracle live leaf expressly degrades `EligibleSourceLoss` or `Unavailable` before any row. A mismatched schema or malformed/corrupt staged data can therefore be reported as a successful Degraded query over the remaining sources. Keep the originating failure class through the Scribe follower and peer wire: only actual source absence/transport availability may degrade; schema, execution, resource, and integrity faults must fail. Add a focused live-fragment test and real-server query case for a schema refusal before rows, checking Failed terminal and client rejection.

The two findings share a rule (only genuine live availability may degrade) but arise at independent producers, listing and fragment execution. Fixing the final Oracle predicate alone cannot recover error identity already erased upstream.

## Verification limit

The task evidence reports the broad gate and journeys as passing. I did not rerun them because this is a read-only acceptance pass over an immutable candidate. The Drift Failed-to-no-verdict path is established by `consume_to_terminal` and caller propagation in source, but only its lower decision point has an injected failure test; the existing Drift journey exercises success.
