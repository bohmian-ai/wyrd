# TASK-001 r4 Security and Tenancy Domain Review

## Boundary

Fresh review of the immutable complete diff from base
`80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2` to candidate
`a6429060fb011aafa4335f2f736c70adab231739` (tree
`64177d67141993ec18e63b43dc227dbc31d70950`) for the security-sensitive
TASK-001 surface only:

- table and sensitive-column authorization ordering relative to catalog,
  provider, peer, and file IO;
- tenant identity and refusal attribution across public HTTP, private gRPC,
  Oracle/Scribe peers, assignments, and distributed worker errors;
- physical-plan, stage-body, permission, and assignment-authority digests;
- Scribe malformed-Variant refusal before shard dispatch, WAL append, or ACK;
- gateway redaction, audit publication, and audit hash stability after the
  built-in Variant conversions;
- malformed wire/input handling and public error-detail exposure.

TASK-003's deferred physical-field declarations, `ScanLeafRef`, assignment-v8
wire changes, and shredded-leaf pruning are outside TASK-001. This review still
checked that TASK-001's Variant SQL contract version is bound into the existing
plan and stage digest boundaries and that the unchanged assignment authority is
validated before provider/tail IO.

## Authority and source coverage

Authority read: `AGENTS.md`; `architecture/agent-rules.md`;
`architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`;
`architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`;
`architecture/references/domain/analytical-operations-reliability.md`;
`changes/active/bifrost-variant/spec.md` revision 11; original
`TASK-001-variant-storage-and-query.md`; and the repository
`wyrd-task-review` skill.

Source coverage included the complete base-to-candidate diff and the candidate
versions of Oracle planning, sessions, codecs, peer claims, follower preflight,
Analytical transport, distributed errors and query terminals; server peer
authority/audit/service; Scribe ingress/decode/preprocess; gateway capture;
audit staging/projection; tonic conversion; client error reconstruction; and
the task's focused journeys and unit tests. CodeGraph was used first to locate
the execution and authority paths; candidate source was then read directly by
immutable commit.

## Verification limits

- This was a review-only static audit. I did not rerun the task's recorded test
  commands.
- The candidate records passing focused journeys, but the sensitive-expression
  journey observes follower graph leases rather than catalog object reads,
  Scribe live-route listing, or provider setup. It therefore cannot prove the
  required pre-IO ordering identified below.
- Mixed-binary Variant SQL mismatch was established from the digest preimages
  and receiver recomputation; no separate binary with a deliberately changed
  constant was built during this review.
- The current assignment wire remains v7 because revision 11 explicitly
  assigns the v8 physical-leaf/`ScanLeafRef` wire work to TASK-003. No TASK-001
  finding is based on that deferred work.

## Security Audit

### Critical

- None.

### High

- None.

### Medium

- `[crates/vala/vala-bifrost-redux/src/oracle/planner.rs:257; crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3089,3121,3321]` Sensitive-column authorization occurs after storage and peer work, contrary to TASK-001's pre-IO obligation. `prepare_query_attempt` pins the cut first; after table-scope authorization, `acquire_and_materialize` opens the selected metadata/manifests at `planner.rs:257-263`. `build_physical_root` then performs Scribe live-route discovery and provider registration at `mod.rs:3089-3123`. Only inside `plan_physical`, after those operations, does `authorize_payload_columns` reject a caller lacking `gateway:payload:read` at `mod.rs:3321`. An authenticated principal with scoped read access to `vala.gateway.calls` but without payload permission can repeatedly submit `SELECT request_payload ->> 'secret' ...`; no payload rows are returned, but each denied request can still acquire active-read state, read catalog objects, contact the frozen Scribe roster, and build providers. This violates TASK-001 lines 113 and 165-167 and the revision-11 sensitive-data invariant/acceptance at `spec.md:966-971,1146-1150`, permits unauthorized storage/peer resource consumption, and creates timing/availability exposure at a boundary that is required to fail before IO. **Fix:** split schema-only logical planning from cut materialization. Resolve the tenant-bound table identities and register non-reading schema providers, optimize the logical plan, and run the sensitive-column gate before `materialize_acquired_cut`, `discover_live_routes`, and runtime provider construction; only an authorized plan may continue into those operations. Add a negative journey for whole-column, `->`, `->>`, and `to_json` reads that instruments catalog object reads, Scribe live listings, provider creation, follower leases, and row reads and asserts all remain zero on `QueryForbidden`.

### Low / Defense In Depth

- None.

### Positive Controls

- Object authorization uses catalog-resolved tenant/table/UID identities, not
  SQL aliases, and runs before metadata or manifest reads
  (`oracle/planner.rs:242-263`, `oracle/mod.rs:3868-3919`).
- `ORACLE_VARIANT_SQL_VERSION` is committed into both physical-plan and
  Analytical stage-body SHA-256 preimages; followers recompute the physical
  fingerprint before plan decode, and stage authority verifies the body digest
  before cache/provider/storage work (`oracle/codec.rs:25-38`,
  `oracle/peer.rs:302-323`, `oracle/follower.rs:1432-1454`,
  `oracle/analytical.rs:1605-1672`).
- Receiver-derived stage binding compares source/destination nodes and fences,
  tenant, public/private query IDs, snapshot, stage/task/attempt, reservation,
  and permission digest. Pre-binding refusals use the system-owner audit chain;
  only receiver-bound tenants select a tenant audit chain
  (`oracle/peer.rs:253-292,395-440`,
  `wyrd-server/src/oracle/peer_authority.rs:428-495`,
  `wyrd-server/src/oracle/peer_audit.rs:94-117`).
- Scribe validates the logical frame, built-in Variant contract, schema
  fingerprint, card scope, event-time bounds, and registered field IDs before
  preprocessing and shard dispatch. WAL ownership and durable ACK occur only
  after those checks (`scribe/ingress.rs:408-450,552-627`,
  `scribe/execution_lanes.rs:533-577,601-634`,
  `scribe/preprocess.rs:653-714`).
- Distributed tenant-tripwire errors retain the typed
  `QueryTenantInvariant` identity and are audited once by the leader using the
  authenticated query context. Unrecognized DataFusion/external diagnostics
  collapse to the generic public execution error rather than being returned as
  dependency prose (`oracle/mod.rs:2515-2558,4209-4307`).
- Gateway payloads are redacted and canonicalized before Variant encoding;
  absent or invalid payloads are null/refused rather than partially stored
  (`wyrd-server/src/components/gateway/capture.rs:371-397,464-511`).
- Audit publication carries the staging row's existing `entry_hash` and
  `prev_hash` unchanged while encoding only the canonical JSON `detail` as
  Variant. The hash preimage remains owned by the staging writer and includes
  that canonical detail text (`tables/audit/projection.rs:171-287`,
  `vala-sql/src/queries/audit_staging.rs:451-486`).
- Query terminal error bytes must deserialize as a typed `WyrdProblem`; clients
  reconstruct a `BifrostError` only when its serialized variant's derived code
  matches the advertised code, preventing code/detail substitution. Unknown
  errors remain `UpstreamFailure`.

## Overall verdict

**FAIL** — one reachable TASK-001 security-ordering defect remains. The
sensitive-column decision fails closed for returned data, but it is not made
before the catalog/provider/peer IO that the approved task and specification
require it to precede.
