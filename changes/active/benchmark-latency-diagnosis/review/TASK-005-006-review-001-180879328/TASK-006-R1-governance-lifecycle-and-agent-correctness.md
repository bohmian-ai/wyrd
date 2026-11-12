# TASK-006-R1 — Repair governance ownership and Agent projection

Route directly to `$wyrd-implement`. Status: ready for bounded implementation. Performance workloads retain separate original authorization; the review authorizes no benchmark run.

## Subject and intended outcome

Repository `/home/thorrester/Documents/GitHub/wyrd-worktrees/resource-scaling`.
Approved specification: `changes/active/benchmark-latency-diagnosis/spec.md`, revision 7.
Original task: `changes/active/benchmark-latency-diagnosis/tasks/TASK-006-gateway-local-governance-and-agent-check-latency.md`.
Base `0c81715f2ac52efd2b105e454ee309724df1655f`; reviewed candidate `180879328ac9dde94d7504855e821980e5db0f18`.
Original task/spec, [verdict](verdict.md) and [independent ledger](findings-validation.md) remain acceptance context. First remediation; review did not edit source.

Governed calls must retain committed history through upgrade, retain one live tenant synchronization authority, recover policy reloads, and drain pending usage through commit or deadline. Agent reference projection must preserve previous behavior while keeping the lazy optimization. Complete the explicitly missing failure/performance evidence. All corrections fit approved behavior; no spec revision is needed.

## Diagnosis and selected correction boundaries

### FIND-006-1 — Lazy reference changes public export according to read order

`crates/skald/skald-agent/src/agent.rs:263–308,588–590,773–823` leaves a resolved Agent's reference empty until first read, deriving it from the public mutable runtime `prompt`. Construct A and assign B: exporting yields B if no earlier reference read, A otherwise. Base exported stored A in both cases; explicit `with_prompt` retargets both. Card/wire/YAML/cascade projections and clones consume this value, so a read can change persisted configuration. This violates behavior/identity preservation, INV-001/003.

Keep the construction/explicit-retarget Prompt as a stable lazy source independently of runtime `prompt`, reusing existing cheap shared Prompt ownership. Derive inline native configuration only when needed; preserve named/stored references and `with_prompt`/`try_with_prompt` behavior. Fix Agent once; do not privatize the public field, restore eager per-check deep copies, or guard each exporter. Focused construction/projection tests compare both read orders, initialized clone, runtime B with stored A, explicit retarget B and named-reference preservation.

### FIND-006-3 — Upgrade splits the committed limit representation

New `crates/wyrd/wyrd-sql/src/queries/gateway_admission.rs:145–159` reads only contribution tables created by `20261111000000_gateway_usage_contributions.sql`. Old `gateway_admit` persists/reads `gateway_limit_windows` and `gateway_call_leases`; neither side sees the other's persisted rows. New restart therefore ignores old current-window usage/unexpired calls, and rolling versions enforce disconnected views. Shared budget ledger remains intact. REQ-031 preserves committed history; deployment authority requires compatible overlap. This is not accepted unflushed-memory loss or propagation delay.

Complete both read directions at the existing SQL owner using additive migration(s), never editing applied migrations. New baseline reads aggregate still-applicable old counters/leases plus new other-server contributions once. Replace retained database admission function read projections so running old binaries also see persisted new rows, preserving their old-format writes/settlements and new servers' absolute per-server writes. Preserve RLS, original expiry/settlement boundaries and common budget ledger. Do not copy mutable totals once, dual-charge writes, reprice history, reinstate strict global admission or invent a rollout protocol. Bidirectional SQL read compatibility is the smallest complete correction; new-reader-only aggregation misses overlap.

Existing real-PG fixture proof charges through old SQL and warms the new owner; persists new contributions and invokes old admission; then settles/expires each representation and observes counts changing once, including tenant isolation. No multiprocess release harness is required.

### FIND-006-4 — Eviction can replace an in-flight tenant sync lock

`components/gateway/usage.rs:213–219,284–299,451–455,521–525,553–603` treats cold/idle empty state as clean while its cloned per-entry sync mutex is active. At capacity it can evict T and reinsert T with a different mutex. Both worker and administration→warm (`service.rs:646–664,696–697`) produce syncs. Delayed old empty-spend refresh can install after replacement has committed/acknowledged reservations, erasing their shared cost and permitting local double-spend. REQ-028/029/031 forbid this within one live server.

Keep the existing tenant synchronization identity non-evictable from before waiting through SQL/install/error/cancellation, releasing ownership on every exit. Reuse retained entry/mutex lifetime and existing bounded refusal. Result-generation checks alone do not prevent obsolete writes under the same server identity; do not merely guard apply or add downstream budget guards/new registries. Deterministic small-capacity proof overlaps sync ownership with eviction pressure, confirms the authority cannot be replaced or acknowledged spend disappear, and confirms cancellation/error lets completed clean entries evict. Add one focused real-PG overlap only if necessary for ordering proof.

### FIND-006-5 — Failed/fenced policy reloads lose their obligation

`snapshots.rs:125–166,199–225`, consumed by `owner.rs:141–145`, drains A's reload before IO. B's notification advances the global generation, fencing A's installation, but only B remains queued. A's old cached policy stays usable indefinitely; transient SQL load failure has the same loss. Listener heartbeat and successful usage refresh do not repair policy, and unrestricted old policy can bypass usage health. Normal concurrent administration can thus permanently suppress a committed configuration/credential/capture change, violating REQ-030 and INV-001.

Preserve each outstanding tenant reload until current-generation installation succeeds; failed/fenced attempts stay recoverable under bounded background ownership. Reuse existing invalidation eviction/cache-miss 503 when no usable policy is established. Correct snapshot/worker ownership once; keep async loading, listener fencing and unrelated healthy availability. No synchronous request fallback, TTL framework or another cache. Existing snapshot test must consume A's queue before B invalidates, reject A's old installation, then prove recovery without another A edit; existing PG seam must prove transient load failure recovery too.

### FIND-006-6 — Drain exits on its first usage write failure

`usage.rs:607–640`, `owner.rs:168–179`, actual `app/server.rs:756–804` shutdown: worker stops, calls drain, one-shot flush logs a quick SQL failure and returns unit. Timeout succeeds and drain reports call-task completion while pending dirty usage remains. Graceful exit loses accepted usage before deadline, contrary to REQ-031/AC-024/026. Explicit later retry in a test is absent from production.

Existing Gateway drain must retry still-uncommitted accounting using replay-safe sync until commit or unchanged shared deadline, with bounded retry waiting and accurate deadline/loss reporting. Distinguish pending writes/releases from already committed active-window state; waiting for wholly clean/empty maps cannot terminate normally. Preserve shared sibling lifecycle budget and accepted deadline loss. Reporting failure without trying recovery is insufficient; durable spool/extended deadline is out of scope. Exercise actual Gateway::drain against PG: first write fails promptly, recovery before deadline commits each contribution once; persistent failure returns by deadline with unsuccessful accounting-drain reporting.

### FIND-006-7 — Stale/full refusal and recovery lack required proof

The existing `pg_invocation_tests.rs:1141–1223` covers cold load/restart/policy replacement, not expiry of a usable view or full pending entries. Inline tests likewise miss `usage.rs:324–337` stale/full branches. REQ-030/AC-024 expressly require failure/boundedness observations. This is a proof gap, not an assertion of runtime misbehavior.

Use focused existing GatewayUsage tests to establish stale baseline and saturated pending state return exact retryable `WYRD_SERVER_503_SERVICE_UNAVAILABLE` without charging; successful background refresh/acknowledgement restores admission. Reuse one existing server/PG seam as needed for no provider dispatch and no synchronous SQL fallback. Avoid real thirty-second sleeps/new harness. Change runtime only for a demonstrated failure, and update evidence honestly.

### FIND-006-2 — Final Agent reductions have no performance closure

Original task lines 365–408,425–427 explicitly leave the three reductions unmeasured. Saved `20261111T150216Z-0626a7b2d` predates them; CPU localization is not elapsed/throughput effect. REQ-033/AC-025 require attributable performance evidence, without promising a gain.

After correctness corrections, record final source/build identity and obtain authorization for the original bounded unprofiled direct-Verify experiment with its four assertion/model control steps, standard repetitions and unchanged fixture/recorder settings. Compare retained references only after establishing method/workload/build comparability; disclose gaps. Retain canonical phases/waits, process CPU, populations/response bytes/completion/drain and all repetitions, reporting gains/regressions/unresolved observations separately. Additional current reference requires bounded authorization; do not rebuild historical baselines or claim sampled CPU shares as measured savings. Existing experiment/report machinery suffices; no new performance framework.

### Shared FIND-005-3 — TASK-006 documentation subset

Add accurate rustdoc for `crates/skald/skald-agent/src/agent.rs:773::from_resolved_parts` (changed lazy-construction/reference invariant); add Panics contracts to `crates/wyrd/wyrd-server/src/components/gateway/snapshots.rs:173::listening` and `usage.rs:674,718,753::{fresh,local_admission_spends_headroom_once_per_tenant,budget_reservations_count_once_across_refreshes}`. Legitimate deletion by functional correction closes a row. TASK-005-R1 owns the other subset; no duplicate editing or style-only round. All new/changed items in this remediation also obey the gate.

## Constraints, acceptance and focused verification

Keep approximate global quotas and memory-only pending usage. Abrupt crash/expired-deadline loss remains accepted. Preserve atomic local headroom, before-dispatch capture reservation, provider errors/retries/deadlines, stream execution-end concurrency release, credentials/permissions/audit/RLS, compact/detail contracts and stored evidence. Keep exact-schema answer/declared Verdict/custom-data/repair/citation validation and bench-default-off activation. No new API, dependency, service, global coordinator, local WAL, stronger durability/SLO, synthetic host load or unrelated optimization.

| Finding | Acceptance observation |
|---|---|
| FIND-006-1 | Prompt export independent of first-read order and clone state; original stored/ref retarget semantics preserved without eager copy. |
| FIND-006-2 | Authorized attributable final-candidate canonical comparison closes required performance evidence or honestly remains incomplete. |
| FIND-006-3 | Old and new persisted limit representations are counted once in both readers during overlap/restart; settlement/expiry/isolation preserved. |
| FIND-006-4 | Active tenant sync authority cannot be evicted/replaced; all completion/cancellation paths release ownership; acknowledged budget baseline remains counted. |
| FIND-006-5 | Failed/fenced reload remains recoverable; obsolete policy cannot serve forever; unrelated healthy tenants remain available. |
| FIND-006-6 | Actual drain retries a transient failure to commit once before deadline; enduring failure reports bounded incomplete drain at deadline. |
| FIND-006-7 | Stale/full exact 503 before charge/dispatch and background recovery are observed. |
| FIND-005-3 (006 subset) | Bounded listed rustdoc gaps closed once, behavior preserved. |

Use owning mise tasks/setup and repository-managed PG. Record and discover exact new/extended selectors before execution. Run narrow Agent construction/projection slice, snapshot/usage unit slice, and focused Gateway PG tests for migration overlap, eviction ordering if needed, reload recovery and actual drain. Retain existing local-admission/reconciliation selectors where the changed owner can affect them; no full SDK/journey aggregate. Append migrations and run affected SQL/migration proof plus tenant/dependency boundaries. Run affected format/lint lanes and `git diff --check`; no broad gate. Performance protocol remains separate from builds/gates and separately authorized. No new helper layout/private control flow/fixture structure is mandated. Later remediation review retains original acceptance plus cumulative candidate and checks closure and affected paths.

## Implementation evidence (R1)

Commits on `change/resource-scaling`: `471f6f181` (FIND-006-1), `0735dae1b`
(FIND-006-5), `36a3e8c66` (FIND-006-4/6/7, 006 rustdoc subset), `d7dcdb466`
(FIND-006-3). Postgres proofs ran through
`scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked ..."`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-006-1: Prompt export independent of first-read order and clone state; stored/ref retarget semantics kept without eager copy | `skald-agent/src/agent.rs`: private `authored_prompt: Arc<Prompt>` set at construction and by `with_prompt`/`try_with_prompt`; `prompt_ref()` derives from it, never from the public runtime `prompt`; the `OnceLock` laziness is unchanged | `wyrd-skald-agent` integration `agent_construction::agent_prompt_reference_ignores_runtime_assignment_in_any_read_order` (both read orders, initialized clone, runtime B over stored A), `agent_with_prompt_overrides_prompt_reference` (retarget B also in the exported model), `construct::agent_named_prompt_reference_survives_runtime_assignment`, `agent_fields::agent_clone_bumps_prompt_arc` (clone shares the Prompt Arc); crate 130/130 | PASS |
| FIND-006-3: old and new persisted limit representations counted once in both readers; settlement, expiry, isolation kept | migration `20261111000001_gateway_usage_overlap.sql` replaces only the read projections of `wyrd.gateway_admit` (windows UNION ALL contribution rows; leases + unexpired `active`); `queries/gateway_admission.rs` `WINDOWS_SQL`/`CONCURRENCY_SQL` add the strict counters and leases; writes and `gateway_settle` unchanged | `wyrd-sql --test integration pg_gateway_usage::previous_and_current_usage_count_once_in_both_readers` (old admit → new view, new sync → old admit, settle/expire changes each once, other tenant reads nothing); `pg_migration::` 23/23 incl. `migrations_apply_and_are_idempotent` | PASS |
| FIND-006-4: an in-flight tenant sync cannot be evicted or replaced; completion and cancellation release it | `components/gateway/usage.rs` `TenantUsage::clean` is false while `Arc::strong_count(&sync) > 1`, so the LRU never evicts a tenant whose sync handle is held, from before the lock wait through SQL/install/error; dropping the future drops the handle | `wyrd-server --lib components::gateway::usage::pg_tests::in_flight_synchronization_pins_its_tenant` (capacity 1: blocked real sync → other tenant's sync/admit refused 503 and the serialization Arc is unchanged; completion and abort each release); fails with the guard removed | PASS |
| FIND-006-5: failed or fenced reload stays owed; obsolete policy cannot serve forever; other tenants stay available | `snapshots.rs` `reload` → `install`: a fenced load requeues the tenant, a failed load evicts its snapshot (cache-miss 503) and requeues; `owner.rs` worker waits `RELOAD_RETRY` (1 s) after a failing pass | `components::gateway::snapshots::tests::reloads_that_do_not_install_stay_owed` (A drained, B invalidates, A's install rejected, A recovers without another edit); `components::gateway::pg_invocation_tests::snapshot_reload_recovers_from_a_transient_load_failure` (pricing table renamed, notify, refusal, restore, recovery) | PASS |
| FIND-006-6: actual drain retries a transient failure to one commit before deadline; enduring failure reports bounded incomplete drain | `usage.rs` `GatewayUsage::drain` (flush, then retry tenants with dirty or pending usage every 250 ms); `owner.rs` `Gateway::drain` runs it under the shared deadline, warns and returns `false` when not persisted; periodic `flush` stays single-pass | `components::gateway::pg_invocation_tests::gateway_drain_retries_usage_writes_until_commit_or_deadline` (two replicas, table renamed: lost replica returns `false` within its 500 ms deadline with repeated sync failures logged; restored mid-drain: `true`, `SUM(requests) = 1`) | PASS |
| FIND-006-7: stale/full exact 503 before charge, background recovery observed | runtime unchanged; proof only | `components::gateway::usage::tests::stale_and_full_usage_refuse_until_background_recovery` (stale view and full pending entries each refuse `WYRD_SERVER_503_SERVICE_UNAVAILABLE`, wake recovery, charge nothing; batch + apply restores admission); existing `pg_invocation_tests::usage_cache_bounds_staleness_restarts_and_policy_changes` kept green | PASS |
| FIND-005-3 (006 subset) | rustdoc on `Agent::from_resolved_parts`, `snapshots::listening` `# Panics`, `usage.rs` `fresh`, `local_admission_spends_headroom_once_per_tenant`, `budget_reservations_count_once_across_refreshes` `# Panics` | source inspection; `mise run fmt` | PASS |

Lanes: `wyrd-server --lib -E 'test(components::gateway::)'` 75/75;
`wyrd-sql --test integration -E 'test(/^pg_gateway_usage::/) | test(/^pg_migration::/)'`
23/23; `wyrd-skald-agent` 130/130; `mise run fmt`; clippy `--all-targets -D warnings`
on `wyrd-server wyrd-sql wyrd-gateway wyrd-testing wyrd-skald-providers wyrd-skald-agent`;
`mise run check:tenant-isolation`; `git diff --check` — all PASS.

Diagnosis (Gateway slice timeout during this remediation):
- **Symptom:** `background_usage_reconciles_retries_and_remote_updates` timed out.
- **Evidence:** its `persist()` helper calls `GatewayUsage::flush` while a NOT VALID
  constraint fault is installed; the first drain-retry loop lived in `flush`.
- **Cause:** retrying inside `flush` never returns while storage keeps failing, and
  the periodic and test callers have no deadline.
- **Fix site:** retry moved to `GatewayUsage::drain`, called only by
  `Gateway::drain` under its deadline; `flush` callers (worker, tests) unchanged.

Non-goals stayed excluded: no new API, dependency, service, global coordinator,
local WAL, durability promise, or synchronous request fallback; quotas stay
approximate and pending usage memory-only; no applied migration edited.

### FIND-006-2 — final-candidate unprofiled direct-Verify run

Authorized by the maintainer for the current code only ("just run a
benchmark on the current code as is"); no historical baseline was rebuilt.

Command: `mise run bench:l1:verification -- --steps direct-verify-c1,direct-verify-c64,direct-verify-model-c1,direct-verify-model-c64`
(the original four assertion/model steps, standard 5 s warmup and 3 x 5 s
repetitions, default fixture and recorder settings, no profile). Build:
`mise run bench:build` at the measured commit.

| Record | Commit | Verdict | Steps |
|---|---|---|---|
| `target/benchmarks/verification/direct-verify/20261111T193105Z-bfd1748c9.json` (current) | `bfd1748c9`, clean | VALID | all four, 3 repetitions each |
| `target/benchmarks/verification/direct-verify/20261111T150216Z-0626a7b2d.json` (retained reference) | `0626a7b2d` | VALID | same four steps and plan |
| `benchmarks/history/verification/direct-verify/20261111T192933Z-bfd1748c9.json` (default two-step history run) | `bfd1748c9`, clean | VALID | assertion c1/c64 |

Medians, with min–max over the three repetitions (reference → current):

| Step | server CPU µs/verification | rate /s | client p50 ms | server p50 ms | overhead p99 ms |
|---|---|---|---|---|---|
| direct-verify-c1 | 137.6 (122.2–178.5) → 106.0 (105.9–122.0) | 9274 (9073–9518) → 10414 (10197–10459) | 0.083 (0.081–0.084) → 0.077 (0.076–0.077) | 0.043 → 0.040 | 0.169 → 0.151 |
| direct-verify-c64 | 263.7 (237.3–319.2) → 212.2 (212.1–228.6) | 68965 (58147–70921) → 87717 (83517–87759) | 0.805 (0.803–0.927) → 0.671 (0.670–0.698) | 0.115 → 0.108 | 0.544 → 0.259 |
| direct-verify-model-c1 | 861.3 (841.2–889.8) → 605.9 (596.7–614.6) | 1402 (1348–1421) → 1940 (1919–1987) | 0.667 (0.660–0.694) → 0.489 (0.484–0.496) | 0.566 → 0.401 | 0.405 → 0.229 |
| direct-verify-model-c64 | 1386.2 (1385.4–1454.4) → 1035.9 (1012.6–1077.0) | 16163 (16090–17054) → 22587 (21898–22815) | 3.671 (3.586–3.745) → 2.703 (2.676–2.783) | 2.37 → 1.71 | 1.508 → 0.798 |

Every case improved, with non-overlapping repetition ranges, in server CPU
per verification, throughput, client p50 and server p50; no regression was
observed. Correctness, refusal and lost-evidence criteria held in every
repetition of both records.

Attribution limit: 231 commits separate `0626a7b2d` and `bfd1748c9`
(including the rebase onto `card-authoring-r1` and the TASK-006 Gateway
work), and the assertion steps, which no Agent reduction touches, also
improved 19–23% in server CPU per verification. The model-step gain (−30%
c1, −25% c64 CPU per verification) therefore exceeds but is not wholly
attributable to Reductions 1–3; the host-load share of the reference is
not recorded in it. Separating the three reductions would need same-source
A/B builds, which this authorization excludes. Status: measured on the final
candidate; attributable per-reduction gain remains unresolved.
