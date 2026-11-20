---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-verdict-response-mapping
spec_revision: 1
requirements: [REQ-002, REQ-003, REQ-005, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014]
depends_on: [TASK-001]
---

# Ask multiple questions and receive one explained verdict

## Outcome and dependency

An author selects several questions from one native response and receives one
requirement verdict with a retained per-question explanation. Deliver flat
AllTransform across Rust, Python and TypeScript, local/server validation,
execution, capture and existing public contracts. No cross-check batching,
nested composites or dependent-question execution is included.

TASK-001 supplies the actual Jev Prompt/question constructors, shared native
response handle, leaf policies, assembled graph validation and result recording.
Consume these outputs directly: AllTransform dispatches those same leaves over
one response; it does not introduce another decoder, evaluator or provider call.
Google executes the completed policies in TASK-003. Governing inputs are the
[approved spec](../spec.md), the design's
[one-request aggregation](../design/multi-question-decisions.md#one-request-several-questions-one-check-verdict),
[SDK transform projections](../design/sdk-interfaces.md#typed-verdict_transform-projections),
and locked journeys J4–J7.

## Owners, input and output

Extend TASK-001's canonical transform union/check wrapper and shared validation
in wyrd-spec and graph authoring owners. Project AllTransform through existing
SDK exports, Python optional Pydantic/coercion/stubs, TypeScript interfaces and
canonical mappings. A policy on the versioned Verifier selects explicit native
question keys; ordinary check inputs and existing Jev state binding supply the
shared context to the configured gateway deployment.

Vala's concrete VerdictTransformer consumes the same preserved typed response
and the resolved questions used by TASK-001. Reuse its leaf outcomes/reasons;
validate every selected answer before concluding. Return one CheckOutput,
with the existing actual capture holding selected answers and question-level
verdicts/reasons, and an overall factual reason identifying question outcomes.
The existing engine, capture controls and Bifrost writer/read path consume it.
Do not create independently addressable requirements or repurpose Verdict.data.
Include generated/public projections and extend the delivered example/docs as
part of this outcome. Private module/helper choices remain implementation-owned.

## Ordered acceptance and implementation evidence

Append assertion locations, actual focused commands, observed RED/GREEN and
limitations to these entries as implemented; keep their expected behavior fixed.

1. **Two questions, one passing check** — REQ-002–003, REQ-005, REQ-009,
   REQ-012–013; J4. Starting with TASK-001's configured Jev deployment and
   public constructors, author one check with two Noul questions, expected
   false policies and thresholds 0.9/0.1. Supply real object context containing
   answer/policy and quoted/newline text. Through the real SDK/server/gateway
   path, a mock response of 0.02/0.03 yields one passed requirement. The mock
   observes one native request with complete JSON-as-text state and both
   questions; the result explains both answers. First add the Rust journey,
   then exercise the same boundary through Python and TypeScript. Initial RED
   is absent AllTransform acceptance/execution or loss of its selected keys,
   rather than a fabricated multi-call input. Tier-1 SDK journeys establish the
   consumer path, supplemented by pure aggregation tests.

2. **Mixed outcomes follow policy, not response order** — REQ-009–010,
   REQ-012; J4. Reuse that authored check with 0.02/0.98 producing failed,
   0.02/0.5 producing inconclusive, and a failed member alongside an
   inconclusive member producing failed. Mix Choice/probability/Score leaves
   from TASK-001 and change answer-map order while adding an unselected
   ancillary answer. The selected keys determine the same overall verdict and
   per-question reasons; no extra invocation is made. Initial RED is absent
   aggregation/precedence or order-dependent selection. Tier-1 representative
   cases accompany focused Vala tests for combinations; no SDK-only policy
   implementation is introduced.

3. **Correct an invalid aggregate while authoring** — REQ-005, REQ-009,
   REQ-011, REQ-013; J6. Construct an empty typed AllTransform and observe the
   self-contained error. In the locally assembled real Prompt/Agent/Verifier
   graph, validate duplicate selected keys, missing questions, incompatible
   leaves and nested composition. Refuse at the appropriate typed-value or
   graph boundary with field/remediation; incremental editing still works.
   Canonical mappings and server declaration cannot bypass these rules.
   Correct the definition, validate, declare and execute it successfully.
   Python accepts AllTransform directly without model_dump and dictionary-only
   use still works without Pydantic; Rust and TypeScript expose the same union
   and wire semantics. Round-trip the aggregate graph through public read-back
   and redeclaration. No invalid definition reaches upstream. Initial RED is
   absent structural/cross-object validation or a policy dropped in projection.
   Use public language authoring tests plus tier-1 corrected/declaration flows.

4. **A failed member cannot conceal an unusable answer** — REQ-009–010;
   J7. Declare a valid aggregate and script a valid failed selected answer
   alongside another selected answer that is missing, mistagged, undeclared
   or numerically invalid. The result follows the explicit existing validation
   error path for unusable output instead of accepting an aggregate judgment.
   Reorder these answers to show that a first failure cannot short-circuit
   validation. Initial RED is missing full selected-answer validation or early
   return after failure. This is a tier-1 negative with focused typed/Vala
   cases where the wire cannot encode a nonfinite number.

5. **Read the retained question breakdown** — REQ-005, REQ-012–014; J5/J8.
   Execute an aggregate, then use existing flush/wait/read controls and public
   SDK result inspection after recording becomes visible. Assert selected
   native answers/probabilities and each question's verdict/reason agree with
   the one overall verdict and exact version/policy/provenance. Reuse the
   existing full versus hash/redacted capture and foreign-tenant read paths
   against this aggregate content; raw answers do not escape configured
   capture or permission controls. Include representative shipped HTTP/MCP/CLI
   aggregate declaration/execution/read-back proof. Initial RED is a missing
   recorded breakdown or dropped policy; response-handle-only capture cannot
   pass. Existing leaf and direct-Verdict regression proof must continue to
   pass. This is tier-1 retained-result proof, not an in-memory-only test.

## Test owners and verification

Extend TASK-001's actual SDK/native-mock and result-inspection story owners;
the input to these tests must be its real authored declarations and leaf
policies. Rust uses `wyrd-ai` target `integration` with the journey profile;
Python uses integration-marked pytest stories; TypeScript uses the public
Vitest integration stories. Their repository Postgres/setup recipes and
HTTP/MCP/CLI owners are described in
[TASK-001](TASK-001-jev-verdict-checks.md#test-owners-and-focused-verification).
Register new modules under existing test targets and keep matching story
semantics across the SDKs. Record exact focused commands once the added tests
exist and before RED; this task names no new executable test selectors.

Put exhaustive pure composition/validation cases in the owning wyrd-spec and
Vala modules; keep language constructor/coercion/lifetime tests in that language.
Use the affected contract/codegen, export/stub/type and N-API checks; Rust and
touched Python/TypeScript format/lints; and the changed docs/example checks.
Full integrated SDK/gateway lanes and the applicable sanctioned-wait inventory
and clean benchmark obligations remain with
[packet verification ownership](README.md#verification-ownership). Do not add a
new runner, store, IO wait or model call to make composition work.
