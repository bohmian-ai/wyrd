# TASK-006 data, durability, and concurrency review

## Immutable subject

- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `55c5bff84fbfcc8f43967a8ad3aeac6057f10c3f`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-006-continuous-eval-verifier.md`
- The candidate object remained the fixed requested commit. The checkout later
  advanced to `01146cf87d22147b87d0c9224aa2bdf67decad92` only for repository review-skill
  documentation; `git diff 55c5bff8..01146cf8` showed no reviewed product source,
  task, specification, architecture, migration, or test change.

## Reviewed boundary

This review traced the native Eval observation write from Gate through Scribe's durable completion, the post-ACK asynchronous enqueue, the tenant-scoped `verifier_runs` insert and unique key, claim/reclaim/settlement behavior, and the runner's exact observation read and canonical result publication. It also inspected the applicable integration and journey tests for duplicate protection, failure after ACK, restart, frozen input identity/time, result/detail publication, and tenant isolation.

## Authority and source coverage

| Boundary | Governing obligation | Source and test coverage | Result |
|---|---|---|---|
| Durable ACK and best-effort enqueue | Spec `REQ-077`; task Scenario 4; Bifrost durability and Scribe ACK authority | `gate/mod.rs`, `scribe/ingress.rs`, `scribe/shards.rs`, `verification/observations.rs`, Eval journey forced-insert failure | FAIL — DATA-001 |
| Immediate queue time and idempotent insertion | `REQ-077`, `REQ-079`, task Scenario 4 | `verifier_runs.rs` `INSERT_RUN_SQL`/`EXISTING_RUN_SQL`; migration `20260601000029_verifier_runs.sql`; `pg_verifier_runs::observation_enqueue_targets_active_ready_bindings_once` | PASS apart from the upstream event-time defect in DATA-001 |
| Tenant/RLS ownership | `REQ-078`, `REQ-108`; `AGENTS.md` and `agent-rules.md` `TenantConn` rules | `ObservationEnqueue::enqueue`, `VerifierRunQueue::enqueue_observation`, forced-RLS migration and queue tenant-isolation tests | PASS |
| Claims, restart, and fenced settlement | `REQ-079`, `REQ-083`; task Scenarios 5–6 | `verifier_runs.rs` claim/retry/trace-wait/complete paths; `runner.rs`; SQL claim and restart-oriented journey coverage | PASS |
| Exact record/day load | `REQ-079`, `REQ-083`; Eval architecture | `eval.rs::BifrostReader::record`, frozen `RunInput::EvalRecord`, journey comparison of row and run event time | PASS when the frozen time is authoritative; DATA-001 makes a replay-created run violate that precondition |
| Canonical Eval summary/item persistence | `REQ-085`–`REQ-087`, `REQ-119`, `REQ-121`, `REQ-122`; table-schema authority | `runner.rs::publish`, `results.rs`, `publisher.rs`, terminal-matrix journey assertions | PASS |

## Material finding

### DATA-001 — A deduplicated replay can freeze a fresh receipt time instead of the committed observation's event time

- **Violated obligation:** `REQ-077` requires `input_event_time` to be the exact server-managed `wyrd_event_time` assigned to the committed observation, and requires duplicate ACK/replay to converge idempotently. `REQ-079` then requires that frozen time to select the committed row's UTC-day partition. Task Scenario 4 explicitly requires duplicate ACK/replay convergence; Scenario 5 requires the frozen day/record read.
- **Exact location:**
  - `crates/vala/vala-bifrost-redux/src/scribe/ingress.rs:552-582` computes a new `receipt_micros` for every ingest attempt and returns that attempt-local value in `FrameAdmission` after durable completion.
  - `crates/vala/vala-bifrost-redux/src/scribe/shards.rs:3670-3765` suppresses a second insertion when the durable batch fence reports `AlreadyCommitted`; the logical digest deliberately excludes fresh managed timestamps (`scribe/preprocess.rs`, `LogicalBatchDigest`), so an honest replay may have a different receipt instant.
  - `crates/vala/vala-bifrost-redux/src/gate/mod.rs:956-992` invokes the observation hook for both the original commit and a deduplicated replay, passing the current attempt's `admission.receipt_micros` without a new-versus-replay disposition.
  - `crates/vala/vala-bifrost-redux/src/tables/eval/observations.rs:104-120` substitutes that receipt for frames without caller-authored `wyrd_event_time`, which is the ordinary Eval observation path.
  - `crates/wyrd/wyrd-server/src/verification/observations.rs:66-85` persists the derived time in the run. `crates/wyrd/wyrd-server/src/verification/eval.rs:332-340` later uses its UTC day to prune the exact record read.
- **Evidence:** The Scribe batch fence accepts the same logical batch ID/payload on replay while suppressing its second memtable insertion. Nevertheless Gate treats the replay as another observation ACK and reconstructs its activation keys from the original unstamped frame plus the replay attempt's fresh receipt. Because enqueue is asynchronous, two ACK hooks for the same batch can race: the replay task can insert first under the unique `(tenant, binding, input_record_id)` index, after which the original task merely finds that wrong frozen row. The existing SQL replay test calls `enqueue_observation` twice with the same supplied `event_time`; it does not exercise duplicate Scribe ACKs with distinct receipt instants. The journey exercises only a single ACK per observation.
- **Observable reachable consequence:** A normal at-least-once retry near a UTC boundary can create a durable run whose `input_event_time` points at the replay day while the sole committed observation remains in the original day. The runner repeatedly reports `eval_record_unavailable` and eventually settles the run `errored`, even though the acknowledged observation exists. Away from a day boundary, the run still stores a false immutable event fact, violating reproducibility and exact evidence identity.
- **Required testable correction:** Make the post-ACK activation consume only authoritative facts from the commit that actually established the row. A deduplicated replay must not be able to enqueue with attempt-local managed timestamps or overwrite/win against the original activation. Preserve the required best-effort, asynchronous Postgres insert outside Scribe's batch-fence transaction and add no outbox. Add a real Scribe/Gate test that sends the same sealed Eval batch ID and payload through two ACK attempts with receipt instants on opposite UTC days (including concurrent hook scheduling), then proves exactly one run is created and its frozen `input_event_time` equals the one committed row's `wyrd_event_time`; the runner must load that row from the original day.

## Verification limits

- This was a static acceptance review; I did not modify reviewed source or rerun the broad repository lanes reported in the task evidence.
- The candidate's journey covers post-ACK SQL failure, pending-run recovery when the runtime starts, terminal result behavior, and one row/run time equality check. It does not cover Scribe-level duplicate ACK/replay, concurrent hook ordering, or a replay across a UTC boundary.
- The SQL queue test proves unique-key convergence only when both calls are handed the same event time, so it cannot close DATA-001.
- No new cross-tenant journey was added for the enqueue hook. The inspected implementation nevertheless uses tenant-owned `TenantConn` under forced RLS and resolves subjects from the authenticated signed scope; no reachable tenant escape was found.

## Overall result

**FAIL** — the bounded DATA-001 replay race can persist a non-authoritative input event time and make the required exact record/day read fail. No other material in-scope data, durability, concurrency, restart, idempotency, or tenant-isolation finding was identified.
