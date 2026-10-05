# Python integration SDK test audit

All paths are relative to `sdks/wyrd-sdk-python/tests/` unless stated otherwise.

## Summary
- Files audited: 23 (17 test modules, 2 `conftest.py`, `integration/gateway/support.py`, 3 `__init__.py`); test functions: 96.
- Verdict counts: KEEP 27, TIGHTEN 35, REWRITE 11, MOVE 10, DELETE 13.
- The Bifrost client, Cards registry, offline `WyrdState`, and third-party gateway-client tests are mostly sound. They drive the public SDK, assert on SDK objects or rows read back through `Bifrost.sql`, and several already read like user examples. The verification side is the problem. All six verification tests (`test_verification_journey.py`, `test_verification_execute_journey.py`, `test_drift_journey.py`) use the `Verification` handle that revision 64 removes. They build Card YAML from f-strings with computed digests and shell out to the `wyrd get` CLI to read Card status. They poll server internals through `verification_runs()` and assert PSI bins and SPC limits that belong in Rust engine tests. Across the scope, the tests most often work around three missing SDK capabilities: downloading a hydrated bundle, reading a Card's served status, and getting an access token or principal for an arbitrary key. Each gap is filled by its own subprocess, urllib, httpx, or JWT-decoding helper, and these helpers are duplicated across files. Harness overuse is the second theme. About 23 tests seed or assert through test-only `WyrdTestServer` hooks, and 11 of those hooks are missing from `testing/__init__.pyi`. Five Bifrost tests are vacuous or misnamed (`dropped == 0` on an async queue). Two files (`test_error_contract.py`, `examples/test_examples_smoke.py`) are not selected by any `mise` lane.

## API gaps revealed by the tests
| Gap | Evidence (file:line) | What a user should be able to write instead |
|---|---|---|
| No SDK way to download a complete hydrated Service bundle. Every journey shells out to the `wyrd` CLI binary next to `sys.executable`. | integration/state/test_state_journey.py:95-122, 198-211; integration/state/test_observe_journey.py:254-277; integration/test_drift_journey.py:115-145; integration/test_verification_journey.py:198-221 | `state = WyrdState.pull(service_ref, dest, server_url=..., credential=...)` or `cards.download(service_ref, dest)` |
| No SDK read of a Card's served `status`, such as Verifier baseline readiness or Service binding ids. Tests download the bundle to disk and re-parse `cards/root/card.yaml` on every polling iteration. | integration/test_drift_journey.py:148-162 (`wait_ready`); integration/test_verification_execute_journey.py:135-144; integration/test_verification_journey.py:222-225; integration/state/test_state_journey.py:352-357 | `cards.get(verifier_ref).status.baseline.state`, or a typed `cards.wait_ready(verifier_ref, timeout=...)` |
| Trusted artifact hashes are computed by re-implementing the canonical blake3 manifest hash in the test, from the same bundle they are meant to verify. | integration/state/test_state_journey.py:164-189 | An SDK-provided hash on the registered Card, e.g. `cards.model.get(uid=...).artifact_manifest_hash`, passed to `from_path(trusted_artifact_hashes=...)` |
| A run view's subject Card UID is only available by string-splitting `card_ref` on `#`. The stub documents `card_ref` as `space/Kind/name@version`, which does not match what the tests parse. | integration/state/test_observe_journey.py:607-608, 488; integration/test_drift_journey.py:1119-1121 | `view.card_uid` or a typed `view.card_ref: CardRef` |
| No SDK token exchange for an arbitrary API key. The harness `access_token()` covers only its own key, so the same raw `POST /auth/token` is written four times. | integration/gateway/support.py:312-318; integration/state/test_observe_journey.py:376-385; integration/bifrost/test_bifrost_e2e.py:1000-1014; smoke/test_gateway_live_providers.py:178-183 | `WyrdClient(credential=key).access_token()` |
| No SDK way to learn the caller's principal id. The test base64-decodes the JWT payload. | integration/gateway/support.py:321-326, used at test_gateway_native_clients.py:100,149 and test_gateway_openai_client.py:124,254-255 | `WyrdClient(...).principal_id` (or a `whoami()` call) |
| No read-after-write barrier for queries. Gateway evidence reads loop 40 times over `flush_bifrost()` and retry on Oracle admission 429s. | integration/gateway/support.py:329-363 | `bifrost.sql(query, model=GatewayCall)` after an SDK-level `wait_published()`, or a typed `gateway.calls(caller=...)` read |
| No SDK fetch of a stored gateway payload object. | integration/gateway/test_gateway_openai_client.py:303-308 | `gateway.payload_object(digest) -> bytes` |
| Provider credential mutation exists only over raw HTTP or the CLI (`cargo run`). This is deliberate for managed secrets, but it also forces raw HTTP for secret-free `environment` bindings. | integration/gateway/support.py:238-267; integration/gateway/test_gateway_admin.py:30-32, 62-68, 83-94; smoke/test_gateway_live_providers.py:105-157 | `gateway.put_credential(EnvironmentCredentialSource(...))` for non-secret sources; managed secrets stay CLI/MCP |
| The OTel exporter endpoint and auth header are assembled by hand from `os.environ["WYRD_GRPC_URL"]` plus a token. | integration/bifrost/test_bifrost_e2e.py:730-733, 783-786; integration/state/test_observe_journey.py:388-404 | `wyrd.otel.exporter_options(client)`, returning endpoint and headers, or `client.otlp_headers()` |
| Persisted OTLP attributes, bodies, and events come back as protobuf bytes that the test decodes with `opentelemetry.proto`. | integration/state/test_observe_journey.py:499-504; integration/bifrost/test_bifrost_e2e.py:749-767, 907-911 | A queryable map column or an SDK decoder, e.g. `wyrd.otel.decode_attributes(row.attributes)` |
| `observe.drift` raises `QUEUE_FULL`, so the user must write a flush-and-resubmit loop. | integration/state/test_observe_journey.py:677-690 | A blocking or backpressure-aware emit (e.g. `observe.drift(row, block=True)`) or documented automatic drain |
| Persisted verification results expose `details` as a JSON string that tests `json.loads` and index into. | integration/test_drift_journey.py:224-241, 906-918; integration/test_verification_execute_journey.py:253 | An SDK row model, `bifrost.sql(q, VerificationResult)`; for real-time use, the typed `Judgment` from `observe.verify` (REQ-188) |
| Query terminals are untyped dicts. | integration/test_bifrost_query.py:28-32, 59-62; integration/bifrost/test_bifrost_e2e.py:478, 601 | `result.terminal.outcome`, `result.terminal.row_count` on a typed `QueryTerminal` |
| `wyrd.observe.record` takes a JSON-Schema string and a JSON row string. | integration/bifrost/test_bifrost_e2e.py:46, 180-186, 317-323 | `record(bifrost, table, row_model_instance)` with the schema derived from the model |
| Being closed by rev 64: artifact digests are computed in test code (REQ-191), the Operator hook URL is spliced into YAML (REQ-190), and direct judging uses the removed `Verification.execute` (REQ-188/189). | integration/state/test_observe_journey.py:142-143; integration/test_drift_journey.py:813-814, 845; integration/test_verification_execute_journey.py:147-166 | Static fixture Cards without `sha256`; an `http` Operator with a path-only `url` and a named connection; `run.observe.verify("verifier-name", input)` returning `Judgment` |

## Cross-cutting problems
1. **Card YAML built in code (8 tests).** Strings are concatenated or spliced with f-strings, sometimes with computed digests and `.replace()` edits. Examples: integration/test_drift_journey.py:71-89 and 760-857; integration/state/test_observe_journey.py:135-240; integration/test_verification_execute_journey.py:47-61 and 200-208. Fix: use checked-in YAML fixture directories shared by the three SDKs (REQ-192), enabled by REQ-190/191. `test_state_journey.py` already loads a checked-in graph (`crates/wyrd/wyrd-cli/tests/fixtures/card_lifecycle/typed_state`, lines 30-33), which is the right model. Move that graph to a shared fixtures root.
2. **CLI subprocess for reads (16 tests).** These are every `test_state_journey.py` test, both observe journeys, all three drift journeys, the verification journey, and both execute journeys (through `download`). Examples: test_state_journey.py:95-122; test_drift_journey.py:115-145; test_observe_journey.py:254-277. Fix: add the bundle-download and Card-status SDK APIs (gaps 1-2). Until those exist, keep one shared fixture helper instead of four copies.
3. **Removed `Verification` handle (6 tests).** test_verification_journey.py:236; test_verification_execute_journey.py:169 and 301; test_drift_journey.py:258, 546, 1065. Fix: rewrite on `observe.verify` (REQ-188). Assert binding behaviour through Operator requests received at a local endpoint (REQ-192). Binding and run status stay on HTTP/MCP tests (REQ-189).
4. **Test-only server hooks and internal polling (about 23 tests).** Hooks used: `verification_runs()` polling (test_drift_journey.py:182-189, 869-876; test_verification_execute_journey.py:298), `retire_fitted_format` (test_drift_journey.py:353), `table_describe_count` (test_observe_journey.py:601, 626, 639), `bifrost_read_decision_count`/`wait_oracle_audit_staged` (test_bifrost_query.py:118, 133; test_bifrost_e2e.py:474-483), fault injection and resource snapshots (test_bifrost_query.py:71, 146-172), and `prepare_oracle_query_fixture`/`ensure_builtin_table` for seeding (10 e2e tests and 7 query tests). Eleven of these hooks are absent from `python/wyrd/testing/__init__.pyi`, so the stub no longer matches `src/testing.rs` (stub drift). Fix: seed tables through `Bifrost.register` + `insert`, keep only `make_binding_due` (sanctioned by REQ-192), and move internal-state assertions to Rust.
5. **Engine statistics asserted in SDK tests (4 tests).** `assert_spc_evidence` computes NIST X-bar/S limits (test_drift_journey.py:224-241). `assert_psi_bins` checks per-bin proportions (test_drift_journey.py:906-918). The method-edge journey (test_drift_journey.py:546-653) and the verdict matrix in test_verification_execute_journey.py:227-254 do the same. Fix: move these to Rust drift engine tests (REQ-192). SDK journeys assert only `Judgment.verdict` and attribution.
6. **One test, many stories (11 tests).** test_operator_connections_journey.py:26-131 has about 8 stories. test_observe_journey.py:580-662 has about 10. test_drift_journey.py:1065-1176 has 7. Others: test_gateway_admin.py:25-75; test_gateway_openai_client.py:191-262; test_cards_crud.py:379-417 (GIL release plus denied-identity). Fix: split into one test per story, sharing a fixture for setup.
7. **Bare `pytest.raises(WyrdError)` without a code assertion (10 tests).** Examples: test_cards_crud.py:156, 174, 314, 430, 445, 468-471. Message-substring matches appear at test_bifrost_e2e.py:335, 346, 356. Fix: assert `error.value.code` against the stable catalog code.
8. **Process environment mutated without restore.** test_cards_crud.py:121-125 and 231-232 write `os.environ` and never restore it. test_bifrost_e2e.py:135-147 calls `os.unsetenv` because the harness exports from Rust. The OTel tests rely on `os.environ["WYRD_GRPC_URL"]` (test_bifrost_e2e.py:783, 880, 937). Fix: pass `server_url`/`credential` explicitly, use `monkeypatch`, and run the session server with `mutate_env=False`, except in the one test that proves environment resolution.
9. **Vacuous or misnamed tests (5 tests).** test_bifrost_e2e.py:156, 177, 303 (`propagates_queue_full` never fills a queue), 314 (`swallows_and_counts` asserts zero drops), and 114. Each asserts `dropped == 0` or `producer_count` right after an asynchronous insert and never reads anything back. Fix: delete these, or end each with a read-back.
10. **Orphaned files.** `test_error_contract.py` and `examples/test_examples_smoke.py` carry no marker and sit outside `tests/unit`. `py:test:unit` runs only `tests/unit` (scripts/run_unit_tests.py:13-16), and `py:test:integration` selects `-m integration`. No `mise` task names either file. Separately, `test_negative_reserved_column_is_refused_locally` (test_bifrost_e2e.py:339) is marked `integration` but needs no server.
11. **Raw HTTP helpers duplicated across files.** The token exchange appears four times (see the gaps table). `put_credential`/`delete_credential` appear at support.py:238-267. Fix: add the SDK methods and delete the helpers.
12. **Docstring drift.** test_observe_journey.py:6-8 promises "Startup is first refused once per fixed table whose describe the server fails", which no code exercises. harness/test_test_server.py:30-34 cites a past refactor. test_drift_journey.py:1-30 is a 30-line module docstring that narrates three journeys.

## Per-file findings

### test_error_contract.py
Covers the `WyrdError` attribute contract through a local prompt-validation error. No lane runs this file.
| Test | Verdict | Q1 production behaviour | Q2 user understanding | Fix |
|---|---|---|---|---|
| test_wyrd_error_exposes_every_required_attribute (23) | TIGHTEN | Yes; public `wyrd.WyrdError` | Clear | Move to `tests/unit`; merge with :41 |
| test_aggregate_problem_attribute_is_absent (35) | DELETE | Name-ban of a removed attribute | n/a | Delete |
| test_direct_attributes_agree_with_the_problem_projection (41) | TIGHTEN | Yes | Clear | Merge into :23 |
| test_error_is_catchable_as_the_shared_base_class (49) | DELETE | Duplicates the helper's own `pytest.raises` | n/a | Delete |
| test_errors_module_projects_the_root_error_class (54) | TIGHTEN | Yes | `__all__ == [...]` is trivia | Keep only the `is` identity check |

### examples/test_examples_smoke.py
Imports and runs every example's `main()`. No lane runs this file.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_example_runs (25) | TIGHTEN | Yes; runs real examples | Clear | Wire into `check:examples`, or delete if that task already covers it; drop the `sys.path` mutation at :10 |

### harness/test_test_server.py
Smoke-tests the `WyrdTestServer` harness itself.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_import (8) | DELETE | Tests nothing | n/a | Delete |
| test_construct_default (12) | DELETE | Asserts `is not None` | n/a | Delete |
| test_construct_no_env_mutation (17) | DELETE | Same | n/a | Delete |
| test_bootstrap_service_raises_without_context_manager (22) | TIGHTEN | Private `wyrd._wyrd` import (:4) | Clear | Import `wyrd.WyrdError` |
| test_enter_fails_without_db (29) | TIGHTEN | Environment-conditional skip | Docstring cites a refactor | Rewrite the docstring; drop the in-body `import os` |

### bifrost/test_public_typing.py
A static-typing fixture for the Bifrost TypedDict graph, checked by `ty`.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_described_field_types_are_recursive_without_any (20) | KEEP | Yes (static contract) | Clear purpose | None |
| test_client_value_types_are_declared_without_any (63) | KEEP | Yes | Clear | None |

### integration/conftest.py, integration/gateway/conftest.py, integration/gateway/support.py
Fixtures for a session server with `mutate_env=True` and a recording mock upstream. support.py also holds the raw-HTTP credential, token, JWT-decoding, and polling helpers listed in the API gaps. The `Upstream` handler (support.py:172-218) is a good shared fake and should stay. `call_rows` (329-363), `principal` (321-326), `exchange` (312-318), `put_credential`/`delete_credential` (238-267), and `admin_headers` (228-235) should go once the SDK covers them.

### integration/gateway/test_gateway_admin.py
Covers gateway administration through `wyrd.gateway.Gateway`.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_gateway_admin_journey (25) | REWRITE | Credential CRUD over raw HTTP (:30, :62, :67) | Five stories; `set(view) == {...}` key trivia (:39-48) | Split into credential, deployment, conflict-on-delete, and capture-policy tests; use the SDK credential write once it exists |
| test_gateway_admin_requires_gateway_permissions (78) | TIGHTEN | Half of the test is raw HTTP | Clear | Assert the SDK denial only, by its code |

### integration/gateway/test_gateway_native_clients.py
Runs unmodified Anthropic and Google GenAI clients through the gateway.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_anthropic_client_calls_gateway_natively (54) | REWRITE | Client use is exemplary; evidence comes from polled SQL and JWT decoding | Four stories (call, stream, refusals, evidence) | Split; read evidence once through an SDK read-after-write |
| test_google_genai_client_calls_gateway_natively (107) | REWRITE | Same | Same | Same |

### integration/gateway/test_gateway_openai_client.py
Runs the unmodified OpenAI client through the compatible surface.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_openai_client_reaches_every_backend_through_the_server (77) | REWRITE | Asserts adapter URL paths by position (:105-113); polled evidence | Loop over 4 backends inside one test | Parametrize by backend; assert path per backend |
| test_openai_client_receives_stable_refusals_without_dispatch_or_leakage (133) | TIGHTEN | Yes | Refusal list is clear; cancelled-stream story bolted on (:179-188) | Split out the cancelled-stream test |
| test_two_users_have_distinct_model_access_and_traceable_usage (192) | REWRITE | `revoke_scoped_role` hook; JWT decode | Access, usage, revocation, and evidence in one test | Split; keep the scoped-access story as the example |
| test_embedding_and_image_evidence_reaches_bifrost_and_storage (266) | TIGHTEN | Raw `httpx.get` payload fetch (:303) | Clear | Use the SDK payload fetch (gap) |

### integration/test_operator_connections_journey.py
Covers Operator connection admin through `OperatorConnections`. It uses only the public SDK, which is good.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_admin_manages_redacted_connections_with_permission_and_tenant_separation (27) | TIGHTEN | Yes | About 8 stories in 105 lines; :66-68 re-asserts `created` after the update | Split into Slack lifecycle, PagerDuty/HTTP redaction, auth-scheme update, RBAC, and tenant isolation; keep `assert_redacted` |

### integration/test_verification_journey.py
Covers the `Verification.get_binding`/`start_run`/`get_run` handle that REQ-189 removes.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_service_starts_a_keyed_manual_run_and_reads_its_status (237) | DELETE | Removed surface; CLI subprocess read (:198-221); YAML strings (:156-195) | n/a | Delete; binding and run coverage stays in HTTP/MCP tests |

### integration/test_verification_execute_journey.py
Covers `Verification.execute` direct judging, which REQ-188 replaces with `observe.verify`.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_verifiers_judge_supplied_input_directly (170) | REWRITE | Removed surface; raw dict requests (:147-159); `retire_fitted_format` and `verification_runs()` hooks; engine verdict matrix | 13-case refusal table plus a 10-case verdict table; YAML in code; imports helpers from another test module (:35-45) | Rewrite as `observe.verify` journeys over static fixtures: Eval passed/failed, Drift passed/failed, unknown verifier, wrong shape, RBAC, tenant. Move the input-bound and baseline matrix to Rust |
| test_direct_judge_past_the_deadline_times_out (302) | MOVE | Server deadline behaviour; sleeps 65 s | Clear | Rust server/Vala test |

### integration/test_drift_journey.py
Covers Drift baselines, scoring, bindings, scheduled runs, Operator dispatch, and Eval bindings.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_parquet_baselines_fit_and_score_drift_server_side (259) | REWRITE | `start_run`/`get_run`; CLI status polling; SPC limit maths; `retire_fitted_format`; `weco_rule` name-ban (:400-410) | About 7 stories over 170 lines | Keep "Pandas/Polars/Arrow baselines become ready and verify" via `observe.verify`; scheduled binding to a local Operator receiver; legacy, weco, and IPC to Rust |
| test_drift_method_edges_score_through_oracle (547) | MOVE | Engine edge semantics (sparse PSI, partial SPC subgroup, Custom averaging) | Nested closures over many subjects | Rust drift engine tests |
| test_service_bindings_verify_drift_and_eval_through_an_http_operator (1066) | REWRITE | Most valuable story, but uses the removed handle, `verification_runs()` polling, PSI-bin and SPC maths, a CLI download, and a 90-line YAML builder (:769-857) | 7 stories | Split: (a) failed Drift binding delivers its HTTP Operator to a local endpoint, (b) Eval `observations_ready` runs judge records, (c) mapping/dataclass/Pydantic payloads persist identical rows; retired kinds to a unit test |

### integration/state/test_state_journey.py
Covers CLI-downloaded bundles hydrated offline by `WyrdState`. It uses a checked-in fixture graph, which is good.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_service_bundle_hydrates_complete_python_runtime_offline (215) | TIGHTEN | CLI download; blake3 re-implementation; asserts CLI JSON counts | Long but readable | Use the SDK download and hash once available; drop CLI JSON assertions |
| test_metadata_only_bundle_is_rejected_by_python_state (263) | KEEP | Yes | Clear | None |
| test_underprivileged_get_publishes_no_runnable_bundle (273) | MOVE | Tests CLI stderr JSON and remediation text, not the SDK | Clear | wyrd-cli Rust tests |
| test_tampered_downloaded_artifact_is_rejected_offline (295) | KEEP | Yes | `rglob` pick is slightly opaque | Optional: target a named artifact |
| test_same_kind_aliases_return_correct_distinct_runtime_objects (314) | DELETE | Duplicates :249 | n/a | Delete |
| test_missing_model_trust_returns_recoverable_runtime_error (325) | KEEP | Yes | Clear | None |
| test_missing_relationship_projection_is_rejected_offline (337) | KEEP | Yes | Clear | None |
| test_bound_service_serves_stable_uuid7_binding_ids (361) | MOVE | Server registry identity; UUIDv7 version is trivia; reads raw status dict | Clear | Rust registry test |

### integration/state/test_observe_journey.py
Covers `state.run()` views emitting Drift, Eval, and generic rows, OTel scope correlation, and burst backpressure.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_scoped_run_emits_drift_eval_and_generic_rows (572) | REWRITE | `table_describe_count` cache trivia (:601, 626, 639); CLI pull; UID string-split; urllib token; protobuf decoding; the stale-writer fence duplicates Bifrost e2e :369 | About 10 stories behind `run_journey`; 105 lines of YAML in code | Split into: emit-and-read-back by subject; Eval trace/session/media; Eval refusals; OTel scope joins; nested/async scopes. Load fixtures; drop the describe-count and fence assertions |
| test_drift_burst_survives_a_byte_budget_override (694) | TIGHTEN | Yes; read-back by `record_id` | The resubmit loop exposes an API gap; the unsealable-budget negative is bundled in (:710-716) | Split out the negative; drop the loop once emits apply backpressure |

### integration/cards/test_cards_crud.py
Covers Card register/get/list/delete for Prompt, Data, and Model, plus registry negatives. These are the closest to the opsml reference style.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_prompt_cards_register_get_list_resolve_latest_and_delete (129) | TIGHTEN | `_cards` mutates `os.environ` with no restore (:121-125) | Clear; bare raises at :156 | Explicit `Cards(server_url, credential)`; assert the 404 code |
| test_data_card_custom_interface_get_requires_interface_and_loads_artifacts (161) | TIGHTEN | Yes | Bare raises at :174 and :179 | Assert codes |
| test_data_card_pandas_interface_eager_loads_after_real_registry_round_trip (195) | KEEP | Yes | Exemplary | Only drop `_cards` |
| test_eager_data_load_uses_constructed_server_and_retains_workspace (218) | TIGHTEN | Leaks `WYRD_SERVER_URL=http://127.0.0.1:1` into later tests (:231-232) | Clear | Use `monkeypatch.setenv` |
| test_model_card_custom_interface_get_requires_interface_and_loads_artifacts (242) | TIGHTEN | Yes | Bare raises at :260 and :265 | Assert codes |
| test_model_card_sklearn_interface_eager_loads_after_real_registry_round_trip (283) | KEEP | Yes | Exemplary | None |
| test_typed_registry_rejects_a_card_of_the_wrong_kind (310) | TIGHTEN | Yes | No code asserted | Assert code |
| test_card_registration_replay_is_idempotent (319) | KEEP | Yes | Clear | None |
| test_registration_version_modes_match_server_contract (337) | KEEP | Yes | Clear | None |
| test_registration_releases_gil_and_failure_preserves_holder_identity (380) | REWRITE | GIL counter is a binding property, not a journey | Two unrelated stories | Split: keep "denied registration leaves uid/version unchanged" with a code; move the GIL check to a focused Python runtime test |
| test_card_registration_rejects_invalid_artifact_layout (421) | TIGHTEN | Yes | No code | Assert code |
| test_card_registration_rejects_underprivileged_writer (435) | TIGHTEN | Yes | No code | Assert `WYRD_PERMISSION_403_DENIED_RBAC` |
| test_card_registry_enforces_cross_tenant_isolation (450) | TIGHTEN | Yes | No codes | Assert the not-found code on get and delete |

### integration/test_bifrost_query.py
Covers async query streaming through `AsyncBifrost.stream`, plus canonical signal writes.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_bifrost_query_yields_pyarrow_and_terminal (13) | TIGHTEN | Seeds through the `prepare_oracle_query_fixture` hook (missing from the stub); `"freshness" not in terminal` trivia | Clear | Seed through the SDK; merge with the sync `sql`/`stream` test |
| test_query_stream_schema_once_eos (36) | MOVE | Asserts IPC EOS bytes (wire trivia) | Clear | wyrd-client Rust test |
| test_bifrost_query_missing_terminal_fails_closed (67) | MOVE | Server fault-injection hooks | Clear | Rust `pg_bifrost_e2e` |
| test_bifrost_query_out_of_range_deadline_is_the_shared_validation_error (94) | TIGHTEN | Yes; seeds an unused fixture table | Clear | Drop the fixture call |
| test_bifrost_query_gate_denial_has_no_oracle_side_effect (113) | MOVE | Counts audit decisions through a hook | Clear | Rust server test (denial already covered at e2e :351) |
| test_bifrost_query_cancellation_releases_all_resources (137) | MOVE | Admission, memory, and peer slot snapshots are server internals | Dense | Rust Oracle test |
| test_canonical_signal_arrow_write_and_sql_read_round_trip (388) | DELETE | Hand-builds canonical OTLP Arrow batches with protobuf blobs, which no Python user does; `ensure_builtin_table` hook | 140 lines of builders | Delete; the OTel exporter journeys (e2e :770-992) cover the user path; move the payload-reader scope check to Rust |

### integration/bifrost/test_bifrost_e2e.py
Covers the `Bifrost` client: transport resolution, table lifecycle, reads, negatives, OTel exporters, and delegation.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_bootstrap_service_mints_api_key (97) | DELETE | Tests the harness | n/a | Delete |
| test_bootstrap_service_empty_permissions_returns_key (103) | DELETE | Tests the harness | n/a | Delete |
| test_omitted_transport_resolves_from_the_environment (114) | TIGHTEN | Unregistered `genai.resolved` table; only `dropped == 0` | Clear intent | Register, insert, flush, and read back |
| test_no_resolvable_credential_raises (127) | TIGHTEN | `os.unsetenv` workaround for harness env export | Clear | Use a `mutate_env=False` server and monkeypatch |
| test_bifrost_insert_no_drops (156) | DELETE | Vacuous; `producer_count` trivia | n/a | Delete |
| test_insert_without_an_active_table_refuses (165) | TIGHTEN | Yes | `producer_count` trivia | Drop :173 |
| test_observe_record_no_raises (177) | REWRITE | JSON-string `record()`; no read-back | Opaque | Record a typed row and read it back |
| test_register_insert_flush_read_and_swap (191) | TIGHTEN | Seeds the second table through a hook; `producer_count` | Good journey | Register the second table through the SDK; split out the swap |
| test_sql_and_stream_return_the_same_rows (237) | TIGHTEN | Fixture hook | Clear | Seed through the SDK |
| test_describe_binds_an_existing_table_without_restating_its_schema (252) | TIGHTEN | Fixture hook | Clear | Seed through the SDK |
| test_uncorrelated_row_is_a_valid_write (282) | TIGHTEN | Fixture hook | Clear | Seed through the SDK |
| test_bifrost_insert_propagates_queue_full (303) | DELETE | Name says queue-full; body never fills a queue | Misleading | Delete |
| test_observe_record_swallows_and_counts (314) | DELETE | Asserts zero drops | Misleading | Delete |
| test_negative_bad_card_ref_raises (333) | TIGHTEN | Yes | Matches message text | Assert code |
| test_negative_reserved_column_is_refused_locally (340) | TIGHTEN | Yes; needs no server | Message match | Move to `tests/unit`; assert code |
| test_negative_empty_permissions_denied_rbac_on_write (351) | KEEP | Yes, with a no-effect read-back | Clear | Prefer `.code` over a `match=` string |
| test_registering_a_different_schema_on_one_name_conflicts (369) | KEEP | Yes | Clear | None |
| test_compaction_target_registers_describes_and_conflicts (392) | KEEP | Yes | Clear | None |
| test_negative_non_select_query_is_refused (426) | KEEP | Yes | Clear | None |
| test_negative_oversized_query_is_refused (439) | KEEP | Yes | Clear | None |
| test_negative_invalid_sql_query (455) | KEEP | Yes | Clear | None |
| test_positive_audit_trail (472) | MOVE | Counts audit decisions through hooks | Clear | Rust server/audit test |
| test_read_back_as_arrow_pandas_and_polars (547) | KEEP | Yes | Exemplary | None |
| test_sql_returns_model_instances_when_a_model_is_supplied (585) | KEEP | Yes | Exemplary | None |
| test_analytical_sql_over_written_tables (621) | MOVE | Tests DataFusion SQL semantics, not the SDK | Clear | Rust Oracle tests |
| test_standard_otel_tracer_exports_to_bifrost (771) | TIGHTEN | Yes; hand-built exporter config; protobuf decoding | Positional `rows.column(...)[index]` juggling | Read with `sql(query, SpanRow)`; use an SDK exporter helper |
| test_stdlib_logging_exports_to_bifrost (866) | TIGHTEN | Same | Same | Same |
| test_standard_otel_metrics_export_to_bifrost (926) | TIGHTEN | Same | Same | Same |
| test_delegated_client_reads_as_a_and_cannot_write_with_b_authority (1032) | TIGHTEN | urllib token exchange; fixture hook | Clear story | Use the SDK token method; seed through the SDK |
| test_client_cannot_be_combined_with_transport_options (1070) | KEEP | Yes | Clear | None |

### smoke/test_gateway_live_providers.py
Opt-in live-provider connectivity through the gateway. It is credential-safe and uses third-party clients as users do.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_openai_compatible_client_reaches_a_live_provider (228) | KEEP | Yes | Clear | Replace the httpx token helper (:178) once the SDK has one |
| test_anthropic_client_reaches_live_anthropic (244) | KEEP | Yes | Clear | None |
| test_google_client_reaches_live_gemini (263) | KEEP | Yes | Clear | None |
| test_openai_client_reaches_live_responses (291) | KEEP | Yes | Clear | None |
| test_openai_client_reaches_live_embeddings (323) | KEEP | Yes | Clear | None |
| test_openai_client_round_trips_live_audio (334) | KEEP | Yes | Clear | None |
| test_openai_client_reaches_live_images (363) | KEEP | Yes | Clear | None |
| test_openai_client_runs_a_live_batch_lifecycle (378) | KEEP | Yes | Clear | None |

## Good examples to keep
- integration/cards/test_cards_crud.py:195 `test_data_card_pandas_interface_eager_loads_after_real_registry_round_trip`: a typed DataCard, an SDK round trip, and an assertion on the returned frame.
- integration/cards/test_cards_crud.py:283 (sklearn eager load), :319 (replay idempotent), :337 (version modes).
- integration/bifrost/test_bifrost_e2e.py:547 (Arrow/pandas/polars read-back), :585 (`sql(query, Model)`), :369 (schema conflict), :351 (denied write has no effect), :426/:439/:455 (query floor refusals), :1070 (client/credential conflict).
- integration/state/test_state_journey.py:295, :325, :263: offline negative hydration with stable codes, built on a checked-in fixture graph (lines 30-33). This fixture style is the model for REQ-192.
- integration/state/test_observe_journey.py:407-425 `emit_framework_scope`: reads like real framework code inside `with state.run(card=...)`.
- integration/state/test_observe_journey.py:694 burst test: a clear, quantified story with a read-back.
- integration/gateway/test_gateway_native_clients.py:62-74 and test_gateway_openai_client.py:278-283: unmodified third-party clients used exactly as users would.
- integration/test_operator_connections_journey.py `assert_redacted` (20-23): public SDK only, typed requests.
- bifrost/test_public_typing.py and the whole smoke/ file.

## Proposed target structure
- **Fixtures.** Use a shared repository-level `tests/fixtures/cards/` (REQ-192, shared across Rust, Python, and TS) with one directory per graph: `typed-state/` (moved from `crates/wyrd/wyrd-cli/tests/fixtures/...`), `observe-service/`, `verified-service/` (Model + Agent + Drift/Eval Verifiers + an `http` Operator with a named connection and path-only `url`, no digests), and `baseline/` data. No YAML in Python.
- **`tests/conftest.py`.** Provide a session `wyrd_server` (`mutate_env=False`), `admin` and `writer` `Cards` fixtures, `registered(fixture_dir) -> WyrdState` (SDK download once the gap closes), an `operator_endpoint` local receiver that records requests, and a `judge_provider` mock. Keep the gateway `Upstream` in `integration/gateway/conftest.py`. Delete support.py's raw-HTTP helpers as the SDK gains their capabilities.
- **`tests/integration/` by user story.**
  - `cards/`: `test_register_get.py`, `test_registration_errors.py`
  - `state/`: `test_hydrate.py`, `test_bundle_trust.py`
  - `observe/`: `test_emit_and_read_back.py`, `test_scopes_and_otel.py`, `test_backpressure.py`
  - `verify/`: `test_observe_verify.py` (REQ-188: Eval and Drift pass/fail, unknown verifier, wrong shape, RBAC, tenant) and `test_binding_delivers_operator.py` (`make_binding_due` triggers a request at a local endpoint; `observations_ready` Eval)
  - `bifrost/`: `test_tables.py`, `test_query.py`, `test_query_refusals.py`, `test_otel_export.py`, `test_delegation.py`
  - `gateway/`: split per story
  - `operators/test_connections.py`
- **`tests/unit/`.** Receives the error contract, the reserved-column check, and the trimmed harness tests.
- **Moved to Rust.** Drift method edges, PSI/SPC statistics, legacy/weco baselines, judge deadline, query EOS/fault/cancel/resources, audit counts, binding-id stability, CLI error output, and analytical SQL.
