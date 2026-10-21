# Independent orchestrator verification

Immutable candidate before/after tests: `3d0b301c51cfe6e50cbbde41b8b2ee3dfc2e9fb4`.
Review reports only; no source/spec/task/test/fixture/generated edits. All tests
used mise; PostgreSQL-backed commands ran strictly sequentially.

## One-year contract boundary

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::trigger::tests::group_end_decodes_its_duration)'
```

Exit 0; exactly one selected, one passed, 765 skipped; test 0.003s, summary
0.005s. Nextest run `9446799d-b559-4c58-8b01-a8f53bd16f1a`.

## Pending scheduling and SQL snapshot/settlement

```bash
WYRD_LOG=info,wyrd_server::verification=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_groups::pending_groups_debounce_and_stop_after_handoff)" && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(=pg_verifier_executions::group_snapshots_are_idempotent_and_react_in_settlement_order)"'
```

Exit 0. Repository-managed isolated Postgres setup succeeded. Wyrd and Vala
migration-idempotence setup tests selected/passed one each (0.344s, 0.200s), runs
`243ac80a-efa0-4fa3-b136-17413e1267ae` and
`ed2c41a3-dc0d-4a2e-988f-c898249907c2`.

- Server pending test: exactly one selected/passed, 275 skipped; 7.774s, summary
  7.775s; run `1c47de0a-ee31-4eb4-89c2-ce83929c44af`.
- SQL snapshot/settlement test: exactly one selected/passed, 103 skipped; 0.844s,
  summary 0.845s; run `8b6a1cb3-a468-4484-b53b-8b0e6e9e1523`.

These results establish current assertions execute and pass. Independent source
review must determine their acceptance coverage; green tests alone do not close
an unasserted invariant.

## Supplied evidence and limits

R1 implementation appendix records focused SDK/MCP/registration and static
checks, decoder/fixture diagnoses and results, including a sequential rerun of
formerly colliding Postgres lanes. These are supplied evidence, not root runs.
Root did not repeat full SDK suites, broad gates, live providers, deployment
faults, host stress or benchmarking. REQ-015/AC-009 are intentionally pending.
No performance result is claimed.

## Additional sequential domain proof

After root's successful P1/P2/P7, the SQL/data reviewer was authorized to run
one managed-Postgres session; it completed and shut down before root began the
owner-grant journey. `domain-review-data.md` records the exact command prefix,
actual-schema CHECK clone, rollback, fixed-date UTC/DST expressions and results.
This is coordinated reviewer-executed proof, not a root rerun or a public exploit.
The sole production refusal writer currently emits the correct NULL shape.

## Existing owner-grant journey

```bash
WYRD_LOG=info,wyrd_server::verification=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_reads_need_the_owner_grant)"'
```

First run: exit 100; exactly one test selected, zero passed/one failed, 130 skipped;
test 109.087s (summary 109.093s), run
`34525715-aee3-4418-8f38-93be7cacc5e3`. Setup migration tests passed one each.
The returned tool output was truncated by the large failure metrics dump; it
contained `wyrd_verification_schedule_ticks_total{outcome="group_errored"} = 1`.
This result alone is not used to infer the precise failure diagnosis. The same
traced focused command is being rerun with full output captured outside the repo
at `/tmp/wyrd-r1-owner-grant-3d0b301c5.log` to preserve the actual assertion and
preceding trace. No code/timeout/expectation is being changed.

### Full-trace repeat result

The same command with output redirected to the temporary capture completed
exit 100: exactly one selected, zero passed/one failed, 130 skipped; test
109.997s, summary 110.003s; nextest run
`091a7dc6-1d46-424c-8d68-436fa0adb007`. Setup migration tests again passed one each.
The full trace was read before diagnosis:

- At capture line 80 the scheduler logged `group owner refused; recorded a
  terminal errored execution`, group `guarded`, with existing error
  `WYRD_PERMISSION_403_DENIED_RBAC` (missing BifrostQuery/Read).
- Capture lines 308–309 record the actual assertion at
  `sdks/wyrd-sdk-rust/tests/integration/support/mod.rs:409`: the group binding
  `completed no new execution within 90s`.
- The same run's metric was `group_errored = 1`; subsequent runner rounds had
  no runnable work. No completion was expected after consumed denial without
  fresh source activity under revision 18. Follow-up source tracing found no
  such activity between the role swaps and final completed-result wait.

This confirms the observed failure matches the source-supported outdated journey
expectation, not the original failed-publication/snapshot semantics. Correct
production terminal denial is preserved; a test fix must add real source activity
for each later authorization phase and inspect terminal error status. The full
raw log is temporary runtime evidence outside the repository; no raw log/source
archive is stored in this report directory. No test/assertion/timeout was edited.
