# BIFROST-OTEL-T03 finding validation — Ponytail full

## Subject and method

- Immutable base: `8c107b344b3e4644e783be275de725be281a8dc2`
- Immutable candidate: `8999fcb9d99b4152c0c5662f261344b81b9a812d`
- Reviewed verdict: `BIFROST-OTEL-T03-8999fcb9d-verdict.md`
- `HEAD` resolved to the candidate. All source citations below were read from the candidate object with `git show`, so unrelated working-tree edits did not enter the review.
- Applied `ponytail:ponytail` at full intensity: reuse the current owner first, reject new dependencies and parallel paths, fix shared causes rather than adapter symptoms, and retain all explicit security, validation, completeness, and losslessness requirements.

## Validation summary

| Finding | Result | Severity | Disposition |
|---|---|---:|---|
| `FIND-BIFROST-OTEL-T03-1` | VALID | MAJOR | Retain; remove the trace row cap while preserving real byte/time ceilings. |
| `FIND-BIFROST-OTEL-T03-2` | VALID | MAJOR | Split into payload-projection and canonical-decoding findings because they have different root owners. |
| `FIND-BIFROST-OTEL-T03-3` | VALID | MAJOR | Retain; the proposed `Option<&CardRef>` fix must preserve existing queue callers by passing `Some`. |
| `FIND-BIFROST-OTEL-T03-4` | VALID | MAJOR | Retain; choose UTC `Z` formatting rather than adding URL machinery. |
| `FIND-BIFROST-OTEL-T03-5` | VALID | MODERATE | Retain; use one fallible parser in both `proto_timestamp` and `proto_window`. |
| `FIND-BIFROST-OTEL-T03-6` | VALID | MAJOR | Retain unchanged. |
| `FIND-BIFROST-OTEL-T03-7` | VALID | MODERATE | Split by Python and TypeScript owner; do not combine their closure evidence. |

No finding was falsified. The seven IDs represent nine independently closable defects after the two required splits.

## Finding validation

### FIND-BIFROST-OTEL-T03-1 — VALID, MAJOR

**Authority.** `changes/active/bifrost-canonical-otel-signals/spec.md:280-289,456-459,599-605` requires one complete trace cut and prohibits silent loss. Task lines 111-115 remove pagination from trace detail and require one complete authorized cut.

**Source evidence.** `crates/wyrd-spec/src/vala/api.rs:416-418` fixes the page size at 1,000. `crates/wyrd/wyrd-server/src/query/service.rs:370-466` retains only `limit + 1`, computes `has_more`, and returns batches truncated to `limit`. HTTP `vala_query/routes.rs:803-836` and gRPC `vala_query/grpc.rs:307-338` both pass 1,000 and discard `has_more`. The existing journey asserts only four rows at `tests/pg_router_smoke.rs:908-911`.

**Reachable path.** A stored authorized trace with 1,001 spans reaches `build_get_trace_plan` -> `run_typed_query(..., 1000)` -> `collect_bounded` -> successful truncated batches plus `true` -> either adapter ignores `true` -> successful incomplete response with no continuation mechanism.

**Counterevidence attempted.** Oracle terminal validation, the encoded/result-byte ceiling, and the timeout remain intact, but none recovers rows already truncated by the pagination collector. The extraction-side trace filter cannot recover those rows, and no later response field communicates truncation.

**Ponytail correction.** The verdict's proposed 1,000-row refusal is not acceptable because it preserves an arbitrary trace-size cap. Change the existing shared collector's row limit from `u32` to `Option<u32>`: the sixteen legitimate paginated HTTP/gRPC callers pass `Some(limit)` and retain current `limit + 1`, truncation, and `has_more` behavior; the two GetTrace adapters pass `None`, retain and return every decoded batch, and remain bounded only by the collector's existing encoded/result-byte ceiling and Oracle timeout. For `None`, skip row truncation and return `has_more = false`. This is one shared execution path with one optional pagination policy, not a second collector. Do not use `u32::MAX` as a disguised cap, add trace pagination, add configuration, duplicate collection, or remove the real byte/time protections.

**Smallest closure check.** Add one trace with more than 1,000 spans but below the existing byte ceiling to the current HTTP `pg_router_smoke` owner and the existing gRPC service-test owner; both must succeed with every span. Retain one existing collector byte-ceiling assertion proving an oversized encoded result fails explicitly. Do not duplicate pagination tests: the current paginated callers and `has_more` coverage remain authoritative.

### FIND-BIFROST-OTEL-T03-2A — VALID, MAJOR: unauthorized log projection

**Authority.** `spec.md:478-482` requires authorization before sensitive projection or return. Task lines 127-132 and Scenario 2 lines 171-183 explicitly require the log gate to cover new sensitive resource/scope columns before IO. `AGENTS.md` forbids duplicating table schema authority.

**Source evidence.** The sole table ledger declares `body`, `attributes`, `resource_attributes`, `resource_entity_refs`, and `scope_attributes` sensitive at `crates/vala/vala-bifrost-redux/src/tables/logs/records.rs:29-73`. `crates/wyrd/wyrd-server/src/vala_query/service.rs:458-483` drops only `body` and `attributes` for callers without `BifrostLogPayload:Read`.

**Reachable path.** An authenticated metadata-only caller reaches `query_logs` -> `build_query_logs_plan`; DataFusion projects three protected canonical columns into Oracle execution and collection before `extract_log_rows` ignores them. They are not serialized today, so this is protected-column IO rather than a direct response disclosure, but it violates the explicit pre-projection security boundary.

**Counterevidence attempted.** Tenant isolation, ordinary query permission, and response omission remain intact. None authorizes reading the three payload columns. The absence of serialization does not satisfy the pre-IO requirement.

**Ponytail correction.** Split this from row decoding. Reuse `vala_bifrost_redux::tables::builtin_table("logs", "records").sensitive_payload_columns`, the existing immutable registry field populated from `DomainTable::SENSITIVE_PAYLOAD_COLUMNS`; pass that slice to `drop_columns`. Do not restate five strings, import a concrete table solely for its associated constant, add another permission service, or redact after collection.

**Smallest closure check.** Extend the existing Scenario 2 plan inspection with one unauthorized QueryLogs assertion that none of the registry-declared sensitive columns appear. No new fixture or harness is needed.

### FIND-BIFROST-OTEL-T03-2B — VALID, MAJOR: canonical log decoding

**Authority.** `spec.md:290-297,456-459,614-618` requires complete supported log timing, severity, body, resource, and scope fidelity with no defaulting. Scenario 2 lines 185-191 directs the existing row extractors to canonical columns.

**Source evidence.** The canonical ledger stores `observed_time_unix_nano: Int64`, `severity_number: Int32`, and `body: Binary` at `records.rs:29-57`. `extract_log_rows` instead requests `observed_time` as a timestamp, `severity_number` as `Int64`, and `body` as `Utf8` at `vala_query/routes.rs:695-725`; failed lookups/downcasts become epoch, zero, and `None`. HTTP and gRPC both consume this extractor (`routes.rs:1110-1145`, `grpc.rs:640-680`).

**Reachable path.** Any representative canonical row reaches Oracle with the declared types. The extractor cannot find/downcast the three columns and successfully returns altered data. An authorized log body is always omitted; observed time and severity silently default.

**Counterevidence attempted.** Trace/span IDs now use the correct fixed-binary helper, but that isolated repair does not affect these three fields. The current `LogRow.body: Option<String>` at `wyrd-spec/src/vala/api.rs:914-932` and protobuf `string body` at `wyrd.v1.proto:320` can represent only string bodies and cannot preserve payload omission separately from an empty value, while the canonical OTLP `AnyValue` column supports more shapes.

**Ponytail correction.** The verdict's suggested string-only body decode is too narrow and would preserve data loss. Change the existing Rust/HTTP `LogRow` body contract to `Option<serde_json::Value>`, project the protobuf field as optional JSON text, expose one small response-edge helper in the existing `vala_query::payload` module that decodes a canonical `AnyValue` with its current lossless tagged-byte rule, and make `extract_log_rows` fallible as the trace/GenAI extractors already are. Read `observed_time_unix_nano` and `Int32` directly. Regenerate the existing proto/schema artifacts; do not add a log service, UDF, cache, alternate schema, or string coercion.

**Smallest closure check.** Add one authorized canonical log row to the existing Postgres-backed Scenario 2 test with nonzero observed nanos, a non-default severity, and a structured/byte-bearing body; assert exact HTTP output and payload omission for the unauthorized plan. One gRPC conversion assertion over the same corrected DTO is enough; do not duplicate the full dataset.

### FIND-BIFROST-OTEL-T03-3 — VALID, MAJOR

**Authority.** Approved revision 9 already exists at the immutable base and states `card_ref` is optional (`spec.md:182-201`, `INV-011` at 508-513, `AC-012` at 696-705). `architecture/wyrd-design.md:481-535` and `architecture/bifrost-design.md:39-59` agree. The task's revision-7 non-null statement at lines 46-53 is superseded current-authority drift, not a new product decision.

**Source evidence.** `catalog/wire.rs:93-106` synthesizes described `card_ref` with `nullable: false`; `writable_schema` reproduces that declaration (`wyrd-queue/src/schema.rs:75-100`). `BatchBuilder` stores a mandatory string, requires `&CardRef`, creates only `Some` values, and emits a non-null field (`batch_builder.rs:39-43,108-173`). In contrast, the canonical table correlation owner emits nullable `card_ref` and `run_id` (`tables/signal.rs:379-390`), and Scribe accepts either an absent column or null values (`scribe/execution_lanes.rs:695-724,3875-3948`).

**Reachable path.** A generic authenticated writer calls describe -> writable schema or `BatchBuilder::from_description` -> insert without Card correlation. The advertised schema rejects null and the JSON builder cannot express absence before the already-correct Scribe path can stamp `principal_id` and null `card_uid`.

**Counterevidence attempted.** Present Card references still resolve through signed scope and trusted UID mapping. Optionality does not bypass authorization. Directly constructing a custom Arrow schema can omit the field and reach Scribe, but that bypasses the new server-described first-class workflow and therefore does not close it.

**Ponytail correction.** Reuse the nullable table contract: set the synthesized description field nullable, store `Option<String>` in `BuiltRow`, accept `Option<&CardRef>` in `append_json_row`, and emit the existing nullable string array/schema. Existing queue callers pass `Some(&row.card_ref)`; no queue redesign is required to close this task's described-builder path. Do not infer a root Card, create defaults, query storage, add a cache, or add compatibility configuration.

**Smallest closure check.** Update the current describe/builder assertion to append one row with `None`, then extend the existing described-schema Postgres journey with exactly one absent/null `card_ref` row and assert accepted insert, non-null `principal_id`, and null `card_uid`. Reuse the existing harness.

### FIND-BIFROST-OTEL-T03-4 — VALID, MAJOR

**Authority.** `REQ-009`, `INV-002`, and task lines 111-115 and 139-152 require valid optional bounds through every first-class SDK without semantic loss.

**Source evidence.** `vala-sdk/src/query.rs:379-389` concatenates `DateTime<Utc>::to_rfc3339()` directly into the query string. The candidate test locks the raw `+00:00` target at `query.rs:2006-2009`. `wyrd-client/src/transport/http.rs:524-544` joins the path without query-pair encoding. Form query parsing decodes `+` as space; the concrete string becomes `2026-07-01T00:00:00 00:00`, which is not RFC3339. Python and TypeScript delegate to this same Rust method (`vala-sdk/src/python.rs:332-357`, `wyrd-node/src/lib.rs:539-568`).

**Reachable path.** Any bounded Rust request carries `DateTime<Utc>` and emits `+00:00`; Python and TypeScript parse caller offsets into `Utc` before delegation, then hit the same formatting. Axum `Query<TraceDetailBounds>` rejects the altered value before the handler.

**Counterevidence attempted.** Unbounded requests work. A caller-supplied literal `Z` does not survive as text because the SDK stores `DateTime<Utc>` and reformats it. The mock test sees the raw request line and therefore cannot prove Axum decoding.

**Ponytail correction.** Do not add a URL builder or dependency. Both values are already `DateTime<Utc>`, so use Chrono's existing `to_rfc3339_opts(..., true)` to emit the standard `Z` suffix at the two formatting sites. This is the smallest complete root fix shared by Rust, Python, and TypeScript.

**Smallest closure check.** Update the existing request-line unit test to include both bounds and assert `Z` with no raw plus. One tiny Axum/query-deserialization assertion for that emitted target closes the actual boundary; language-specific duplicates add no evidence because all three delegate to the same Rust owner.

### FIND-BIFROST-OTEL-T03-5 — VALID, MODERATE

**Authority.** `REQ-009`, `INV-002`, task lines 111-115, and the public structured-error rule require explicit invalid timestamp failure and equivalent HTTP/gRPC behavior.

**Source evidence.** `vala_query/grpc.rs:95-100` discards `parse_from_rfc3339` errors with `.ok()`. GetTrace uses it at lines 315-319, so malformed non-empty input becomes `None`. The older `proto_window` duplicates the same lossy parsing at lines 78-92 and is called by eight sibling gRPC query handlers. HTTP's typed Axum query at `routes.rs:791-815` rejects malformed text instead.

**Reachable path.** A gRPC caller sends `since = "bad"`; conversion yields an unbounded request, which can successfully return rows outside the caller's intended range. A valid parsed reversed range is still rejected later, so the bug is specifically discarded parse failure.

**Counterevidence attempted.** Empty strings intentionally mean no bound and valid bounds normalize correctly. Neither case distinguishes malformed non-empty input. No interceptor or later planner can recover the original invalid text.

**Ponytail correction.** The verdict's proposed fallible helper is correct but should be the sole parser used by both direct GetTrace fields and `proto_window`; make `proto_window` return `Result<QueryWindow, Status>` and propagate `?` at its eight existing callers. Leaving its duplicate `.ok()` would patch only the named symptom. Do not add a timestamp wrapper or new error type.

**Smallest closure check.** Add a table-driven converter/service test covering empty, valid, and malformed text once, plus one GetTrace assertion that malformed `since` becomes `InvalidArgument`. The shared parser makes per-handler malformed cases redundant.

### FIND-BIFROST-OTEL-T03-6 — VALID, MAJOR

**Authority.** `REQ-004`, `AC-002`, task lines 75-91, and `architecture/bifrost-design.md:602-621` require the Python described-schema direct Arrow insert to use the real configured server transport.

**Source evidence.** The established Python `Bifrost` constructor starts with `ClientConfig::from_env` and then overrides explicit HTTP/auth fields (`vala-sdk/src/python.rs:62-95`). The new `_NativeBifrostQueryClient` instead starts from `ClientConfig::default`, overrides only HTTP, and stores the default gRPC config (`python.rs:200-227`). `insert_batch` later connects through that retained client (`python.rs:416-455`). `ClientConfig::from_env` is the existing owner of `WYRD_GRPC_URL`; the default is localhost:50051 (`wyrd-client/src/config.rs:62-91`).

**Reachable path.** A valid remote, split-plane, or test deployment sets `WYRD_GRPC_URL` and constructs public `BifrostQueryClient(server_url, token)`. Reads use the explicit HTTP URL, but `insert_batch` connects to localhost:50051 and fails unless the server coincidentally uses the default.

**Counterevidence attempted.** Default local deployments work. TypeScript accepts an explicit optional gRPC URL in its native constructor. Neither makes the Python public workflow configurable. No later transport layer re-reads the environment.

**Ponytail correction.** The verdict already names the smallest fix: copy the adjacent established construction pattern by starting from `ClientConfig::from_env`, overriding the supplied HTTP URL, and retaining the resulting gRPC config while preserving bearer-token middleware. Do not add a Python constructor parameter, selector, new config owner, or transport abstraction.

**Smallest closure check.** Extend the existing Python direct-insert integration owner under its current `WyrdTestServer` fixture, whose non-default gRPC endpoint is already repository-managed, and assert the ACKed batch ID. A config-only unit test would not prove the transport reaches the intended server.

### FIND-BIFROST-OTEL-T03-7A — VALID, MODERATE: Python public typing

**Authority.** Task lines 27-32, 93-99, 139-152, and Scenario 3 lines 205-221 require exact first-class DTOs and generated projections. `architecture/references/languages/python-api-and-stubs.md:11-32` requires precise public types and source-driven stub regeneration.

**Source evidence.** `python/wyrd/bifrost/__init__.py:248-268` types description fields, the entire trace, and GenAI rows as `Any`; it omits `canonical_physical_fingerprint` and `next_page_token` entirely. Both generated stub copies repeat that shape (`__init__.pyi:64-81`, `stubs/bifrost.pyi:62-79`). The Rust wire explicitly exposes the fingerprint (`wyrd-spec/src/vala/api.rs:98-129`), exact nested trace DTOs (`647-800`), exact GenAI rows (`823-856`), and continuation token (`982-991`).

**Reachable path.** Runtime JSON contains these fields, but a typed Python caller either loses all field checking through `Any` or receives a type error for valid fingerprint/token access. `py:typecheck` passes because no fixture accesses the omitted fields through the declared return types.

**Counterevidence attempted.** Runtime serialization retains the values, and stubs match their source. Those prove runtime transport and mechanical generation only, not first-class typing precision.

**Ponytail correction.** Add the minimum nested `TypedDict` graph in the existing public Python source, using `NotRequired` for serde-omitted fields, and regenerate both current stub outputs. Do not introduce Pydantic models, runtime validation, a second module, or hand-edit generated `.pyi` files.

**Smallest closure check.** Add one existing-lane type fixture that accesses recursive field metadata, fingerprint, a nested event/link value, structured GenAI messages, and `next_page_token` through the public return types; run `py:typecheck` and `codegen:check`.

### FIND-BIFROST-OTEL-T03-7B — VALID, MODERATE: TypeScript public typing

**Authority.** The same task obligations apply. `architecture/references/languages/typescript-guide.md:14-29,31-48` requires stable explicit public return types and forbids ergonomic wrappers from forking wire contracts. The task additionally says description types come from `wyrd-spec` and TypeScript reads use generated DTOs.

**Source evidence.** `typescript/wyrd/src/index.ts:32-73` declares recursive data types as `unknown`, table entry/layout and trace/GenAI rows as `Record<string, unknown>`. The candidate test must cast a span to access `events` at `tests/unit/bifrost-query.test.ts:457-463`. Generated N-API declarations expose only `NativeLifecycleResult.valueJson`, so they do not supply the missing DTOs (`typescript/wyrd/index.d.ts:68-108,204+`).

**Reachable path.** `getTrace` and `queryGenAi` cast parsed JSON to the imprecise wrapper interfaces. Valid callers must narrow or cast fields whose exact Rust wire shapes are fixed, defeating the promised typed client while runtime values remain present.

**Counterevidence attempted.** TypeScript correctly uses `unknown` rather than unsafe `any`, and fingerprint/token top-level fields are declared. That is appropriate for genuinely arbitrary JSON, but these contract positions are not arbitrary: `wyrd-spec` defines them exhaustively.

**Ponytail correction.** Split this from Python. Project only the already-approved recursive `FieldSpec`/`DataTypeSpec`, trace, and GenAI shapes into the existing TypeScript source-generation owner, then reference them from the wrapper. Do not create runtime validators, duplicate transports, or a new package. The verdict's blanket instruction to hand-add shapes is insufficient if it leaves the task's explicit `wyrd-spec` generation requirement unmet; remediation must reuse an existing generation path or extend the current codegen task minimally. Do not add an independent generator unless the existing codegen owner demonstrably cannot emit these declarations.

**Smallest closure check.** Remove the current event cast and add one compile-only fixture accessing a recursive data type, nested event/link, structured GenAI message, and continuation token. Run `ts:typecheck`, `ts:napi:check`, and `codegen:check`; runtime unit coverage need only retain the existing one representative response.

## Duplicate, split, downgrade, and rejection decisions

- Split original FIND-2. Pre-IO authorization belongs to the plan/table registry; exact log decoding belongs to the response DTO/payload extractor. One fix cannot close the other, and combining them hides the lossless-body contract change.
- Split original FIND-7 by language. Python source/stub generation and TypeScript declaration generation have independent owners and gates. Their common symptom is imprecision, not a shared implementation root.
- Do not combine FIND-4 and FIND-5. Both involve timestamps, but FIND-4 is outbound HTTP encoding in the shared SDK while FIND-5 is inbound gRPC validation; they have different causes, callers, and closure tests.
- Do not combine FIND-1 with ordinary pagination. Paginated endpoints already consume `has_more` correctly; only the complete-detail adapters violate the contract.
- No severity downgrade is warranted for FIND-1, FIND-2A/B, FIND-3, FIND-4, or FIND-6 because each breaks an explicit public/security path. FIND-5 and FIND-7A/B remain MODERATE: they are material contract defects but do not bypass tenancy or silently mutate durable data.
- Reject none of the seven original IDs.

## Verification notes

- Reused the candidate-bound verification recorded in the reviewed verdict; no broad build or test lane was rerun because every retained defect is an uncovered behavior the passing assertions do not exercise.
- Reran `git diff --check base..candidate`; it passed.
- Executed a minimal standards-equivalent query-decoding probe confirming a raw `+` is decoded as a space. The candidate's source and request-line assertion establish that this exact raw form is emitted.
- No production, task, spec, prior review, or verdict file was edited.
