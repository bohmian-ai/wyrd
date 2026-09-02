# T1 remediation — unified authenticated Bifrost peer plane

Source task:
[`00-d1-inactive-distributed-execution-engine.md`](../tasks/00-d1-inactive-distributed-execution-engine.md)

Source review: `$wyrd-review` of
`3978aa7a2bbe49299fc8cd50e36c2e3c8e6e9d09..eae68e5ca3080b4e58624eb394fba6988afabe02`
on branch `oracle-distributed`.

Reviewer handoff:
<https://claude.ai/code/artifact/7ee98917-aae0-4e81-9ed6-0491ad940123>

Planning revision inspected:
`823a362f419f7f0de9a58af7fd3f26177198e5e6`.

This packet supersedes the earlier version of this remediation task. The user
has approved a dedicated private Bifrost peer listener with mandatory mTLS.
That listener is not a second peer system: `wyrd-server` owns one peer plane,
one authenticated peer context, one outbound transport owner, and one
reservation registry. Two wire adapters remain because Wyrd's Oracle control
RPCs and the upstream DataFusion worker protocol have different contracts.

Task 2 remains downstream. It MUST NOT begin until this remediation is
implemented, verified, and re-reviewed without unresolved blocking findings.

## Task contract

```yaml
task_id: BIFROST-T1-UNIFIED-PEER-REMEDIATION
execution_skill: wyrd-implement
implementation_style: tdd
primary_outcome: >-
  Replace T1's split and incomplete peer behavior with one wyrd-server-owned,
  mutually authenticated Bifrost peer plane; make the inactive Analytical
  execution path consume its authenticated identity, signed authority, exact
  graph reservation, bounded resources, structured lifecycle, and production
  telemetry without making StageGraph reachable from production routing.
owners:
  - crates/wyrd/wyrd-server/src/boot
  - crates/wyrd/wyrd-server/src/grpc
  - crates/wyrd/wyrd-server/src/oracle
  - crates/wyrd/wyrd-tonic
  - crates/wyrd-spec/src/vala
  - crates/vala/vala-bifrost-redux/src/oracle
  - crates/wyrd/wyrd-testing/tests/bifrost
dependencies:
  - T1 candidate eae68e5ca3080b4e58624eb394fba6988afabe02
  - existing wyrd-server serving and shutdown owners
  - existing workload-token verification and security-audit owners
  - existing OraclePeerService and upstream worker.WorkerService wire contracts
  - existing ReservationRegistry, OracleResources, OracleSpillRuntime, and OracleTelemetry
  - pinned datafusion-distributed and DataFusion dependency cone
downstream:
  - Task 2 may activate the private StageGraph execution seam only after this task passes FINAL re-review
required_claims:
  - T1-U-C01
  - T1-U-C02
  - T1-U-C03
  - T1-U-C04
  - T1-U-C05
  - T1-U-C06
  - T1-U-C07
  - T1-U-C08
```

This is one cohesive remediation. Listener trust, peer identity, purpose
authority, reservation ownership, runtime construction, retry, cleanup, and
evidence form one security and lifecycle chain. Do not merge a partial state in
which a later layer assumes a property that an earlier layer does not prove.

## Outcome

After this task:

- `wyrd-server` exposes one public serving listener and one private Bifrost
  peer listener. Both belong to the same server lifecycle; Vala owns neither.
- The private listener requires a peer certificate issued by the configured
  Bifrost peer CA. Server-authenticated TLS alone is insufficient.
- All private peer RPCs share one pre-body authentication layer and receive one
  typed `AuthenticatedPeerContext`.
- Peer-purpose tickets use a dedicated keyring and cannot be forged with a
  user/API workload-token key.
- `OraclePeerService` and upstream `worker.WorkerService` remain separate
  wire adapters but use the same listener, identity, authorization, audit,
  transport, reservation, deadline, and cancellation owners.
- The existing `ReservationRegistry` owns both Fragment and StageGraph
  reservations. There is no second reservation system or follower-local
  resource root.
- Each selected follower activates one exact graph-qualified lease. The lease
  transfers its runtime, memory pool, exchange budget, spill share,
  cancellation token, deadline, authority digest, and immutable participants
  into the graph exactly once, then remains active across all stage RPCs and
  the optional second attempt.
- The inactive handle owns one pre-egress retry and all attempt cleanup. Every
  spawned task is joined, every lease is released exactly once, and retry
  cannot overlap the drained attempt.
- Raw-SQL tests prove real follower work, physical join/aggregate/pushdown/
  exchange evidence, and a real spilling DataFusion operator.
- Production telemetry records the authenticated peer and Analytical lifecycle
  without secret, SQL, plan, or unbounded labels.
- Production routing still uses the existing Fragment path. Only the private
  execution-path seam needed by Task 2 exists after remediation.

## Authority and selected references

The implementation MUST follow:

- `AGENTS.md`: one `wyrd-server` serving owner, Rust struct-centered
  ownership, narrow async boundaries, TDD/user journeys, and KISS.
- `architecture/agent-rules.md`: repository execution rules.
- `architecture/wyrd-design.md` and `architecture/wyrd-doctrine.mdx`:
  server-owned durable behavior and typed language-neutral contracts.
- `architecture/bifrost-design.md`: Bifrost admission, distributed
  execution, resource, and reliability authority.
- `architecture/wyrd-security-posture.md`: mutual peer identity plus
  purpose-scoped workload authority, authentication before first-frame
  admission, tenant isolation, and audit.
- `architecture/references/languages/implementation-execution.md`: implement,
  verify, repair, and report claim-aligned evidence.
- `architecture/references/languages/rust-core.md`: cohesive concrete owners
  and bounded async orchestration.
- `architecture/references/languages/testing-workflows.md`: TDD and journey
  priority.
- `architecture/references/domain/olap-serving.md`,
  `architecture/references/domain/datafusion.md`, and
  `architecture/references/domain/analytical-operations-reliability.md`:
  query admission, physical execution, spill, lease, cancellation, and
  failure evidence.
- `architecture/references/domain/telemetry-observations.md`: bounded,
  correlated, non-sensitive telemetry.

The original Task 1 assumption that the Analytical path must not add another
listener is explicitly replaced by the user's approved security decision:
use a dedicated private listener. The constraint that still applies is **no
second serving owner and no second peer architecture**. `wyrd-server` starts,
supervises, reports readiness for, and shuts down both listeners.

Primary best-practice grounding for the three reviewed blockers:

- SPIFFE distinguishes a scalable workload identity from node/machine
  identity; Wyrd follows that separation without adding a SPIFFE dependency:
  <https://spiffe.io/docs/latest/spiffe/concepts/>.
- RFC 5280 makes SAN the certificate identity binding, while RFC 8705 shows
  that binding an application token to a specific certificate is a separate
  proof-of-possession decision. Wyrd does not add certificate-bound tokens in
  this remediation:
  <https://datatracker.ietf.org/doc/html/rfc5280#section-4.2.1.6> and
  <https://datatracker.ietf.org/doc/html/rfc8705>.
- Kubernetes NetworkPolicy is defense in depth at network/port scope, not
  workload authentication; application mTLS, Wyrd authentication, and tickets
  remain mandatory:
  <https://kubernetes.io/docs/concepts/services-networking/network-policies/>.

## Complete finding ledger

Every item below is blocking unless its disposition says otherwise.

| ID | Finding | Required disposition |
|---|---|---|
| F1 | Upstream Worker RPCs have no workload authentication or Wyrd authorization. | Route them through the common peer authentication layer and require operation-specific authority. |
| F2 | Analytical reservation identity, permission, and snapshot state are synthetic; follower execution creates a fresh local resource grant. | Extend the existing reservation into one follower-local GraphLease per graph, activate its resources exactly once, and retain it across all graph RPCs and the optional retry. |
| F3 | Follower cancellation/deadline propagation is incomplete and exchange accounting is not based on actual owned buffers/messages. | Bind both to the GraphLease and charge/release real exchange ownership. |
| F4 | The pre-egress retry helper is proved only by direct/manual tests rather than the execution handle used by the raw-SQL journey. | Put retry in the restored handle and prove it through the real transport journey. |
| F5 | Stage, connection, metric, cache, and cleanup work can be detached or unjoined. | Use structured attempt ownership and join/drain all work before success, retry, cancellation, or shutdown. |
| F6 | Join, aggregation, pushdown, exchange, and spill evidence does not prove actual follower operators and actual spill. | Capture post-drain physical/runtime evidence from the real execution. |
| F7 | `AnalyticalExecutionRequest`, `AnalyticalExecutionHandle::execute_inactive`, and `AnalyticalExecution` were deleted as dead code. | Restore them as the private binding seam consumed by tests now and Task 2 later. |
| F8 | Analytical telemetry is incomplete or test-capture-only. | Emit production `OracleTelemetry` metrics, spans, and structured logs for every required transition/refusal. |
| F9 | The first-message framing bound is applied incorrectly when multiple frames are coalesced. | Bound only the first framed message while preserving all following bytes. |
| S1 | The current gRPC server and Oracle client establish server-authenticated TLS, not mTLS. | Add mandatory peer-client certificate verification on the private listener and client identity on outbound channels. |
| S2 | Peer, stage, and tail tickets reuse the general workload JWT signing key. | Introduce a separately configured, rotatable peer-ticket keyring. |
| S3 | `GrpcTransportAdmissionService` polls a body frame before `OraclePeerGrpc::authenticate`. | Authenticate transport metadata before any request body is polled, buffered, or decoded. |
| S4 | Reserve/release and lifecycle operations do not uniformly require signed purpose authority. | Define and enforce a purpose domain for every private operation. |
| S5 | Scribe tail identity/permission/audit/release audience behavior is inconsistent. | Apply the shared peer context and explicit tail purpose tickets to list/acquire/page/release. |
| S6 | A Scribe-only peer role can avoid Oracle-specific outbound TLS validation. | Replace role-specific transport configuration with one role-neutral Bifrost peer transport owner. |
| S7 | Middle-stage scheduling can reread live membership and change participants mid-attempt. | Sign and retain one immutable participant/source snapshot per attempt. |
| S8 | Analytical supervision/readiness/shutdown is incomplete when mounted. | Register the peer listener and Analytical supervisor with server readiness and ordered shutdown. |
| S9 | Invalid peer traffic can amplify canonical audit work. | Authenticate cheaply before body admission; bound denial-audit concurrency and cardinality. |
| S10 | Production boot generates a fresh `NodeId` for Scribe even though operations authority requires stable Scribe identity coupled to its durable volume. | Persist/load one versioned Scribe node identity from the Scribe volume; mixed targets reuse it for their selected roles. Keep it independent from certificate identity. |
| D1 | `QueryClass::Analytical` is conflated with the selected execution engine. | Add a private `OracleExecutionPath::{Fragment, StageGraph}`; class remains an admission/resource property. |
| D2 | Comments/tests claim authenticated, mTLS, or test-only behavior that the actual route does not enforce. | Delete or rewrite drifted prose and assert the real boundary. |

## Locked unified architecture

### 1. One server owner, two listener trust zones

```text
                         wyrd-server
                 (one boot/readiness/shutdown owner)
                              |
             +----------------+----------------+
             |                                 |
     Public serving listener          Private Bifrost peer listener
     normal server TLS                mandatory peer mTLS
     public HTTP/gRPC                 private bind/advertise address
             |                                 |
      client/API auth                 PeerWorkloadAuthLayer
                                               |
                              AuthenticatedPeerContext
                                               |
                         +---------------------+--------------------+
                         |                                          |
               OraclePeerService adapter                 worker.WorkerService adapter
               forward/reserve/release/                  set-plan/execute-task/
               fragment control                          worker-info
                         |                                          |
                         +---------------------+--------------------+
                                               |
                              one ReservationRegistry
                              one Oracle resource root
                              one audit/telemetry owner
```

The private listener MUST NOT be implemented as a Vala listener, a second
binary, a second scheduler, or an independently booted service. It is another
socket supervised by the existing server.

The listener and service topology is closed:

| Target/roles | Private listener | Private services |
|---|---:|---|
| `all` / `server` (Scribe + Oracle) | yes | `OraclePeerService`, `ScribeTailService`, `worker.WorkerService`, `OracleLifecycleService` |
| `scribe` | yes | `OraclePeerService` for Scribe fragment operations and `ScribeTailService` |
| `oracle` | yes | `OraclePeerService`, `worker.WorkerService`, and `OracleLifecycleService` |
| `forge-worker` | no | none; Forge continues using its existing durable assignment/lease path |

`OraclePeerService` stays one generated adapter. On a role-specific node,
operations whose local capability is absent fail closed after authentication
and purpose verification; do not fork the protobuf service by role.

The public router contains Gate ingest, OTLP, Vala/Bifrost query, and intended
public health/reflection only. The private router contains only the services in
the matrix and exposes no reflection or public health service. Public
`/readyz` remains the operator readiness surface and includes private-listener
state in each selected Scribe/Oracle readiness entry.

Peer listener activation is derived from selected roles, not `ServeMode`.
Thus an HTTP-only public mode still starts the required peer listener for a
Scribe- or Oracle-bearing target. The canonical deployed peer port is
`50052`; public gRPC remains `50051`.

### 2. Role-neutral peer configuration and identity

Introduce one cohesive server-owned `BifrostPeerConfig` (exact module placement
may follow the nearest configuration owner) containing:

- private bind address, defaulting to `0.0.0.0:50052` in deployed examples
  and to an ephemeral loopback address in tests;
- advertised peer URI/address;
- peer CA trust roots;
- peer workload certificate chain and private key;
- expected peer server DNS name;
- peer workload credential/public identity;
- peer-ticket keyring configuration;
- bounded denial-audit concurrency/queue policy;
- existing transport size, timeout, and keepalive settings that are genuinely
  common to peer traffic.

The identity model is binding and deliberately separates four concerns:

| Identity | Meaning | Authority |
|---|---|---|
| Peer certificate | Membership in this deployment's Bifrost peer workload trust domain | Admits the private transport only |
| Peer Service principal | The exact configured `SYSTEM_OWNER` Wyrd Service allowed to call peer RPCs | Workload authentication plus `bifrost.peer.invoke` |
| Runtime node incarnation | `(NodeId, ClusterRole, fencing_token)` | Receiver-resolved live membership |
| Purpose ticket | One exact operation between fenced source/destination nodes for one tenant/query/body/deadline | Operation authorization |

The certificate MUST NOT encode or be compared to Wyrd's runtime `NodeId`.
`NodeId` is an application identity whose lifecycle follows role authority:

- a Scribe-bearing target loads one stable `NodeId` coupled to its persistent
  Scribe volume, and mixed/server/all targets reuse it for their selected
  roles;
- an Oracle-only target may create a new `NodeId` for a new process
  incarnation; and
- every role incarnation still receives and validates its own membership
  fence.

The stable Scribe identity is stored as a versioned, checksummed file in the
configured Scribe durable root. First creation uses write + fsync + atomic
rename + parent-directory fsync. Startup fails closed on malformed,
unsupported, or ambiguous identity state. Rescheduling without the volume is
node loss and creates a new identity only as part of the existing recovery
contract.

The peer certificate instead proves membership in a dedicated Bifrost peer CA
trust domain. This matches the Wyrd security rule that certificate identity
admits the transport while a signed ticket authorizes an operation.

The peer certificate is one deployment-provided leaf usable by the
`wyrd-server` process as both server and client identity. It MUST:

- chain to the dedicated Bifrost peer CA;
- contain both `serverAuth` and `clientAuth` extended key usages;
- carry a DNS SAN matching the configured peer server name;
- never be accepted by the public listener as a substitute for a Wyrd token;
  and
- be recorded only by a SHA-256 fingerprint or equivalent non-secret digest
  for audit/trace correlation.

Every outbound channel verifies the server chain and configured DNS SAN and
presents the same process's client identity. Every inbound connection requires
a client certificate chaining to the peer CA. CA membership is transport
admission, not application authorization.

Peer CA/certificate/key configuration is static for this task. Rotation uses a
rolling restart: publish trust overlap first, roll new identities, discard old
channel caches/connections, then remove retired trust after no old identity can
remain. Do not add hot-reload, SPIFFE/SPIRE, certificate issuance, or
certificate-bound access tokens in this remediation.

The Wyrd workload credential remains mandatory. It MUST authenticate as the
one configured card-bound peer `Service` principal under
`DataTenantId::SYSTEM_OWNER` and carry only the typed peer permission. The
receiver compares the verified principal/card identity, not a caller header.
The Service principal's system tenant is control-plane identity; the data
tenant and space for an engine operation come only from the verified purpose
ticket and receiver-owned reservation/cut.

Replace Oracle-specific peer TLS/config/environment names rather than keeping
aliases. The canonical environment projections are:

- `WYRD_BIFROST_PEER_BIND_ADDR`;
- `WYRD_BIFROST_PEER_ADVERTISE_ADDR`;
- `WYRD_BIFROST_PEER_CA_CERTIFICATE_PATH`;
- `WYRD_BIFROST_PEER_CERTIFICATE_CHAIN_PATH`;
- `WYRD_BIFROST_PEER_PRIVATE_KEY_PATH`;
- `WYRD_BIFROST_PEER_SERVER_NAME`;
- `WYRD_BIFROST_PEER_API_KEY`;
- `WYRD_BIFROST_PEER_TICKET_ACTIVE_KEY_ID`;
- `WYRD_BIFROST_PEER_TICKET_SIGNING_KEY_PATH`;
- `WYRD_BIFROST_PEER_TICKET_VERIFYING_KEYRING_PATH`.

`BifrostPeerConfig` MUST contain one typed `PeerTicketKeyringConfig` with
`active_key_id`, `signing_key_path`, and `verifying_keyring_path`.
Inline private-key environment values are prohibited.

The signing-key path contains one PKCS#8 PEM Ed25519 private key. The verifying
keyring path contains a versioned JSON document:

```json
{
  "version": 1,
  "keys": [
    {
      "keyId": "peer-2026-09",
      "publicKeyPem": "-----BEGIN PUBLIC KEY-----...",
      "verifyUntil": null
    },
    {
      "keyId": "peer-2026-08",
      "publicKeyPem": "-----BEGIN PUBLIC KEY-----...",
      "verifyUntil": "2026-09-08T00:00:00Z"
    }
  ]
}
```

The active key ID MUST appear exactly once with `verifyUntil: null), and its
public key MUST match the configured signing private key. Every retired key
MUST have a unique ID and a finite UTC `verifyUntil`; expired retired entries,
duplicate IDs, unknown versions/algorithms, malformed PEM, and active-key
mismatch fail startup. Issuance uses only the active private key and writes its
key ID into every ticket. Verification accepts the active public key and a
retired key only through its inclusive `verifyUntil`; unknown or expired key
IDs fail before body admission or IO.

Rotation is publish-before-use: publish the next public key to all replicas,
roll the active key ID and matching private key, retain the old public key with
one bounded `verifyUntil), then remove it after the overlap. Deployment and
rollback manifests MUST project all three inputs from peer-ticket-specific
Secrets and MUST NOT reuse the north-south workload/JWT signing secret.

Do not add aliases for the replaced Oracle-specific names. Startup MUST fail
closed for every Scribe- or Oracle-bearing target when any CA, certificate,
key, server name, advertised address, exact Service principal, workload
credential, or ticket keyring input is missing or invalid. Forge-only does not
construct peer credentials or require peer configuration.

### 3. Authenticate once before body admission

Add one server-owned `PeerWorkloadAuthLayer` in front of all services mounted
on the private listener. It MUST:

1. complete mTLS and extract the verified peer workload certificate digest;
2. parse and verify the peer workload credential from request metadata;
3. require the peer service principal shape and permission
   `bifrost.peer.invoke`;
4. require the exact configured peer Service principal/card identity;
5. create and attach a typed `AuthenticatedPeerContext` to request
   extensions; and
6. reject before polling, buffering, decompressing, or protobuf-decoding a
   request body.

`AuthenticatedPeerContext` is the only handler input for admitted peer
identity. It MUST contain typed, non-secret values sufficient for authorization
and audit:

- control-plane tenant (`SYSTEM_OWNER`);
- authenticated service principal/public identity;
- certificate identity/fingerprint or an equivalent stable digest;
- credential/token identifier digest;
- granted peer permission set;
- authentication timestamp/trace correlation.

The context MUST NOT claim a data tenant, space, source `NodeId`, or fence.
Those are operation authority resolved after ticket verification. Handlers
MUST NOT reparse bearer metadata, construct synthetic principals, or fall back
to an arbitrary system owner. Denials use bounded error codes and bounded
telemetry labels. Canonical denial-audit work MUST be concurrency bounded so
invalid traffic cannot create unbounded tasks or memory.

The layer ordering is binding:

```text
TCP/TLS -> peer certificate verification -> workload metadata authentication
        -> transport admission limits -> purpose-ticket verification
        -> reservation/lease lookup -> protobuf/domain decode requiring IO
        -> operation
```

Only the minimal metadata required for authentication may be read before
transport admission. The implementation MUST prove that an invalid credential
cannot cause a request body poll.

### 4. Independent peer-ticket keyring

Create a dedicated `PeerTicketKeyring`; do not reuse the user/API JWT key.
The keyring has:

- one active private signing key;
- the active public verification key;
- zero or more explicitly retained retired public verification keys;
- a bounded rotation overlap;
- key IDs carried in signed tickets;
- fail-closed unknown/expired key handling.

The existing focused authority types—stage, reservation/fragment, tail, and
lifecycle authority—remain focused types. They share signing and verification
through the keyring; do not replace them with a god claim structure.

Tickets MUST bind, as applicable:

- version and purpose domain;
- tenant and space;
- issuing leader and authenticated peer;
- query ID and attempt ID;
- reservation ID and generation/fence;
- stage/task/partition identity;
- immutable participant/source digest;
- exact request-body digest;
- not-before and expiry/deadline;
- nonce/replay identity.

Verification occurs before reservation consumption or any storage/provider/
runtime IO. Replay state is bounded by ticket expiry and operation semantics.

### 5. One outbound peer transport

Generalize the existing `TonicOraclePeerTransport` and Oracle-specific
credentials into a role-neutral Bifrost peer transport owner. A proposed
concrete shape is:

```text
BifrostPeerTransport
  - authenticated mTLS channel cache/resolver
  - ServerBifrostPeerCredentials
  - PeerTicketKeyring signer
  - immutable destination/node validation
  - OraclePeerService client adapter
  - WorkerService client adapter
  - Scribe tail/lifecycle client adapters where already required
```

One channel owner supplies:

- peer CA and server-name verification;
- client certificate and private key;
- workload authorization metadata;
- deadlines, cancellation, message limits, keepalive, and tracing;
- destination identity validation against the immutable participant snapshot.

Remove the upstream `DefaultChannelResolver` path from Wyrd Analytical
execution. Upstream wire messages may remain, but all channels MUST pass
through Wyrd's authenticated transport owner.

### 6. Every private operation has explicit purpose authority

Define purpose domains for every operation reachable on the private listener:

- forward query;
- reserve slots;
- release reservation;
- execute Fragment;
- set plan;
- execute task;
- tail list;
- tail acquire;
- tail page;
- tail release;
- lifecycle list/get/cancel where mounted.

`GetWorkerInfo` is deliberately not an allowed peer operation. The pinned
upstream coordinator has no caller for it, it carries no graph identity, and
current Wyrd ingress already refuses it. Keep the generated method for upstream
wire compatibility but return a closed refusal before calling the upstream
worker. Do not mint a purpose ticket for it.

Authentication establishes who called. A purpose ticket establishes what exact
operation/body the caller may perform. No operation may rely on network
placement, mTLS alone, a generic bearer, or handler-specific implied trust.

The permission becomes `bifrost.peer.invoke`. Remove
`bifrost.oracle.peer.invoke`; do not preserve a compatibility alias. Update
the typed permission catalog, generated schemas/docs, bootstrap credentials,
tests, and examples in the same change.

### 7. Extend the existing reservation registry

Do not introduce a second registry. Extend the existing
`ReservationRegistry` with an explicit closed reservation kind:

```text
Reservation
  +-- FragmentLease
  |     existing Fragment execution resources
  |
  +-- GraphLease
        public_query_id + datafusion_query_id
        leader + immutable participant/source digest
        destination worker node + fence
        tenant + space + query class
        reservation generation/fence
        signed-authority digest
        slots and admission permit
        query runtime/task context
        shared memory pool
        exchange child budget/account
        qualified spill/scratch share
        cancellation token
        immutable deadline
        pending -> active -> released/cancelled/expired state
```

Reserve creates a pending entry only after authenticated, authorized,
body-bound authority passes. The leader reserves exactly one GraphLease on
each selected remote Oracle follower for the complete DataFusion graph. The
leader itself reuses the already-admitted query owner and MUST NOT create a
second local reservation or query envelope.

The first authorized `SetPlan` **or** `ExecuteTask` for the graph atomically
activates the pending GraphLease and transfers its resources into one
follower-owned active graph record. `ExecuteTask` is allowed to arrive first
because the pinned upstream worker waits for later plan installation.
Activation is idempotent only for the same complete graph/reservation/
principal/authority tuple and never reacquires resources.

An active GraphLease is not consumed by an RPC. It retains:

- zero or more long-lived coordinator channels, because upstream opens one
  `SetPlan` channel per routed stage task;
- zero or more concurrent `ExecuteTask` calls and their returned partition
  streams;
- per-task cache and structured driver ownership;
- attempt zero and, when eligible, attempt one under the same graph runtime,
  cut, resources, cancellation tree, and absolute deadline.

Each stage RPC still carries a fresh single-use purpose ticket binding the
attempt, stage, task, body, destination, and lease. Ticket replay is refused;
multiple distinct authorized tasks do not count as lease replay.

Pending expiry remains the existing bounded reservation rule:
`min(requested_expiry, now + PENDING_TTL)`. Once active, the immutable query
deadline is authoritative. If an `ExecuteTask` activates a lease before any
`SetPlan`, plan installation must occur within the pinned upstream wait bound
(currently ten seconds) and before the query deadline. Failure to install the
plan cancels the waiting task, invalidates its cache entry, joins cleanup, and
releases the otherwise idle graph lease.

The active record tracks coordinator-channel count, active task/partition
streams, attempt children, cache invalidation, exchange ownership, and nested
resource idleness. Normal graph completion, explicit release, cancellation,
deadline, early-plan timeout, and shutdown all converge on one idempotent
cleanup transition. The registry may remove the active record only after:

1. every coordinator channel has ended;
2. every task and returned partition stream has settled;
3. both attempts and all structured drivers have joined;
4. task-cache invalidation has completed; and
5. exchange, spill, and other nested reservations report idle.

Cleanup failure or timeout is a terminal query/shutdown failure and leaves the
owner retained for forced shutdown accounting; it MUST NOT log and silently
release resources whose children remain live.

Unknown, wrong-graph, wrong-attempt-ticket, wrong-principal,
wrong-destination/fence, wrong-authority, expired, replayed, released, or
cancelled requests fail before worker state, cache lookup, provider creation,
or IO.

The reservation wire contract MUST gain the exact graph/authority fields
needed for this behavior. Contract changes belong in the existing typed Wyrd
owner, receive schema/codegen updates, and are not emulated with free-form
metadata.

The typed GraphLease reservation request carries:

- public query ID and DataFusion query ID;
- leader node/fence and destination worker node/fence;
- data tenant and space;
- query class and slot/resource demand;
- snapshot, permission, participant/source, and graph-authority digests;
- immutable query deadline and requested pending expiry.

The follower generates and returns the typed `ReservationId`, reservation
generation/fence, and effective pending expiry. Subsequent stage tickets bind
that complete follower-issued identity plus attempt/stage/task. Release binds
the same graph, leader, destination, generation/fence, and authority digest.
There is no caller-supplied string reservation ID.

### 8. Separate query class from execution path

`QueryClass` remains the admission/resource class. Add a private closed
execution choice owned by Oracle:

```rust
enum OracleExecutionPath {
    Fragment,
    StageGraph,
}
```

The exact visibility and location follow the owning Oracle handle, but the
semantics are fixed:

- production `query_sql` selects `Fragment` throughout this remediation;
- the test-support inactive entry point selects `StageGraph`;
- classification as `QueryClass::Analytical` does not itself select
  StageGraph;
- Task 2 is the only work authorized to make StageGraph production-reachable.

Oracle pods are horizontally interchangeable. There is no Interactive Oracle
pod role, Analytical Oracle pod role, query-type Service, or query-type
membership row. Every Oracle-bearing `wyrd-server` process:

- may accept a public query and coordinate it;
- may execute Fragment work for the production Interactive path;
- mounts the private Worker adapter needed to execute StageGraph follower work;
- advertises one exact fenced peer address; and
- can lead one query while following another.

Scaling from one to N Oracle pods is therefore only a deployment replica-count
and membership change. Query class and execution path select admission and
engine behavior after a request reaches an Oracle; they never select a
different pod type. Do not add Interactive/Analytical pod labels, Services,
targets, listener variants, or transport owners.

This removes the false claim that all production queries classify Interactive
without prematurely activating the new engine.

### 9. Restore the private Analytical execution seam

Restore:

- `AnalyticalExecutionRequest`;
- `AnalyticalExecutionHandle::execute_inactive`;
- `AnalyticalExecution` (or the exact result/owner type the deleted seam
  previously supplied).

These are private Oracle/Vala integration types, not public API. They MUST bind
the admitted request, immutable query snapshot, graph, attempt authority,
reservation set, runtime resources, result stream/drain, telemetry correlation,
and structured cleanup owner. Task 2 must be able to call this seam without
rebuilding T1 state or bypassing authentication/reservation.

### 10. Immutable participants and Scribe sources

Attempt creation snapshots the selected Oracle participants and Scribe source
locations once. Sign their canonical digest into authority. Every transport
destination and stage placement is checked against that snapshot.

Middle-stage scheduling MUST NOT reread live membership. Membership changes may
cause the attempt to fail and retry, but cannot silently change an in-flight
attempt.

Only Oracle nodes execute DataFusion stages. Oracle workers read remote Scribe
tails through the existing authenticated tail service and its acquired,
attempt-qualified tail authority. Do not deploy the Analytical worker on
Scribe or create another data path.

### 11. Exact runtime, exchange, spill, deadline, and cancellation

Follower execution must use the resources transferred by the GraphLease:

- the lease's query runtime/task context;
- the lease's shared memory pool;
- one child exchange account derived from the query budget;
- a qualified spill share from `OracleSpillRuntime`;
- the lease cancellation token;
- the lease immutable deadline.

Exchange accounting charges actual queued/owned Arrow buffers and messages and
releases bytes when ownership leaves the buffer. A fixed or estimated exchange
charge is not acceptable evidence.

Cancellation/deadline must stop follower stages, unblock exchange waiters,
abort storage/provider work where supported, and converge on the same joined
cleanup path. No ad hoc Tokio runtime, unbounded spawn, or detached cleanup is
allowed.

### 12. Retry and structured attempt ownership

The restored execution handle owns exactly one automatic retry when all are
true:

- failure is classified as peer loss or another explicitly enumerated
  retryable pre-egress transport failure;
- zero result batches/rows have crossed the caller egress boundary;
- the immutable query snapshot and plan remain valid;
- the original query deadline has sufficient remaining time.

Before retry:

1. cancel attempt zero;
2. stop and join every stage/driver/collector/connection task;
3. release attempt-local resource children and invalidate attempt task-cache entries;
4. release attempt-owned exchange allocations and spill files while retaining
   the graph's budgets and qualified spill share;
5. record the terminal attempt outcome;
6. create attempt one with a new attempt ID and fresh per-operation ticket
   nonces while retaining the same graph leases, runtime, resource envelope,
   participant/source cut, snapshot, permissions, cancellation tree, and
   absolute deadline.

Step 3 releases attempt-local reservation children and task-cache entries; it
does not release the graph-scoped follower GraphLeases. If a follower lost its
active GraphLease, changed fence, or left the immutable participant cut, the
retry fails terminally rather than reserving a replacement or widening the
participants.

There is no retry after egress and no third attempt. The attempt owner retains
all attempt join handles and the graph owner retains the leases. Dropping the
result stream, client
cancellation, deadline, server shutdown, normal completion, and retry all call
the same idempotent drain path.

### 13. Physical evidence and production telemetry

Evidence is captured from the execution that produced the drained result, not
from plan text or a separate fixture. Bounded post-drain evidence MUST identify:

- participant nodes and stages executed;
- physical join operators;
- partial and final aggregate operators;
- provider projection/filter/limit pushdown;
- exchange bytes/messages charged and released;
- actual spill write/read/file/byte activity by a DataFusion operator;
- retry count and attempt outcome;
- residual reservations, tasks, exchange bytes, cache entries, and spill files.

Production `OracleTelemetry` owns counters, histograms, gauges, spans, and
structured logs for admission, auth refusal, ticket refusal, reservation
transitions, stage/task lifecycle, retry, cancellation/deadline, exchange,
spill, cleanup, and result drain. Labels are bounded enums/roles/outcomes.
Never label with SQL, plans, secrets, raw token/certificate values, query IDs,
attempt IDs, reservation IDs, table names, or arbitrary error strings. Those
identifiers may be scrubbed span fields where repository policy permits.

Test capture observes production telemetry; it does not implement a parallel
telemetry path. Gauges MUST return to baseline after success and every failure.

### 14. Correct first-message framing

The transport admission parser MUST:

- parse the first complete gRPC frame header;
- apply the first-message byte bound to that frame's declared payload only;
- preserve any coalesced bytes belonging to later frames;
- reject malformed, compressed-when-forbidden, or oversized first frames
  without forwarding body bytes;
- preserve normal streaming semantics after first-frame admission.

Tests MUST cover a split header, split first payload, exact-boundary payload,
oversized payload, and first plus one or more coalesced subsequent frames.

### 15. Bind, advertise, deploy, become ready, and shut down as one owner

`wyrd-server` binds the peer socket before publishing any Scribe or Oracle
membership as ready. Membership already stores one private service address per
role, so no new SQL or public wire schema is required. Both Scribe and Oracle
publish the resolved peer listener address; they do not publish the public gRPC
address.

Boot ordering is binding:

```text
validate typed peer config and secrets
  -> construct auth/key/resource owners
  -> bind public sockets and private peer socket
  -> start supervised peer serve task
  -> publish Scribe/Oracle membership with ready=false
  -> complete role recovery/reconciliation
  -> mark role and peer dependency ready
  -> public /readyz may report ready
```

A peer bind failure is a boot failure. Unexpected peer serve-task exit is a
terminal supervision failure. Readiness for Scribe or Oracle becomes false
when peer credentials, ticket keys, listener state, advertised endpoint, audit
path, or required role capability is unhealthy.

Shutdown ordering is binding:

```text
remove public and role readiness
  -> mark Scribe/Oracle membership draining
  -> close public and peer admission
  -> cancel/join graph, fragment, tail, and lifecycle work
  -> release registries/resources
  -> stop and join peer listener within the server deadline
```

The Kubernetes examples and rollback manifests are part of this remediation.
They MUST:

- replace stale `WYRD_ROLES` with the exact `WYRD_TARGET`;
- expose public gRPC on `50051` and peer gRPC on `50052` as distinct named
  container ports;
- set pod-reachable bind addresses and publish each pod's exact peer address
  into `WYRD_BIFROST_PEER_ADVERTISE_ADDR`;
- keep the peer server name constant and certificate-SAN verified even when
  the membership endpoint is a pod IP or per-pod DNS name;
- mount the dedicated peer CA, dual-EKU peer certificate/key, peer workload API
  key, and peer-ticket keys read-only from deployment secrets;
- make the internal peer Service headless and target only port `50052`, with
  no load-balanced dispatch use and no Gateway route;
- apply default-deny peer ingress and permit only selected Wyrd peer pods/
  namespaces/service accounts on `50052`;
- preserve public ingress on the public ports only;
- model Scribe-bearing replicas as stable-volume workloads (StatefulSet in the
  repository examples) so the versioned node-identity file, WAL, and staged
  runs remain coupled to the replica;
- use persistent Scribe WAL/staging and Oracle audit-WAL volumes; and
- carry the same listener/config/secret contract in rollback manifests.

A load-balanced ClusterIP is not a valid membership address because a selected
`NodeId` and fence must dial that exact replica. Deployment examples MUST use
an exact per-pod address (for example, a downward-API pod IP or stable per-pod
DNS) while TLS verifies the configured shared Bifrost peer server name.

The test harness gains independent public-gRPC and peer addresses, a
client-auth-capable TLS fixture, advertisement of the actual bound peer
address, restart handling, and peer-listener leak/shutdown inspection.

## Required cleanup and deletion ledger

The implementation is incomplete until obsolete paths are removed. Delete or
replace all of the following:

1. Oracle and Worker service mounts on the public/shared gRPC router.
2. Comments/tests claiming the public router or Noop interceptor provides peer
   authentication.
3. Handler-local bearer parsing in `OraclePeerGrpc::authenticate` after the
   common pre-body layer is authoritative.
4. Analytical use of upstream `DefaultChannelResolver`.
5. Oracle-only outbound credential/config owners superseded by
   `ServerBifrostPeerCredentials` and `BifrostPeerConfig`.
6. Old Oracle-specific peer environment/config names; no compatibility aliases.
7. `bifrost.oracle.peer.invoke`; replace it with
   `bifrost.peer.invoke` across catalogs, docs, generated artifacts, and
   fixtures.
8. Reuse of the user/API workload JWT signing key for stage, tail,
   reservation, fragment, or lifecycle purpose tickets.
9. Any private RPC that accepts mTLS/bearer identity without an operation- and
   body-bound purpose ticket.
10. Inconsistent Scribe tail ticket audiences and release authorization.
11. Log-only Scribe peer authorization denials that continue execution.
12. Synthetic reservation IDs, permissions, principals, participant snapshots,
    and deadlines in the real inactive execution path.
13. Follower-local “fresh” runtime/resource grants created after reservation.
14. Any second leader admission permit/resource root created for the same
    attempt.
15. Caller-supplied free-form reservation identity where the registry can
    provide a typed value.
16. Mid-attempt live membership lookup for stage placement or transport
    destinations.
17. Fixed/estimated exchange charges standing in for owned buffer accounting.
18. Detached `tokio::spawn` stage, cleanup, connection, or metrics work.
19. Manual temporary-file writes presented as proof of DataFusion spill.
20. Test-only lifecycle/telemetry events that duplicate production
    `OracleTelemetry`.
21. Harness shortcuts that invoke worker methods without the real mTLS
    transport, peer context, purpose ticket, and reservation lookup.
22. Stale `distributed_compat` or similar prose describing an authentication
    boundary that no longer exists.
23. Assertions that production traffic always classifies Interactive.
24. Any production-reachable StageGraph selection introduced by T1.
25. Any Vala-owned socket/listener, independent peer supervisor, second
    scheduler, second reservation registry, or second audit owner.
26. Any use of a system-owner fallback for authenticated peer operations.
27. Any startup/readiness path that can advertise peer readiness before the
    private listener, credentials, keyring, reservation registry, and
    Analytical supervisor are ready.
28. Any Kubernetes Service or NetworkPolicy that labels port `50051` as the
    private peer boundary.
29. Stale `WYRD_ROLES` deployment configuration and any manifest that omits
    the peer secret/port contract for a Scribe- or Oracle-bearing target.
30. Any purpose-authorized `GetWorkerInfo` path; retain the generated method
    only as a fail-closed upstream compatibility surface.
31. Any attempt-scoped or RPC-scoped GraphLease model, including tests or prose
    that expect retry to replace the graph reservation.
32. Unconditional production `NodeId::generate()` for a Scribe-bearing
    target and any Scribe deployment using ephemeral identity/WAL storage.

Do **not** delete:

- the existing production Fragment engine;
- either required wire adapter (`OraclePeerService` or
  `worker.WorkerService`);
- the existing `ReservationRegistry` or Oracle resource owners—extend them;
- the existing authenticated Scribe tail service—unify it;
- the raw-SQL journeys and production telemetry owners—strengthen them;
- the private StageGraph seam required by Task 2.

## Expected write surface

The implementer MUST confirm exact symbols with CodeGraph before editing. The
expected owners are:

- `crates/wyrd/wyrd-server/src/grpc/mod.rs`: split public/private router
  composition, pre-body auth ordering, and private listener supervision hooks.
- `crates/wyrd/wyrd-server/src/app/server.rs`,
  `src/app/supervise.rs`, and `src/components/health/mod.rs`: bind and
  supervise the peer socket, expose role-scoped readiness, and drain/join it
  under the existing server deadline.
- `crates/wyrd/wyrd-server/src/config.rs`: canonical
  `BifrostPeerConfig`, target-derived activation, address collision and TLS
  completeness validation, and removal of Oracle-specific aliases.
- `crates/wyrd/wyrd-server/src/oracle/peer_service.rs`: consume
  `AuthenticatedPeerContext`, focused purpose verification, and remove
  duplicate bearer parsing.
- `crates/wyrd/wyrd-server/src/oracle/peer_credentials.rs`: generalize or
  replace with role-neutral server peer credentials.
- `crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs`: extend
  `ReservationRegistry`, unify outbound transport, and remove synthetic/fresh
  grants.
- `crates/vala/vala-bifrost-redux/src/oracle/analytical_transport.rs`: route
  WorkerService through the unified transport and active GraphLease.
- `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs`: restore the
  execution seam, structured graph/attempt lifecycle, retry, immutable
  participants, early-task cleanup, and evidence.
- `crates/vala/vala-bifrost-redux/src/oracle/analytical_supervisor.rs`: retain
  and join graph-, attempt-, connection-, task-, and cache-scoped owners.
- `crates/vala/vala-bifrost-redux/src/oracle/mod.rs`: one Oracle composition
  owner, execution-path selection, readiness, and shutdown.
- `crates/wyrd/wyrd-server/src/boot/mod.rs`: role-neutral configuration,
  credentials, keyring, listener boot, readiness, and shutdown.
- `crates/wyrd/wyrd-tonic/src/server/mod.rs`: reusable mTLS server support
  without moving Bifrost policy into the transport crate.
- `crates/wyrd-spec/src/vala/api.rs`: typed reservation/graph fields
  and permission catalog changes when owned there.
- `crates/vala/vala-bifrost-redux/src/oracle/**`: domain/runtime owners that
  must consume exact resources without owning listener/auth policy.
- `crates/wyrd/wyrd-testing/tests/bifrost/**`: real transport journeys and
  negative/security/resource cases.
- `crates/wyrd/wyrd-testing/src/server.rs` and
  `crates/wyrd/wyrd-testing/src/bifrost/cluster.rs`: independent peer bind,
  URL, TLS client identity, restart, and listener-lifecycle fixtures.
- `crates/wyrd/wyrd-testing/src/bifrost/process_cluster.rs`,
  `src/bin/bifrost_peer_test_node.rs`, and
  `tests/bifrost/oracle/peer_network/**`: separate-process simulated pods and
  the mandatory Tier-2 peer-network cases inside the existing Oracle target.
- `crates/wyrd/wyrd-testing/Cargo.toml`: register exactly one test-support
  binary target, `bifrost_peer_test_node`; do not add an integration-test
  target.
- `deploy/kubernetes/bifrost/deployment-mixed.yaml`,
  `deploy/kubernetes/bifrost/deployment-role-separated.yaml`, and matching
  rollback/static-check owners: port `50052`, exact targets, secrets,
  advertisement, persistent volumes, and NetworkPolicy.
- generated schemas/docs and `mise.toml` only where the changed contracts or
  existing canonical lanes require updates.

Do not create a new crate unless an existing owner is demonstrably incapable
of holding the cohesive behavior. That would be a material architecture change
and requires escalation.

## Required Tier-2 multi-process peer-network harness

The new peer network MUST be exercised across OS process boundaries in this
remediation. The existing `WyrdTestCluster` remains useful supporting coverage
for fast multi-node tests, but its nodes are Tokio tasks in one process and it
cannot satisfy the peer-network acceptance contract.

Add one cohesive `BifrostProcessCluster` test owner at
`crates/wyrd/wyrd-testing/src/bifrost/process_cluster.rs`. It MUST:

- launch the compiled
  `crates/wyrd/wyrd-testing/src/bin/bifrost_peer_test_node.rs` support binary
  through Cargo's `CARGO_BIN_EXE_bifrost_peer_test_node` path;
- never invoke Cargo recursively or shell-build a binary from a test;
- run every simulated pod as a distinct child PID with its own server
  composition, runtime, public listener, private peer listener, configuration,
  WAL, spill, audit, and temporary roots;
- share only the repository-managed PostgreSQL fixture, the test object-store
  root, the peer workload principal, the peer CA, and the peer-ticket trust
  manifest that real replicas share;
- bind each simulated pod to a distinct loopback address and use the canonical
  peer port `50052`, so membership contains one exact pod-like address rather
  than a load-balanced endpoint;
- issue a distinct server/client leaf certificate to each child from one test
  peer CA, with the common configured peer DNS SAN and both required EKUs;
- pass secret file paths through child environment, with files created under
  the child's private temporary root; never place private key material or API
  keys in process arguments, stdout, snapshots, or failure messages;
- use a private newline-delimited JSON control protocol over the child's piped
  stdin/stdout. The test-only protocol supports `Ready`,
  `ExecuteInactiveSql`, `Inspect`, and `Shutdown`; child logs go to
  stderr. It is not mounted on either network listener and is not a Wyrd wire
  contract;
- emit `Ready` only after both sockets are bound and selected role membership
  is ready at the exact advertised address;
- on drop or test failure, request normal shutdown, wait under the repository
  shutdown deadline, then kill and reap any non-responsive child. No child
  process may survive its test.

Register exactly one support binary in
`crates/wyrd/wyrd-testing/Cargo.toml`:

```toml
[[bin]]
name = "bifrost_peer_test_node"
path = "src/bin/bifrost_peer_test_node.rs"
test = false
bench = false
```

This binary target is required so Cargo provides
`CARGO_BIN_EXE_bifrost_peer_test_node` to the existing Oracle integration
test. It is not a new test target or command.

Each `ProcessNode` MUST own the child stdin, the child-reaper thread, the
stdout control-reader thread, and the stderr drain thread:

- stdout carries only bounded newline-delimited control messages. Its reader
  continuously drains the pipe, rejects an oversized line, parses it, and
  forwards it through an unbounded parent-local channel so a full channel
  cannot block the child pipe;
- stderr is continuously drained into a bounded in-memory tail. When full it
  drops the oldest complete lines; it never blocks the child and never records
  secrets;
- the child-reaper thread owns `std::process::Child`. It waits for normal
  exit while receiving a kill command over a channel; on kill it terminates
  and then unconditionally waits/reaps the child;
- normal teardown sends `Shutdown`, closes stdin, and waits for the exit
  notification under the server deadline. Timeout or protocol failure sends
  kill and waits for reaping;
- teardown joins the reaper and both reader threads before returning. Test
  failure and `Drop` use the same path, and a reader/reaper join failure is a
  test failure rather than detached work.

The process-cluster fixture MUST accept a requested Oracle replica count without
embedding a fixed maximum. Its required table-driven topologies are one, two,
three, and six Oracle child processes. The one-Oracle case proves the identical
configuration, local coordination/execution, peer listener, membership,
readiness, isolation, and terminal cleanup; it has no remote Oracle follower.
For every topology with two or more Oracles, each Oracle MUST coordinate at
least one query and follow at least one query coordinated by a different
Oracle. The fixture also supports:

- one Scribe process;
- follower crash and replacement with a new Oracle incarnation/fence;
- Scribe restart with the same durable root and stable `NodeId`; and
- direct inspection of PIDs, bound/advertised addresses, membership fences,
  accepted peer certificate fingerprints, attempt/resource ownership, and
  terminal cleanup.

Add one Oracle-target module tree:

- root:
  `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/mod.rs`;
- modules: `listener`, `security`, `transport`, `analytical`, and
  `support`;
- registration: `mod peer_network;` in the existing
  `tests/bifrost/oracle/main.rs`.

Do not add a Cargo **test** target or `mise` task. The one support binary
target above is required. The canonical command remains
`mise run test:bifrost:journey:oracle`, which runs the complete Oracle target
with repository-managed PostgreSQL and migrations. During RED/GREEN iteration,
an implementer may use this named nextest expression only while the
repository-managed test database is already running and migrated:

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(<exact-name>)'
```

The whole Oracle journey command remains the required acceptance evidence and
prevents a zero-match focused selector from substituting for completion.

This task's Tier-2 process-cluster acceptance MUST prove:

- one-, two-, three-, and six-Oracle topologies use the same configuration and
  owners except for process identity, addresses, roots, and replica count;
- the one-Oracle topology proves local coordination and the complete listener,
  identity, membership, readiness, isolation, and cleanup contract;
- when the topology has at least two Oracles, every Oracle child can coordinate
  a production Interactive query and an inactive Analytical query, and can act
  as a remote follower for a different query; no test assigns query-type-
  specific pod roles;
- all selected remote work crosses a real private TCP connection between
  distinct PIDs in topologies with at least two Oracles; a local/in-process
  transport cannot satisfy remote evidence;
- PostgreSQL membership maps each `NodeId` and fence to the exact child
  address that accepted the connection;
- mTLS, workload authentication, purpose authority, immutable destinations,
  GraphLease ownership, retry, physical execution, spill, telemetry, and
  cleanup operate across those process boundaries;
- killing or restarting a child changes reachability and membership/fence
  behavior without redirecting an exact-node request to another child; and
- public and peer listener isolation holds for every child.

No task in this plan launches Kubernetes. Kubernetes deploys the same
independently reachable server processes; the process-cluster network proof
plus static manifest checks are the required deployment evidence. Task 2 owns
the held-out Tier-1 public Rust/HTTP/generated-gRPC journeys already named in
`01-d2-route-activate-production-analytics.md`; those journeys activate the
public route and MUST reuse, not replace, this process-cluster peer-network
proof.

## TDD implementation sequence

This task has exactly eight required claims, eight implementation slices, and
eight named acceptance tests. They map 1:1. A slice starts by adding its named
acceptance test and observing the specified failure. Production work may begin
only after that RED result is recorded. GREEN means that same named test passes
without weakening it, and the owning capability lane remains green.

The eight acceptance tests are the task-level proof. Focused unit and
integration tests are still required where they make a branch deterministic,
but they are supporting evidence: they do not create extra claims or slices,
and they never replace the named acceptance test.

| Slice | Claim | Named acceptance test | Owner and canonical lane |
|---|---|---|---|
| 1 | T1-U-C01 | `peer_listener_is_isolated_mtls_and_role_complete` | `tests/bifrost/oracle/peer_network/listener.rs`; `mise run test:bifrost:journey:oracle` |
| 2 | T1-U-C02 | `peer_authentication_precedes_body_admission` | `tests/bifrost/oracle/peer_network/security.rs`; `mise run test:bifrost:journey:oracle` |
| 3 | T1-U-C03 | `peer_tickets_are_independent_exact_and_replay_safe` | `tests/bifrost/oracle/peer_network/security.rs`; `mise run test:bifrost:journey:oracle` |
| 4 | T1-U-C04 | `peer_transport_uses_immutable_fenced_destinations` | `tests/bifrost/oracle/peer_network/transport.rs`; `mise run test:bifrost:journey:oracle` |
| 5 | T1-U-C05 | `graph_lease_owns_exact_resources_for_complete_graph` | `tests/bifrost/oracle/peer_network/analytical.rs`; `mise run test:bifrost:journey:oracle` |
| 6 | T1-U-C06 | `analytical_retry_is_once_pre_egress_and_fully_drained` | `tests/bifrost/oracle/peer_network/analytical.rs`; `mise run test:bifrost:journey:oracle` |
| 7 | T1-U-C07 | `inactive_analytical_raw_sql_proves_physical_execution_and_spill` | `tests/bifrost/oracle/peer_network/analytical.rs`; `mise run test:bifrost:journey:oracle` |
| 8 | T1-U-C08 | `production_fragment_and_peer_deployment_remain_isolated` | `crates/wyrd/wyrd-testing/tests/bifrost/server/grpc_mount.rs`; `mise run test:bifrost:journey:server` |

For each row, the RED command is the canonical lane shown after adding the
named test but before production implementation. Record the failing assertion
and why it demonstrates the missing claim. The GREEN command is the same lane
after implementation. A different selector is acceptable only if repository
evolution replaces the canonical lane with an equivalent non-weaker `mise`
task; record that substitution.

The new `peer_network/mod.rs` MUST register all five declared modules, and
`oracle/main.rs` MUST register `mod peer_network;`.
`crates/wyrd/wyrd-testing/tests/bifrost/server/main.rs` MUST register
`mod grpc_mount;` before Slice 8 RED is recorded. Do not add a Cargo target
or `mise` task for these tests.

### Remediation Ledger

After the completion of a slice, the implementor MUST record the current RED/GREEN result, slice completion evidence and current state in
/Users/stevenforrester/Documents/GitHub/agent-workflows/wyrd/active/bifrost-distributed-analytics-engine/remediation/remediation-slice-ledger.md.

### Slice 1 / T1-U-C01 — peer topology and workload identity

RED test: `peer_listener_is_isolated_mtls_and_role_complete`.

The test MUST use `BifrostProcessCluster` to boot the closed target matrix and
prove, as table-driven scenarios, that:

- one `wyrd-server` lifecycle owns public and peer listeners;
- every simulated pod is a distinct child PID with distinct roots and actual
  public/private sockets;
- peer RPCs are unreachable on the public listener;
- missing CA/cert/key fails startup;
- missing, untrusted, or wrong-SAN certificates fail TLS;
- a trusted workload certificate alone grants neither runtime-node identity
  nor operation authority;
- readiness waits for the actual advertised peer socket and shutdown joins it;
- one-, two-, three-, and six-Oracle process clusters register exact pod-like
  addresses and establish authorized Oracle-to-Oracle and Oracle-to-Scribe
  peer connections without query-type-specific roles;
- every Oracle process can coordinate a public Interactive query and an
  inactive Analytical query; when at least two Oracles exist, every Oracle
  also follows a query coordinated by a different Oracle process;
- mixed/server, Scribe-only, Oracle-only, and Forge-only targets mount exactly
  the approved services;
- Scribe retains its volume-coupled `NodeId` across restart, advances its
  role fence, treats a new/missing volume as node loss, and rejects malformed
  or partial identity state.

Expected RED: the current single/public listener, server-authenticated TLS,
readiness, target matrix, or Scribe identity behavior violates at least one
assertion for the documented reason.

GREEN implementation:

- add role-neutral peer config and reusable mTLS server support;
- supervise public and peer listeners under the existing server owner;
- move Oracle, Worker, tail, and lifecycle services exclusively to the peer
  router;
- bind, advertise, become ready, and shut down through one owner;
- implement the approved target matrix and versioned atomic Scribe identity
  persistence.

Mutation check: removing client-certificate validation, mounting a peer service
publicly, treating the certificate as node/operation authority, publishing the
wrong peer address, or declaring readiness early MUST fail this test.

### Slice 2 / T1-U-C02 — shared authentication before body admission

RED test: `peer_authentication_precedes_body_admission`.

The test MUST drive both Oracle and Worker adapters between child processes
over the real peer listener, with a body-poll probe, and prove:

- missing/invalid credentials, wrong permission, wrong Service card/principal,
  and non-system control tenant cause zero request-body polls;
- the exact configured SYSTEM_OWNER peer Service principal creates one
  `AuthenticatedPeerContext` shared by both adapters;
- that context contains workload/control identity only, never a data tenant or
  caller-asserted runtime `NodeId`;
- fragmented and coalesced first gRPC frames are admitted without overread,
  truncation, or decode before authentication;
- refusal auditing is bounded and remains fail-closed under saturation.

Expected RED: at least one adapter authenticates after decode, creates a
different context, accepts an invalid principal, or mishandles first-frame
framing.

GREEN implementation:

- add `PeerWorkloadAuthLayer`, `AuthenticatedPeerContext`, and
  `bifrost.peer.invoke`;
- authenticate before the first body poll/decode;
- use the same typed context in both adapters;
- remove handler-local duplicate auth and system-owner fallbacks;
- implement bounded refusal audit and correct buffered first-frame replay.

Mutation check: moving auth behind admission/decode, omitting the shared
context from either adapter, or dropping buffered frame bytes MUST fail.

### Slice 3 / T1-U-C03 — exact purpose authority

RED test: `peer_tickets_are_independent_exact_and_replay_safe`.

The table-driven test MUST cover every allowed private Oracle, Worker, tail,
and lifecycle operation and prove:

- peer-purpose tickets use an independent keyring; a user/API key cannot
  validate them;
- purpose, canonical body digest, control/data tenant, space, source and
  destination node/fence, query, attempt, reservation, participant digest,
  nonce, key ID, and expiry are exact where the operation requires them;
- the active key and permitted retired verification keys pass; unknown or
  expired keys fail;
- replay of every state-changing operation fails before IO;
- `GetWorkerInfo` is always refused and has no internal caller.

Expected RED: the current authority/key/replay model accepts at least one
wrong binding, shares the workload key, or exposes an unauthorized method.

GREEN implementation:

- add `PeerTicketKeyring`, rotation, focused ticket domains, canonical body
  binding, and bounded replay state;
- apply the explicit purpose-operation matrix to every private method;
- keep `GetWorkerInfo` fail-closed.

Mutation check: removing any required binding, accepting replay, substituting
the workload key, or enabling `GetWorkerInfo` MUST fail.

### Slice 4 / T1-U-C04 — one fenced outbound transport

RED test: `peer_transport_uses_immutable_fenced_destinations`.

The test MUST prove:

- Oracle and Scribe callers use one role-neutral transport with identical
  mTLS, workload metadata, ticket, size, and deadline behavior;
- the source and destination PIDs differ for every remote acceptance case;
- plaintext and `DefaultChannelResolver` paths are impossible;
- a destination absent from the immutable attempt snapshot is rejected
  locally;
- membership churn cannot change an in-flight participant or Scribe source;
- killing and replacing a selected follower advances or replaces its fence,
  makes the frozen old destination fail, and never redirects that request to a
  different live child;
- destination node/fence/ticket tampering fails before network IO.

Expected RED: Worker traffic can use a default/plaintext path or live
membership can alter the attempt.

GREEN implementation:

- generalize the existing peer credentials/transport owner for both adapters;
- carry the immutable participant/source snapshot through the attempt;
- delete `DefaultChannelResolver` use and role-specific validation gaps.

Mutation check: reintroducing live membership lookup, a default resolver, or
role-specific trust behavior MUST fail.

### Slice 5 / T1-U-C05 — one exact graph lease

RED test: `graph_lease_owns_exact_resources_for_complete_graph`.

The test MUST execute the graph across leader and follower child processes and
prove:

- wrong graph, principal, destination, fence, authority, generation, or expiry
  fails before worker/cache/provider IO;
- the first authorized `SetPlan` or `ExecuteTask` atomically activates one
  follower-local `GraphLease`;
- all coordinator channels, tasks, partitions, and the optional retry reuse
  that same lease without reacquisition;
- the exact reserved runtime, memory pool, exchange account, spill share,
  cancellation token, deadline, and authority digest reach execution;
- an early task whose plan never arrives times out, joins, clears cache state,
  and releases the lease;
- success, mismatch, expiry, cancellation, timeout, and shutdown release every
  child and resource exactly once and restore all gauges to baseline.

Expected RED: the current attempt/RPC-scoped reservation or follower-local
resource construction breaks identity, lifetime, or cleanup assertions.

GREEN implementation:

- extend the existing `ReservationRegistry` with one graph-qualified lease;
- atomically transfer the reservation into graph ownership on the first valid
  stage method;
- register every coordinator/task/stream/cache child under the graph lease;
- delete synthetic values and follower-local grants;
- retain the lease through the optional retry and joined terminal cleanup.

Mutation check: a fresh follower grant, RPC-scoped lease, duplicate activation,
or release with a live child MUST fail.

### Slice 6 / T1-U-C06 — structured attempt and retry lifecycle

RED test: `analytical_retry_is_once_pre_egress_and_fully_drained`.

The test MUST submit raw SQL through the leader child's
`ExecuteInactiveSql` control command and prove that all remote stages cross
the real peer transport into a different child PID:

- one injected pre-egress peer loss triggers exactly one retry;
- attempt one starts only after attempt zero is joined and drained;
- first egress permanently disables retry;
- a second retry is refused;
- success, drop, cancel, deadline, peer failure, retry, and shutdown leave no
  tasks, leases, buffers, cache entries, exchange accounts, or spill files.

Expected RED: retry is detached from the real handle, attempts overlap, retry
occurs after egress, or terminal cleanup leaks.

GREEN implementation:

- restore `AnalyticalExecutionRequest`,
  `AnalyticalExecutionHandle::execute_inactive`, and
  `AnalyticalExecution`;
- make the handle own attempt tasks/resources and one explicit retry state
  machine;
- register Analytical supervision with readiness/shutdown;
- join and drain every terminal path before releasing the graph lease.

Mutation check: detaching a task, overlapping attempts, retrying after egress,
or releasing before drain MUST fail.

### Slice 7 / T1-U-C07 — real analytical result and production evidence

RED test: `inactive_analytical_raw_sql_proves_physical_execution_and_spill`.

The test MUST submit raw SQL through the leader child, execute follower stages
in one or more different child PIDs, and prove from post-drain runtime evidence:

- result equivalence for follower-executed join and partial/final aggregate;
- provider filter/projection/limit pushdown;
- qualified exchange activity;
- a real DataFusion operator performs spill write and read through the
  attempt's reserved spill share, then removes its files;
- production `OracleTelemetry` emits bounded correlated lifecycle/refusal
  signals and balances gauges.

Plan text, a manual temporary file, or test-capture-only telemetry is not
evidence.

Expected RED: current tests can pass without one of the physical operations,
qualified spill, or production telemetry.

GREEN implementation:

- expose bounded post-drain physical/runtime evidence from the owning structs;
- force a real operator spill under the exact graph lease;
- complete production `OracleTelemetry`; keep capture observational only.

Mutation check: replacing physical evidence with plan text, manual file IO,
unbounded labels, or capture-only events MUST fail.

### Slice 8 / T1-U-C08 — production and deployment isolation

RED test: `production_fragment_and_peer_deployment_remain_isolated`.

The test MUST prove:

- production `query_sql` and `QueryClass::Analytical` still select
  Fragment, never StageGraph;
- the private StageGraph handle exists only behind the approved test-support
  seam for Task 2;
- peer services remain unreachable on the public listener;
- mixed/server, Scribe-only, Oracle-only, and Forge-only manifests match the
  target matrix;
- port `50052`, advertised addresses, secrets, persistent volumes,
  Services, NetworkPolicies, readiness, shutdown, and rollback manifests form
  one reachable private deployment contract;
- stale permission/config names, duplicate owners, a second registry, and all
  deletion-ledger paths are absent;
- existing Fragment and Scribe journeys remain green.

Expected RED: current routing, manifests, stale logic, or cleanup violates at
least one isolation assertion.

GREEN implementation:

- complete the deletion ledger;
- register `mod grpc_mount;` in the server journey target;
- update deployment and rollback manifests and the existing deployment check;
- regenerate only source-derived contracts/docs that actually changed;
- retain only the private Task 2 activation seam.

Mutation check: a production StageGraph route, duplicate listener owner,
public peer mount, same-port Service, stale target variable, or missing peer
secret/volume/NetworkPolicy MUST fail.

## Required claims and acceptance evidence

All eight claims are `required`. Each claim is satisfied by its one named
acceptance test plus the listed integrated evidence; no other test may silently
substitute for it.

| Claim | Required obligation | Acceptance test | Additional integrated evidence |
|---|---|---|---|
| T1-U-C01 | One server-owned, role-complete private peer listener scales the same Oracle process shape from one to N replicas, supports each Oracle as coordinator or follower for Interactive and Analytical execution, enforces mTLS/readiness/shutdown/stable Scribe identity, and keeps certificate, node, and operation identities separate. | `peer_listener_is_isolated_mtls_and_role_complete` | Tier-2 process cases in the Oracle journey target; server journey; deployment check |
| T1-U-C02 | One exact SYSTEM_OWNER peer principal creates the shared typed context before any body poll/decode for both adapters, with correct first-frame buffering and bounded fail-closed refusal audit. | `peer_authentication_precedes_body_admission` | Tier-2 process cases in the Oracle journey target; production audit/telemetry capture |
| T1-U-C03 | An independent rotatable peer-ticket keyring enforces the complete operation matrix, exact bindings, expiry, and replay refusal; unused methods stay closed. | `peer_tickets_are_independent_exact_and_replay_safe` | Tier-2 process cases in the Oracle journey target; key-rotation and replay unit proofs |
| T1-U-C04 | One role-neutral outbound transport enforces mTLS and immutable fenced participant/source destinations across child processes for Oracle and Scribe without plaintext/default resolution. | `peer_transport_uses_immutable_fenced_destinations` | Tier-2 process cases in the Oracle journey target; Oracle and Scribe regressions |
| T1-U-C05 | The existing registry activates one exact graph lease whose reserved runtime, memory, exchange, spill, cancellation, deadline, authority, children, and cleanup cover a multi-process graph and optional retry. | `graph_lease_owns_exact_resources_for_complete_graph` | Tier-2 process cases in the Oracle journey target; resource-governance check |
| T1-U-C06 | The restored handle owns exactly one joined pre-egress retry across child processes and drains/releases every attempt resource on every terminal path. | `analytical_retry_is_once_pre_egress_and_fully_drained` | Tier-2 raw-SQL process case in the Oracle journey target; zero-residual snapshot |
| T1-U-C07 | Raw SQL proves correct follower join, aggregate, pushdown, exchange, real qualified operator spill, cleanup, and bounded production telemetry across child processes from post-drain evidence. | `inactive_analytical_raw_sql_proves_physical_execution_and_spill` | Tier-2 process cases in the Oracle journey target; result-equivalence and telemetry snapshots |
| T1-U-C08 | Production remains Fragment-only while the private Task 2 seam, deployment isolation, cleanup ledger, and existing Fragment/Scribe behavior remain intact. | `production_fragment_and_peer_deployment_remain_isolated` | Server, Oracle, and Scribe journey lanes; deployment/codegen checks |

The complete finding ledger is covered without creating one claim per defect:

| Claim | Findings closed |
|---|---|
| T1-U-C01 | S1, S8, S10, and the listener/identity part of D2 |
| T1-U-C02 | F9, S3, S9, and shared-context portions of S5 |
| T1-U-C03 | F1, S2, S4, and purpose-authority portions of S5 |
| T1-U-C04 | S6 and S7 |
| T1-U-C05 | F2, F3, and graph-resource portions of F5 |
| T1-U-C06 | F4, F7, and structured-attempt portions of F5 |
| T1-U-C07 | F6 and F8 |
| T1-U-C08 | D1, the remaining drift cleanup in D2, and deployment regression closure for S8 |

No LLM evaluation or human attestation substitutes for deterministic proof.
The test CA, per-process leaf identities, mTLS success/refusal matrix, and
multi-process peer traffic MUST run inside the Oracle journey target.

## Supporting test requirements

Supporting tests pin branches that are too narrow or fault-injection-heavy for
the eight journeys. Every supporting test MUST name one of T1-U-C01 through
T1-U-C08 in the implementer evidence report. It MUST NOT be presented as a
ninth acceptance test, claim, or slice.

- C01: TLS config parsing, identity-file atomicity/crash recovery, and target
  matrix unit/static tests.
- C02: zero-body-poll middleware probe, split/coalesced frame parser cases,
  denial-audit saturation, and exact context-shape tests.
- C03: ticket canonicalization, binding-field mutation, key rotation, expiry,
  nonce replay, operation matrix, and `GetWorkerInfo` refusal tests.
- C04: resolver prohibition, destination-fence mutation, membership churn, and
  Oracle/Scribe credential parity tests.
- C05: activation ordering, early-task timeout, multiple coordinator/task/
  partition ownership, exact resource pointer/account identity, idempotent
  release, and shutdown cleanup tests.
- C06: retry state-machine transitions, egress latch, task join ordering, and
  leak snapshots for every terminal path.
- C07: physical-operator capture, real spill read/write/cleanup, result
  equivalence, and bounded telemetry label/gauge tests.
- C08: route-selection, public-router exclusion, manifest/rollback parsing,
  stale-name/duplicate-owner checks, and existing Fragment/Scribe regressions.

Tests MUST first fail for the missing behavior. Do not weaken, ignore, delete,
or rewrite an assertion merely to make a lane green.

## Verification

### Diagnostics during implementation

Run the smallest relevant `mise` selector after each RED/GREEN slice. Raw
`cargo test` is permitted only for one pure unit test when no repository setup
is required; invocation still goes through `mise exec`.

### Claim evidence and integrated evidence

Because this remediation crosses server boot, TLS, auth, contracts, Bifrost,
Oracle, Scribe, DataFusion, generated artifacts, and CI boundaries, the broad
gate is warranted. Run:

```bash
mise run fmt
mise run lints
mise run check
mise run test:tonic
mise run test:unit
mise run test:bifrost
mise run test:bifrost:journey:oracle
mise run test:bifrost:journey:scribe
mise run test:bifrost:journey:server
mise run check:bifrost-oracle-deploy
mise run codegen:check
mise run check:bifrost-resource-governance
mise run docs:build  # only if architecture or docs-site files change
git diff --check
```

The existing `check`, `lints`, `test:unit`, and `gate` aggregates MUST
continue to prove the client-tier dependency boundary, tonic ownership, rustls
provider selection, unwrap audit, and the single pinned
DataFusion/Arrow/Parquet/object-store dependency cone. If repository evolution
introduces a narrower canonical `mise` task for one of those properties, run
it as well. If an exact named task above changes at implementation time, use
the current canonical task that owns the same surface and record the
substitution. Do not invent a parallel script or bypass a failing check.

The implementer report MUST include:

- inspected base and final commit;
- changed owners and deleted paths/symbols;
- RED command/failure and GREEN command/result for each slice;
- claim-to-test/command evidence for T1-U-C01 through T1-U-C08;
- raw-SQL result equivalence;
- authenticated peer identity and purpose matrix;
- child-process PID/address/certificate topology and proof that remote work
  crossed private TCP connections rather than the local transport;
- exact PostgreSQL membership address/fence to accepting-child evidence;
- follower crash/replacement and stable-volume Scribe restart evidence;
- certificate/workload-principal/ticket/runtime-node factor-separation evidence;
- exact GraphLease resource identity, activation, multi-channel/task ownership,
  retry reuse, and terminal cleanup evidence;
- pre-egress retry attempt timeline;
- post-drain zero-residual snapshot;
- physical operator and actual spill evidence;
- bounded telemetry snapshot;
- public/private routing and production StageGraph isolation proof;
- role/listener matrix, membership advertisement, deployment/rollback, and
  port/secret/NetworkPolicy evidence;
- all verification exit statuses and any justified canonical task
  substitutions.

## Non-goals

- Do not activate StageGraph for production traffic; Task 2 owns activation.
- Do not remove or redesign the production Fragment engine.
- Do not fork or modify upstream `datafusion-distributed` solely to add Wyrd
  auth; adapt its wire service at the Wyrd boundary.
- Do not create a second server, Vala listener, scheduler, resource root,
  reservation registry, auth system, audit pipeline, or telemetry system.
- Do not expose the private execution-path enum or Analytical seam as a public
  SDK/HTTP/MCP contract.
- Do not add compatibility aliases for replaced peer permission/config names.
- Do not broaden this task into generic service-mesh, certificate issuance, or
  cluster-membership redesign.

## Stop and escalate if

Stop only if repository evidence proves one of these material blockers:

- `wyrd-server` cannot supervise a second listener without creating another
  serving owner;
- the existing reservation registry cannot represent both closed reservation
  kinds without breaking the production Fragment contract;
- upstream WorkerService cannot be mounted behind Wyrd's transport/auth layers
  without an upstream fork;
- a required typed wire change would be externally breaking beyond the private
  peer contract;
- available DataFusion hooks cannot produce actual spill/operator evidence;
- Task 2 requires a public contract or production activation not authorized
  here.

Do not stop for private helper names, module placement within an existing
owner, test selector drift, or other ordinary implementation choices.

## Completion and downstream gate

This task is complete only when:

1. the full deletion ledger is satisfied;
2. T1-U-C01 through T1-U-C08 are green through their eight named acceptance
   tests and required supporting evidence;
3. the Oracle journey's Tier-2 multi-process cases, integrated raw-SQL
   execution, and existing Fragment/Scribe journeys pass;
4. the broad verification set passes;
5. `$wyrd-review` runs again in FINAL mode against the immutable remediation
   candidate and returns no unresolved blocking finding; and
6. the user accepts that re-review.

Task 2 remains blocked until all six conditions are met. Task 2 may then make
the already-authenticated private StageGraph seam production-reachable; it must
not rebuild or replace this peer architecture.
