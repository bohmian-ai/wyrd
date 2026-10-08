# TASK-017-R2 SDK parity table (AC-062, REQ-210)

**Verified against source** in the final R2 pass, after W1, W4, and W5
landed (spec revisions 68, 69, and 70). Parity is measured
on the SDK packages (REQ-210):

- Rust: `wyrd_sdk` (`sdks/wyrd-sdk-rust/src/lib.rs`) re-exports, with
  `wyrd-client` built without its `internal` feature, plus
  `wyrd_sdk::cli` (`wyrd_cli::commands`, feature `testing`).
- Python: `sdks/wyrd-sdk-python/python/wyrd/stubs/*.pyi`.
- TypeScript: `sdks/wyrd-sdk-ts/wyrd/src/index.ts` (declarations in
  `wyrd/index.d.ts`) and `testing/index.d.ts`.

Conventions:

- `client` is the one optional identity argument (REQ-208): Python
  `client: WyrdClient | None = None`, TypeScript `ClientOptions`
  (`{ client?: WyrdClient }`), Rust `with_client(client)` / `&WyrdClient`
  beside `from_env()`.
- `ref` is a selector (REQ-211): Rust `CardSelector` (`CardSelector::exact`
  wraps a `CardRef`), Python `CardRef`, TypeScript `CardRef | string`.
- Paths are Rust `impl AsRef<Path>`, Python `str | os.PathLike[str]`, and
  TypeScript `string`.
- Rust `async` methods, Python `AsyncBifrost` methods, and TypeScript server
  calls are awaited. Errors are `WyrdError` with one catalog code in every SDK;
  Rust Bifrost methods return `BifrostClientError`, which carries the code.
- **Exception** names a REQ-210 idiom exception, or `—` when the row matches.
  `GAP` marks a surface that neither matches nor names an exception; see
  [Remaining gaps](#remaining-gaps).

## WyrdClient

| Operation | Rust | Python | TypeScript | Result | Exception |
|---|---|---|---|---|---|
| construct | `WyrdClient::with_config(ClientConfig)`; `WyrdClient::from_global()` | `WyrdClient(server_url=None, credential=None, grpc_url=None)` | `WyrdClient.connect({ serverUrl?, credential?, grpcUrl? })` | `WyrdClient` | — |
| server URL | `server_url()` | `server_url` | `serverUrl` | `str` | — |
| gRPC URL | `grpc_url()` | `grpc_url` | `grpcUrl` | `str` | — |
| access token | `access_token()` | `access_token()` | `accessToken()` | token (Rust `SecretBearer`) | — |
| act for a subject | `on_behalf_of(subject_token, audience)` | `on_behalf_of(subject_token, audience="bifrost")` | `onBehalfOf(subjectToken, { audience })` | `WyrdClient` | — |

## Cards

| Operation | Rust | Python | TypeScript | Result | Exception |
|---|---|---|---|---|---|
| construct | `Cards::from_env()`; `Cards::with_client(client)` | `Cards(client=None)` | `Cards.connect({ client })` | `Cards` | — |
| register a Card file or bundle directory | `register_from_path(path)` | `register_from_path(path)` | `registerFromPath(path)` | `RegistrationReceipt` | — |
| get | `get(ref)` | `get(card_ref)` | `get(ref)` | typed Card | — |
| list | `list(ListCardsRequest)` | `list(kind=None, space, name, version_range, status, filter, include_prerelease, limit, cursor)` | `list(request = {})` | `ListCardsResponse` / `CardList` | — |
| delete | `delete(ref)` | `delete(card_ref)` | `delete(ref)` | none | — |
| resolve latest | `resolve_latest(kind, space, name)` | `resolve_latest(kind, space, name)` | `resolveLatest(kind, space, name)` | `CardRef` | — |
| hydrate a bundle | `hydrate(ref, destination, HydrationMode)` | `hydrate(card_ref, destination, metadata_only=False)` | `hydrate(ref, destination, { metadataOnly })` | `HydrationSummary` | — (REQ-210 names Rust's `mode`) |
| load a registered Workflow | `workflow().load(&ref)` | `workflow.load(card_ref)` / `(space=, name=, version=)` / `(uid=)` | `workflow.load(selector)` | `Workflow` | — |
| programmatic register | `register(&RegistrationInput)`; `register_with_progress(input, sink)` | `register(card, version_bump=..., save_args=None)`; `WyrdConfig` defaults; `PromptCard` authoring | — | `RegistrationReceipt` | programmatic `register` in Rust and Python |
| per-kind registries and holders | — | `data`, `model`, `prompt` registries; `DataCard`, `ModelCard` holders | — | per kind | Python's Data and Model holders and per-kind registries |

## Bifrost

| Operation | Rust | Python | TypeScript | Result | Exception |
|---|---|---|---|---|---|
| connect | `Bifrost::from_env()`; `connect(&client)`; `connect_with_table(&client, table)`; `connect_with_config(&client, table, QueueConfig)` | `AsyncBifrost(table=None, client=None, client_byte_limit_bytes=None)` | `Bifrost.connect({ client, table, clientByteLimitBytes })` | `Bifrost` | — |
| synchronous twin | `bifrost::blocking` methods on `Bifrost` | `Bifrost(...)` beside `AsyncBifrost` | — | as async | synchronous twin |
| table config from a row type | `TableConfig::from_model::<T>(fqn)` | `TableConfig(model, table, ...)` | `TableConfig.fromJsonSchema(table, schema, options)` | `TableConfig` | row-model typing |
| table config from Arrow | `TableConfig::from_arrow(fqn, schema)` | `TableConfig.from_arrow(table, schema, ...)` | `TableConfig.fromArrow(table, schema, options)` | `TableConfig` | — |
| table config from JSON Schema | `TableConfig::from_json_schema(fqn, &schema)` | `TableConfig.from_json_schema(table, schema, ...)` | `TableConfig.fromJsonSchema(table, schema, options)` | `TableConfig` | — |
| describe a table config | `TableConfig::describe(&client, fqn)`; `describe_from_env(fqn)` | `TableConfig.describe(table, client=None)` | `TableConfig.describe(table, { client })` | `TableConfig` | — |
| table config fields | `fqn()`, `user_schema()`, `compaction_type()`, `compaction_target_file_size_bytes()`, `resolved()` | `fqn`, `arrow_schema`, `compaction_type`, `compaction_target_file_size_bytes`, `resolved` | `fqn`, `arrowSchema`, `compactionType`, `compactionTargetFileSizeBytes`, `resolved` | values | — |
| register table | `register()` | `register()` | `register()` | `"created"` / `"already_exists"` | — |
| use table | `use_table(table)`; `use_table_by_name(fqn)` | `use_table(table)`; `use_table_by_name(table)` | `useTable(table)`; `useTableByName(table)` | previous `TableConfig` / none | — |
| active table | `table()` | `table` | `table` | `TableConfig` or none | — |
| insert | `insert(row: Vec<u8>, correlation)` | `insert(row, correlation=None)` | `insert(row, correlation = {})` | none | row serialization on `insert` |
| write batch | `write_batch(table, &batch)` | `write_batch(table, batch)` | `writeBatch(table, batch)` | none | — |
| flush / shutdown | `flush()`; `shutdown()` | `flush()`; `shutdown()` | `flush()`; `shutdown()` | none | — |
| SQL | `sql(query, &params)` | `sql(query, params=None)` | `sql(query, params)` | `QueryResult` | — |
| typed SQL | `sql_as::<T>(query, &params)` | `sql(query, params, model=T)` | `sql(query, params, rows)` | rows of `T` | row-model typing |
| stream | `stream(query, &params, deadline: Option<Duration>)` | `stream(query, params=None, deadline_ms=None)` | `stream(query, params, { deadlineMs })` | batch stream with `terminal` | — |
| query result | `batches()`, `schema()`, `terminal()`, `num_rows()`, `to_bytes()` | `batches()`, `schema()`, `terminal()`, `num_rows()`, `to_bytes()` | `batches`, `schema`, `terminal`, `numRows`, `toBytes()` | values | — |
| query result as one Arrow table | — (`batches()` returns `&[RecordBatch]`) | `to_arrow()` → `pyarrow.Table` | `toArrow()` → `Table` | Arrow table | ecosystem adapters, REQ-210 rev 70 (a) |
| query result as a dataframe | — | `to_pandas()`, `to_polars()` | — | DataFrame | ecosystem adapters, REQ-210 rev 69 (a) |
| running / status / cancel | `running()`; `status(request_id)`; `cancel(request_id)` | same | `running()`; `status(requestId)`; `cancel(requestId)` | `RunningQuery` list / `RunningQuery` / `CancelRunningQueryResult` | — |
| describe a table | `describe_table(namespace, name)` | `describe_table(namespace, name)` | `describeTable(namespace, name)` | `TableDescription` | — |
| dropped rows | `dropped()` | `dropped` | `dropped` | `int` | — |
| producer count | `producer_count()` | `producer_count` | `producerCount` | `int` | — |
| telemetry record | `bifrost::observe::record(&bifrost, table, &schema, json, correlation)` | `wyrd.observe.record(bifrost, table, schema, row, correlation=None)` | `record(bifrost, table, schema, row, correlation = {})` | none | row serialization on `insert` (Rust takes serialized JSON bytes) |

## WyrdState and Run

| Operation | Rust | Python | TypeScript | Result | Exception |
|---|---|---|---|---|---|
| load a bundle | `WyrdState::from_path(path)`; `from_path_with_client(path, client)` | `WyrdState.from_path(path, client=None, interfaces=None, load_kwargs=None, trusted_artifact_hashes=None)` | `WyrdState.fromPath(path, { client })` | `WyrdState` | — (`interfaces`, `load_kwargs`: Python's Data and Model holders) |
| start Bifrost | `start_bifrost()`; `start_bifrost_with(table)`; `start_bifrost_with_config(table, QueueConfig)` | `start_bifrost(table=None, client_byte_limit_bytes=None)` | `startBifrost({ table, clientByteLimitBytes })` | none | — |
| open a run | `run()`; `run_for_card(alias)` | `run(alias=None)` | `run(alias?)` | `Run` | Rust `run()` / `run_for_card(alias)` |
| flush / shutdown | `flush()`; `shutdown()` | `flush()`; `shutdown()` | `flush()`; `shutdown()` | none | — |
| root ref | `root_ref()` | `root_ref` | `rootRef` | `CardRef` | — |
| root Service | `service()` | `service` | `service` | typed root Card | — |
| aliases | `aliases()` | `aliases` | `aliases` | `str` list | — |
| Card / CardRef by alias | `card(alias)`; `card_ref(alias)` | `card(alias)`; `card_ref(alias)` | `card(alias)`; `cardRef(alias)` | Card / `CardRef` | — |
| artifacts | `artifacts(alias)` | `artifacts(alias)` | `artifacts(alias)` | `HydratedArtifact` list | — |
| typed accessors | `agent`, `prompt`, `verifier`, `workflow`, `model`, `data` | same | same | per kind | — |
| run id / alias | `Run::run_id()`; `alias()` | `run_id`; `alias` | `runId`; `alias` | `str` | — |
| run for another Card | `for_card(alias)` | `for_card(alias)` | `forCard(alias)` | `Run` | — |
| observe | `observe()` | `observe` | `observe` | `Observe` | — |
| drift | `drift(&features, session_id)` | `drift(features, session_id=None)` | `drift(features, { sessionId })` | none | — |
| eval | `eval(&context, options)` | `eval(context, session_id=None, media=None, trace_id=None, span_id=None)` | `eval(context, { sessionId, media, traceId, spanId })` | none | — |
| record | `record(table, &row)` | `record(table, row)` (synchronous) | `record(table, row)` | none | Python's synchronous `record` |
| verify | `verify(verifier, &input)`; `verify_with_media(verifier, &input, media)` | `verify(verifier, input, media=None)` | `verify(verifier, input, { media })` | `Judgment` | Rust `verify` / `verify_with_media` |

## Workflow

| Operation | Rust | Python | TypeScript | Result | Exception |
|---|---|---|---|---|---|
| load a file | `Workflow::from_path(path)`; `from_path_with_client(path, client)` | `Workflow.from_path(path, client=None)` | `Workflow.fromPath(path, { client })` | `Workflow` | — |
| parse YAML | `Workflow::from_yaml(yaml)` | `Workflow.from_yaml(yaml)` | `Workflow.fromYaml(yaml)` | `Workflow` | — |
| run | `run(input: impl Into<WorkflowInput>)` | `run(input=None)` (str or mapping) | `run(input = {})` (string or record) | `WorkflowRun` | — |
| step ids | `steps()` | `steps` | `steps` | `str` list | — |
| identity | `name()`, `version()`, `space()` | `name`, `version`, `space` | `name`, `version`, `space` | `str` or none | — |
| agent runtime | — | `Agent`, `Prompt`, `tool`, `local_registry`, `SessionMemory`, `RunConfig`, provider settings | — | `AgentRun` | Python's agent runtime |
| OTel run correlation | — | `wyrd.otel.install_run_correlation(provider=None)` | — (reads the active span when `@opentelemetry/api` is present) | `bool` | ecosystem adapters, REQ-210 rev 69 (a) |

## Gateway

| Operation | Rust | Python | TypeScript | Result | Exception |
|---|---|---|---|---|---|
| construct | `Gateway::from_env()`; `with_client(client)` | `Gateway(client=None)` | `Gateway.connect({ client })` | `Gateway` | — |
| credential(s) | `credential(name)`; `credentials()` | same | same | `ProviderCredentialView` / list | — |
| deployments | `put_deployment`, `deployment(name)`, `deployments()`, `delete_deployment(name)` | same | `putDeployment`, `deployment`, `deployments`, `deleteDeployment` | `ProviderDeployment` / list / none | — |
| fallback policy | `put_fallback_policy`, `fallback_policy`, `delete_fallback_policy` | same | `putFallbackPolicy`, `fallbackPolicy`, `deleteFallbackPolicy` | `GatewayFallbackPolicy` / none | — |
| governance policy | `put_governance_policy`, `governance_policy`, `delete_governance_policy` | same | `putGovernancePolicy`, `governancePolicy`, `deleteGovernancePolicy` | `GatewayGovernancePolicy` / none | — |
| capture policy | `put_capture_policy`, `capture_policy` | same | `putCapturePolicy`, `capturePolicy` | `GatewayCapturePolicy` | — |

## OperatorConnections

| Operation | Rust | Python | TypeScript | Result | Exception |
|---|---|---|---|---|---|
| construct | `OperatorConnections::from_env()`; `with_client(client)` | `OperatorConnections(client=None)` | `OperatorConnections.connect({ client })` | handle | — |
| create / list / get / update / disable | `create(&req)`, `list()`, `get(id)`, `update(id, &req)`, `disable(id)` | `create(request)`, `list()`, `get(connection_id)`, `update(connection_id, request)`, `disable(connection_id)` | `create`, `list`, `get(connectionId)`, `update(connectionId, request)`, `disable(connectionId)` | `OperatorConnectionView` / list | — |

## Client configuration

| Operation | Rust | Python | TypeScript | Result | Exception |
|---|---|---|---|---|---|
| client constructor options | `ClientConfig` (and `ClientConfig::from_*`), `Environment`, `GlobalConfig::{load, load_in, load_from}` | `WyrdClient(...)` keyword arguments | `WyrdClient.connect({...})` options | config values | Rust constructor options, REQ-210 rev 69 (b) |

## Test-only CLI functions (REQ-196)

Rust `wyrd_sdk::cli` (feature `testing`), Python `wyrd.testing.cli`,
TypeScript `@wyrd/sdk/testing` `cli`.

| Operation | Rust | Python | TypeScript | Result | Exception |
|---|---|---|---|---|---|
| plan | `plan(path)` | `plan(path)` | `plan(path)` | `PlanReport` | — |
| apply | `apply(path, client)` | `apply(path, client=None)` | `apply(path, { client })` | `RegistrationReceipt` | — |
| get | `get(&selector, output_dir, metadata_only, client)` | `get(output_dir, kind, space, name, version, uid, metadata_only=False, client=None)` | `get(selector, outputDir, { client, metadataOnly })` | `HydrationSummary` | — |
| load | `load(&selector, path: Option<&Path>, client)` | `load(kind, space, name, version, uid, path=None, client=None)` | `load(selector, { client, path })` | `LoadOutput` | fixture-only CLI helpers, REQ-210 rev 69 (c) |
| issue key | `issue_key(kind, name, version, space, label, expires_in_seconds, client)` | `issue_key(kind, name, version, space, label=None, expires_in_seconds=None, client=None)` | `issueKey({ kind, name, version, space, label, expiresInSeconds }, { client })` | `IssueKeyResponse` | — |
| grant role | `grant_role(kind, name, version, space, role, client)` | `grant_role(kind, name, version, space, role, client=None)` | `grantRole({ kind, name, version, space, role }, { client })` | `GrantRoleResponse` | — |
| provider credential | `put_provider_credential`, `revoke_provider_credential`, `delete_provider_credential` | same | `putProviderCredential`, `revokeProviderCredential`, `deleteProviderCredential` | `ProviderCredentialView` / none | — |

## Operator-only Rust handles

`Principals` and `Platform` (`Platform::with_client(&client)`) exist in Rust
only: the operator-only Rust handles exception. `wyrd_sdk` no longer
re-exports `storage`, `auth`, `workflow`, `Workflows`, or
`PublicWyrdGatewayCaller`; `transport` is narrowed to `GrpcConfig`,
`HttpConfig`, and `ResolvedCredential`.

## Parity boundary: `internal`-only items

Items compiled only under `wyrd-client`'s `internal` feature are binding
plumbing, not SDK surface: `wyrd-sdk-rust` never enables `internal`, so
`wyrd_sdk` users cannot reach them. They take no row. Nothing under
`sdks/wyrd-sdk-rust` reaches one, including the integration support, which
builds clients with `WyrdClient::with_config(ClientConfig { .. })`. As of this pass they
are: `bifrost::{client_from_options, register_outcome_name, ClientScope,
BifrostIngestSink, IngestTransport}`; `Bifrost::{with_sink, query_only,
writer_table, cached_writer_table, insert_into, insert_rows_into,
enqueue_batch, query, collect_bounded, scope, metrics, observe_losses}` and
their blocking twins plus `into_async`; `QueryResultStream::{collect_bounded,
settle, encoded_bytes}`; `Observe::{drift_json, eval_json, record_json,
verify_json}` and `Run::subject`; `WyrdClient::from_parts`, the raw
`request_*` / `submit_*` calls, `auth`, `http`, and `connect_grpc`; `Cards`
download, `registry_context`, `get_response`, and `load` plumbing; `WyrdState`
by-key and introspection accessors; `Workflow::{run_with, as_skald,
as_skald_mut, into_skald}`; and `SavedLoginStore::source`. The
`fieldspec_to_arrow` / `json_schema_to_arrow` re-exports are deleted.
`wyrd_client::saved_login` stays public in `wyrd-client` for the CLI, but
`wyrd_sdk` does not re-export it, so it is outside SDK parity.

## Remaining gaps

None. Every surface matches or names a REQ-210 exception (spec revisions
68, 69, and 70).
