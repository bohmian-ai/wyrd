# Focused verification

Subject: eeb3bbd4f891ae88b00ce1979810eb2a11d04871...c78ad7b5241fbbfb54e85e2390c6a694787debfc. Source matches worktree HEAD 3139c08cb0b7c0194583a66af3ab31bef28dc14f.

## Executed during review

Command:

```sh
mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=authority::tests::snapshots_serve_only_while_fresh_and_current) | test(=authority::tests::requests_never_extend_an_unconfirmed_snapshot) | test(=authority::tests::a_stream_keeps_only_unchanged_authority)'
```

Exit 0. Nextest run 199d0eea-aafa-4312-a942-1ec910840ffa, default profile: 3 tests run, 3 passed, 128 skipped. Cached build completed in 0.19 seconds. The assertions cover sequential freshness and stream authority behavior; this run does not establish concurrent notification/reload correctness.

## Environment-limited attempt

Command:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_browser_sessions::scoped_invitations_enroll_exactly_their_assignments) | test(=pg_principal_roles::binding_writes_require_tenant_administration)"'
```

Exit 1 before migration or test execution. Docker API access at /home/thorrester/.config/colima/default/docker.sock was denied. The wrapper retried its normal endpoint setup and reported the same denial during cleanup. No assertion result is claimed for these tests.

## Recorded results and limits

The original task records functional suites, targeted runs and checks at their named commits. Review reports assess their source assertions and relevance independently; they are not reruns of those suites here.
