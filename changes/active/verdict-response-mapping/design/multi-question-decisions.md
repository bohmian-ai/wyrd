# Shared context, multiple questions, and Gemini Decisions

Status: finalized; maintainer locked the revised extension on 2026-11-18.

The maintainer subsequently required Prompt-class constructors, typed SDK
transforms, and Gemini Decisions onboarding in this change. The recommended
SDK experience is now [SDK interfaces](sdk-interfaces.md); the native mapping
example below remains a supported low-level representation, not the recommended
authoring experience. Multi-question `all` is agreed.

## State injection already exists

Use a normal check input binding, for example `inputs={"context": "inputs.context"}`,
and a native Prompt body with `state: ${context}`. Vala resolves the evidence,
renders JSON strings as raw text and other JSON values as serialized JSON, and
Skald substitutes the value into the typed request. Each question receives that
same state. The context need not be named state; it can combine policy, answer,
and other evidence in one JSON input, or use several ordinary text placeholders.
Do not add a second state-injection mechanism.

Verified owners:
- `crates/vala/vala-verify/src/blocks/agent.rs::bind` and `render`: JSON strings
  remain raw strings; objects and arrays become JSON text.
- `crates/skald/skald-spec/src/prompt.rs::jev_state_renders_quotes_and_newlines_verbatim`
  pins typed state binding with quotes/newlines and a three-question map. Source
  inspected; this test was not executed in this design session.
- `crates/skald/skald-spec/src/authoring.rs::portable_request` explicitly refuses
  portable chat-style Jev prompts and requires native format/body.
- `examples/gateway-setup/agents/jev-agent.yaml` uses this native form today.

Jev's wire struct currently accepts string state, despite current upstream
support for richer JSON states. Serializing a JSON context to state text works
within the existing Wyrd contract; native object-state support is not needed here.

## One request, several questions, one check verdict

Keep the accepted leaf transforms. Add one flat composition:

```yaml
verdict_transform:
  kind: all
  transforms:
    - kind: probability
      question: promises_refund
      expected: false
      pass_at: 0.9
      fail_at: 0.1
    - kind: probability
      question: exposes_personal_data
      expected: false
      pass_at: 0.9
      fail_at: 0.1
```

Invoke the Agent once with both questions. Transform each selected answer once
in memory. Use the same verdict precedence as the existing `decide: all`:
any failed => failed; otherwise any inconclusive => inconclusive; otherwise passed.
Require a nonempty list and unique question keys; allow only leaf transforms
inside `all`. Reject missing/malformed selected answers through the existing
check-validation-error path, including when another selected answer failed;
do not short-circuit away validation errors. Unselected ancillary answers do not
affect the verdict. Do not infer question dependencies: both providers evaluate
questions independently over shared context.

Example: noul answers 0.02 and 0.98 yield per-question passed and failed, then
one failed requirement. Answers 0.02 and 0.5 yield inconclusive. Answers 0.02
and 0.03 yield passed. This remains one Wyrd requirement, not two independently
addressable requirements and not two billable provider invocations.

Keep per-question reasons/outcomes and selected native answers in the aggregate's
existing captured `actual` JSON, for example `{answers: {...}, verdicts: {...}}`;
the overall reason identifies each selected question's outcome. Existing capture
policy applies. No result table or public result-envelope expansion is needed.
Single-leaf capture behavior remains as locked. Do not add other combinators or
nested expression trees without an actual use case.

## Python SDK: complete proposed authoring example

The Agent, native Prompt mapping, input bindings, and Verifier authoring patterns
already exist. The `verdict_transform` keyword and `all` variant below are proposed
contract additions, not runnable features today. No YAML file or raw HTTP needed.

```python
from wyrd.verify import Agent, Check, ExecutionRoute, Requirement, Verifier

questions = {
    "promises_refund": {
        "type": "noul",
        "instructions": "Does the answer promise a guaranteed refund?",
    },
    "exposes_personal_data": {
        "type": "noul",
        "instructions": "Does the answer expose the customer's personal data?",
    },
}

judge = Agent(
    "support-judge",
    "1.0.0",
    execution_route=ExecutionRoute.wyrd_gateway(deployment="judge-pool"),
    prompt={
        "provider": "jev",
        "model": "jev-judge",
        "format": "system_one",
        "body": {"state": "${context}", "questions": questions},
    },
)

check = Check.agent(
    judge,
    inputs={"context": "inputs.context"},
    verdict_transform={
        "kind": "all",
        "transforms": [
            {
                "kind": "probability",
                "question": question,
                "expected": False,
                "pass_at": 0.9,
                "fail_at": 0.1,
            }
            for question in questions
        ],
    },
)

verifier = Verifier(
    "safe-support-answer",
    "1.0.0",
    inputs={"context": dict},
    requirements=[Requirement("safe-answer", "The answer is safe to send", check)],
)
```

The execution input named context can be an object with answer, policy, and
customer facts. It becomes JSON text in state; both questions can inspect it.
Adding a question to this example's map deliberately adds a matching policy
through the comprehension. Applications may explicitly select a subset instead.

TypeScript uses the same authored Prompt and verdict_transform objects through
its existing Agent/Check authoring owners; Rust uses the canonical authored
Prompt and typed check contract. Provide typed projections in all three SDKs.
Do not introduce another SDK-only durable request format. Existing Python
Prompt(messages, ...) is a chat builder and explicitly refuses Jev; add
provider-shaped Prompt constructors as detailed in the SDK supplement.

## Gemini Decisions: same transforms, additional native integration

The supplied [official Decisions documentation](https://aistudio.google.com/docs/decisions-api)
defines `POST https://generativelanguage.googleapis.com/v1beta/decisions`, with
model, shared input, questions, and keyed answers. It supports choice, noul,
and score. Noul also includes confidence and values are bounded to 0.01..0.99;
Score uses authored level indices, as Jev does. Google usage has a distinct
shape. These are compatible transform semantics but distinct native wire types.

The equivalent proposed Wyrd Prompt is:

```python
prompt={
    "provider": "google",
    "model": "gemini-decision-1",
    "format": "decisions",
    "body": {"input": "${context}", "questions": questions},
}
```

`format: decisions` is proposed, not currently supported. Point execution_route
at a Gemini deployment serving that model/operation. The example's questions
omit optional Noul criteria, so their shape works on both APIs. If criteria are
used, Google has true_description/false_description where Jev uses true/false;
keep each native contract explicit. Never silently reinterpret one provider's body.

Gemini implementation scope is now explicitly required by the maintainer:
- Skald owns native Google Decisions request/response types and `decisions`
  Prompt format, placeholder discovery/rendering, provider client dispatch, and
  native usage handling. Its Agent runtime dispatches this fixed-answer request
  once without a tool loop, like Jev, and preserves final_response.
- Gateway owns a `decisions` operation/capability and native `/v1/decisions`
  route to the upstream v1beta endpoint, with its existing Google auth, permission,
  admission, usage, and error owners. Reject unsupported tools/generation settings
  and incompatible providers before upstream invocation.
- Vala's facade matches the typed Google response and reads the same semantic
  answer fields; no conversion to JevResponse and no generic JSON-path parser.
  Existing leaf/composite policies and capture rules apply unchanged. Preserve
  Google's extra Noul confidence as native answer evidence.
- Project the new native operation/format through SDKs, schemas, MCP/HTTP/CLI
  owners, deployment capability validation, examples, and docs. No migration or
  new evidence store is proposed.

The examples exercise text/JSON-as-text shared context. Follow the documented
native Google input contract and reuse existing media binding conventions;
do not silently restrict onboarding to text or claim media support without
the corresponding typed rendering and journey proof.
Google safety refusals remain provider/check errors, not an authored failed claim.
Keep explicit Noul bounds; do not infer policy from provider confidence. The
example's 0.1/0.9 bounds lie inside Google's documented attainable interval.

## Proof and agreement boundary

Keep the locked single-question contract. Flat `all` composition and its captured
question breakdown, native Gemini Decisions integration, and typed ergonomic SDK
authoring are agreed. Carry them into corresponding architecture/spec additions
before implementation. Typed question classes are finalized in question-models.md.

Proof must capture one actual upstream request containing the correctly bound
shared context and both questions, then assert the aggregate SDK verdict and
captured per-question answers/outcomes. Exercise pass/fail/inconclusive mixtures,
malformed selected answers, and unchanged leaf behavior for both providers.
Use native mock responses against real SDK/server journeys, including provider
admission/refusal and the new gateway operation. No live calls or runtime tests
were performed during this design research.
