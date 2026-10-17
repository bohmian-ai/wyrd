# Focused follow-up: supported-platform journey proof

Result: **RESOLVED**. The production path is not shown to have a new Linux dependency. The required new restart journey does have a reachable, deterministic macOS fixture failure. This is a bounded task-proof violation, despite the helper itself predating the candidate.

## Immutable subject and scope

- Base: `bc3bf25e3f78b0c429110350a3de4d58290e632f`.
- Candidate: `5cfb0a3ac61434256fa97a86f3506b124158780c` (HEAD unchanged at inspection).
- Approved spec: `changes/active/local-dev-setup/spec.md`, revision 1.
- Original task: `changes/active/local-dev-setup/tasks/TASK-001-guided-local-development.md`.
- Scope: only the conflicting assessment of INV-006 and the newly added P8 journey. Other discovery findings were not reassessed.

Read the cumulative range, the behavior report's INV-006 row and proof-limit discussion, the invariant report's supported-platform row, and the standards report's supported-target discussion. Read the complete relevant `LocalDev` constructor, command/start/start_with/stop methods, `free_bind`, P8 body and sibling callers. Compared the helper with the base version. Consulted AGENTS platform, journey and completion rules; agent rules; spec-driven-development; testing-workflows; spec INV-006/AC-006; and task Scenario 4/P8. Checked the owning CLI test registration and mise selection.

## Source trace and resolution

1. `tests/cli.rs:40` includes `pg_tests` without a Linux-only exclusion. The candidate adds the exact P8 test to `test:local-dev:journey` in `mise.toml`.
2. The new `pg_tests::server_dev_saved_settings_survive_working_directory_change` starts at `crates/wyrd/wyrd-cli/tests/pg_tests.rs:700`; its first operation at line 701 is `LocalDev::new().await`.
3. `LocalDev::new` creates the managed database and installation fixture, then calls `free_bind()` for all three selected listeners at lines 94–96. No test/CLI input can bypass these calls.
4. `free_bind` at lines 310–330 unconditionally reads `/proc/sys/net/ipv4/ip_local_port_range` at line 312 and immediately `expect`s success at line 313. macOS does not provide this Linux procfs file. Therefore P8 panics during fixture construction, before its missing-input, restart, identity or data-retention assertions run.
5. The complete helper is unchanged from the base. Its other consumers are the existing first-run, partial-failure, selected-host and managed-secret journeys (constructor calls at lines 343, 502, 630, 800 and 852). These are one shared fixture cause, not separate defects. P2/P9 also receive material new assertions in this candidate.
6. There is no evidence here that the shipped `server dev` command reads this file. Unix terminal handling covers both supported OS families. A platform source check restricted to changed production code can therefore pass while the task's required portable proof still fails.

The unchanged-helper observation narrows the classification; it does not discharge the explicit new proof obligation. Spec INV-006 couples supported-platform workflow behavior to journeys and forbids copying platform-specific fixture/process assumptions. The original task specifically requires P8 and explicitly warns against importing `/proc` assumptions into changed runtime/test code. Reusing a fixture is normally preferred, but this reachable dependency makes the newly required proof unusable on one mandated OS. This is not a request to clean unrelated fixtures or build a four-target CI system.

## Proposed finding FUP-001

- **Classification:** VIOLATION (required supported-platform journey proof); not a demonstrated production-runtime regression.
- **Obligation:** spec INV-006 and AC-006; task Scenario 4/P8 and its portable-fixture constraint; AGENTS §2 supported platforms and §11 required journey proof.
- **Changed location:** `crates/wyrd/wyrd-cli/tests/pg_tests.rs:701` (new mandatory journey reaches the incompatible dependency).
- **Shared correction boundary:** the same file's `LocalDev::new` listener selection and `free_bind` at lines 310–330.
- **Observable consequence:** on macOS, the exact required P8 invocation panics before exercising the new user-visible saved-settings/restart behavior. Linux results do not establish that proof on macOS.
- **Smallest correction:** remove the Linux procfs dependency from this shared test-fixture listener selection. Reuse ordinary standard-library loopback binding/address discovery, the mechanism already used by the installed Python fixture's `local_listener` (`sdks/wyrd-sdk-python/tests/integration/test_local_development.py:311–315`) and other CLI fixtures. Preserve distinct selected listeners, saved listener identity across restart, child cleanup and every behavioral assertion. The current helper already relinquishes its listener before startup; do not claim perpetual port reservation or add a production listener-inheritance API. Do not add OS-specific substitutes, target exclusions, retries/timeouts to mask collisions, a new dependency or another harness. Retire the obsolete Linux-range-specific documentation/imports when the mechanism is removed.
- **Focused closure proof:** run the task's exact P8 command in the managed Postgres environment on macOS and Linux, retaining its existing assertions:

  ```sh
  mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_saved_settings_survive_working_directory_change)'
  ```

  Because the correction is shared, run the existing exact P2/P9 selectors as focused regression proof, and the applicable Rust format/lint checks. No new product journey, broad aggregate or full architecture matrix is prescribed. Unavailable macOS execution must remain an explicit evidence limit rather than a green claim.

## Verification limits

Source review only: no tests, Cargo, builds or platform execution were run. No claim is made that a four-target matrix was executed. No implementation files, task authorities or other reports were changed. The follow-up resolves the classification conflict and forwards FUP-001 for the required independent Ponytail validation; it does not assign a final `FIND-*` ID.
