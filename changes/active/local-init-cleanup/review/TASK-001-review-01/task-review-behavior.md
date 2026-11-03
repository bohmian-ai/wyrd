# TASK-001 behavior review

Result: **FAIL**. The requested runtime behavior is supported by source and
focused proof. Two repository-rule proposals remain for independent validation:
an existing log-capture owner is duplicated, and several new Rust items omit
explicitly required rustdoc sections.

## Immutable subject and authority

- Base: `d7de12a74c4bdbf93e727cc2383147e34298f689`.
- Candidate: `862a69b187c81a5395bc73b872695a7bb0150330`.
- Reviewed the complete `git diff BASE...CANDIDATE`, including packet context;
  TASK-002 is not an implementation obligation here.
- Original task: candidate `changes/active/local-init-cleanup/tasks/TASK-001-local-setup-cleanup.md`.
- Approved authority: working-tree revision 1 of
  `changes/active/local-init-cleanup/spec.md`, blob
  `440649c7563dede7724b56f358b3ca1d3ecd1e80`. Its diff from candidate changes
  approval metadata/prose only. No requirement changed.
- First review; no prior acceptance or finding closure carried forward.
- Read root AGENTS, Python SDK AGENTS, `changes/spec-driven-development.md`,
  `agent_docs/index.md`, `rust.md`, `maintainer-style.md`, `agent-surfaces.md`,
  architecture authority map, operations readiness rule and local-server-bundle
  contract, and applicable security-posture tenant/secret rules.
- No nested AGENTS applies to the touched server/CLI code. CodeGraph is absent.
- Candidate source remains unchanged; HEAD was rechecked as the recorded candidate.

## Coverage and adversarial paths

| Changed surface | Source/callers inspected | Negative/recovery and reuse assessment |
|---|---|---|
| MCP Start guide | `guide.rs::response`, shared setup-prompt/guide onboarding references; candidate-wide retired-origin search outside changes | Current base preserves `/wyrd/`, `llms.txt`, `llms-full.txt`. No active retired-origin occurrence remains. Historical packet records are not treated as active onboarding. No generated content was hand-edited. |
| HTTP logging | Complete router, `build_router`, `traced`, `RequestFailures`, `request_span`, `record_response`, protected-edge composition, new router test | Only readiness-path 503 is reclassified; other 5xx and service errors delegate to the original Tower classifier. Span/status recording and middleware order remain intact. Expected unready responses remain 503. No blanket log-level change or new startup state exists. |
| Health logging | `Health`, snapshot initialization/all_ok, readiness publisher, dependency/role probes, public readiness projection/handler, server worker startup and peer status owner | Publisher compares the previous cached snapshot before replacing it. Readiness loss emits WARN, recovery emits INFO; unchanged unavailable state is quiet. Existing Postgres/storage WARN diagnostics and deadlines remain unchanged. Role supervision and readiness decisions are unchanged. Existing public report omits peer status; this pre-existing projection issue does not establish a new supported-use regression here, and peer failure remains supervised. |
| CLI setup reporting | `DevReady`, `GatewaySummary`, complete `configure_gateway`, `stored_deployments`, input/offer selection, `DevServer::run`, `ready`, shutdown/exit handling | Every no-apply path reads existing state. Failed connection/read is Unknown, successful empty read is NotConfigured. Explicit-file application still calls existing atomic apply and readback owners. Saved-client creation and authenticated tenant binding are reused. Child-exit observation covers the new awaited read; original startup timeout/exit handling remains. |
| Tenant gateway owner | `wyrd_client::Gateway::connect_with`, `deployments`, server `Gateway::deployments` and permission/tenant transaction flow | Existing redacted GET/list owner is used; only names reach summary output. No credentials, provider requests, tenancy selector, durable write, or key replacement was added. |
| Regression fixtures/tests | Full new unit bodies and new `CapturedLogs`; existing private `keys.rs::Captured`; existing public `wyrd_testing::logs::LogCapture`; Cargo dev-dependency and integration consumers | New capture duplicates an available public fixture; see BEH-001. Existing public owner already implements current-thread DEBUG-and-above, no-ANSI capture with lifetime guard and text access. |
| CLI/Python journey changes | Changed CLI first-run, saved-directory, managed-secret restart tests, Python guided setup assertion and surrounding interactive setup flow | Existing persistence, inference, secret-hiding, output and process-cleanup assertions remain. New assertions target empty JSON state, readiness logs and retained deployment names. No test weakening, retry/timeout increase, skip addition or failure-clearing assertion deletion appears in the diff. |

The classifier is a small implementation of the installed Tower extension
point; it does not introduce a transport or health owner. `GatewaySummary` is a
closed local output value that keeps status/detail and legacy JSON string type
consistent. The deployment read delegates to the existing client instead of
creating alternate discovery. Those additions earn their role. The capture
fixture does not, because its complete workflow already has an available owner.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-008 / AC-006: current onboarding origin and machine-index base | `mcp/guide.rs:124`; candidate-wide `git grep bohmian-ai.github.io` outside changes returned no matches | Static inspection; no manufactured URL test | PASS |
| REQ-009 / AC-007: startup readiness remains 503 without ERROR per response | `http/router.rs:241-264`; unchanged `health::readyz` and snapshot readiness predicate | Router unit checks readiness 503, DEBUG visibility and ordinary 503 ERROR control; independently rerun and passed | PASS |
| REQ-009 / INV-004: genuine faults, readiness loss, ordinary errors and CLI failure paths retain diagnostics | `health/mod.rs:197-219`, unchanged dependency WARN branches; classifier delegation; unchanged CLI `ready`/`exited` | Health-transition unit proves INFO/WARN and failed-check reason; independently rerun and passed. Supplied first-run journey checks startup diagnostic log | PASS |
| REQ-010 / AC-008: no-apply restart reports stored tenant deployments | `init.rs:1675-1682`, `1713-1721`; existing authenticated Gateway handle | Supplied managed-secret restart journey asserts existing `local-mock` and still performs inference, checks retained credentials and secret hiding | PASS |
| REQ-010: successful empty read differs from failed read | `GatewaySummary::read/status/detail` at `init.rs:352-381`; read result is passed without converting errors to empty vectors | Gateway-summary unit proves nonempty/empty/error and JSON; independently rerun and passed. Supplied saved-settings journey asserts empty text | PASS |
| REQ-010: explicit file and first-run offer behavior preserved | Same resolved input, same provider offer, same atomic apply owner and same applied-deployment readback; only outcome value changes | Supplied Python guided/file gateway journeys; source inspection confirms no provider call added | PASS |
| REQ-011: labelled HTTP/gRPC/UI, storage/Bifrost, settings/credential location, gateway checklist | `DevReady::print_text` at `init.rs:310-330` | Static output-format inspection; supplied CLI/Python assertions exercise configured/not-configured checklist | PASS |
| REQ-011: JSON fields/types and a single readiness document; MCP/log/shutdown retained | `DevReady` still derives Serialize; custom gateway serialization emits one string; unchanged `DevServer::run` JSON branch, MCP report handling and shutdown text | Supplied JSON first-run journey plus independently passing gateway serialization assertion | PASS |
| INV-005: no credential replacement/disclosure, tenant-scoped reads, no key-validity claims | Shared authenticated client and redacted list; summary carries deployment names only; unchanged settings save and key proof | Supplied managed-secret persistence/secret-hiding journey; source tracing | PASS |
| Task boundary: no provider validation, new state/dependency/schema/health framework/card logging | Complete production diff is local URL/logging/reporting change; manifests/contracts unchanged | Complete diff inspection | PASS |
| Mandatory reuse: existing owner before adding fixture/machinery | New `CapturedLogs` duplicates existing public LogCapture already available through dev dependency | BEH-001 below | FAIL |
| Mandatory changed-item rustdoc, including error/panic sections | New serialize/test/capture methods omit required sections | BEH-002 below | FAIL |

## Proposed findings

### BEH-001 — DRIFT: duplicate the available thread-local log capture

- **Obligation:** root AGENTS Code requires finding/reusing the existing owner
  before adding a fixture/helper; the task specifically requires reusing
  existing telemetry fixtures, without a new harness. Task-review reuse audit
  requires comparing complete workflows, not merely leaf calls.
- **Exact location:** `crates/wyrd/wyrd-server/src/test_support.rs:288-332`;
  consumers `http/router.rs:402-403` and `components/health/mod.rs:678-679`.
- **Existing owner:** `crates/wyrd/wyrd-testing/src/logs.rs:18-55`,
  `LogCapture::install` and `text`. This file exists at both base and candidate.
  `wyrd-server/Cargo.toml:137` already declares `wyrd-testing` as a dev dependency.
  Existing server integration tests use this public fixture, e.g.
  `tests/integration/pg_operator_delivery.rs:43,632`.
- **Reachable path/evidence:** both newly required logging tests create
  CapturedLogs, install a thread-default DEBUG/no-ANSI subscriber, execute their
  observed branch on that thread, and inspect `text`. The public LogCapture
  supplies that entire workflow with the same semantics. Neither test needs
  cloning the capture or a separately scoped guard. Its public type carries no
  AppState or server-owned test type, so the test-support module's warning
  about cycling through the Postgres/AppState harness does not explain this
  duplicate sink. Existing private `operators/keys.rs::Captured` is another
  inspected sibling, not the recommended owner to expand.
- **Consequence/materiality:** the patch adds and owns another complete log
  capture/subscriber/writer stack for behavior the repository already exposes;
  log-capture fixes and contract changes now have an unnecessary additional
  implementation to keep consistent. This is a concrete reuse-rule breach,
  not a preference for a different logger or a line-count threshold.
- **Smallest testable correction:** delete the newly added CapturedLogs block
  and have its two new consumers use `wyrd_testing::logs::LogCapture::install()`
  and the existing `text()`. Keep the same assertions and current-thread
  runtime. Do not change dependencies, harnesses, readiness code or unrelated
  private sinks. Correction burden is two consumer substitutions and deletion;
  no new abstraction or feature is needed.
- **Closure proof:** the existing two logging units continue to pass with
  their complete readiness/ordinary-error and transition assertions intact.

### BEH-002 — VIOLATION: required rustdoc sections are missing on new items

- **Obligation:** root AGENTS explicitly blocks merging changed Rust items
  without required rustdoc, including `# Errors` on fallible items and
  `# Panics` when they can panic. `agent_docs/rust.md` explicitly includes
  private helpers and tests. This mandatory gate is independent of runtime
  test results.
- **Exact locations/evidence:**
  - `crates/wyrd/wyrd-cli/src/init.rs:386`,
    `GatewaySummary::serialize`: returns the generic serializer's fallible
    result but documents no `# Errors`.
  - `crates/wyrd/wyrd-cli/src/init.rs:1913`,
    `gateway_summary_distinguishes_empty_and_failed_reads`: uses `expect` and
    assertions but has no `# Panics`.
  - `crates/wyrd/wyrd-server/src/components/health/mod.rs:677`,
    `readiness_transitions_are_logged`: assertions panic when the logging
    contract fails, with no `# Panics`.
  - `crates/wyrd/wyrd-server/src/test_support.rs:314,320`, newly added writer
    methods omit `# Errors`; `write` also calls `expect` on the sink lock with
    no `# Panics`. Deleting this fixture under BEH-001 closes these locations.
- **Reachable path/consequence:** JSON readiness serialization delegates to
  an arbitrary Serializer whose error can propagate. The required regression
  tests execute their assertions. Writer calls execute under the test
  subscriber. These changed items therefore fail the stated documentation
  gate; passing behavior tests cannot establish compliance.
- **Materiality:** an explicit mandatory changed-item rustdoc gate, rather
  than an optional style suggestion. The newly added router test already
  documents its panic behavior and is a nearby compliant pattern.
- **Smallest testable correction:** document serializer-error propagation and
  the two tests' failure conditions under the required headings. Delete the
  redundant fixture as proposed above; if retained after validation, its
  writer methods also need accurate error/panic sections. Do not edit unrelated
  items or introduce behavior changes. This is bounded documentation work in
  the same remediation, not a separate style-only round.
- **Closure proof:** inspect required headings against the changed method
  bodies; no new runtime test is warranted for prose.

## Verification and limits

The orchestrator's `verification.md` independently records nextest run
`6ceabc3a-b77d-49b0-9c3d-652331f87d71`: all three changed units selected and
passed, and both worktree/range diff whitespace checks passed. I inspected the
actual unit bodies and supplied journey assertions; I ran no Cargo/test lane.

The committed task records the CLI Postgres journeys, Python local-development
journeys, format and lint commands as passing implementation evidence. These
are supplied results, not independent reruns in this review. No source proves
a runtime failure requiring another journey or broader execution. The remaining
proposals need independent source/materiality validation; neither requires
expanding product scope or adding a test harness.

Skipped broad suites, provider calls and benchmarks. No new runtime defect was
established; the outstanding risks are the proposed reuse and rustdoc gate breaches.
