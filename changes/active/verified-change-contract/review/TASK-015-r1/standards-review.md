# TASK-015 Repository Standards Review

## Immutable subject and scope

- Candidate: `9b560d00569656ee7fdde19a9c76fa55c8d55fbb` (the current `HEAD` when this review began and immediately before writing this report).
- Task change: commit `9b560d005`, whose parent is `3f8767a5f`.
- Merge resolution: only the combined-resolution hunks of merge commit `c5527627a` with parents `ca2950856` and `52e1144b5`.
- Excluded as directed: merge `3f8767a5f`, previously passed TASK-013/TASK-014, the audit-outbox implementation and benchmark, and generic-outbox internals not changed by TASK-015.
- Review lens: repository standards only. This report does not repeat task-acceptance or Ponytail review.

CodeGraph was not used because this checkout has no `.codegraph/` directory. The changed-symbol map was established from the immutable Git diffs, combined merge hunks, current owners, callers, and tests.

## Navigation map

| Changed surface | Owner and consumers | Relevant proof |
|---|---|---|
| `wyrd-server/src/verification/observations.rs` | `ObservationRunSink` adapts `VerifierRunQueue` to `wyrd_runtime::outbox::Outbox`; `ObservationEnqueue` is Gate's post-ack hook. | `pg_verification_runtime::observation_outbox_retains_through_an_outage_and_flushes_at_shutdown`; continuous Eval journeys. |
| `wyrd-server/src/boot/mod.rs` | Composition root constructs the Eval outbox, gives Gate a narrow `ObservationAck` adapter, and retains the outbox in `Bifrost`. | Server journey lane and shutdown path in `app/server.rs:873-878`. |
| `pg_verification_runtime.rs` | Postgres-backed supporting integration proof for retention, idempotent recovery, shutdown drain, and deadline loss reporting. | Focused test recorded in the task plus `test:sql`. |
| `wyrd-testing/.../eval_verification.rs` | Real Rust SDK → server → Scribe/Gate → outbox → Postgres journeys for continuous Eval, outage recovery, replay suppression, and frozen input identity. | `test:bifrost:journey:server`. |
| `wyrd-sdk-rust/tests/observe_run.rs` | Rust journey fixture arithmetic only; `u32::from(u16)` removes an unnecessary fallible conversion. | Formatting/lints and existing journey coverage. |
| `c5527627a` combined Gate/boot/state/auth-test resolution | Keeps `AuditOutbox` as Gate's process sink and combines it with the existing Eval outbox; removes the `AuditUnavailable` behavior while keeping verification result writes and reads tokenless. | Combined diff plus comparisons against both parents; current Gate, boot, state, and issuer source. |

## Authority coverage

| Changed surface | Applicable repository authority | Coverage result |
|---|---|---|
| All Rust production and test edits | `AGENTS.md` §§1-6, 9-12, 15-16; `architecture/agent-rules.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/maintainer-style.md` | Covered: ownership, typed tenant identity, async/IO boundary, transaction ownership, rustdoc, test placement, and verification evidence inspected. |
| Eval post-ack run-request path | `architecture/wyrd-design.md` Doctrine #20 and §§Observation identity, Verifier/Eval, Trigger; `architecture/wyrd-doctrine.mdx`; `architecture/references/domain/evaluation.md`; approved spec revision 60 | Covered: the sink remains server-owned, runs follow Scribe acknowledgement, replay suppression retains first-commit identity, and continuous Eval remains a real user journey. |
| Outbox durability/retry/shutdown | `architecture/bifrost-design.md` §§Durability and visibility, Resource and failure invariants; `architecture/references/domain/analytical-operations-reliability.md`; `architecture/references/domain/vala-architecture.md` | Covered under the more specific active design and approved requirement: this Eval outbox intentionally has no count limit, retries unavailable Postgres without dropping, and drains on graceful shutdown. The generic implementation itself was not reopened. |
| Server/Vala composition boundary | `AGENTS.md` §§2-3, 9; `architecture/references/architecture/patterns.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/domain/vala-architecture.md` | Covered: Gate remains a Vala engine component; `wyrd-server` owns composition/lifecycle and the SQL-backed run-request adapter. |
| Eval journey changes | `AGENTS.md` §§11-12; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/wyrd-design.md` Doctrine #20 | Covered: both changed journeys and the separate integrated outage/recovery journey were read end to end; the deleted pre-phase is redundant with, and semantically superseded by, revision-60 coverage. |
| Merge resolution: audit staging and error removal | `AGENTS.md` §2 audit decisions; `architecture/agent-rules.md` audit rules; `architecture/wyrd-security-posture.md` §§Security principles, Authorization and policy, Audit integrity and privacy; `architecture/references/languages/agent-harness.md`; `architecture/bifrost-design.md` §§Read audit and terminal contract, Resource and failure invariants | Covered: the combined Gate hunk stages every reached allow/deny decision on `AuditOutbox` without awaiting its commit and uses the ordinary internal composition error when the required sink is absent. |
| Merge resolution: SYSTEM-token removal | `architecture/wyrd-design.md` Doctrine #18 and Runtime identity; `architecture/wyrd-security-posture.md` Principal lifecycle; approved revision-59/60 authority carried by the task | Covered: the merge retained the first parent's tokenless internal result/capture path, removed the second parent's system-token issuer/tests and Gate token path, and kept public result-table writes reserved. Pre-existing, out-of-scope authority prose was not reopened. |
| Verification commands and evidence | `AGENTS.md` §§11-12; `architecture/references/languages/testing-workflows.md`; `mise.toml` | Covered: named test evidence uses exact nextest selectors through `mise`; broader recorded lanes match the touched SQL/server/Bifrost journey surfaces. No Cargo-backed lane was rerun in this shared checkout. |

## Applicable rule verdicts

| Rule | Source evidence | Result |
|---|---|---|
| Stateful workflows have a clear concrete owner and reuse existing mechanisms. | `ObservationRunSink` owns `WyrdPostgres` and `VerifierRunQueue`; `ObservationEnqueue` owns only the Gate adapter; queueing/retry/shutdown remain on `Outbox` (`observations.rs:31-103`). The hand-written writer is deleted. | PASS |
| Tenant SQL uses the sanctioned owner and transaction lifecycle. | The sink opens `WyrdPostgres::tenant_conn(tenant)`, calls the tenant-scoped queue API, and commits the transaction it opened (`observations.rs:76-87`). It neither accepts a raw pool nor commits a caller-supplied `TenantConn`. | PASS |
| Async exists only at real IO boundaries. | `ObservationRunSink::write` awaits tenant connection, SQL insert, and commit (`observations.rs:76-87`); `ObservationEnqueue::acknowledged` remains synchronous and stages without awaiting (`observations.rs:105-136`). | PASS |
| New/materially modified Rust items have substantive rustdoc, including error/panic and durable retry behavior where relevant. | Module, constant, alias, structs, fields, constructor, sink write, adapter, and hook are documented (`observations.rs:1-15,27-49,67-75,90-110`). The implemented associated items inherit their semantic contract from the fully documented `OutboxSink` trait and the `write` implementation adds sink-specific idempotency and errors. | PASS |
| Durable derived work follows acknowledgement without delaying it, retains unavailable-Postgres work, and drains at shutdown. | Gate's hook contract is synchronous (`gate/mod.rs:299-316`); boot composes the outbox and hook (`boot/mod.rs:1104-1120`); shutdown drains it only after Gate/Scribe close (`app/server.rs:873-878`); the integration test covers outage, recovery, repeat idempotency, drain, and deadline loss (`pg_verification_runtime.rs:2411-2488`). | PASS |
| User-facing behavior remains proved at the user-journey tier. | `integrated_enqueue_outage_preserves_ack_and_recovers` drives the real ingest path through forced post-ACK SQL failure and recovery (`eval_verification.rs:1181-1315`). The terminal matrix still proves the complete continuous Eval result/error workflow (`eval_verification.rs:640-974`). | PASS |
| Tests are not deleted or weakened merely to clear a failure. | The removed terminal-matrix pre-phase asserted permanent refusal/drop behavior superseded by revision 60. The same required negative path is proved more directly by the integrated outage/recovery journey and Postgres integration test. The task records trace-based diagnosis rather than changing a timeout, skip, or assertion without cause. | PASS |
| Replay proof remains deterministic and detects an incorrectly staged replay. | With the run table locked, Gate stages before acknowledgement returns; `pending() == 2` therefore represents exactly original plus sentinel, while either replay staging would raise the count (`eval_verification.rs:1009-1124`). This replaces an implementation-shaped blocked-backend count with the outbox's exact observable state. | PASS |
| A permanently failing item blocking only its tenant is a task finding only when approved authority requires another behavior. | REQ-077 requires retention/retry for slow or unavailable Postgres and ordered idempotent tenant batches; it does not define terminal classification or dead-letter behavior for a permanently invalid item. The task does not change the generic outbox. | PASS — no task finding |
| Audit decisions use the one non-blocking process outbox; no audit-unavailable public behavior remains. | Gate's production `GateAudit` is `AuditOutbox`, which stages the event synchronously (`gate/mod.rs:255-296`); boot injects the process outbox (`boot/mod.rs:1114-1119`); missing composition maps to internal error, not `AuditUnavailable` (`gate/mod.rs:490-518`). The old tonic enum name is reserved rather than active. | PASS |
| Merge resolution preserves the tokenless internal verification-result path. | Comparing `c5527627a` against both parents shows the merged Gate removed the system-token/native-frame authorization path and reserves result tables from every public write; issuer-side system-token tests from the audit-outbox parent were not retained. Current design and runtime use the internal capture writer and in-process scoped reads. | PASS |
| No unrelated contract, generated artifact, dependency, Python, or TypeScript surface changed in TASK-015. | `3f8767a5f..9b560d005` changes only the task record, two server source files, two Rust test surfaces, and one Rust SDK test conversion. | PASS |

## Material repository-rule findings

None.

The two user-directed Eval journey edits do not weaken a revision-60 proof. The permanent-refusal pre-phase encoded the superseded drop behavior and is replaced by stronger, dedicated outage/recovery coverage. The replay edit reads the exact outbox pending count after acknowledgements whose staging happens synchronously, so it directly detects any replay that incorrectly reaches the run-request outbox.

## Verification notes and limits

- Inspected the complete `3f8767a5f..9b560d005` diff, the relevant current owners/callers/tests, `git show --cc c5527627a`, and differences from both merge parents.
- `git show --remerge-diff c5527627a` could not run because Git could not create its temporary object directory; the combined diff and explicit comparisons against both parents provided the resolution evidence instead.
- `git diff --check 3f8767a5f..9b560d005` passed.
- The task records passing focused Postgres runtime coverage, `mise run test:sql`, `mise run test:bifrost:integration:server`, `mise run test:bifrost:journey:server` (31/31), `mise run fmt`, and `mise run lints`. This specialist did not rerun Cargo/mise in the shared checkout, as directed.
- No generated contract changed, so `codegen:check` is not independently required by this task diff.

## Overall result

**PASS**
