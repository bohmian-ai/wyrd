# Orchestrator verification

Candidate before and after execution: `7cda4e93159ab328de2a74068d5e1978a2590818`.
Source remained unchanged. The only new files are assessment reports in this directory.

## Independent focused rerun

```bash
WYRD_LOG=info,wyrd_server::verification=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_groups::pending_groups_debounce_and_stop_after_handoff)"'
```

Result: exit 0. Repository-managed isolated Postgres provisioned successfully. The owning setup ran the Wyrd and Vala migration-idempotence tests, one passing test each. Targeted server test selected exactly one test; one passed, 275 skipped. Focused test duration 4.480 s (summary 4.481 s), nextest run `c3092534-4f3b-4f26-a2f5-4bd1db09517f`.

This rerun proves the current C13 test passes; whether its assertions establish every task obligation remains an independent source/acceptance question. It is not evidence that any suspected unasserted branch is correct.

## Other supplied verification

The candidate task appendix records C1–C13 and static/owner checks with selected counts, diagnoses and feature sets. Discovery reviewers independently assess the source and evidence, including assertion changes. Root did not rerun all SDK journeys or broad gates; no runtime source was changed by this assessment. Benchmarking REQ-015/AC-009 remains explicitly deferred by approved authority.

## Subsequent owner-approved document revision

After the immutable candidate review completed, the owner revised failure handling
and capped group_end.after at write time. The working specification is now approved
revision 18; architecture prose, R1 task and verdict amendment were updated.
The original candidate commit remains 7cda4e93159ab328de2a74068d5e1978a2590818.
No runtime code, schema implementation, fixture or test was edited during this
document revision. Independent readiness/system/cold-rehearsal review passed;
its artifact validator and git diff --check passed. No executable implementation
tests/builds were rerun. Earlier C13 results apply to the original candidate.
