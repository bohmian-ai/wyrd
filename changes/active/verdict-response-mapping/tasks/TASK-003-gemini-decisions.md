---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-verdict-response-mapping
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014]
depends_on: [TASK-001, TASK-002]
---

# Use Gemini Decisions through the same verification workflow

## Outcome and dependency

An author configures a normal Google Decisions gateway deployment, authors
Prompt.gemini_decisions with shared native input and typed questions, and uses
the existing leaf or all policy to receive and inspect a normal verdict. The
native gateway route also accepts authorized direct Decisions requests, using
the same provider/auth/usage/error owners as Agent calls.

TASK-001 provides the typed question objects, leaf policies, shared response
facade and retained-result path; TASK-002 provides completed aggregation and
its SDK contract. Reuse these outputs against Google's own response variant.
This task owns Google's native input/request/response onboarding and its complete
consumer path, not a second verdict engine or a raw-body-only workaround.

Follow the [approved spec](../spec.md),
[Gemini scope](../design/multi-question-decisions.md#gemini-decisions-same-transforms-additional-native-integration),
[SDK/provider interfaces](../design/sdk-interfaces.md#gemini-onboarding-is-part-of-this-change),
and [typed question lowering](../design/question-models.md#provider-differences-and-ownership).
Use the official contract researched in the design to pin native fixtures;
new material contract differences require specification clarification, while
ordinary transport/library details remain implementation-owned.

## Existing owners and connected flow

Skald's skald-spec native wire/request/response/authoring/Prompt owners currently
model Google GenerateContent and Jev SystemOne, but not Decisions. Extend them
with Google's distinct native Decisions contract, `decisions` format, placeholder
discovery/rendering and supported native input/media forms. Extend the existing
skald-prompt builder/question lowering and the actual public SDK Prompt
constructors; do not reinterpret Google as Jev or translate raw native Jev maps.
New Python bindings stay in the Python SDK. Use Google's existing client/auth
and shared transport/retry owners, and the Agent fixed-answer call_once path in
`skald-agent/src/loop_runtime.rs`, to preserve the typed final_response without
streaming/tools or another normalized response model.

The gateway contract/deployment owners are
`wyrd-spec/src/gateway/{mod,deployment,native}.rs`; gateway execution is owned by
`wyrd-server/src/components/gateway`, including caller, ingress, routes and
capture. Extend the existing workflow with `decisions` operation/capability,
native `/v1/decisions` ingress and upstream Google `/v1beta/decisions` dispatch.
Apply existing provider compatibility, credentials, permissions, admission,
usage, error and capture owners; adding an operation does not authorize new
identity, authorization, tenancy, billing or retry semantics. In particular,
Google Prompt provider naming and the existing gateway Google adapter naming
retain their established meanings; introduce no aliases or second provider.

Input: public deployment configuration through existing administration, an
ordinary authored Google Prompt/Agent/Verifier graph and execution evidence.
Output: a real native Google request with bound input and keyed questions;
Google's decoded answer then feeds TASK-001/002's policies and existing recorded
result. Retain native usage and additional Noul confidence as evidence. Include
normal schema/codegen, all SDK exports/stubs/interfaces, gateway administrative
and HTTP/MCP/CLI projections, tested examples and docs within this outcome.

## Mandatory provider-pattern parity at review

REQ-004's provider conventions are a hard review requirement. Use
`skald-spec/src/wire/jev.rs` as the concrete type pattern: `wire_contract!`
produces owned native request/question/nested-criteria contracts and their
borrowed views; validation reads those views without copying request text.
Decisions must use that existing mechanism and typed native response/answer/usage
contracts, not an independently hand-written JSON validator or substitute
response envelope. Extend normal ProviderRequest/ProviderResponse dispatch and
reuse gateway RequestJson/body forwarding without forcing an owned tree or
serializing/reparsing intermediate provider bodies. Reuse shared transport,
client and retry owners as Jev and the other providers do, while keeping Google's
own authentication and wire semantics.

The implementation evidence must identify the existing owner reused for each
of these paths and the corresponding Decisions source. Review must examine
that source parity as well as the scenario and benchmark evidence; passing
functional journeys or aggregate benchmarks cannot excuse a divergent provider
implementation. This adds no new provider identity or generic adapter framework.

Provider-pattern parity also includes the spec's
[SDK/server allocation constraint](../spec.md#sdk-and-server-allocation-review).
Inspect the existing Prompt and Card authoring owners before adding Google
constructors, native input/question lowering or SDK views. Preserve shared
captures, lazy reference derivation and native views across construction,
attachment, validation, declaration and invocation. Do not introduce eager
Prompt/media/question-map copies or repeated Pydantic/dictionary/native
conversions to add Decisions. Record this source comparison and any needed
matched allocation measurements in scenarios 1–3 and 5, covering affected
managed/native SDK memory as well as the server path. The clean hot-path
benchmarks remain separate proof; they do not establish constructor allocation
costs by themselves.

## Ordered acceptance and implementation evidence

Implement each entry as a test-first cycle, recording assertion locations,
exact focused commands before RED, the observed missing behavior, GREEN and
material limitations in place. Reuse already-correct proof without fabricating
failures or introducing broader promises.

1. **A native Google Choice reaches the same verdict consumer** — REQ-001–006,
   REQ-012–013; J2. With normal gateway administration, configure a local mock
   Google deployment serving Decisions. Author the Google Prompt via the
   public Rust constructor using TASK-001's typed ChoiceQuestion and policy,
   declare and verify through the real SDK/server/gateway path. The mock sees
   upstream POST `/v1beta/decisions` with resolved model, bound top-level input
   and questions, using existing Google auth. Its native selected choice
   reaches the canonical SDK verdict, reason and capture. There is one request,
   no tool loop, no Verdict output schema and no caller-written native body.
   Initial RED is unsupported native format/operation, missing dispatch or
   absent Google response consumption. This is tier-1 Rust delivery proof;
   provider transport/auth tests supplement it. Inspect the same path against
   the mandatory provider-pattern section: the request has native owned/borrowed
   contracts and dispatch reaches the shared typed transport/response owners;
   a JSON-based parallel implementation cannot satisfy this entry.

2. **All SDKs retain Google question and input meaning** — REQ-001–003,
   REQ-011, REQ-013; J1/J2/J6. Repeat declaration/execution/read-back through
   the actual Python and TypeScript Prompt classes and typed question objects.
   Compare typed, mapping and mixed authoring; direct Python models need no
   model_dump, dictionary-only use needs no extra, and rebuilt declarations
   preserve native meaning. Shared NoulCriteria lowers to Google's
   true_description/false_description while the existing Jev path still emits
   true/false; omission stays omission. Choice labels and Score order persist.
   Native maps are validated as Google input, not silently translated from
   Jev. Check the provider's distinct question/criteria limits during
   construction/local validation/declaration, then correct and execute a valid
   graph. Invalid definitions make no upstream call. Initial RED is absent
   constructor/projection/lowering or reuse of Jev limits. Public SDK tier-1
   journeys plus owner/type tests establish the boundaries.

3. **Native content and media survive real binding** — REQ-003–004; J2/J4.
   Through an authored Google check, supply text and quoted/newline object
   context through normal input bindings, then native Content and Content-array
   inputs including media through the existing media evidence/binding workflow.
   Assert the mock receives correct Google native parts/media representation,
   not JSON-as-text in place of media. The native answer still reaches the same
   canonical verdict; no additional provider call or new media transport is
   introduced. Initial RED is lost placeholders, unsupported native input
   shape, incorrect media lowering or media erased at gateway dispatch. Use
   existing Skald native rendering tests and the real SDK/server media journey;
   every advertised native input form has connected proof, with representative
   media-binding assertions at each changed language boundary.

4. **Reuse leaf and aggregate policies on Google answers** — REQ-005–010,
   REQ-012; J2/J3/J4/J5. Feed Google's distinct native responses through the
   declarations/policies delivered by TASK-001 and TASK-002, including all
   canonical/custom Choice outcomes, both Noul polarities/bounds/middle,
   native fractional Score, and the all mixtures 0.02/0.03, 0.02/0.98 and
   0.02/0.5. A failed member dominates inconclusive; answer ordering and
   ancillary keys do not change selection. Reasons/answers and aggregate
   breakdown are retained and read through the ordinary recorded-result path
   with provenance and capture controls. Assert Google's extra Noul confidence
   and its usage shape survive the normal provider/gateway projections without
   changing authored outcomes. One invocation supplies all selected answers.
   Initial RED is wrong response-variant dispatch, lost native values or
   Google answers bypassing the shared policies. Tier-1 representative cases
   and native decoder/usage unit tests connect the actual consumers; do not
   repeat every arithmetic case already proved in TASK-001/002.

5. **The native gateway operation respects its existing boundaries** —
   REQ-004, REQ-013–014; J2/J7. Through real gateway configuration and its
   public native HTTP route, submit a valid authorized Decisions request and
   receive the native response intact. Use existing HTTP/MCP/CLI administration
   owners to configure/read the new capability rather than a provider-specific
   control path. A deployment missing capability, an incompatible provider,
   unsupported tools/generation options or a principal denied that operation
   refuses before upstream. Pin structured refusal fields/catalog code at
   the appropriate boundary. Use the existing tenant/provenance/capture paths
   to demonstrate a second tenant cannot inspect the resulting answers.
   Initial RED is a missing route/capability projection, bypassed compatibility
   or permission check, or loss of native fields. Gateway tier-1 journeys and
   focused served OpenAPI/runtime MCP checks establish this behavior; static
   permission checks alone are insufficient.

6. **Refusals and malformed output cannot become judgments** — REQ-004,
   REQ-009–012; J6/J7. Declare a valid Google check and have the native mock
   return a documented safety refusal, transient error, or missing/mistagged/
   undeclared/out-of-range selected answer. Preserve existing retry/error
   conventions and explicit check-validation errors, including Google's native
   Noul bounds and an unusable aggregate member beside a valid failed member.
   Assert nonfinite handling at the typed seam where not JSON-representable.
   Also submit an invalid locally available graph and an invalid graph resolved
   from registered references to prove local and authoritative validation at
   their specified boundaries. Failures never become a provider-authored
   failed claim or accepted pass; invalid declarations never reach upstream.
   Initial RED is missing native error mapping, contract validation or selected
   response checking. Use tier-1 negative flows and native owner tests.

7. **Existing providers and public contracts still work** — REQ-013–014; J8.
   Reuse Google GenerateContent, Jev native gateway, transformed Jev leaf/all
   and direct-Verdict regression journeys. Shared typed questions also build
   an ordinary Google Agent action without a verdict policy, retaining its
   existing action-output semantics. Served OpenAPI, schemas, runtime MCP and
   SDK exports/stubs include the operation/format consistently. Provide a
   journey-tested Google example and update the relevant provider/verification
   documentation. Existing behavior should already pass; new projection and
   constructor assertions fail initially where Decisions is absent.

## Test owners and focused verification

Use TASK-001/002's actual SDK story declarations and shared conversion/capture
assertions, with a distinct native Google mock response and request recorder.
Do not pretend Google support is established by deserializing Jev fixtures.
The SDK targets/setup recipes are in
[TASK-001](TASK-001-jev-verdict-checks.md#test-owners-and-focused-verification).

Native provider/render/usage unit tests belong in the existing Skald owners;
pure operation/deployment contracts in wyrd-spec; facade cases in Vala.
Gateway native delivery belongs in `wyrd-testing` target `gateway`
(`tests/gateway/{native,operations,...}.rs`), whose repository Postgres/setup
and journey selectors are owned by `test:gateway:native` and
`test:gateway:journey`. SDK gateway owners already have Jev and Google patterns.
CLI target `cli`, MCP target `mcp` and server integration/OpenAPI owners cover
their respective shipped boundaries. Reuse their existing setup and select
actual focused cases; record exact new commands after tests exist and before
RED. Do not add another test binary or treat gateway-admin proof as a substitute
for verifier execution through MCP/HTTP/CLI.

Alongside native contract tests, exercise equivalent valid and invalid compact
RequestJson input and existing decoded input through the Decisions ingress
validation seam. Both follow the same native validation rules and field errors;
borrowed validation preserves the forwarding body without materializing an
owned tree or copying its text. Assert this using existing provider-contract
and body-preservation test patterns; source inspection proves the ownership and
allocation path that JSON equality alone cannot establish. Record actual tests
and focused commands in scenario 5's evidence, not a second acceptance ledger.

Existing gateway lanes enumerate selected SDK modules/files explicitly. If
adding a Decisions story module/file, include it in those existing selectors so
`test:gateway:journey` actually runs the new proof; no new parallel lane is
needed. Full SDK lanes already collect their integration stories.

Finish affected Rust/Python/TypeScript format/lint/type checks, Python export
and stub checks, TypeScript N-API declarations, codegen:check, check:deps,
served/runtime contract checks and changed docs/examples. Apply the
[packet verification ownership](README.md#verification-ownership) for full
integrated SDK/gateway proof and mandatory hot-path inventory/clean benchmarks.
No schema migration, new dependency, per-call client/pool construction or new
hot-path wait is authorized by this onboarding.
