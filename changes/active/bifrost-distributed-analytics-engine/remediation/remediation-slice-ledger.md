# BIFROST-T1-UNIFIED-PEER-REMEDIATION — slice ledger

Branch `oracle-distributed`. Base for this remediation: `eae68e5ca`.

Canonical acceptance lane: `mise run test:bifrost:journey:oracle`
(slice 8 uses `mise run test:bifrost:journey:server`).

Status legend: `open` (not started), `in progress` (implementation landed,
named acceptance test not yet written), `green` (named acceptance test written
and passing).

---

## Slice 1 — T1-U-C01 `peer_listener_is_isolated_mtls_and_role_complete`

Status: **green** (one bullet deferred to slice 6, recorded below)

### Landed

| Commit | Outcome |
|---|---|
| `6a2489911` | Role-neutral `BifrostPeerConfig` + `PeerTicketKeyringConfig` in `wyrd-server/src/config.rs`; the ten canonical `WYRD_BIFROST_PEER_*` env names; `MutualTlsServerConfig` / `mutual_tls_server` in `wyrd-tonic`; `mutually_authenticated_tls_endpoint` in `wyrd-tonic::transport`. Removed the three `WYRD_ORACLE_*` peer projections and `OracleRuntimeConfig::{advertise_addr, peer_ca_certificate_path, peer_server_name}` with no aliases. |
| `037686331` | `OraclePeerTls` → role-neutral `BifrostPeerTls` carrying CA + server name + client leaf chain + `SecretString` key, with a redacting `Debug`. Every peer dial (`dispatcher`, `forwarding`, `tail_discovery`, `lifecycle_transport`) now goes through `BifrostPeerTls::endpoint`; the plaintext fallbacks are deleted. `ServerOraclePeerCredentials` → `ServerBifrostPeerCredentials::from_configured_key`, reading `bifrost.peer.api_key` rather than the environment. |
| `6b2eedf4b` | Two listener trust zones under one server lifecycle: `build_app_grpc` is public-only; new `build_peer_grpc` mounts `OraclePeerGrpc`, `ScribeTail`, the Analytical worker, and `OracleLifecycleGrpc` with no health/reflection. `WyrdServer`/`BoundServer` bind and serve the private listener under `TaskId::BifrostPeer`. `load_peer_tls` fails closed for any target where `serves_peer()`. |
| `e3eb9ff57` | Test-tier `BifrostPeerCa` (`wyrd-testing/src/bifrost/peer_ca.rs`) minting one CA and a distinct dual-EKU leaf per replica via `rcgen`. `WyrdTestServer` provisions a peer identity and a reserved private address for every node. Deleted the static server-auth-only `oracle-peer-{ca,cert,key}.pem` fixtures. |
| `148e94061` | Peer authority is minted once per cluster and shared by every replica. Previously it was gated on a per-test flag, so a topology started without it minted a distinct CA per node and east-west mTLS failed. The flag and its two TLS-named start wrappers are gone. |
| `654a1144e` | Versioned, checksummed, crash-safe Scribe node identity: `wyrd-server/src/boot/node_identity.rs` (`ScribeNodeIdentityStore`, `NodeIdentityDocument`, domain-separated SHA-256 checksum, write + `sync_all` + atomic rename + parent-directory fsync). Boot reclaims the stored identity on a Scribe-bearing target and generates a fresh one otherwise. Six unit tests. |
| `fdedfef70` | Peer-listener readiness: `app/peer_plane.rs::PeerPlaneStatus` (required / serving / satisfied), retained on `AppState`, required when `config.role.serves_peer()`, marked serving around the peer serve task, probed by `compute_snapshot` as `ReadinessSnapshot.peer` with `ProbeReason::PeerPlaneDown` and the `bifrost_role_ready{role="peer"}` gauge. Shutdown joins the peer task through the existing supervisor. |
| `de586477f` | `PgFixture::attach` and `TestDatabase::attach`, so a child process joins an already-created, migrated, seeded fixture database without owning or dropping it. |
| `e8d9b6d9d` | `BifrostProcessCluster` (`wyrd-testing/src/bifrost/process_cluster.rs`, ~1000 lines) plus its child implementation. Each simulated pod is a real child process with its own composition, listeners, roots, WAL, and spill; only the database, object store, peer CA, and peer Service principal are shared. NDJSON control protocol on piped stdio, bounded control lines, bounded stderr tail, dedicated reaper thread owning the `Child`. `bifrost_peer_test_node` is now a three-line shim over the library. Four unit tests. |
| `7cab69c02` | The `peer_network` journey target: `tests/bifrost/oracle/peer_network/{mod,listener,security,transport,analytical,support}.rs`, registered as `mod peer_network;` in `tests/bifrost/oracle/main.rs`, with the named acceptance test `peer_listener_is_isolated_mtls_and_role_complete`. Harness additions the scenarios needed: `PeerTlsDefect` + `probe_startup_failure`, `VolumeAction` + `restart`, observed `membership` and the actually published `advertise_addr` on every `NodeReport`, pod-to-pod `dial_peer`, `register_table`, and fixture-level public-principal provisioning. |
| `269ede1e2` | The pod-to-pod dial presents its bearer under `x-wyrd-access-token`, the metadata key the production peer transport uses; it had been sending `authorization`, so every authorized dial was refused. |
| `f9e064777` | Listener scenarios prove trust through a real peer RPC rather than through `connect()`, because TLS 1.3 surfaces a rejected client certificate after the client's own handshake completes. A dedicated Forge worker composes no listener and runs no readiness loop, so it reports ready once its worker is spawned. Readiness timeouts now name the failing probes. |

### Evidence

- `mise exec -- cargo test -p wyrd-server config::` — 64 config unit tests pass,
  including the rewritten `peer_production_requires_complete_tls` and
  `peer_environment_names_land_on_validated_fields`.
- `peer_ca.rs` unit tests pass: `issued_leaves_are_distinct_under_one_authority`,
  `materialize_writes_a_complete_replica_identity`.
- `node_identity.rs` (6 tests), `peer_plane.rs` (3 tests), and
  `process_cluster.rs` (4 tests) unit suites pass.
- Observed at boot: `INFO wyrd_server::app::server: Bifrost peer server listening addr=Some(127.0.0.1:63502)`.
- RED→GREEN on the shared-CA defect: `mise run test:bifrost:journey:oracle`
  went 6 passed / 4 failed → 10 passed / 0 failed (see slice 4).

### RED → GREEN — `peer_listener_is_isolated_mtls_and_role_complete`

`mise run test:bifrost:journey:oracle`.

RED at `f9e064777`:

```
Summary [  76.262s] 11 tests run: 10 passed, 1 failed, 0 skipped
FAIL wyrd-testing::oracle peer_network::listener::peer_listener_is_isolated_mtls_and_role_complete
```

The scenarios run in order, so the failure moved forward as each cause was
fixed, which is itself the evidence that the earlier ones hold:

1. `Child("child pid ... readiness: postgres=Warmup storage=Warmup scribe=Warmup
   oracle=Warmup peer=Warmup peer_required=false peer_serving=false")` — a
   dedicated Forge worker returns from `WyrdTestServer::bind` before the serving
   path, so it never runs a readiness loop and never becomes ready under a
   serving definition. Harness defect, fixed in `f9e064777`.
2. `"an anonymous client completed the peer handshake"` — not a listener defect:
   TLS 1.3 completes the client's own handshake before the server's
   client-certificate alert arrives, so `connect()` returning `Ok` proves
   nothing. Test defect, fixed in `f9e064777` by driving one real peer RPC.
3. `"an authorized Oracle-to-peer dial to https://127.0.0.1:60860 was refused as
   The request does not have valid authentication credentials"` — the harness
   dial presented the bearer as `authorization`; the production peer transport
   (`TonicOraclePeerTransport::authenticated`) sends `x-wyrd-access-token`.
   Harness defect, fixed in `269ede1e2`.

GREEN at `269ede1e2`:

```
Summary [  52.337s] 11 tests run: 11 passed, 0 skipped
```

Proved: one `wyrd-server` lifecycle owns two isolated listeners, with every pod
a distinct child PID holding distinct roots and distinct actual sockets; no peer
service answers on the public listener; a missing CA, certificate, or private
key stops a peer-bearing target from starting; only a leaf issued by the
configured authority, under the configured server name, reaches the application
layer, while an anonymous, foreign, or misnamed client loses the connection; a
trusted certificate alone authorizes nothing and does not encode the runtime
`NodeId`; readiness waits for the advertised peer socket and shutdown joins it;
one-, two-, three-, and six-Oracle topologies register each pod's exact peer
address and every Oracle both coordinates a public Interactive query and
establishes authorized peer connections to every other pod; mixed, Scribe-only,
Oracle-only, and Forge-only targets mount exactly their approved services; and a
Scribe keeps its volume-coupled `NodeId` across restart, advances its fence,
becomes a new node on a replaced volume, and refuses to start over malformed or
partial identity state.

`mise run fmt` applied; `mise run lints` clean.

### Deferred bullet

T1-U-C01 also requires that every Oracle can coordinate an *inactive Analytical*
query. That seam (`AnalyticalExecutionHandle::execute_inactive`) is restored by
slice 6, and `ControlRequest::ExecuteInactiveSql` is already wired through the
harness end to end, currently answering `"inactive Analytical execution is not
yet mounted on this node"`. The assertion is added in slice 6 rather than
asserted against a seam that does not exist; `peer_network/listener.rs` records
this at the topology scenario so it cannot be lost.

### Known substitution

`AddressPlan::detect()` probes `127.0.0.2:0`. Linux routes all of
`127.0.0.0/8`, so each pod gets a distinct loopback address at the canonical
port `50052`, exactly as a deployment does. macOS routes only `127.0.0.1`
without an `ifconfig lo0 alias`, so the harness falls back to distinct ephemeral
ports on `127.0.0.1` and reports which plan it used via
`BifrostProcessCluster::address_plan()`. Local runs on macOS therefore do not
prove canonical-port addressing; CI on Linux does.

---

## Slice 2 — T1-U-C02 `peer_authentication_precedes_body_admission`

Status: **green**

### Landed

`9acff36e1` — F9 first-frame bounding. `GrpcFirstFrame` buffers HTTP/2 DATA
frames until the 5-byte gRPC header is complete and replays them verbatim, so a
split header is not rejected and a coalesced second message is not dropped.
`Resource::BifrostPeer` renamed off the Oracle-specific name.

`5fcd31bbc` — the Tier-2 probe harness and the RED test.
`BifrostProcessCluster` gained `PeerProbePlan` (`address` × `PeerProbeService`
× `PeerProbeCredential` × `PeerProbeFraming`), which issues a *raw* HTTP/2
request from one pod to another — the claim is about bytes on the wire, so a
generated client would hide it — plus a `PeerBodyPolls` control verb reading a
peer-plane-only counter incremented at the destination's first body poll.
`PeerPrincipalShape` seeds the three near-miss principals.

`d09d3a2f1` — the boundary itself. `PeerWorkloadAuthLayer` wraps every service
mounted on the private listener, runs to a verdict on connection and metadata
alone, and attaches one typed `AuthenticatedPeerContext` (control tenant,
principal, Service card, permissions, credential digest, certificate digest,
request id, timestamp — no data tenant, space, source `NodeId`, or fence)
before the inner service is called. Handler-local bearer parsing is deleted
from `OraclePeerGrpc`; the Analytical worker adapter refuses any request that
arrives without the shared context. The admitted identity is resolved at boot
from this process's own peer credential, so a replica's presented and accepted
identities are the same principal by construction and a peer-serving target
that cannot prove its own identity fails to boot. Refusal audit is bounded by a
semaphore that drops only the record, never the refusal.

The Analytical east-west channels now present that workload credential
outbound (`AnalyticalPeerCredentialLayer` on every dialed peer channel),
threaded as `peer_credentials` through `OracleBuildConfig` into
`AnalyticalStageEgress` and `AnalyticalExecutionConfig`. This was not
anticipated: the ingress requirement made the four `analytical_inactive`
journeys red, because those channels carried a peer certificate and a stage
ticket but no workload identity.

### Evidence

RED (before the layer existed):

```
peer authentication journey: "OraclePeer polled 1 request bodies for no credential at all"
```

Authentication was handler-local, so an uncredentialed request was decoded
before its identity was known.

GREEN (`mise run test:bifrost:journey:oracle`, after `d09d3a2f1`):

```
Summary [  64.465s] 12 tests run: 12 passed, 0 skipped
```

`mise run lints` clean; `mise run fmt` applied; `wyrd-server --lib grpc::` 18
passed plus the new `only_the_configured_peer_service_principal_is_admitted`;
`vala-bifrost-redux --lib analytical` 23 passed.

`mise run check:unwrap-audit` fails on `otlp_trace_json.rs` and
`forge_harness.rs`, both outside this slice's write set and failing on `main`.

### Harness defects fixed on the way

- An Oracle-only process cluster has no catalog or tail source; the journey
  topology now carries one Scribe, matching slice 1.
- `oracle_peer_credentials_from_key` exchanged every API key on a
  `SYSTEM_OWNER` connection, so a data-tenant near-miss principal failed the
  exchange instead of reaching the plane's verdict. The tenant now travels with
  the key.
- `wyrd-testing/src/bifrost/cluster.rs` still queried the pre-rename
  `'bifrost-oracle-peer'` service account.

---

## Slice 3 — T1-U-C03 `peer_tickets_are_independent_exact_and_replay_safe`

Status: **green**

### Landed

`df90e7360` — the independent keyring. `PeerTicketKeyring` loads the active
Ed25519 PKCS#8 signing key plus a versioned JSON verifying manifest
(`keyId`/`publicKeyPem`/`verifyUntil`), resolves verification by the key id the
presented ticket names, and enforces that key's own window. Every peer and tail
authority is built `from_keyring` instead of `from_pem(&signing_key, ..)`:
`BifrostBuildInputs::signing_key` is gone, replaced by
`peer_keyring: Arc<PeerTicketKeyring>`. A production profile with no configured
keyring now fails to boot; development generates an ephemeral one and warns.

The keyring is a topology invariant, not a per-node one. Both multi-node
harnesses (`bifrost/cluster.rs`, `bifrost/process_cluster.rs`) materialize one
`TestPeerKeyring` and hand every node the same three paths — an unshared
keyring turns all east-west traffic into unknown-key refusals, which is how the
first six journey failures presented.

The purpose-operation matrix. `ReserveSlots` and `ReleaseSlots` were the two
state-changing private operations with no ticket and no replay protection; they
now carry one. `ReservationOperationV1` gives each its own signing domain,
`ReservationTicketClaims` binds source and destination node/fence, query,
canonical ticket-free body digest, nonce, and expiry, and
`OraclePeerAuthority::verify_reservation` checks bounds → signature under the
*receiver's* own operation domain → body digest → binding → expiry → nonce
consumption, before any capacity moves. The transport mints one ticket per
attempt, so the single Unauthenticated retry does not replay a nonce.
`GetWorkerInfo` is unconditionally refused and has no internal caller.

### Evidence

RED (`-E 'test(peer_tickets_are_independent)'`, before the keyring):

```
peer ticket journey: "the deployment's own workload signing key was accepted"
```

RED (harness defect, after the matrix landed):

```
peer ticket journey: "no live Oracle lease for 01a05dcd-3bdf-74a0-aab7-9c256b52decd"
```

The leader answers readiness before the follower registers, so its startup
report knows only itself. `ReservationPlane::observe` now takes a fresh
membership cut via `ControlRequest::Inspect`.

GREEN (`mise run test:bifrost:journey:oracle`):

```
Summary [  65.353s] 13 tests run: 13 passed, 0 skipped
```

`wyrd-server --lib oracle::peer_authority::tests::reservation` 1 passed
(`reservation_tickets_are_operation_body_and_use_exact`: a release ticket does
not authorize a reserve, a substituted body is refused, and the same ticket is
accepted exactly once). `mise run fmt` applied; `mise run lints` clean;
`mise run codegen:check` clean. `check:proto-drift` regenerates
`proto/wyrd.v1.bin` for the two new `ticket` fields; it is committed with the
slice.

---

## Slice 4 — T1-U-C04 `peer_transport_uses_immutable_fenced_destinations`

Status: **green**

### Landed

`458d7c9ba` — deletion-ledger item 4 and finding S6. Analytical coordinator
channels went out through upstream's `DefaultChannelResolver`, which dials each
worker URL anonymously. `AnalyticalChannelResolver` now owns
`AnalyticalPeerChannels`, a per-query connection cache built from the immutable
`BifrostPeerTls`, threaded from server boot through `OracleBuildConfig` into
both the leader handle (`AnalyticalExecutionConfig::peer_tls`) and the follower
egress owner (`AnalyticalStageEgress::peer_tls`). The Analytical owners are
composed only when the stage authority *and* the peer identity are both
present. The upstream import is gone from Wyrd code.

### Evidence

RED (before, with the private listener already on mTLS):

```
Summary [ 19.270s] 10 tests run: 6 passed, 4 failed, 0 skipped
FAIL analytical_inactive::pg_inactive_analytical_cancel_deadline_and_slow_consumer_leave_six_clean_nodes
FAIL analytical_inactive::pg_inactive_analytical_raw_sql_proves_pushdown_exchange_and_qualified_spill
FAIL analytical_inactive::pg_inactive_analytical_production_telemetry_covers_every_hot_path
FAIL analytical_inactive::pg_inactive_analytical_raw_sql_executes_join_and_partial_final_aggregate_on_followers
```

with the diagnostic cause:

```
Execution error: Error connecting to Distributed DataFusion worker on
'https://127.0.0.1:65372/': transport error
```

GREEN (`mise run test:bifrost:journey:oracle`, after `458d7c9ba`):

```
Summary [  19.954s] 10 tests run: 10 passed, 0 skipped
```

`mise run lints` clean; `mise run fmt` applied.

`f7662c00d` — the immutable cut. The follower egress owner held an
`AnalyticalPeerDirectory` closure over live membership and resolved a
destination's fence "at send time", so a node joining, leaving, or re-fencing
could move a participant out from under an in-flight graph, and a middle stage
could address a node its coordinator never selected.

`AnalyticalParticipantCut` is now the only source of destinations. The leader
freezes its pinned `OracleQueryAttemptCut` once in `leader_session` and stamps
the cut into every signed stage ticket (`StageTicketClaims.participants`,
bounded by `MAX_STAGE_PARTICIPANTS`). A follower adopts that cut in
`AnalyticalStageEgress::record` and can neither widen it nor read membership:
`resolver` and `peer_urls` both read the recorded entry. A URL outside the cut
resolves to no minter and fails locally, before any dial. A plaintext endpoint
enters no cut in either direction, and the wire form is ordered by membership
so two coordinators that froze the same participants sign identical claims.
`AnalyticalPeerDirectory`, `StageMinterFactory`, and the composition site's
`ClusterRegistry` read are deleted.

### Evidence

RED (`-E 'test(peer_transport_uses_immutable)'`, first run of the new journey):

```
peer transport journey: "the replacement pod came back under a different node identity"
```

The scenario assumed an Oracle keeps its `NodeId` across restart. Only Scribe
carries a volume-coupled identity; an Oracle replacement is a new incarnation
by node *or* fence, which is the packet's "advances or replaces its fence". The
scenario now asserts that disjunction and that the replacement inherits the
frozen pod's address, so the refusal that follows is attributable to the stale
incarnation rather than to an unreachable endpoint.

GREEN (`mise run test:bifrost:journey:oracle`):

```
Summary [  58.783s] 14 tests run: 14 passed, 0 skipped
```

`mise run test:vala` 1272 passed, including three new
`analytical_transport::tests` covering freeze/adopt round trip, the
order-independent encoding, and the participant bound in both directions.
`mise run fmt` applied; `mise run lints`, `mise run check:proto-drift`, and
`mise run codegen:check` clean.

### Coverage split recorded for later slices

The journey proves the process-level half of C04: one mutually authenticated
transport reaches both the Oracle peer adapter and the Scribe tail adapter from
a distinct PID, refuses an unverifiable bearer identically on both, admits no
body over a plaintext dial, and refuses authority frozen against a replaced
incarnation while the live incarnation accepts. The frozen-cut half — a
destination absent from the snapshot refused locally, and a middle stage
addressing only its coordinator's cut — is proved at the vala tier, because
driving it end to end needs inactive Analytical execution mounted in the
process-cluster child, which is Slice 6's work. Close that gap in Slice 6 by
extending the journey once `ControlRequest::ExecuteInactiveSql` is real.

### Harness changes

- `PeerProbePlan` gained `PeerProbeTransport::{Mutual, Plaintext}` so a child
  can dial in the clear through the same probe path.
- `ReservationPlane`, `KeyringSigners`, `sign_ticket`, `stamped`, `reserve`,
  `probe`/`probe_from`, and `polls_at` moved from `security.rs` into
  `support.rs`; both journeys now share one ticket-minting fixture.

---

## Slice 5 — T1-U-C05 `graph_lease_owns_exact_resources_for_complete_graph`

Status: **reconciliation required; successor task proposed**

### Landed

`56252b48a` — the graph-qualified lease. `ReserveNodeSlotsRequest` gained an
optional `AnalyticalGraphRef { public_query_id, datafusion_query_id }` on the
wire (spec, proto, private conversion), so one reservation RPC distinguishes a
fragment quantum from a distributed graph envelope and the existing reservation
ticket's body digest already signs which one was asked for. `PendingReservation`
now holds `ReservedCapacity::{Fragment, Graph}`; a graph reservation charges
`try_acquire_query` and is refused by `take_for_execute`, so a graph envelope
can never be spent as a fragment.

`ReservationRegistry` gained `graph_leases` and `lease_graph`, which atomically
removes the pending entry and transfers its `OracleQueryResources` and running
permit into one `Arc<GraphLease>` keyed by the graph. The first authorized stage
method activates it; every later message on the same graph reuses the same
`Arc`. `AnalyticalStageIngress` lost `oracle_resources` entirely — the follower
no longer self-grants an envelope; it takes the leader's. The synthetic
`reservation_id: format!("reservation-{}", ...)` values in the journey and the
process-cluster child are deleted, and `StageParticipantV1` carries each
participant's real reservation id inside the signed cut.

`f1ac4cb01` — the release paths that holding the lease exposed.
`AnalyticalParticipantReservations` returns every participant reservation the
leader took but whose plan never addressed, inline on `settle` and via a
spawning `Drop` fallback; a retry reuses them rather than taking a second set.
`Oracle::shutdown` now shuts the follower ingress down before it reports
capacity, and `AnalyticalStageIngress::shutdown` releases each graph lease it
still holds — a follower graph pins this node's peer running permit, and its
coordinator will never send the releasing message during shutdown.

### Evidence

GREEN, acceptance test in a real three-Oracle + one-Scribe process cluster
(`peer_network::analytical::graph_lease_owns_exact_resources_for_complete_graph`):
each follower reports `(activated == 1, live == 0)` after the first grouped
query and `(2, 0)` after an identical second, proving one lease per follower per
attempt and full release between attempts.

```
Summary [  11.144s] 1 test run: 1 passed
```

`mise exec -- cargo check --offline --workspace --all-features --all-targets`
clean. `mise run fmt` applied.

### Not yet proved

The full `mise run test:bifrost:journey:oracle` lane last completed at **14
passed, 1 failed**:
`analytical_inactive::pg_inactive_analytical_raw_sql_executes_join_and_partial_final_aggregate_on_followers`,
refused at the shutdown gate with `peer_pending=0 peer_running=2`. The two
release fixes in `f1ac4cb01` were written against exactly that signature but the
lane has **not** been re-run since they landed. Slice 5 is not closeable until
it is.

### Open design question for review

`GraphLeaseRequest` deliberately does not re-derive or compare the reservation's
leader identity and fence at lease time, because a follower running a middle
stage is itself a coordinator and legitimately signs under its own identity. The
binding is left to the stage ticket, which independently binds the presenting
principal, this follower's node identity, and its current role fence before
`lease_graph` is ever reached. If that indirection is judged too thin, the fix
belongs here and not in a later slice.

### Checkpoint reconciliation at `f1ac4cb01`

The current-state `$wyrd-plan` pass resolved the open question: the three-field
`GraphLeaseRequest` is too thin. It also confirmed eight related slice-local
defects: reserving too early for the approved two-second pending TTL;
destructive Fragment/Graph refusal;
activation-error lease leakage; per-RPC participant/deadline overwrite;
detached and fail-open cleanup; accidental connection-lifetime retry retention;
estimated rather than actual exchange ownership; and acceptance evidence that
proves the basic process path but not the complete C05 contract.

Superseded task
[`BIFROST-T1-C05-R1`](../tasks/00-d1-r1-reconcile-slice-5-graph-lease.md)
preserved the pending-expiry, exact-ownership, activation, and drain repairs but
added an exact pre-dispatch exchange-byte proof, operator/exchange-partitioned
pool, and external dependency prerequisite. Those additions were rejected as
disproportionate for v1.

Human-approved
[`SPEC-bifrost-distributed-analytics-engine` revision 1](../spec.md) replaces
them with one aggregate query memory envelope plus finite graph counts,
concurrency, queues, scratch, deadline, cancellation, deployment memory, and
operational evidence. It explicitly prohibits a dependency fork. The three
named architecture authorities and Plan v7 are reconciled. Executable successor
[`BIFROST-T1-C05-R2`](../tasks/00-d1-r2-reconcile-slice-5-kiss-graph-lease.md)
retains the required GraphLease correctness work and deletes the fixed exchange
precharge without replacing it with another pool or framework.
The earlier GREEN output above remains valid evidence for the basic success
case; it is not sufficient to close slice 5. Slices 6–8 remain untouched and
must not absorb these lease defects.

## Slice 6 — T1-U-C06 `analytical_retry_is_once_pre_egress_and_fully_drained`

Status: **open**

## Slice 7 — T1-U-C07 `inactive_analytical_raw_sql_proves_physical_execution_and_spill`

Status: **open**

## Slice 8 — T1-U-C08 `production_fragment_and_peer_deployment_remain_isolated`

Status: **open**

`tests/bifrost/server/grpc_mount.rs` exists but is empty and is not yet
registered from `tests/bifrost/server/main.rs`.
