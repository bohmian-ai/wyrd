# Data, durability, and concurrency domain review

## Subject and boundary

Base `f8811ac5035c3aa165d34c38992f9889b3c9081f`; candidate `81d2221b4d3b1a9fb6bd36c591d421385cc8c67d`; approved `spec.md` revision 36; original `TASK-005-production-drift-verifier.md`. This is the complete cumulative candidate, including round 1 remediation.

Reviewed registration and fitted-baseline state, PostgreSQL claims and fences, fit permits and cancellation, decoded Parquet bounds, scheduled query stream settlement, verification result publication, and Forge namespace admission. Authorities: `AGENTS.md` sections 5–12; `architecture/agent-rules.md` transaction/RLS/audit rules; `architecture/wyrd-design.md` verification and client/server boundaries; `architecture/bifrost-design.md` query and Forge reliability rules; `architecture/references/domain/{drift-monitoring,olap-serving,datafusion,analytical-operations-reliability}.md`; approved specification `REQ-073`, `REQ-078`, `REQ-080`, `REQ-085`, `REQ-146`, `REQ-152` and task scenarios 2–6.

## Coverage

| Boundary | Source and evidence inspected | Result |
|---|---|---|
| Atomic pending registration, exact Data identity, status | `components/cards/{resolve,service}.rs`, `drift_baselines.rs`, migration `20260601000032_drift_baselines.sql`, `pg_drift_baselines.rs`, task evidence | PASS |
| PostgreSQL time, `SKIP LOCKED` claim, lease token fencing, retry/reclaim | `drift_baselines.rs`, migration, SQL integration cases | PASS |
| Shared capacity and fit artifact bounds | `verification/{mod,fitter,permits}.rs`, `vala-drift` fit entry and PSI/SPC fit loops, focused unit/runtime evidence | FAIL (`DATA-R2-001`) |
| Query streaming and result durability | `query/scheduled.rs`, `verification/{drift,runner,publisher,results}.rs`, runtime/journey evidence | PASS |
| Forge maintenance namespace | `vala-bifrost-redux/src/namespaces.rs`, `vala-sql` namespace migration and `ForgeTaskTableIdentity`, compaction admission tests | PASS |

## Material finding

### DATA-R2-001 — VIOLATION: baseline fitting cannot honor the shutdown and execution deadline once scoring begins

- **Obligation:** `REQ-146` requires shutdown to stop new claims, wait 30 seconds, then cancel remaining work and release or expire its fenced lease. Round 1 `FIND-TASK-005-9` additionally requires the blocking decode/fit operation to observe timeout and shutdown before its lease is released.
- **Location:** `crates/wyrd/wyrd-server/src/verification/fitter.rs:215-232,278-294`; `crates/vala/vala-drift/src/baseline/mod.rs:34-72`; `crates/vala/vala-drift/src/{psi,spc}/mod.rs` fit loops; `crates/wyrd/wyrd-server/src/verification/mod.rs:190-213`.
- **Evidence:** `fit_next` races `self.fit` with shutdown and `execution_timeout`, then cancels a token and awaits `fit.await`. The spawned blocking closure checks that token during Parquet decode and once immediately before `vala_drift::fit_baseline`, but `fit_baseline` and its per-feature collection/scoring loops receive no cancellation token. If shutdown or timeout occurs after that check, the blocking fit keeps running and `fit_next` cannot reach `queue.release`/`fail` until it finishes. The runtime supervisor waits for the fitter capability to exit. A tenant-authored, within-budget Parquet baseline with many feature fits can keep that work active beyond the 30-second drain or five-minute execution deadline. The lease can then expire and be reclaimed while the original blocking fit still uses CPU and memory.
- **Consequence:** The server can exceed its shutdown budget; execution timeout is not a bound on baseline work; two processes can fit the same row concurrently after lease expiry despite the attempt fence protecting final settlement.
- **Testable correction:** Make the existing `vala-drift` fit loops cooperatively observe the fitter cancellation during expensive per-feature work, using the current cancellation token and without changing fitted-profile semantics. Preserve the token-fenced SQL transition. Prove with a controlled fit that cancellation after decode stops the blocking computation before fenced release/timeout settlement; the current `cancelled_decode_returns_before_fitting` test only covers cancellation before decoding.

## Verification limits

I inspected source and named test assertions; I did not rerun the broad lanes. The task records green format/lint, Vala/SQL/Wyrd/Bifrost, SDK journeys, storage matrix, codegen, boundary checks, and exact named commands. Those runs do not exercise cancellation after `fit_baseline` begins. Forge's Rust allowlist has a unit guard against the Bifrost namespace enum, while the SQL migration explicitly widens both relevant CHECK constraints; no separate SQL constraint drift guard is claimed.

**Overall: FAIL**
