---
id: TASK-CHANGE-R3
kind: remediation
status: ready
spec: SPEC-storage-upload-consolidation
spec_revision: 2
requirements: [INV-003, INV-005]
remediates: [FIND-CHANGE-004, FIND-CHANGE-006, FIND-CHANGE-008, FIND-CHANGE-009, FIND-CHANGE-010, FIND-CHANGE-011]
depends_on: []
---

# Repair integrated test observation boundaries

Governing authorities: root AGENTS testing gates, canonical verification payload owners in architecture, and [storage revision 2](../../spec.md) canonical evidence and unchanged sibling-producer obligations. These affected tests/helpers are inherited unchanged in the actual integrated range; they are not new storage-authored behavior or a new product capability. No new product specification is needed for correcting private test observers.

Reviewed combined base: `3899b0a35ec2eede7bf294db0cf66b27defa1498`; immutable target: `462ebee97c81772103054f7d090053ac7054bafa`.
Review/findings/remediation only: the implementor owns all source changes.
Every change below closes a mandatory repository gate. A red test alone is
not evidence of a supported-use production defect. No benchmark measurement,
scaling optimization or excluded benchmark remediation is authorized here.
Route this bounded task directly to `$wyrd-implement`; retain exact durable,
tenancy, permission and assertion contracts. See [change review](change-review.md)
for the complete acceptance matrix and fresh verification evidence.

## Put SDK pressure after public fixture setup

FIND-CHANGE-004 (inherited SDK subset), INCORRECT / mandatory red gate:
Python `test_verify_in_real_time.py:1042` fails while declaring `support.yaml`,
before pressure assertions. Its capacity-six process leaves only two
unreserved items; audit/span traffic can shed the required envelope Object,
and normal Card compensation marks the declaration Failed. The full Python
lane records `RegistryArtifactVerifyFailed: registration no longer pending`.
Rust and TypeScript use the identical capacity-six setup and intended
four-reserved-slot slow-judgment geometry. Batch that equivalent source-proven
fixture assumption here, preserving each language's public journey.

Required outcome: let public Card/credential/bundle setup finish with normal
unreserved staging capacity, then establish exactly the same four-slot
reserved capacity for the existing two slow judgments, neighbor progress,
refusals and recovery. Keep actual slow judgments exercising production
reservations. Do not merely enlarge capacity and lose saturation, mutate
production limits, add sleeps, bypass public declaration or add a second
queue/admission owner.

Rust/MCP can retain the existing public outbox Reservation directly. Existing
Python/TypeScript harnesses expose only constructor capacity and the existing
flush/baseline/schedule controls; they expose no late Reservation or restart
control. A larger small fixture capacity still couples mandatory setup to
ambient unreserved telemetry, while starting normal capacity and issuing
hundreds of thousands of slow calls is not a bounded pressure proof.
If a private test-only projection is needed, the minimum supported owner is
an unspent Reservation of the same process outbox's reserved share minus the
four slots this existing story exercises, armed only after setup settles.
Derive the share from actual immutable server config and public RESERVED_SHARE;
release it normally and on exceptional harness teardown. This leaves normal
unreserved audit/inventory traffic intact. The shared Rust harness already
owns server/config/lifecycle; keep any binding a thin projection there, with
lifetime/type details implementation-owned. Do not add a production API,
public SDK pressure capability, dependency, counter, timing seam or arbitrary
capacity constant. Reconcile the current descriptive “exactly three controls”
rustdoc/stub prose if the private test contract grows; no architecture or
AGENTS rule locks that prose. Prove new Python/Node lifetime behavior in their
own harness language, including exceptional cleanup, rather than emulating it
in Rust. This is justified only to express the existing pressure story after
its public fixture has become ready.

## Observe the producer before installing a fault or reading retained state

FIND-CHANGE-006, INCORRECT / mandatory red gates: three journeys place their
observer/fault boundary before the relevant producer settles:

- `crates/wyrd/wyrd-testing/tests/bifrost/scribe/telemetry.rs:653`,
  `scribe_hot_path_telemetry_reconciles`: the audited `sorted_values` query
  precedes the raw Scribe freeze/age-tick proof. Its late audit input can enter
  a fresh active memtable after the freeze; the test then mistakes that row
  for the frozen table failing publication. Settle the process outbox through
  the existing helper before freezing the measured generation. Preserve the
  exact writable/frozen/backlog and age-publication assertions.
- `.../scribe/write_read.rs:1196`,
  `acknowledged_rows_survive_stage_pressure_and_restart`: after the audited
  readability check, occupying the shared root precedes `flush_bifrost`, whose
  first operation settles the process outbox. The trace repeatedly refuses
  the outstanding audit write and expires after 60 seconds, before testing
  the intended retained-generation stage refusal. Use the existing
  `server.settle_bifrost_outbox` before occupying the root; do not call a
  whole `flush_bifrost` there, which would publish the measured generation
  before the intended fault. Keep the full stage refusal, exact rows,
  acknowledged WAL, release, publication, restart and resend proof.
- `.../server/query.rs:877`,
  `generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup`:
  `prove_scheduled_analytical_completion` deliberately submits through the
  ingress to the remote Oracle, which owns the permission decision. Its
  ingress-only retained-audit helper cannot settle that remote producer.
  Use the existing cluster `await_audit_retained` boundary before reading
  exact terminal audit counts/details, including equivalent later calls in
  this journey. Keep the exact one-decision, chain, cancellation and cleanup
  assertions. Do not alter audit durability or permission semantics.

These are observer corrections using existing settlement owners. Do not add
sleeps, extend deadlines, synthesize load, change production admission, weaken
assertions or settle an outbox after installing a fault that prevents it.

## Read retained verdict details from the canonical output owner

FIND-CHANGE-008, INCORRECT / mandatory red gate:
`sdks/wyrd-sdk-rust/tests/integration/verifier_building_blocks.rs:1477–1500`,
`retained`, queries `to_json(outcome)` from `verification.results`; that table
owns `verdict`, and custom verdict/step payloads belong to
`verification.execution_outputs.output`. The full Rust SDK journey fails
`custom_verdict_data_survives_readback` with typed QueryInvalidSql at :1493.

Update the existing retained reader to the canonical `execution_outputs` row
and `output` field with matching SQL and local projection semantics. Reuse
or rename the existing projection; local representation remains implementor-owned. `StoredOutput` contains
requirements and steps; do not decode it as a full `VerificationOutcome` or
fabricate its absent verdict/evidence. Match the existing Python/TypeScript
canonical query and compare both exact retained requirements/custom data and retained steps
with the corresponding waited outcome fields, preserving the original
whole-payload assertion’s coverage. Preserve every bound case and the existing separate
full `status.result == waited` assertion for each case. Do not add aliases, new
schema fields, duplicate payloads or change SDK production readback.

## Count accepted group execution inputs as well as submitted turns

FIND-CHANGE-009, INCORRECT / mandatory red gate:
`sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:3259`,
`verification_staging_refuses_and_recovers`, sees seven input rows against
six (`4 + offered`, offered=2). Its tenant-wide count includes the group
execution itself: `verification/groups.rs::record` publishes one
`InputRecord::Group` for every newly enqueued group, in addition to the
accepted grouped turn. There are two direct burst inputs, `offered` normal
inputs, one recovered input, one accepted grouped turn and one group input:
exactly `5 + offered`. This is canonical payload ownership, not refused
requests leaking input rows.

Batch the same source-proven omission at Python
`tests/integration/test_verify_in_real_time.py:1110` and TypeScript
`wyrd/tests/integration/verify-in-real-time.test.ts:1115`. Keep an exact total
and identify the one generated group execution input by the returned group
execution ID/origin so the extra row cannot hide a refused request. Preserve
typed 429/retry details, neighbor progress, in-flight saturation, accepted
recovery and the exact one-turn group evidence. Do not raise outbox capacity,
change reservation semantics, or merely relax the count to a bound.

## Measure canonical source reads after scheduling has settled

FIND-CHANGE-010, INCORRECT / mandatory red gate:
`crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:7575`,
`group_lookup_preserves_source_occurrences`, asserts exactly 14 Oracle query
profiles across the concurrent runtime. The captured work is 15. At
14:38:29.980067 the short-group execution is deferred with
`evidence_unavailable`: its ACKed group input is not visible yet. The same
execution legitimately reads its input again when reclaimed. Deferred claims
spend no execution attempt; this is the documented ACK/query distinction,
not a duplicate group/source occurrence or an input-owner regression.

Keep concurrent runtime work as informational evidence; an exact global 14
assumes stronger visibility timing than the approved contract. Preserve the
exact source-read shape through the existing settled readback owner instead:
after `settle_all` has stopped the runtime, settle via existing
`server.flush_bifrost` and `cluster.refresh_oracle_snapshots`. Wrap the
existing per-execution SDK `Execution.status` read in `ScanEvidence::of`.
`InputResolver::result` makes one exact canonical summary/output/input join;
for a group, `evidence`/`slots` adds one canonical source-input query and one
selected-output query. Require exactly three reads per settled group and one
per ordinary turn. Preserve all six completed executions, both group
verdicts and exact ordered per-slot long/short source occurrence arrays.
This tests canonical read shape without coupling correctness to a deferred
claim's scheduling race. Do not change 14 to 15, add a loose allowance, add
metrics/test hooks or force publication before production handoff.

## Keep the terminal security observer valid after its session leader exits

FIND-CHANGE-011, INCORRECT / mandatory red gate:
`sdks/wyrd-sdk-python/tests/integration/test_local_development.py:1016`,
`interrupt_hidden_question`, makes the CLI the PTY controlling-session leader,
then queries its slave fd after `communicate` reaps it. Darwin hangs up that
slave observer: the gate gets ENOTTY25 even though exit130 passed. This
assertion never observes the post-cancellation ECHO state.

Use the existing retained PTY master fd for the terminal-attribute observation
that must survive session-leader exit. No new supervisor/fixture is needed.
Read-only Darwin arm64 and Linux probes establish both positive and negative
controls: master reports ECHO enabled when the child restores it and ECHO disabled
when it deliberately leaves it disabled. Thus the master detects an actual
restoration failure, rather than relying on automatic hangup reset. Darwin's
slave fails after exit; Linux's stays queryable. Keep actual ECHO-off while
concealed input waits, ECHO-on after interruption, exit130 for SIGINT/SIGTERM,
no secret disclosure, and no installation/configuration/credential side effects.
Do not suppress ENOTTY, skip a platform/signal or weaken the security proof.
The CLI EchoGuard restoration owner needs no source change from this evidence.

## Acceptance and proof

Preserve each exact property described above and run these focused selectors through existing lifecycle owners:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_testing=debug,wyrd_server=debug,vala_bifrost_redux=debug cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=telemetry::scribe_hot_path_telemetry_reconciles) | test(=write_read::acknowledged_rows_survive_stage_pressure_and_restart)'"
```

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_testing=debug,wyrd_server=debug,vala_bifrost_redux=debug cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=query::generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup) | test(=verification_runtime::group_lookup_preserves_source_occurrences)'"
```

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_testing=debug,wyrd_server=debug,vala_bifrost_redux=debug cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::custom_verdict_data_survives_readback) | test(=verify_in_real_time::verification_staging_refuses_and_recovers)'"
```

Run the corresponding exact Python and TypeScript staging stories after their
edits. If a private binding projection changes, rebuild it through its
existing owning tasks (`mise run py:setup`, `mise run ts:build`, and
`mise run ts:build:testing`) before testing, and run applicable Python typecheck,
TypeScript declaration/typecheck and binding checks. Do not test a stale native
artifact. Preserve authored Python stubs and the owning TypeScript generation
workflow. No new bindings are presumed when an existing-owner solution suffices:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && WYRD_LOG=info,wyrd_server=debug uv run python -m pytest -q tests/integration/test_verify_in_real_time.py::test_verification_staging_refuses_and_recovers"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && WYRD_LOG=info,wyrd_server=debug pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t '^verification_staging_refuses_and_recovers$'"
```

After the exact selectors pass, run `mise run fmt:check`, `mise run lints`
and `git diff --check`. For Python/TypeScript test edits, run their applicable
format/lint tasks too. No repeated broad aggregate, complete language suite or
benchmark is required for these bounded corrections. Record the resulting
immutable candidate and exact diff for closure review.

For the PTY observer change, run the exact existing guided setup selector
using the owning task's built CLI/server/UI artifacts:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_installed_cli_guided_setup
```

The failed aggregate stopped this owning local-dev task before its subsequent
installed-CLI commands. Those commands are an explicit remaining verification
limit, not passing coverage; reconcile their first execution once the initial
blocker is corrected. Do not repeat already completed unrelated full suites.
