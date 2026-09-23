# Bifrost publication, Arrow, and durability domain review

## Immutable subject

- Base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Candidate: `2af4cc3ff95a609d1df682be4f633345f96934e1`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Prior review: `changes/active/verified-change-contract/review/TASK-004-r1/`
- Remediation task: `changes/active/verified-change-contract/review/TASK-004-r1/TASK-004-R1-close-validated-runtime-gaps.md`

The candidate remained exactly `2af4cc3ff95a609d1df682be4f633345f96934e1` throughout this review.

## Reviewed boundary

This review traced the complete verification-result path within the cumulative base-to-candidate change: result report mapping into Arrow batches; name-preserving schema assembly; detail-before-summary publication through `wyrd_client::Bifrost`; Gate admission and Scribe acknowledgement; Oracle reads on a role-separated deployment; runner settlement and Operator dispatch; sealed ambiguous-ACK replay; and crash/reclaim behavior after a durable detail acknowledgement but before a known summary outcome. It specifically reassessed prior findings `FIND-TASK-004-2`, `FIND-TASK-004-6`, and `FIND-TASK-004-7`.

## Authority and source coverage

| Boundary | Authority | Source and evidence inspected | Result |
|---|---|---|---|
| Arrow field identity | `AGENTS.md` column-name rule; `architecture/agent-rules.md`; `architecture/references/domain/arrow-analytical-interop.md`; `FIND-TASK-004-2` | `crates/wyrd/wyrd-server/src/verification/results.rs:220-545,1045-1148`; table-owned `arrow_fields()` declarations and focused mapping tests | PASS |
| Detail-before-summary and all-required-ACK settlement | Spec `REQ-086`, `REQ-087`, `INV-007`; TASK-004 Scenario 5; `architecture/bifrost-design.md`; analytical reliability reference | `crates/wyrd/wyrd-server/src/verification/results.rs:205-244`; `publisher.rs:125-178`; `runner.rs:372-527`; existing replay/fresh-attempt evidence in TASK-004 and R1 | PASS |
| Exact SYSTEM/Verifier/subject/owner/binding identity through role separation | Spec `REQ-115`, `AC-023`, `AC-030`; TASK-004 Scenario 5; Bifrost public-surface and tenant authority | `crates/wyrd/wyrd-testing/src/verification.rs:60-219`; `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:44-194`; Gate/Scribe correlation path | PASS |
| Gate/Scribe scope and acknowledgement boundary | Spec `REQ-086`, `REQ-145`, `INV-007`; Bifrost acknowledgement definition and audit invariant | `crates/vala/vala-bifrost-redux/src/gate/mod.rs:435-505,973-1010`; `scribe/execution_lanes.rs:715-787`; authenticated gRPC evidence recorded by the remediation | PASS |
| Crash after detail ACK, lease reclaim, and dispatch fencing | Spec `REQ-086`, `REQ-087`, `REQ-146`, `AC-030`; TASK-004 Scenario 5; analytical reliability lease/fence and uncertain-effect rules | `publisher.rs:212-358`; `runner.rs:153-357,477-581`; `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs:1371-1481` | PASS |

## Prior-finding closure

### `FIND-TASK-004-2` — closed

`ResultPayloadBuilder` now authors `(field name, ArrayRef)` pairs. Its single `assemble` boundary builds a name map, rejects duplicate, missing, and unexpected names, orders arrays from the destination table's own `arrow_fields()`, and only then appends the three correlation fields. The independently reversed-schema test proves adjacent same-typed fields retain their declared identities; the negative test proves malformed authored field sets fail closed. This is the minimum correction and introduces no second schema owner or generic mapping framework.

### `FIND-TASK-004-6` — closed

The existing no-local-Scribe journey now creates a binding-owned, distinct-subject, two-feature Drift result and publishes it from the Oracle-only runner through the Scribe node's gRPC ingest endpoint. Oracle queries assert the exact tenant-scoped SYSTEM principal, Verifier UID, subject UID, owner UID, binding ID, run ID, result ID, and shared event time for the summary and both detail rows. It retains the explicit proof that the runner node has no local Scribe. No additional cluster or harness was added.

### `FIND-TASK-004-7` — closed

The focused Postgres runtime test waits until the detail rows are durably visible, verifies the summary is still blocked before send, crashes the runner capability, and observes the run still `running` on attempt 1 with no result identity and no dispatch. After lease expiry it proves the same durable run is reclaimed as attempt 2, completes only after detail and summary ACKs, and creates exactly one dispatch. The two attempts' four detail rows are permitted partial analytical visibility; only the later summary and fenced Postgres settlement authorize completion and dispatch.

## Independent verification

The following commands were run against the immutable candidate and exited 0:

```text
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::results::tests::reordered_table_fields_keep_values_bound_to_names) | test(=verification::results::tests::missing_duplicate_and_unexpected_columns_are_refused)'

scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E "test(=crash_after_detail_ack_reclaims_the_same_run_before_dispatch)" --test-threads=1'

scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::runner_without_local_scribe_publishes_through_the_ingest_endpoint)"'
```

The remediation record additionally reports passing authenticated Gate/Scribe scope tests, sealed lost-ACK replay, fresh-attempt publication, full Bifrost integration and server-journey lanes, `test:wyrd`, `test:vala`, `test:sql`, tenant isolation, code generation, formatting, and lints.

## Verification limits

- This domain pass did not rerun every broad lane recorded in the remediation evidence; it independently reran the three focused proofs for the prior Arrow, role-separated identity, and detail-ACK crash gaps.
- The accepted specification permits partial detail rows and does not promise atomic cross-table analytical visibility. The review therefore verifies settlement and dispatch fencing rather than requiring rollback of the first attempt's acknowledged detail rows.
- The role-separated journey proves tenant identity through a tenant-authorized Oracle query plus the SYSTEM `principal_id` column; tenant is server-managed authority and is not a client-authored result column.

## Material proposed findings

None.

## Overall result

**PASS** — the Bifrost publication boundary satisfies the original task and remediation obligations. Result fields remain attached by name, every non-empty detail precedes the summary, completion and dispatch require all acknowledgements plus fenced Postgres settlement, the role-separated path preserves exact result/detail identities, and a crash after detail acknowledgement safely reclaims the same run without premature dispatch.
