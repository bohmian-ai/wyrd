# Data, durability, and concurrency domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `3593bbc31273673f87159315fbf66a73562d3c99`
- Scope: durability, persistent data, concurrency, and the Bifrost/Scribe boundary only
- Candidate remained `HEAD` throughout this review.

## Reviewed boundary

The review traced the complete cumulative implementation from the native Eval observation write through Gate and Scribe acknowledgement, the fail-open post-ACK task, tenant-scoped binding lookup and run insertion, immutable sampling order, claim/reclaim/retry/trace-wait settlement, exact-partition input reads, ordered result/detail publication, completed-run and Operator-dispatch settlement, restart behavior, and the remediation's audit-publisher concurrency change. It also inspected the verifier-run migrations and SQL constraints/indexes that persist the frozen selectors and observation ordinal.

## Authority and source coverage

| Boundary | Authorities | Source and proof inspected | Result |
|---|---|---|---|
| Post-ACK activation and accepted loss ceiling | Approved spec revision 36, especially REQ-077, REQ-078, REQ-079, INV-010, INV-015; TASK-006 Scenario 4 and AC-014/AC-020; `architecture/bifrost-design.md` durability transition | `gate/mod.rs`, `contracts.rs`, `scribe/{ingress,preprocess,shards}.rs`, `verification/observations.rs`, and `eval_verification::continuous_eval_runs_the_terminal_matrix` | PASS |
| Replay identity and frozen event partition | REQ-077, REQ-079, AC-014; R1 `FIND-TASK-006-1`; `table_schema.md` managed daily partition | `FrameAdmission.first_commit`, `DurableCompletion`, `GroupWalState::inserted_batch_ids`, Gate's `first_commit` filter, `EvalObservationsTable::acknowledged`, and `sealed_replay_on_a_later_day_activates_once` | PASS |
| Idempotent run creation and immutable sampling | REQ-077, REQ-083, REQ-152, INV-010; R1 `FIND-TASK-006-2` | `verifier_runs.rs` binding-row lock, `INSERT_RUN_SQL`, observation-record unique index, migration `20260601000032_verifier_run_observation_ordinal.sql`, `ClaimedRun.observation_ordinal`, Eval `sampled`, and `observation_ordinal_is_fixed_at_serialized_enqueue` | PASS |
| Claim, retry, trace wait, deferral, stale lease, and restart | REQ-079, REQ-083, REQ-152, AC-016/AC-020; analytical reliability authority | PostgreSQL-clock `CLAIM_RUN_SQL`, `RETRY_RUN_SQL`, `AWAIT_TRACE_SQL`, `RELEASE_RUN_SQL`; runner permit-before-claim, fenced settlement, shutdown release, and journey/SQL restart evidence | PASS |
| Exact Eval input read | REQ-079, REQ-083, INV-010; R1 `FIND-TASK-006-5`, `-6`, and `-8`; `eval.md` | `BifrostReader::record` constrains `input_record_id` to the frozen `wyrd_event_time` UTC day; trace read has a total order, closed event-time interval, and `LIMIT 10001`; cross-day and bounded-trace journeys | PASS |
| Canonical item/summary persistence and ACK ordering | REQ-085, REQ-086, REQ-119, REQ-121, REQ-122; `table_schema.md`; Bifrost remote-Scribe boundary | `ResultPayloadBuilder`, `ResultPublisher`, and `VerifierRunner::publish`: detail batches precede the summary, empty details emit no batch, each non-empty batch crosses `wyrd_client::Bifrost`, and completion/dispatch settlement follows all ACKs | PASS |
| Terminal settlement and dispatch atomicity | REQ-083, REQ-084, REQ-085; TASK-006 Scenario 6 | Token-fenced `COMPLETE_RUN_SQL` and dispatch inserts share one caller-owned tenant transaction; errored/timed-out paths never point to a result or insert dispatches; partial Bifrost rows remain within the approved separate-ACK ceiling | PASS |
| Migration and SQL ordering | AGENTS.md tenant/transaction rules; REQ-078, REQ-152 | Migration 32 follows verifier-run/idempotency migrations, backfills observation runs deterministically by `(created_at, run_id)`, enforces positive observation-only ordinals and per-binding uniqueness; queue operations use `TenantConn` and PostgreSQL time | PASS |
| Audit publisher remediation seam | Repository audit durability rules and Bifrost replay/fence design | `AuditPublisher::run`, `TenantCycles`, freeze/publish/settle transactions, deterministic range batch identity, blocked-tenant unit proof, and the unchanged stalled-tenant journey assertions | PASS |

## Prior-finding closure

- `FIND-TASK-006-1` is closed. Scribe now carries first-commit disposition to Gate; a suppressed replay cannot activate a second enqueue or substitute its later receipt instant. The focused journey uses a real Gate/Scribe/Postgres path, later-day concurrent replays, and verifies the stored and frozen managed event time.
- `FIND-TASK-006-2` is closed. Observation creation locks the binding, assigns the next ordinal in the same transaction, stores it under uniqueness and shape constraints, and returns it with every later claim. Duplicate insertion consumes no ordinal; retry and fresh runtime handles read the same value.
- `FIND-TASK-006-5`, `FIND-TASK-006-6`, and `FIND-TASK-006-8` are closed for this domain. Input reads use the frozen managed day, trace rows have a deterministic total order and bounded time/row selectors, and the journeys exercise cross-day and restart paths.
- The remediation's Oracle-admission classification preserves durability semantics: admission pressure releases the lease with its attempt refunded instead of consuming the run's bounded retry budget.

## Material findings

None.

## Verification limits

- This reviewer performed source, migration, diff, caller, and test inspection but did not rerun Cargo/Postgres lanes inside the 20-minute sub-review budget. Candidate evidence records focused Postgres tests, the full Bifrost server/Scribe journeys, SQL and Vala families, format, lints, codegen, and boundary checks as passing; those command results were not independently reproduced in this review.
- The accepted specification ceilings remain: post-ACK enqueue can be lost after the observation ACK, separately acknowledged result tables can retain partial rows, and a process exit can leave a claim to expire before another runtime reclaims it. This review found no implementation widening beyond those approved limits.
- No live crash was injected between result publication and PostgreSQL settlement in this review; the conclusion relies on the inspected deterministic batch fence, frozen run identity, token-fenced settlement, lease expiry, and the existing restart/partial-publication coverage.

## Overall result

**PASS** — no material durability, persistent-data, concurrency, migration, or Bifrost/Scribe finding remains for TASK-006 at the immutable candidate.
