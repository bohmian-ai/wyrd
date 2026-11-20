# Fixed-contract verdict transforms

Status: finalized

## Problem

Jev cannot emit Wyrd's canonical Verdict: it has a fixed typed response contract.
Wyrd already decodes that contract but discards the typed response before an
Agent check consumes it. Requiring users to implement parsing or a second code
step defeats the intended verification workflow.

## Desired outcome

An author uses Jev in a normal Agent check. A built-in Wyrd transform reads the
known response contract, applies the declared judgment policy, and produces a
canonical verdict. One conversion facade supports future fixed-contract providers.
The maintainer accepted and locked the complete design on 2026-11-18:
leaf transforms, flat `all` aggregation, shared context binding, native Gemini
Decisions onboarding, ergonomic Prompt constructors, typed SDK transforms, and
typed question objects. The complete agreement includes
[multiple-question decisions](multi-question-decisions.md),
[SDK interfaces and provider onboarding](sdk-interfaces.md), and
[typed question authoring](question-models.md). All supplements are finalized.
The [observable journeys](user-journeys.md) elaborate proof of these agreed
outcomes for specification and planning without changing the finalized behavior.

## Constraints and non-goals

Design only. No production code, architecture edits, spec, tasks, or commits.
Keep provider wire handling in Skald and verification semantics in Vala. Reuse
canonical validation, existing aggregation, context capture, and asynchronous
Bifrost recording. No generic expression language, plugin registry, new dependency,
migration, second model call, or additional external IO from conversion. Each
leaf transform selects one question; flat `all` combines selected answers from
one provider invocation. Gemini Decisions integration and ergonomic SDK
interfaces are part of this change.

## Proposed implementation

### Complete authoring example

This is the agreed Wyrd syntax to implement. The gateway deployment judge-pool and its Jev model
alias jev-judge are assumed configured, following the existing gateway example.
Only verdict_transform is new public syntax here.

```yaml
apiVersion: wyrd/v1
kind: Verifier
metadata:
  name: no-refund-promise
  version: 1.0.0
spec:
  inputs:
    answer: {type: json, schema: {type: string}}
  requirements:
    - id: no-refund-promise
      statement: The answer never promises a guaranteed refund
      check:
        kind: agent
        agent:
          execution_route: {kind: wyrd_gateway, deployment: judge-pool}
          prompt:
            provider: jev
            model: jev-judge
            format: system_one
            body:
              state: ${answer}
              questions:
                refund_policy:
                  type: choice
                  instructions: Does the answer promise a guaranteed refund?
                  criteria:
                    passed: The answer does not promise a guaranteed refund
                    failed: The answer promises a guaranteed refund
                    inconclusive: There is insufficient information to decide
        inputs:
          answer: inputs.answer
        verdict_transform:
          kind: choice
          question: refund_policy
```

Caller input: `answer: "You are guaranteed a full refund."`.
The binder substitutes it into state; the gateway resolves the model alias.
Illustrative native response, conforming to Jev's documented shape:

```json
{
  "model": "jev-1.13.0",
  "answers": {
    "refund_policy": {
      "type": "choice",
      "choice": "failed",
      "confidence": 1.0,
      "probabilities": {"passed": 0.0, "failed": 1.0, "inconclusive": 0.0}
    }
  },
  "usage": {"input_tokens": 120, "output_tokens": 12}
}
```

This is an example fixture, not a live prediction or measured token usage.
Wyrd's built-in transform produces:

```json
{
  "outcome": "failed",
  "reason": "Question 'refund_policy' selected 'failed' (confidence 1).",
  "citations": []
}
```

The requirement fails and the normal default decide: all makes the execution
verdict failed. There is no response_format: verdict on this Prompt, and Wyrd
never attempts to send a dynamic Verdict schema to Jev.

### Concrete transform contract

A closed verdict_transform union lives on Agent checks, not Agent actions or
provider configuration. The transform kind declares policy; Wyrd dispatches its
native decoding by the actual response contract. Users never specify JSON paths.

- **choice**: required question key. With no outcomes map, the question's option
  labels must be canonical passed/failed/inconclusive; copy the selected label.
  For existing custom labels, an optional exhaustive outcomes map assigns every
  declared choice label a canonical outcome. Example:
  `{kind: choice, question: refund_policy, outcomes: {compliant: passed, violation: failed, unknown: inconclusive}}`.
  Confidence is retained but does not implicitly change the verdict.
- **probability**: required question, expected boolean, pass_at, fail_at. For a
  Jev Noul p, q=p when expected is true, else q=1-p. Pass when q>=pass_at;
  fail when q<=fail_at; otherwise inconclusive. Validate
  `0 <= fail_at < pass_at <= 1`. Example:
  `{kind: probability, question: promises_refund, expected: false, pass_at: 0.9, fail_at: 0.1}`.
  Native noul 0.02 passes, 0.98 fails, 0.5 is inconclusive. Bounds are authored,
  not Wyrd defaults. Reasons state the original value and the applied policy.
- **score**: required question, numeric comparison operator, expected number.
  Reuse existing scalar numeric ComparisonOperator semantics and wire names;
  pass when comparison holds, fail otherwise. Example policy: compliance score
  greater than or equal to 2 on an authored three-level scale. Score 2 passes;
  1.8 fails. Do not normalize the score, add confidence gates, or invent an
  uncertainty interval. The concrete declaration is
  `{kind: score, question: compliance, operator: greater_than_or_equals, expected: 2}`,
  using the existing ComparisonOperator wire spelling.

All three modes require an explicit question key. Do not silently select the
first answer. Unknown labels, missing answers, mismatched answer tags, or invalid
numbers follow the existing check-validation-error path, not a guessed verdict.
Validate Noul within 0..1 and Score within 0..N-1 using the resolved request.
An explicit inconclusive answer or probability uncertainty interval is an ordinary
inconclusive verdict. Provider outage/retry behavior remains unchanged.

### Connected implementation flow

1. **wyrd-spec** defines VerdictTransform and a check-specific Agent wrapper
   that reuses/flattens AgentCall, retaining the current kind: agent wire shape.
   Agent actions remain AgentCall. All authored/compiled reference traversal
   carries the check policy. This avoids allowing transforms on preparation actions.
2. **Registration validation** resolves the Agent and Prompt as today. Without
   a transform, require the existing canonical Verdict response schema. With a
   transform, validate its response-contract compatibility, selected question,
   option coverage, and numeric policy before invoking the provider. Keep the
   current refusal to attach named dynamic response formats to system_one.
3. **Invocation** preserves the existing Arc<ProviderResponse> from
   AgentRun.final_response in Vala's Invoked result. Skald already owns the
   JevResponse/JevAnswer types. No serialization/reparse or duplicate output model.
4. **Vala verdict facade**: one concrete VerdictTransformer with inherent methods,
   accepting typed provider response, resolved question, and validated policy.
   Match the response variant and policy, apply the built-in conversion, and
   return CheckOutput. No single-implementation trait or adapter registry.
5. **Validation/result**: produce the canonical Verdict and reuse its existing
   validation owners, including the engine's citation validation. Generate a
   factual reason from the answer/policy; never pretend Jev generated an
   explanation. Use empty citations, which the architecture permits. Preserve
   existing direct-Verdict checks and ordinary requirement aggregation.
6. **Evidence**: place the selected native answer object, including probabilities,
   in CheckOutput.actual. Existing context_capture and Bifrost result recording
   preserve/hash/redact it as configured. Retain existing Agent/Prompt/route
   provenance; the versioned Verifier identifies the transform policy. Do not
   repurpose Verdict.data or introduce another evidence table/detail variant.
7. **Surfaces**: project the same check contract through Rust, Python, and
   TypeScript authoring, Python authored stubs, and normal schema/codegen paths.
   HTTP/MCP/CLI consume the canonical contract. Update the relevant examples/docs.

Future providers add their typed response handling at this facade, reusing these
policies when their semantics fit. They need neither Jev field names nor a
caller-written mapper. Add a new policy only when a real contract requires it.
Conversion itself is synchronous and in memory, with no new PostgreSQL, Scribe,
object storage, Vault wait, or provider call.

## Evidence and rationale

- [Official Choice contract](https://docs.typesafe.ai/primitives/choice): keyed
  choice, confidence and probabilities; option labels are authored by the caller.
- [Official Noul contract](https://docs.typesafe.ai/primitives/noul): probability
  of yes, no separate confidence, caller-owned decision thresholds.
- [Official Score contract](https://docs.typesafe.ai/primitives/score): position
  from 0 to N-1. The comment in skald-spec/src/wire/jev.rs:118 says normalized
  0..1 and disagrees; correct that comment when implementing this owner.
- crates/skald/skald-spec/src/wire/jev.rs:87-129 already defines typed answers.
- crates/skald/skald-agent/src/run.rs:74 retains final_response as an Arc.
  It is serde-skipped; retaining that handle alone does not persist evidence.
- crates/vala/vala-verify/src/blocks/agent.rs::invoke currently drops that handle;
  CheckExecutor::execute requires structured output and a Verdict schema.
- crates/wyrd-spec/src/card/verifier/validate.rs:839-850 rejects non-Verdict
  check schemas. A runtime decoder alone cannot repair the user journey.
- crates/vala/vala-verify/src/blocks.rs::CheckOutput.actual already accepts JSON;
  blocks/engine.rs::checked captures it under context_capture. Reuse this evidence path.
- examples/gateway-setup/agents/jev-agent.yaml establishes the native Prompt/gateway
  syntax; examples/quickstart/no-refund-promise.yaml supplies the existing journey.

## Decisions and alternatives

Maintainer agreement: automatic Wyrd conversion for fixed provider contracts and
an extensible facade, with the authoring syntax, three transforms, execution flow,
failure behavior, and evidence capture defined above. The earlier
generic ordered-rule/JSON-path proposal is superseded. A separate Python parsing
step makes users implement provider decoding that Wyrd already owns.

## Standards conflicts

architecture/wyrd-design.md's Prompt rules around 547-562 forbid named formats
on system_one, preserved here. Its Agent-check table at 1649 currently requires
the provider to emit Verdict directly. This proposal needs a targeted authority
revision permitting explicitly transformed native answers. By locking this
design, the maintainer chose to revise that provider-emits-Verdict restriction
for checks with an explicit transform. Carry this authority update into the
specification handoff before implementation; no authority edits have been made.
The citation rules around 1750-1763 permit empty citations.

## Open questions

No unresolved material decisions. Typed question objects and all eight observable
journeys are agreed, including J6 authoring-time detection and correction through
local validation. Ordinary module layout/helper choices remain implementation-owned.

## Handoff

The complete design, including typed question objects, is ready for `$wyrd-spec`.
Finalization is not implementation approval. Preserve the agreement in all
supplements and targeted architecture updates; do not automatically start
specification or planning.

Use [the eight finalized observable journeys](user-journeys.md) as the connected
acceptance scenarios; the maintainer locked J1-J8 on 2026-11-18. They exercise real authoring, request binding, provider invocation,
verdict conversion/aggregation, error handling, and evidence inspection, rather
than treating private helpers or type additions as delivered outcomes.

Proof: real SDK -> WyrdTestServer ->
local native Jev mock -> SDK result for Choice pass/fail/inconclusive and custom
labels, both Noul polarities/bounds/middle, Score scale/comparison, declaration
refusals, invalid response handling, and selected-answer context capture. Preserve
the existing direct-Verdict journeys on Rust/Python/TypeScript and HTTP/MCP.
Use existing verification/schema/codegen lanes. Any hot-path review must list
added/moved waits (none proposed) and include the required full clean verification
benchmark compared with the same-host baseline.
