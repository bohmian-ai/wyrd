# Supplied evidence and independent execution

Immutable candidate4478271030da79e7e2eb09fe108fbf1ed10145d8, prior0c2bfe70d5186dff4dc224c7e402b57c24f354d9; original9692bfd06. User-authorized skill edit pre-exists review and is excluded implementation workflow; reviewed source untouched.

## Caller-supplied diagnoses (not a separately supplied diagnostician report)

Full gate reportedly found four test:wyrd and three test:bifrost:gate failures. No standalone prior diagnostician report was provided. Supplied diagnoses, independently challenge against source:
- Credential issue_credential extracted/returned typed-body errors before authorization and tenant-scoped require_principal since personal-key ba19430b5. Raw Bytes now parse to CreateUserCredentialRequest only after those boundaries. Empty body means absent request. Existing system-principal/foreign-tenant refusal tests exposed the precedence regression.
- Queued-password fixture predates required User role. Seed built-in roles and create fixture as viewer; retain window-exhaustion/KDF queue assertions.
- Public-controls fixture allowlists lacked personal-key bootstrap/seed helpers. seed_user_keys/seedUserKeys write identical-created_at rows for key-pagination fixtures; independently inspect helpers' actual ownership/exposure and semantics rather than approving an arbitrary allowlist expansion.
- MCP expected advertised tools/count omitted existing principals.me read tool; expected count now39. Inspect actual registration/authorization and affected assertion strength.

The user expressly included these gate fixes in implementation for review. Commit447827103 records the compact diagnoses. Absence of a separate report is disclosed; no fabricated independent historical diagnosis is claimed.

## Independent root runs

- PASS: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.test.ts src/lib/ui-library.test.ts` — 22 tests / 2 files, current package rebuilt.
- PASS: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::scribe_persistence_path::a_stop_after_durable_series_rows_recovers_them_and_admits_the_retry) | test(=scribe::scribe_persistence_path::a_stop_after_durable_unanswered_points_recovers_every_point_indexed) | test(=scribe::scribe_persistence_path::a_failed_series_admission_fails_the_points_admission) | test(=scribe::shards::tests::post_sync_retry_does_not_enter_precommit_cleanup) | test(=scribe::wal_closeout::failure_after_fsync_before_ack_reuses_stable_batch_once)'` — all5 selected passed / 913 skipped. Source reviewers separately assess actual causal proof.
- Running: fresh repository-Postgres wrapper/migrate:all, then exact two server credential-refusal cases, queued password case, testing public-control case, and three MCP connectivity/discovery cases sequentially. Append final outcomes and precise selector/feature corrections if any.

Committed R2 task appends supplied implementation evidence: five focused Rust cases, UI unit22, one filtered Metrics journey, UI check/build, fmt/clippy (all-feature/all-target and test-support lib), whitespace. Do not convert supplied claims into independent executions. No human visual signoff is supplied, previous latency miss/AC040 uncertainty remains, and no new synthetic host load or broad gate is automatically scheduled.

## Focused gate-fix reruns completed

Fresh repository Postgres wrapper plus `mise run db:migrate:all:inner` passed both SQL migration checks. Commands executed sequentially:

```bash
cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_verification_routes::system_writer_is_unreachable_through_public_principal_routes) | test(=platform_admin_e2e::one_tenant_cannot_reach_another_tenants_identities)'
cargo nextest run --locked -p wyrd-auth --lib -E 'test(=passwords::pg_tests::queued_logins_see_windows_exhausted_while_waiting)'
cargo nextest run --locked -p wyrd-testing --lib -E 'test(=server::tests::public_controls_are_the_three_sanctioned_operations)'
cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=connectivity::pg_tests::streamable_http_client_reaches_authenticated_server_context) | test(=discovery::pg_tests::agent_discovers_only_authorized_tables_and_layout) | test(=discovery::pg_tests::agent_discovers_mcp_guide_and_sql_examples)'
```

All selected passed: server2, auth1, testing1, MCP3. No skipped/no-selected case is credited; MCP3 explicitly ran ignored journey tests using journey profile. These runs validate actual assertions; separate source reviews assess whether the changes preserve contract/gate strength.

Immutable range `git diff --check 0c2bfe70d 447827103` passed. HEAD still447827103; tracked working changes remain only the authorized skill edit.

Additional sequential run started: `pg_browser_sessions::user_keys_preserve_owner_scope_and_credential_lifecycle` through the same fresh Postgres/migrate:all owner; then `mise run ts:build:testing` to bind current native harness to candidate's changed server, then filtered authenticated Metrics browser journey rebuilding current UI/BFF. Append outcomes when complete.

## Additional credential and UI results

Fresh Postgres/migrate:all completed. Exact `cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_browser_sessions::user_keys_preserve_owner_scope_and_credential_lifecycle)'` passed1 / 320 skipped. Current `mise run ts:build:testing` completed (native harness rebuilt against candidate server); then `WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t '^authenticated metrics investigation$'` passed1 / 5 skipped after current UI/BFF build. Both executed within `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc` with migrate:all setup and sequential Cargo ownership.

No live configured>2MiB credential-body rejection probe was executed. Existing Rust WyrdTestServerBuilder::with_limits_for_test can exercise it; current Node NativeTestServerOptions lacks a limits field, and embedded construction uses WyrdServerConfig defaults rather than environment loading. Security/standards proposals trace the conditional path through current source and locked Axum dependency, and independent validation must assess that proof/materiality. Do not describe this scenario as runtime-reproduced or a privilege escalation.
