# Deployment and Release

This document defines Wyrd's supported deployment topologies, configuration
authority, network boundaries, database lifecycle, and safe release contract.

## Supported topologies

`wyrd-server` is the only external serving surface. One logical Wyrd server may
contain multiple replicas and role-targeted pods behind one gateway.

| Topology | Tenant model | Required properties |
|---|---|---|
| Self-hosted | One or more tenants selected by the operator | One logical gateway, durable Postgres, durable object storage, persistent Scribe and Oracle audit volumes, explicit backup and recovery objectives |
| Cloud SaaS | Multi-tenant | Shared serving fleet with full tenant separation in identity, RLS, object paths, encryption, resource admission, audit, telemetry, and generated artifacts |
| Enterprise cloud | One tenant per deployment | The same server and protocol contracts, isolated storage and keys, and no in-tree commercial feature or compatibility fork |

Replicas may enable a subset of server roles through the canonical target
configuration. Targeting changes activated subsystems and resource ownership;
it does not create another public service, protocol, or durable contract.

The gateway owns public addressability, request-size and connection bounds,
rate-limit integration, and external TLS termination when TLS is not terminated
by `wyrd-server`. It does not establish tenant identity, reinterpret Wyrd
errors, retry non-idempotent writes, or buffer an unbounded query stream.

## Network and TLS boundaries

- Every external connection uses TLS. Plaintext listeners are confined to
  loopback development or a mutually authenticated, policy-enforced local
  transport boundary.
- Gateway-to-server transport is authenticated and encrypted. Forwarded client
  identity metadata is advisory; the Wyrd token remains authoritative.
- Replica-to-replica Bifrost traffic uses mutually authenticated TLS and the
  signed peer-ticket contract in `../wyrd-security-posture.md`.
- Postgres, object storage, secret providers, OIDC/JWKS endpoints, and audit
  anchors use certificate verification and explicit trust roots.
- Network policy restricts data-plane pods to required peers and dependencies.
  Source egress passes through the SSRF-screened adapter path and cannot use
  unrestricted platform-network access.
- Health, metrics, profiling, and administrative endpoints are independently
  authenticated or restricted to the operations network. They never expose
  secrets, tokens, query payloads, or tenant labels with unbounded cardinality.

## Configuration authority

Typed Rust configuration is the source of truth. Environment variables,
mounted secret files, and deployment manifests are projections into that typed
model; documentation does not define aliases that the model does not accept.

Configuration follows these rules:

1. Each setting has one canonical name, type, unit, owner, default behavior,
   validation rule, and reload classification.
2. Secrets are referenced by a secret-provider identity or mounted path and
   are redacted before logging. A secret value never appears in general
   configuration output.
3. Unknown keys, malformed values, zero or overflowed capacities, incoherent
   budgets, missing production secrets, and unsafe combinations fail startup.
4. Static values that affect storage identity, tenancy, durable formats,
   encryption, or resource geometry require a controlled restart. Dynamic
   reload is allowed only for settings whose owner defines atomic validation,
   activation, rollback, and observability.
5. Every replica emits a redacted configuration fingerprint. Replicas assigned
   the same role must agree on contract-critical values before they become
   ready.
6. Deployment tooling renders and validates configuration before rollout. It
   never silently clamps a value or falls back from a production provider to a
   local emulator.

The canonical Postgres application DSN is `WYRD_DATABASE_URL`. Role passwords
are supplied through `WYRD_DATABASE_MIGRATOR_PASSWORD` and, when operator work
is enabled, `WYRD_DATABASE_PLATFORM_ADMIN_PASSWORD`. Production never boots an
embedded database because a DSN is absent.

Unsuffixed `WYRD_DB_*` variables tune the `wyrd_app` pool. `_MIGRATOR` and
`_PLATFORM_ADMIN` suffixes tune the boot-only migrator and privileged operator
pools. Missing suffixed values use that role's typed defaults; they do not
inherit the application value.

## Database roles and connection budget

| Role | Lifetime | Purpose |
|---|---|---|
| `wyrd_app` | Runtime | Tenant-scoped RLS traffic through `TenantConn` |
| `wyrd_migrator` | Migration only | DDL for Wyrd and Vala schemas; closed before normal serving |
| `wyrd_platform_admin` | Runtime only where required | Named, audited, cross-tenant operator capabilities through `OperatorPool` |

The deployment connection budget is:

```text
replicas * (app_max + platform_admin_max)
  + concurrent_migrator_max
  + database_reserved
  <= postgres_max_connections
```

`database_reserved` is an explicit operator decision that covers administration,
replication, monitoring, failover, and extension roles. Capacity validation
uses the maximum replicas permitted by autoscaling or rollout surge, not the
steady replica count.

Transaction-mode PgBouncer requires statement-cache capacity `0` on every pool
routed through it. Tenant binding remains transaction-local. Session-scoped
state, session advisory locks, and `LISTEN`/`NOTIFY` are not used on a
transaction-pooled path.

## Persistent storage placement

- Postgres and object storage are authoritative durable systems and are not
  placed on an ephemeral pod filesystem.
- Every Scribe replica has stable node identity and a persistent local volume
  for WAL and durable staged runs. Rescheduling without that volume is a node
  loss and invokes recovery; it is not a clean restart.
- Oracle has a persistent local volume for its read-audit acceptance WAL and a
  bounded, encrypted scratch volume for spill. Spill is disposable after query
  termination; the audit WAL is not.
- Audit projection uses the current Scribe and Forge publication path to write
  retained `vala.system.audit_log` history. Deployment readiness includes
  durable idempotent publication and bounded outbox retirement;
  `vala.audit_staging` is not retained history.
- Forge requires no local scratch volume: managed compaction does not enable
  DataFusion disk spilling. Its pod memory limit remains the final physical
  boundary behind estimated-memory admission.
- Volume purpose, tenant/table path grammar, encryption, capacity, inode
  budget, cleanup owner, and alert thresholds are explicit. A volume cannot be
  shared across incompatible purposes merely to increase apparent free space.

## Migration contract

Migrations are immutable, ordered, idempotent where re-entry is required, and
executed with the dedicated migrator role. Runtime handlers never receive the
migrator pool.

The deployment migration lease is one Postgres advisory lock for the physical
migration domain: the connected Postgres cluster/database plus the fixed
`platform`, `wyrd`, and `vala` schema set. Every release, blue/green slot, and
replica targeting that database derives the same lock key; a deployment name
or caller-controlled identifier never changes it. One dedicated direct
migrator session holds the lock for checksum verification, migration, and
post-migration validation. The migrator path bypasses transaction-mode
PgBouncer; losing the session loses the lock and fails the gate. A contender
does not wait unboundedly or serve against an intermediate schema.

The release pipeline must:

1. Back up and verify the recovery point required by the deployment's recovery
   contract.
2. Acquire one deployment-scoped migration lease.
3. Verify the source schema and application compatibility range.
4. Apply Wyrd and Vala migrations in their declared order.
5. Verify schema invariants, RLS policies, role grants, required sentinels, and
   migration checksums.
6. Close all migrator connections before application replicas become ready.

Schema changes use expand-and-contract when old and new replicas overlap.
During the overlap window, writers emit a representation readable by both
versions, readers tolerate the declared old representation, and no destructive
contract step occurs. Contracting a column, index, table, object format, or
wire field is a separate release after every supported old replica and worker
has drained.

A migration rollback is permitted only when a tested down migration preserves
all accepted data and audit evidence. Otherwise rollback means restoring the
verified backup and reconciling object storage under the recovery procedure.
Destructive best-effort SQL is not rollback.

## Rollout and version skew

Every build publishes an immutable version and contract fingerprint covering
wire protocols, durable schemas, Bifrost internal RPCs, peer-ticket claims,
storage formats, and configuration schema.

The signed release manifest uses this versioned envelope:

```yaml
apiVersion: wyrd.release/v1
kind: ReleaseManifest
spec: { ... }
```

An unknown `apiVersion`, kind, field, or required-contract variant fails
closed. The manifest is stored beside the immutable artifact and contains:

- artifact digest and build identity;
- public OpenAPI/schema/stub digest;
- ordered Wyrd and Vala migration-registry digests;
- configuration-schema digest;
- Bifrost RPC, peer-ticket, WAL, staged-manifest, Parquet-layout, and Iceberg
  operation versions;
- minimum and maximum compatible peer/application versions for each contract;
- irreversible-format boundary; and
- provenance signature and verification identity.

Release tooling derives those values from owning sources and rejects a manual
override. The deployment gate verifies the signature, artifact digest,
database migration digests, live peer versions, configured role set, and every
declared compatibility interval before surge starts. Overlap is permitted only
when both manifests mutually accept the other version for every shared
contract. Missing or one-sided compatibility fails closed.

- The gateway and peers reject protocol versions outside their declared
  compatibility set before payload decoding.
- Adjacent application versions may overlap only when the release manifest
  proves compatible database, object, wire, and peer contracts.
- Background workers, leases, tasks, staged runs, and query fragments retain
  explicit format or protocol versions. A new worker cannot claim work it
  cannot decode; an old worker cannot claim work whose writer requires newer
  semantics.
- Rollout proceeds by role and limits unavailable capacity so Scribe admission,
  Oracle interactive capacity, Forge lease progress, and audit relay remain
  within their service objectives.
- Readiness is removed before connection drain. Shutdown stops new admission,
  cancels or hands off bounded work according to its owner, waits for durable
  settlement, and then releases runtime and volumes.
- Automatic rollback stops when the new release has emitted an irreversible
  durable format or contract. The incident commander then chooses roll-forward
  remediation or verified restore; deployment automation does not guess.

## Release evidence

A production release records:

- artifact identity and software bill of materials;
- signed provenance and vulnerability-policy result;
- configuration and contract fingerprints;
- migration rehearsal and checksum results;
- backup/restore qualification against the release;
- security-key and peer compatibility checks;
- targeted user journeys for enabled surfaces;
- Scribe recovery, Oracle cancellation/peer-loss, Forge reconciliation, and
  audit publication evidence; and
- rollout owner, rollback boundary, and incident contacts.

Passing unit tests alone does not establish deployment readiness.
