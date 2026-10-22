---
id: TASK-006
kind: implementation
status: proposed
spec: SPEC-docsite-incremental-reset
spec_revision: 8
requirements: [REQ-008, AC-009]
depends_on: [TASK-005]
parent_task:
remediates: []
---

## Outcome and Value

Wyrd identifies externally owned models and baseline data while owning
expectations, evidence, judgment, and reaction. One cohesive contract change
delivers REQ-008/AC-009 through persistence, drift fitting, and every consumer.

## Owners, Scope, Consumers, and Prohibited Changes

Use `$wyrd-implement`. Model retains metadata identity and optional external
`uri`; Data references an Artifact holding Parquet rows. Remove Experiment,
Mcp, Policy, and Audit as Card kinds. Keep Data, Model, Prompt, Agent, Service,
Artifact, Verifier, Trigger, Operator, and Source. Authorization remains
role-based; audit remains the system table with non-blocking writes.

Delete Model/Data packaging, payload upload/pull/trust, framework interfaces,
signatures, splits, hydration and interface overrides, including
`crates/wyrd/wyrd-interfaces` and its dependency/export closure. Preserve
Artifact upload/integrity and Verifier artifact associations. Each drift
Verifier pins its own Data baseline; a new baseline requires a new Data version.
The server fitter reads that Data's referenced Artifact, never a loaded SDK
dataset. Delete stored removed-kind and old Model/Data envelopes by migration,
without compatibility reads or aliases.

TASK-005 supplies the declaration/state seam; revision 8 supersedes its
conflicting Model/Data hydration and backing-file obligations. Preserve the
minimal assertion journey, offline bundle loading, and no-files default.
Source-query baselines, model integrations, new authorization systems, and
Bifrost engine changes are excluded. This task does not expand later guides.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Reference-only Cards and kind removal | `wyrd-spec/src/envelope.rs`: `CardKind`, `Spec::from_kind_and_value`; `card/{model,data}.rs`: `ModelSpec`, `DataSpec`; `refs/mod.rs` | `card/mod.rs` kind/schema tests; SDK declaration journeys | Four obsolete kinds and payload-bearing Model/Data contracts remain | Reduce existing contracts, reference derivation, validators, and projections | No replacement parser or Card system |
| Declaration, storage, state | `wyrd-client`: `Cards::declare_from_path`, `CardGraphPuller::pull`, `WyrdState::declare_with`; Python `PythonStateHydrator::hydrate`; `wyrd-cards` Model/Data holders | Three SDK declaration journeys; CLI `tests/card_lifecycle.rs`; Python state hydration tests | Model/Data still package or hydrate payloads and expose framework configuration | Remove those branches and interface crate; retain reference projections and Artifact owners | No new loader, cache, transport, or lifecycle owner |
| Baseline fitting | Server `EffectiveSpecs::validate_baseline`, `BaselineFitter::resolve`/`fit`; registration's `DriftBaselineQueue`; `vala_drift::fit_baseline_until` | Rust scheduled drift journey; server `pg_verification_runtime.rs`; SQL `pg_drift_baselines.rs` | Validation requires Data interface/schema; fitter reads a Data-owned `data/data.parquet` | Resolve pinned Data → Artifact through current registry/storage; fit actual Parquet using existing bounded fitter | No Source execution or duplicate fitting pipeline |
| Stored-envelope removal | `wyrd-sql` cards migrations and `wyrd.cards`; migration lease/runner | `tests/integration/pg_migration.rs`, `UnmigratedDatabase` | Stored old envelopes survive contract removal | Add forward migration with dependent-row closure and retained-state regression | Existing migration owner suffices |
| Public discovery and authority | Server MCP `GetCardArgs::read`, `descriptors`; shared Card HTTP owners; CLI lifecycle; schema generators | MCP `discovery.rs`; SDK typed reads; served OpenAPI lane | Kind lists, schemas, exports and wording advertise old ownership | Update existing surfaces/generators and four named authority documents | No new route, alias, catalog, or compatibility facade |

Paths are relative to their current crate/SDK owners and guide scope, not
private implementation structure.

## Approach

1. Update the four REQ-008 authority documents and pure Card/reference contracts
   together, replacing the superseded ownership and kind statements.
2. Remove obsolete behavior and dependencies across server, shared client,
   SDKs, CLI, MCP, UI consumers, examples, and fixtures; retain useful coverage
   through the surviving reference/Artifact workflows.
3. Connect baseline validation/fitting to Data → Artifact Parquet while
   preserving exact identities, tenant isolation, bounds, and stable failures.
4. Migrate old stored envelopes and dependent rows through the current migration
   owner; regenerate contracts and close consumers with the scenarios below.

## Ordered Implementation Scenarios

### Scenario 1 — Reference Cards round-trip; removed contracts are refused

**Behavior.** All SDKs declare/read Model references with and without `uri`
and Data → Artifact references. Removed kinds and old Model/Data fields are
refused with catalog errors, including raw HTTP submissions; CLI and MCP
discovery/read contracts agree (REQ-008/AC-009).

**RED.** Add focused journeys J-Rust, J-Python, J-TypeScript, J-CLI, and
J-MCP below. Fail on old required fields, accepted removed contracts, stale
discovery schemas, or payload loading. Support them with pure serde/schema
tests; unit-only refusal proof is insufficient.

**GREEN.** Reduce contracts and projections at their owners; remove obsolete
runtime/dependency branches. Keep Artifact upload, digest refusal, and exact
graph reads working. A Model `uri` is a reference, not a fetch instruction.

**REFACTOR.** Delete unreachable adapters and public-only types; reuse the
existing declaration, graph, error, and runtime boundaries. Rerun this scenario.

### Scenario 2 — Drift fits the Verifier's versioned Data baseline

**Behavior.** Upload Parquet as an Artifact, declare its Data reference and
drift Verifier, wait for baseline readiness, and judge observations on a real
server. Two Verifiers may name different Data versions without changing each
other's baseline. Missing/unusable baseline artifacts fail through existing
stable errors; tenant and authority boundaries remain enforced (REQ-008/AC-009).

**RED.** Add J-Drift below using the existing SDK/server journey owner. Expect
failure because declaration and fitting still require the old Data shape and
Data-owned upload path. Cover version pinning and unusable Parquet in that
journey; retain corresponding SQL/fitter regressions.

**GREEN.** Extend existing resolution and fitting to Data → Artifact, reading
feature information from the Parquet rather than removed Data interface/schema
fields. Preserve bounded decoding, cancellation, readiness, and judgment.
Rerun scenarios 1–2 and the minimal assertion Quickstart regression.

**REFACTOR.** Remove Data-payload path assumptions; keep one baseline queue,
registry resolver, storage path, and fitter. No Source-query preparation.

### Scenario 3 — Upgrade removes only the obsolete stored envelopes

**Behavior.** Migration removes stored Experiment/Mcp/Policy/Audit and old
Model/Data shapes, resolves dependent-row constraints, and leaves valid
surviving Cards, Artifact integrity, role authorization, and system audit
history intact. Normal reads need no compatibility decoder (REQ-008/AC-009).

**RED.** Add M-Upgrade below in the existing unmigrated-database test owner.
Seed historical envelopes before applying the new migration; expect obsolete
rows to survive or upgrade/readback to fail.

**GREEN.** Add the forward migration through the existing leased runner and
prove retained-state readback and repeat migration safety. Do not rewrite
historical migrations. Rerun scenarios 1–3 against the upgraded schema.

**REFACTOR.** Remove obsolete live queries/constraints and fixture assumptions;
retain historical migration inputs and the supported schema's tenant boundary.

## Acceptance Criteria

- Server, wire, SDKs, CLI, MCP, and generated schemas expose the ten surviving
  native kinds and reference-only Model/Data shapes; old inputs receive stable
  errors. J-Rust/J-Python/J-TypeScript/J-CLI/J-MCP prove this closure.
- J-Drift proves Data → Artifact Parquet fitting, independent pinned baselines,
  a new Data version, and meaningful failure/refusal behavior on a real server.
- M-Upgrade proves stored-envelope removal and surviving-state preservation.
- No Model/Data packaging/hydration/interface override or removed Card-kind
  implementation remains; Artifact upload/integrity and assertion verification
  still work. SDK export/typing and dependency checks pass.
- The four named authorities and derived contracts agree with REQ-008.
  Source-query baselines remain deferred.

## Expected Write Set and Consumer Closure

Pure `wyrd-spec` contracts/references/errors/schema sources; `wyrd-cards`,
`wyrd-loader`, `wyrd-client` state/declaration/pull; server registration,
verification fitter and HTTP/MCP projections; `wyrd-sql` migration/query owners;
three SDK bindings/exports/authored stubs/generated declarations; CLI/UI
consumers, affected examples/tests/fixtures, manifests and lockfiles. Remove
`wyrd-interfaces` and dependencies used only by removed behavior. Keep framework
or data utilities still needed by surviving capabilities.

Update `AGENTS.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-design.md`, and
`architecture/wyrd-design.md`; regenerate
derived contracts/docs through their owners. This is consumer closure, not a
general documentation rewrite or permission to edit other change packets.

## Verification and Evidence

The following new tests are planned in inspected existing targets. Use their
current real-server fixtures; private fixture design remains implementation-owned.
Prepare with `mise run build:server`, `mise run py:setup`, `mise run ts:build`,
and `mise run ts:build:testing`; keep Cargo-backed operations sequential.

```bash
# J-Rust: existing wyrd-ai integration target/module
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration --run-ignored ignored-only -E 'test(=declare_and_pull::verification_reference_cards_round_trip_and_refuse_old_contracts)'
# J-Python: existing integration-marked declaration module
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_declare_and_pull.py::test_verification_reference_cards_round_trip_and_refuse_old_contracts
# J-TypeScript: existing Vitest declaration module
mise exec -- scripts/postgres/with-test-postgres.sh -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/declare-and-pull.test.ts -t '^verification reference cards round trip and refuse old contracts$'
# J-CLI: existing cli target, card_lifecycle module
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=card_lifecycle::pg_tests::verification_reference_cards_round_trip_and_refuse_old_contracts)'
# J-MCP: existing MCP discovery journey target/module
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-mcp --test mcp -E 'test(=discovery::pg_tests::verification_reference_card_discovery_and_reads)'
# J-Drift: existing scheduled drift SDK journey module
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration --run-ignored ignored-only -E 'test(=scheduled_drift_alerts_operator::verifiers_fit_their_pinned_data_artifacts)'
# M-Upgrade: existing SQL migration target/module
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-sql --test integration -E 'test(=pg_migration::pg_tests::verification_owned_kinds_remove_obsolete_envelopes)'
# Existing assertion journey regression
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_docsite_quickstart.py::test_docsite_quickstart_verifies_and_repeats
```

Run affected pure contract/client/fitter tests with exact selectors recorded
in implementation evidence, plus `mise run test:cards:unit`,
`mise run py:test:unit`, and `mise run ts:test:unit`. Run `mise run fmt`,
`mise run lints`, `mise run py:format`, `mise run py:lints`,
`mise run py:typecheck`, `mise run ts:format:check`, `mise run ts:lints`,
`mise run ts:typecheck`, `mise run ts:napi:check`, `mise run codegen:check`,
`mise run check:deps`, `mise run check:tenant-isolation`, and
`mise run test:principals:integration` for the served OpenAPI contract.
Run `mise run docs:check` for changed derived docsite content and the UI's
own scoped checks if its consumers change. Finish with `git diff --check`.

Authority wording, generated output, and deletion inventory use static proof,
not manufactured RED. Record a repository search for removed kind APIs and
`wyrd-interfaces`: no live references outside history. Distinguish historical
migration/refusal inputs from supported contracts; preserve system audit and
MCP protocol functionality. Do not add a permanent name-ban check. Enable
tracing before diagnosing test failures.

## Material Stop Conditions

Stop if closure requires deleting surviving evidence/Artifact data, changing
role authorization or audit semantics, adding compatibility behavior, a new
dependency/feature, or executing Source queries. Contract removal and the
specified destructive envelope migration are already authorized by revision 8;
ordinary field naming and private structure remain implementation-owned.

## Authority Links

- [Approved spec revision 8](../spec.md), REQ-008/AC-009
- [Declaration dependency](TASK-005-declare-ready-state.md), subject to revision 8
- [Repository standards](../../../../AGENTS.md) and [agent rules](../../../../AGENTS.md)
- [Protocol](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-design.md), and [vocabulary](../../../../architecture/wyrd-design.md)
- [Security](../../../../architecture/wyrd-security-posture.md) and [Bifrost boundary](../../../../architecture/bifrost-design.md)
- [Spec workflow](../../../../changes/spec-driven-development.md), [implementation execution](../../../../changes/implementation-execution.md), and [testing workflows](../../../../TESTING.md)
