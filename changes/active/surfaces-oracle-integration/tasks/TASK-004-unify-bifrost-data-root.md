---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-surfaces-oracle-integration
spec_revision: 4
requirements: [REQ-055, REQ-055A, INV-022, AC-018]
depends_on: [TASK-001]
parent_task:
remediates: []
---

## Objective

After the core Redux integration, make `WYRD_BIFROST_DATA_DIR` the one server
configuration root for every Bifrost-managed local path and prove boot,
readiness, restart, and replica ownership behavior. This outcome blocks final
integration closeout.

## Constraints

- Derive all managed Scribe and Oracle local paths from one root, defaulting
  locally to `.wyrd/bifrost`.
- Remove `WYRD_SCRIBE_WAL_DIR` and the independent Oracle audit-WAL-root
  setting without aliases or compatibility fallback.
- Create required managed paths before role activation and fail before
  readiness when the resolved root is unusable.
- Durable replicas must not share one writable WAL identity. Tenant, node,
  writer epoch, ownership, replay, and fencing semantics cannot weaken.
- Forge has no spill or scratch child path; do not invent one to make the root
  appear exhaustive.
- This task depends on the integrated server, Scribe, Oracle, and deployment
  owners from TASK-001 and completes before TASK-003 closeout.

## Relevant Surface

- Bifrost server configuration and boot/readiness owners
- Redux Scribe WAL and Oracle read-audit WAL path ownership
- Deployment configuration, examples, and operations documentation
- Bifrost server, Scribe, Oracle, restart, and multi-replica journeys

Paths are ownership guidance, not a private implementation allowlist.

## Approach

1. Resolve the single root once at the server boundary and pass derived owned
   paths into the integrated Scribe and Oracle owners.
2. Remove legacy independent settings and update deployment configuration and
   documentation to expose only `WYRD_BIFROST_DATA_DIR`.
3. Create and validate managed directories before activating any selected
   Bifrost role; connect readiness to usable and healthy local ownership.
4. Preserve per-replica WAL identity, restart recovery, fencing, and shutdown
   behavior for default and overridden roots.
5. Add the smallest production-shaped configuration, boot, restart, and
   multi-replica evidence needed to prove the outcome.

## Acceptance Criteria

- With no override, every Bifrost-managed local path is created below
  `.wyrd/bifrost`; with an override, every path is created below the resolved
  `WYRD_BIFROST_DATA_DIR`.
- Scribe and Oracle activate only after their derived paths are usable. An
  invalid, unwritable, contradictory, or unavailable root prevents readiness
  with a structured failure and no partial role activation.
- The removed Scribe and Oracle settings are rejected or ignored as absent
  legacy configuration according to the current configuration parser; no
  alias, fallback, or second root survives.
- Restart from the same root recovers acknowledged Scribe and accepted Oracle
  audit-WAL state without loss, duplication beyond approved replay semantics,
  or authority widening.
- Multiple durable replicas use distinct writable WAL identities even when
  configured beneath the same deployment-level storage location; no process
  opens another node's local WAL path.
- Forge remains free of a spill/scratch child path and all deployment targets
  preserve their approved memory and readiness behavior.

## Verification

- `mise run fmt`
- `mise run lints`
- `mise run check:bifrost-oracle-deploy`
- `mise run check:bifrost-resource-governance`
- `mise run check:unwrap-audit`
- `git diff --check`

Run and record exact focused `mise exec -- cargo nextest run` commands for the
configuration, boot, readiness, restart, and multi-replica scenarios added or
changed here. Do not run a Bifrost aggregate in this task.
