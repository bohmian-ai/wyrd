# SDK interfaces and decision-provider onboarding

Status: finalized; maintainer locked this revision on 2026-11-18.

## Explicit maintainer directions

- Answer whether Wyrd actually sends Jev's top-level state field today.
- Make the actual Prompt class the normal authoring path for decision providers.
- Expose declarable Rust transform types, Python Pydantic models, and TypeScript
  interfaces in addition to ordinary mappings/dictionaries.
- Onboard Gemini Decisions in this change, through the existing provider
  conventions, not as a future adapter or raw-request workaround.

## Current wire behavior: state is already present

Yes. `crates/skald/skald-spec/src/wire/jev.rs::JevRequest` has required model,
state and questions fields. `JevClient::send_native` passes that typed struct
to send_json_with_retry at `/v1/systemone`. The outbound JSON is the struct's
body, not a provider/body wrapper and not chat messages. The same native request
travels through the gateway system_one operation; its adapter may replace model
with the deployment's upstream model, while retaining state/questions.

The existing test
`crates/skald/skald-providers/src/clients/jev.rs::tests::systemone_posts_bearer_body_and_decodes_the_answer`
matches the exact JSON body, endpoint and bearer auth. Its fixture includes
`{"model":"jev-latest","state":"s","questions":{"q":{"type":"noul","instructions":"x"}}}`.
Source inspected; test not executed during design.

For dynamic state the existing binder resolves check.inputs, renders strings
verbatim/other JSON as text, and binds `${context}` into JevRequest.state.
No new state field or transport path is needed. The gap is the ergonomic Prompt
constructor and conversion of the fixed response into a check verdict.

## Concrete Prompt constructors

Extend the existing Prompt class and existing provider-shaped builder owner.
Each constructor returns the ordinary Prompt type, whose authored form remains
the native provider/format/body contract already used for declaration and hashing.
No new durable portable decision format or separate DecisionPrompt class.

| Surface | Jev | Gemini Decisions |
|---|---|---|
| Python | `Prompt.jev(model, *, state, questions)` | `Prompt.gemini_decisions(model, *, input, questions)` |
| TypeScript | `Prompt.jev(model, {state, questions})` | `Prompt.geminiDecisions(model, {input, questions})` |
| Rust | `Prompt::jev(model, JevPromptOptions { state, questions })` | `Prompt::gemini_decisions(model, GeminiDecisionsOptions { input, questions })` |

Rust options reuse owner-native question/input types and existing re-exports.
Keep the existing Rust builder module as the construction owner; inherent
entry points delegate there rather than repeat request lowering. Python and
TypeScript use their SDK boundary owners and the shared Rust path. Do not put
new PyO3 code in skald-prompt; it belongs in wyrd-sdk-python under repo rules.
The new constructors follow existing Prompt.openai_chat/Prompt.gemini and
Prompt.openaiChat/Prompt.gemini patterns. They set native protocol/model fields
and validate through the same owner rather than inventing a parallel compiler.

Question maps remain natural dictionaries/typed objects, with exact native
question unions exposed in the language bindings. Preserve Jev versus Google
criteria field differences. Do not use ordinary chat messages as a decision
request, or add response_format/output generation arguments these APIs cannot use.
For Gemini expose the documented input forms and use existing media-binding
facilities, with their native request types and acceptance proof.

## Typed verdict_transform projections

One canonical closed `VerdictTransform` contract lives in wyrd-spec. It contains
the locked Choice/Probability/Score policies, plus agreed flat All composition.

- Rust: public enum with declarable variants using typed payload structs where
  useful; serde mapping support through existing authoring facilities. Expose
  through the client/SDK owning re-exports. Do not require callers to construct
  serde_json::Value to use a supported transform.
- Python: `ChoiceTransform`, `ProbabilityTransform`, `ScoreTransform`, and
  `AllTransform` are Pydantic models with literal kind defaults;
  `VerdictTransform` is their discriminated union. Public imports are under
  `wyrd.verify`. Constructors/setters accept either these model instances or
  canonical mappings. Lower via the established SDK coercion boundary into the
  same Rust contract. Models provide language typing/serialization; Rust remains
  the behavioral and cross-field validation owner. Use installed Pydantic;
  do not add a competing PyO3 transform-class family.
- TypeScript: export discriminated interfaces named `ChoiceTransform`,
  `ProbabilityTransform`, `ScoreTransform`, and `AllTransform`, and
  their `VerdictTransform` union. Keep wire fields in snake_case, matching all
  other stored fields. Ordinary object literals satisfy these interfaces;
  no runtime wrapper class or `.toJSON()` ceremony.

Examples of the same probability transform:

```python
ProbabilityTransform(
    question="billing",
    expected=True,
    pass_at=0.9,
    fail_at=0.1,
)
```

```typescript
const billing: ProbabilityTransform = {
  kind: "probability",
  question: "billing",
  expected: true,
  pass_at: 0.9,
  fail_at: 0.1,
};
```

```rust
VerdictTransform::Probability(ProbabilityTransform {
    question: "billing".to_owned(),
    expected: true,
    pass_at: 0.9,
    fail_at: 0.1,
})
```

Exact private struct layout is implementation-owned; these are the agreed
public interfaces, not code that runs in the current release.

## Complete agreed Python authoring flow

```python
from wyrd.prompt import Prompt
from wyrd.verify import (
    Agent, AllTransform, Check, ExecutionRoute, ProbabilityTransform,
    Requirement, Verifier,
)

prompt = Prompt.jev(
    "jev-judge",
    state="${context}",
    questions={
        "billing": {"type": "noul", "instructions": "Is this message about billing?"},
        "refund": {"type": "noul", "instructions": "Does the customer request a refund?"},
    },
)

judge = Agent(
    "billing-judge", "1.0.0",
    prompt=prompt,
    execution_route=ExecutionRoute.wyrd_gateway(deployment="judge-pool"),
)

check = Check.agent(
    judge,
    inputs={"context": "inputs.message"},
    verdict_transform=AllTransform(transforms=[
        ProbabilityTransform(question="billing", expected=True, pass_at=0.9, fail_at=0.1),
        ProbabilityTransform(question="refund", expected=True, pass_at=0.9, fail_at=0.1),
    ]),
)

verifier = Verifier(
    "billing-refund", "1.0.0",
    inputs={"message": str},
    requirements=[Requirement("billing-refund", "This is a billing refund request", check)],
)
```

With execution input message equal to the maintainer's charged-twice text, the
upstream body contains the literal state key with that bound text and both
questions. Transform each answer once and aggregate in memory, without another
provider call. Mappings remain accepted in place of the typed model.

The Google equivalent uses `Prompt.gemini_decisions("gemini-decision-1",
input="${context}", questions=...)` and a Google deployment, retaining the same
check policies and SDK flow. Each constructor preserves its actual provider
contract rather than making callers author format/body dictionaries.

## Gemini onboarding is part of this change

Follow the actual [Decisions API](https://aistudio.google.com/docs/decisions-api)
through all existing provider owners: typed wire request/response, Prompt
format/rendering, provider client/auth/dispatch, Agent one-call fixed-output
execution, gateway operation/capability/native route, usage and error mapping,
deployment admission, and native response delivery to Vala's verdict facade.
Retain provider-specific values: Google Noul confidence, usage shape, choice
labels/distribution, and Score scale. Do not deserialize Google into JevResponse.

Use the documented upstream v1beta/decisions endpoint and existing Google auth
and shared transport/retry configuration. Proposed Wyrd format and operation are
`decisions`, with `/v1/decisions` as the native gateway route. Preserve the usual
permissions, tenancy, admission, audit staging, and asynchronous recording. No
new pool/client per request and no migration are proposed.

Provide Rust/Python/TypeScript typed authoring plus raw mapping support, normal
schema/codegen, docs, and the same SDK/server/gateway/native-upstream journeys as
other providers. Native media rendering follows existing binding conventions;
advertised input forms need corresponding tests. Existing streaming/tool-loop
features are not sent to an API that does not support them.

## Mandatory provider-pattern review clarification

The maintainer explicitly requires Gemini Decisions types and provider execution
to follow the existing provider patterns, using Jev's native contracts as the
reference. Reuse `wire_contract!` owned/borrowed request views, borrowed native
validation, typed native responses, ProviderRequest/ProviderResponse dispatch,
the existing RequestJson forwarding path and shared transport/client/retry
owners. Do not replace these throughput patterns with an owned JSON-tree
validation path or intermediate serialize/reparse conversions. Google keeps its
own native contract and established provider identity. This is a hard review
requirement under spec REQ-004, alongside the existing clean benchmark proof;
functional verdict journeys alone do not satisfy it.

## Verification and status

Add constructor render/round-trip examples proving top-level state/input and
all question keys survive typed SDK authoring. Assert equivalent model and
mapping declarations produce the same canonical contract. Drive both providers
through real SDK -> WyrdTestServer -> native local mock and inspect the exact
upstream JSON, then assert verdict and captured answers. Cover the three locked
leaf modes and agreed all composition, invalid inputs, gateway admission,
provider refusals, and unchanged existing Prompt providers/Agent checks.

No production edits or runtime checks were performed. The maintainer locked
these interfaces, aggregation, and Gemini onboarding for specification. Typed question objects are also finalized in question-models.md; they do not
change the agreed behavior here.
