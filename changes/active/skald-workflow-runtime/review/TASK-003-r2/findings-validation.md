# Independent findings validation — TASK-003 r2

## Subject and evidence boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `2e6f13f505daf19050e588ea0d1d7fc936697e27`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12, including the human-approved 2026-10-03 addition of `pub model: ModelRef` to `WyrdGatewayCall`
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior review and remediation: `changes/active/skald-workflow-runtime/review/TASK-003-r1/`, including the human-approved same-spec correction of the native-`401` task wording

This fresh validation read the cumulative base-to-candidate diff, current source,
applicable repository and architecture authority, the complete prior r1 verdict,
ledger, and remediation, and all eight r2 discovery/follow-up reports. It traced
the changed producers through their callers and consumers. No build, compile,
test, Cargo, mise, pnpm, pytest, package-manager, formatter, linter, test-listing,
or other verification command was run. The implementer's recorded evidence is
accepted only for the exact source paths it exercises; missing, unclear, or
source-contradicted proof is retained as a finding.

The candidate identity remained unchanged during validation. The standing
DRIFT direction was applied throughout: no new mechanism, check, file, setting,
option, allowlist, dependency, or harness is required unless established Wyrd
authority or ordinary comparable practice already requires it.

## Disposition of every proposed finding

| Discovery source ID | Disposition | Final ID | Source-backed decision |
|---|---|---|---|
| `B-001` | **REJECTED** | — | Revision 12 requires the producer's exact JCS/base64url encoding and requires the receiver to decode, deserialize, bound, and semantically validate the value. It does not require receiver-side byte recanonicalization. Adding a serialize-and-compare rejection would be unapproved JSON strictness and DRIFT. |
| `INV-R2-001`, `FU-R2-001` | **REVISED** | `FIND-TASK-003-2` | The prior send-once finding reopens at the same native-`401` renewal invariant: a known `401` status bypasses renewal when body collection fails. |
| `STD-R2-001`, `MAINT-TASK-003-R2-1` | **CONFIRMED** | `FIND-TASK-003-6` | The reports identify the same mandatory bare-type/import-manifest violation. This is distinct from the closed r1 function-local-import finding. |
| `STD-R2-002` | **REVISED** | `FIND-TASK-003-7` | The missing `# Panics` documentation is confirmed. Validation narrows the correction to newly added panic-capable items and includes the new `FallbackIngress::model_ref` helper, whose call to the existing panicking model parser was omitted by discovery. |
| `STD-R2-003` | **CONFIRMED** | `FIND-TASK-003-8` | The new durable/async operations omit relevant cancellation and partial-progress contracts required by `AGENTS.md` §16. |
| `SEC-R2-001` | **REJECTED** | — | Revision 12 and TASK-003 expressly select OpenAI's `param` as the portable optional field. No approved grammar, bound, provenance signal, or allowlist distinguishes another value from that selected member. Dropping or constraining it is a security/public-contract decision, not a candidate defect. |
| `CONC-R2-001`, `FU-R2-002` | **CONFIRMED** | `FIND-TASK-003-9` | Source composes cancellation correctly, but the required named proof cancels only before dispatch and separately proves an in-flight timeout. Post-dispatch cancellation remains unproved. |

Agreement among discovery reports was not treated as proof. The retained ledger
below is independently grounded in current source and approved authority.

## Validated finding ledger

### FIND-TASK-003-2 — renew after every observed native `401`, including an unreadable body

- **Status:** REVISED (prior stable finding reopened)
- **Classification:** INCORRECT
- **Discovery sources:** `INV-R2-001`, `FU-R2-001`
- **Violated obligation:** The approved r1 outcome and corrected TASK-003 wording require one native model POST and, once its response is known to be `401`, renewal through the existing `AuthMiddleware::force_refresh` owner without replay. Renewal failure remains authoritative; successful renewal preserves the original refusal.
- **Exact location:** `crates/shared/wyrd-client/src/transport/http.rs:389-400`, especially body collection before the status branch at lines 393-395.
- **Producer-to-consumer evidence:** `request.send()` has completed and `response.status()` has produced `UNAUTHORIZED`. `response.bytes().await?` can then fail on a truncated body or connection reset after headers, returning `WyrdError::Internal` before `force_refresh`. The sole production caller, `PublicWyrdGatewayCaller::call` at `workflow/gateway.rs:94-116`, maps that error to `ProviderError::Connect`; the refused cached credential remains available to a later call. Existing focused cases use complete JSON bodies and cannot exercise this ordering. Raw `TcpListener` response fixtures are already ordinary repository practice in `crates/shared/wyrd-client/tests/transport/http.rs` and other client tests.
- **Observable consequence:** A renewable client can retain and reuse a refused token after receiving a real `401` whose body cannot be fully collected. The model call is not replayed, but the architecture-required reactive renewal does not occur.
- **Decision-complete minimum correction:** Keep the single POST and existing auth owner. Preserve the body-read result, attempt `force_refresh` whenever the already-known status is `401`, propagate a renewal failure, then return the original status/body when collection succeeded or the existing body-read transport error when it failed. Do not alter sibling JSON/framed/gRPC retry owners, replay the model request, add response provenance, or add configuration.
- **Focused closure proof:** Through the existing raw-HTTP fixture pattern, return `401` headers with an intentionally incomplete body and prove exactly one model POST, one reactive renewal, no resend, and the existing body-read/connection failure after successful renewal. Retain the complete-body renewal-success and renewal-failure cases and record the exact focused selector.

### FIND-TASK-003-6 — use imported bare type names in changed declarations

- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Discovery sources:** `STD-R2-001`, `MAINT-TASK-003-R2-1`
- **Violated obligation:** `architecture/agent-rules.md` requires types to be imported at module top and used by bare names in fields, parameters, return types, trait bounds, and `where` clauses. The module import block is the dependency manifest.
- **Exact locations:**
  - `crates/shared/wyrd-client/src/transport/http.rs:367-372` — `bytes::Bytes` in the new native-POST signature.
  - `crates/shared/wyrd-client/src/workflow/mod.rs:183` — `wyrd_loader::LoadedTree` in `load_bundle`'s return type.
  - `crates/shared/wyrd-client/tests/workflow_transport.rs:100,387` — `wiremock::Request` in new helper return types.
  - `crates/skald/skald-workflow/src/route.rs:286` — `skald_spec::Prompt` in `resolve_route`.
  - `crates/wyrd/wyrd-server/src/components/gateway/pg_invocation_tests.rs:5329,5538` — qualified `Response` and the typed `wiremock::Request` closure parameter.
  - `sdks/wyrd-sdk-python/src/workflow.rs:213,575-578` — the shared-client Workflow in a field, impl header, and parameter.
- **Evidence and consequence:** Every cited declaration is added by the cumulative diff. The qualified paths bypass the owning import blocks and make type ownership harder to read at precisely the Python boundary where the shared-client Workflow must remain distinct from Skald's Workflow.
- **Decision-complete minimum correction:** Import the types at their owning module tops and use bare, role-specific names. Where both Workflow types coexist, alias them by ownership, such as `SkaldWorkflow` and `ClientWorkflow`, and use those aliases consistently. Add no wrapper, checker, allowlist, feature, or runtime behavior.
- **Focused closure proof:** Static source review of the changed declarations plus the ordinary implementation format/lint evidence. No new runtime test or repository check is warranted.

### FIND-TASK-003-7 — document the panic contracts of new Rust tests and helpers

- **Status:** REVISED
- **Classification:** VIOLATION
- **Discovery source:** `STD-R2-002`
- **Violated obligation:** `AGENTS.md` §16 and `architecture/agent-rules.md` require substantive rustdoc for every new or materially modified Rust item, including test helpers and test functions, and a `# Panics` section whenever a panic remains possible.
- **Exact locations and evidence:**
  - The new `crates/shared/wyrd-client/tests/workflow_transport.rs` omits `# Panics` from panic-capable helpers `client`, `run_id`, `create_request`, `received`, `chat_request`, `gateway_call`, `fallback`, `api_key_client`, and `model_posts`, and from both assertion-bearing test functions. `catalog_problem` and `problem` already show the required local form.
  - `crates/wyrd-spec/src/gateway/policy.rs:621-656` adds `fallback_header_round_trips_and_refuses` with `expect` and assertions but no `# Panics` section.
  - `crates/wyrd/wyrd-server/src/components/gateway/pg_invocation_tests.rs:5318-5321` adds `FallbackIngress::model_ref`, which reaches the existing panicking model parser, and lines 5416-5427 add `fallback_headers` with two `expect` calls; neither documents the invariant. The owning test already documents its panic contract.
- **Observable consequence:** Fixture and serialization invariants that can abort these new tests remain outside the owning item documentation, violating the repository's hard documentation gate and making future wire-contract edits less safe.
- **Decision-complete minimum correction:** Add concise `# Panics` sections to the identified items, naming only their actual fixed-fixture, parsing, serialization, request-recording, or assertion invariants. Do not introduce a harness, lint, checker, allowlist, or production refactor. Items with no real panic path do not need boilerplate.
- **Focused closure proof:** Static inspection that every newly added panic-capable item has substantive panic documentation, followed only by the ordinary implementation documentation/lint evidence. No new behavioral test is required.

### FIND-TASK-003-8 — document cancellation and partial progress on new durable operations

- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Discovery source:** `STD-R2-003`
- **Violated obligation:** `AGENTS.md` §16 requires async and durable operations to document cancellation, partial progress, and retry behavior when relevant.
- **Exact locations:** `crates/shared/wyrd-client/src/workflow/remote.rs:41-59,72-87` and `crates/shared/wyrd-client/src/transport/http.rs:346-400`.
- **Source evidence:** `Workflows::create` documents stable idempotent retry but not that dropping the future after dispatch can leave an accepted run whose response was not observed. `Workflows::cancel` does not state that the server may apply cancellation even when the caller drops or loses the response. `post_native` documents send-once and reactive renewal, but not that the caller's timeout/cancellation drops local IO after the gateway may already have accepted or dispatched the model call. `Workflows::wait` in the same new module demonstrates the required explicit drop contract.
- **Observable consequence:** Rust callers and maintainers cannot tell from the owning APIs whether local future cancellation rolls back server work, a material ambiguity for run creation, run cancellation, and non-idempotent model dispatch.
- **Decision-complete minimum correction:** Extend the existing rustdoc on `create`, `cancel`, and `post_native` with their actual post-dispatch cancellation and partial-progress boundaries. Preserve current idempotency, send-once, renewal, and server ownership. Add no cancellation protocol, option, marker, setting, or retry mechanism.
- **Focused closure proof:** Static source review plus ordinary implementation documentation/lint evidence. Runtime behavior is unchanged, so a new test is not warranted for this finding.

### FIND-TASK-003-9 — directly prove cancellation after public-gateway dispatch

- **Status:** CONFIRMED
- **Classification:** MISSING
- **Discovery sources:** `CONC-R2-001`, `FU-R2-002`
- **Violated obligation:** TASK-003 Scenario 2 assigns `public_gateway_call_context_and_errors` direct proof that cancellation drops caller IO. AC-011A and AC-020 separately require public-caller cancellation evidence; source plausibility cannot replace the mapped proof.
- **Exact location:** `crates/shared/wyrd-client/tests/workflow_transport.rs:564-613`; implementation at `crates/shared/wyrd-client/src/workflow/gateway.rs:94-116`.
- **Producer-to-consumer evidence:** The production caller races the complete authenticated `post_native` future against `cancellation.cancelled()` and a timeout in one `tokio::select!`, so no production defect is established. The test cancels its token before constructing/polling the call at lines 578-587, proving only pre-dispatch refusal. Its delayed request at lines 589-612 terminates through the call timeout, not cancellation. The recorded GREEN claim therefore does not prove cancellation of already-pending IO.
- **Observable consequence:** Required evidence does not establish that a dispatched public-gateway request stops promptly on Workflow cancellation without resend, independently of its deadline.
- **Decision-complete minimum correction:** Extend the existing focused test using its existing client, `CancellationToken`, and deterministic local HTTP/mock pattern. Hold a response pending, establish that the model POST reached the boundary, cancel that call's token, and assert the existing cancellation/timeout category returns promptly with no resend. Do not add a production cancellation mechanism, generalized harness, sleep-based host-load test, setting, option, checker, or transport.
- **Focused closure proof:** Record the exact existing `public_gateway_call_context_and_errors` selector after adding the post-dispatch cancellation case. Its synchronization must distinguish cancellation from timeout and prove one model POST.

## Rejected proposals and correction alternatives

### `B-001` — receiver-side JCS byte equality

`GatewayFallbackOverride::to_header_value` already uses `serde_jcs` and
unpadded base64url; its test asserts the exact bytes. The receiver at
`policy.rs:142-172` enforces encoded and decoded size limits, unpadded
base64url, typed `deny_unknown_fields` deserialization, and semantic model
validation. Revision 12 says the ingress “decodes, deserializes, and validates”
and names malformed, oversized, empty, duplicate, and self-referential values;
it does not require rejecting insignificant JSON whitespace or member order.
Where Wyrd requires canonical input bytes, `gateway/record.rs:344-369` says so
explicitly and compares the serialization. Ordinary JSON receivers accept
equivalent JSON unless signatures, hashes, or explicit canonical-input
authority say otherwise. A second serialize-and-compare guard and its proposed
negative test are therefore rejected as DRIFT.

### `SEC-R2-001` — constrain or omit OpenAI `error.param`

The source provenance concern is real: a relayed native OpenAI-compatible
provider controls `error.param`, and `Ingress::problem` projects it into
`RemoteProblem.field`. But Revision 12 explicitly selects “OpenAI's safe
`param` as `field` when present,” TASK-003 repeats that contract, and the r1
approved remediation deliberately preserves the permitted field while replacing
message text with catalog metadata. No authority defines a syntax, maximum,
fixed set, or provenance marker that this client could apply. Requiring an
allowlist/validator would add unsupported machinery; omitting the field would
contradict the approved public contract. If the meaning of “safe `param`” is to
change, that requires `$wyrd-spec`, not TASK-003 remediation. The proposal is
rejected from this implementation ledger.

## Prior finding closure

| Prior finding | Validation result |
|---|---|
| `FIND-TASK-003-1` | **CLOSED.** Python stores `wyrd_client::Workflow`; registered and authored-ref loaders preserve it; authoring methods mutate only `as_skald_mut`; `run` delegates to the retained owner. The recorded public Python journey covers explicit server A, ambient B/absence, successful and refused edits, and client-less local refusal. |
| `FIND-TASK-003-2` | **REOPENED / REVISED.** Duplicate model replay is removed, complete-body `401` renewal and renewal-failure propagation are implemented, but unreadable-body `401` responses bypass renewal as described above. |
| `FIND-TASK-003-3` | **CLOSED.** Recognized-code messages/remediation come from the derive-backed catalog for all three dialects; provider-controlled message text is discarded. `SEC-R2-001` is a separate rejected contract-change proposal. |
| `FIND-TASK-003-4` | **CLOSED.** `read_secret_file` is private and all cross-module consumers receive `SecretString` through `read_secret_ref`, with the established file checks preserved. |
| `FIND-TASK-003-5` | **CLOSED.** The three r1 function-local imports moved to their owning module/test-module blocks with cfg preserved. `FIND-TASK-003-6` concerns separate qualified declaration types and therefore receives the next unused stable ID rather than reusing this closed finding. |

## Complexity and correction assessment

The new `Workflows`, `PublicWyrdGatewayCaller`, `SelectedRoutes`, fallback DTO
codec, and shared secret reader have real owners and callers and reuse existing
transport, auth, codec, runtime, and redaction mechanisms. No unsupported
abstraction, option, feature, compatibility layer, or permanent check survives
validation. The five retained findings close through ordering in the existing
transport, direct proof in the existing focused test, import aliases, and
documentation. None requires a new product, public API, architecture, security,
compatibility, cross-service, concurrency-semantics, resource-ownership, or
persistent-data decision.

## Validation result

The independently validated ledger contains five bounded findings:
`FIND-TASK-003-2`, `FIND-TASK-003-6`, `FIND-TASK-003-7`,
`FIND-TASK-003-8`, and `FIND-TASK-003-9`. The other proposed findings are
rejected, not softened into optional advice. The candidate therefore requires
bounded implementation remediation and recorded focused proof before TASK-003
can pass.
