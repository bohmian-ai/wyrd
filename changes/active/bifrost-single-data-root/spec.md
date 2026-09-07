---
id: SPEC-bifrost-single-data-root
revision: 2
status: approved
---

# Automatic Bifrost local storage from one filesystem root

## Human intent and user value

An operator shall provide one durable Bifrost filesystem root and let the
server create and use every local path it owns. Deploying Scribe, Oracle, or a
mixed Bifrost target must not require subsystem-specific WAL or scratch paths
or a Kubernetes-specific storage concept. The same behavior shall work locally
without configuration.

## Scope

- Establish one durable Bifrost filesystem root for every server target.
- Derive and create Scribe WAL, stable node identity, staged runs, Scribe
  output scratch, Oracle audit WAL, Oracle spill, and Forge spill paths beneath
  that root.
- Make `WYRD_BIFROST_DATA_DIR` the sole environment override for that root.
- Default the root to `.wyrd/bifrost` when the override is absent.
- Remove the independent `WYRD_SCRIBE_WAL_DIR` and
  `bifrost.oracle.audit_wal_root` configuration surfaces.
- Align local/test startup, checked-in Kubernetes manifests, deployment
  contract checks, and architecture/operations documentation.

## Non-goals

- Changing Scribe acknowledgement, WAL, staging, publication, replay, or
  retirement semantics.
- Changing Oracle read-audit acceptance, relay, checkpoint, or fail-closed
  semantics.
- Provisioning storage, selecting a deployment platform's volume product, or
  detecting whether an operator-provided filesystem survives process or host
  replacement.
- Combining Bifrost resource accounting or disk limits merely because paths
  share one filesystem root.
- Adding a compatibility alias, migration layer, alternate root, or per-role
  storage override.
- Changing Postgres, object-storage, Iceberg, query, ingest, or public SDK wire
  contracts.

## Definitions

- **Durable Bifrost filesystem root:** the single operator-selected filesystem
  directory from which Bifrost derives every managed local path. This is the
  deployment contract; an attached disk, network filesystem, container volume,
  or Kubernetes volume claim is only a way to provide it.
- **Managed path:** a role-owned child path for durable WAL/state or disposable
  scratch whose lifecycle and resource accounting remain with its current
  owner.
- **Local default:** `.wyrd/bifrost`, used when
  `WYRD_BIFROST_DATA_DIR` is absent.

## Required behavior

### REQ-001 — One root controls Bifrost local storage

The server shall resolve exactly one durable Bifrost filesystem root from
`WYRD_BIFROST_DATA_DIR`, falling back to `.wyrd/bifrost` when it is absent.
Every Bifrost target shall use that resolved root consistently.

### REQ-002 — Managed paths are automatic

Before activating a selected role, the server shall derive that role's WAL,
identity, staging, audit, and scratch paths beneath the resolved root and create
the required directories. Operators shall not configure or create individual
managed paths.

### REQ-003 — Role durability remains unchanged

Scribe WAL, Scribe stable node identity, staged runs, and Oracle audit WAL shall
remain locally durable and recoverable under their existing protocols. Oracle
and Forge scratch shall remain disposable under their existing lifecycle
owners. Co-location beneath one root shall not merge their admission,
accounting, cleanup, or fail-closed boundaries.

### REQ-004 — Independent root settings are removed

`WYRD_SCRIBE_WAL_DIR` and `bifrost.oracle.audit_wal_root` shall no longer be
accepted configuration. No replacement per-subsystem path or compatibility
alias shall be introduced.

### REQ-005 — Local startup requires no path configuration

A local development server shall create and use `.wyrd/bifrost` and its
managed paths without a server TOML file or storage-root environment variable.
An explicit `WYRD_BIFROST_DATA_DIR` shall provide the same behavior at the
selected path.

### REQ-006 — Deployments provide one filesystem-root contract

Every deployment shall expose one writable filesystem root at the value of
`WYRD_BIFROST_DATA_DIR` for each process that owns durable Bifrost state. The
server shall require no platform-specific volume type, subdirectory mounts, or
Oracle-specific configuration. A deployment that requires restart recovery
shall back the root with storage that survives process replacement. Replicated
durable targets shall not share one writable WAL identity.

### REQ-007 — Root preparation fails before activation

If the resolved root or a required managed path cannot be created or used, boot
shall fail with the owning typed boot error before the affected role becomes
ready. The server shall not fall back to a temporary directory or another
filesystem.

## Invariants and prohibited outcomes

- One process never resolves different roots for different Bifrost roles.
- Oracle audit acceptance never silently falls back to the operating-system
  temporary directory.
- An operator is never required to name or pre-create a Bifrost child path.
- A root failure never produces a partially ready Scribe, Oracle, or Forge
  owner.
- Stable Scribe node identity remains colocated with the durable state it
  identifies.
- Existing per-purpose resource ceilings and cleanup ownership remain
  independent.
- No legacy environment variable, TOML field, compatibility route, or alias is
  retained.

## Externally observable behavior and failure modes

- With no root setting, local startup creates and uses `.wyrd/bifrost`.
- With `WYRD_BIFROST_DATA_DIR=/var/lib/wyrd/bifrost`, all selected Bifrost
  roles create and use managed paths only below `/var/lib/wyrd/bifrost`.
- Any deployment platform needs one root variable and one matching writable
  filesystem per durable Bifrost process; it needs no platform-specific Bifrost
  configuration or Oracle audit-WAL TOML entry.
- A deployment may use ephemeral storage at that same root only when its
  operator accepts losing unrelayed or unpublished local state after process
  replacement.
- An empty root value or an uncreatable/unusable root fails startup without
  activating the affected role.
- Removed subsystem-specific settings are rejected by typed TOML parsing where
  they are part of the schema and otherwise have no effect on root resolution.

## Material constraints

- Preserve the current local-fsync hot paths; Postgres and object storage shall
  not replace local WAL acceptance.
- Preserve current durable formats, identities, replay behavior, audit
  guarantees, resource ceilings, tenant boundaries, and shutdown ordering.
- Add no dependency, Cargo feature, public wire type, database migration, or
  generated SDK contract.
- The server owns directory derivation and creation. The deployment platform
  owns provisioning and mounting the filesystem at the selected root.
- Use the current Bifrost server/configuration owners and repository-managed
  test harnesses.

## Required system boundaries and cross-boundary flow

1. Server configuration resolves one root from the environment or local
   default.
2. Bifrost boot derives the complete set of role-owned managed paths from that
   root and creates them before resource detection and role activation.
3. Existing Scribe, Oracle, and Forge owners receive only their derived paths
   and keep their current durability, accounting, and lifecycle behavior.
4. Local/test builders inject one root rather than separately modeling WAL,
   spill, and audit roots.
5. The deployment platform provides one writable filesystem per durable
   Bifrost process at the configured root; Bifrost manages everything below
   that root. Kubernetes volume claims are one implementation of this boundary,
   not the boundary itself.

This is server deployment configuration, not a public client or Card contract.

## Acceptance obligations and evidence classes

### AC-001 — Root resolution

Focused configuration tests prove the local default, the
`WYRD_BIFROST_DATA_DIR` override, empty-value rejection, removal of the Oracle
TOML field, and absence of the Scribe-specific environment path. Covers
REQ-001, REQ-004, and REQ-005.

### AC-002 — Automatic managed paths

Focused boot tests prove all managed paths are derived below one root, created
without operator intervention, and never replaced by a temporary fallback.
Uncreatable-root evidence proves failure precedes activation. Covers REQ-002,
REQ-003, REQ-005, and REQ-007.

### AC-003 — Local restart durability

A repository-managed Bifrost server test starts from one explicit temporary
root, writes through the real Scribe path, stops, restarts from the same root,
and proves the existing replay/read result without any separate audit, WAL, or
spill path. Covers REQ-003 and REQ-005.

### AC-004 — One-mount deployment contract

Semantic deployment tests prove checked-in mixed and role-separated Kubernetes
examples implement the platform-neutral contract with the current
`WYRD_TARGET` vocabulary, one `WYRD_BIFROST_DATA_DIR`/mount pairing, persistent
per-pod filesystems for recovery-capable targets, and no subsystem-specific
path. Negative fixtures reject an ephemeral root presented as recovery-capable,
a mismatched mount, and a shared writable filesystem across replicas. Covers
REQ-004 and REQ-006.

### AC-005 — Focused Bifrost verification

The complete Bifrost verification lane, formatting, linting, deployment checks,
and clean diff checks pass without weakened durability, audit, or recovery
evidence. Covers all requirements and invariants.

## Open material decisions

None.

## Planning-decision inventory

`$wyrd-plan` must fix the concrete configuration owner, root-derivation owner,
child-path layout, removal and consumer closure for old settings, test-builder
shape, platform-neutral deployment documentation, Kubernetes workload/storage
topology, exact RED tests, and focused verification commands while preserving
existing durable formats and lifecycle owners.

## Revision history

- 2026-09-07 — Revision 2 approved explicitly by the user: the deployment
  contract is one durable Bifrost filesystem root, not a PVC or any other
  platform-specific volume product. Ephemeral backing remains valid only when
  the operator accepts loss of local recovery state.
- 2026-09-06 — Revision 1 approved explicitly by the user with the direction:
  one mounted root, automatic Bifrost path management, and identical local
  behavior.

## Authority links

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/wyrd-security-posture.md`
- `architecture/operations/README.md`
- `architecture/operations/deployment-and-release.md`
- `architecture/operations/reliability-and-recovery.md`
- `architecture/references/doctrine/architecture-constraints.md`
- `architecture/references/domain/olap-serving.md`
- `architecture/references/domain/analytical-operations-reliability.md`
- `architecture/references/languages/spec-driven-development.md`
