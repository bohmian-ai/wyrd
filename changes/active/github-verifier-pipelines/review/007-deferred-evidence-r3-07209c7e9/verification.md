# Verification

Candidate `07209c7e9af1664a30bf548e64be08937c4f88df`; prior `74fc2190e255a3e3cf7515091ff45ab86a288f6b`.

Fresh commands observed by orchestrator:

- `git rev-parse HEAD`: exact candidate; initial worktree clean.
- `git merge-base --is-ancestor 74fc2190e255a3e3cf7515091ff45ab86a288f6b 07209c7e9af1664a30bf548e64be08937c4f88df`: exit 0; no patch-correspondence issue.
- `git diff --check 74fc2190e255a3e3cf7515091ff45ab86a288f6b 07209c7e9af1664a30bf548e64be08937c4f88df`: exit 0.
- `mise exec -- cargo fmt --all -- --check`: exit 0.
- `mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=components::verification::service::evidence::tests::a_lost_race_replays_the_first_acceptance)'`: exit 0; **1 passed, 631 skipped**, one binary. This proves the assertions in that selected unit, not unexecuted service reconciliation branches.

Connected tests were not attempted: Docker is unavailable by the current request. R2 remediation Evidence records the changed Artifact selector passing and the deferred-evidence module 9/9 passing, plus custody/sequential replay cases and focused Clippy. These remain implementation-recorded results, separately assessed from source and fresh unit execution. Prior R2 review lint/format and earlier connected/cross-tier evidence carry forward only for unchanged scope. No broad gates, SDK journeys, benchmarks or generated-contract checks were rerun for this proof-only diff. No source or test modifications, Git configuration, commit or push.
