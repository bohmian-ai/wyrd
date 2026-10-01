# Verification run by the orchestrator

## `mise run gate` (once, as integrated proof)
- Tree: worktree HEAD 78bd1049b at start, ddb214d17 at end. `git diff --stat 990803fc0 ddb214d17 -- . ':!changes'` is
  empty: only changes/active/opitimization-and-benchmarks spec commits landed during the run, so the code under test
  equals the candidate 990803fc0.
- Result: **exit 100 (FAIL)** after 888.5 s. `test:wyrd` failed; mise then stopped the remaining lanes, so the
  bifrost gate, gateway, identity, Python/TypeScript integration, codegen, docs and check lanes were not all
  observed to completion. Lanes observed passing in the log before abort: fmt:check, lints start, db migrations,
  test:bifrost:unit:python, the gateway native/vault/surfaces journeys, codegen regen start (no drift result
  recorded), py format/lints/typecheck invocations; the `test:wyrd` family ran 2244 tests: 2242 passed, 2 failed.
- Failures (deterministic; reproduced alone with the Postgres wrapper and WYRD_LOG=info, exit 100):
  - `wyrd-server::platform_admin_e2e an_operator_initializes_the_deployment_through_the_shipped_binary`
    panics at platform_admin_e2e.rs:238: `init failed: status=Some(70) ... this deployment is already initialized`.
  - `wyrd-server::platform_admin_e2e an_operator_recovers_from_losing_every_platform_credential` (same init step, :313).
- Orchestrator observation (handed to ponytail-rev for validation, not self-validated): in this range
  (commit 4ae6a1992) `scripts/postgres/with-test-postgres.sh` now exports `WYRD_PLATFORM_DATABASE_URL` pointing at
  the lane database `/wyrd`, and the `wyrd-server init` subcommand builds its operator pool from that platform DSN
  (`main.rs::operator_pool` -> `ResolvedDsns::from_env`). The test helper `operator_command`
  (platform_admin_e2e.rs:201-210) swaps only `WYRD_DATABASE_URL` to the per-test fixture database, so the child
  `init` writes to the shared lane database instead of the test's fixture.
- Logs: scratchpad/task004-gate-saved.log, scratchpad/task004-platform.log.

## Not run (owner constraints)
- Bifrost capacity benchmark (standard/heavy): forbidden by the owner; recorded task evidence used.
- No other lanes were re-run because the gate is red at a deterministic failure; a rerun cannot turn it green.
