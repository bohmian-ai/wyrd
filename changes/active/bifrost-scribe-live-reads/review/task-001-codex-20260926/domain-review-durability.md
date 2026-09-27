# Independent domain review: published and live authority

**Subject:** `d1ec13200d332745af2fed8069a21d5b5c39cb47..f9115fbbf6b6f116cf5ec5fe5582a9543107955a` (`TASK-001`). Reviewed the cumulative candidate, with the skill-only commit excluded from task conclusions. **Result: FAIL.**

## Boundary and authority covered

| Boundary | Authority | Source and proof inspected | Result |
|---|---|---|---|
| WAL acknowledgement, staging and publication | Approved spec REQ-006, INV-001; `architecture/bifrost-design.md` authority transitions; `architecture/references/domain/analytical-operations-reliability.md` | Scribe `memtable.rs`, `shards.rs`, `staging_runtime.rs`, `hot_source.rs`, `claim_publication.rs`; cumulative diff | PASS: the query rewrite does not change ACK or publication commit ordering. |
| Pinned published cut plus live source | Spec REQ-001, REQ-002, REQ-006; Iceberg reference reader-pinning rule | Oracle `planner.rs`, `mod.rs`, `live.rs`, `analytical_scan.rs`; server `tail_discovery.rs`; distributed Oracle journey | PASS for the approved best-effort handoff. Cut pinning precedes discovery/open. A commit between cut and open can omit or duplicate data as explicitly allowed. |
| Memtable to staged to published local authority | Spec REQ-003, REQ-005, INV-005; Bifrost live-tail design | `FetchLiveTailService::open_live_batches`, `LiveTailBatches::into_stream`, `StagedTailReader`, `ScribeHotSourceRegistry::staged_sources`, `ClaimPublisher::publish`; source tests `a_generation_staged_before_the_shard_settles_is_read_once`, `an_open_live_read_keeps_staged_runs_across_publication` | PASS: the memtable snapshot precedes the staged lease, staged authority is registered before `complete_staged`, and cleanup waits on existing staged leases. |
| Required fault versus permitted live omission | Spec REQ-002, REQ-004, AC-005; Bifrost failure semantics | `TonicTailReadTransport::tonic_error`, `RegistryTailStreamDiscovery::discover`, `Oracle::discover_live_routes`; `distributed::live_query_terminal_failure_matrix` | **FAIL: DUR-001.** |

## Material proposed finding

### DUR-001 — Authentication or protocol failure during Scribe listing becomes a degraded result

- **Classification:** INCORRECT.
- **Violated obligation:** REQ-002 and REQ-004 require security, tenant, schema, and protocol-integrity faults to fail the query; only an unavailable ready Scribe's listing may degrade it.
- **Location and evidence:** [`tail_rpc.rs`](../../../../../crates/vala/vala-bifrost-redux/src/scribe/tail_rpc.rs) `tonic_error` around line 328 maps every gRPC status, including `PermissionDenied` and `InvalidArgument`, to `TailReadError::State`. [`tail_discovery.rs`](../../../../../crates/wyrd/wyrd-server/src/oracle/tail_discovery.rs) around lines 160-175 wraps every `list_active_streams` error again as `State { detail: "tail stream listing failed" }`. [`mod.rs`](../../../../../crates/vala/vala-bifrost-redux/src/oracle/mod.rs) `Oracle::discover_live_routes` around lines 3518-3539 treats every non-deadline discovery error as `listing_lost`, omits that table's live routes, and proceeds to a `Degraded` terminal. The Scribe gRPC handler maps ticket authorization failure to `PermissionDenied` in `grpc/scribe_tail.rs`; thus the path is reachable when a private listing ticket is rejected.
- **Observable consequence:** A query can return published rows with `Degraded` and `LiveTailUnavailable` after a live listing was refused for security or malformed protocol state. The client accepts that result although the approved contract requires `Failed`. The journey covers a generic listing outage and a *fragment* ticket rejection, but no listing authorization rejection.
- **Testable correction:** Preserve failure class through the existing private listing transport/discovery path. Oracle may mark only actual availability loss as `listing_lost`; it must fail on ticket/tenant/schema/protocol refusals. Add the smallest focused real-server listing fault case that rejects the query and leaves no accepted result; keep generic listing unavailability degraded.

## Verification limits

The task records green `verify:bifrost`, 33/33 Oracle journeys, and the full gate. I did not rerun lanes because this is an immutable source review. The staged lease test proves an already-open staged source stays readable after publication; it does not claim an exact published/live handoff, which the approved specification excludes. The failure matrix does not exercise a listing-layer authorization refusal, so green lanes do not close DUR-001.
