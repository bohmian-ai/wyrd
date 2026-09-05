---
id: BIFROST-R6-T05-R03-OTLP-MATERIAL
title: Admit OTLP from actual projected material facts
kind: remediation
status: review
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 6
parent_task: BIFROST-R5-T05-MCP
depends_on: [BIFROST-R6-T05-R01-MCP-QUERY-CORRECTNESS]
requirements: [REQ-010, REQ-012]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-009]
acceptance: [AC-009]
reviewed_candidate: d4fdd6e6d
remediates: [FIND-BIFROST-R5-T05-MCP-7]
---

# Correct the production dependency of the real trace journey

Required execution skill: `$wyrd-implement`.

## Outcome and scope

Small valid OTLP exports must reach durable storage and the existing MCP trace
journey. Scribe must reserve the actual Arrow-plus-IPC projection before it
materializes, rather than treating decoded request size as projected size.
Fix all three typed OTLP callers of the shared material estimator: traces,
logs, and metrics. Preserve schemas, mapping semantics, transport decode
ownership, authentication, tenant attribution, WAL/ack ordering, source
partitioning, resource ceilings, and cancellation. No new dependency, public
contract, setting, or transport is required.

Owners: `vala-bifrost-redux/src/scribe/{material_plan,ingress,preprocess,
direct_traces,direct_logs,direct_metrics,otlp_managed}.rs`. The existing
`FixedIpcPlan` encoder and validators remain authoritative. The real journey
already committed in `wyrd-mcp/tests/bifrost/mcp/query.rs` remains the primary
cross-boundary proof. This task unblocks the unfinished OTEL journey task;
it does not replace or erase that task's failed evidence.

## Scenario 1 — the admitted plan covers the projection

**RED.** Extend the existing direct trace/log/metric unit fixture helpers to
use the production `ScribeIngressPlanner` limit instead of an unlimited limit
for their normal success tests. Add one focused test in each direct module,
`ingress_plan_covers_projection`, using that module's existing valid fixture,
a small payload, and the same authenticated identifiers for planning and
projection. Assert successful projection, exact Arrow-plus-encoded-IPC size
within current material, and persistence candidate coverage. Include repeated
resource/scope values and escaped values to catch expansion rather than merely
covering schema overhead. The existing standalone exact-limit tests continue
to test projection refusal at one byte below its exact requirement.

Run each exact selector separately:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support,bench-support --lib -E 'test(=scribe::direct_traces::tests::ingress_plan_covers_projection)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support,bench-support --lib -E 'test(=scribe::direct_logs::tests::ingress_plan_covers_projection)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support,bench-support --lib -E 'test(=scribe::direct_metrics::tests::ingress_plan_covers_projection)'
```

**GREEN.** Add a private cohesive `OtlpProjection` owner in `otlp_managed.rs`
borrowing the authenticated principal and request ID and carrying the expected
schema fingerprint, batch ID, and receipt timestamp. Move the three existing
state-dependent `project` operations to its inherent signal methods. Reuse
this owner both at Scribe preflight and the existing post-admission projection
sites; do not thread six context arguments through another free-function graph.

Each signal's existing count pass remains the sole source of accepted-row,
null, and variable-byte facts. Extract its material-planning portion into an
inherent method on that existing invariant-bearing count-plan struct. Reuse
`OtlpManagedProjection::plan`, its managed facts, and `FixedIpcPlan` counting to
measure the exact Arrow-plus-IPC bytes without creating Arrow arrays, JSON
containers, rendered rejection strings, or input-sized serialization buffers.
Fixed schema/descriptor scratch remains bounded by the built-in schema and
existing fixed descriptor capacity, independent of request cardinality.

Pass the same projection owner into `ScribeIngressPlanner`'s three typed OTLP
methods after existing wire/cardinality/depth/value validation. Replace the
request-size estimate with the measured current material. Set persistence
candidate bytes to the measured Arrow bytes; recompute source facts and root
and replay peaks through `IngestMaterialPlan`'s existing checked finish method.
The retained decoded request stays independently charged in request bytes.
Empty/all-rejected exports require no projected source, while preserving
existing partial-rejection semantics. Native and already-projected ingress
remain unchanged.

Use the existing configured OTLP candidate ceiling (configured request bytes +
value bytes + maximum managed projection) as the upper material envelope, not
the current request's decoded length. Derive that ceiling in one shared checked
helper used by boot envelope sizing and typed OTLP planning. Refuse measured
material above this existing boot envelope before allocation with the existing
material-too-large error carrying measured bytes and this actual ceiling.
Keep the subsequent root maximum-envelope decision and global/tenant/table
reservation authoritative. No arbitrary multiplier, schema-size constant,
request padding, or reservation growth after materialization is permitted.

**REFACTOR.** Keep deterministic count helpers pure and synchronous. Share
production count/encode facts; do not duplicate OTLP mapping or FlatBuffer
sizing. Document every changed item and its error behavior. Ensure all call
sites use the dependency-owning projection owner, including tests.

## Scenario 2 — limits and real operational debugging

**RED/GREEN.** Add the focused unit
`scribe::material_plan::tests::otlp_projection_respects_configured_envelope`.
Use a valid export whose expanded output exceeds deliberately small configured
material bounds while its wire counters fit; assert typed refusal before any
projection. Also exercise empty/all-rejected input without phantom material
and checked overflow. Run:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support,bench-support --lib -E 'test(=scribe::material_plan::tests::otlp_projection_respects_configured_envelope)'
```

Keep the original real MCP journey as the already-observed RED (HTTP 413,
8193 measured bytes against the incorrect 2440 estimate). Run its exact command
to GREEN and retain its discovery, schema, time-filtered ERROR result, ceilings,
and pre-disconnect cancellation assertions:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey -E 'test(=query::pg_tests::agent_debugs_otel_error_trace_through_mcp)' --run-ignored=all"
```

Preserve the production guard and correct planning if the projection exposes
another undercount. Do not increase node budgets to hide incorrect arithmetic.

## Verification and handoff

Run `mise run test:bifrost:integration:redux`,
`mise run test:bifrost:journey:mcp`, `mise run test:bifrost:journey:oracle`,
`mise run fmt`, `mise run lints`, and `git diff --check`.
Review the complete Task 05 candidate cumulatively after this dependency and
the OTEL journey are green. Keep the recorded unrelated family/boundary
failures explicit; this task does not authorize changing their checks.

Authority: `AGENTS.md`; `architecture/agent-rules.md`;
`architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`;
`architecture/bifrost-design.md` Ingest/Resource invariants;
`architecture/wyrd-security-posture.md`; approved revision 6 Journey E/AC-009;
`architecture/references/languages/{implementation-execution,testing-workflows}.md`.

Return for plan revision if the fixed preflight scratch cannot remain bounded
independently of request size, or if existing configured envelope arithmetic
cannot bound the chosen exact material cap without changing operator-facing
settings or boot behavior. No public behavior revision is authorized here.

## Readiness

Independent `wyrd-task-readiness`: READY, no blocking findings.

## Execution evidence

Scenario 1 shared-root RED: exact trace selector failed because 2337 admitted
bytes could not hold the existing exact 7512-byte projection (0.456s).
Initial GREEN: the same exact trace selector passes (0.021s). Signal methods
now live on one borrowed authenticated `OtlpProjection` owner; all typed
preflight and post-admission call sites use it. Existing test identity setup
was consolidated into a small test-only owner because the borrowed projection
must retain its principal/request lifetime; no test transport or fixture
framework was added. Sibling regression and cap/journey verification follows.

Scenario 1 coverage GREEN: all three exact signal selectors pass (traces
0.022s, logs 0.014s, metrics 0.016s). They compare admitted bytes with actual
Arrow memory plus actual encoded IPC, including repeated resource/scope data
and escaped values. Existing normal fixture projections also use production
admission; standalone exact-limit tests retain their explicit limits.

Scenario 2 RED mutation: bypassing the configured log material ceiling makes
the exact envelope selector fail at its expected typed-refusal assertion
(0.435s). Restoring the guard yields GREEN (0.027s), including empty and
all-rejected inputs with zero projected material and checked configuration
overflow. This cap failure has no cross-boundary state; it is deliberately
forced in the pure planner rather than by reconfiguring a live server.

Real OTLP journey GREEN: the exact Postgres-wrapped MCP selector passes
(2.551s), including actual error-span discovery/query, ceilings, and cancellation.
Refactor removes the obsolete decoded-input projection estimate entirely:
request bytes are validated first; validated counters and measured material
form one final root/replay plan. No intermediate estimated root is retained.

## Final verification

- All four exact unit selectors passed after the final borrow correction;
  the envelope selector passed again after its test-only initializer cleanup
  (0.024s). The exact real OTLP MCP journey passed again (2.628s).
- `mise run test:bifrost:integration:redux`: PASS, 965 tests (36.036s).
- `mise run test:bifrost:journey:mcp`: PASS, all 6 journeys (4.219s).
- `mise run test:bifrost:journey:oracle`: PASS, all 23 journeys (70.851s).
- `mise run fmt` and `mise run lints`: PASS; final workspace all-features,
  all-targets lint run completed in 40.09s.
- The broad lanes preceded only the behavior-preserving borrow correction and
  test initializer cleanup; the four exact units, real OTLP journey, and final
  formatting/linting cover those final differences.
- `git diff --check`: PASS. Public wire/schema/generated contracts unchanged.
- Final client-tier/error-coverage checks PASS. Tenant-isolation/unwrap checks
  retain only the previously recorded unrelated failures; no check was changed.
