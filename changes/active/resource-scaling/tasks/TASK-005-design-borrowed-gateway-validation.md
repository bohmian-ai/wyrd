---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-resource-scaling
spec_revision: 5
requirements: [REQ-003, REQ-010, REQ-012, REQ-013, INV-006, INV-008, AC-003, AC-008, AC-010, AC-011]
depends_on: []
---

# Remove redundant Gateway and direct verification copies

## Outcome and Value

Implement both Gateway and direct verification copy removal in this task.
Gateway work covers ingress validation through provider encoding.
The server already owns parsed request JSON. Today validation builds an owned
typed request only to throw it away; the recorded cost is 2,098,793 allocated
bytes for a 2 MiB body. Other remaining paths clone media/batch JSON or build a
translated payload during candidate selection and build it again for dispatch.

After this task, validation and candidate selection inspect the retained input
without another owned payload. Native forwarding serializes borrowed input with
the deployment's model substitution. A selected translated attempt constructs
its provider representation once. Capture still receives the original caller
content under the existing selection, redaction and lifecycle rules.

Direct verification currently duplicates the full input map entering the engine
and again entering the hosted evidence reader. It also copies selected JSON
solely for comparisons, duplicates successful step output, and allocates a full
JSON buffer only to hash captured context. Remove those redundant representations
while preserving original evidence, exact outcomes and staging behavior.

TASK-005 is not complete until both implementations and their required proof
pass. Neither verification work nor its benchmark may be deferred to an audit,
design deliverable, follow-on task or unowned scope correction.

This replaces the November 8 design-only proposal following the maintainer's
November 9 instruction to plan implementation. No separate design deliverable or
follow-on task is required. The existing filename is retained for packet links.
TASK-005 owns TASK-004's removed scenario 3 and the request-copy work that task
deferred, including translated double preparation. TASK-004's implementation,
review history and measured evidence remain unchanged. There is no completion
dependency on its outstanding performance verdict; revalidate the source below
against the implementation checkout before starting.

The maintainer explicitly confirmed the combined scope on 2026-11-09. Approved
spec revision 5 records it as REQ-013/AC-011. The
[direct-route audit](../direct-verification-copy-audit.md) supplies supporting
source evidence; this task is the authoritative implementation/proof packet
for both paths.

## What Changes and What Must Stay True

### Decision: derive borrowing from the existing contracts

Use contract-local declarative expansion to emit the existing owned types and
borrowed validation projections from one declaration of fields and Serde rules.
Use Serde decoding for both. Extend the existing Skald and Wyrd contract owners;
do not add a procedural-macro crate, dependency, Cargo feature, schema validator,
independent field-rule table or second JSON parser.

The owned expansion preserves public names, fields, types, constructors, methods,
traits, serialization and generated schemas. Only the internal validation
capability and necessary cross-crate wiring are added; SDK projections do not
change. Shared declarations must cover the transitive request graph, not only
the six top-level requests. Payload-free enums can be reused directly.

Borrowed validation strings refer to the original JSON. Unconstrained JSON leaves
that are only validated use Serde's ignored-value decoding rather than owned
`Value`; object-valued fields still require objects and retain borrowed keys
where semantic checks need them. Nested typed fields keep their corresponding
Serde rules. Preserve numeric types/ranges, enum order, defaults, missing/null
distinctions, renames, flattening and unknown-field policies. A newly declared
field must enter both expansions or fail compilation; unsupported rules must
fail visibly rather than silently fall back to owned or permissive validation.

Borrowed projections are validation data, not replacement serializable request
APIs. Consumers that need arbitrary JSON content borrow it from the original
`Value`; do not construct a second JSON tree to make the projection serializable.
Flattened/untagged Serde decoding may allocate structural bookkeeping. Removing
payload-byte ownership, rather than promising zero allocation or zero traversal,
is the objective.

Replace Responses' temporary `Vec<Value>` easy-input rewrite with shared typed
normalization for owned and borrowed decoding. Accept canonical items first,
then easy message forms. Default a missing message discriminator, reject an
explicit null discriminator, retain role/content requirements, and normalize
text into output text for `assistant` and input text otherwise. Preserve every
canonical variant and ignored-extra behavior. Move owned strings or borrow input
strings as appropriate; never mutate the original ingress JSON. Preserve the
existing manually adapted easy-input schema. The enclosing untagged input's
failure projection must remain unchanged.

### Decision: borrow until the actual encoding/conversion boundary

Apply the shared borrowed contracts to all six typed ingress operations: Chat
Completions, Responses, Embeddings, Speech, Image Generation and Batch Create.
Keep validation at its current position before authorization/dispatch, with the
same stable error code and missing-field versus `body` attribution. Embeddings'
preflight, dispatch and response-faithfulness checks also use borrowed request
data, preserving input ordering/count/emptiness, dimension and encoding rules.

Keep native Chat/Responses' existing borrowed `NativeBody` model overlay. Extend
the existing provider encoding workflow so media and batch routes accept borrowed
members plus that overlay without rebuilding a `Value`. Preserve existing public
provider call signatures for their callers; they and the Gateway path converge
on the same encoding owner. JSON writes directly to its final transport buffer.
Multipart writes scalar text and framing directly into the existing final
segments, retaining scalar-array/null behavior, escaping, file streaming and
length/bound checks. Batch JSONL writes each retained line directly to its final
buffer with nested `body.model` substitution, preserving line order, unknown
members and final newlines. Delete the full-map/line clone conversions once unused.

Candidate representability checks stay before admission for every candidate in
the existing order. Separate those checks from output construction inside the
existing translator owners, sharing the checks with actual translation. Planning
must not call owned request decoding or construct discarded provider payloads.
Dispatch constructs a target only for the attempt actually selected. Do not cache
prepared payloads by candidate, provider family or fallback; a retry after a
failed attempt must not accumulate retained translated bodies.

Preserve first-error selection, candidate filtering, capability/model checks,
stream restrictions and unsupported-member attribution. Shared checks cover
system-message position, roles/content, images, tools/arguments/output modes,
Google tool-call references and Jev request forms. Validate JSON argument syntax
and object shape through existing raw JSON support without building an owned
argument tree just for preflight. Google tool-result interpretation belongs to
actual conversion. Jev retains its distinct rules: native `systemone` validates
documented bounds; Chat pass-through validates its embedded request shape without
adding those native-route bounds. Preserve fragment-concatenation and escaped
JSON semantics. Necessary parser/unescaping or joined-fragment scratch may remain
bounded by current input limits and must be identified in evidence; it is not a
license to decode/copy the complete outer request again.

Initial HTTP parsing, actual provider translation/encoding, selected redacted
capture construction and final Arrow buffers still require work and ownership.
Keep existing cheap `Bytes`/request-handle sharing across transport retries. No
change to admission, budgets, fallback policy, tenant/audit attribution, capture
content, cancellation, post-response ownership, queue geometry or scheduling.
Do not adopt the separate `gateway-latency-capture` draft, retune batching, add
pools/caches, or reopen Scribe/Variant optimization.

### Decision: retain one direct-verification evidence owner

Transfer the decoded direct request's evidence into one immutable execution-owned
backing, shared by the engine and existing hosted evidence reader. Keep small
identity/routing metadata and mutable attachment/resolution bookkeeping separate.
Preserve caller-declared slots versus synthetic attachments. Stage the same
original declarations and exact outcome through the existing result builder;
release internal readers before reclaiming data needed by an owned result.
Moving the deep copy from admission to result assembly does not meet the task.

HTTP and MCP direct callers must converge on the same existing control path.
Preserve owned public request/result and executor APIs, exact evidence identity,
verdicts/captured actual values, input validation, file/digest limits, citations,
permissions/audit, stage pressure/order, deadlines, cancellation and independent
step concurrency. Direct persistence always retains `queue_execution=false`.
No queued/group/worker or scheduler redesign is authorized; adapt shared callers
only as needed to retain their current behavior.

Borrow an already selected JSON value for read-only comparison. Preserve the
existing declaration-to-JSON representation only for non-JSON evidence comparisons.
Keep successful step output in one internal owner until dependent steps and final
result assembly have consumed it. Context hashing writes the same Serde byte
sequence into SHA-256 without retaining serialized JSON; preserve digests exactly,
including member order and escaping. Initial decoding, required owned executor
inputs, independently owned extracted/cited results, captured actual values and
final Variant encoding still require work. Do not change public executor
lifetimes to remove a required ownership boundary. Explain remaining retry and
citation copies rather than claim all execution allocation is gone.

## Approach

1. Record the immutable immediate baseline and revalidate the inspected copy
   inventory. Preserve existing move/borrow paths rather than rewriting them.
2. Add shared contract declarations and borrowed projections; integrate all six
   ingress callers and embeddings consumers, including shared Responses decoding.
3. Remove native media, multipart and batch clone intermediates through existing
   encoding owners, preserving caller compatibility and transport retries.
4. Share translator checks between borrowed candidate preflight and selected
   conversion; eliminate planning-only owned decoding and target construction.
5. Share direct verification's immutable original evidence through the engine,
   hosted reader and staging; remove read-only comparison copies, redundant
   successful-output ownership and hash-only serialization buffers.
6. Complete focused checks, then paired allocation profiles and full unprofiled
   Gateway and Verification L1. Record required remaining materializations.

## Ordered Implementation Scenarios

### Scenario 1 — Validation inspects the retained request

**Behavior.** Each of the six ingress operations accepts/rejects the same JSON
and projects the same Gateway error field/code as before, while validation no
longer copies payload strings or arbitrary extension values. Owned request APIs,
Responses normalization and schemas remain compatible. Validation success or
failure leaves the original input unchanged.

**RED.** Add one planned table-driven [validation proof](#validation-proof) using
the existing server allocation counter. First exercise current `typed_body` with
prebuilt large messages/extensions and assert absence of payload-byte allocation
growth: the owned decoder fails that assertion, not compilation/setup. In the
same test, pin expected acceptance/error projection before refactoring and compare
owned decoding with borrowed validation afterwards. Cover missing versus null,
integer-valued floats/range edges, unknown members, tagged/untagged forms and
Responses easy/canonical forms. Grow message and arbitrary extension strings
independently at fixed node counts, excluding input construction and the reference
owned decode from allocation observation. Do not create a test for every field.

**GREEN.** Implement the shared declarations/normalization and route integration.
Reuse borrowed embeddings data for semantic and answer checks. Run the validation
proof, existing Responses checks and embeddings regression; preserve original
input content and inspect the emitted schema/codegen for unchanged surfaces.

**REFACTOR.** Remove obsolete disposable decoders and share only repeated contract
declaration mechanics needed by these request owners. Keep arbitrary JSON access
on the retained original and all scenario checks green.

### Scenario 2 — Native forwarding writes the final payload directly

**Behavior.** JSON media, multipart and batch lifecycle calls carry the correct
deployment model, credentials and untouched unknown fields without a cloned JSON
map. Batch uploads rewrite each line's model without cloning the line or creating
a temporary JSON string. Multipart text is not copied into intermediate full-text
segments. Input remains unchanged across attempts; file and retry behavior stays
the same.

**RED.** Extend the existing [native-copy proof](#native-copy-proof), using the
existing server allocation observer when allocation assertions are needed, to
exercise actual media/batch encoding with large text. Observe allocations through
the production encoding boundary, excluding fixture/mock setup: current map/line
clones and multipart string copies must fail the reduced-copy observation.
Separate required final encoded bytes from disposable intermediates. Existing
media, batch and JSONL tests assert native model/content/order; extend those cases
only where the changed boundary lacks an assertion.

**GREEN.** Route borrowed input/model overlays through the existing provider
encoder and write JSONL/multipart directly to their final storage. Rerun scenario
1 and the focused native/transport checks. No serialize-then-reparse workaround
and no duplicated provider transport implementation.

**REFACTOR.** Delete unused `to_value` and cloned-line paths. Preserve one encoder
per transport form, existing public callers and immutable retry-body sharing.

### Scenario 3 — Planning checks candidates without constructing their payloads

**Behavior.** An unrepresentable candidate is filtered with the same offending
field, order and first-error result before admission. A representable translated
call constructs its provider payload only when dispatched, with the same output
and fallback behavior. Large input text is borrowed during planning; the planner
does not retain translated bodies for candidates it may never use.

**RED.** Extend the same [native-copy proof](#native-copy-proof) with translated
candidate selection under the server's allocation observer. At fixed request
shape, increase text bytes and candidate count; current full `prepare` during
planning fails the no-discarded-payload-copy assertion. Extend existing
[translation/fallback checks](#translation-and-fallback-proof) only for gaps in
error ordering, embedded Jev JSON and exact target output. One table of equivalent
candidates is enough; no new benchmark matrix or language suite.

**GREEN.** Move/share checks inside the current translation owners and use borrowed
input for preflight. Materialize only the selected attempt's target and release
it on completion/failure/cancellation through the existing attempt lifecycle.
Rerun scenarios 1–2 and the translation/fallback checks. Keep syntax checks,
cross-message references and native-versus-chat Jev rules intact.

**REFACTOR.** Remove discarded full preparation from routing; ensure target
builders cannot acquire a divergent second set of representability rules. Keep
required parser scratch separate from outer-body copies and report its cost.

### Scenario 4 — Direct verification reuses original evidence through staging

**Behavior.** A direct call with large inline evidence uses the same original
input backing in engine evaluation, hosted evidence reads and eventual staging.
It returns/stores exact original declarations and outcomes, distinguishes
synthetic attachments, and never queues execution. There is no equivalent deep
clone at result assembly or retention beyond the execution's existing lifecycle.

**RED.** Add the one planned [direct ownership proof](#direct-verification-proof)
at the existing server test owner, observing prebuilt large input outside fixture
construction. Current hosted-reader construction fails the shared-input ownership
or payload-growth assertion. Extend the existing direct route regression only
where needed for original-input/output readback after drain, including passed and
failed results, identity/audit counts and no enqueue activity. Profiles must also
identify the control-path `request.inputs.clone()`; a reader-only fix is incomplete.

**GREEN.** Move/share evidence through the existing direct control, engine and
reader owners; adapt HTTP/MCP/shared callsites without public contract changes.
Preserve original declarations for result staging and reclaim without a late
clone. Run the ownership, direct route, validation and staging regressions.

**REFACTOR.** Remove obsolete duplicate evidence owners. Keep attachments and
resolution state at existing owners and all original-input/result assertions
green; preserve the already direct `EncodedVariant::from_serialize` path.

### Scenario 5 — Verification reads and hashes JSON without disposable copies

**Behavior.** An assertion compares its already selected JSON without another
copy; hash capture produces exactly the existing SHA-256 digest without a full
temporary serialized buffer. Full/hash/redact capture and non-JSON evidence
comparisons preserve current outcomes and representations.

**RED.** Extend the same direct ownership proof with a fixed-shape large selected
value and hash capture, separating the executor's required selected-value copy
and final captured value from disposable comparison/serialization allocation.
Current comparison and hash paths fail that observation. Extend the existing
context-capture regression with independently computed compact-Serde digests for
nested, Unicode and escaped values, rather than comparing the hash helper to
itself. Pin non-JSON comparison representation through existing engine cases.

**GREEN.** Borrow selected JSON for comparisons and stream the unchanged Serde
byte sequence into the existing hash implementation. Keep the non-JSON conversion
where required. Rerun direct ownership, input/trace and context-capture checks.

**REFACTOR.** Delete temporary comparison/hash payload storage, retaining exact
capture policies and error behavior. Do not introduce a second canonicalization
policy or general serializer framework.

### Scenario 6 — Successful step output serves dependents and results once

**Behavior.** A successful action's large JSON output remains available to all
dependent requirements/steps and appears unchanged in the final public result,
without a second full output held only for the result ledger. Failed/skipped
steps and retry attempts preserve their existing outcomes and accounting.

**RED.** Extend the same direct ownership proof with a successful large output
and dependent read; current settlement fails the duplicate-output observation.
Reuse the existing dependency-order regression for exact dependent/final output,
and existing concurrency/deadline/retry checks for lifetime preservation. Input
and required final result construction remain outside the duplicate-copy claim.

**GREEN.** Retain one output under the existing engine ledger and assemble the
public step result once dependency evaluation finishes. Preserve current public
types, result ordering, provenance, tool calls and retry/error state. Run all
earlier direct checks and the dependency/concurrency/deadline/retry regressions.

**REFACTOR.** Remove redundant settled/result payload ownership, releasing state
at its existing completion/cancellation boundaries. No prepared-future/executor
lifetime migration or step-concurrency change.

## Acceptance Criteria

| Obligations | Observation and proof |
|---|---|
| REQ-010; AC-008 | Scenario 1 covers all six decoders and embeddings consumers: unchanged acceptance/error projection, no payload-string/extension-byte allocation slope at fixed structure, unchanged input, public API and schema. Shared declarations prevent rule drift. |
| REQ-010; AC-008 | Scenario 2 exercises actual media/batch/JSONL/multipart encoding: required model/content/ordering and retry ownership remain exact; profiles/owner observations show the cloned map/line/text intermediates disappear. |
| REQ-003/010; INV-008; AC-008 | Scenario 3 preserves pre-admission candidate filtering and first errors; dispatch output/fallback remain exact without planning-only owned payloads or prepared-body caches. |
| INV-006/008; AC-008 | Existing focused real-server Gateway journeys preserve native/translated streaming and exact attributed capture after drain, including peer Scribe and cancellation. Reuse existing coverage; add no new user-facing capability or language surface. |
| REQ-013; INV-006/008; AC-011 | Scenario 4 proves original evidence sharing through direct engine/reader/staging without a late clone; route readback preserves exact inputs/results, identities, audit and no enqueue activity. |
| REQ-013; INV-008; AC-011 | Scenario 5 removes second read-only comparison copies and hash-only buffers while preserving full/hash/redact behavior, non-JSON representation and independently pinned digests. |
| REQ-013; INV-008; AC-011 | Scenario 6 retains one successful step output for dependent evaluation and final results, preserving order, provenance, concurrency, cancellation and retries. |
| REQ-012/013; AC-003/010/011; INV-008 | Paired profiles attribute eliminated copies. Full unprofiled Gateway AND Verification L1 are mandatory, with zero errors/refusals/lost/shed/undrained work and exact verdict populations. Existing whole-change main/pre-TASK-002 allocation/RSS obligations remain; neither benchmark substitutes for the other. |

Other approved obligations remain with TASK-001–004. This task does not reset
their baselines, weaken their acceptance or claim that earlier blocked evidence
has passed.

## Implementation Reference

Inspected source at planning HEAD `785897fa90f29b1ded30f0a9255588a548164aa4`:

| Existing owner and consumers | Gap and selected extension |
|---|---|
| Server `components/gateway/routes.rs::typed_body`, callers at 926/981/1027/1386/1390/1752 | `DeserializeOwned` allocates a discarded request. Retain JSON/error projection and decode borrowed contract projections. |
| `wyrd-spec/src/gateway/openai.rs` wrappers plus local Speech/Image/Batch declarations; Skald `wire/openai_chat.rs`, `openai_embeddings.rs`, `openai_responses.rs` and transitive request types | Existing contracts already own the rules. Generate owned/borrowed declarations together locally; shared Serde replaces a second validator. Keep three transparent Gateway wrappers and all owned consumers intact. |
| Skald `openai_responses.rs::easy_items`, `input_items_schema`; existing owned authoring/prompt/provider/SDK consumers | Temporary JSON normalization copies content. Shared typed normalization moves/borrows text while preserving public shapes and schema. |
| Gateway `adapter/mod.rs::embeddings`, `inputs`, `faithful_embeddings` | Repeated owned request decoding. Extend semantic/answer checks to borrowed data without changing response validation. |
| Gateway `adapter/mod.rs::NativeBody`; `adapter/http.rs` media/open-media/batch calls; providers `clients/openai.rs` media/batch/multipart encoding | Native Chat/Responses already borrow; other routes call full-map `to_value`. Extend the existing encoder to the borrowed overlay, retaining current provider callers and file transport. |
| Gateway `adapter/batch.rs::BatchInput::native` and `BatchAction::route` | Clones each complete line and then allocates JSON text. Serialize borrowed nested model overlays directly to final JSONL. |
| Gateway `routing.rs::CallPlan::retain_representable` → `adapter/mod.rs::prepare` → Anthropic/Google/Jev request builders; `adapter/http.rs::dispatch` repeats it | Check-only preflight shares translator-owned guards; only selected dispatch constructs output. Existing system/text/image/argument helpers must not construct payloads merely to validate them. |
| Gateway engine/admission/capture; provider `clients/mod.rs::send_bytes_with_retry` | Request is already moved into the call, borrowed during execution and moved into terminal capture. Retry request clones share bytes. Preserve these owners; no redesign is needed. |
| Server `components/verification/service.rs::VerificationControl::run_direct` → `VerificationEngine::run`; direct HTTP/MCP consumers | `request.inputs.clone()` duplicates original evidence while request inputs are retained for staging. Transfer evidence into execution ownership and stage those same original declarations. |
| Server `verification/hosted/blocks.rs::HostedBlocks::engine` → `hosted/evidence.rs::TenantEvidence` | `execution.evidence.clone()` creates another full map. Share the immutable execution backing; retain mutable attachment/resolution bookkeeping separately and reclaim original data without a late clone. |
| `vala-verify/src/blocks/scope.rs::Scope::resolve/select` → `blocks/engine.rs::judge/json` | Selection already produces an owned executor value; comparison clones JSON again solely to read it. Borrow that selected JSON; retain necessary non-JSON declaration conversion and existing executor ownership. |
| `vala-verify/src/blocks/engine.rs::Attempted::settle` and dependency ledger | Successful output is cloned into both `StepResult` and `Settled::Succeeded`. Keep one internal output and assemble the public result after dependent reads complete. |
| `vala-verify/src/assertion.rs::hash_context_value`; engine context-capture consumer | Compact JSON is allocated solely to hash. Stream identical Serde bytes into the existing SHA-256 owner; no hash/canonicalization semantics change. |
| Server `verification/results.rs::ResultPayloadBuilder::inputs/build`; `pg_verification_routes` regressions | Already borrows declarations/outcomes and encodes directly to Variant. Reuse this staging workflow; preserve original/synthetic slot distinction, result-before-summary ordering and `queue_execution=false`. |

Installed Serde supports borrowed strings from `&Value`, including its buffered
content used by flatten/untagged decoding. `IgnoredAny` avoids owned arbitrary
values. Existing JSON Schema validation is unsuitable as a decoder replacement:
integer-valued floats and custom Responses forms need the actual Serde rules.
The existing error derive crate does not own request generation. No new general
compiler/validation framework is justified.

Likely writes are the contract, Gateway adapter/routing, provider encoding,
direct verification control/engine/reader and associated test owners above.
No dependency, feature, SDK binding, durable
format or architecture-policy change is expected. Paths guide reuse, not a
private helper/type allowlist. There are no nested `AGENTS.md` files in these
owners. `wyrd-spec` stays IO/async-free and Skald stays independent of server/Vala.

## Verification and Evidence

Minimal additions: one new table-driven test for Gateway validation and one
focused direct-verification ownership/allocation test with the necessary cases.
Extend existing preparation and engine/route cases rather than add new suites.
Use the server's already-installed thread-local `CountingAllocator` to observe
production calls where necessary; do not add another allocator, test crate or
benchmark framework. If allocation observation needs a server-side invocation
of an existing Gateway case, keep it in the same planned test rather than copy
the case into another suite. Label those measured subcases separately.

### Validation proof

New name **planned**, in the existing credential-free server library module.
Confirm its actual nextest selector after adding it, before RED:

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::routes::usage_bound_tests::request_validation_preserves_contract_without_payload_copies)'
```

Existing Responses regressions (no Postgres):

```sh
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=wire::openai_responses::tests::easy_input_messages_decode_as_typed_messages)'
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=wire::openai_responses::tests::responses_input_keeps_its_form_until_edited)'
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --features schemars --lib -E 'test(=wire::openai_responses::tests::input_items_schema_admits_easy_messages)'
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::embeddings_preserve_batches_and_reject_invalid_requests)'
```

### Native-copy proof

Extend existing cases; mock providers, no Postgres:

```sh
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::request_preparation_avoids_duplicate_body_materialization)'
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::media_routes_relay_json_binary_and_text_answers)'
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::batch_actions_reach_their_routes_with_native_files)'
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::batch::tests::batch_input_keeps_one_model_and_uploads_native_lines)'
mise exec -- cargo nextest run --locked -p wyrd-skald-providers --lib -E 'test(=clients::bounded_answers::native_transport_reuses_owned_buffers)'
```

### Translation and fallback proof

Existing checks, no Postgres:

```sh
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=routing::tests::fallback_keeps_only_capable_and_representable_candidates)'
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::unrepresentable_chat_features_are_rejected_with_the_offending_member)'
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::openai_chat_translates_to_anthropic_and_back)'
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::openai_chat_translates_to_gemini_and_back)'
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::jev_bad_forms_are_rejected_before_provider_io)'
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::jev_chat_pass_through_carries_the_typed_request_and_answer)'
```

### Direct verification proof

One new name **planned**, at the existing server hosted-block library test owner
with its existing engine construction and server `CountingAllocator`. Confirm
the actual selector after adding it, before RED. Observe only production
ownership/comparison/hash/settlement work, excluding input/fixture construction
and unavoidable final encoding. Keep async observations on the measured thread
and out of unrelated network/setup work so a missing allocation cannot pass due
to task migration. Independently exercise current direct control through the
existing real-route tests and profiles; a hosted-reader-only saving is not enough.

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::blocks::tests::direct_execution_reuses_evidence_without_disposable_payload_copies)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E "test(=pg_verification_routes::direct_execution_judges_inline_without_enqueueing)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E "test(=pg_verification_routes::direct_execution_refuses_nonconforming_evidence)"'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::no_step_verifier_judges_inputs_and_traces)'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::context_capture_hashes_or_redacts_actual_values)'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::dependents_wait_for_upstream_step_requirements)'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::independent_actions_overlap_within_the_concurrency_bound)'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::execution_deadline_cancels_running_work)'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::retries_spend_the_budget_and_keep_attempts_visible)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::results::tests::outcome_writes_output_then_summary)'
```

These are correctness/ownership checks, not throughput tests. Existing direct
route assertions protect passed/failed outcomes, exact identities, one audit per
call and no enqueue activity; extend only missing original-input/output readback
after drain. Existing dependency, capture, concurrency, deadline and retry checks
protect the affected lifetime seams. No new per-language suite or benchmark
matrix is required.

### Existing server journeys and owner checks

Use repository Postgres/migrations and the checked-in local mock-provider key.
These are regression proof, not new TDD scenarios or requests for new suites:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=operations::responses_audio_and_batches_dispatch_through_the_public_server)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=peer::oracle_only_gateway_captures_through_the_peer_scribe)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::openai_compatible_streams_terminate_and_survive_a_capture_outage)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=resilience::a_cancelled_call_is_still_settled_and_the_server_keeps_serving)"'
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib
mise exec -- cargo nextest run --locked -p wyrd-skald-providers --lib
mise exec -- cargo nextest run --locked -p vala-verify --lib
mise run fmt:check
mise run lints
mise run check:deps
mise run codegen:check
mise exec -- git diff --check
```

The two planned server tests and direct-route regressions are the changed-server
owner checks. No SQL owner
changes are planned. Codegen/schema checking is regression-only: retain unchanged
public output, regenerate only through its owner if incidental generated output
is required, and investigate any semantic difference rather than accept churn.
Run Cargo commands sequentially and keep builds/tests off the benchmark host
during measurements. Integrated change review retains existing whole-change
journeys and aggregate obligations; do not rerun every language suite here.

### Primary performance proof: Gateway and Verification L1

Before editing production, record an immutable immediate baseline containing the
current TASK-004 production work. Planning HEAD above is a source reference,
not permission to assume a later dirty checkout equals that baseline. Record
revision/diff, toolchain, settings, effective resource/outbox geometry and payload
controls on both sides. No commits/worktrees are created by this planning task.

1. After focused checks, run paired profiles for both engines on the same quiet host, at least two
   matched intervals per side. Match allocation deltas to the same population of
   succeeded calls across warmup/measurement/drain. Attribute validation, native
   encoding/planning and direct-verification input/reader/comparison copies
   separately; do not sum inclusive parent stacks. Use focused observations for
   step-output and hash-mode changes absent from default Verification L1.
2. Run complete unprofiled Gateway AND Verification L1 on baseline and candidate,
   using all default steps and the driver's three repetitions. Compare sustained rate,
   CPU, p99 and sampled peak RSS. Apply AC-003's fresh back-to-back rerun rule
   to worse-outside-baseline-range medians. Zero errors, lost/shed captures and
   undrained work are mandatory. A valid record/zero exit alone is not a
   performance pass. Verification must preserve both expected verdict populations,
   zero refusals/lost writes and completed drain under AC-011. Apply its same
   baseline-range/fresh-pair rule independently. Neither engine can be omitted
   or substituted by the other's result. No improvement is asserted before measurements.
3. Preserve the original main comparison and pre-TASK-002 allocation/RSS baseline
   required by AC-003/010. Reuse compatible recorded evidence only when method,
   source and controls match. The final integrated candidate still owes the
   approved beyond-variation allocation/RSS reduction; an immediate-baseline
   throughput gain cannot waive it. If higher closed-loop throughput obscures
   memory comparison, use TASK-004's existing matched-load diagnostic policy.

Commands, each applied to the required baseline/candidate pair:

```sh
mise run bench:l1:gateway -- --allow-dirty --profile
mise run bench:l1:gateway -- --allow-dirty
mise run bench:l1:verification -- --allow-dirty --profile
mise run bench:l1:verification -- --allow-dirty
mise run bench:l1:gateway -- --allow-dirty --steps <regressed-step>
mise run bench:l1:verification -- --allow-dirty --steps <regressed-step>
mise run bench:l1:gateway -- --allow-dirty --steps buffered-64kib-capture-payload.c64 --pace <matched-rate>
```

The subset/rate commands are conditional diagnostics/reruns, not substitutes for
full default coverage. Keep raw records/profiles and explain required residual
scratch, final buffers, live heap, peak/post-drain RSS and outstanding/completed
work. Default L1 covers native compatible Chat, not embeddings/media/batches or
translation: focused owner observations prove removal there. Do not expand the
benchmark matrix to compensate. Full cross-engine L1 already belongs to TASK-004
and integrated verification; it is not an additional local task loop. Verification
L1's `direct-verify-c1`/`direct-verify-c64` use four scalar assertions over a 2 KiB
context with alternating passed/failed calls. They reach both full-input copies
and scalar comparison, but not full-root comparison, action-output duplication
or hash capture; the focused direct proof is mandatory for those cases.

## Material Stop Conditions

- Preserving behavior requires changed public owned fields/APIs, SDK/schema/wire
  semantics, a dependency/feature, a new contract owner or dependency direction:
  return `SPEC_REVISION_REQUIRED` with the precise conflict.
- A shortcut requires a separate handwritten validator, permissive decode
  fallback, changed input limits, skipped candidate checks, different first
  errors, capture loss/redaction, or retained per-candidate payload caches:
  do not take it; revise the implementation approach within these constraints.
- A required copy has distinct lifetime/encoding authority: retain and explain
  that copy rather than weaken ownership/accounting. If it prevents the required
  outcome, report the exact remaining boundary; do not claim measured acceptance.
- Failed correctness or L1 proof leaves implementation unverified. A performance
  miss is not permission to reduce coverage, retune geometry or reset baselines.

## Planning Evidence

2026-11-09: source research traced the six Gateway ingress callers, contract decoders,
candidate filtering, provider encoding, retries and terminal capture, plus direct
verification control, evidence reader, scope selection, comparison, step-output
settlement, context hashing and staging. The maintainer explicitly required both
implementations in TASK-005; spec revision 5 records that scope instruction. Named
selectors and test targets above were confirmed from their current owners and
`mise.toml`. A bounded independent reader reviewed the approved spec and existing
task packet together, found no material comprehension gap, and confirmed the
scope/ownership explanation; this was not implementation verification or task
approval. Local document links, whitespace and scenario structure passed. No
production code, tests or benchmarks were executed by this planning pass.

## Authority Links

- [Approved specification, revision 5](../spec.md)
- [Supporting direct verification copy audit](../direct-verification-copy-audit.md)
- [TASK-004 implementation and preserved evidence](TASK-004-ingress-allocation-and-bounded-capture-batches.md)
- [Repository rules](../../../../AGENTS.md)
- [Spec-driven development](../../../spec-driven-development.md)
- [Implementation execution](../../../implementation-execution.md)
- [Testing](../../../../TESTING.md)
- [Benchmark method](../../../../benchmarks/README.md)
- [Protocol doctrine](../../../../architecture/wyrd-design.md#doctrine)
- [Security principles](../../../../architecture/wyrd-security-posture.md#security-principles)
- [Rust ownership](../../../../agent_docs/rust.md)
