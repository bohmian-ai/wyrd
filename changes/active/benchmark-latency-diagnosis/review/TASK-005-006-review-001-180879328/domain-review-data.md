# Persistent-data and tenant-boundary review

Result: **FAIL**.

Subject: first joint review of TASK-005 and TASK-006, `0c81715f2...180879328` (candidate `180879328ac9dde94d7504855e821980e5db0f18`). Source was not changed. This report covers persistence, replay, aggregation, migration compatibility, and RLS; it does not independently accept benchmark or SDK behavior.

## Authority and source coverage

Read the shared scope map, root and SQL AGENTS rules, applicable task requirements and acceptance scenarios, spec REQ-028–031/AC-023–024/026, architecture Gateway quota enforcement, security posture Postgres tenant binding, and deployment-and-release expand-and-contract rules. CodeGraph was used before source navigation. The actual cumulative SQL/Gateway diff and surrounding owners were inspected.

| Boundary | Producers, authority, consumers inspected | Assessment |
|---|---|---|
| Process-owned contributions | `GatewayLedger::admit`, `reservation`, `settlement`, `Admission::drop`, `Accounted::new`; `GatewayUsage::{admit,settle,batch,apply,sync}` | Absolute windows and reservation identities preserve replay semantics for a retained tenant; eviction can invalidate serialization (DATA-002). |
| Tenant persistence | `sync_gateway_usage`, its seven SQL statements; `TenantConn` caller transaction in `GatewayUsage::sync`; new migration policies; accounting table and reservation unique indexes | Tenant identity comes from typed `TenantConn`, inserts bind it, reads/deletes rely on forced RLS, and caller alone commits. No raw-pool authority or manual tenant filter was introduced. |
| Aggregation and restart | Window/concurrency SQL; common budget ledger `SPEND_SQL`; old admission tables and `gateway_admit`/`gateway_settle` functions | Same-version server aggregation and common budget history are preserved. Old/new request/token/concurrency representations do not meet (DATA-001). |
| Coordination time | `UPSERT_CONCURRENCY_SQL`, `PRUNE_CONCURRENCY_SQL`, `CONCURRENCY_SQL` | Lease expiry derives from `statement_timestamp()` and a bound duration. Domain window/period dates remain caller-owned. |
| Lifecycle and sibling sync producers | `Gateway::refresh_worker`, `drain`; `GatewayUsage::{run,sync_where,flush}`; `GatewayAdministration::{warm,audited}` | Worker and administrative warming both invoke sync. Per-entry mutex serialization is insufficient across eviction and replacement (DATA-002). |
| Proof | `pg_invocation_tests::{background_usage_reconciles_retries_and_remote_updates,usage_cache_bounds_staleness_restarts_and_policy_changes,gateway_ledger_fences_replays_and_settles_unknown_cost}`; usage inline tests | Existing source proofs cover same-version replay, reservation fences, shared totals, same-version restart, and local tenant separation. They do not seed old-version quota rows or overlap sync with eviction/reinsertion. |

Existing-owner comparison: the old `GatewayAdmissions` SQL wrapper owns strict cross-server decisions and cannot simply be queued because its admission result arrives after dispatch and token corrections are deltas. The new absolute contribution mechanism belongs in the existing Gateway process lifecycle and SQL query owner. Common budget-ledger unique indexes are correctly reused; a separate durable queue or registry is unnecessary. The two findings below require completing those owners, not adding another service.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| Persist each process's contribution without overwriting another process | SQL primary keys include tenant/limit/window/server; absolute upserts update only the conflicting identity; reads exclude self | Real-Postgres reconciliation source asserts 15 then 30 tokens across two instances | PASS |
| Uncertain/retried writes must not repeat token/cost/releases | Absolute upserts; reservation-id unique indexes; `ON CONFLICT DO NOTHING`; `sync` applies only after commit | Reconciliation test repeats absolute rows and duplicate reservation using a new entry UUID; ledger remains two rows | PASS for stable retained identity |
| Refresh cannot lose pending usage or double-spend local headroom | `batch` retains pending entries until `apply`; serialized `sync` mutex belongs to an evictable entry | Inline budget proof covers ordinary acknowledgment, not eviction during IO | FAIL: DATA-002 |
| Preserve tenant RLS and commit ownership | Both new tables ENABLE/FORCE RLS with tenant USING/WITH CHECK and migration-login-only operator policy; SQL accepts `&mut TenantConn`; commit is in caller | SQL source plus existing local tenant test; reported isolation gate not rerun | PASS |
| Preserve committed historical usage and use existing migration overlap rules | Migration leaves old tables intact but readers/writers split into disjoint representations | Restart proof seeds only current-version rows | FAIL: DATA-001 |
| Preserve historical budget semantics | `SPEND_SQL` reads original append-only ledger and joins settlement by reservation within tenant; immutable old indexes remain | Existing abandoned reservation and replay proofs retained | PASS |
| TASK-005 compact/details changes must not rewrite accounting authority | SQL changes are TASK-006 governance; common durable reservation documents remain original schema | Cumulative SQL diff and ledger document production | PASS within this domain |

## Proposed findings

### DATA-001 — Upgrade discards the active committed limit baseline

Classification: **REGRESSION / INCORRECT**, TASK-006.

Violated obligations: REQ-031 requires restart to load committed shared usage and preserve committed historical usage using existing deployment migration rules. `architecture/operations/deployment-and-release.md:193–198` requires overlapping writers to emit representations both versions can read and readers to tolerate the old representation. Approximate coordination permits delayed visibility and unflushed loss, not an entire committed representation becoming permanently invisible while it remains current.

Location: `crates/wyrd/wyrd-sql/src/queries/gateway_admission.rs:145–159`; `crates/wyrd/wyrd-sql/migrations/20261111000000_gateway_usage_contributions.sql:10–12,17–57`.

Producer-to-consumer evidence:

1. Existing release `wyrd.gateway_admit` (`20261023000001_gateway_admission_statements.sql:59–78,108–124`) reads/charges `gateway_limit_windows` and `gateway_call_leases`; `gateway_settle` corrects/releases those same rows.
2. New migration creates empty `gateway_usage_windows` and `gateway_usage_concurrency`, leaving old functions unchanged. Its comment that old tables remain for rolling deployment supplies no data handoff or cross-version observation.
3. New `WINDOWS_SQL` and `CONCURRENCY_SQL` read only new tables. `GatewayUsage::sync` installs that empty result as a usable baseline; `admit` then treats the old committed counters and unexpired calls as zero.
4. Conversely, an old replica cannot observe any new server's persisted request/token/concurrency contributions. Common budget ledger entries remain visible to both, so the defect concerns limit windows/concurrency, not budget history.

Reachable scenario: old release commits one active call under concurrency cap 1 and/or exhausts the current request/token window. Apply the additive migration and start a new replica before the old call/window ends. Its first successful background refresh reports no old usage and allows another call. Even a full replacement restart resets already-committed current-minute usage. Existing same-version restart tests cannot detect this.

Consequence/materiality: upgrading a supported deployment drops its still-applicable committed quota baseline; during overlap the two versions enforce independent partitions rather than sharing persisted contributions. This is a required data/migration boundary, independently of approximate global ceiling semantics.

Smallest correction boundary: extend the existing Gateway SQL admission/shared-usage readers and additive migration handling so both supported representations contribute once during overlap, and existing settled/expired rows stop contributing at their original boundaries. Reuse the existing old windows/leases plus new absolute per-process rows and the common ledger; do not copy mutable old totals once, overwrite process rows, change historical pricing, add coordination services, or introduce a rollout protocol. The old reader must observe persisted new contributions and the new reader must include still-applicable old rows under existing tenant RLS. This stays within approved approximate enforcement; it does not restore strict global admission.

Focused closure proof: real Postgres seeds charges through the old SQL function, then loads a new `GatewayUsage` baseline and proves the current window/unexpired call remain counted. Exercise new persisted charges through the old reader too; apply old settlement/expiry and refresh to prove release without double-counting. Use the existing Gateway Postgres fixture and narrow exact selectors, not a multi-process release benchmark or broad journey suite.

### DATA-002 — Eviction replaces the synchronization authority while a refresh is in flight

Classification: **INCORRECT**, TASK-006.

Violated obligations: REQ-028/029 local admission cannot double-spend local headroom; REQ-030 requires existing generation-fencing behavior; REQ-031 refresh must not erase pending/committed ownership. This concerns one live server, not accepted inter-server approximation.

Location: `crates/wyrd/wyrd-server/src/components/gateway/usage.rs:213–219,284–299,451–455,521–525,558–569,591–598`.

Producer-to-consumer evidence:

- `TenantUsage::clean` tests dirty/pending/maps but has no in-flight synchronization condition. A cold tenant's empty first batch or an idle tenant with no local charges satisfies `clean`, including while SQL is pending.
- `retain` evicts a clean LRU tenant and inserts a fresh `TenantUsage`, including a new `sync` mutex. `sync` clones the previous mutex, awaits it, and later retrieves state and applies results by tenant key only, never checking that the retained entry still owns that mutex/generation.
- `apply` unconditionally replaces `usage.shared`; `admit` computes unacknowledged cost as `local.cost.saturating_sub(local.acked)`. Once a newer successful sync acknowledges a reservation, that reservation relies on its cost being present in shared spend.
- Production has multiple sync producers: the background `run` and successful public administrative writes through `GatewayAdministration::audited` → `warm` (`service.rs:646–664,696–697`). Worker pass serialization therefore does not prevent overlapping refreshes using different replacement mutexes.

Reachable scenario: on a server at its retained-tenant bound, a cold/idle T begins an administrative or worker refresh and obtains an empty budget-spend result, with transaction completion delayed. Other tenant traffic makes clean T the LRU and evicts it. A later successful administrative write for T reinserts it and warms a fresh entry/mutex; an ordinary call reserves budget and a subsequent sync commits/acknowledges it. The original refresh then completes and applies its earlier empty shared-spend view to the replacement entry. For the acknowledged reservation, both shared spend and unacknowledged local spend are now zero, so T can spend that same local headroom again before the next refresh. The original completion can also modify the replacement dirty flag after a failure.

Consequence/materiality: supported multi-tenant cache churn plus delayed SQL lets a stale completion invalidate one server's own committed reservation baseline. It violates the explicit no-local-double-spend invariant; the 4096-entry bound is an intended operating boundary, not an arbitrary malformed input.

Smallest correction boundary: keep the existing tenant synchronization identity stable and non-evictable from before waiting for its sync lock through completion or cancellation; make eviction/installation respect that ownership. Reuse the existing retained entry and mutex/lifecycle rather than adding another scheduler, global registry, or consumer-side budget guard. A generation-only discard must also prevent old writes from committing after replacement writes under the same process identity, so merely checking `apply` is insufficient. Pinning the authority through the existing synchronization lifetime closes both ordering and stale-installation paths while preserving bounded refusal.

Focused closure proof: deterministic in-process test with a small existing cache capacity exercises retain → in-flight sync ownership → attempted eviction → reinsert/late completion, proving the active authority cannot be replaced and acknowledged budget spend cannot disappear. Include cancellation/error release so clean completed entries remain evictable. One focused real-Postgres overlap test is appropriate if needed to prove write ordering; no new harness/service or performance run is required.

## Verification limits

No tests, builds, benchmark, or profiles were run. Existing proof source and task-reported successful commands were inspected, with their limits stated above. The findings are static producer-to-consumer traces requiring independent validation. Abrupt process death and expired shutdown loss remain accepted; strict global quotas and crash-safe local persistence were not demanded.
