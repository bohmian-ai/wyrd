# System-resilience review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `5c3bb79b3598abd88a3a234611fc400096adc975`
- Cumulative range: `7d96c30066425e0cde2290842d5801307843283d..5c3bb79b3598abd88a3a234611fc400096adc975`
- Remediation range inspected: `852894689388124960993014a46934e73c0ed2a8..5c3bb79b3598abd88a3a234611fc400096adc975`
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior verdict and remediation: `changes/active/verified-change-contract/review/TASK-008-r1/verdict.md` and `TASK-008-CLOSEOUT-R1-capacity-closure.md`

The requested authority cannot be established. The caller names approved
specification revision 58, but the candidate's tracked
`changes/active/verified-change-contract/spec.md` is revision 57 and approved,
and the task and remediation both map revision 57. `git log` shows no revision
58 commit for this specification. Because the requested approved revision is
not present in the immutable candidate, this review cannot decide whether the
runtime behavior satisfies revision 58.

The resilience analysis below records what can be established against the
available revision-57 task/remediation boundary. It is not acceptance of the
missing revision-58 contract.

## Deployed-path evidence

The remediation remains confined to the opt-in release benchmark and its
release-server harness; it does not change production server, queue, Scribe,
Oracle, Forge, verification-engine, SDK, database, or deployment behavior.

The deployed path is:

1. `mise.toml`'s `bench:capacity` task starts repository-managed Postgres and
   RustFS, builds release `wyrd-server`, and invokes the `capacity` binary.
2. `capacity/main.rs:240-295` constructs the benchmark lifetime, output and
   scratch roots, local TLS judge, peer certificates, signing key, and owner.
3. `Benchmark::provision` at `capacity/main.rs:411-429` asks
   `LocalServer::start` to migrate, start replica 0, and run tenant setup, then
   provisions four tenants and connects the cross-tenant evidence reader.
4. `capacity/main.rs:359-394` runs warmup and one-replica ramp/sustained steps,
   starts replica 1 against the same Postgres/RustFS deployment, reconnects
   each tenant to both replicas, and runs two-replica sustained and scale-out.
5. `Deployment::run` at `capacity/step.rs:320-380` drives the public-client
   mix, waits for request tails and client flush, closes the common CPU/memory
   window, drains run/Scribe/audit/Forge work, and records evidence.
6. `Benchmark::clean_up` at `capacity/main.rs:509-531` attempts client
   shutdown, then sends `SIGTERM` to each release replica newest first and
   copies its log.

Affected capabilities are benchmark evidence for direct and queued
verification, client ingest, Scribe publication, Oracle queries, audit
publication, Forge planning, and two-replica scale-out. Production availability
and recovery semantics are consumers under measurement, not changed owners.

## Failure and recovery paths

| Failure or interruption | What stops / remains available | Recovery and proof assessment |
|---|---|---|
| A request is refused, times out, or returns a wrong judgment | The affected operation is counted as an error; other lanes and replicas continue. | The harness does not retry arrivals, so it does not amplify uncertain non-idempotent direct executions. The step fails from public-client evidence. |
| Driver permits are exhausted | That arrival is counted as missed; already-started requests and all server capabilities continue. | The achieved/offered SLI exposes driver saturation instead of lowering offered load silently. |
| Postgres, RustFS, `/metrics`, or evidence SQL becomes unavailable during a step | The local scrape/query/client error propagates from `Deployment::run`; the shared server process is not deliberately crashed by the harness. | `Benchmark::run` proceeds to owned cleanup and writes a failed report when the async error is returned. No dependency-outage injection is recorded. |
| Measurement reaches its Tokio deadline while awaiting request work | `Lifetime::measure` drops `Benchmark::measure`; `Lane::drive` drops its `JoinSet`, aborting client tasks. Requests already admitted may retain durable server effects. | The deployment remains owned for cleanup. No partial `Record` survives, so the report fails rather than presenting incomplete evidence as a completed step. |
| Scribe persistence has handed work to durable staging | `scribe_backlog` now includes `bifrost_scribe_staging_live_members` in addition to persistence and immutable gauges (`capacity/evidence.rs:89-104`). | A staged ready or claimed member prevents the drain from passing. The focused test covers the handoff state; no new ledger or retry path was introduced. |
| A backlog first appears empty after 60 seconds | `Drain::judge` returns `Expired`, including for an empty observation after the deadline (`capacity/step.rs:239-261,395-420`). | The record carries `None` for drain time and the report fails the saturation cell. Focused tests cover below, exactly at, and above the boundary. |
| Client shutdown stalls | Remaining clients are dropped when `Lifetime::shut_down` reaches its absolute `clients_until`; their queued, unflushed observations may be lost, while durable server state remains. | Replica shutdown still follows, and abnormal `LocalServer` drop kills/reaps the process and preserves its log. The client timeout is not a 30-second duration from cleanup start; it can consume the whole unused measurement allowance. |
| A release replica ignores `SIGTERM` | `LocalServer::terminate` waits 45 seconds, kills and reaps it, then returns a failed shutdown result (`release_server.rs:347-388`). | The other replica is then stopped. Logs are copied on the explicit stop path and retained in the abnormal-drop path. |

## Material proposed findings

### SYS-R2-001 — INCORRECT — the advertised absolute command deadline cannot interrupt setup subprocesses

- **Violated obligation:** Available revision-57 REQ-171 and remediation R2
  require one absolute 30-minute command boundary covering setup through
  bounded cleanup. The prior finding specifically required slow dependency
  and setup paths to return failure rather than outlive the command.
- **Exact locations:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:240-259,311-314,623-628`;
  `crates/wyrd/wyrd-testing/src/release_server.rs:147-169,618-635`.
- **Evidence:** `Benchmark::prepare` begins `Lifetime` but is awaited before
  `Benchmark::run` installs `timeout_at`, so preparation itself is not inside
  a deadline future. More importantly, the bounded `Benchmark::measure` calls
  `LocalServer::start`, whose migration and tenant-setup subprocesses use
  synchronous `std::process::Command::output()`. A Tokio timeout cannot poll or
  cancel while that call blocks the runtime thread. The only deadline test at
  `capacity/main.rs:655-680` uses `std::future::pending`, so it proves an
  await-cooperative future is dropped, not that the real setup subprocess path
  is bounded. Replica log copying after each synchronous stop is likewise only
  budgeted by estimate, not guarded by the absolute deadline.
- **Observable system consequence:** If migration or `setup` stalls during a
  Postgres outage, process interruption, or wedged child, `bench:capacity` can
  remain alive after 30 minutes with no report and no timeout-driven recovery.
  The claimed command boundary therefore does not hold on the dependency/setup
  failure path it was added to cover.
- **Testable correction:** Put fixture preparation, subprocess setup, measured
  work, client cleanup, replica termination, and report finalization under one
  enforceable absolute deadline. Make operator subprocess ownership
  cancellation-aware so expiry kills and reaps the child rather than merely
  dropping an outer Tokio future; cap cleanup by the remaining absolute budget
  (and the intended per-phase maximum). Add a focused test with a deliberately
  non-terminating setup child, not only a pending async future, and prove the
  command kills/reaps it, retains diagnostics, reports failure, and exits by
  the shortened deadline.

## Recovery and proof assessment

The remediation closes the prior Scribe-staging false-drain path and exact
60-second drain edge in source. It also preserves owned process cleanup on
ordinary async errors and cooperative cancellation. The recorded focused unit
evidence does not exercise an unavailable Postgres/RustFS dependency, a
stalled migration/setup child, a replica restart, or deadline expiry through
the real deployment lifecycle. The task record still contains only the earlier
shortened failing smoke and no authoritative unmodified default execution;
there is therefore no empirical proof that the two-replica deployment reaches
its required recovery/cleanup state inside the advertised total lifetime.

## Overall result

**BLOCKED**

The immutable candidate does not contain the caller-specified approved
revision 58, so the governing acceptance contract is missing. Against the
available revision-57 remediation boundary, SYS-R2-001 remains a material
system-resilience failure that also requires correction before approval.
