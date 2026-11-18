# Verification evidence and limits

Candidate source is the clean detached checkout `/tmp/wyrd-task012-review-5c9808411`, HEAD `5c980841140721cfaa1945fba1057675805ca77a`. Original worktree HEAD matches; its preexisting Oracle source modification and skill/workflow modifications are excluded. The Oracle modification removes the `test-support` gates and updates its field documentation. No reviewer applies that correction to the candidate.

Fresh orchestrator check: `git -C /tmp/wyrd-task012-review-5c9808411 diff --check 0ae00f429...5c9808411` exited 0. Range contains 64 commits.

No new Cargo command was launched: an existing external sequential lane runner is using the shared build target. `TESTING.md` prohibits overlapping Cargo builds. Reviewers inspect assertions, source, candidate-recorded evidence and available external logs. Benchmarks remain prohibited by the user's current instruction, and are not run. Required missing L1 qualification is not waived.

## Existing external runner observations

Logs: `/tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-worktrees-github-verifier-pipelines-task-004/30723aa4-ef06-4da3-8bfa-ca4c57262da5/scratchpad/lanes7/`.

Observed summary during review:

| Command | External observation | Candidate binding |
|---|---|---|
| mise run fmt:check | exit 0 | Live worktree, not a clean-candidate run |
| mise run lints | exit 101 | Exact E0609 at oracle/mod.rs:2293 agrees with immutable candidate's conditional field/unconditional access |
| mise run test:verification:sandbox | exit 0; owning summary 34 passed | Historical/live execution; dirty source timing not proven |
| mise run test:bifrost:integration:redux | exit 0; owning summary 1021 passed | Historical/live execution; dirty source timing not proven |
| mise run test:bifrost:integration:server | exit 0; owning summary 126 passed | Historical/live execution; dirty source timing not proven |
| mise run test:gateway:native | exit 0; Rust 17 passed, Python 8 passed | Live source includes excluded Oracle fix; not candidate build evidence |
| Four Bifrost journey lanes and three named regressions | Runner not complete at initial evidence capture | No assumed result; see final verdict for review cutoff |

The lints task first checks the workspace feature union and then the shipped server alone. The log reports `error[E0609]: no field resources on type &oracle::Oracle` at the query start access. Test-support-enabled success cannot prove the shipped feature set.

The runner is owned by another session and is not terminated, redirected or modified. Its later results require identity/source reconciliation before acceptance. No failure is dismissed as preexisting. No test result is inferred from a name, a launch, or a partially written log.

Behavioral acceptance, historical TDD compliance, and measured performance are distinct. Existing task records disclose production-first/mutation-only histories; review cannot repair history or convert those checks to genuine pre-production RED.

Later cutoff update: the external `test:bifrost:journey:server` summary reached exit 0 and the runner began `test:bifrost:journey:oracle`. This remains mutable-worktree evidence, not a repaired immutable candidate. Both reviewed checkout HEAD and original HEAD still equal the candidate, and the detached source remains clean.
