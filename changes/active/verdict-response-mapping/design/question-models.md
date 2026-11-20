# Typed decision-question authoring

Status: finalized; maintainer agreed and locked this addition on 2026-11-18.

## Outcome and agreed public shape

Authors can use actual typed objects for Noul, Choice and Score questions with
both Prompt.jev and Prompt.gemini_decisions, alongside native dictionaries.
Recommend three concrete question types, not one class with type-dependent
optional fields. Each carries its discriminator automatically:

| Type | Fields |
|---|---|
| NoulQuestion | instructions; optional NoulCriteria |
| ChoiceQuestion | instructions; required criteria mapping option name to description |
| ScoreQuestion | instructions; required ordered criteria list |

`DecisionQuestion` is their discriminated union. NoulCriteria has
true_description and false_description. Shared question helpers cover text
instructions/criteria, which both providers support. Existing native mappings
remain available for provider-specific forms; do not weaken or remove them.

## Concrete Python experience

Pydantic models, with Literal discriminator defaults and unknown-field refusal:

```python
from wyrd.prompt import Prompt
from wyrd.prompt.decisions import (
    ChoiceQuestion, NoulCriteria, NoulQuestion, ScoreQuestion,
)

questions = {
    "billing": NoulQuestion(
        instructions="Is this message about billing?",
        criteria=NoulCriteria(
            true_description="A charge, invoice, payment, or refund issue.",
            false_description="An issue unrelated to billing.",
        ),
    ),
    "department": ChoiceQuestion(
        instructions="Which department should handle this message?",
        criteria={
            "billing": "Payments, invoices, and refunds.",
            "technical": "Problems using the product.",
        },
    ),
    "urgency": ScoreQuestion(
        instructions="How urgently does this need attention?",
        criteria=["Routine", "Time-sensitive", "Immediate attention"],
    ),
}

jev = Prompt.jev("jev-judge", state="${context}", questions=questions)
google = Prompt.gemini_decisions(
    "gemini-decision-1", input="${context}", questions=questions,
)
```

No explicit type string, model_dump, native body, or dictionary conversion is
required from the caller. Mixed typed/native question values are accepted.
Question names remain the map keys; each question object need not repeat its ID.

## Other SDKs

- Rust: declarable NoulQuestion, ChoiceQuestion, ScoreQuestion structs and a
  DecisionQuestion enum; From conversions permit heterogeneous question maps.
  Provider-shaped Prompt options accept them, and the shared Rust builder lowers
  each to its existing/native provider-owned wire type.
- TypeScript: expose the same discriminated interfaces, plus small concrete
  classes NoulQuestion, ChoiceQuestion, and ScoreQuestion with option-object
  constructors and a readonly literal type discriminator. Existing object literals
  remain structurally accepted. For example:
  `new NoulQuestion({instructions: "Is this message about billing?"})`.
  The classes provide the requested actual-object authoring path; no redundant
  family of factories or toJSON wrappers.
- Python: the three Pydantic model classes above plus their union are public.
  Pydantic is currently an optional extra in this SDK. Keep model imports scoped
  to the existing Pydantic-enabled path and avoid making core dictionary/native
  use require Pydantic. Constructor coercion accepts models directly, through
  the existing language boundary and shared Rust lowering. Do not introduce
  duplicate PyO3 question classes alongside Pydantic models.

## Provider differences and ownership

The shared objects are SDK authoring helpers, not a new durable Prompt format.
After construction each Prompt still stores/hashes its actual native contract.
Place reusable question lowering in the existing Rust Prompt builder owner,
expose it through the existing SDK re-exports/bindings, and reuse native question
types for the final wire body. Validation stays at its existing owner; language
models supply field typing and serialization rather than duplicate provider rules.

For typed NoulCriteria the Jev constructor lowers descriptions to
`criteria: {"true": ..., "false": ...}`; the Google constructor lowers to
`criteria: {"true_description": ..., "false_description": ...}`. This is explicit
constructor behavior for shared helpers. Raw native dictionaries keep the selected
provider's field names and are validated as that provider's contract. Never infer
or silently translate raw body dictionaries from another provider.

Jev and Google have different limits, so validate against the chosen provider at
Prompt construction/declaration. Keep Score criteria ordering. No automatic
label rewriting, verdict policy, or confidence policy belongs in a question
object. The same questions work in an ordinary Agent action or in a verdict check.

This authoring layer adds no request-time IO or additional model call. The
transform, aggregation, state binding, provider routes, and result contracts
remain as locked in the preceding revision.

## Proof and agreement

Typed and equivalent native-map questions must render identical native requests
for each provider, including Noul criteria field translation. Pin omitted Noul
criteria, Choice labels, Score order, mixed question maps, placeholder binding,
model serialization and provider-specific refusal through existing constructor
tests and SDK/server journeys. Exercise the typed public imports in all three
SDKs and preserve dictionary-only Python use without its Pydantic extra.

The maintainer accepted this complete question-model design. No production changes,
runtime checks, or live provider calls have been performed.
