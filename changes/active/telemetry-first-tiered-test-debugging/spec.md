---
id: SPEC-telemetry-first-tiered-test-debugging
revision: 1
status: approved
---

# Telemetry-first tiered-test debugging

## Human intent and user value

Tier-1 user journeys and Tier-2 integration tests must be debugged through the
same production tracing, logging, and metrics that operators use in production.
The test harness may replace telemetry destinations with test-owned consumers,
but tests must not create a parallel diagnostic path with temporary direct
printing.

This makes tiered tests continuously qualify production observability, exposes
hot-path blind spots before deployment, and removes the repeated cost of adding
and later removing scattered diagnostic output.

The human approved revision 1 and the recommendations it captures on
2026-09-02.

## Scope

- Establish telemetry-first debugging as the repository rule for Rust Tier-1
  Bifrost journeys and Tier-2 `vala-bifrost-redux` integration tests.
- Preserve the existing in-process `wyrd-testing` production-shaped trace and
  metric capture.
- Make production logs, completed spans, and metric changes from
  `BifrostProcessCluster` child processes available to the parent while a child
  control request is blocked or times out.
- Install production-shaped tracing and a test-owned metric collector once per
  Tier-2 integration-test process.
- Report bounded, redacted telemetry automatically through the tiered-test
  failure path.
- Reject common direct-print debugging macros in the canonical Rust Tier-1 and
  Tier-2 Bifrost test sources.
- Teach the shared Wyrd implementation workflow to diagnose these tests from
  telemetry and to repair missing observations at the production owner.

## Non-goals

- Changing a Wyrd public, wire, generated, persisted, SDK, CLI, MCP, HTTP, or
  Card contract.
- Replacing OpenTelemetry, `tracing`, Prometheus, nextest, or the existing
  `wyrd-testing` capture types.
- Adding an OTLP sidecar, external collector service, telemetry daemon,
  persistent diagnostic artifact store, or new network protocol.
- Requiring every operation or test to emit a trace, log event, and metric when
  one or two signals correctly describe the behavior.
- Adding Python or TypeScript print-output enforcement without a demonstrated
  violation.
- Banning legitimate harness transport output, production formatter output, or
  last-resort fatal-process output.
- Instrumenting every dormant or no-op telemetry hook independent of a real
  production-path blind spot.
- Changing test semantics, timeouts, concurrency, process topology, Oracle
  execution behavior, metric labels, sampling policy, or production telemetry
  destinations.

## Definitions

- **Tier-1 test:** A Bifrost user journey identified by `TESTING.md` and the
  `test:bifrost:journey:*` lanes, including the `wyrd-testing`, `vala-sdk`, and
  `wyrd-mcp` Bifrost targets those lanes own.
- **Tier-2 test:** A `vala-bifrost-redux` integration test owned by the
  `test:bifrost` lane.
- **Production producer:** A production `tracing` span or event, structured log
  event, or `metrics` emission from the concrete owner that performs the
  observed operation.
- **Test consumer:** A test-owned exporter, recorder, formatter, buffer, or
  renderer that consumes production producers without changing what production
  code emits.
- **Direct diagnostic output:** Test-authored use of `print!`, `println!`,
  `eprint!`, `eprintln!`, or `dbg!` to localize or explain a tiered-test failure.
- **Failure diagnostics:** Bounded, non-secret production telemetry retained or
  rendered by the harness when a tiered test panics, a child exits, a control
  channel fails, or a bounded wait expires.
- **Observability defect:** A production path whose available telemetry cannot
  identify the last meaningful lifecycle transition, refusal, failure, or
  resource outcome needed to diagnose a qualified tiered-test failure.

## Required behavior

### REQ-001 — Tiered-test debugging uses production telemetry

Rust Tier-1 and Tier-2 Bifrost tests shall use production tracing, structured
logging, and metrics as their diagnostic evidence. A test may inspect or assert
against a test consumer, but it shall not add a separate diagnostic fact that
the production path did not emit.

### REQ-002 — Applicable test processes install production-shaped capture

Every Tier-1 or Tier-2 process that executes an instrumented Bifrost production
runtime shall install the repository's production telemetry producers before
that runtime starts and shall retain test-owned consumers for the process
lifetime.

The existing in-process `wyrd-testing` installation remains authoritative.
Tier-2 integration and multi-process child installations shall follow the same
producer configuration and differ only in their test-owned destinations and
failure rendering.

An installation failure shall be explicit; the process shall not silently run
an observability-qualified test without its required capture.

### REQ-003 — Multi-process evidence remains available during a blocked child

A `BifrostProcessCluster` parent shall receive bounded production logs,
completed-span evidence, and changed or non-zero production metric evidence
from each child independently of the request-response control loop.

The evidence path shall continue operating while a child waits inside a
long-running control request, including the existing execute-pause wait. A
control timeout, disconnect, or child failure shall identify the child and
include its retained telemetry without requiring another request to that child.

### REQ-004 — Failure diagnostics are bounded, correlated, and safe

Failure diagnostics shall prefer the most recent useful evidence and shall not
grow without bound. They shall retain available closed-cardinality lifecycle
dimensions and scrubbed trace correlation while preserving the existing rule
that tenant, query, task, attempt, path, SQL, and other high-cardinality values
never become metric labels.

Diagnostics shall not render credentials, peer key material, protected payloads,
query text, object paths, or other sensitive values prohibited by production
telemetry policy.

### REQ-005 — Missing evidence is repaired at the production owner

When a qualified Tier-1 or Tier-2 failure cannot be localized from captured
production signals, the missing observation shall be added to the concrete
production owner that performs the lifecycle transition or effect. Tests may
then assert that the production signal reaches the test consumer.

Tests shall not emit surrogate spans, events, or metrics to make an
observability assertion pass.

### REQ-006 — Direct-print debugging is rejected automatically

A repository check shall reject `print!`, `println!`, `eprint!`, `eprintln!`,
and `dbg!` in the canonical Rust Tier-1 and Tier-2 Bifrost test sources.

The check shall cover the current `wyrd-testing`, `vala-bifrost-redux`,
`vala-sdk`, and `wyrd-mcp` Bifrost test roots, explain the telemetry-first
remediation, and run in the aggregate gate. Its own sanctioned harness sinks
remain outside the prohibited source scope or use an explicit non-macro writer.

### REQ-007 — Both agent harnesses receive one shared workflow rule

Repository authority shall instruct implementors to inspect captured telemetry
first, add missing production instrumentation at its owner, and rerun the exact
focused test without temporary direct prints. The canonical shared workflow
skill shall be synchronized to its generated Claude mirror.

## Invariants and prohibited outcomes

- Tiered-test code never becomes the source of a diagnostic fact that production
  does not emit.
- Child stdout remains exclusively the newline-delimited process-control
  protocol.
- Telemetry reporting never blocks the child control loop or requires that loop
  to answer after it has timed out.
- Failure telemetry is bounded and cannot recursively instrument itself.
- Test consumers do not change production signal names, status, attributes,
  metric label sets, or lifecycle ownership.
- High-cardinality or sensitive values are never added to metric labels or
  unsafely rendered to failure output.
- A telemetry installation or reporting failure never replaces or conceals the
  original test or child failure.
- Legitimate harness protocol output, production log formatter output, and
  fatal-process fallback remain possible outside the prohibited test-source
  scope.
- No test timeout, retry, sleep, ignored marker, assertion, or production
  behavior is weakened to satisfy this change.
- No new public interface, service, external dependency, or Cargo feature is
  introduced.

## Externally observable behavior and failure modes

- A developer running a focused Tier-1 or Tier-2 test receives production log
  output and, on failure, bounded trace and metric evidence without editing the
  test to add prints.
- A multi-process timeout identifies the affected node and includes telemetry
  produced before and during the blocked request.
- Setting the existing production log-filter environment variables increases or
  narrows tracing detail without changing test source.
- A test-source direct-print macro fails the repository check with instructions
  to use captured telemetry or instrument the production owner.
- If telemetry cannot initialize, the affected harness reports that failure
  explicitly instead of silently discarding signals.
- If a test consumer cannot render some telemetry, it preserves the original
  failure and reports the diagnostic degradation.

## Material constraints

- Reuse `wyrd_telemetry::init_test_capture`, the production Prometheus recorder
  pattern, `BifrostTelemetryCapture`, `OracleTelemetryCapture`, the existing
  bounded child stderr tail, and existing process lifecycle ownership where
  applicable.
- Keep test capture out of production behavior and public contracts.
- Respect the `vala-bifrost-redux` dependency boundary: its Tier-2 target may use
  test-only shared telemetry dependencies but shall not depend on
  `wyrd-testing` or `wyrd-server`.
- Use existing workspace dependencies only; do not add a new third-party
  package or feature.
- Keep stdout free for child control frames. A test consumer may write its
  bounded diagnostic stream to stderr without that output being considered
  test-authored direct debugging.
- Preserve the Bifrost telemetry authority in `architecture/bifrost-design.md`
  and the correlation, cardinality, sampling, and payload rules in
  `architecture/references/domain/telemetry-observations.md`.
- The repository check protects a live architectural boundary not enforced by
  Rust or ordinary behavioral tests; it shall remain one narrow check rather
  than a second lint framework.

## Required system boundaries and cross-boundary flow

1. A production Bifrost owner emits its existing trace, structured event, or
   metric through the normal production producer.
2. The process-global production-shaped telemetry stack receives that signal.
3. An in-process, Tier-2, or process-child test consumer retains the signal
   without changing the producer.
4. In a process child, a lifecycle-owned reporter transfers bounded incremental
   telemetry to stderr independently of the control request loop; the parent
   already drains and retains that stream.
5. A failure renderer combines the available recent telemetry with the original
   failure and child identity.
6. The developer or agent diagnoses the production path from those signals. If
   a transition is absent, its production owner gains the missing observation
   and the test verifies that real signal.
7. The repository check prevents test-local direct printing from becoming a
   competing workflow.

No public API or durable schema changes.

## Acceptance obligations and evidence classes

### AC-001 — Multi-process telemetry survives a blocked control request

A Tier-1 Oracle process-cluster journey proves that a real child production
operation emits production logs plus completed-span and metric evidence, that
the parent retains the evidence without a second control request, and that it
remains available while the child's control loop is blocked in a bounded wait.
Covers REQ-001–REQ-005.

### AC-002 — Tier-2 capture installs once

A focused Tier-2 integration test proves the integration target installs and
retains one production-shaped tracing consumer and one test-owned metric
recorder before production test behavior, without depending on server-tier or
`wyrd-testing` code. Covers REQ-001, REQ-002, and REQ-004.

### AC-003 — Direct prints fail with an actionable remedy

The repository check's bounded self-test proves a clean scoped source passes,
each prohibited Rust macro fails, and the failure directs the contributor to
captured production telemetry. The real canonical Tier-1 and Tier-2 roots pass
the same scan. Covers REQ-006.

### AC-004 — Shared workflow authority is synchronized

Repository documentation states the telemetry-first rule, the canonical
implementation skill applies it, and the skill-sync check proves the Claude
mirror is current. Covers REQ-005–REQ-007.

### AC-005 — Existing tiered behavior remains intact

The complete Tier-2 Bifrost lane and affected Tier-1 Oracle journey lane pass
without weakened tests, changed timeouts, or direct diagnostic prints. Covers
all requirements and invariants.

## Open material decisions

None.

## Planning-decision inventory

The implementation plan must fix:

- the child reporter's concrete owner, lifetime, bounded cadence, incremental
  snapshot format, shutdown behavior, and non-recursive stderr path;
- how the Tier-2 integration target installs and retains one tracing consumer
  and metric recorder before libtest execution without a server-tier dependency;
- how parent errors select and render child telemetry without masking the
  original failure;
- the exact canonical test roots and sanctioned harness-output boundary scanned
  by the repository check;
- the exact focused tests, negative self-test, and repository-native commands
  proving the change.

## Revision history

- **Revision 1 — 2026-09-02 — approved.** Created from the human-approved
  telemetry-first debugging recommendation. No unresolved material decisions.

## Material authority links

- `AGENTS.md` §§11–12 — tier definitions, verification, permanent checks, and
  completion standard.
- `architecture/agent-rules.md` — external-test ownership and gate integrity.
- `architecture/wyrd-design.md` doctrine 20 — user journeys are the primary
  test contract.
- `architecture/bifrost-design.md` §§Resource and failure invariants, Telemetry
  — closed production lifecycle telemetry and bounded diagnostic dimensions.
- `architecture/references/languages/spec-driven-development.md` — approved
  specification and evidence lifecycle.
- `architecture/references/languages/testing-workflows.md` — Tier-1/Tier-2
  topology and canonical verification lanes.
- `architecture/references/domain/telemetry-observations.md` — correlation,
  cardinality, lifecycle, sampling, and payload-safety rules.
