---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-benchmark-latency-diagnosis
spec_revision: 4
requirements: [REQ-003, REQ-005, REQ-006, REQ-007, REQ-011, REQ-013, REQ-014, REQ-015, REQ-016, INV-001, INV-002, INV-003, INV-004, AC-009, AC-010, AC-011, AC-012]
depends_on: [TASK-003]
---

# Reduce client and native JSON work with attributable performance evidence

## Outcome and Value

Implement the remaining SDK, benchmark-client, native request/response and buffer
optimizations in one task, then demonstrate their correctness and measured costs.
For example, a 64 KiB metadata call should not create an owned response tree in
the benchmark client merely to inspect two fields, and SDK retries should share
already serialized bytes instead of copying them on every attempt.

All work below is mandatory within TASK-004. Do not deliver a research-only
proposal, defer an item to a future task, or repeat a general diagnosis. Source
findings identify the work; focused implementation discovery chooses private
mechanics. The outcome includes code, caller/test closure, current allocation
evidence and separate production-versus-fixture comparisons.

The 702 KiB/call figure is a historical pre-TASK-003 server allocation estimate,
not today's allocation or copied bytes. The 64 KiB fixture sends and receives
64 KiB of content, plus framing. Report those facts accurately. Client p50 below
1 ms and residual p50 below 500 us remain separate investigation objectives,
not guaranteed deliverables or changes to benchmark judgments.

## What Changes and What Must Stay True

### Required implementation scope

| Work | Required result and boundary |
|---|---|
| SDK request replay | Transfer serialized JSON/Arrow/idempotent request backing once and share it across applicable attempts. Preserve exact bytes, request/idempotency identity and existing replay safety. |
| SDK per-call work | Remove Verify's whole-client timeout wrapper, temporary verifier-binding collections and avoidable key/header/origin/auth formatting at existing owners. Preserve effective timeouts, lookup/error ordering, fresh invocation/request IDs, scoped headers and current token-cache/refresh behavior. No new auth/header cache. |
| Provider response accumulation | Retain a single incoming chunk without an unconditional payload copy through eligible native/typed consumers; coalesce only when needed. Preserve bounds, decoded-byte counting, UTF-8 behavior, errors and retries. Multi-chunk fallback is required, not a reason to defer the fast path. |
| Native metadata ingress | Avoid the full owned request tree for eligible native metadata traffic, including the existing 64 KiB case. Keep owned materialization/fallback where validation, translation, capture or input form requires it. Preserve canonical usage bounds and upstream semantics. |
| Native response processing | Consolidate Gateway-native validation/adaptation so metadata handling does not first validate into a public raw wrapper and then scan the whole body again. Selected capture still owns scrubbed content. Standalone public Skald raw APIs keep their current validated `RawValue` contract. |
| Gateway driver and fake upstream | Decode only fields actually used by their existing predicates, borrowing unescaped content and using owned fallback when decoding requires it. Reuse static URL/auth/content-type setup; preserve per-window case/phase and caller context. |
| Allocation and performance proof | Obtain fresh current-reference and production-candidate server heaps. Measure production changes under the original fixture behavior, then fixture changes with the identical retained optimized server. Persist controls and canonical reports for every repetition. |

There are three different clients: Gateway's timed caller is direct Reqwest;
Verification's timed caller uses `WyrdClient`; the server's provider client is
Skald. The assertion-only Verification benchmark does not call a provider.
Model-calling Verification does share the provider path through the internal
Gateway caller. Do not attribute a change to a workload that does not exercise it.

Retain the single recorder, collection precision/cohorts, byte accounting,
admission-before-dispatch, capture policy and redaction, audit cardinality,
settlement/cancellation, tenant isolation, credentials, errors and durable retries.
Keep public SDK and HTTP/MCP/wire contracts unchanged. Internal owner-local
representation/signature changes and their consumer closure are implementation
work; no new parser, transport, graph index/cache, scheduler, dependency, Cargo
feature, public option or record-schema version is selected.

Do not reimplement work already present: request retries in Skald already share
backing; UTF-8 Vec-to-String transfer is a move; TASK-003 removed metadata's full
response Value and transient capture-key strings. Arrow/staged-row sharing is
already shallow. Neither source deletion nor allocation counts alone prove latency
savings. Preserve exact output and measure the effect.

### Compatibility cases that decide fallback

Native and fixture fast paths must preserve the old decoded-Value behavior:

- Escaped strings/names, duplicate members with last-value semantics, null and
  wrong types, unknown extensions, nesting and numeric bounds, and trailing
  whitespace/input errors. Direct raw deserialization into current borrowed
  `&str` views is not equivalent for escaped strings.
- Native token admission uses compact, normalized Value serialization length,
  not raw byte length: whitespace, escapes, duplicate collapse and number
  normalization matter. Model rewriting and provider-specific placement remain
  unchanged. Raw lexemes may be forwarded only where equivalence is established.
- Native JSON endpoint coverage includes OpenAI chat/Responses/embeddings,
  image/speech JSON and batch creation; Anthropic Messages; Gemini/Vertex
  GenerateContent; and Jev systemone. Eligible fast handling must reach these
  existing native owners. Multipart/files, batch lifecycle, translated requests,
  selected request capture and in-process typed callers retain the owned path
  where necessary; close their callers and regressions rather than rewrite them.
- Provider bodies preserve decoded-size limits despite absent/misleading
  Content-Length or compression, exact-limit acceptance, over-limit refusal,
  truncation/read failure, empty chunks and existing invalid-UTF-8 replacement.
- Driver validation still requires valid JSON, first choice message content of
  the expected decoded byte length, and unsigned `usage.total_tokens`; unrelated
  members/additional choices remain accepted. The upstream marker still defaults
  empty and streams only for Boolean `true`. Do not tighten these predicates
  accidentally with a naive derived struct.

The fallback policy is fixed: preserve existing behavior through the existing
owned path when a form cannot safely use the optimized representation. Do not
reject previously accepted input or weaken validation to obtain borrowing.

## Approach

1. Retain the current post-TASK-003 binaries, original fixture behavior, source
   identities and records. Obtain the bounded current-reference evidence below.
2. Remove SDK replay and per-call temporary work, preserving the existing transport,
   pool, auth cache, deadlines, identities and binding behavior.
3. Extend the shared bounded provider reader and existing Gateway native-body
   owners to remove the remaining copy/tree/validation work with compatible fallback.
4. Complete production tests and real SDK/server regressions. Retain and measure
   the production candidate while original fixture parsing/setup is still used.
5. Optimize driver/upstream decoding and invariant request setup with differential
   predicate, borrowing and dynamic-header proofs. Record the new fixture method.
6. Run the final same-server fixture comparison and report fresh allocations,
   client/server CPU, latency, throughput and all repetitions through the canonical
   renderer. Complete every scope item; do not silently skip unresolved proof.

## Ordered Implementation Scenarios

For changed executable branches, establish the smallest meaningful RED before
GREEN. Existing correct output behavior supplies regression evidence, not a
manufactured semantic failure. Pure allocation/ownership deletion also requires
structural or pointer-backed proof and the staged performance evidence. Never use
microsecond timing assertions or add a general allocation-counting framework.

### Scenario 1 — SDK attempts share serialized request ownership

**Behavior.** Initial and eligible replay attempts use the same serialized backing
with unchanged body, method, request ID and idempotency key. Auth refresh changes
only the credentials it currently changes. An ambiguous non-idempotent failure is
not made replayable by the new storage.

**RED.** Add the [planned SDK backing and replay tests](#sdk-request-and-replay-proof).
The current per-attempt `to_vec` must fail actual-request backing reuse; test
nonempty POST replay through a 401 and preserve ambiguous-failure policy. Existing
retry tests remain regression proof. Do not infer large-body correctness from the
existing mock's single 8192-byte read; extend that local fixture only as needed.

**GREEN.** Remove the initial/replay copy at the existing request owner and close
JSON, Arrow and idempotent consumers. Keep all prior retry/refresh rules.

**REFACTOR.** Reuse installed immutable byte/request primitives and existing
request assembly. Do not add a second client or body cache.

### Scenario 2 — Verify preserves deadlines without cloning the client

**Behavior.** Verify retains its effective timeout floor of the greater of the
configured timeout and execution deadline plus 10 seconds. Applying that floor
does not mutate ordinary request deadlines or copy unrelated gRPC/client state;
the stored Reqwest pool remains reused.

**RED.** Extend [the existing timeout proof](#sdk-timeout-and-binding-proof) to
exercise Verify and an ordinary request on the same client. Preserve the existing
longer-than-configured execution success. Pin behavior before removing the wrapper;
record the wrapper-copy removal structurally rather than inventing a timeout bug.

**GREEN.** Apply the effective timeout at the existing HTTP request owner without
whole-client cloning. Rerun scenario 1 and timeout/cancellation regressions.

**REFACTOR.** Remove obsolete wrapper plumbing and close its only Verify caller.
No public timeout option, pool replacement or runtime change is required.

### Scenario 3 — Binding and request assembly avoid temporary work

**Behavior.** Service-root, component and Agent bindings select the same verifier
and subject in the same order, including duplicate component occurrences and local
refusals. Calls still receive distinct run/request identities. Auth cache hit,
single-flight refresh, reactive 401 handling and scoped caller/benchmark headers
retain their exact meaning while unnecessary collection/formatting is removed.

**RED.** Add the [planned binding case](#sdk-timeout-and-binding-proof) for uncovered
selection/order edges; keep [identity/auth/header regressions](#sdk-auth-and-header-proof).
Where behavior already passes, record it as baseline regression proof and identify
the temporary collection/copy eliminated. Any changed selection/error branch must
have its focused failing assertion before GREEN.

**GREEN.** Reuse the hydrated graph, references and current auth/request owner to
remove temporary work. Do not hoist required run identity or input construction
out of timed Verify calls merely to improve the benchmark.

**REFACTOR.** Prefer direct traversal and existing values; no new graph index,
credential/header cache, longer token lifetime or logging of secret headers.

### Scenario 4 — Bounded response collection retains eligible chunks

**Behavior.** A single chunk can reach native relay or typed decoding without the
unconditional chunk-to-Vec copy. Multiple chunks preserve byte order through
coalescing. All size, compression, UTF-8, status/error and retry cases above keep
their existing result; empty and exact-limit bodies are handled explicitly.

**RED.** Add the [planned bounded-collection proof](#provider-buffer-proof), including
actual accumulator ownership and downstream consumption. Current unconditional
copying must fail the eligible single-chunk pointer/backing assertion. Pin
multi-chunk, limits, refusals and decoding as behavioral regressions.

**GREEN.** Extend the existing reader and its raw/typed callers so downstream
conversion does not immediately reintroduce the saved copy. Owned fallback remains
valid where UTF-8 replacement or coalescing requires it. Rerun SDK scenarios.

**REFACTOR.** Keep one bounded transport path; preserve public raw APIs and avoid
unchecked conversion, unbounded body collection or a new buffering framework.

### Scenario 5 — Native metadata ingress avoids an unnecessary owned tree

**Behavior.** Eligible native metadata requests, including the 64 KiB fixture,
retain borrowed/raw content through validation and dispatch instead of owning all
payload strings in a Value. Routing, model override, compact token bound, typed
errors, extension semantics and capture are identical. Other forms use compatible
owned fallback without losing endpoint coverage.

**RED.** Add the [planned native ingress proof](#native-ingress-proof), differentially
covering the compatibility cases and all native families. Require backing/ownership
evidence for the eligible large unescaped case; the current full Value path fails
that property. Escaped/duplicate/numeric/depth cases must match existing validation
and bounds, not become new refusals or raw-length shortcuts.

**GREEN.** Extend existing request, validation and adapter owners with the private
representation and necessary caller closure. Preserve the repository body limiter
and existing multipart/translated/capture/in-process paths. Rerun scenarios 1–4.

**REFACTOR.** Reuse existing Serde validation/serialization and native preparation.
Do not introduce a parallel parser, public request shape or bypass around admission.

### Scenario 6 — Gateway response adaptation performs one full validation pass

**Behavior.** Gateway-native successful JSON is validated and its usage/capture
projection obtained without a redundant full-response validation scan. Relay keeps
the same bytes. Selected capture remains scrubbed and owned; metadata remains
bounded. Standalone Skald `send_raw` still returns validated public RawValue.

**RED.** Add the [planned native response proof](#native-response-proof) for usage
precedence, duplicate/escaped fields, scalar/array responses, malformed/trailing
JSON, nesting/numeric bounds, capture selection, refusals and exact answer bytes.
Pin compatible results; document the current two-scan path and add focused
ownership/pass-count evidence at the actual adaptation owner where practical.
New validation branches get their failing cases before GREEN.

**GREEN.** Reuse bounded transport and existing usage/capture validation through a
Gateway-private validated response path. Close relay, typed/in-process caller and
native provider consumers without broadening the public raw-response enum/API.
Keep media, translated and streaming paths compatible. Rerun earlier scenarios.

**REFACTOR.** Remove redundant Gateway wrapping/scanning only after callers use
the validated result. No unsafe RawValue construction or skipped body validation.

### Scenario 7 — Driver and fake upstream borrow only the fields they use

**Behavior.** Buffered validation and upstream request selection satisfy exactly
their previous predicates, including escaped content and duplicate last-value
behavior. Large unescaped content is borrowed; unknown fields are validated but
not materialized into owned trees. Full receipt and validation remain inside the
completion clock; upstream pacing, markers and bytes remain unchanged.

**RED.** Add the [planned fixture predicate/borrowing proofs](#fixture-json-proof).
Compare a test-only old Value predicate against the new path over a shared edge
corpus. Current decoding fails the unescaped-content borrowing property. Negative
cases must still fail for the same reason; do not weaken response checks.

**GREEN.** Extend the existing driver/upstream owners only after the production
milestone has been measured. Preserve owned decoded fallback for escaped text and
all framing/stream behavior. Rerun prior scenarios and real public-path regressions.

**REFACTOR.** Reuse installed decoding primitives; no second benchmark driver or
new JSON grammar. Declare the fixture method change in existing workload controls.

### Scenario 8 — Prepared client setup preserves dynamic request context

**Behavior.** Stable URL, per-client credential and content-type preparation are
reused. Case/phase/caller headers are resolved for each current scope, never cached
across warmup/measured windows. Requests remain authenticated identically and their
completion timing still includes the same send/receive/validation boundary.

**RED.** Add the [planned prepared-request proof](#fixture-request-proof) spanning
two differently labelled windows and absent/present caller context. Retain scoped
SDK header regressions. Pin existing output behavior; static setup removal has
structural proof rather than an artificial timing assertion.

**GREEN.** Move only invariant setup to the existing per-client owner and preserve
dynamic labels, timeouts, pooling and request bytes. Rerun fixture predicate tests.

**REFACTOR.** Keep one request path; no stale header cache, shared mutable builder
or changed benchmark concurrency/workload.

## Acceptance Criteria

| Obligations | Owner/proof and failure observation |
|---|---|
| REQ-013; AC-009; INV-001,003 | Scenarios 1–3, SDK retry/auth/identity tests and real Verify journeys prove shared request backing, effective deadlines and unchanged selection/authority. New replay, stale context, altered IDs/deadlines or duplicated request backing fails. |
| REQ-015; AC-010; INV-001–003 | Scenarios 4–6 and native/typed provider/server journeys prove avoided eligible copy/tree/scan plus compatible fallbacks. Weaker decoded limits, changed canonical bounds, lost extension fields, changed captures/refusals or unsafe/public API changes fail. |
| REQ-014; AC-011; INV-001,003–004 | Scenarios 7–8 preserve old predicates, pacing and completion boundaries, prove borrowing, and keep dynamic labels. Dropped checks, stricter duplicate/escape handling or unrecorded fixture changes fail. |
| REQ-003,005–007,011,016; AC-012; INV-004 | Required A/B/C evidence below retains current server allocation units/populations, separate profiled/unprofiled records, component identities, every repetition and canonical reports. Historical 702 KiB presented as current/copies, fixture gains credited to production, or missing profiles counted as zero fails. |
| REQ-013–016; AC-009–012 | Every listed reduction and its consumer/test closure is implemented within this task. Fresh server heaps and stage-specific comparisons identify measured effects and unresolved individual attribution. Required work cannot be replaced with proposals, fabricated speedups or a deferred task. |

## Implementation Reference

Paths are inspected reuse guidance, not an implementation allowlist.

- **SDK:** `crates/shared/wyrd-client/src/transport/http.rs::HttpTransport::{request_json,request_arrow,send_with_retry}`
  serializes once but copies into attempts; JSON/Arrow/idempotent callers share
  this owner. `verify/mod.rs` calls `client.rs::with_min_request_timeout`, whose
  only Verify use clones gRPC config/origin despite an HTTP request. Existing
  request construction already supports effective request-local deadlines.
  `state.rs` verifier resolution, `observe/mod.rs`, auth and `transport/trace.rs`
  own binding lookup, fresh invocations, cached credentials and task-local headers.
  Extend them without a second transport or resolution cache. Existing local
  HTTP mocks close connections and read only 8192 bytes once; they do not prove
  pooling or large-body replay unless their owner fixture is extended.
- **Provider buffering:** `crates/skald/skald-providers/src/clients/mod.rs::read_bounded_bytes`
  copies each chunk; `send` and raw/typed OpenAI/Anthropic/Google/Vertex/Jev callers
  consume the result. `raw.rs::send_raw` owns the standalone RawValue promise.
  Reuse this bounded reader; downstream ownership determines whether a copy was
  actually removed. Package name is `wyrd-skald-providers`.
- **Native requests:** `wyrd-server/src/components/gateway/routes.rs::{typed_body,usage_bound}`
  starts with `Json<Value>` and computes compact bounds. `GatewayCallRequest`,
  `wyrd-gateway::{CallInput,ProviderAttempt}`, adapter check/prepare and
  `RequestBody::to_json` carry/serialize it. Existing borrowed contract views in
  `skald-spec/src/wire/borrowed.rs` operate over decoded Values and cannot simply
  be applied to escaped raw text. Close routes, ingress, caller, batches, engine,
  adapter and affected test constructions; preserve the owned route for typed
  Verification and noneligible inputs. A private representation extension is
  justified because the current always-owned body cannot supply borrowed ingress.
- **Native responses:** adapter `complete`/`usage_member`, provider bounded send,
  Gateway response relay and `ServerWyrdGatewayCaller` are the existing closure.
  Safe public RawValue construction validates before adapter validation, so the
  private native path must retain validated text/bytes at the existing owner;
  retaining both scans under new names does not satisfy this outcome. Keep the
  standalone public `ProviderResponse::RawV1` contract unchanged.
- **Fixtures:** `wyrd-testing/src/bin/capacity/gateway/workload.rs::{buffered,exchange}`
  parses the answer and builds request headers. `src/gateway_capture.rs::answer`
  parses the stub request; its buffered answer is already prebuilt shared Bytes.
  `gateway/run.rs` prebuilds the body and reuses one HTTP client per worker.
  Optimize these actual gaps, not client construction or response-byte cloning
  already absent. Verify uses `WyrdState`/SDK with an assertion fixture.
- **Evidence:** `Population`, `Repetition`, `Record::render`, `profile::{Profiler,HeapProfiler}`,
  `capacity::release_binary`, and existing `bench:*` tasks own counts, reports,
  profiling and process execution. Extend controls/readings where needed rather
  than create an alternate report/analysis service. `release_binary` finds the
  server beside the actual executable; there is no server-path override flag.

## Verification and Evidence

All tests use confirmed existing packages, targets and runners. New selectors
below are **planned**: confirm their actual name after adding them, before RED;
never count a combined filter as proof that a missing planned test ran. Default
features suffice except the named server tests using `test-support`. Run all
Cargo commands sequentially. Credential-free unit/local-mock tests need no
Postgres; real SDK/server journeys use the explicit setup below.

### SDK request and replay proof

`mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=transport::http::tests::serialized_body_backing_is_shared_across_attempts)'`

Planned actual request-backing pointer/length/content proof, including spare input
capacity, initial attempt and replay.

`mise exec -- cargo nextest run --locked -p wyrd-client --test integration -E 'test(=transport::http::transport_behavior::json_body_and_request_identity_survive_auth_refresh) | test(=transport::http::transport_behavior::non_idempotent_json_is_not_replayed_after_ambiguous_failure) | test(=transport::http::transport_behavior::request_json_round_trips_json) | test(=transport::http::transport_behavior::submit_idempotent_replays_key_across_retry) | test(=transport::http::transport_behavior::submit_with_idempotency_key_replays_caller_key_across_retry) | test(=transport::http::transport_behavior::four_oh_one_re_exchanges_api_key_and_retries_with_fresh_token) | test(=transport::http::transport_behavior::revoke_credential_stops_after_a_second_refusal)'`

First two tests planned; others exist. Prove exact replay bytes/identity, refresh
and bounded retry behavior through the real HTTP transport and local mock.

### SDK timeout and binding proof

`mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=observe::tests::verifier_resolution_preserves_subjects_and_binding_order) | test(=observe::tests::execute_outlives_the_configured_timeout) | test(=observe::tests::execute_pins_files_and_completes_shared_uploads) | test(=observe::tests::values_project_onto_named_slots_by_slot_type) | test(=observe::tests::enqueue_replays_its_key_and_wait_returns_only_a_real_result)'`

First test planned; others exist. Extend the timeout test for ordinary request
deadline isolation. Cover root/component/Agent selection and negative ordering.

### SDK auth and header proof

`mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=observe::tests::each_run_is_its_own_invocation) | test(=observe::tests::scoped_views_share_one_invocation_and_keep_their_subjects) | test(=auth::tests::exchange_caches_access_token_and_drops_refresh) | test(=auth::tests::force_refresh_re_exchanges_once) | test(=auth::tests::concurrent_bearer_callers_single_flight) | test(=auth::tests::workload_exchange_caches_and_serves_from_shared_cache) | test(=auth::tests::cached_token_debug_redacts_secret) | test(=transport::credential::tests::redacted_debug_does_not_leak_secrets) | test(=transport::trace::tests::scoped_benchmark_labels_are_sent) | test(=transport::trace::tests::scoped_caller_context_is_outbound)'`

`mise exec -- cargo nextest run --locked -p wyrd-client --test integration -E 'test(=transport::http::transport_behavior::wyrd_request_id_is_present_on_every_request) | test(=transport::http::transport_behavior::wyrd_access_token_header_is_injected_not_authorization) | test(=transport::http::transport_behavior::cross_origin_absolute_url_is_rejected_for_authenticated_requests)'`

Existing identity, cache/refresh, context, origin and credential-scrubbing proofs.

### Provider buffer proof

`mise exec -- cargo nextest run --locked -p wyrd-skald-providers --lib -E 'test(=clients::bounded_answers::bounded_collection_reuses_single_chunks_and_coalesces_multiple_chunks) | test(=clients::bounded_answers::oversized_answers_fail_and_refusals_keep_status_and_body) | test(=clients::bounded_answers::native_transport_reuses_owned_buffers) | test(=raw::raw_passthrough::raw_v1_writes_body_bytes_directly_and_returns_raw_response)'`

First test planned; others exist. Extend limit/UTF-8/compression cases as needed.

### Native ingress proof

`mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=components::gateway::routes::usage_bound_tests::native_raw_ingress_matches_value_contract_and_bounds) | test(=components::gateway::routes::usage_bound_tests::request_validation_preserves_contract_without_payload_copies) | test(=components::gateway::routes::usage_bound_tests::usage_bound_requires_an_output_maximum_where_output_exists)'`

First test planned; others exist. Differential validation, canonical bounds and
backing proof must cover all listed native families and owned fallback cases.

### Native response proof

`mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::native_response_validation_preserves_usage_capture_and_answer_bytes)'`

Planned equivalence and actual adaptation ownership/validation-path proof.

`mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(/^adapter::tests::/)'`

Existing complete narrow adapter regression owner, including TASK-003 usage,
capture, scrubbing and native/translated response behavior.

### Fixture JSON proof

`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=gateway::workload::tests::buffered_validation_matches_the_existing_json_predicate) | test(=gateway::workload::tests::buffered_validation_borrows_unescaped_content) | test(=gateway::workload::tests::events_parse_across_any_split_and_judge_the_whole_stream)'`

First two tests planned; final stream/framing regression exists.

`mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=gateway_capture::tests::paced_request_projection_preserves_validation_and_routing)'`

Planned fake-upstream predicate/borrowing proof in its existing library owner.

### Fixture request proof

`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=gateway::workload::tests::prepared_requests_keep_dynamic_window_headers)'`

Planned invariant setup and dynamic scoped-context proof through actual requests.

### Real SDK and server regressions

Run `mise run build:server` before these journeys; it is their existing build
prerequisite, not a release performance build. Use repository Postgres/migrations
and local mocked providers; no external credentials or new SDK capability.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::agent_answer_passes_its_verifier) | test(=verify_in_real_time::agent_answer_fails_its_verifier) | test(=verify_in_real_time::unbound_verifier_fails_locally) | test(=verify_in_real_time::caller_without_the_verifier_grant_is_refused) | test(=verify_in_real_time::another_tenant_cannot_verify_the_assistant) | test(=verify_in_real_time::submitted_answer_is_judged_in_the_background_and_resumes_by_id)"'
```

Existing real SDK Verify success/failure, authority, tenant and resumable/idempotent
execution proof. The model-calling runtime covers provider work absent from the
assertion-only capacity experiment.

```bash
mise exec -- env WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::configured_gateway_headers_are_authorized_and_captured) | test(=compatible::openai_compatible_streams_terminate_and_survive_a_capture_outage) | test(=native::anthropic_messages_ingress_is_native_governed_and_fail_closed) | test(=native::gemini_generate_content_ingress_is_native_governed_and_fail_closed) | test(=resilience::capacity_capture_accounting_waits_for_post_response_work) | test(=resilience::benchmark_mode_keeps_capture_and_snapshot_counts_without_prometheus)"'
```

Existing public native/compatible routes, exact capture/accounting and recorder
regressions. Extend the existing journey owners for any newly changed negative
wire behavior not exercised by this set; do not substitute unit mocks for it.

### Owner and static closure

- `mise exec -- cargo nextest run --locked -p wyrd-client --lib` — affected SDK owner.
- `mise exec -- cargo nextest run --locked -p wyrd-skald-providers --lib` — shared raw/typed provider owner.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(/^gateway::/) | test(/^verification::/)'` — affected driver consumers.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=profile::tests::heap_captures_and_failures_become_profile_evidence)'` — existing profile-evidence regression.
- `mise run fmt:check` — Rust format.
- `mise exec -- cargo clippy --locked -p wyrd-client -p wyrd-skald-providers -p wyrd-gateway -p wyrd-server -p wyrd-testing --all-features --all-targets -- -D warnings` — affected-owner lint policy.
- `mise run check:deps` — preserve tier and production dependency boundaries.
- `mise exec -- git diff --check` — patch hygiene, followed by final source/link inspection.

Include the exact focused regressions of any additionally touched shared owner in
the execution record. No aggregate `gate`, new dependency/feature or generated
schema is required by the selected scope.

### Bounded allocation and performance protocol

This is non-TDD performance verification, not a manufactured unit-test failure.
Planning runs no builds or experiments. Present this concrete protocol for the
spec's explicit experiment authorization; after authorization, perform it within
TASK-004 rather than moving its required evidence into another task. No historical
baseline rebuild/rerun or profiler installation is authorized by planning.

Retain the existing post-TASK-003 unprofiled records
`target/benchmarks/gateway/overhead/20261110T123522Z-57e143636.json` and
`target/benchmarks/gateway/overhead/20261110T124026Z-7d7c73b9d.json` unchanged.
The old November 9 heap is historical context only. No current per-call total is
known until fresh server allocation evidence exists.

| Milestone | Measured components | Required invocations and interpretation |
|---|---|---|
| A — current reference | Current post-003 server and original fixture/SDK behavior, retained before implementation changes | One unprofiled Gateway subset, one matching profiled Gateway subset and one unprofiled Verification subset. Establish current costs and fresh server allocations; this is a newly declared current-reference experiment, not reconstruction of a historical baseline. |
| B — production candidate | Scenarios 1–6 implemented; original Gateway driver validation/setup and stub behavior | Same three invocations as A. A→B measures production server/SDK changes under the same fixture method. Keep per-component attribution; Verify exercises SDK changes, Gateway's timed caller does not. |
| C — fixture candidate | Identical retained B server; scenarios 7–8 added to the driver/stub, other production code unchanged | One unprofiled Gateway subset. B→C measures driver/stub changes and their shared-host effects. No extra Verification run/profile is needed for Gateway-only fixture changes. |

Use the existing `mise run bench:build` before retaining each build milestone;
never compile while measuring. Retain real copies of the executables, not
symlinks, in `target/benchmarks/task004/{a-current,b-old-driver,c-new-driver}/`.
Each directory contains `capacity` and `wyrd-server`. C's server must be copied
from retained B, not silently taken from a new build, and their SHA-256 hashes
must match. `capacity::release_binary` discovers its sibling server; there is
no invented `--server-bin` flag. Run from repository cwd.

The exact seven invocations follow. Execute each block at its milestone, not all
at once: retain and measure A before production edits, B before fixture edits,
and C after fixture edits. There is no full-suite sweep.

A — current reference:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task004/a-current/capacity" l1 gateway --allow-dirty --steps buffered-1kib-capture-metadata.c64,buffered-64kib-capture-metadata.c64,buffered-64kib-capture-payload.c64
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task004/a-current/capacity" l1 gateway --allow-dirty --profile --steps buffered-1kib-capture-metadata.c64,buffered-64kib-capture-metadata.c64,buffered-64kib-capture-payload.c64
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task004/a-current/capacity" l1 verification --allow-dirty --steps direct-verify-c1,direct-verify-c64
```

B — production candidate, original fixture behavior:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task004/b-old-driver/capacity" l1 gateway --allow-dirty --steps buffered-1kib-capture-metadata.c64,buffered-64kib-capture-metadata.c64,buffered-64kib-capture-payload.c64
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task004/b-old-driver/capacity" l1 gateway --allow-dirty --profile --steps buffered-1kib-capture-metadata.c64,buffered-64kib-capture-metadata.c64,buffered-64kib-capture-payload.c64
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task004/b-old-driver/capacity" l1 verification --allow-dirty --steps direct-verify-c1,direct-verify-c64
```

C — optimized fixture, identical B server:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- "$PWD/target/benchmarks/task004/c-new-driver/capacity" l1 gateway --allow-dirty --steps buffered-1kib-capture-metadata.c64,buffered-64kib-capture-metadata.c64,buffered-64kib-capture-payload.c64
```

These use normal declared warmup and three measured repetitions, no smoke/pace
or workload-size changes. `--steps`/`--allow-dirty` are diagnostic controls; retain
source/diff identities and never commit merely to run a benchmark. One invocation
per listed milestone/mode is the authorization scope. If invalid or inconsistent,
retain/report it and obtain a bounded rerun authorization rather than silently
repeating until favorable. Keep existing integrity/resource/registry validity rules.

Retain each component's source/build identity and SHA-256, toolchain/settings,
original versus new fixture revision, commands, record, every raw window snapshot,
server log and matching executable/symbols. Source detection uses the current
checkout, so it does not by itself identify a retained mixed driver/server pairing.
Declare that component lineage in existing controls/evidence; never mislabel a
retained binary as built from the current checkout. C changes only the fixture
method control, not server timing boundaries or record schema. Canonical reports
must show declared controls and all repetitions; do not introduce another formatter.

`--profile` requests CPU and server heap captures; there is no heap-only flag.
Missing samply is recorded nonfatally and must not prevent independent heaps.
Inspect actual heap files/sample types, not just the aggregate profile-complete
flag. Do not install profilers, change OS permissions, or claim unsymbolized CPU
attribution. Server heaps do not measure driver allocation.

Compute current server allocation/call and objects/call from named `alloc_space`
and `alloc_objects` cumulative step deltas by frame-stack identity. Use the full
profiled step's reconciled calls, including warmup and all repetitions, not only
measured throughput. Validate sample units, nonnegative deltas, nonempty evidence
and positive counts. Keep statistical sampling and background/inter-step work
explicit. Report `inuse_space`/`inuse_objects` and RSS separately, never as
allocation or copied-byte totals. Missing profile evidence is not zero. Use
borrowing/request-backing tests for driver/SDK ownership; no new allocator profiler.

Compare A→B and B→C separately with each repetition's client/server/residual
quantiles, phase counts/bounds, provider exchange, CPU/work, throughput, driver
CPU and capture/drain validity. Under their matching methods, later-stage median
client p50, residual p50/p99 and server/driver CPU per request must be no worse
than the previous stage's worst repetition; throughput must not be lower than its
worst repetition. This task-local nonregression envelope is not a product SLO,
benchmark judgment change or statistical significance claim. Investigate a miss
and fix it within scope; never silently widen the envelope or conceal an outlier.

Every reduction needs concrete source/ownership proof and its relevant measured
comparison; individual speedups below repetition/sampling resolution must be
reported as unresolved rather than invented. Keep all scope items implemented,
report whether both investigation objectives were met, and do not infer a median
hardware limit from mean throughput, subtract CPU/work from wall-time percentiles,
or credit A→C directly to production. Current allocation/profile analysis and
report updates belong in the existing research report and this task's evidence.

## Material Stop Conditions

- Correctness requires public wire/SDK/raw-API changes, a new durable contract,
  dependency/feature or owner, or a changed retry/admission/security policy.
- The only proposed fast path weakens decoded-size, canonical-bound, numeric,
  nesting, duplicate/escape, secret-redaction or capture/acknowledgement rules.
- The experiment cannot identify the retained components, preserve B/C's identical
  server or distinguish fixture methods. Resolve evidence/lineage first; do not
  replace it with an unsupported performance claim.
- A required experiment lacks explicit authorization or fails validity. Complete
  independent code/test work, retain the gap and present the bounded protocol;
  do not claim task completion or defer required implementation to another task.

Private representation, compatible owned fallback, existing fixture extensions,
request-local timeout plumbing and narrow Serde/byte ownership changes belong to
the implementor. They are not reasons to stop for another design exercise.

## Authority Links

- [Spec revision 4](../spec.md) and [implemented prerequisite](TASK-003-recorder-overhead-and-standard-reporting.md).
- [Existing research](../research/gateway-latency.md), [benchmark authority](../../../../benchmarks/README.md).
- [Workflow](../../../spec-driven-development.md), [implementation execution](../../../implementation-execution.md), [testing](../../../../TESTING.md), [repository rules](../../../../AGENTS.md).
- [Rust](../../../../agent_docs/rust.md), [maintainer style](../../../../agent_docs/maintainer-style.md), [agent surfaces](../../../../agent_docs/agent-surfaces.md).
- [Capture/admission authority](../../../../architecture/wyrd-design.md#principal-model), [Bifrost](../../../../architecture/bifrost-design.md), [security boundaries](../../../../architecture/wyrd-security-posture.md).

## Execution evidence

Commits: `b211aab41` (scenarios 1–3), `03c0b1d99` (4), `9b7dccc94` (6),
`a31d121c4` (5), `a5fed5839` (7–8), `3ae65fe4a` (post-B heap finding, below).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-013; AC-009; INV-001,003 — shared request backing, effective deadlines, unchanged selection/authority (scenarios 1–3) | `wyrd-client/src/transport/http.rs` (`serialize_body` → `Bytes` shared by every attempt; `request_json_within`, `request_json_with_min_timeout` per-request deadline, no client clone); `verify/mod.rs`, `state.rs`, `observe/mod.rs` (`target` borrows bound Verifiers) | `transport::http::tests::serialized_body_backing_is_shared_across_attempts`; integration `json_body_and_request_identity_survive_auth_refresh`, `non_idempotent_json_is_not_replayed_after_ambiguous_failure`, `execute_outlives_the_configured_timeout` (ordinary request keeps configured deadline); `observe::tests::verifier_resolution_preserves_subjects_and_binding_order`; `wyrd-client --lib` 271/271; wyrd-ai verify journeys 6/6 | PASS |
| REQ-015; AC-010 — bounded collection keeps an eligible single chunk (scenario 4) | `skald-providers/src/clients/mod.rs` bounded reader returns the single chunk's `Bytes`; raw/typed callers in `openai.rs`, `anthropic.rs`, `google.rs`, `vertex.rs`, `raw.rs`; `3ae65fe4a` stops `JsonAnswer::validated` shrinking that buffer into a copy (`engine.rs`, `JsonAnswer(String)`) | `clients::tests::bounded_collection_reuses_single_chunks_and_coalesces_multiple_chunks` (pointer/backing, multi-chunk order, limits, empty and exact-limit); `adapter::tests::native_response_validation_preserves_usage_capture_and_answer_bytes` now also pins kept capacity (RED: 148 ≠ 212 before `3ae65fe4a`); `skald-providers --lib` 50/50 | PASS |
| REQ-015; AC-010; INV-001–003 — native ingress without an owned tree (scenario 5) | `wyrd-gateway/src/request.rs::RequestJson` (kept compact text only when it equals the decoded value's compact serialization, else decoded); `routes.rs::JsonBody` with axum's own content-type probe; `adapter/http.rs::media_json`; Skald `send_media_json`/`stream_media_json` | `routes::usage_bound_tests::native_raw_ingress_matches_value_contract_and_bounds` (46 cases, tally kept/decoded/refused `[18, 20, 8]`; every native validator and bound; `to_json` against `RequestBody::Object`; content-type and refusal parity with `Json`; 64 KiB pointer and allocation checks); `adapter::tests` native body pointer check; server Gateway component tests with Postgres 66/66 | PASS |
| REQ-015; AC-010 — one validation pass for native answers (scenario 6) | `adapter/mod.rs::native_answer` validates while reading usage, scans again only to classify a rejection; `JsonAnswer::validated`; `raw.rs` keeps Skald `send_raw` returning a validated public `RawValue` | `native_response_validation_preserves_usage_capture_and_answer_bytes` (differential with the former two-scan path, outcomes `[20, 8, 12]`, kept allocation); `wyrd-gateway --lib` 69/69 | PASS |
| REQ-014; AC-011; INV-001,003–004 — driver and stub borrow only judged fields (scenario 7) | `wyrd-testing/src/gateway_capture.rs::pick` (`Step`, `Leaf`, decoded-value rules, duplicate last-wins); `capacity/gateway/workload.rs::buffered` | `gateway_capture::tests::paced_request_projection_preserves_validation_and_routing`; `gateway::workload::tests::buffered_validation_matches_the_existing_json_predicate` (answered 15), `buffered_validation_borrows_unescaped_content`; capacity bin 58/58 | PASS |
| REQ-014; AC-011 — prepared caller keeps dynamic window labels (scenario 8) | `workload.rs::Caller`; `run.rs`, `capacity.rs` (`Driver::callers`); `mod.rs` control `fixture_method` | `gateway::workload::tests::prepared_requests_keep_dynamic_window_headers` (method, path, headers and body equal to the former request in two labelled windows and outside one) | PASS |
| REQ-003,005–007,011,016; AC-012; INV-004 — A/B/C evidence | seven authorized invocations, each run once, all VALID; retained executables and hashes below | tables below; heap step deltas from `alloc_space`/`alloc_objects` | PASS |
| REQ-013–016; AC-009–012 — every reduction implemented and attributed | scenarios 1–8 in the commits above | stage comparisons below; unresolved items named | PASS |
| Owner and static closure | — | `mise run fmt:check`; clippy `-D warnings` on wyrd-client, wyrd-skald-providers, wyrd-gateway, wyrd-server, wyrd-testing; `cargo check --workspace --all-targets --all-features`; `mise run check:deps`; `git diff --check`; Gateway journeys 6/6 (after `a31d121c4`, `a5fed5839`, `3ae65fe4a`); profile regression `profile::tests::heap_captures_and_failures_become_profile_evidence` | PASS |

### Experiment lineage

Executables built with `mise run bench:build` (release, `-C force-frame-pointers=yes`,
line tables, rustc 1.99.0) and retained as real copies:

| Milestone | Source | `capacity` SHA-256 | `wyrd-server` SHA-256 | Fixture method |
|---|---|---|---|---|
| A `task004/a-current` | `d4ea94988` | `126ec2f2f707a5c4…` | `16deac5dc4b4cb79…` | original (no `fixture_method` control) |
| B `task004/b-old-driver` | `a31d121c4` | `2cb5edd465796d8a…` | `b092b1aaf40b525a…` | original (no `fixture_method` control) |
| C `task004/c-new-driver` | `a5fed5839` | `2d4ec1a45d230927…` | `b092b1aaf40b525a…` copied from B, matches | `prepared-borrowed` (declared control) |

Full hashes: `target/benchmarks/task004/{b-old-driver,c-new-driver}/SHA256SUMS`.
Logs: `target/benchmarks/task004/<milestone>/*.log`. Records (all VALID, exit 0):

| Run | Record | SHA-256 prefix |
|---|---|---|
| A Gateway | `gateway/overhead/20261110T144132Z-d4ea94988.json` | `b579114b8ff054c4` |
| A Gateway `--profile` | `gateway/overhead/20261110T144306Z-d4ea94988.json` | `2d810cb638b13eef` |
| A Verification | `verification/direct-verify/20261110T144441Z-d4ea94988.json` | `cc7734014ea7a67c` |
| B Gateway | `gateway/overhead/20261110T162206Z-a31d121c4.json` | `1280ecab42ca7abf` |
| B Gateway `--profile` | `gateway/overhead/20261110T162345Z-a31d121c4.json` | `b68b6322ed9dcb11` |
| B Verification | `verification/direct-verify/20261110T162531Z-a31d121c4.json` | `25155648c69059cf` |
| C Gateway | `gateway/overhead/20261110T163901Z-a5fed5839.json` | `76a60d8c65282153` |

Commands are the seven in the protocol above, run from the repository root.

### A → B: production changes, original fixture

Unprofiled Gateway, per repetition (A | B). Envelope: B median against A's worst.

| Step | client p50 ms | residual p50 µs | residual p99 µs | server CPU µs/req | driver CPU µs/req | throughput /s |
|---|---|---|---|---|---|---|
| 1 KiB meta | 1.264 1.368 1.227 \| 1.002 1.040 1.052 | 181.1 176.1 175.4 \| 172.9 174.2 172.5 | 743 772 592 \| 469 512 516 | 345.2 349.8 340.4 \| 334.2 347.4 337.4 | 64.7 63.3 61.7 \| 60.4 61.2 60.7 | 46288 40887 47839 \| 59376 57472 56541 |
| 64 KiB meta | 2.619 2.676 2.528 \| 2.333 2.284 2.204 | 391.4 390.9 387.3 \| 355.3 360.2 357.6 | 1712 1710 1552 \| 1431 1420 1358 | 752.8 751.5 745.8 \| 721.8 743.5 724.8 | 187.9 186.7 182.3 \| **190.8** 192.5 189.1 | 22831 22547 23961 \| 26078 26575 27264 |
| 64 KiB payload | 4.849 4.645 4.442 \| 4.873 4.600 4.653 | 2474 2806 1706 \| 2572 1759 2486 | 64815 57180 64979 \| 60326 59736 43549 | 1810.6 1617.3 1643.9 \| 1825.7 1677.2 1589.6 | 293.6 272.3 275.0 \| 303.7 288.6 283.3 | 8263 8667 8798 \| 8239 8608 9168 |

- Every figure meets the envelope except 64 KiB metadata driver CPU per request
  (B median 190.8 against A worst 187.9 µs, +1.5%). The driver source is identical
  between A and B (`git diff d4ea94988 a31d121c4 -- crates/wyrd/wyrd-testing/src/bin`
  is empty). It still decodes each 64 KiB answer into a `Value`; B served 16% more
  requests on the same shared host. The matched profiled pair meets the envelope
  (Bp median 193.4 against Ap worst 198.9 µs), and C's driver work removes the
  cost (173.9 µs, below). No production change is credited or blamed for it.
- Phase means: `parse` 79 → 71 µs (64 KiB meta), `translate` 92 → 64 µs (64 KiB
  meta) and 134–147 → 102–110 µs (payload). In the 1 KiB case: −1 µs and −2 µs.
- Errors 0, every arrival staged, nothing undrained, in all repetitions.
- Profiled pair (Ap | Bp) meets the envelope on every figure.

Verification, the SDK changes (A | B):

| Step | client p50 ms | server CPU µs/verification | driver CPU µs/verification | throughput /s |
|---|---|---|---|---|
| c1 | 0.079 0.080 0.081 \| 0.078 0.082 0.080 | 106.1 121.6 109.1 \| 104.6 126.9 108.7 | 25.2 25.6 26.9 \| 25.9 27.3 26.8 | 10233 10095 9955 \| 10344 9663 9988 |
| c64 | 0.708 0.711 0.706 \| 0.705 0.728 0.703 | 216.8 225.1 217.1 \| 215.9 224.6 218.1 | 73.0 72.9 73.0 \| 71.4 72.1 71.0 | 83453 82522 84222 \| 83256 80144 83247 |

The envelope holds. The SDK work (shared body, per-request deadline, borrowed
binding) is below this protocol's resolution at c1; the c64 driver CPU −2% is
near resolution. Gateway's timed caller does not use the SDK, so no Gateway
figure is credited to scenarios 1–3.

### B → C: driver and stub changes, identical server

| Step | client p50 ms | residual p50 µs | residual p99 µs | server CPU µs/req | driver CPU µs/req | throughput /s |
|---|---|---|---|---|---|---|
| 1 KiB meta | 1.002 1.040 1.052 \| 0.983 1.007 1.005 | 172.9 174.2 172.5 \| **175.5** 175.7 173.4 | 469 512 516 \| 467 482 464 | 334.2 347.4 337.4 \| 328.0 341.7 339.7 | 60.4 61.2 60.7 \| 50.9 51.7 51.2 | 59376 57472 56541 \| 60935 59314 59850 |
| 64 KiB meta | 2.333 2.284 2.204 \| 2.179 2.168 2.259 | 355.3 360.2 357.6 \| **362.5** 361.5 367.6 | 1431 1420 1358 \| 1300 1296 1317 | 721.8 743.5 724.8 \| 729.5 726.5 757.2 | 190.8 192.5 189.1 \| 173.9 171.8 176.5 | 26078 26575 27264 \| 27818 28040 27003 |
| 64 KiB payload | 4.873 4.600 4.653 \| 4.656 4.440 4.532 | 2572 1759 2486 \| **2826** 2808 2873 | 60326 59736 43549 \| 56787 56263 55411 | 1825.7 1677.2 1589.6 \| 1751.0 1589.5 1604.9 | 303.7 288.6 283.3 \| 264.5 248.4 254.9 | 8239 8608 9168 \| 8537 9494 9274 |

- Driver CPU per request drops 16% / 9% / 11%. Client p50 and throughput improve
  in every case. These are fixture gains, not production gains.
- **Residual p50 misses the envelope in all three cases** (+0.7%, +0.6%, +9.9%
  against B's worst). The server executable is byte-identical, so no server
  code moved. Residual means are unchanged (182.2–185.4 against 182.9–185.6 µs;
  396–404 against 397–401 µs). Calls held inside the server rise (rate × mean
  `server_elapsed`: 36.5 → 36.9, 36.7 → 37.0, 52.7 → 54.0), and `queue_delay`
  p50 rises (32.8 → 33.4 µs, 52.2 → 53.5 µs, payload 0.80–1.93 → 2.24–2.28 ms).
  The faster driver returns each client to the closed loop sooner, so each
  server call shares the host with more concurrent work. The payload B
  repetitions are bimodal (1.76 against 2.49–2.57 ms); C is not. This is a
  shared-host effect of the fixture change. It cannot be fixed inside C,
  whose server must stay identical to B, and it is retained here rather than
  explained away.

### Server allocation per call (profiled A and B)

Source: `heap-<step>.pb.gz` under `20261110T144306Z/profiles/` (A) and
`20261110T162345Z/profiles/` (B). Sample types `alloc_objects`/count,
`alloc_space`/bytes, `inuse_objects`/count, `inuse_space`/bytes. Cumulative
captures are differenced by stack identity (function names, else address)
from the previous step. Every per-stack allocation delta is non-negative.
Calls come from the step's reconciled `gateway.buffered` count, warmup plus three
repetitions, in the last window's cumulative timing snapshot. These are A:
1,006,371 / 443,997 / 154,018 and B: 1,078,644 / 476,463 / 164,796.

| Step | A bytes/call | B bytes/call | A objects/call | B objects/call |
|---|---|---|---|---|
| 1 KiB meta c64 | 193.9 KiB | 193.4 KiB | 1117 | 1086 |
| 64 KiB meta c64 | 646.7 KiB | 584.3 KiB | 1155 | 1081 |
| 64 KiB payload c64 | 2065.7 KiB | 2037.8 KiB | 1591 | 1596 |

- The 1 KiB figure includes startup and pre-step work, because its capture has no
  earlier one. All figures include background and between-step work and are
  statistical samples.
- `inuse_*` are held memory, not allocation: B ends the steps at 279 / 446 /
  1356 MB.
- RSS peaks are in the records and are not allocation totals.
- **64 KiB metadata, −62 KiB/call.** By the innermost non-allocator frame:
  - Removed: the anonymous async-fn frame (65.4 → 1.0 KiB) and `visit_str`
    (64.3 → 0.3 KiB). They match the ingress owned tree and the response
    `RawValue` copy; the frames alone do not prove which is which.
  - Added: `JsonAnswer::validated` (64.0 KiB), where `into_boxed_str` shrank the
    reader's buffer into a fresh allocation.
  - Net: one payload-sized copy removed per call. Smaller owners: borrowed views
    replace `SeqRefDeserializer`/`MapRefDeserializer` visits.
- **Payload, −27.9 KiB/call.** The same two copies are removed and the same
  `validated` copy is added. Capture still needs the request tree, so a
  `deserialize` frame decodes it (64.0 KiB). The net change is in Arrow and
  Parquet capture encoding (`extend_from_dictionary` −22.1 KiB), which is
  background capture work, not request ownership. It is not credited to this
  task, and attribution is unresolved.
- **1 KiB, −0.5 KiB/call:** below sampling resolution. Objects −31/call.
- **Post-B finding, `3ae65fe4a`:** `JsonAnswer` now keeps the validated
  `String`, so the 64 KiB `validated` copy is gone. The kept-capacity assertion
  proves it. It postdates B and C, and no authorized run measured it.

### Investigation objectives (c64 p50, C record medians)

| Case | Client < 1 ms | Gateway-added < 500 µs |
|---|---|---|
| 1 KiB metadata | missed: 1.005 ms (one repetition 0.983) | met: 175 µs |
| 64 KiB metadata | missed: 2.179 ms | met: 362 µs |
| 64 KiB payload | missed: 4.532 ms | missed: 2.826 ms |

The payload tail stays with `admit` (Scribe capture-slot wait), outside this
task's scope.

### Limits

- Batch creation keeps the owned path for its JCS digest. Multipart media and
  translated requests decode at ingress, as before.
- `trace::headers()`/`benchmark_headers` still copy each window's labels; the
  labels are dynamic per window.
- `03c0b1d99` adds direct `bytes` and `http` edges to skald-providers. Both were
  already locked workspace dependencies through `reqwest`, so no package or
  feature was added. `check:deps` passes.
- Individual SDK speedups are below resolution, and so are the 1 KiB heap and
  phase changes. They are reported as unresolved, not claimed.
- No median hardware limit is inferred from throughput, and A → C is not
  credited to production.
- Missing samply on a run would have been nonfatal; both profiled runs captured
  CPU and heap evidence. CPU samples are not attributed to symbols.

### Non-goals

- No public SDK, wire or raw-API change.
- No new durable contract, owner, parser, dependency or feature, and no
  generated schema.
- No change to retry, admission, security, capture or redaction policy.
- No unsafe `RawValue` construction, and no weakened decoded-size, bound,
  numeric, nesting, duplicate or escape rule.
- No historical baseline rebuilt or rerun, and no profiler installed.
- No benchmark repeated.
- Files outside the task's owners: only callers updated in the same commits
  (`pg_invocation_tests.rs` and the `wyrd-testing` Bifrost verification runtime).
