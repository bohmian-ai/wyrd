# Python unit SDK test audit

All paths are relative to `sdks/wyrd-sdk-python/tests/unit/` unless stated.
Read-only audit. No test was run.

## Summary
- Files audited: 72 (67 test modules, `cards/model/_helpers.py`, `state/support.py`, three empty `__init__.py`); test functions: 461 (a parametrized function counts once).
- Verdict counts: KEEP 183, TIGHTEN 132, REWRITE 70, MOVE 12, DELETE 64.
- The core authoring tests are mostly sound. Prompt templating, media binding, model settings, CardRef validation, workflow wiring, WyrdState hydration errors and the OTel run-scope tests call the public API and pin stable error codes. Those are the 183 KEEPs. What hurts readability is mostly in the data and model card files. 29 DataCard round-trips and every ModelCard round-trip reload a saved card by reading `card.json` by hand into `model_validate_json` and then calling `load(path)`, because the SDK has no `DataCard.from_path` or `ModelCard.from_path`. About 20 more tests build an invalid card by editing a `model_dump_json()` dict, because DataCard cannot be authored with `splits=` or `target_columns=`. Card YAML is written in code across 8 files, including the whole WyrdState bundle in `state/support.py`, which REQ-192 forbids. The tests are also heavily duplicated: 64 DELETEs, mostly copies, name-bans and checks of the test double itself. Every "accepts" test for observation inputs proves acceptance only by catching `WYRD_SDK_400_BIFROST_NOT_STARTED`. `test_verification_surface.py` tests the `Verification` handle that REQ-189 removes, and nothing in this tier covers `observe.verify` (REQ-188).

## API gaps revealed by the tests
| Gap | Evidence (file:line) | What a user should be able to write instead |
|---|---|---|
| No loader for a saved DataCard or ModelCard directory | `cards/data/test_datacard_save_load.py:112` (same at 128, 144, ... 588; 29 times), `cards/model/test_modelcard_save_load.py:96`, `cards/model/test_tensorflow_modelcard.py:31`, `cards/data/test_datacard_custom_subclass.py:36` | `card = DataCard.from_path(path)` / `ModelCard.from_path(path, interface=...)`, which loads the envelope and data in one call, as `PromptCard.from_path` already does |
| Data and Model Cards cannot be loaded from a YAML file | `cards/data/test_datacard_yaml.py:11-36`, `cards/model/test_modelcard_serde.py:59-89`, `cards/data/test_datacard_validation.py:73` (`yaml.safe_load` then `json.dumps` then `model_validate_json`) | `DataCard.from_path(FIXTURES / "data/churn-train.yaml")` |
| DataCard has no typed authoring for splits or target columns | `cards/data/test_datacard_yaml.py:46-51`, `cards/data/test_datacard_splits.py:81-90`, `:98-103`, `cards/data/test_datacard_validation.py:19-20`, `:46-48` (edit the `payload["spec"]` dict, then revalidate) | `DataCard(df, splits={"train": Split.column("year", "<=", 2024)}, target_columns=["churn"])`, plus a typed `card.splits` |
| Interface options cannot be read back as typed values | `cards/data/test_datacard_interfaces_explicit.py:23-60`, `cards/data/test_datacard_interface.py:134-182` (`to_dict()["meta"]["compression"] == "Zstd"` exposes Rust enum casing) | `PandasInterface(compression="zstd").compression == "zstd"` |
| No documented offline model provider. `provider="mock"` appears in no public stub | `runtime/test_callbacks.py:11`, `runtime/workflow/test_workflow_parameter_injection.py:9`, `runtime/agent/test_agent_parsed_output.py:35` (about 40 tests) | A documented `Prompt.mock(...)` or `provider="mock"` in `wyrd.prompt` stubs, with defined echo semantics |
| No way to set a canned model response, so tests fake one by returning raw OpenAI JSON from `after_model_callback` | `runtime/agent/test_agent_parsed_output.py:10-24`, `:38`, `runtime/agent/test_agent_structured_output.py:7-20`, `runtime/test_callbacks.py:22-38` | `Prompt.mock(messages=..., responses=['{"summary":"hi"}'])` or a typed `ModelResponse.text("...")` as the callback return |
| Agent-to-card returns an untyped dict | `runtime/agent/test_agent_journey.py:48-63`, `runtime/agent/test_agent_no_wrapper_class.py:34-37` | `card: AgentCard = agent.to_card()`, then `card.name` and `card.prompt` |
| Callback context is an untyped dict | `runtime/test_callbacks.py:18`, `runtime/test_session.py:67-69` (`ctx["conversation"]["turns"]`, turns keyed `type`, not `role`), `runtime/agent/test_agent_journey.py:122` | `ctx.agent_id`, `ctx.conversation.turns[0].role` |
| Prompt has no typed response-schema accessor | `cards/prompt/test_prompt_structured_output.py:18-26` (a helper that branches on 4 provider body shapes), `cards/prompt/test_prompt_output_schema.py:16`, `cards/prompt/test_prompt_output_schema_rust.py:22-26` | `prompt.response_schema` (a JSON Schema dict) and `prompt.response_schema_name` |
| No SDK call for the trusted artifact-manifest hash. The test reimplements the canonical blake3 hashing | `state/support.py:386-406`, used at `state/test_hydration.py:114`, `:144` | `WyrdState.artifact_manifest_hash(bundle, "model")`, or the hash returned by whatever SDK call writes the bundle |
| WyrdConfig has no readback, so a test asserts on `repr` | `config/test_load.py:15-16` | `cfg.defaults.space == "prod"` and `cfg.defaults.labels["team"]` |
| Observation acceptance cannot be checked offline. Every positive input-form test asserts `BIFROST_NOT_STARTED` as a stand-in | `state/test_observe_surface.py:93-97`, `:100-112`, `:136-140`, `:195-200`, `:253-254`, `:265-270`, `:549-550` | Either drop the positive cases from unit (the journey proves admission) or expose a pure validator, for example `Observation.drift(payload)` that returns the converted row or raises |
| ModelSignature shapes are raw dicts | `cards/model/test_signature_sample.py:18-22`, `:65-69`, `cards/model/test_modelcard_validation.py:37-47` | `Dim.dynamic("batch")`, `Dim.fixed(3)`; `FieldSpec("x", "float64", shape=[Dim.fixed(0)])` |

## Cross-cutting problems
1. **Card YAML and JSON built in code instead of checked-in fixtures (REQ-192).** About 35 tests. Examples: `state/support.py:192-383`, which writes an entire seven-Card bundle with hand-computed digests and feeds 26 hydration and 36 observe tests; `cards/prompt/test_prompt_declarative.py:17-58`; `cards/test_prompt_model_settings.py:187-204`; `cards/data/test_datacard_validation.py:73-83`; `cli/test_cli_surface.py:24-62`; `state/test_wyrdstate_surface.py:23-35`. Fix: add `tests/fixtures/` with `cards/` (one YAML per Card, including deliberately invalid ones) and `bundles/complete/`, `bundles/builtin-model/` and `bundles/metadata-only/`, and load them with `from_path`. Copy a bundle into `tmp_path` only when a test mutates it.
2. **Reloading a saved card by reading `card.json` by hand.** 40 tests. Examples: `cards/data/test_datacard_save_load.py:112`, `cards/model/test_modelcard_save_load.py:94-103` (the `_round_trip` helper also hides the asserts), `cards/model/test_tensorflow_modelcard.py:31`. Fix: add `from_path` (gap 1), then collapse the 29 DataCard cases into one parametrized table of `(input, kind, artifact_file)`.
3. **Editing serialized dicts to build cards.** About 20 tests. Examples: `cards/data/test_datacard_splits.py:80-90`, `cards/data/test_datacard_validation.py:10-15` (`_payload()` round-trip surgery), `cards/prompt/test_prompt_authoring.py:193-195`, `cards/prompt/test_promptref.py:30-31`. Fix: build valid cards with typed constructors (gap 3) and invalid ones from checked-in fixture files.
4. **Duplicate tests.** 50 or more. `cards/data/test_datacard_interface.py:134-188` repeats `test_datacard_interfaces_explicit.py:22-85` almost line for line. `cards/prompt/test_prompt_output_schema_rust.py` repeats `test_prompt_output_schema.py`. `cards/prompt/test_errors.py` repeats codes covered in render, media and load. Module-identity checks appear 5 times (`cards/prompt/test_module_layout.py:12`, `state/test_public_surface.py:13`, `state/test_wyrdstate_surface.py:12`, `config/test_public_surface.py:12`, `contracts/test_negative_pins.py:4`). `cards/data/test_datacard_validation.py:89` repeats `test_datacard_interfaces_explicit.py:72`. Fix: delete per the tables below and keep one owner per behaviour.
5. **Name-bans and hasattr checks for removed legacy surfaces.** 14 tests. Examples: `cards/prompt/test_module_layout.py:19-22`, `runtime/agent/test_agent_no_wrapper_class.py:15-24`, `state/test_public_surface.py:20-24`, `cards/test_registry_surface.py:49-64`, `runtime/agent/test_agent_save_load.py:20-21` (`"version" + "_req"`), `cards/test_prompt_model_settings.py:140-146`. Per AGENTS.md section 12, these protect nothing reachable. Delete them. Keep only the live security boundary at `gateway/test_gateway_surface.py:38`.
6. **Private extension imports.** 6 sites: `bifrost/test_bifrost.py:9-10`, `:35-37`, `gateway/test_gateway_surface.py:30`, `cards/prompt/test_prompt_output_schema_rust.py:8`, `cards/test_card_ref.py:4` (`wyrd._wyrd.WyrdError`), and `state/test_observe_surface.py:326`, `:541`, `:544-545` (OTel private `_active_span_processor`, `wyrd.otel._SCOPE_KEY`, `_otel_context`). Fix: import `WyrdError` from `wyrd`, delete the extension-path tests, and assert observable span attributes instead of processor counts.
7. **Packaging, docstring and stub-text lint written as unit tests.** 12 tests. Examples: `cards/data/test_public_surface.py:27-68`, `cards/model/test_model_public_surface.py:46-91`, `state/test_public_surface.py:27-113`, `cards/test_registry_surface.py:28-34`, and the `ty` fixtures in `gateway/test_gateway_typing.py` and `test_operator_connections_typing.py`. Fix: move them to `codegen:check` or a `check:py-stubs` lane, and put the `ty` fixtures in a `typecheck/` directory that pytest does not collect.
8. **Testing the test double or Python itself.** 9 tests. Examples: `runtime/test_observer.py:10-114` (calls methods on its own Observer subclass and tests `threading`), `cards/data/test_datacard_custom_subclass.py:41-46` (`isinstance`, and `super_called` set by the test class). Delete them. `runtime/workflow/test_workflow_observers.py` already proves real dispatch.
9. **Several stories in one test, or loops over lambdas.** About 15 tests. Examples: `cards/prompt/test_errors.py:12-96`, `cards/prompt/test_prompt_media.py:110-124`, `state/test_hydration.py:73-118` (three stories, one with a reimplemented hash), `cards/prompt/test_prompt_authoring.py:131-188` (a loop with branching asserts), `cli/test_cli_surface.py:21-104`. Fix: one parametrized case per story, and `pytest.mark.parametrize` instead of `for fn, code in cases`.
10. **Weak or permissive assertions.** About 20 tests. Some raise a bare `WyrdError` with no code: `cards/data/test_datacard_validation.py:32`, `:40`, `:60`, `:85`, `cards/data/test_datacard_save_load.py:600`, `:629`, `cards/prompt/test_prompt_declarative.py:151`. Some accept any of several codes, including a legacy `SKALD_` one: `cards/prompt/test_prompt_output_schema.py:77-82`. One accepts either a Wyrd or a Pydantic error: `runtime/agent/test_agent_parsed_output.py:128`. One asserts "code is not X": `client/test_client.py:36`. Fix: pin the single catalog code.
11. **Credential-chain scrubbing copied three times.** `bifrost/test_bifrost.py:24-26`, `client/test_client.py:11-13`, `cards/test_registry_surface.py:71-73`. Fix: one `no_credentials` fixture in `tests/unit/conftest.py`.
12. **Test modules import from each other and from sys.path helpers.** `cards/model/test_modelcard_serde.py:9` and `cards/model/test_modelcard_validation.py:7` import `_sklearn_model` from `test_modelcard_save_load`. `from _helpers import` depends on rootdir import mode. Fix: move model factories into `cards/model/conftest.py` fixtures.
13. **Spec revision 64 drift.** `test_verification_surface.py:7-25` tests the `Verification` handle that REQ-189 removes. `state/test_observe_surface.py:276-292` uses `state.run(card=...)`, which REQ-188 changes to a positional argument. No unit test covers `observe.verify` failing locally with `WYRD_SDK_404_UNKNOWN_VERIFIER` or `WYRD_SDK_400_INVALID_OBSERVATION` (REQ-188). That is a missing test, and it belongs next to the drift and eval refusal tests.
14. **Misleading names.** Examples: `cards/data/test_datacard_interfaces_explicit.py:64` and `:68` ("requires_manifest_or_directory", but they only assert `has_source is False`), `:79` ("round_trips_extra", asserts kind only), `cards/data/test_datacard_interface.py:203` and `:209` ("to_data_spec", "to_card_body"; these are internal names), `cards/data/test_datacard_custom_subclass.py:92` ("cards_get_style"), `runtime/agent/test_agent_journey.py:13`. That last one is called a journey, but it runs a different mock agent rather than the one it loaded.

## Per-file findings

### bifrost/test_bifrost.py
Covers Bifrost construction and table config.
| Test | Verdict | Q1 production behaviour | Q2 user understanding | Fix |
|---|---|---|---|---|
| test_extension_submodules_import :8 | DELETE | Private `wyrd._wyrd` import | n/a | Delete |
| test_bifrost_without_a_resolvable_credential_raises :13 | TIGHTEN | Yes | Copied env scrub | Use shared `no_credentials` fixture |
| test_producer_key_and_client_scope_are_not_importable :33 | DELETE | Name-ban on a private path | n/a | Delete |
| test_table_config_carries_an_optional_compaction_target :40 | TIGHTEN | Yes | Two stories; `256*1024*1024` and the table name are unexplained | Split Pydantic and Arrow cases; name the constant |

### cards/data/test_datacard_custom_subclass.py
Covers Python-subclassed DataInterface.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_subclass_constructs_and_is_instance_of_base :41 | DELETE | Tests Python inheritance | n/a | Delete |
| test_subclass_init_calls_super_init :45 | DELETE | Tests the test class | n/a | Delete |
| test_subclass_kind_is_custom :49 | DELETE | Duplicate of :82 | n/a | Delete |
| test_subclass_inherits_from_metadata_default :53 | TIGHTEN | Yes, extension hook | OK | Merge with :105 as default and override cases |
| test_datacard_accepts_custom_subclass_interface :60 | DELETE | Duplicate of :74 | n/a | Delete |
| test_datacard_constructor_rejects_custom_interface_class :67 | KEEP | Yes | Clear | None |
| test_custom_subclass_save_returns_datastats_and_card_save_records_it :74 | KEEP | Yes | Clear | None |
| test_custom_subclass_serializes_as_custom_with_empty_loader_fields :82 | TIGHTEN | Yes | Asserts on a dict | Merge with :142; use `card.interface` typed fields |
| test_custom_subclass_cards_get_style_resolves_interface_class_and_loads :92 | TIGHTEN | Yes | Helper hides the `card.json` workaround; misleading name | `DataCard.from_path(path, interface=JsonInterface)` (gap) |
| test_custom_subclass_from_metadata_override_receives_card_metadata :105 | KEEP | Yes | Clear | None |
| test_model_validate_json_interface_none_for_subclass_card_raises :120 | KEEP | Yes | Clear | None |
| test_base_save_load_without_override_raise_required_override :130 | TIGHTEN | Yes | Two stories | Split save and load |
| test_custom_subclass_class_path_is_not_stored_or_reimported :142 | DELETE | Duplicate of :82 | n/a | Delete |

### cards/data/test_datacard_dtype.py
Covers dtype inference per framework.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_pandas/polars/pyarrow/numpy/torch_dtype_table :20, :29, :38, :47, :54 (5) | TIGHTEN | Yes | Chained one-expression asserts; five copies | One parametrized `(data, expected_dtype)` test |
| test_unknown_dtype_raises_unknown_data_type :61 | KEEP | Yes | Clear | None |

### cards/data/test_datacard_interface.py
Covers interface inference, option mapping and Artifact inputs.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| pandas/polars/pyarrow/numpy/torch/huggingface `_captures_*` :36, :45, :54, :63, :71, :78 (6) | TIGHTEN | Yes | Dict digging; overlaps the dtype file | Parametrize; assert `card.schema.columns`, `card.interface.kind` |
| test_sql_interface_captures_queries :86 | TIGHTEN | Yes | Dict | Typed `card.sql.queries` |
| test_parquet_interface_uses_path_metadata :95 | TIGHTEN | Yes | Dict | Typed schema |
| test_jsonl/image/text_interface_uses_*_metadata :104, :111, :119 (3) | DELETE | Duplicates `test_datacard_save_load.py:232/264/280` | n/a | Delete |
| test_datacard_rejects_non_data_interface :127 | KEEP | Yes | Clear | None |
| `*_maps_to_rust_*_meta` :134, :138, :142, :146, :150, :154, :158, :167, :171, :175, :179, :186 (12) | DELETE | Duplicates `test_datacard_interfaces_explicit.py`; names are internal | n/a | Delete |
| test_interface_metadata_contains_no_live_python_objects :197 | DELETE | Duplicate of `test_datacard_serde.py:44` | n/a | Delete |
| test_datacard_to_data_spec_uses_rust_interface_metadata :203 | DELETE | Internal name; duplicate | n/a | Delete |
| test_datacard_to_card_body_returns_data_variant :209 | DELETE | Trivia | n/a | Delete |
| test_artifact_card_input_requires_or_infers_interface_metadata :213 | TIGHTEN | Yes | `metadata.to_dict()` dict | Typed `card.card_refs[0]` |
| test_artifact_card_input_rejects_non_artifact_card_ref :227 | KEEP | Yes | Clear, with details | None |
| test_manifest_ref_allows_authored_ref_without_space :243 | TIGHTEN | Yes | Raw dict ref | Pass `CardRef`-like input; typed readback |
| test_set_interface_replaces_spec_metadata_and_schema :255 | KEEP | Yes | Clear | None |

### cards/data/test_datacard_interfaces_explicit.py
Covers interface constructor options.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| pandas, polars, arrow, parquet, numpy, torch, jsonl option tests :22, :26, :30, :34, :38, :45, :57 (7) | TIGHTEN | Yes | Rust enum casing `"Zstd"` in a dict | Parametrize; typed option readback (gap) |
| test_explicit_sql_interface_requires_sql_dict :51 | TIGHTEN | Yes | Misnamed; dict | Rename; typed `card.sql` |
| test_explicit_image/text_interface_requires_manifest_or_directory :64, :68 (2) | TIGHTEN | Yes | Misnamed (asserts `has_source`) | Rename to `..._has_no_source_until_data_given` |
| test_explicit_huggingface_interface_revision_validation :72 | KEEP | Yes | Clear | Owner of this check |
| test_explicit_custom_interface_round_trips_extra :79 | DELETE | Duplicate of custom-subclass tests; misnamed | n/a | Delete |
| test_interface_data_conflict_raises_validation_error :88 | KEEP | Yes | Clear | None |
| test_invalid_interface_option_lists_allowed_values :95 | KEEP | Yes | Clear | None |

### cards/data/test_datacard_save_load.py
Covers local save/load round-trips for every interface.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| Raw-input round-trips :104, :120, :136, :152, :168, :184, :200, :216, :232, :248, :264, :280 (12) | REWRITE | Yes, but reloads through hand-read `card.json` (gap) | 12 near-identical bodies | One parametrized `(raw_input, kind)` test using `DataCard.from_path` |
| Interface round-trips :296, :314, :332, :350, :368, :386, :404, :422, :440, :458, :475, :492, :509, :526, :544, :562, :580 (17) | REWRITE | Same workaround; asserts on-disk filenames (`data/data.npz`) as trivia | 17 copies | One parametrized `(interface, kind)` table; drop the filename asserts |
| test_load_without_path_requires_registry_configuration :597 | TIGHTEN | Yes | Bare `WyrdError` | Pin the code |
| test_save_does_not_create_artifact_cards_or_card_refs :604 | TIGHTEN | Yes | Reads a JSON dict | Typed `card.card_refs == []` |
| test_huggingface_pointer_only_save_writes_pointer_json :613 | TIGHTEN | Yes | Filename trivia | Assert reload yields the same pointer |
| test_huggingface_pointer_load_without_allow_remote_raises :623 | TIGHTEN | Yes | Bare error | Pin the code |

### cards/data/test_datacard_serde.py
Covers DataCard JSON serde and user metadata.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_model_dump_json_round_trips_through_rust_for_all_interfaces :20 | TIGHTEN | Yes | Loop; unused `tmp_path` | Parametrize |
| test_model_validate_json_rehydrates_interface_from_metadata :35 | KEEP | Yes | Clear | None |
| test_serialized_datacard_contains_spec_interface_metadata_not_python_state :44 | TIGHTEN | Yes | Dict | Keep as the single owner of this check |
| test_datacard_str_is_pretty_card_json :52 | KEEP | Yes | Clear | None |
| test_labels_annotations_replace_tags_in_metadata :59 | TIGHTEN | Yes | Two stories, including a legacy `tags` ban | Drop the `tags` part |
| test_user_metadata_rejects_invalid_reserved_and_secret_values :77 | TIGHTEN | Yes | Loop | Parametrize with ids |
| test_model_validate_json_rejects_wrong_card_kind :91 | TIGHTEN | Yes | Inline dict; bare error | Fixture file; pin the code |

### cards/data/test_datacard_splits.py
Covers Split builders and splits on cards.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_split_column_eq_ne_gt_ge_lt_le_in :10 | TIGHTEN | Yes | Loop over Rust tokens | Parametrize |
| test_split_column_invalid_operator_raises_invalid_split_rule :17 | KEEP | Yes | Clear | None |
| test_split_index_range_serializes_start_stop :24 | TIGHTEN | Yes | `to_dict` | Typed readback |
| test_split_index_range_rejects_negative_start_or_stop :28 | KEEP | Yes | Clear | None |
| test_split_index_range_start_after_stop_rejected :35 | KEEP | Yes | Clear | None |
| test_split_indices_serializes_values :42 | TIGHTEN | Yes | `to_dict` | Typed readback |
| test_split_indices_rejects_negative_values :46 | KEEP | Yes | Clear | None |
| test_split_indices_rejects_duplicate_values :53 | KEEP | Yes | Clear | None |
| test_split_materialized_accepts_artifact_card_ref :60 | TIGHTEN | Yes | Raw dict; the stub accepts `CardRefLike` | Pass `CardRef(...)` |
| test_split_materialized_allows_authored_ref_without_space :69 | TIGHTEN | Yes | Dict | Same |
| test_mixed_materialized_ref_and_rule_based_splits_round_trip :78 | REWRITE | Payload surgery (gap) | Hard to read | `DataCard(..., splits={...})` |
| test_split_key_must_match_serialized_label :95 | REWRITE | Payload surgery | Same | Invalid fixture file |
| test_datacard_does_not_expose_split_data_execution :106 | DELETE | Name-ban | n/a | Delete |
| test_split_indices_rejects_empty_list :110 | KEEP | Yes | Clear | None |

### cards/data/test_datacard_validation.py
Covers rejection of invalid DataCard envelopes.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :18, :28, :36, :44, :56, :64, :72 (7) | REWRITE | Right rules, but cards are built by dict surgery or an inline dict; 4 of them (:32, :40, :60, :85) assert a bare error | Opaque `_payload()` mutation | One invalid YAML fixture per rule, loaded with `from_path`, each pinning its code |
| test_huggingface_revision_must_be_hex_7_to_40_chars :89 | DELETE | Duplicate of `test_datacard_interfaces_explicit.py:72` | n/a | Delete |

### cards/data/test_datacard_yaml.py
Covers loading a DataCard from YAML.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_datacard_yaml_fixture_loads_through_public_surface :10 | REWRITE | Inline YAML string converted to JSON; not a fixture | Misleading name | Checked-in YAML + `DataCard.from_path` (gap) |
| test_datacard_yaml_round_trips_locked_envelope_with_splits :42 | REWRITE | Mostly tests PyYAML and dict edits | Opaque | Typed splits + save/`from_path` |

### cards/data/test_public_surface.py
Covers package exports and stub lint.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_wyrd_data_all_exports_are_importable :22 | TIGHTEN | Marginal | OK | Fold into one `test_public_exports.py` |
| test_top_level_exports_match_init_all :27 | MOVE | Parses `__init__.py` AST; packaging lint | n/a | `codegen:check` / stub lane |
| test_data_stub_public_classes_and_methods_have_docstrings :35 | MOVE | Docs lint | n/a | Stub lint lane |
| test_wyrd_python_package_layout :55 | MOVE | File-layout lint | n/a | Packaging check |

### cards/model/test_model_public_surface.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_wyrd_model_all_exports_are_importable :41 | TIGHTEN | Marginal | OK | Fold into the shared exports test |
| test_top_level_model_exports_match_init_all :46 | MOVE | Duplicate AST lint | n/a | Stub lane |
| test_model_stub_public_classes_and_methods_have_docstrings :56 | MOVE | Docs lint | n/a | Stub lane |
| test_model_stub_args_docs_use_name_type_description_format :76 | MOVE | Docs-format lint | n/a | Stub lane |

### cards/model/test_modelcard_save_load.py
Covers ModelCard framework autodetection and round-trips.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_raw_sklearn_model_autodetects_and_round_trips :108 | TIGHTEN | Yes | `_round_trip` hides the reload workaround and the asserts | `ModelCard.from_path` (gap); inline the asserts |
| test_raw_model_autodetection_for_framework_models :133 | TIGHTEN | Yes | Repeats the helper's assert | Same |
| test_sklearn_interface_loads_a_local_materialization :145 | KEEP | Yes | Clear | None |
| test_modelcard_projects_model_and_preprocessor_directly :164 | KEEP | Yes | Clear | None |
| test_xgboost/lightgbm/catboost_interface_round_trips :179, :188, :197 (3) | TIGHTEN | Yes, but duplicates :133 with explicit interfaces | Copies | Add an `explicit` axis to the :133 parametrization |
| test_torch_interface_round_trips_with_safetensors / _with_pickle :206, :219 | TIGHTEN | Yes | Filename trivia | Parametrize on save_format |
| test_lightning_interface_round_trips_with_checkpoint :231 | TIGHTEN | Yes | 35 lines of setup before the point | Move the model and trainer into fixtures |
| test_lightning_interface_round_trips_trainerless_checkpoint :272 | TIGHTEN | Yes | Duplicate class definition | Shared fixture |
| test_huggingface_interface_round_trips_from_local_pretrained_dir :292 | TIGHTEN | Yes | Filename trivia | Same as above |
| test_custom_python_model_interface_round_trips_with_explicit_loader :304 | TIGHTEN | Yes | Reads well apart from the `card.json` read | `from_path` |
| test_joblib_none_model_artifact_raises_model_validation_error :337 | KEEP | Yes | Clear | None |
| test_modelcard_save_without_live_model_raises_model_error :351 | KEEP | Yes | Clear | None |
| test_modelcard_metadata_to_dict_accepts_interface_instance :364 | TIGHTEN | Yes | `to_dict` with Rust casing | Typed `metadata.task_type` |
| test_sample_input_round_trips_through_modelcard :376 | TIGHTEN | Yes | `card.json` read; `to_dict` | `from_path`; typed sample |

### cards/model/test_modelcard_serde.py
Imports a factory from another test module (:9).
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_model_dump_json_round_trips_through_public_validator :15 | TIGHTEN | Yes | Mixes typed and dict asserts | Typed asserts only |
| test_modelcard_str_is_pretty_card_json :35 | KEEP | Yes | Clear | None |
| test_model_validate_json_rejects_wrong_card_kind :46 | TIGHTEN | Yes | Inline dict; bare error | Fixture; pin the code |
| test_modelcard_yaml_fixture_loads_through_public_surface :58 | REWRITE | Inline YAML (REQ-192) | Misleading name | Checked-in YAML + `from_path` |
| test_modelcard_yaml_round_trips_locked_envelope :96 | DELETE | Tests PyYAML; duplicate of :15 | n/a | Delete |

### cards/model/test_modelcard_validation.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :19, :26, :58, :65, :73, :95, :108, :121 (8) | KEEP | Yes, with stable codes | Clear | None. Note that :121 expects a `WYRD_DATA_` code from a model interface; confirm that is intended |
| test_invalid_signature_shape_raises_stable_model_error_code :36 | TIGHTEN | Yes | Raw shape dict | Typed `Dim` (gap) |
| test_unknown_model_artifact_path_does_not_leak_absolute_path :83 | TIGHTEN | Yes (security) | Duplicates :73 setup | Merge into :73 |
| test_custom_json_without_explicit_interface_raises_stable_model_error_code :128 | REWRITE | Inline card dict | Long setup | Invalid fixture file |

### cards/model/test_signature_sample.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :25, :49, :72 (3) | TIGHTEN | Yes | Three frameworks per test; raw shape dicts | Parametrize; typed `Dim` |
| test_sample_input_classification_save_and_load :106 | TIGHTEN | Yes | Asserts on-disk filenames | Assert reload equality instead |

### cards/model/test_tensorflow_modelcard.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :23, :41, :59, :80 (4) | TIGHTEN | Yes | `card.json` read; filename trivia | `from_path`; parametrize save_format |

### cards/prompt/test_errors.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_prompt_envelope_error_codes :12 | REWRITE | Yes; `UNDECLARED_PLACEHOLDER` is covered only here | Loop over lambdas | Move each case into its owning file as a named test |
| test_media_error_codes :36 | REWRITE | Mostly duplicates; `MISSING_MEDIA_VARIABLE` is unique | Same | Same |
| test_loader_and_settings_error_codes :72 | REWRITE | Duplicates load and settings | Same | Delete the duplicates; keep the `str(error)` format check once |

### cards/prompt/test_module_layout.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_wyrd_prompt_exposes_prompt_and_promptcard :6 | TIGHTEN | Marginal | OK | Shared exports test |
| test_wyrd_state_exposes_local_projections :12 | DELETE | Duplicate (third copy) | n/a | Delete |
| test_top_level_runtime_task_and_embedder_are_absent :19 | DELETE | Name-ban | n/a | Delete |
| test_top_level_workflow_is_exposed :25 | TIGHTEN | Marginal | OK | Shared exports test |

### cards/prompt/test_prompt_authoring.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :18, :29, :57, :110, :123 (5) | TIGHTEN | Yes | Deep `request.model_dump()[...]` paths | Typed `request.openai()` / `.anthropic()` views |
| test_bind_returns_new_prompt_and_bind_mut_updates_in_place :42 | KEEP | Yes | Clear | None |
| test_bind_media_requires_existing_placeholder :68 | DELETE | Duplicate of `test_prompt_media_binding.py:50` | n/a | Delete |
| test_prompt_response_format_and_str_are_json_inspectable :75 | TIGHTEN | Yes | Two stories | Split |
| test_every_provider_builder_constructs_promptcard_and_round_trips_json :91 | TIGHTEN | Yes | Loop; the vertex-to-google special case is hidden | Parametrize with expected provider |
| test_provider_settings_constructor_smoke :131 | TIGHTEN | Yes | Branching asserts inside a loop | Parametrize; drop the branches |
| test_openai_responses_input_getter_projects_text_and_item_forms :191 | REWRITE | Builds the text form by JSON surgery | Opaque | Fixture file for the text-form request |

### cards/prompt/test_prompt_declarative.py
Card YAML lives in module string constants (:17-58).
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_promptcard_constructor_with_python_prompt :61 | KEEP | Yes | Clear | None |
| :72, :84, :94, :103, :113, :122 (6) | TIGHTEN | Yes | YAML strings in code | Checked-in `fixtures/cards/prompt/*.yaml` |
| test_saved_card_has_no_spec_type_field :135 | DELETE | Legacy ban; duplicate of `test_promptcard_roundtrip.py:40` | n/a | Delete |
| test_bad_provider_raises_error :147 | TIGHTEN | Yes | Bare error | Pin the code; invalid fixture file |
| test_load_is_alias_for_from_path :155 | KEEP | Yes | Clear | None |

### cards/prompt/test_prompt_envelope.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_happy_path_promptcard_construction :6 | KEEP | Yes | Clear | None |
| test_promptcard_json_envelope_shape :13 | TIGHTEN | Yes | Dict | Typed `card.name`; fold into the round-trip file |

### cards/prompt/test_prompt_load.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_prompt_load_yaml_and_json_preserve_deep_fields :85 | REWRITE | Yes | `message_text` helper branches on 4 body shapes; `if name != "raw_v1"` inside the test | Split raw from typed providers; use typed request views |
| test_prompt_load_raw_v1_preserves_provider_and_body :107 | KEEP | Yes | Clear | None |
| test_prompt_load_error_codes :118 | KEEP | Yes | Clear | Optionally parametrize |
| test_prompt_load_declarative_yaml_with_model_settings :138 | TIGHTEN | Yes | YAML via `safe_dump`; duplicate of `test_prompt_model_settings.py:124` | Fixture file; keep one |

### cards/prompt/test_prompt_media.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :11, :43, :57, :71 (4) | TIGHTEN | Yes | Two or three stories each; deep dict paths | Split; typed views |
| :34, :83, :94, :101 (4) | KEEP | Yes | Clear | None |
| test_oversized_file_directory_and_system_media_raise_exact_codes :110 | TIGHTEN | Yes | Three stories; overlaps `test_prompt_media_binding.py:59` | Parametrize |

### cards/prompt/test_prompt_media_binding.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :11, :19, :50, :79, :86 (5) | KEEP | Yes (:79 pins injection safety) | Clear | None |
| test_bind_media_openai_anthropic_gemini_and_vertex_native_replacement :30 | TIGHTEN | Yes | Four providers in one test | Parametrize |
| test_system_message_media_rejection_raises_exact_code :59 | DELETE | Duplicate of `test_prompt_media.py:122` | n/a | Delete one |
| test_text_and_media_binding_are_independent :66 | TIGHTEN | Yes | `json.dumps` substring asserts | Typed views |

### cards/prompt/test_prompt_output_schema.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_output_dict_of_types_builds_response_format :9 | TIGHTEN | Yes | Deep dict | `prompt.response_schema` (gap) |
| :28, :45, :57 (3) | KEEP | Yes | Clear | None |
| test_output_invalid_class_raises :70 | REWRITE | Accepts any of 4 codes, including legacy `SKALD_` | Hides the contract | Pin one code |

### cards/prompt/test_prompt_output_schema_rust.py
Duplicates the file above; imports `wyrd._wyrd.WyrdError` (:8).
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :11, :39, :57 (3) | DELETE | Duplicates of `test_prompt_output_schema.py` | n/a | Delete |
| test_pydantic_model_class_name_is_schema_name :30 | KEEP | Yes | Clear | Move into `test_prompt_output_schema.py` |
| test_additional_properties_not_overwritten :69 | KEEP | Yes | Clear | Same |
| test_invalid_output_type_raises :80 | TIGHTEN | Yes | Private import; bare error | Public import; pin the code |
| test_rust_schema_extraction_does_not_use_output_to_json_schema :90 | DELETE | Name refers to an internal function; behaviour duplicated | n/a | Delete |

### cards/prompt/test_prompt_render.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :33, :39, :45, :52 (4) | KEEP | Yes | Clear | None |
| test_bind_bind_mut_and_render_compose_without_mutating_original :61 | TIGHTEN | Yes | Two stories (text and media) | Split |

### cards/prompt/test_prompt_structured_output.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_pydantic_basemodel_schema_extraction :39 | REWRITE | Yes | `schema_from_request` branches per provider | Typed `prompt.response_schema` (gap) |
| test_response_format_json_schema_rejects_non_object_schema :49 | KEEP | Yes | Clear | None |
| test_structured_output_model_dump_json_preserves_schema :56 | TIGHTEN | Yes | Dict | Typed accessor |

### cards/prompt/test_prompt_template_syntax.py
All 4 (:6, :11, :16, :21) KEEP. They read well and pin user-visible template rules.

### cards/prompt/test_promptcard_lifecycle.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_dropping_promptcard_does_not_invalidate_held_prompt :6 | KEEP | Yes (Python lifetime) | Clear | None |
| test_round_tripping_card_preserves_live_prompt_handle :17 | KEEP | Yes | Clear | None |

### cards/prompt/test_promptcard_roundtrip.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :20, :45, :66, :78 (4) | KEEP | Yes | Clear | None |
| test_save_then_from_path_yaml_roundtrip_and_no_type_field :32 | TIGHTEN | Yes | Legacy substring ban tacked on | Drop the `type: Prompt` assert |
| test_json_and_yaml_roundtrip_equivalence :53 | DELETE | Implied by :20 and :32 | n/a | Delete |

### cards/prompt/test_promptref.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :7, :18 (2) | TIGHTEN | Yes | `kind == "card"` but `model_dump()["kind"] == "Prompt"` is confusing; dict asserts | Typed `ref.card_ref.name` |
| test_promptref_kind_tag_only_card_or_inline :26 | TIGHTEN | Yes | Forges the payload by JSON edit; the first assert is a tautology | Invalid fixture file; pin the code |

### cards/test_card_as_card_ref.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :11, :51 (2) | KEEP | Yes | Clear | None |
| test_model_card_as_card_ref :26 | TIGHTEN | Yes | Trains a model it does not need | `SklearnInterface()` with no model; parametrize all three |

### cards/test_card_ref.py
Imports `WyrdError` from `wyrd._wyrd` (:4).
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :8, :18, :31, :51, :59, :87, :121, :126, :133 (9) | KEEP | Yes | Clear | Switch to the public import |
| test_repr_contains_identity_fields :40 | TIGHTEN | Marginal | Repr formatting trivia | One `repr(ref) == "..."` assert, or drop |
| test_every_native_kind_enum_is_accepted :114 | DELETE | Duplicate of :87 | n/a | Delete |

### cards/test_card_surface_contracts.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_card_surfaces_do_not_expose_server_registration :41 | DELETE | hasattr name-ban | n/a | Delete; `Cards.register` is the positive contract |

### cards/test_prompt_model_settings.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :24, :37, :47, :55, :66, :77, :84, :101, :113, :155, :166, :176 (12) | KEEP | Yes | Clear | None |
| test_yaml_prompt_load_with_model_settings :124 | DELETE | Duplicate of `test_prompt_load.py:138` | n/a | Delete |
| test_old_sampling_kwargs_are_absent :140 | DELETE | Legacy signature ban | n/a | Delete |
| test_settings_classes_import_from_root_and_prompt_module :149 | TIGHTEN | Marginal | OK | Shared exports test |
| test_yaml_prompt_card_load_with_model_settings :185 | TIGHTEN | Yes | Inline Card YAML | Fixture file |

### cards/test_registry_surface.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_agent_cards_are_not_registerable :16 | KEEP | Yes | Clear | None |
| test_registry_stub_documents_patch_default_and_exact_delete :28 | DELETE | Greps stub text | n/a | Delete (codegen owns stubs) |
| test_exact_pin_and_explicit_bump_are_rejected_before_network :37 | KEEP | Yes | Clear | None |
| test_data_get / test_model_get_rejects_legacy_load_args_before_network :49, :58 | DELETE | Legacy kwarg ban; Python's own TypeError | n/a | Delete |
| test_cards_without_a_credential_raise_the_client_catalog_error :67 | TIGHTEN | Yes | Copied env scrub | Shared fixture |
| test_cards_with_an_empty_server_url_raise_config_invalid_details :82 | KEEP | Yes | Clear | None |

### cards/test_validate_registrable.py
test_validate_registrable_rejects_runtime_local_tools :5: KEEP. It is a clean model test.

### client/test_client.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_client_without_a_resolvable_credential_raises :8 | TIGHTEN | Yes | Copied env scrub | Shared fixture |
| test_on_behalf_of_rejects_an_unknown_audience :20 | KEEP | Yes | Clear | None |
| test_on_behalf_of_runs_the_exchange_in_rust :29 | REWRITE | Asserts "code is not X"; the name describes the implementation | Vague | Pin the transport-unreachable code; rename |
| :39, :57 (2) | KEEP | Yes | Clear | None |

### cli/test_cli_surface.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_packaged_cli_preserves_help_and_usage_codes :15 | TIGHTEN | Yes (packaging entrypoint) | Magic `64` | Named `EX_USAGE` constant |
| test_packaged_cli_preserves_runtime_codes :21 | REWRITE | Yes | 40 lines of inline Verifier JSON; two stories; magic exit codes 1 and 2 | Checked-in Verifier and records fixtures; one story per exit code |

### config/test_load.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_load_round_trip :11 | REWRITE | Asserts on a `repr` substring (gap) | Opaque | Typed `cfg.defaults.space` |
| :19, :26, :36, :45, :54, :64, :73, :82, :91, :100, :110 (11) | KEEP | Yes | Clear, though setup repeats | Optional `wyrd_toml` fixture. :19 maps a missing file to `INVALID_TOML`; confirm that code is intended |

### config/test_public_surface.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_wyrd_config_all_exports_are_importable :7 | TIGHTEN | Marginal | OK | Shared exports test |
| test_wyrd_config_is_re_exported_at_package_root :12 | DELETE | Duplicate | n/a | Delete |

### contracts/test_negative_pins.py
test_agent_card_is_public_root_export :4: DELETE. It is a hasattr duplicate, and the file name misleads.

### gateway/test_gateway_surface.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_gateway_is_exported_with_every_administration_method :29 | DELETE | Private import; method-name list that `ty` already checks | n/a | Delete |
| test_gateway_offers_no_provider_credential_mutation :38 | KEEP | Live security boundary | Clear | None |
| test_gateway_invalid_body_raises_validation_without_echoing_it :51 | KEEP | Yes (secret redaction) | Clear | None |

### gateway/test_gateway_typing.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :20, :77 (2) | MOVE | Static-type fixtures; the runtime asserts are trivial | n/a | Move to an uncollected `typecheck/` dir checked by `py:typecheck` |

### runtime/agent/test_agent_construct.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_agent_constructs_from_prompt :5 | KEEP | Yes | Clear | None |
| test_agent_rejects_banned_provider_kwargs :16 | DELETE | Legacy kwarg ban (Python TypeError) | n/a | Delete |

### runtime/agent/test_agent_journey.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_journey_author_save_load_run :13 | REWRITE | Builds a second mock agent to run; the loaded agent never runs | Misleading | Load from a fixture and run the loaded agent on a documented offline provider |
| test_journey_to_card_round_trip :40 | TIGHTEN | Yes | Dict asserts (gap) | Typed `AgentCard` |
| :66, :84, :102, :114 (4) | KEEP | Yes | Clear | None |
| test_journey_module_paths :154 | DELETE | Module trivia; duplicate | n/a | Delete |

### runtime/agent/test_agent_no_wrapper_class.py
All 3 (:6, :15, :27) DELETE. They are implementation trivia, a legacy name-ban, and a duplicate of journey :40.

### runtime/agent/test_agent_parsed_output.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :26, :48, :69, :90, :132 (5) | REWRITE | Right story, but the model output is faked by returning OpenAI JSON from `after_model_callback` (gap) | Callback hack hides intent | Canned mock response |
| test_parsed_is_none_for_text_prompt :62 | KEEP | Yes | Clear | None |
| test_parsed_decode_failure_raises :113 | REWRITE | Accepts a Wyrd or a Pydantic error | Contract unclear | Pin one error and code |

### runtime/agent/test_agent_save_load.py
test_agent_save_load_round_trips_yaml :6: TIGHTEN. Drop the obfuscated `"version" + "_req"` ban at :20-21.

### runtime/agent/test_agent_structured_output.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_structured_output_parses_into_dict :23 | REWRITE | Callback hack | Same as above | Canned mock response |
| test_structured_output_is_none_for_text_prompts :42 | DELETE | Duplicate of `test_agent_parsed_output.py:62` | n/a | Delete |
| test_structured_output_decode_failure_raises :49 | REWRITE | Callback hack; this is the pinned-code owner | Same | Canned mock response |

### runtime/test_callbacks.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_before_model_callback_fires_with_ambient_mock :4 | TIGHTEN | Yes | `ctx["agent_id"]` dict; "ambient mock" jargon | Typed ctx (gap); rename |
| test_after_model_replace_with_changes_output :21 | TIGHTEN | Yes, replacement is the feature | 17-line raw OpenAI dict | Typed `ModelResponse.text("synthetic")` |
| test_before_model_raise_aborts_run :50 | KEEP | Yes | Clear | None |

### runtime/test_observer.py
All 3 (:10, :22, :72) DELETE. They call methods on their own Observer subclass and test Python threading. Note that the `on_model_call` signature here (:29) differs from `runtime/workflow/test_workflow_observers.py:24`, which takes `request`. The Observer contract needs a single documented signature.

### runtime/test_session.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_session_turn_is_python_constructible_and_serializable :19 | KEEP | Yes | Clear | None |
| test_python_session_receives_appends :33 | TIGHTEN | Yes | Magic `50` (default history limit) | Name the default |
| test_python_session_recent_can_return_dicts :47 | TIGHTEN | Yes | `ctx["conversation"]["turns"]`, keyed `type` | Typed ctx (gap) |
| test_invalid_session_object_is_rejected :72 | KEEP | Yes | Clear | None |
| test_python_journal_surface_is_not_public :81 | DELETE | Legacy ban | n/a | Delete |

### runtime/workflow/test_workflow_builder.py
All 3 (:12, :21, :30) KEEP. They are clear and could be parametrized.

### runtime/workflow/test_workflow_construct.py
All 4 (:13, :21, :27, :38) KEEP.

### runtime/workflow/test_workflow_parameter_injection.py
All 5 (:7, :19, :30, :51, :72) KEEP. They are good stories. They depend on the undocumented `mock` provider echoing the message (gap).

### runtime/workflow/test_workflow_observers.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :53, :61, :79, :115 (4) | KEEP | Yes | Clear | None |
| test_workflow_observer_receives_workflow_events :70 | DELETE | Covered by :53 plus :79 | n/a | Merge the `workflow_finish` assert into :53 |
| test_workflow_observer_typed_provider_request_and_response :90 | TIGHTEN | Yes | `total_tokens == 0` is mock trivia | Assert the typed view type and the model name |
| test_workflow_loaded_from_yaml_has_empty_observers :101 | TIGHTEN | Yes | `assert loaded.to_yaml()` and `observer.events == []` assert nothing | Assert the YAML has no observers and the loaded workflow runs without the observer firing |

### runtime/workflow/test_workflow_save_load.py
test_workflow_save_load_round_trips_yaml :14: TIGHTEN. Replace the YAML substring asserts with a typed comparison of the loaded workflow (name, version, steps).

### state/test_hydration.py
Behaviour and error details are good. The setup comes from the code-built bundle in `state/support.py` (see cross-cutting 1).
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :37, :47, :55, :61, :67, :152, :162, :171, :186, :201, :216, :224, :246, :256, :268, :282, :293, :307, :323, :333, :350, :373, :406 (23) | KEEP | Yes, with stable codes and details | Clear one-story tests | Repoint to a checked-in bundle fixture |
| test_builtin_model_loads_from_bundle_artifact_directory :73 | REWRITE | Yes (security), but uses a reimplemented trust hash (gap) | Three stories | Split into missing hash, wrong hash and trusted load; use the SDK hash API |
| test_builtin_override_is_rejected_before_artifact_load :121 | TIGHTEN | Yes | Uses `trusted_artifact_hash` | Same hash API |
| test_malformed_bundle_retains_stable_error :418 | TIGHTEN | Yes | Overwrites the manifest in code | Checked-in malformed bundle |

### state/test_public_surface.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_wyrdstate_exports_only_from_state_module :13 | DELETE | Duplicate | n/a | Delete |
| test_statecard_and_runtime_module_are_removed :20 | DELETE | Legacy ban | n/a | Delete |
| test_public_state_surfaces_have_runtime_docstrings :27 | MOVE | Docstring-length lint | n/a | Stub/docs lane |
| test_generated_state_stubs_retain_docstrings :69 | MOVE | Greps stub text | n/a | `codegen:check` |
| test_card_envelope_exposes_complete_registered_card :116 | TIGHTEN | Yes | `isinstance(..., dict)` checks | Assert values (name, kind, the metadata space) |
| test_wrong_kind_accessor_raises_stable_wyrd_error :139 | KEEP | Yes | Clear | None |

### state/test_wyrdstate_surface.py
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| test_public_wyrdstate_and_cards_exports :12 | DELETE | Duplicate | n/a | Delete |
| test_metadata_only_bundle_is_rejected_with_stable_error :21 | TIGHTEN | Yes | Inline manifest; try/except/else instead of `pytest.raises` | Fixture dir; `pytest.raises` |

### state/test_observe_surface.py
Covers run identity, observation refusals and OTel run-scope correlation. The OTel scope tests are among the best in the tier.
| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| :61, :70, :80, :86 (4) | KEEP | Yes | Clear | None |
| Positive "accepts" tests :93, :100, :136, :195, :265 (5) | REWRITE | Acceptance is proved only by catching `BIFROST_NOT_STARTED` (gap) | A test named "accepts" that expects an error | Move positive admission to the journey, or assert through a pure validator |
| test_eval_checks_keys_after_reduction :239 | REWRITE | Two stories, one of them a proxy assert | Same | Split; keep the refusal half |
| Refusals :115, :122, :174, :188, :203, :210, :217, :224, :231, :257 (10) | KEEP | Yes, refusal before admission | Clear; :174 is a good parametrized table | None |
| test_run_card_selects_the_initial_view_and_shares_its_invocation :276 | TIGHTEN | Yes | Several stories; `card=` keyword is replaced by positional in REQ-188 | Split; `state.run("model")` |
| test_run_card_refuses_an_unknown_alias :288 | KEEP | Yes | Clear | Update to positional per REQ-188 |
| OTel scope :330, :352, :374, :409, :447 (5) | KEEP | Yes, real OTel SDK and observable span attributes | Clear | None |
| test_repeated_entry_registers_once_on_a_marked_provider :472 | REWRITE | Counts processors through OTel private `_active_span_processor` and the class name `_RunCorrelationProcessor` (:326) | Internals | Assert that the span carries the pair once and that `install_run_correlation` is idempotent through its return value |
| test_unsupported_providers_are_refused_without_raising :510 | KEEP | Yes | Clear | None |
| test_run_exit_accepts_conventional_keywords_and_omitted_arguments :519 | TIGHTEN | Partly; `inspect.signature` names are stub trivia | OK | Keep the "never suppresses" part; drop the signature check |
| test_missing_opentelemetry_is_a_no_op :535 | REWRITE | Subprocess plus private `_SCOPE_KEY`, `_otel_context`, `_otel_trace` monkeypatches | Hard to follow | Keep only the subprocess check, through the public `install_run_correlation()` and a run enter and exit |
| test_registration_and_attach_failures_never_block_observations :565 | KEEP | Yes (fault containment) | Clear | None |
| test_enrichment_failure_never_blocks_observations :582 | TIGHTEN | Yes | Swaps a monkeypatch mid-scope; two stories | Split |
| test_exit_context_update_failure_never_blocks_observations :602 | TIGHTEN | Yes | Manual `__exit__` cleanup at the end | Fixture-managed cleanup |
| test_mismatched_exit_changes_nothing :619 | KEEP | Yes | Clear | None |
| test_nested_entry_never_overwrites_an_active_span_correlation :637 | KEEP | Yes | Clear | None |
| (missing) observe.verify local failures | n/a | REQ-188 has no unit coverage | n/a | Add `UNKNOWN_VERIFIER` and wrong-shape `INVALID_OBSERVATION` refusal tests here |

### test_operator_connections_surface.py
test_malformed_arguments_are_refused_without_echoing_secrets :13: TIGHTEN. Parametrize the three cases. The behaviour, validation before transport with no secret echoed, is right.

### test_operator_connections_typing.py
:21 and :87: MOVE to an uncollected `ty` fixture directory. Their runtime asserts (`len(...) == 7`) prove nothing.

### test_verification_surface.py
test_malformed_arguments_are_refused_before_any_request :12: DELETE. It targets the `Verification` handle and `wyrd.verification`, which REQ-189 removes.

## Good examples to keep
- `cards/prompt/test_prompt_template_syntax.py:6-23`: one rule per test, public constructor, one assert.
- `cards/prompt/test_promptcard_roundtrip.py:20-28`, `:66-75`: save, load and compare with a typed content hash.
- `cards/prompt/test_prompt_media_binding.py:79-83`: pins template-injection safety in four lines.
- `cards/test_validate_registrable.py:5-20` and `cards/test_registry_surface.py:37-46`: a local refusal with a stable code, before any network call.
- `cards/model/test_modelcard_validation.py:19-33`, `:83-92`: stable codes, plus no leak of the absolute path.
- `runtime/workflow/test_workflow_parameter_injection.py:30-48`: structured output feeds the next step, a clear user story.
- `runtime/agent/test_agent_journey.py:114-151`: callback ordering, readable top to bottom.
- `state/test_hydration.py:246-347`: alias and kind errors with precise `details`, one story each.
- `state/test_observe_surface.py:143-185`: a parametrized refusal table with ids.
- `state/test_observe_surface.py:330-463`: run-scope OTel correlation through a real SDK provider, asserting observable span attributes.
- `gateway/test_gateway_surface.py:51-79` and `test_operator_connections_surface.py:13-27`: validation before transport with no secret echoed.

## Proposed target structure
```
sdks/wyrd-sdk-python/tests/
  fixtures/                       # shared with Rust and TS where the Card is the same (REQ-192)
    cards/{data,model,prompt,agent,verifier}/*.yaml      # valid Cards
    cards/invalid/*.yaml                                 # one file per validation rule
    bundles/{complete,builtin-model,metadata-only,malformed}/   # checked-in WyrdState bundles
    config/*.toml
  unit/
    conftest.py                   # fixtures(): path helper, no_credentials, model factories, bundle copy-to-tmp
    test_public_exports.py        # ONE exports test (replaces 9 scattered surface tests)
    cards/test_card_ref.py
    cards/data/test_author_datacard.py      # inference, options, splits (typed)
    cards/data/test_save_and_reload.py      # one parametrized table via DataCard.from_path
    cards/data/test_invalid_datacards.py    # parametrized over fixtures/cards/invalid
    cards/data/test_custom_interface.py
    cards/model/test_author_modelcard.py, test_save_and_reload.py, test_invalid_modelcards.py
    cards/prompt/test_author_prompt.py, test_bind_and_render.py, test_media.py,
                 test_structured_output.py, test_model_settings.py, test_promptcard_files.py
    runtime/test_agent_run.py, test_callbacks.py, test_sessions.py   # documented mock provider
    runtime/test_workflow.py, test_workflow_observers.py
    state/test_hydrate_bundle.py, test_hydration_errors.py
    state/test_observe_refusals.py          # drift/eval/record/verify local refusals
    state/test_run_scope_otel.py
    clients/test_client_construction.py     # WyrdClient, Cards, Bifrost, Gateway, OperatorConnections
    cli/test_packaged_cli.py
    config/test_wyrd_config.py
  typecheck/                     # ty-only fixtures, not collected by pytest
```
Lint-style checks for stub docstrings, stub text and package layout move to `codegen:check` or a stub lint lane.
