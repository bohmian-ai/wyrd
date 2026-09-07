---
id: BIFROST-ROOT-T01
title: Resolve and prepare every Bifrost local path from one data root
kind: implementation
mode: DECOMPOSE
status: proposed
spec: SPEC-bifrost-single-data-root
spec_revision: 1
depends_on: []
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005]
---

# Automatic Bifrost local storage from one root

## Outcome and value

An operator supplies at most one `WYRD_BIFROST_DATA_DIR`, mounts one persistent
filesystem there when durability is required, and Bifrost creates and uses all
managed local paths beneath it. Local development works from `.wyrd/bifrost`
without configuration.

Required execution skill: `$wyrd-implement`.

## Current repository facts

- `WyrdServerConfig::load` owns TOML/default/environment precedence, but
  `WYRD_SCRIBE_WAL_DIR` is read directly in `boot/mod.rs` and bypasses that
  owner.
- `build_bifrost_external_dependencies` already treats the Scribe WAL path as
  a common base and derives Scribe stage/output scratch, Oracle spill, and
  Forge spill beneath it.
- `OracleRuntimeConfig::audit_wal_root` independently crosses into
  `OracleAuditWalConfig`; production validation requires it, while development
  silently falls back to a process-specific system temporary directory.
- Scribe stable identity is stored beside the current WAL root and must remain
  attached to the same durable volume.
- `WyrdTestServerBuilder::with_bifrost_roots` and process-cluster fixtures
  currently model WAL, spill, and Oracle audit roots independently.
- Checked-in Kubernetes manifests and their semantic checker use obsolete
  `WYRD_ROLES`; the role-separated Oracle example mounts `emptyDir` at
  `WYRD_SCRIBE_WAL_DIR`, and the mixed example provides no durable mount.

## Owners, scope, consumers, and prohibited changes

- `BifrostRuntimeConfig` owns the configured root and environment/default
  precedence.
- A new private, state-bearing `BifrostLocalRoots` in `wyrd-server` boot owns
  child derivation, directory creation, and the invariant that every path is
  beneath one root. It is not a generic filesystem abstraction.
- Existing Scribe, Oracle audit, Oracle spill, and Forge owners keep their
  formats, accounting, cleanup, and recovery behavior; they consume derived
  paths only.
- `wyrd-testing` owns local/process-cluster root injection and the semantic
  Kubernetes contract.
- Architecture and operations documents own the durable-volume deployment and
  recovery descriptions.

Do not alter append or query hot paths, fsync ordering, WAL/staged formats,
Postgres/object-store boundaries, resource limits, public wire contracts,
database schemas, dependencies, or Cargo features. Do not retain the old
settings as aliases or add additional root knobs.

## Selected implementation architecture

Add `data_dir: PathBuf` to `BifrostRuntimeConfig`, with a serde default of
`.wyrd/bifrost`. `WyrdServerConfig::apply_env_overrides` applies the sole
`WYRD_BIFROST_DATA_DIR` override through the existing non-empty environment
validation. Remove `OracleRuntimeConfig::audit_wal_root`, its default, its
production-presence validation, and its projection into the Oracle WAL
configuration. Remove every direct read of `WYRD_SCRIBE_WAL_DIR`.

Add one private `BifrostLocalRoots` owner constructed from
`config.bifrost.data_dir`. Preserve the current Scribe durable on-disk layout by
using the data root itself as the WAL/node-identity base; derive the existing
`scribe-stage`, `scribe-output-scratch`, `oracle-spill`, and `forge-spill`
children unchanged, and add `oracle-audit` as the Oracle audit WAL child. Its
single preparation operation creates the root and children required by the
selected target before resource detection/role activation and returns the
existing role-appropriate typed boot error on failure. No consumer may derive
or replace these paths independently.

Change `OracleAuditWalConfig::audit_wal_root` from optional to required
`PathBuf`; `OracleAuditPublisher::new` always opens that path and deletes the
system-temporary fallback and sequence counter. Construct its config at boot
from the derived `oracle-audit` path plus the existing Oracle bounds. Test-only
builders may inject one root, but not distinct production-shaped roots.

Replace `WyrdTestServerBuilder::with_bifrost_roots` with one internal
`with_bifrost_root` seam and update `ClusterStorageRoot`, process-cluster node
resources, and all callers so restart retains the exact same root. Keep any
test-only temporary-directory ownership alive for the server lifetime through
that single root handle.

Update the mixed, role-separated, and rollback Kubernetes examples to the
current `WYRD_TARGET` values. Durable mixed, Scribe, and Oracle workloads use a
`StatefulSet` with `volumeClaimTemplates` so every replica receives one
`ReadWriteOnce` claim mounted at `/var/lib/wyrd/bifrost`; set
`WYRD_BIFROST_DATA_DIR` to that exact mount. Forge-worker-only scratch does not
create a durability claim. Extend the existing semantic deployment checker to
understand the selected workload kind and claim templates, require one
root/mount pairing and per-pod claim for durable targets, and reject
`emptyDir`, mismatched paths, shared claims, old root settings, and obsolete
role vocabulary.

Update Bifrost, security, deployment, and recovery authority only where they
currently describe separately configured or mounted local roots. State that
one mounted Bifrost root contains purpose-separated, independently governed
paths; Kubernetes persistence and recovery requirements remain unchanged.

## Ordered implementation scenarios

### Scenario 1 — One configuration root

**Behavior.** The server resolves `.wyrd/bifrost` by default or one non-empty
`WYRD_BIFROST_DATA_DIR`, while removed subsystem-specific configuration cannot
select another path. Covers REQ-001, REQ-004, REQ-005, AC-001.

**RED.** Add
`config::tests::bifrost_data_dir_has_one_default_and_environment_override`.
Assert the default, the environment override, empty override rejection, TOML
rejection of `bifrost.oracle.audit_wal_root`, and that no Oracle field remains
to diverge. It initially fails because no common field/env exists and the old
field still parses.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(=config::tests::bifrost_data_dir_has_one_default_and_environment_override)'
```

**GREEN.** Implement the selected `BifrostRuntimeConfig::data_dir` default/env
precedence; remove the Oracle root field/validation and every direct old-env
read. Update existing production configuration tests to prove production no
longer needs a separate Oracle path while all calibration, TLS, and audit
bounds still fail closed.

**REFACTOR.** Root precedence remains solely in `WyrdServerConfig`; boot reads
only validated configuration and no compatibility alias survives.

### Scenario 2 — Automatic path preparation and no temporary fallback

**Behavior.** One prepared root supplies every managed path before activation,
and Oracle audit can never fall back elsewhere. Covers REQ-002, REQ-003,
REQ-007, AC-002.

**RED.** Replace the narrow spill-only boot test with
`boot::tests::bifrost_local_roots_prepare_every_managed_path`. From one temp
root, assert exact derived paths, directory creation, common ancestry, and the
root-as-Scribe-WAL layout. Extend the existing uncreatable-root test to assert
that preparation fails and creates no sibling fallback. It initially fails
because boot has separate derivation and Oracle still owns a temporary
fallback.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(=boot::tests::bifrost_local_roots_prepare_every_managed_path)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(=boot::tests::bifrost_local_roots_reject_uncreatable_root)'
```

**GREEN.** Add and wire `BifrostLocalRoots`, prepare paths once in
`build_bifrost_external_dependencies`, carry the resolved paths through
`BifrostExternalDependencies`, and pass the required audit path into
`OracleAuditPublisher`. Remove `prepare_oracle_spill_root` and the Oracle
temporary-root branch after all consumers use the owner.

**REFACTOR.** No role recomputes paths; the owning struct contains meaningful
path state and preparation invariants without becoming a general utility.

### Scenario 3 — One-root restart journey

**Behavior.** A real server started and restarted with one retained root
recovers accepted Scribe state and supplies Oracle audit storage without
separate test roots. Covers REQ-003, REQ-005, AC-003.

**RED.** Add
`bifrost::scribe::source_boundary_recovery::single_bifrost_root_survives_server_restart`
to the existing Scribe journey binary. Start the production harness with one
dedicated root, append and durably acknowledge a uniquely identified batch,
stop, restart the same node/root, and assert exact query visibility and a
configured Oracle audit path below the same root. It initially fails because
the builder and cluster require independent roots.

```bash
mise run test:bifrost:journey:scribe
```

**GREEN.** Collapse `WyrdTestServerBuilder`, cluster/process node resources,
restart wiring, and retained temporary-directory ownership to the single-root
seam. Production and test composition must use the same derivation owner.

**REFACTOR.** The journey may control only one root; it cannot inject a hidden
Oracle or spill path that production operators cannot configure.

### Scenario 4 — One persistent mount per durable Kubernetes pod

**Behavior.** Checked-in manifests express one automatic Bifrost root and one
stable claim per durable replica. Covers REQ-004, REQ-006, AC-004.

**RED.** Update both existing deployment-contract tests. The acceptance test
requires current targets, `WYRD_BIFROST_DATA_DIR`, a matching mount, and
per-pod persistent claims. The mutation test proves rejection of `emptyDir`, a
mismatched root/mount, a shared claim, `WYRD_SCRIBE_WAL_DIR`, and
`bifrost.oracle.audit_wal_root`. They initially fail against the checked-in
manifests and current parser.

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --lib \
  -E 'test(=bifrost::deployment_contract::bifrost_deployment_contract_accepts_checked_in_manifests)'
mise exec -- cargo nextest run --locked -p wyrd-testing --lib \
  -E 'test(=bifrost::deployment_contract::bifrost_deployment_contract_rejects_broken_fixtures)'
```

**GREEN.** Update the semantic manifest projection/validation and all three
checked-in manifests to the selected StatefulSet/root/claim design. Update
authority documents in the same scenario so deployment and recovery prose
describe the one-root contract without weakening purpose-specific ownership.

**REFACTOR.** Manifest validation remains semantic rather than formatting
sensitive and does not grow into a Kubernetes framework.

## Cross-scenario decisions and invariants

- Root configuration is a server deployment contract; it does not enter
  `wyrd-spec`, OpenAPI, Card schemas, or SDKs.
- The data root itself remains the current Scribe WAL/node-identity base to
  avoid a durable-layout migration. Every other managed path is a child.
- Oracle audit WAL receives a required derived path. Development and tests do
  not use `std::env::temp_dir()` as an implicit second root.
- Sharing a filesystem root never shares WAL identity, resource accounting,
  admission, cleanup, or recovery state between owners or pods.
- Each replicated durable Kubernetes target owns its own claim; no two live
  writers use one WAL/node-identity directory.

## Expected write set and consumer closure

- `crates/wyrd/wyrd-server/src/config.rs` — single root schema, default/env,
  removed Oracle root, validation tests.
- `crates/wyrd/wyrd-server/src/boot/mod.rs` — root owner, one preparation path,
  and role wiring.
- `crates/wyrd/wyrd-server/src/oracle/audit_wal.rs` and
  `crates/wyrd/wyrd-server/src/oracle/query_audit.rs` — required derived audit
  root and no temporary fallback.
- `crates/wyrd/wyrd-testing/src/server.rs`,
  `crates/wyrd/wyrd-testing/src/bifrost/cluster.rs`, and process-cluster child
  wiring — one injected/retained root across restart.
- `crates/wyrd/wyrd-testing/src/bifrost/deployment_contract.rs` — one-root PVC
  semantic validation.
- `crates/wyrd/wyrd-testing/tests/bifrost/scribe/source_boundary_recovery.rs`
  — restart journey.
- `deploy/kubernetes/bifrost/deployment-mixed.yaml`,
  `deploy/kubernetes/bifrost/deployment-role-separated.yaml`, and
  `deploy/kubernetes/bifrost/rollback-mixed.yaml` — current targets and one
  persistent root per durable pod.
- `architecture/bifrost-design.md`, `architecture/wyrd-security-posture.md`,
  and relevant `architecture/operations/` authorities — aligned deployment and
  recovery contract.

No dependency manifest, lockfile, database migration, client SDK, public wire
schema, generated artifact, or UI file belongs in the implementation diff.

## Verification

Run the named scenario commands sequentially, then:

```bash
mise run check:bifrost-oracle-deploy
mise run test:bifrost:integration:server
mise run test:bifrost:journey:scribe
mise run verify:bifrost
mise run fmt
mise run lints
git diff --check
```

`mise run gate` is not required for this Bifrost-scoped change. No codegen lane
is required unless an unapproved public/generated contract enters the diff, in
which case stop for a specification revision instead of expanding scope.

## Completion evidence

- Record RED and GREEN results for every named test command.
- Record the one-root restart result and exact replay/read evidence.
- Record semantic manifest acceptance and every negative mutation result.
- Record `verify:bifrost`, formatting, linting, and clean diff results.
- Inspect every changed file against the expected write set and confirm no
  durability, audit, resource, tenant, or public-contract behavior changed.

## Material stop conditions

- A separate role root is proven necessary for correctness rather than tuning.
- Preserving durability requires a WAL format, identity, database, object, or
  public contract change.
- One-root test injection cannot preserve production-equivalent restart
  authority without weakening a recovery assertion.
- Kubernetes cannot provide one stable claim per durable replica under the
  selected workload topology.
- Any such condition returns to `$wyrd-spec`; it is not resolved by adding an
  override, alias, fallback, or second task.

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
- `architecture/references/languages/implementation-execution.md`
- `architecture/references/languages/testing-workflows.md`
- `changes/active/bifrost-single-data-root/spec.md`
