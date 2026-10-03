# Repository Standards Review — TASK-003

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `a1792c45323489157e818eb29722913114014927`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Approved amendment: the 2026-10-03 human approval adding `pub model: ModelRef` to `WyrdGatewayCall` is authoritative and is not treated as drift.
- Candidate identity was rechecked after inspection and remained `a1792c45323489157e818eb29722913114014927`.

## Authority coverage

| Changed surface | Applicable authority read | Coverage and evidence | Result |
|---|---|---|---|
| Shared Workflow client handles, local execution composition, transport-private native call path, config, and Rust SDK projection | `AGENTS.md` §§2–6, 9, 11–12; `architecture/agent-rules.md`; `architecture/wyrd-design.md` Client model and Workflow; `wyrd-doctrine.mdx`; `references/doctrine/architecture-constraints.md`; `references/architecture/patterns.md`; `references/languages/rust-core.md`; `maintainer-style.md`; `errors.md`; approved spec/task | Read the complete base-to-candidate diff and the full changed owners in `wyrd-client/src/workflow/{mod,gateway,local,remote}.rs`, `transport/http.rs`, config, SDK exports, and transport tests. The stateful capabilities have concrete owners and reuse `WyrdClient`; no second HTTP client or public arbitrary-header API was added. RR-001 remains on native refusal trust. | FAIL |
| Gateway fallback wire contract and validation in `wyrd-spec` | `AGENTS.md` §§2–4, 9; `wyrd-design.md` public-surface and contract rules; doctrine; architecture constraints; patterns; errors; agent harness; approved spec/task | `GatewayFallbackOverride` remains pure, synchronous, typed, schema-capable, and PyO3/IO-free. The header contract is centralized in `wyrd-spec`; generated artifacts were not hand-edited. | PASS |
| Authenticated OpenAI/Anthropic/Gemini ingress and served OpenAPI | `AGENTS.md` §§2, 6, 9, 11; `agent-rules.md`; `wyrd-design.md`; `wyrd-security-posture.md`; patterns Server/Provider/Audit patterns; agent harness; errors; testing workflows | Traced middleware authentication to `requested_fallback`, then `GatewayCallRequest.fallback`, and inspected OpenAPI registration/proof. Header interpretation occurs only in authenticated handlers and is not forwarded. No new ingress or Vertex endpoint was added. RR-001 concerns the existing provider-refusal relay combined with the new client normalizer. | FAIL |
| Shared environment/file secret reader and gateway/server/client consumers | `AGENTS.md` §§2–4, 6; `agent-rules.md`; security posture Secret handling; rust-core Secrets, visibility, async, and import rules; patterns ownership/dependency-cost rule | Inspected `wyrd-utils/src/secret.rs`, gateway credential resolution, server key loading, and local Workflow binding resolution. One common open-handle reader is a repository-native reuse, not standing-direction drift. RR-002 and RR-003 remain. | FAIL |
| Skald Workflow route planning and approved `WyrdGatewayCall.model` amendment | `AGENTS.md` §§2–6; `wyrd-design.md` Workflow; doctrine Runtime/Service boundaries; architecture constraints; patterns Provider Runtime; rust-core; approved amendment | Traced Prompt provider/model through `ExecutionPlan::build`, `resolve_route`, `StepRoute`, and the per-attempt caller. The added model field is the explicitly approved minimal amendment and stays immutable per call. Skald retains no reverse dependency on `wyrd-client`. | PASS |
| Python local-run projection | `AGENTS.md` §§7–8, 11; PyO3 boundaries; Python API and stubs; testing workflows; doctrine Client model | The existing Python wrapper now delegates local execution to `wyrd_client::Workflow::run`, uses the shared runtime bridge with `py.detach`, and introduces no Python lifecycle or transport implementation. No generated stub was hand-edited. | PASS |
| Tests, manifests, lockfile, verification evidence, and task/spec packet | `AGENTS.md` §§1, 11–12; `agent-rules.md`; spec-driven development; implementation execution; testing workflows; maintainer style | Reviewed the external HTTP transport test, inline owner tests, PG ingress test, OpenAPI test, manifests, lockfile, and the recorded successful focused/family/boundary/codegen/language checks. The external `workflow_transport.rs` earns its test binary by driving an HTTP surface. `git diff --check base..candidate` passed during this review. RR-003 is a source-layout failure despite green checks. | FAIL |

## Applicable rule results

| Authority rule | Source evidence | Result |
|---|---|---|
| Shared SDK behavior belongs in `wyrd-client`; language SDKs project it without separate transport or durable semantics. | `wyrd-client::Workflows`, `Workflow`, and `PublicWyrdGatewayCaller` own the Rust implementation; `wyrd-sdk-rust` re-exports it and Python delegates to it. | PASS |
| Stateful and dependency-backed workflows use cohesive owning structs and inherent methods. | `Workflows` owns `WyrdClient`; `PublicWyrdGatewayCaller` owns `WyrdClient`; `Workflow` owns the hydrated Skald workflow plus optional loading client; route preparation is on `SelectedRoutes`. | PASS |
| Async is limited to real IO or intentional composition; blocking filesystem work leaves Tokio workers. | Remote HTTP/polling and workflow execution await IO; authored loading and secret reads use `spawn_blocking`; pure fallback encoding/validation remains synchronous. | PASS |
| `wyrd-spec` stays pure, synchronous, IO-free, and PyO3-free. | The fallback header DTO/codec adds only serde/JCS/base64 contract work in `crates/wyrd-spec/src/gateway/policy.rs`. | PASS |
| Public errors use stable catalog metadata and must not expose raw provider, credential, prompt, or internal error strings. | `gateway.rs::Ingress::problem` trusts any native envelope whose provider-controlled `code` text happens to equal a `WyrdError` code and preserves its raw `message`; gateway native refusal relay preserves provider JSON. See RR-001. | FAIL |
| Secret-bearing Rust values use `SecretString` (or equivalent redacted wrappers), and public visibility is intentional. | `read_secret_ref` returns `SecretString`, but new public `read_secret_file` returns plaintext `String` and has no caller outside its own module. See RR-002. | FAIL |
| All `use` statements live at module scope, except the documented narrow generic-trait exception. | Three added imports occur inside ordinary functions/tests. See RR-003. | FAIL |
| Authentication and tenant authority remain server-owned; request fields/headers do not select tenant. | Existing gateway middleware authenticates before handlers; fallback carries only typed model candidates and never tenant/principal identity. | PASS |
| Gateway invocation authorization/audit behavior is not bypassed or duplicated. | All affected ingress paths still converge on `GatewayInvocation::invoke`; the change only populates typed `fallback`. | PASS |
| Public gateway headers are consumed at ingress and never forwarded to providers. | `requested_fallback` reads `HeaderMap`; only the decoded override enters `GatewayCallRequest`; provider requests are assembled from typed bodies. PG proof asserts non-forwarding. | PASS |
| Secrets are resolved only when the selected route executes and remain absent from Cards/config values/logging. | `SelectedRoutes` selects names first; `resolve_binding` reads only selected `SecretRef`s at run time into `SecretString`; loading does not call it. | PASS |
| Server/client/provider ownership and dependency direction remain intact. | `wyrd-client` depends on Skald for the adapter; Skald has no reverse client/server edge; `wyrd-server` remains the only serving surface. | PASS |
| First-class Rust remains Python-free; PyO3 behavior stays in the Python SDK and uses the shared runtime bridge. | No Python feature was enabled in Rust SDK/client manifests; `sdks/wyrd-sdk-python/src/workflow.rs` is the only changed PyO3 boundary. Recorded `check:pyo3-scope` and SDK checks passed. | PASS |
| Generated schemas/stubs/OpenAPI are proved from owners, not hand-edited. | No generated file changed. Recorded `codegen:check` passed; `pg_openapi_contract.rs` tests the served header declaration. | PASS |
| New external tests must wire multiple crates or drive a real HTTP/server/external-service surface. | `crates/shared/wyrd-client/tests/workflow_transport.rs` drives the real shared HTTP transport against deterministic HTTP servers. | PASS |
| Every new or materially modified Rust item has substantive rustdoc, including private helpers/tests and fallible error behavior. | Changed production types/helpers and added test helpers/tests are documented with relevant errors/panics/cancellation where applicable. | PASS |
| No gate weakening, ignored test, lint allowance, compatibility alias, legacy vocabulary, ad-hoc runtime, public arbitrary-header API, or new model ingress. | Diff contains none of these. `post_native` remains crate-private and uses the existing pooled transport/auth owner. | PASS |
| Added machinery must be established repository/native/standard practice or justified against existing owners; otherwise it is DRIFT. | The client facade, narrow two-implementation gateway trait, typed header DTO, shared secret reader, `spawn_blocking` boundary, and served OpenAPI parameter all extend existing owners and common Rust/HTTP patterns required by the approved task. No separate checker, cache, parser, transport, polling option, or lifecycle owner was added. The unused public plaintext helper surface in RR-002 is the only unjustified widening. | FAIL |

## Material findings

### RR-001 — VIOLATION: provider-controlled messages can enter the supposedly redacted Workflow error contract

- Governing rules: `architecture/wyrd-security-posture.md` (secrets are never errors/logs/traces; uncertainty at security boundaries fails closed); `architecture/references/languages/errors.md` (do not leak raw provider error strings across public boundaries); approved Revision 12 and TASK-003 (no arbitrary upstream text, prompt, credential, or protocol-only detail survives `RemoteProblem`/`WorkflowRunError`).
- Changed location: `crates/shared/wyrd-client/src/workflow/gateway.rs:161-207`, especially lines 190-197; interacting existing source: `crates/wyrd/wyrd-gateway/src/adapter/mod.rs:258-308`.
- Evidence: native provider refusals are deliberately relayed as the provider's remaining JSON at `adapter/mod.rs:297-299`. The new client parser accepts the envelope's raw `message` whenever its `code` string is present in `WyrdError::codes()`. A provider can therefore return, for example, an OpenAI error envelope with `code = "WYRD_GATEWAY_400_INVALID_REQUEST"` and `message = "echoed prompt ..."`; the new client classifies it as a trusted Wyrd problem and copies that message into `RemoteProblem`. The added test covers an uncoded/provider-specific canary (`workflow_transport.rs:664-690`) but not a provider refusal spoofing a real Wyrd code.
- Consequence: provider-controlled prompt or other sensitive upstream text can cross the redaction boundary into a portable Workflow error and terminal snapshot while presenting as a stable Wyrd error. This contradicts both the public error rule and the candidate's documented `RemoteProblem` safety invariant.
- Testable correction: at the existing gateway refusal owner, ensure provider-originated native envelopes cannot retain a Wyrd-owned code that makes their message appear gateway-authored (without changing gateway-originated errors), then add a native OpenAI/Anthropic/Gemini refusal case whose provider body uses a real Wyrd code and a canary message and prove the public caller returns only the safe category/catalog projection with no canary. Do not add a new error catalog, transport, response header, or option.

### RR-002 — VIOLATION/DRIFT: the shared utility publishes plaintext secret-file contents as `String`

- Governing rules: `AGENTS.md` §4 and `architecture/references/languages/rust-core.md` Secrets (secret-bearing values use `secrecy::SecretString`); rust-core Modules and visibility (`pub(crate)` by default and `pub` only for intentional public surface); the human standing direction against mechanisms or options with no established need.
- Changed location: `crates/shared/wyrd-utils/src/secret.rs:39-59`.
- Evidence: `pub fn read_secret_file` returns `Result<String, _>` even though the file is explicitly a secret. Repository search shows its only call is the adjacent `read_secret_ref`, which immediately converts the returned `String` to `SecretString`; no other crate needs or uses the public plaintext helper.
- Consequence: the candidate unnecessarily widens the shared crate's API with a reusable plaintext-secret escape hatch and makes accidental `Debug`, cloning, or error interpolation possible outside the redacted owner. The safe public API already exists as `read_secret_ref -> SecretString`.
- Testable correction: keep the file-reading stage private to `wyrd-utils::secret` (or have it return `SecretString`) so the only cross-module result remains redacted, preserving the existing open-handle, permissions, size, UTF-8, and error behavior. Add no new secret abstraction or check.

### RR-003 — VIOLATION: new imports are hidden inside ordinary functions

- Governing rule: `architecture/agent-rules.md` requires every `use` at the top of its module; its only function-local exception is the rare `use TraitName as _` inside a single generic function where module scope is inappropriate. Tests are allowed their own module scope, not arbitrary function-local dependency blocks.
- Changed locations:
  - `crates/shared/wyrd-utils/src/secret.rs:66` — `PermissionsExt` inside non-generic production helper `restrictive`;
  - `crates/shared/wyrd-client/src/workflow/mod.rs:472` — `PermissionsExt` inside test helper `secret_file`;
  - `crates/wyrd-spec/src/gateway/policy.rs:622-623` — base64 imports inside `fallback_header_round_trips_and_refuses`.
- Evidence: all three were added in the candidate and none qualifies for the documented generic-function exception. The same candidate already demonstrates the required test-module pattern in `wyrd-gateway/src/credential.rs`, where `PermissionsExt` is imported at the test module top.
- Consequence: module dependency surfaces are incomplete at their declared import blocks, directly violating a hard repository readability rule.
- Testable correction: move each import to the top of its owning module under the existing `#[cfg(unix)]` where necessary; behavior and tests remain unchanged.

## Verification assessment

- Reviewed the task's recorded green evidence: focused Workflow transport, public gateway, Skald route, PG ingress, and served OpenAPI tests; shared/gateway/server families; Rust SDK; Python and TypeScript unit/type/declaration lanes; codegen; client-tier, SDK-tier, and PyO3 boundary checks; format, lints, and diff check.
- Independently ran `git diff --check 58d07d7260df1f022a721e720a28ea48e5096e35..a1792c45323489157e818eb29722913114014927`: PASS.
- No Cargo-backed lane was rerun in this reviewer because the immutable task evidence already records the required successful commands and repository rules prohibit overlapping Cargo work in a shared checkout. Green gates do not close RR-001's missing adversarial path or the source-level hard-rule violations RR-002/RR-003.
- The review found no unavailable authority and no incomplete changed-surface coverage.

## Overall result

**FAIL**

The candidate otherwise follows the required client/server ownership, struct-centered Rust, async, PyO3, contract, authentication, and test-tier patterns, and the approved `WyrdGatewayCall.model` amendment is correctly authoritative. RR-001, RR-002, and RR-003 are material repository-rule violations that require correction before PASS.
