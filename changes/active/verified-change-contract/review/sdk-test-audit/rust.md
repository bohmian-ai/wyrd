# Rust SDK test audit

## Summary

- Files audited: 22 (18 Rust sources, 4 JSON schema goldens); test functions: 116 (13 in `sdks/wyrd-sdk-rust/tests`, 103 in `crates/shared/wyrd-client/tests`).
- Verdict counts: KEEP 46, TIGHTEN 10, REWRITE 7, MOVE 26, DELETE 27.
- No Rust SDK test reads like user code today. All 13 SDK tests are multi-story omnibus tests. They build Card YAML with `format!` and string concatenation, and none of them loads a checked-in fixture directory. The verification tests depend on surfaces that rev 64 removes or forbids: the `Verification` handle (`get_binding`, `start_run`, `get_run`, `execute`), `VerificationFixture` Postgres access, raw SQL on `vala.verification.*` / `vala.drift.*` / `vala.eval.*` tables, test-only server hooks, and assertions on PSI bins and SPC limits.
- The tests also reveal real SDK gaps, which they work around by hand:
  - Card-bound key issuance is a raw `POST /auth/issue-key`.
  - Errors are reached through `wyrd_sdk::verification::WyrdError`, and an async write refusal is dug out of `UpstreamFailure.details["original_code"]`.
  - Reads by run are `format!` SQL, and an unwritten built-in table raises TABLE_NOT_FOUND.
  - Read-your-writes needs `server.flush_bifrost()` or 90 s polling.
  - Backpressure needs a QUEUE_FULL resubmit loop.
- In `wyrd-client`, the transport behaviour tests are solid and mostly KEEP. Roughly a quarter of the crate's tests are serde-derive or config-default trivia (DELETE). About a third of `pg_bifrost_e2e.rs` is Bifrost engine or reliability work that never needs a user-facing journey (MOVE).
- Best models to copy:
  - `typed_sql_projects_rows_and_refuses_a_mismatch`
  - `blocking_client_registers_writes_and_reads_back`
  - the wiremock Operator-endpoint assertion in `drift_verification.rs`

## API gaps revealed by the tests

| Gap | Evidence (file:line) | What a user should be able to write instead |
|---|---|---|
| No local judgment call; tests start a run and poll it, or call the removed `Verification::execute` | sdks/wyrd-sdk-rust/tests/drift_verification.rs:319-340, 2560, 2627-2630, 2699-2716; verification_run.rs:165-219; crates/shared/wyrd-client/tests/transport/http.rs:946 | `let j: Judgment = run.observe().verify("latency-drift", rows).await?; assert!(!j.passed);` (REQ-188) |
| No SDK method to issue a Card-bound API key | sdks/wyrd-sdk-rust/tests/observe_run.rs:854-867 | `principals.issue_card_key(&card_ref, scopes).await?` |
| No top-level typed error; code lookups need `wyrd_sdk::verification::WyrdError` (verification.rs:21, which is being removed) or `sdk_code` helpers | observe_run.rs:987-994; drift_verification.rs:406-413; crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs:51-63 | `wyrd_sdk::Error` with `err.code() == "WYRD_..."` on every handle's error |
| An async write refusal arrives as `UpstreamFailure` carrying `original_code`, on either `record` or `shutdown` (nondeterministic) | observe_run.rs:980-994 | `shutdown().await` returns a typed `WYRD_VALA_403_BIFROST_CARD_SCOPE` error at one documented point |
| Reads by run or subject need `format!`-interpolated SQL | observe_run.rs:280-282, 304-305, 333-335, 418-421; drift_verification.rs:381-415, 2160-2168 | Parameterised `bifrost.sql_as::<Row>("... WHERE run_id = $1", [run.id()])`, or `run.observations()` |
| Verification results and items are read through SQL on internal tables | drift_verification.rs:381-415, 2060-2098, 2297-2307 | Assert on the returned `Judgment` (REQ-192); history through a documented public query view |
| A built-in table that has not been written yet raises TABLE_NOT_FOUND | drift_verification.rs:406-413, 1592-1595, 2171-2176 | Querying an existing but unwritten built-in table returns zero rows |
| No read-your-writes barrier; tests call `server.flush_bifrost()` or poll for 90 s | observe_run.rs:680; startup_image_journey.rs:70-90; pg_bifrost_e2e.rs (most write tests) | `writer.flush().await?` guarantees the next `sql` on the same client sees the rows (or a documented `flush_visible`) |
| No awaitable or backpressure-aware emit | observe_run.rs:1035-1045 | `run.observe().drift(...).await` waits for capacity, or a documented `try_emit` / `emit` pair |
| No "await Verifier baseline ready" | drift_verification.rs:262-292 | `cards.wait_ready(&verifier_ref, timeout).await?` |
| The schedule test clock is `VerificationFixture` writing Postgres directly | drift_verification.rs:1337-1350, 2129-2142 | `WyrdTestServer::clock().make_due(&binding)` as the single documented test-clock call (REQ-192) |
| No SDK OTEL span export; the test hand-builds an OTLP protobuf with a raw token header | observe_run.rs:360-404, 699-702 | `wyrd_sdk::otel::layer(&client)` or `client.otlp_exporter()` |
| Operator-connection requests are built from `json!` | operator_connections.rs:69-79, 184-188, 214-216, 236-238 | `CreateOperatorConnectionRequest::slack(name, webhook_secret)` |
| A `CardRef` / `CardSelector` is built from a JSON wire object | verification_run.rs:87-92 | `CardRef::new(CardKind::Verifier, "name", "1.0.0")` |
| Artifact `sha256` / `size_bytes` are hand-computed in tests | cards_state.rs:31-38; observe_run.rs:127-136; drift_verification.rs:147-153; startup_image_journey.rs:131-141 | Omit them; the client computes them (REQ-191) |
| Gateway credential writes need the non-SDK `CredentialWriter` | gateway_admin.rs:7-14, 20, 84-95 | Either an SDK `gateway.credentials().put(...)` or a documented harness setup call, if writes are intentionally CLI/MCP-only |
| Tables are registered through `state().bifrost_catalog().create_table` instead of the SDK | pg_bifrost_e2e.rs:765-778 | `bifrost.register(table_def).await?` |

## Cross-cutting problems

1. **Card YAML built in code.** Affects all 13 SDK tests and `startup_image_write`.
   - Examples: cards_state.rs:30-80; drift_verification.rs:146-245, 1678-1749; observe_run.rs:126-156.
   - The drift test also generates its Parquet baseline in code (drift_verification.rs:115-140).
   - Fix: check in shared `fixtures/cards/<graph>/` YAML directories and baseline files, used by all three SDKs (REQ-192).
2. **Several stories in one test.** Affects all 13 SDK tests and about 5 `pg_bifrost_e2e` tests.
   - Examples: cards_state.rs:197 (register, replay, RBAC, hydrate, offline load); observe_run.rs:601 (views, negatives, describe cache, activity, OTLP); drift_verification.rs:531.
   - Fix: one story per test, named after the user outcome.
3. **Test-only server hooks and server internals.** Affects about 14 tests.
   - Examples: `table_describe_count` / `last_authenticated_at` (observe_run.rs:519-530, 631-638); `with_verification_runtime_for_test` (drift_verification.rs:537, 830, 1828, 2525); `retire_fitted_format` (drift_verification.rs:1059-1067); `stall_next_query_after_schema` (pg_bifrost_e2e.rs:333); SQL on `vala.audit_staging` / `vala.file_list` (pg_bifrost_e2e.rs:187-210, 1177-1190, 1286-1318).
   - Fix: keep journeys on the public SDK, `WyrdTestServer` lifecycle, and the test clock. Move internal assertions to server or Bifrost integration tests.
4. **Removed `Verification` handle.** Affects 5 tests.
   - Examples: verification_run.rs:165-219; drift_verification.rs:563, 1547-1560, 2560; http.rs:946.
   - Fix: rewrite onto `run.observe().verify` (REQ-188/189). Binding and run lifecycle move to HTTP/MCP tests.
5. **Engine statistics asserted from SDK journeys.** Affects 2 tests.
   - Examples: PSI bins at drift_verification.rs:1211-1227; NIST X-bar/S limits with a magic `c4 = 0.9399856` at 1260-1279; `evidence()` JSON dict access at 776-785.
   - Fix: move these to Vala drift-engine unit tests. Journeys assert only `Judgment.verdict`, `passed`, and counts.
6. **Wire JSON instead of typed values.** Affects about 8 tests.
   - Examples: `serde_json::from_value(json!)` at verification_run.rs:115-121 and operator_connections.rs:69-79; `to_value(response)["kind"]` at drift_verification.rs:2704-2716; JSON row bytes built with `format!` at pg_bifrost_e2e.rs:1782-1784 and startup_image_journey.rs:240-242.
   - Fix: typed request constructors and typed row structs (see `sql_as` at pg_bifrost_e2e.rs:1961).
7. **Helper sprawl and duplication.**
   - Each SDK file re-declares `api_key` / `machine_key` / `connect`.
   - Three hand-rolled HTTP mock servers: http.rs:233-354, cards_transport.rs:40-149, and storage_dispatch.rs:44-238 (with a hand chunked decoder at http.rs:1098-1179).
   - Raw Bifrost config construction is repeated five times (pg_bifrost_e2e.rs:258-316, 783-796, 864-877, 1253-1265, 1366-1380).
   - Fix: one SDK `tests/common/mod.rs`; one shared mock (wiremock is already an SDK dev-dependency); one `bifrost_client(&srv)` helper.
8. **Serde and config-default trivia.** Affects 26 tests.
   - Examples: mock.rs:4-85; config_enum.rs:7-81; http.rs:13-55.
   - The JSON schema goldens (schema_drift.rs:35-53) already pin the config shape.
   - Fix: delete the round-trips. Move `validate` tests to unit tests beside `config.rs`.
9. **Reliability, bench, and in-process sink tests in the client e2e lane.** Affects 9 tests.
   - Examples: observe_run.rs:1057, 1156; pg_bifrost_e2e.rs:1335, 1510, 1551, 1628.
   - Some of these start a server and never use it (pg_bifrost_e2e.rs:1510-1628 drive `MockSink` / `StallSink`).
   - Fix: move them to the Bifrost reliability lane, a bench, or wyrd-queue unit tests.

## Per-file findings

### sdks/wyrd-sdk-rust/tests/cards_state.rs
Covers Card registration, replay, RBAC, hydration, and offline WyrdState load.

| Test | Verdict | Q1 production behaviour | Q2 user understanding | Fix |
|---|---|---|---|---|
| `registers_reads_hydrates_and_loads_offline_state` (197) | TIGHTEN | Public SDK only (`seed_role` is acceptable setup). The UUIDv7 version check (111) is trivia. | `format!` YAML (30-80), manual sha256 (31-32), and about 8 stories in one test | Split into register/replay, RBAC 403 (241-255), hydrate-complete/metadata, and offline-load tests. Use fixture YAML and drop the digest. |

### sdks/wyrd-sdk-rust/tests/gateway_admin.rs
Covers gateway configuration administration with redaction.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| `administers_redacted_gateway_configuration` (61) | TIGHTEN | Uses the non-SDK `wyrd_client::gateway_credential::CredentialWriter` (20, 84-95). | Typed inputs are good (76-118), but the capture-policy and 403 stories are mixed in. | Split into three tests. Set up the credential through an SDK or harness call (see gaps). |

### sdks/wyrd-sdk-rust/tests/verification_run.rs
Covers manual verification runs through the `Verification` handle.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| `starts_a_keyed_manual_run_and_reads_its_status` (129) | DELETE | Uses `get_binding`, `start_run`, and `get_run` (165-219), which REQ-189 removes. | Requests are built from `json!` (115-121), and the `CardSelector` is JSON (87-92). | Delete it. Move the retired-kind refusal (55-98) into a Cards registration test with a fixture. |

### sdks/wyrd-sdk-rust/tests/operator_connections.rs
Covers operator-connection CRUD, redaction, RBAC, and tenant isolation.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| `manages_redacted_connections_with_permission_and_tenant_separation` (165) | TIGHTEN | Strong production behaviour: redaction, tenant isolation, RBAC. | `json!` requests (69-79, 184-188, 214-216, 236-238), assertions on serialized `["auth"]` (202-206), and magic SECRETS indexes (22-29). | Split into redaction, RBAC, and tenant tests. Add typed constructors and assert through typed fields. |

### sdks/wyrd-sdk-rust/tests/observe_run.rs
Covers scoped runs, observation views, Card-key scope, and drift ingest reliability.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| `scoped_run_emits_drift_eval_and_generic_rows` (601) | REWRITE | Uses `without_audit_publication_for_test` (609), `table_describe_count` (519-530, 579-586, 662-666), `last_authenticated_at` (631-638, 680-688), and `flush_bifrost` as a barrier (680). | `format!` SQL by run (280-335, 418-421), a hand-built OTLP export (360-404), and about 5 stories | Split into: drift, eval, and record rows are visible through public queries; and OTEL join (after the gap is closed). Drop the describe-count and auth-time assertions. |
| `observations_store_the_client_emit_time` (740) | MOVE | Uses `scribe.shift_receipt_clock_for_test` (763, 803). | Clear intent | Move to a wyrd-client or Bifrost integration test (REQ-187). |
| `issued_card_key_writes_and_queries_within_its_scope_only` (909) | REWRITE | Raw `POST /auth/issue-key` (854-867) | Matches `verification::WyrdError::UpstreamFailure` and `details["original_code"]` (987-994); refusal may arrive on record or on shutdown (980-986). | Use the SDK key issuance and a typed error at one point (see gaps). |
| `drift_burst_survives_a_byte_budget_override` (1057) | MOVE | Reliability behaviour | QUEUE_FULL resubmit loop (1035-1045) and magic constants (1009-1018) | Move to the Bifrost reliability lane. Keep the unsealable-budget refusal (1070-1078) as a small SDK test. |
| `sustained_hundred_feature_drift_lands_exactly_once_with_flat_client_bytes` (1156) | MOVE | Bench: 15 s paced, samples `bifrost_metrics().owned_bytes` | Not a user story | Move to the bench or reliability lane. |

### sdks/wyrd-sdk-rust/tests/drift_verification.rs
Covers drift and eval Verifier fitting, scoring, scheduling, dispatch, and direct execution. All four tests are `#[ignore]`d.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| `drift_methods_fit_score_persist_and_dispatch` (531) | REWRITE | Uses `with_verification_runtime_for_test` (537), `start_run`/`get_run` polling (563, 319-340), SQL on result tables (381-415), PSI and SPC statistics (1211-1279), and `retire_fitted_format` (1059-1067). | String YAML (146-245), Parquet built in code (115-140), magic c4 constant, many stories | Fixture graph with `observe.verify` per method asserting a `Judgment`. Move the statistics to engine tests. |
| `drift_method_edges_score_through_oracle` (824) | MOVE | Engine edge semantics: per-row mean, partial subgroup, sparse, text metric, legacy | `evidence()` dict access (776-785) | Move to Vala drift-engine or server tests. |
| `service_verifies_drift_and_eval_through_the_sdk` (1816) | REWRITE | The Operator request at the wiremock endpoint (1786-1794, 1869-1873) is the right assertion. Also uses `make_binding_due` through Postgres (1337-1350), `runs()` ledger reads (1343-1369), and `last_authenticated_at` (1991-1998). | `skald_prompt` JSON envelope and YAML built in code (1678-1749); `owned_results` SQL polling (2153-2189) | Fixtures plus the documented test clock. Assert on the Operator request and public-query rows only. |
| `direct_execution_judges_supplied_input_through_the_sdk` (2517) | REWRITE | Uses the removed `Verification::execute` (2560) and `EXECUTION_DEADLINE` (2439). | JSON `ExecuteVerificationRequest::decode` (2475-2486), dict builders (2489-2499), `to_value(...)["kind"]` (2704-2716) | `run.observe().verify(name, rows)` returning a `Judgment`. Map the refusal matrix (2725-2785) to the REQ-188 local codes. Move `retire_fitted_format` (2848). |

### crates/shared/wyrd-client/tests/transport.rs
Module wiring only; no tests.

### crates/shared/wyrd-client/tests/pg_discovery_against_fixture.rs
Covers `ClientConfig::from_env`.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| `discovery_against_fixture` (34) | MOVE | Tests config wiring. The `WYRD_API_KEY` assertion (69-73) only reads back the value it set. | Unsafe env mutation behind a mutex | Move to a `config.rs` unit test (no server), or delete it as covered elsewhere. |

### crates/shared/wyrd-client/tests/pg_auth_e2e_against_fixture.rs
Covers the server auth-header contract.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| `wyrd_client_authenticates_via_wyrd_access_token_header` (36) | TIGHTEN | A valid integration-tier contract | Raw `request_json` GET `/v1/cards` plus a raw reqwest control | Use `Cards::list` for the client side. Keep the raw control. |

### crates/shared/wyrd-client/tests/transport/schema_drift.rs (plus tests/schemas/*.json)
Golden JSON-schema contract for transport configuration.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| `transport_config_enum_schema_matches_golden` (35), `grpc_config_schema_matches_golden` (41), `http_config_schema_matches_golden` (47), `mock_config_schema_matches_golden` (53) | KEEP | Contract guard | Clear | None |

### crates/shared/wyrd-client/tests/transport/mock.rs
Covers `MockConfig` serde behaviour.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| 4, 12, 20, 31, 41, 52, 70, 85 (defaults, round-trips, equality) | DELETE | Tests serde derives | Trivia | Delete; the golden covers the shape. |
| `mock_config_rejects_unknown_fields` (63) | KEEP | `deny_unknown_fields` is a real config contract | Clear | None |

### crates/shared/wyrd-client/tests/transport/config_enum.rs
Covers the `TransportConfig` enum.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| 7, 13, 23, 31, 39, 50, 81 (defaults, tag shape, round-trips, `name()`) | DELETE | Derive trivia, covered by the golden | Trivia | Delete |
| `transport_config_validate_grpc_empty_endpoint_returns_err` (103), `..._validate_mock_always_ok` (116) | MOVE | Real validation | Clear | Move to a unit test in `config.rs`. |
| `transport_config_rejects_deferred_variant_tags` (122) | DELETE | A name-ban on removed kafka/redis tags (AGENTS.md section 12, retiring checks) | n/a | Delete; `deny_unknown` tag handling already rejects them. |

### crates/shared/wyrd-client/tests/transport/grpc.rs
Covers gRPC config and `GrpcConnection`.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| `grpc_default_values` (5), `grpc_default_round_trips` (13) | DELETE | Trivia | Trivia | Delete |
| validate tests (21, 31, 37, 47) | MOVE | Real validation | Clear | Move to a unit test. |
| `grpc_rejects_unknown_fields` (56) | KEEP | Config contract | Clear | None |
| `dial_failure_invalid_url_maps_to_transport_down` (117), `connect_stub_yields_channel_and_auth` (133), `max_message_bytes_reflects_config` (156) | MOVE | Internal `GrpcConnection` details such as `Arc::ptr_eq` (148-152) | Stub HTTP/2 listener | Move to an in-crate unit test. |
| `keepalive_disabled_when_interval_zero` (173) | DELETE | Asserts nothing | n/a | Delete, or assert the observable effect. |
| `https_endpoint_trusts_the_platform_roots` (195) | KEEP | A real TLS trust property | `SSL_CERT_FILE` set under `unsafe`, acceptable here | None |

### crates/shared/wyrd-client/tests/transport/http.rs
Covers HTTP config and the transport behaviour of `WyrdHttpClient` against a mock server.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| 13, 21, 26, 34, 43, 55, 114 (defaults, round-trips) | DELETE | Trivia | Trivia | Delete |
| 67, 78, 103 (validate base url / timeout) | MOVE | Real validation | Clear | Move to a unit test. |
| `http_validate_rejects_remote_cleartext` (88), `http_rejects_unknown_fields` (126) | KEEP | Security and config contract | Clear | None |
| `running_query_request_id_and_controls_round_trip` (143) | KEEP | Request-id echo | Clear | None |
| 412, 462 (`revoke_credential_*`), 518, 533, 556, 589, 603, 626, 663, 697 | KEEP | Public `Principals`, re-exchange, idempotency, problem-json mapping | Clear, one behaviour each | None |
| 754, 777 (same-origin vs cross-origin URL) and 821 (`wyrd_access_token_header_is_injected_not_authorization`) | KEEP | Token-leak security property | Clear | None |
| 848, 907 (json stream auth and timeout), 988, 1012, 1042, 1061 (slow transfers), 1183 (unapproved media type) | KEEP | Real transport guarantees | Clear | None |
| `verification_execute_outlives_the_configured_timeout` (946) | REWRITE | Uses the removed `Verification::execute` | Clear | Retarget to the `observe.verify` transport deadline. |

### crates/shared/wyrd-client/tests/cards_transport.rs
Covers the Cards registry client wire contract against a hand-rolled server (40-149).

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| `list_uses_typed_query_parameters_and_an_empty_get_body` (187) | TIGHTEN | Asserts the exact query-string order (217), which is trivia | Clear | Assert the parsed parameter set instead. |
| `uid_delete_uses_kind_qualified_path_and_preserves_idempotence` (224), `server_problem_code_and_status_survive_registry_boundary` (250), `client_debug_redacts_constructor_secret` (279) | KEEP | Real contract and redaction | Clear | None |

### crates/shared/wyrd-client/tests/startup_image_journey.rs
Script-driven deployment journeys. All are `#[ignore]`d.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| `startup_image_write` (119) | TIGHTEN | Public SDK | `format!` YAML plus a manual digest (131-141); JSON bytes with `Correlation::default()` (161-184) | Use a fixture YAML and a typed insert, and drop the digest (REQ-191). |
| `startup_image_verify` (201), `kind_seed` (257), `kind_append` (273), `kind_join_read` (284) | KEEP | Deployment restart and readback | 90 s visibility polling (70-90) is legitimate but reveals the read-your-writes gap | None |

### crates/shared/wyrd-client/tests/storage_dispatch.rs
Covers the `WyrdStorageClient` upload and download protocol over server-minted plans, against a hand-rolled server with a chunked parser (44-238).

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| 301, 349, 391 (provider dispatch), 447 (verified download), 504 (invalid plan), 535 (backend failure) | KEEP | Integration-tier protocol contract, not a user example | Assertions by request index | Optionally replace the server with the shared mock. |

### crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs
Covers Bifrost query and write through the client against a real server.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| `oracle_query_status_cancel_is_tenant_scoped` (2752, impl 352) | TIGHTEN | Public lifecycle, but uses the `stall_next_query_after_schema` hook (333) and audit SQL predicates (138-171, 428-459) | Long | Keep running/status/cancel tenant scoping. Move the audit assertions to a server test. |
| `oracle_query_grpc_status_cancel_is_tenant_scoped` (2759, impl 467) | MOVE | Raw tonic proto client with a manual bearer header (213-220, 475-578) | Wire-level | Move to server gRPC contract tests. |
| `oracle_query_returns_arrow_batches_and_terminal` (759) | TIGHTEN | Uses `bifrost_catalog().create_table` (765-778) and the test `BifrostWriter` | OK | Use `Bifrost::register` and insert. |
| `pg_bifrost_multi_batch_query_stream_reuses_schema` (840) | MOVE | Asserts `arrow_ipc_bytes` and `peak_pending_frame_bytes` | Wire internals | Move to a Bifrost integration test. |
| `described_canonical_and_dynamic_schemas_reach_exact_physical_schema` (997) | MOVE | `vala_bifrost_redux` and `wyrd_queue` internals; SQL on `vala.file_list` (1177-1190) | Engine-level | Move to a Bifrost integration test. |
| `public_sdk_bidi_write_ack_and_durable_readback` (1228) | REWRITE | `vala.file_list` SQL plus a storage `stat` (1286-1318) | Mixed | Read back through the public query. |
| `public_sdk_owned_batch_timeout_retry_retains_then_deduplicates` (1335) | MOVE | `with_wal_sync_delay`, `RecordingTransport` (702-755), metrics, and file-list SQL | Reliability | Move to the Bifrost reliability lane. |
| `observe_and_bifrost_roundtrip` (1510), `backpressure_and_drain_no_silent_drops` (1551), `downstream_stall_remains_bounded_and_recovers_after_drain` (1628) | MOVE | `MockSink` / `StallSink` (590-683); the server they start is unused | Unit-level | Move to wyrd-queue or client unit tests. |
| `unified_client_registers_writes_swaps_and_reads_both_tables` (1747) | KEEP | Public API, including the stale-writer fence negative flow (1853-1891) | `format!` JSON rows (1782-1784) are a minor issue | Use typed rows. |
| `blocking_client_registers_writes_and_reads_back` (1910) | KEEP | Public blocking API with typed `sql_as` (1961) | Clear | None |
| `register_never_stamps_one_tables_identity_onto_another` (1991) | MOVE | Race regression that polls a future | Internal | Move to a unit or integration test. |
| `typed_sql_projects_rows_and_refuses_a_mismatch` (2201) | KEEP | Typed rows, empty result, mismatch code | Clear | None |
| `analytical_sql_over_written_tables` (2278) | TIGHTEN | Tests DataFusion SQL semantics | Many queries | Trim to one representative query. |
| `canonical_signal_arrow_write_and_sql_read_round_trip` (2439) | TIGHTEN | Uses the `ensure_builtin_table_for_test` hook (2450) | Very long | Drop the hook once built-ins read as empty. Split the queries. |
| `denied_describe_is_audited_before_admission` (2766, impl 2678) | MOVE | Raw SQL on `vala.audit_staging` (187-210, 2693-2702) and `cached_writer_table` internals | Server-level | Move to a server audit integration test. |

## Good examples to keep

- `typed_sql_projects_rows_and_refuses_a_mismatch` (pg_bifrost_e2e.rs:2201): typed rows, one story, and a stable error code.
- `blocking_client_registers_writes_and_reads_back` (pg_bifrost_e2e.rs:1910): a short register, write, and typed read.
- `unified_client_registers_writes_swaps_and_reads_both_tables` (pg_bifrost_e2e.rs:1747): a public journey that includes a negative fence.
- The Operator assertion in drift_verification.rs:1786-1794 and 1869-1873: it asserts the request received by a local wiremock Operator endpoint, exactly as REQ-192 prescribes.
- Typed `Features` / `Exchange` structs in drift_verification.rs:58-66 and 1609-1615.
- The transport security tests in http.rs: 754, 777, 821, and the `revoke_credential_*` tests at 412 and 462.

## Proposed target structure

```
fixtures/cards/                  # shared by the Rust, Python, and TS SDKs
  service-graph/                 # service + agent + prompt + artifact (no sha256)
  drift-verifiers/               # verifier YAML + baseline.parquet
  eval-verifiers/
  scheduled-dispatch/            # trigger + operator + bindings
sdks/wyrd-sdk-rust/tests/
  common/mod.rs                  # connect(), api_key(), load_fixture(name)
  cards_register.rs              # register/replay, retired kind refused, RBAC 403
  cards_hydrate_offline.rs
  observe_record.rs              # drift/eval/record rows via public query
  verify_drift.rs                # observe.verify -> Judgment, unknown verifier, bad input
  verify_eval.rs
  scheduled_dispatch.rs          # test clock -> Operator endpoint receives request
  card_key_scope.rs
  operator_connections.rs        # redaction / RBAC / tenant, one test each
  gateway_admin.rs
crates/shared/wyrd-client/
  src/**/config.rs #[cfg(test)]  # validate tests; serde trivia deleted
  tests/transport_http.rs        # one shared mock server
  tests/cards_transport.rs, storage_dispatch.rs, startup_image_journey.rs
  tests/bifrost_query_write.rs   # the KEEP/TIGHTEN pg_bifrost journeys
```

Move these out of the SDK and client tests:
- engine statistics and edge cases to Vala drift-engine tests;
- reliability and bench tests to the Bifrost reliability lane;
- audit and gRPC wire tests to `wyrd-server` integration tests.
