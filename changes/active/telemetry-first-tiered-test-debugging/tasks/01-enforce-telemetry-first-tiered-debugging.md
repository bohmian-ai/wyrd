---
id: TELEMETRY-DEBUG-T01
title: Capture tiered-test telemetry and reject direct-print debugging
kind: implementation
mode: DECOMPOSE
status: proposed
spec: SPEC-telemetry-first-tiered-test-debugging
spec_revision: 1
depends_on: []
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005]
---

# Capture tiered-test telemetry and reject direct-print debugging

## Outcome and value

Rust Bifrost Tier-1 journeys and Tier-2 integration tests expose production
logs, traces, and metrics through their owning harnesses. A process-cluster
failure retains child telemetry even while its control loop is blocked, and one
repository check prevents tests from substituting temporary print debugging for
production observability.

Required execution skill: `$wyrd-implement`.

## Current repository facts

- `wyrd_testing::bifrost::cluster::process_telemetry` already installs
  `wyrd_telemetry::init_test_capture`, the production Prometheus recorder, and
  `BifrostTelemetryCapture` once for in-process journeys. Its panic hook renders
  `BifrostTelemetryCapture::failure_diagnostics`.
- `BifrostTelemetryCapture` already owns production checkpoints, metric deltas,
  completed spans, and bounded failure-oriented rendering. Do not create a
  second in-process capture model.
- `OracleTelemetryCapture` already projects the process recorder and trace
  exporter for Oracle-specific assertions.
- `process_cluster::child::install_child_tracing` currently installs only a
  stderr formatter and only when `RUST_LOG` is set. Child processes install no
  metric recorder or trace consumer.
- The parent already drains each child's stderr concurrently and retains the
  newest 512 complete lines. Its stdout reader is the sole owner of the JSON
  control protocol.
- `ControlRequest::AwaitExecutePaused` awaits inside the child control loop.
  Diagnostics requested through that same loop would therefore deadlock or
  time out in the exact failure being addressed.
- `OracleAnalyticalStageIngress::authorize_stage_message` already owns the
  production `bifrost.oracle.analytical.stage` span and increments
  `bifrost_oracle_analytical_stage_operations_total` before the test-support
  execute pause. It does not emit a success transition before that pause, and
  its span cannot finish until the pause is released.
- The compiled Tier-2 `integration` target currently contains one compatibility
  test and has no telemetry installation. Empty archived modules are not an
  implementation target for this task.
- The canonical Rust Bifrost tiered-test roots are currently free of
  `print!`/`println!`/`eprint!`/`eprintln!`/`dbg!`. Other repository test roots
  contain legitimate environment-skip messages and are outside this check.

## Owners, scope, consumers, and prohibited changes

- `wyrd-testing::bifrost::process_cluster` owns child telemetry installation,
  the independent child reporter, parent stderr retention, and process failure
  rendering.
- `OracleAnalyticalStageIngress` owns the missing authorized-stage transition
  because it performs the verified graph activation and records the existing
  stage-operation metric.
- The `vala-bifrost-redux` `integration` target owns its process-global Tier-2
  test consumer. It may depend on shared telemetry and Prometheus test support,
  but not on `wyrd-testing` or `wyrd-server`.
- `scripts/checks`, `mise.toml`, `AGENTS.md`, `TESTING.md`, and the canonical
  `wyrd-implement` skill own enforcement and contributor workflow.

Do not change a public or persisted contract, production telemetry destination,
metric name or labels, sampling policy, process-control frame, test timeout,
Oracle execution result, or production lifecycle. Do not add a new service,
socket, OTLP collector, dependency package, Cargo feature, test retry, fixed
test sleep, or persistent telemetry artifact. Do not restore or edit empty
archived Tier-2 modules.

## Selected implementation architecture

### Process-child capture and reporting

Replace `install_child_tracing` with one concrete child telemetry owner retained
by `run_peer_test_node` until `serve` and ordered shutdown finish. The owner:

1. calls `wyrd_telemetry::init_test_capture` with the existing production
   filter resolution and a child-specific service name;
2. calls `wyrd_server::app::metrics::install_recorder`, matching production
   recorder configuration in the fresh child process;
3. constructs the existing `BifrostTelemetryCapture` from those two handles;
4. passes the retained `TelemetryGuard` into `WyrdTestServerBuilder` through
   `with_telemetry_for_test`, so the server and reporter share one process
   provider; and
5. owns one dedicated reporter thread, its stop flag, and its join handle.

The reporter uses the existing `BifrostTelemetryCapture` checkpoint/delta API.
At the already-established 100 ms `READY_POLL` cadence, it closes one
checkpoint, emits only newly completed spans and changed production metric
series, then opens the next checkpoint. Empty deltas emit nothing. Each tick
caps its rendered observations so reporter work and stderr volume remain
bounded; the parent's existing 512-line tail remains the final retention bound.

The reporter writes its compact, clearly prefixed diagnostic records directly
to locked stderr with `std::io::Write`. It must not call `tracing` while
reporting telemetry, which would recursively observe itself. Ordinary
production `tracing` events continue through the formatter to the same stderr
drain. Reporter parse or render failures add one bounded diagnostic-degradation
line and continue from a fresh checkpoint; they never replace the child result.

Dropping the owner sets the stop flag, unparks the thread, joins it, flushes the
telemetry guard, and then permits provider teardown. A child installation
failure is a startup failure reported through the existing fatal stderr
fallback; the child does not silently run without capture.

Parent timeout, disconnect, unexpected response, and child-failure construction
must append the affected node label and its current retained stderr tail. Keep
that assembly on an inherent `ProcessNode` helper so every request path uses one
bounded diagnostic formatter. Do not add another IPC channel: stdout remains
control-only and the stderr drain already operates independently of the blocked
control loop.

### Production stage transition

Inside `OracleAnalyticalStageIngress::authorize_stage_message`, after verified
graph activation and the existing `record_stage_operation` call but before the
test-support pause, emit one production `debug` event stating that the
analytical stage operation was authorized. Retain the current span fields
(`operation`, scrubbed `node_id`, and `outcome`) rather than adding new metric
labels or query payloads. This event is the live transition visible while the
owning span is still open; after release, the same production span completes and
the child reporter exports it.

No other Oracle telemetry hook is changed unless the RED journey proves this
exact production transition still cannot be observed. Any broader missing
signal is a specification revision, not permission to instrument unrelated
paths.

### Tier-2 process capture

Add one `tests/integration/telemetry.rs` module backed by `std::sync::OnceLock`.
Its concrete capture owner retains:

- the `TelemetryGuard` and `TestTraceCapture` returned by
  `wyrd_telemetry::init_test_capture`; and
- a `PrometheusHandle` installed by a test-owned `PrometheusBuilder`.

Use existing workspace `wyrd-telemetry` with `test-support` and
`metrics-exporter-prometheus` as direct dev-dependencies. Add no package or
feature to the workspace. The simple Tier-2 recorder is a test consumer; it
does not copy the server's histogram policy or move production metric ownership
downward.

The integration module installs one panic hook after capture initialization.
It invokes the previous hook, then renders only the newest bounded completed
spans and a bounded tail of Prometheus exposition to locked stderr. Formatter
logs already flow to nextest stderr through `init_test_capture`. Rendering
failure degrades to one line and never panics recursively.

The target's current compatibility test calls the initializer before it creates
its production `OracleSessionShape`. Future Tier-2 fixtures that start an
instrumented Bifrost owner must call the same initializer at their construction
boundary; this rule is documented in the Tier-2 test README. Do not introduce a
constructor dependency or annotate every test.

### Repository enforcement

Add one `scripts/checks/tiered-test-telemetry.sh` check. Its default invocation
uses `rg` over exactly:

- `crates/wyrd/wyrd-testing/tests/bifrost/**/*.rs`;
- `crates/vala/vala-bifrost-redux/tests/**/*.rs`;
- `crates/vala/vala-sdk/tests/pg_bifrost_e2e.rs`; and
- `crates/wyrd/wyrd-mcp/tests/bifrost/**/*.rs`.

Reject the five direct-output macros from the approved spec and print the
telemetry-first remediation. Do not scan all repository tests: storage and cloud
tests have legitimate skip output. Do not add an allowlist while the canonical
roots are clean.

The same script owns a bounded environment-selected self-test using `mktemp -d`:
a clean Rust file passes and one occurrence of each prohibited macro fails. The
one `check:tiered-test-telemetry` mise task runs the self-test and real scan; add
that task to `gate`.

Add the normative rule to `AGENTS.md` §11, list the check in `TESTING.md`, and
add the telemetry-first failure-localization sequence to the canonical
`.agents/skills/wyrd-implement/SKILL.md`. Run `skills:sync`; do not hand-edit the
generated Claude mirror or add harness-specific hooks.

## Ordered implementation scenarios

### Scenario 1 — A blocked process child remains diagnosable

**Behavior.** A real analytical `ExecuteTask` reaches a follower, records its
production authorized-stage log event and stage-operation metric before the
existing pause, and later completes its production span. The parent observes
those signals through the independent stderr drain while the control loop is
waiting, without test-authored direct output. A process failure retains the same
bounded evidence with node identity. Maps REQ-001–REQ-005 and AC-001.

**RED.** Add
`peer_network::listener::child_production_telemetry_remains_visible_while_control_waits`
to the existing Oracle process-network target. Reuse the current real
process-cluster analytical setup and execute-pause seam. Poll a telemetry
predicate to a bounded deadline while the pause is held; do not use a fixed
sleep. Assert the follower tail contains the production authorized-stage event
and changed ExecuteTask metric, then release the pause and assert the completed
`bifrost.oracle.analytical.stage` span appears. Finally exercise the existing
bounded process-error renderer and assert it names the node and retains those
signals. It initially fails because the child has neither trace nor metric
capture and emits no success transition before the pause.

Run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=peer_network::listener::child_production_telemetry_remains_visible_while_control_waits)"'
```

**GREEN.** Implement the child telemetry owner, non-recursive incremental stderr
reporter, shared process-error rendering, and the one production authorized-stage
event exactly as selected above. Use the current control protocol, stderr tail,
capture parser, and execute-pause authority. Do not add a diagnostic request or
another transport.

**REFACTOR.** Keep process lifecycle ownership on the child telemetry owner,
keep stdout control-only, retain the existing capture types, and keep the Oracle
event on the concrete stage-ingress owner. Remove the old conditional
`install_child_tracing` path rather than preserving two initialization modes.

### Scenario 2 — Tier-2 owns one production-shaped process capture

**Behavior.** The Tier-2 integration target initializes one production-shaped
tracing consumer and one Prometheus test recorder, retains them for the process,
and exposes bounded panic diagnostics without a server-tier dependency. Maps
REQ-001, REQ-002, REQ-004, REQ-005 and AC-002.

**RED.** Add
`telemetry::tier_two_capture_installs_once_before_runtime_use`. Call the
initializer twice and assert both calls return the same retained owner and that
its guard, trace capture, and recorder are available. The test initially fails
because the integration target has no telemetry module or initializer.

Run:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support,bench-support --test integration -E 'test(=telemetry::tier_two_capture_installs_once_before_runtime_use)'
```

**GREEN.** Add the `OnceLock`-owned integration telemetry module, its bounded
panic renderer, the two existing-workspace dev-dependencies, and the initializer
call at the start of the current compatibility test. Update the Tier-2 README so
new production-runtime fixtures reuse this initializer.

**REFACTOR.** Keep the owner inside the integration target, keep Vala free of
server-tier and `wyrd-testing` dependencies, and do not extract a shared
abstraction until another real lower-tier consumer needs the same dependency
shape.

### Scenario 3 — Direct-print debugging is rejected at the repository boundary

**Behavior.** Canonical Bifrost Tier-1 and Tier-2 Rust tests cannot introduce
direct-print debugging, and both Codex and Claude implementation workflows point
contributors to captured production signals and production-owner
instrumentation. Maps REQ-005–REQ-007 and AC-003–AC-004.

**RED.** Add the single check script and run its self-test before wiring the
real scan. The negative cases must fail for each prohibited macro and the clean
case must pass. Confirm the check is absent from `mise` and `gate` before GREEN.

Run:

```bash
WYRD_TIERED_TEST_TELEMETRY_SELF_TEST=1 bash scripts/checks/tiered-test-telemetry.sh
```

**GREEN.** Implement the exact-root `rg` check, its actionable error, one mise
task that runs self-test plus repository scan, aggregate-gate membership, the
`AGENTS.md` and `TESTING.md` rules, and the canonical implementation-skill
workflow. Regenerate the Claude skill mirror through `mise run skills:sync`.

Run:

```bash
mise run check:tiered-test-telemetry
mise run check:skills-sync
```

**REFACTOR.** Keep one shell check, one canonical workflow skill, and one
normative repository rule. Do not add a general print linter, per-agent hooks,
or duplicate handwritten Claude instructions.

## Cross-scenario decisions and invariants

- Scenario 3 lands after Scenarios 1 and 2 so the repository does not prohibit
  direct debugging before its required telemetry paths work.
- Test consumers may differ by dependency boundary, but every observation must
  originate from the same production producer used outside tests.
- The child reporter uses stderr because it is already independently drained;
  using the control loop would fail the blocked-request requirement, while a
  new network collector would add an unearned service.
- The reporter's own output is a sanctioned harness sink and never enters the
  prohibited test roots. Test-authored output remains forbidden.
- Metric evidence uses closed production labels. Node and execution identity
  remain scrubbed trace/log fields.
- No telemetry assertion may be satisfied by emitting a signal from the test.
- The original test or process error remains primary when diagnostics degrade.

## Exact named-test commands

Run the named tests sequentially:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=peer_network::listener::child_production_telemetry_remains_visible_while_control_waits)"'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support,bench-support --test integration -E 'test(=telemetry::tier_two_capture_installs_once_before_runtime_use)'
```

## Broader verification

Run sequentially:

```bash
mise run check:tiered-test-telemetry
mise run check:skills-sync
mise run test:bifrost
mise run test:bifrost:journey:oracle
mise run fmt
mise run lints
mise run gate
git diff --check
```

`mise run gate` is required because the task changes shared test and CI gate
infrastructure. Do not run Cargo-backed verification concurrently in the shared
checkout.

## Completion evidence

- The Scenario 1 RED fails for missing child telemetry, then passes with the
  production event, metric, and completed span visible through the parent.
- The Scenario 2 RED fails for absent Tier-2 capture, then passes with one
  retained process owner and no forbidden dependency direction.
- The check self-test proves every prohibited macro is rejected and the clean
  case passes; the canonical real roots pass.
- `AGENTS.md`, `TESTING.md`, the canonical implementation skill, and its
  generated Claude mirror agree on the telemetry-first workflow.
- The exact named tests, full Tier-2 lane, Oracle journey lane, format, lints,
  skill sync, aggregate gate, and diff check pass sequentially.
- Final inspection finds no temporary prints, duplicate telemetry stack,
  changed public contract, new package/feature, weakened test, or unrelated
  cleanup.

## Material stop conditions

- The production child cannot retain both the shared telemetry guard and metric
  recorder without changing a public server contract or dependency direction.
- Correct child evidence requires a new network service, wire protocol, Cargo
  feature, or third-party package rather than the existing stderr drain and
  workspace dependencies.
- The authorized-stage event would require exposing sensitive payloads or
  adding high-cardinality metric labels.
- Tier-2 capture requires a dependency on `wyrd-testing` or `wyrd-server`.
- An applicable architecture authority contradicts the approved telemetry-first
  behavior or its acceptance obligations.

Any such discovery returns to `$wyrd-spec`; ordinary private wiring, bounded
renderer details, and exact existing symbol-name corrections remain local
implementation decisions.

## Material authority links

- `AGENTS.md` §§4, 5, 6, 11, 12, and 15.
- `architecture/agent-rules.md` external-test, gate-integrity, struct-centered,
  rustdoc, and synchronous-default rules.
- `architecture/wyrd-design.md` doctrine 20.
- `architecture/bifrost-design.md` §§Distributed analytical execution, Resource
  and failure invariants, Telemetry.
- `architecture/references/languages/spec-driven-development.md`.
- `architecture/references/languages/implementation-execution.md`.
- `architecture/references/languages/testing-workflows.md`.
- `architecture/references/domain/telemetry-observations.md`.
- `crates/wyrd/wyrd-testing/tests/README.md`.
- `crates/vala/vala-bifrost-redux/tests/README.md`.
