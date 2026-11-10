# R1 review verification

Candidate `f74a576348cb9805ec7a2ef0e1c0bde3dd07928d`, prior `384a0def73ed6eac4041dc1e428dc30e981514fe`. Orchestrator alone runs Cargo sequentially. No source changes.

## Fresh exact engine selection

```bash
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::waiting_checks_spend_only_after_admission) | test(=blocks::engine::tests::capacity_refusal_lets_dispatched_checks_settle) | test(=blocks::engine::tests::interrupted_agent_checks_keep_their_invocation) | test(=blocks::engine::tests::agent_budget_counts_retries_and_is_never_replenished) | test(=blocks::engine::tests::capacity_refusal_lets_dispatched_work_settle) | test(=blocks::engine::tests::waiting_blocks_release_unrelated_permits) | test(=blocks::engine::tests::speculative_steps_withhold_content_until_admitted)'
```

PASS: 7 exact tests selected and passed, 66 excluded; nextest run `4227e917-4303-48ad-a0f9-1bdcdb73079f`, 0.019s test summary. Includes all three new check regressions and existing budget/permit/Busy/speculation proof. Source adequacy independently reviewed; green result is not a conclusion by itself.

`git diff --check 384a0def7 f74a57634`: PASS. Prior candidate is an ancestor of candidate; no rebasing. Implementation source/authority pinned in subject.

## Evidence limits

R1 contains supplied RED/GREEN, exact Rust/MCP/Python/TS runs and owner-check claims; reviewers inspect assertion bodies. Historical original TDD exceptions remain disclosed, not manufactured RED. Fresh real-Postgres Rust speculative journey result follows. No broad aggregate, credentialed/platform lane, or full original 93-file rediscovery is planned for this remediation review.

## Fresh public speculative journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=verifier_orchestration::speculative_disposition_controls_consumer_admission)"'
```

PASS: repository Postgres healthy and both migration-readiness tests selected one and passed. Nextest run `883885bf-d2e3-4035-b021-36a052fd6165`: 1 exact journey selected and passed, 168 excluded, 14.487s. Build 7.73s. This runs the changed HeldUpstream-backed public milestone journey. Source proof and original assertion preservation remain independently audited.
