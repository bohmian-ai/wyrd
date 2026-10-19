# Independent diagnostician reports (verbatim, read-only agents, given only command, trace and diff)

## Report A: TS staging test fails only in its full file (support/server.ts provider scope)

The cause is fixture caching in the test harness. The server and the outbox are behaving as designed.

Code-level cause. Vitest 4.0.16 caches file-scoped fixtures by fixture name for each file. The resolver at node_modules/.pnpm/node_modules/@vitest/runner/dist/index.js:213-215 returns fileContext[fixture.prop] if a value already exists under that name. In sdks/wyrd-sdk-ts/wyrd/tests/support/server.ts:150-157, the provider fixture is always scope: "file". That holds even when perTest: true. verify-in-real-time.test.ts builds two extended tests from serverTest: test at line 38 and stagingTest at line 424. Both declare a fixture named provider. When the whole file runs, the first test creates the file's provider with the instant judgeReply(AGREES) judge. stagingTest then gets that cached receiver, so its counting, delayed judge never starts. The judgments still pass instantly through the shared receiver, but judgeArrivals stays 0 and awaitJudgeCalls(1) times out. Run alone, stagingTest is the first to create provider, so it passes.

Fix site. sdks/wyrd-sdk-ts/wyrd/tests/support/server.ts:157: change the provider's scope to { scope: options.perTest ? "test" : "file" }, matching the server fixture at line 171. Alternatively move stagingTest into its own file.

Other tests affected: only perTest users get a fresh receiver per test: integration/verify-in-real-time.test.ts (all 20; the one at line 149 reads provider and would see only its own requests, which is safer), integration/verifier-building-blocks.test.ts:182, integration/scheduled-drift-alerts-operator.test.ts:62, and indirectly support/assistant.ts:113 and support/observed-service.ts:17 when called with perTest. Non-perTest tests are unchanged. Trace noise: setup audits overflowing the 6-item outbox are dropped by design and do not cause the failure.

## Report B: every_tenant_is_claimed_in_the_first_round (claimed=64) and two_hundred_released_runs_complete_on_their_first_attempt (timeout)

Both failures come from one deliberate change, the new execution_slots cap. The production code is doing what the task asks; the two tests still expect the old unlimited behaviour.

Cause. RuntimeLimits::execution_slots defaults to 64 (crates/wyrd/wyrd-server/src/verification/mod.rs:105), passed to ClaimLoop at runner.rs:234. The claim loop stops claiming once spawned.len() >= self.active, before a round (claims.rs:157) and inside a round (claims.rs:238). Both tests use Harness::limits() (pg_verification_runtime.rs:117-126), which takes the default 64.
1. every_tenant_is_claimed_in_the_first_round: with the script held, the first round claims 64 of 70 due tenants (log due=70 claimed=64); the last 6 were most likely claimed after the 20s execution_timeout freed slots, letting the 70-entered wait pass before the assertion at 2702 failed.
2. two_hundred_released_runs_complete_on_their_first_attempt: only 64 held runs can run at once; the rest are reached only through ~20s timeout cycles, so the 60s WAIT runs out, and retried runs would end with attempts > 1.

Production or test? The test expectations are wrong. TASK-003 Scenario 1 requires "the active-work bound is enforced where the runner claims work (runner.rs:227, claims.rs:140–165)". Holding more than 64 runs at once in one process can no longer happen.

Fix site: the tests in crates/wyrd/wyrd-server/tests/integration/pg_verification_runtime.rs: spawn with RuntimeLimits { execution_slots: 70, ..Harness::limits() } at 2699, and execution_slots: 200 at 2836 (or keep 64 and drop the all-200-held wait). Optionally add a test where tenants outnumber slots and assert claimed == execution_slots.

Other callers: FLOOD + 1 = 61 (line 1355) and the 22-run wait (line 1222) are under 64 and unaffected. OperatorWorker passes operator_global_permits (16) as its own cap (operators.rs:188), already enforced by the global permit gate. Nothing else sets execution_slots.

## Not independently diagnosed

The Rust journey admission diagnosis (task evidence Diagnosis 1) had a diagnostician before the session was compacted; its verbatim text was not retained. Diagnoses 2 (Python neighbor baseline/key) and 4 (clippy large_futures) did not touch timing, retries or assertions, so no diagnostician was spawned.
