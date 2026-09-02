# Production Runbooks

These provider-neutral procedures define the required evidence, authorized
mutations, and stop/go criteria for Wyrd production recovery. A deployment's
infrastructure automation maps each named capability to concrete commands and
records their output in the incident evidence set. Operators never mutate
Postgres, object storage, WAL, staged runs, Iceberg metadata, leases, or audit
state outside the owning capability.

Every procedure starts by recording the incident ID, artifact and release
manifest, redacted configuration fingerprint, affected roles and tenants,
current readiness matrix, database recovery point, object-store version cut,
audit-chain head and anchor, and assigned incident/evidence owners.

## Credential or signing-key compromise

### Contain

1. Remove token issuance and every role that requires the affected key from
   readiness. Preserve verification for unaffected keys.
2. Disable the compromised signing key or credential at its authority and
   advance affected principal, credential, or tenant authorization epochs in
   one audited operation.
3. Revoke refresh-token families and API keys whose confidentiality cannot be
   established. Fence affected peer-ticket issuers independently from user JWT
   issuers.
4. Preserve JWKS versions, KMS access logs, token/credential audit events,
   policy decisions, gateway logs, and external audit anchors.

### Recover

1. Create replacement material under a new identity and access policy.
2. Publish replacement public verification state before enabling issuance.
3. Verify issuer, audience, algorithm, `kid`, epoch, delegation, expiry, and
   replay behavior with positive and negative journeys.
4. Rotate workload secrets, Source credentials, peer trust, or anchor keys in
   the dependency order dictated by the compromised class.
5. Prove that retired material is rejected at gateway, server, peer, and
   background-worker boundaries.

### Go/no-go

Restore readiness only when the new key path is healthy, old material is
rejected, authorization epochs have propagated, refresh replay is contained,
the audit chain and external anchor remain verifiable, and the incident owner
accepts the identified exposure window. Otherwise remain fenced.

## Full database and object-store restore

### Contain and select a cut

1. Fence all writes and background publication. Preserve database WAL, object
   versions, Scribe volumes, audit WALs, and external anchors.
2. Select one database recovery point and object-store version cut that can be
   reconciled without inventing catalog state. Record the intended RPO/RTO
   evaluation before mutation.
3. Restore into an isolated deployment with external serving disabled.

### Verify and reconcile

1. Verify release-manifest and migration-registry digests, schema owners,
   grants, RLS policies, system tenant, role separation, and audit hash chains.
2. Walk every retained Iceberg ref and prove that referenced metadata,
   manifests, data files, and delete files exist with the expected identity.
3. Reconcile the external audit checkpoint and name the exact unanchored
   suffix.
4. Attach Scribe volumes to their recorded node identities; replay WAL and
   staged manifests and reconcile publication operations without admission.
5. Reconcile Forge tasks, attempts, operation IDs, uncertain commits,
   retention watermarks, and cleanup cursors from catalog evidence.
6. Rebuild only disposable indexes and caches.

### Go/no-go

Run tenant-isolation, credential, append/replay, live-tail, pinned-query,
Forge-reconciliation, audit, and Rust/Python/TypeScript/MCP journeys. Admit one
role at a time only when its readiness dependencies pass and the recovered cut
meets the declared RPO/RTO. Any unexplained catalog/object mismatch, broken
audit chain, cross-tenant evidence, or ambiguous publication is a no-go.

## Scribe node or volume loss

### Classify

Identify the stable node ID, all sixteen shard generations, last valid WAL
frame and batch fence, immutable cohorts, staged manifests, live-tail leases,
publication claims, `file_list` rows, and object operations. Classify each
claim as definitely unpublished, committed, or uncertain. A missing volume is
node loss, not an empty restart.

### Recover

1. Remove the node's Scribe role from routing and stop new admission.
2. If the volume is available, attach it only to the recorded node identity,
   verify encryption and filesystem integrity, truncate only a WAL tail class
   explicitly permitted by the WAL format, and replay by recorded shard.
3. Validate every staged run's checksum, schema, layout, footer, and cohort
   coverage before using it or retiring WAL.
4. Reconcile deterministic publication operations against exact object and
   `file_list` evidence. Never create a new identity for an uncertain PUT.
5. Rebuild the live-tail registry and confirm that every acknowledged row has
   exactly one highest authority.

### Go/no-go

Restore Scribe readiness only when every acknowledged batch is replayed or
represented by validated later authority, all uncertainty is settled, durable
capacity is healthy, and append/replay/live-tail journeys pass. Missing
acknowledged authority, contradictory lineage, tenant mismatch, or corrupt
non-tail WAL is a no-go and invokes full restore or incident escalation.

## Oracle audit-WAL or peer failure

### Audit-WAL path

1. Remove Oracle readiness before the acceptance WAL reaches its configured
   backlog or durability limit; do not return rows without a successful fsync.
2. Verify CRC frames, sequence/relay identity, tenant binding, and the last
   canonical audit-outbox acknowledgement.
3. Relay valid frames at least once and prove duplicates converge under the
   canonical audit writer. Preserve a corrupt frame and its surrounding bytes
   as evidence; never skip it to regain readiness.

### Peer path

1. Cancel and join the affected stage tree and release query-owned memory,
   exchange, scratch, peer, and slot resources.
2. Treat every failure after Analytical selection as terminal. Do not construct
   a successor attempt; the caller may submit a new logical query after receiving
   the terminal failure.
3. Fence a peer that presents invalid tickets, tenant/digest mismatch, stale
   epoch, corrupt frames, or repeated availability loss. Preserve ticket and
   transport evidence without recording sensitive payloads.

### Go/no-go

Oracle is ready only when the acceptance WAL is writable and recoverable, relay
lag is within its bound, canonical audit append is healthy, query resources
release exactly once, peer trust is current, and cancellation/terminal
stream journeys pass. A skipped audit frame or stream interpreted as success
after terminal failure is a no-go.

## Forge ambiguous commit or lease loss

### Classify and fence

1. Stop the affected table's scheduler lane and capture tenant, table, task,
   plan hash, base snapshot, attempt, operation ID, output generation, lease,
   fence, handoff, possible output paths, and last catalog response.
2. A lost lease cancels and drains physical work. The stale owner cannot
   publish, settle, audit, or delete.
3. Classify the catalog result only as committed, definitely uncommitted, or
   uncertain. Object existence alone never means committed.

### Reconcile

1. Refresh authoritative branch metadata and locate exact operation and lineage
   properties plus manifest entries for every output.
2. If exact committed evidence exists, perform the one fenced SQL/audit
   settlement and retain/delete source authority according to the committed
   snapshot.
3. If exact non-commit evidence exists, settle the attempt as uncommitted,
   preserve outputs through the orphan safety window, and return durable demand
   for a new plan and attempt.
4. If evidence remains ambiguous, keep the attempt and outputs protected and
   keep the table lane fenced. Do not retry commit, create a new attempt, or run
   cleanup against those objects.

### Go/no-go

Resume the table lane only after exact catalog evidence and SQL/audit state
agree, no stale fence can complete, all possible outputs are protected or
safely classified, and lease-loss/ambiguous-commit/reconciliation journeys
pass. Contradictory snapshot properties, missing manifests, or unbounded
uncertainty is a no-go requiring incident escalation.
