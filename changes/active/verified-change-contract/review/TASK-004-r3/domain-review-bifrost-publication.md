# Bifrost publication, Arrow, and analytical durability domain review

## Immutable subject

- Base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Candidate: `29721b7e33854633b025b25948fd5d2eaebe7bfd`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Prior remediation inputs:
  `changes/active/verified-change-contract/review/TASK-004-r1/TASK-004-R1-close-validated-runtime-gaps.md`
  and
  `changes/active/verified-change-contract/review/TASK-004-r2/TASK-004-R2-close-scheduler-ordering-and-source-shape.md`

The candidate remained exactly
`29721b7e33854633b025b25948fd5d2eaebe7bfd` throughout this review, and the
base is its ancestor. `.codegraph/` is absent, so source and caller tracing used
repository search and direct inspection.

## Reviewed boundary

This review traced the complete cumulative result path: Drift/Eval report
mapping into the three fixed analytical result tables; Arrow column binding by
name; shared result identity and event time; detail-before-summary publication;
authenticated remote Bifrost/Gate/Scribe admission; durable acknowledgements;
ambiguous-ACK sealed replay versus fresh-attempt identity; crash/reclaim after
a durable detail ACK; fenced completion and Operator dispatch; and
Oracle-visible result/detail identity on a role-separated deployment. It also
checked the human-directed deletion of the uncalled
`ResultPublisher::endpoint` accessor.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Canonical result and detail shape | Spec `REQ-061`, `REQ-062`, `REQ-063`, `REQ-085`, `REQ-119`, `REQ-121`, `REQ-122`, `INV-010`; `architecture/logic/table_schema.md`; `AGENTS.md` name-mapping rule | `verification/results.rs`; `tables/verification/results.rs`; `tables/drift/result_features.rs`; `tables/eval/result_items.rs` | PASS |
| Ordered remote publication and ACK-gated settlement | Spec `REQ-086`, `REQ-087`, `REQ-097`; TASK-004 Scenario 5; `architecture/bifrost-design.md` append/dedup authority | `verification/publisher.rs:116-175`; `verification/runner.rs` publication and settlement path; `pg_verification_runtime.rs` ordered, partial, replay, and crash cases | PASS |
| Authenticated writer and Verifier identity | Spec `REQ-086`, `REQ-119`, `REQ-121`, `INV-007`; Bifrost Gate/Scribe identity rules | Gate closed result-table matrix and mandatory frame scope; Scribe signed-scope UID resolution and managed stamping; role-separated journey | PASS |
| Sealed replay and fresh attempt distinction | Spec `REQ-087`; Bifrost logical-batch identity and durable batch fence | `Bifrost::write_batch` path through the recording real gRPC transport; lost-ACK and fresh-attempt tests | PASS |
| Role-separated persistence and Oracle visibility | Spec `AC-023`, `AC-030`; Bifrost Scribe/Oracle ownership | `wyrd-testing/tests/bifrost/server/verification_runtime.rs:32-192` | PASS |
| Owner-directed dead-code deletion | Original task public/consumer surfaces and cumulative caller graph | Candidate removes only `ResultPublisher::endpoint`; repository-wide caller search finds no invocation or contract consumer; the private endpoint field remains used by `publish` to configure the gRPC client | PASS |

## Boundary trace

1. `VerifierRunner::publish` creates one fresh `VerificationResultId` and one
   server event time for an attempt, builds the payload, and does not form a
   completion transition until publication returns success.
2. `ResultPayloadBuilder` authors named columns, rejects missing, duplicate, or
   unexpected names, orders them from each table's own `arrow_fields()`, and
   appends `card_ref`, `run_id`, and the same `wyrd_event_time`. Empty detail
   sets produce no batch; any non-empty detail batch precedes the summary.
3. `ResultPublisher::publish` mints the tenant SYSTEM token immediately before
   the attempt, creates `wyrd_client::Bifrost` against the configured remote
   endpoint, and awaits every batch sequentially. It owns no Scribe handle and
   has no direct/local write path.
4. Gate reserves the three result tables to SYSTEM, requires every row's
   `card_ref` to fall inside the exact signed Verifier scope, and records the
   one canonical decision. Scribe derives tenant and principal from authority,
   resolves the signed Verifier UID, stamps managed identity, and acknowledges
   only after its WAL/batch-fence durability boundary.
5. A lost ACK inside one `write_batch` call reuses the same sealed table, batch
   ID, and bytes. A later run attempt rebuilds the result and calls
   `write_batch` again, producing a fresh result and batch identity; previously
   acknowledged detail rows remain legitimate partial analytical evidence.
6. Only all required ACKs yield `Transition::Complete`; the lease-fenced
   Postgres settlement then records the result pointer and creates any failed
   binding dispatches. A crash after detail ACK leaves the run unsettled and
   dispatch-free until lease reclaim and a later fully acknowledged attempt.
7. The role-separated journey publishes from an Oracle-only runner through the
   Scribe node and queries the summary and two detail rows through Oracle,
   proving the exact SYSTEM principal, Verifier, subject, owner, binding, run,
   result join, and shared event time.

## Prior-finding closure

| Finding | Result | Evidence |
|---|---|---|
| `FIND-TASK-004-2` | CLOSED | Table-owned schemas and name-based assembly are intact; both focused malformed/reordered-column tests pass on the final candidate. |
| `FIND-TASK-004-6` | CLOSED | The final role-separated journey publishes a non-empty binding result and asserts summary/detail identities through Oracle. |
| `FIND-TASK-004-7` | CLOSED | The final crash test crosses durable detail ACK, blocked summary, runner crash, lease reclaim, full later ACKs, and dispatch fencing. |

The R2 remediation changes scheduler ordering and source spelling, not this
publication protocol. Its publisher edit aliases `std::fmt::Result` for the
required bare-type rule without changing behavior. The final accessor deletion
removes a zero-caller diagnostic getter; it does not remove configuration,
transport selection, observability, a public wire contract, or a test seam.

## Independent verification

These commands were run against the immutable final candidate and exited 0:

```text
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(=verification::results::tests::reordered_table_fields_keep_values_bound_to_names) | test(=verification::results::tests::missing_duplicate_and_unexpected_columns_are_refused)'

scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked \
   -p wyrd-server --features test-support --test pg_verification_runtime \
   -E "test(=lost_result_ack_replays_the_identical_sealed_batch_and_scribe_deduplicates) | test(=unacknowledged_summary_retries_with_a_fresh_result) | test(=crash_after_detail_ack_reclaims_the_same_run_before_dispatch)" --test-threads=1'

scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
   -p wyrd-testing --test server -P journey --run-ignored=all \
   -E "test(=verification_runtime::runner_without_local_scribe_publishes_through_the_ingest_endpoint)"'
```

The selected runs executed six tests: two Arrow name-mapping tests, three
Postgres-backed publication durability tests, and the role-separated
Scribe/Oracle journey. All six passed.

## Verification limits

- This domain review did not rerun every broad repository, SDK, MCP, codegen,
  lint, or formatting lane recorded in the task and remediation evidence. It
  reran the focused executable proofs for every prior finding and the principal
  publication risks in this boundary.
- The approved contract permits acknowledged partial detail rows and does not
  promise cross-table atomic visibility. The review therefore verifies durable
  evidence, completion/dispatch fencing, and fresh-attempt identity rather than
  requiring rollback or deduplication across distinct attempts.
- Oracle tenant identity is enforced by the tenant-authorized query context and
  server-managed storage boundary; it is not a caller-authored result column.

## Material proposed findings

None.

## Overall result

**PASS** — the cumulative candidate satisfies the Bifrost publication,
Arrow-name mapping, ordered ACK durability, replay/fresh-attempt identity,
role-separated Oracle visibility, and persistent analytical evidence
obligations. The owner-directed `ResultPublisher::endpoint` deletion removes
only unreachable code and does not weaken a required contract or proof.
