# Independent focused verification

Candidate remained `f1f6a9c833061ec6f0a781247b762ec3c0de4bea`; source unchanged.

Command:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_hosted_artifacts::evidence_metadata_is_resolved_once) | test(=pg_verification_runtime::inventory_readiness_preserves_execution_bounds) | test(=pg_verification_routes::canonical_readback_does_not_read_payload_bytes)'"
```

Result: repository Postgres provisioned successfully; both migration-idempotency tests passed. Nextest run `2e8390bb-6504-4c00-903d-139d10a739d5`: exactly three selected tests passed, 309 skipped, in 6.497 seconds after compilation. The linker emitted its compact-unwind-size warning; no test failed.

`git diff --check 3a99eb05f...f1f6a9c83` also passed.

Additional evidence reconciliation: `mise run ts:napi:check` passed, including frozen-lockfile install and its existing `ts:build` dependency (3.89 seconds total). The generated `index.d.ts` had no Git difference. This closes the standards report's missing TypeScript generated-declaration-check evidence; no source or declaration was edited.

No performance benchmark or full SDK suites were rerun in this review. The existing inline-JSON benchmark does not exercise File/ArtifactRef inventory or upload completion, so another run of that unchanged workload cannot close the affected-path measurement obligation. Recorded implementation checks and language journeys remain subject to reviewers' source and evidence assessment.

The locally retained `target/benchmarks/verification/direct-verify/20261029T023734Z-47ba4087c.json` was inspected independently: schema 3, full mode, verdict `INVALID`; `host.busy` reports 1.57 of 11 CPUs (14.3%, above 10%) before the run, and both `direct-verify-c1` and `direct-verify-c64` have `step.unmeasured`, "the run admitted no workload". This confirms no performance reading was obtained; it does not demonstrate performance regression. The original task already identifies the inline-only workload limitation.
