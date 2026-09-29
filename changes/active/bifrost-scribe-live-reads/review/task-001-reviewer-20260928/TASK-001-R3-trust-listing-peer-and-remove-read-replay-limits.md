---
id: TASK-001-R3
kind: remediation
status: blocked
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 3
parent_task: TASK-001
remediates: [independent-reviewer-recommendation-20260928]
---

# Trust the listing peer, remove read-only replay limits, and prove capacity

## Outcome and value

The previous benchmark answered about 8.5 short reads/s on 4 CPU / 8 GiB
because every query minted, verified, audited, and replay-recorded tickets for
metadata listing, local forwarding, and local fragments, and the replay records
refused work once they were full. This remediation removes that machinery from
metadata listing and from read-only operations. Signed, expiring tickets stay
for remote requests that can return rows or reserve resources. The benchmark
now fails when a required row misses its requirement.

## Contract (independent reviewer recommendation, accepted by the user)

### Trust model

- Scribe listing returns metadata only. It trusts the authenticated internal
  peer: mTLS plus the shared Bifrost workload credential. There is no ticket,
  replay record, or audit event, and no new identity system.
- Remote row reads keep signed, expiring tickets that bind tenant, table,
  fragment, and deadline. Invalid-signature and wrong-binding rejection stay.
- ReserveSlots and stage tickets keep single-use replay rejection. The
  1,024-entry capacity refusal is removed; expiry pruning stays.
- Oracle user and table authorization happens before any listing.

### Delete

- `oracle/tail_authority.rs` and `oracle/tail_audit.rs`.
- `TailAudience`, `TailBinding`, `TailFence`, and `TailReplay` audit details,
  and their schema entries.
- `tail_ticket` and `query_id` on `ListActiveStreamsRequest`.
- The discovery minter and its hooks.
- The shared `TailTicket*`, `TailSecurityAudit`, and `NoopTailSecurityAudit`
  types.
- Nonce consumption for read-only forwarding and fragments, and
  `ReplayCapacity`.
- Same-process tickets: Gate to its local Oracle, and leader-local fragments.
- The benchmark's 31-second pause.

### Performance

- One reused channel per Scribe node.
- Listings run concurrently under the query deadline, with no detached work.
- Key scans are filtered to the named table.
- Discovery uses the frozen roster. A stale epoch restarts the whole attempt
  once on a refrozen roster.
- Live dispatch uses the same frozen endpoint and fence.

### Tracing

Oracle records phase times for:

- snapshot pin;
- Scribe listing;
- provider setup;
- physical planning;
- admission;
- first row;
- terminal.

The process-cluster report shows the server-side phase means next to the
client round-trip latency. Request IDs appear only in traces.

### Benchmark

- Setup: 4 CPU / 8 GiB process_cluster; Docker is used only for Postgres.
- Rows: 500 and 1,000 reads/s, each with 0 and with 4 held live readers.
- A remote-Scribe run.
- Each row requires:
  - at least 99% success;
  - no missed sends;
  - no security refusals;
  - valid live holds.
- A durable write-rate measurement reads back every acknowledged batch. It
  has no pass/fail target.

## Implementation

| Commit | Change |
|---|---|
| `0ad30dbe8` | Listing machinery deleted. Adds reused channels, concurrent table-scoped listing, frozen roster, stale restart, and `LiveScribeRoute.endpoint`. Proto and schemas regenerated in place. |
| `121554a21` | Read-only forwarding and fragments consume no nonce. `ReplayCapacity` removed. Leader-local forwarding and fragments pass verified claims without signing. |
| `cc0f7a238` | The unused nonce was dropped from `PeerTicketClaims`. |
| `c6951dc55` | `QueryPhase` telemetry (`oracle_query_phase_seconds{phase}`) added; per-row phase means added to the report. |
| `9833cddc2` | Benchmark fails when a requirement is missed. Adds the remote-Scribe placement, a `SecurityRefused` outcome, and the write-to-durable-ACK measurement with read-back. |
| `42041181f` | Stale-epoch and listing-cancellation journey cases. |
| `e2df6e155` | Security posture and Bifrost design docs updated; stale replay rustdoc fixed. |

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Listing ticket, replay, and audit machinery deleted | `0ad30dbe8` | `rg -i 'TailTicket\|tail_authority\|tail_audit\|TailReplay' crates` finds no matches; `codegen:check` | PASS |
| Authorized listing; unauthenticated peer refused | `grpc/scribe_tail.rs` | `pg_grpc_ingest_smoke`: `scribe_tail_listing_trusts_the_peer_and_scopes_to_the_table`, `scribe_tail_unauthenticated_is_rejected_before_lookup`, `scribe_tail_tonic_lists_seeded_partition` | PASS |
| User/table authorization refused at Oracle | existing Oracle authorization | `server::query::tenant_scoped_roles_reach_only_their_granted_bifrost_tables` (`WYRD_VALA_403_QUERY_FORBIDDEN`) | PASS |
| Local and remote Scribe live reads | frozen endpoint in `LiveScribeRoute` | `test:bifrost:journey:oracle` 33/33, including `distributed::published_workers_and_live_scribes_share_one_plan`, `live_query_routes_only_relevant_scribes`, and `peer_transport_uses_immutable_fenced_destinations` | PASS |
| Stale epoch restarts once and never mixes cuts | `discover_live_routes`, `refreeze_roster` | `distributed::live_query_terminal_failure_matrix`: 1 stale listing gives Success with all 6 rows; 2 stale listings give Degraded. The trace shows 2 restarts and 2 omissions. | PASS |
| Cancellation bounds listing | `tokio::time::timeout` around `join_all` | Same matrix: a listing stalled past a 2 s deadline returns 504 at 2002 ms, and the next query succeeds | PASS |
| Repeated valid read-only tickets verify | `peer_authority.rs` | `oracle_peer_authority_rejects_tamper_expiry_and_restart_fence` (fragment ticket used twice; forwarding envelope repeated) | PASS |
| Reservation replay still rejected; no capacity refusal | `peer.rs` | `reservation_tickets_are_operation_body_and_use_exact`, `oracle_peer_replay_cache_consumes_nonce_atomically`, `oracle_peer_replay_cache_is_unbounded_and_expiry_reclaims`, `peer_network::security::peer_tickets_are_independent_exact_and_replay_safe` | PASS |
| Phase telemetry and report | `QueryPhase`, `phase_series` | `load::capacity::run::tests::phase_series_pair_into_means`; the benchmark report below | PASS |
| Security posture documented | `architecture/wyrd-security-posture.md`, `architecture/bifrost-design.md` | review of the diff | PASS |

### Benchmark result

`mise run bench:bifrost:query-capacity` at `e2df6e155` (4 CPU / 8 GiB pod
scope, Postgres in Docker only). **FAIL**: every query row missed its
requirement and the write read-back failed. The benchmark now fails as
required. Report: `target/bifrost-query-capacity/report.txt` and
`remote-scribe/`.

| Row | ok/s | refused | p99 ms | Boundary | Server phase means (ms) |
|---|---|---|---|---|---|
| 500 / 0 live | 247.1 | 15,172 | 15.7 | Oracle admission | pin 6.8, list 0.3, plan 0.7, admission 0.01, first row 3.2 |
| 1000 / 0 live | 168.6 | 49,884 | 39.7 | cgroup CPU | pin 11.0, list 0.8, plan 0.8, first row 6.7 |
| 500 / 4 live | 126.5 | 22,403 | 72.2 | Oracle admission | pin 14.4, list 0.6, first row 42.6 |
| 1000 / 4 live | 125.7 | 52,447 | 74.3 | Oracle admission | pin 15.0, list 0.8, first row 42.8 |
| 500 / 4 live, remote Scribe | 187.3 | 18,700 | 304.0 | Oracle admission | admission 242.1, first row 284.5 |
| 1000 / 4 live, remote Scribe | 115.4 | 53,067 | 300.9 | cgroup CPU | admission 21.9, first row 64.7 |

- There were no security refusals, deadlines, transport failures, or wrong
  rows in any row.
- Live holds were invalid in every live row: 4 held streams used 0 slot units,
  and most held streams were refused.
- Scribe listing now costs 0.3–1.5 ms, down from the ticket path's ~8.5
  queries/s ceiling.

Writes: 4 writers × 16,384 rows over 60 s.

- 1,014 batches were acknowledged, 276,780 rows/s (16.9 batches/s). ACK
  latency p50/p95/p99 was 43.8/86.0/98.8 ms.
- 4,600 writes were refused with `WYRD_VALA_507_WAL_DISK_FULL`.
- All 1,014 read-back queries were refused with
  `WYRD_VALA_429_QUERY_ADMISSION_REJECTED`, even though they ran one at a time
  after the writers finished. The pod logs were not kept, so the server-side
  reason is not known.

### Verification commands

- `mise run test:bifrost:journey:oracle`: 33/33 passed.
- `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_grpc_ingest_smoke --features test-support --run-ignored=all -E "test(/^scribe_tail_/)"'`: 3/3 passed.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::live_query_terminal_failure_matrix)'`, run under the Postgres wrapper: passed.
- Not yet run: `mise run lints`, `codegen:check`, `verify:bifrost`, and `git diff --check`.

### Material limits

- Tenant-mismatch refusals on tail listing are no longer audited, because
  listing trusts the internal peer; this is the accepted tradeoff.
- `dispatcher.rs` `client()` and forwarding `connect()` still open a fresh
  TLS connection for each remote fragment or forward. This is outside the
  recommendation's scope; reuse them if the phase report attributes cost to
  them.
