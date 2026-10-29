---
id: TASK-CHANGE-R1
kind: remediation
status: ready
spec: SPEC-storage-upload-consolidation
spec_revision: 2
requirements: [REQ-015, REQ-016, INV-002, AC-007]
remediates: [FIND-CHANGE-001, FIND-CHANGE-003, FIND-CHANGE-004]
depends_on: []
---

# Close storage-specific review gates

Approved [storage revision 2](../../spec.md); original [TASK-001](../../tasks/TASK-001-object-inventory.md), [TASK-002](../../tasks/TASK-002-shared-file-uploads.md) and [TASK-003](../../tasks/TASK-003-canonical-evidence.md). TASK-002 owns this packet’s upload handlers and first MCP fixture.

Reviewed combined base: `3899b0a35ec2eede7bf294db0cf66b27defa1498`; immutable target: `462ebee97c81772103054f7d090053ac7054bafa`.
Review/findings/remediation only: the implementor owns all source changes.
Every change below closes a mandatory repository gate. A red test alone is
not evidence of a supported-use production defect. No benchmark measurement,
scaling optimization or excluded benchmark remediation is authorized here.
Route this bounded task directly to `$wyrd-implement`; retain exact durable,
tenancy, permission and assertion contracts. See [change review](change-review.md)
for the complete acceptance matrix and fresh verification evidence.

## Diagnosis

FIND-CHANGE-001, VIOLATION: root `AGENTS.md` requires server handlers to carry
`#[tracing::instrument]` with scrubbed arguments. In
`crates/wyrd/wyrd-server/src/components/storage/routes.rs`, `part_url` (line
161), `abort` (338), and `local_blob` (403) have no handler instrumentation.
TASK-002 materially changes all three: the first two now await persisted-owner
authorization, and local transfer also reads and bounds the body against the
persisted session. Errors at those new boundaries precede the instrumented
storage service call and therefore lack the required operation span. The
generic HTTP request span and leaf service spans do not satisfy this explicit
handler rule. TASK-001 corrected its two affected sibling handlers,
`complete` and `download_init`; TASK-002's additional siblings were missed.

## Required correction

Apply the existing scrubbed handler instrumentation pattern to these three
handlers. Retain safe operation identification while excluding AppState,
Caller and credentials, request headers, and upload bytes from automatic
argument recording. Reuse the adjacent `complete`/`download_init` pattern;
three conventional attributes are sufficient. No helper, tracing framework,
metric, additional authorization, or downstream guard is needed.

Preserve persisted-owner permission selection, audit staging, local body
limits and validation, response schemas, transfer/completion semantics, and
tenant isolation. Do not sweep untouched handlers or rewrite unrelated documentation,
contracts, storage state, retry policy, or benchmarks.

## Document the uploaded MCP fixture

FIND-CHANGE-003, VIOLATION: TASK-002 adds function-local `const DOCUMENT` at
`crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:1224` inside
`file_inputs_use_shared_uploads`, with no rustdoc. Root AGENTS and
`agent_docs/rust.md` explicitly include private and test items. TASK-003's
accepted remediation documents the identical fixture in the later readback
journey; this first changed item remains undocumented. Add the equivalent
one-line purpose doc here, preserving bytes, declaration, digest and tests.
Batch this explicit gate correction here; no documentation sweep or runtime
test is warranted.

## Keep the MCP refusal check small

FIND-CHANGE-004, INCORRECT / mandatory red gate: the existing
`verification::pg_tests::staging_pressure_is_retryable` test starts a server
with a one-record outbox. Card registration now needs an acknowledged inventory
record through that same outbox. Setup fails before the test calls either MCP
verification tool, so the intended MCP error check never runs.

The queue's capacity, refusal and recovery behavior already has lower-level
coverage in `crates/shared/wyrd-runtime/src/outbox.rs` and the HTTP recovery
journey in `crates/wyrd/wyrd-server/tests/integration/pg_verification_routes.rs`.
This MCP test only needs to prove the remaining public contract: when verification
admission refuses a call, both execute and enqueue expose
`WYRD_VERIFICATION_429_STAGING_FULL` and its `retry_after_seconds` hint.
Preserve its existing assertion that refused calls add no execution-input rows.

Remove the one-record startup setup. Complete the ordinary Card, credential and
MCP discovery setup successfully, then use the existing outbox reservation API
locally in this test to make the verification admission branch refuse. The
existing Reservation guard can hold the reservable share until the two MCP
calls finish; release it before the final flush/readback and server shutdown.
Use the existing runtime-owned capacity/share values rather than inventing a
new setting or guessing occupancy.

This is deterministic error injection for a small MCP contract check. It does
not need to simulate production traffic, fill the real queue with records,
retest queue recovery, or prove a particular queue size. No new harness,
projection, configuration option, dependency, load generator, sleep or polling
loop is required or authorized by this packet. Keep the correction local and
preserve the existing error and no-input assertions.

The current MCP test already has the needed interfaces and dependencies:
`WyrdTestServer::state()`, `state.bifrost.scribe_outbox()`, `Outbox::reserve`,
Reservation's release-on-Drop and `wyrd_runtime::outbox::RESERVED_SHARE`.
The queue implementation and production Card/verification behavior need no
change. SDK fixture issues remain separately scoped in R3; they do not expand
this MCP correction.

## Acceptance and proof

Inspect the three scrubbed handler attributes and the one local fixture rustdoc directly; comments/attributes do not need invented runtime tests. The small MCP contract test must reach both tool calls, assert the exact refusal code and retry hint, and retain its existing zero-inputs readback. The one-record setup and any particular occupancy are not acceptance criteria:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_testing=debug,wyrd_server=debug,vala_bifrost_redux=debug cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::staging_pressure_is_retryable)'"
```

After the exact selectors pass, run `mise run fmt:check`, `mise run lints`
and `git diff --check`. No repeated broad aggregate, complete language suite or
benchmark is required for these bounded corrections. Record the resulting
immutable candidate and exact diff for closure review.

## Implementation evidence

Reuse: handler attributes copy the adjacent `complete`/`download_init` pattern; the MCP check reuses `WyrdTestServer::state()`, `ScribeOutbox::reserve`, Reservation release-on-Drop, `RESERVED_SHARE`, and the default `WyrdServerConfig` Scribe capacity the test server starts with. No production, queue, harness, or configuration change.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-CHANGE-001: `part_url`, `abort`, `local_blob` carry scrubbed handler spans | `crates/wyrd/wyrd-server/src/components/storage/routes.rs` `#[tracing::instrument]` skipping `state`, `caller`, and `headers`/`body`, with `operation` fields | direct inspection; `mise run lints` | PASS |
| FIND-CHANGE-003: first MCP `DOCUMENT` fixture documented | `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs` `file_inputs_use_shared_uploads` one-line rustdoc, bytes unchanged | direct inspection | PASS |
| FIND-CHANGE-004: MCP refusal check reaches both tools, asserts code and retry hint, keeps zero-inputs readback | same file, `staging_pressure_is_retryable`: ordinary startup, settle, hold whole reservable share, drop before flush/readback | exact selector below: 1 passed | PASS |

Commands: the packet's exact `staging_pressure_is_retryable` journey command (PASS, 2.7s); `mise run fmt:check` (PASS after `mise run fmt`); `mise run lints` (PASS); `git diff --check` (PASS). Non-goals untouched: no other handler, docs, queue, retry policy, benchmark, or SDK fixture (R3) changes. Diff: two files, +27/−9, uncommitted on top of `462ebee97`.
