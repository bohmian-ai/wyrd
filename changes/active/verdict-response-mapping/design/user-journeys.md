# Observable implementation journeys

Status: finalized; maintainer locked all eight scenarios on 2026-11-18.

These scenarios define the agreed observable proof of the finalized design,
including authoring-time validation in J6. They do not authorize implementation. They are inputs to specification and planning,
not a task set or invented executable test names. The agreed design remains
finalized. Implement in observable increments rather than completing disconnected
wire types, SDK classes, and transform helpers as separate deliverables.

## Common proof path

A caller authors a real Prompt, Agent, and Verifier through a public SDK,
declares the graph, supplies execution evidence, and inspects the returned and
recorded result. Use the real SDK -> WyrdTestServer + repository Postgres ->
configured gateway -> local native-provider mock -> SDK path. The mock records
the exact upstream request and returns a documented fixed response. It replaces
the provider only, not the registration, invocation, transform, or result path.

Exercise this connected capability through Rust, Python, and TypeScript. Cover
the HTTP/MCP/CLI surfaces that ship the changed declaration, execution, gateway,
and inspection contracts through their existing owners; do not invent a new
provider-specific MCP workflow. Typed and mapping authoring are alternative inputs
to the same contract, not two independent runtimes.

## J1 — Author a Jev judge and receive a normal verdict

Start with the configured Jev gateway deployment. Author Prompt.jev with a
state placeholder and a typed ChoiceQuestion whose categories are passed,
failed and inconclusive. Attach a typed ChoiceTransform to an Agent check,
declare the Verifier, and execute it with an actual support answer.

Observe a native `/v1/systemone` request containing the bound top-level state,
the declared question key, instructions and option descriptions, and the resolved
model. The native selected choice becomes the SDK's canonical requirement and
execution verdict, with a factual reason and no fabricated citations. Exercise
all three outcome labels; a valid citation-free pass stays passed.

Authoring variants: plain native maps, typed questions/transforms, and mixed
question maps render the same provider request and canonical declaration when
otherwise identical. Declare/read the graph back and rebuild/redeclare it with
the same semantics; no type discriminator or criteria is lost. Python models are
accepted directly without model_dump, while dictionary-only use still works
without the optional Pydantic extra. Assert enum/union types in Rust and actual
question classes/interfaces in TypeScript through their public imports.

## J2 — Use the same verification workflow with Gemini Decisions

Configure a Google deployment for Decisions through existing gateway administration.
Author Prompt.gemini_decisions with a shared input placeholder and the same
typed questions and transforms as J1. Declare and run normally.

Observe the request reaching Google's native `/v1beta/decisions` endpoint with
input rather than state, its declared questions and the correct model. Receive
the same canonical verdict semantics. Request/response usage and Google's extra
Noul confidence are handled by the existing provider/gateway result conventions.
The new native gateway operation accepts authorized requests through its normal
public route; configuration lacking the Decisions capability is refused before
an upstream call. Credentials remain in the existing provider configuration flow.

Question criteria variants: typed NoulCriteria produces true/false on Jev and
true_description/false_description on Google; omitted criteria stays omitted;
Choice labels and Score order survive unchanged. Raw native dictionaries retain
their provider-specific meanings. Exercise documented native media input forms
advertised by this onboarding through the established media-binding journey;
the provider mock must observe the correct media representation, not just a
text substitution. This discharges the already agreed Google input scope.

## J3 — Declare what the provider's answer means

Use a real hosted check with authored acceptance policy and known native responses.
The observable result must follow that exact policy on either provider:

| Policy | Inputs and expected result |
|---|---|
| Custom Choice labels | compliant -> passed, violation -> failed, unknown -> inconclusive using the declared exhaustive map |
| Probability, expected true | p at pass_at passes; p at fail_at fails; a value strictly between is inconclusive |
| Probability, expected false | The same comparisons apply to 1-p; for the locked 0.9/0.1 example, p=0.02 passes, p=0.98 fails, p=0.5 is inconclusive |
| Score | On three levels with greater_than_or_equals 2, score 2 passes and 1.8 fails; never reinterpret 1.8 as an invalid normalized probability |

Reasons expose the compared answer and applied policy. Provider confidence alone
does not change an authored outcome. Keep exact boundary enumeration in focused
pure tests and representative positive, edge and negative journeys through the
server; do not substitute decoder-only tests for the observable flow.

## J4 — Ask several questions once and receive one explained verdict

Author one Agent check with two typed Noul questions over shared context and an
AllTransform selecting both. Supply an object containing the answer and policy;
include quotes/newlines to exercise ordinary safe state/input binding.

Observe one native provider request containing both questions and the complete
bound context. Native probabilities 0.02/0.03 for the expected-false example
produce passed; 0.02/0.98 produce failed; 0.02/0.5 produce inconclusive. A failed
answer dominates an inconclusive answer under all. Inspect the question-level
outcomes/reasons and selected native answers in the captured result. There is
one requirement verdict and no second provider call for conversion/aggregation.

Variants: mix Choice/Probability/Score leaf transforms, reorder answer-map keys,
and include an unselected ancillary answer. Selection follows declared question
keys, not map order; ancillary answers do not alter the verdict. This does not
promise batching across separate checks or nested transform expressions.

## J5 — Inspect why a recorded verification passed or failed

After a successful execution, read its result through the normal SDK inspection
surface once asynchronous recording becomes visible. The canonical verdict,
reason, selected native answer/probabilities, aggregate breakdown when present,
and existing Agent/Prompt/route provenance agree with the executed versioned
Verifier policy. Do not require execution to wait for recorded evidence.

With full context capture, the selected answer is inspectable; with the existing
hash/redaction setting, the result follows that setting rather than leaking the
raw answer. The capture/read journey proves evidence is recorded, not merely
retained in an in-process final_response handle. Existing tenant/read permissions
continue to govern this new captured content.

## J6 — Detect and correct an invalid check during authoring

Author an invalid Verifier and receive an actionable correction during local
validation. Correct it, validate successfully, then declare and execute it.
No provider request occurs for the invalid definition.

Typed-object construction catches invalid fields, types, and self-contained
constraints, including invalid probability bounds and an empty AllTransform.
The existing `verifier.validate()` workflow checks the assembled graph for
missing selected question keys, transform/question mismatches, incomplete
Choice maps, and duplicate selected question keys. Errors identify the field
and correction. Preserve incremental authoring: cross-object consistency is
checked when validating the assembled graph, not after every mutable edit.

Use provider-specific invalid question counts/criteria as variants to prove
that shared authoring objects honor the selected provider's contract. Exercise
public language constructors and the local validation API, followed by the
corrected real declaration/execution journey. Reuse shared validation semantics;
do not create separate SDK-owned behavioral rules.

When a registered Agent or Prompt cannot be resolved locally, authoritative
registration resolves and validates that relationship before upstream invocation.
Server validation remains authoritative even after successful local validation;
SDK/API refusals must remain actionable structured errors. This exception does
not defer locally knowable cross-object mistakes until execution.

## J7 — Distinguish an unusable provider answer from a failed claim

Run a valid declaration, then return a missing selected answer, mismatched tag,
unknown Choice label, or invalid numeric value from the native provider mock.
Observe the existing check-validation-error behavior and explicit reason, never
a silently accepted pass or a fabricated provider-authored failed verdict. A
second valid failed answer must not conceal the malformed answer in AllTransform.

Exercise the provider's refusal and existing retry/error conventions through
the hosted flow. Verify the new Google operation preserves existing permission
and tenant boundaries: a caller denied that operation does not reach the provider,
and another tenant cannot inspect its captured results. These are applications
of existing repository requirements, not new authorization mechanisms.

## J8 — Keep existing authoring and verification working

Run an existing Agent check whose Prompt directly emits Verdict without a
verdict_transform. Its verdict, citations, declared data and error handling stay
unchanged. Existing Google generation and Jev native gateway use remain valid
alongside Decisions and transforms. Question helpers also construct a normal
Agent action Prompt without requiring verdict policy; do not imply this change
alters the existing action-output representation.

Reuse existing journey proof where it already establishes these invariants.
Run new round-trip checks only where the changed contract can lose information.

## Planning and proof ownership

J1 is the smallest complete delivery increment: real authoring -> one native
Jev question -> canonical SDK verdict. Extend that working path with J2-J4;
exercise J5-J7 alongside the behavior that introduces their inputs, not as a
last testing-only task. Carry J8 as regression proof. Specification states the
user outcomes once; tasks reference those outcomes and order their behavioral
scenarios for RED/GREEN implementation. Private file/type choices are not outcomes.

Use existing Prompt/authoring tests for construction and wire equality, SDK
journeys for the connected consumer path, native provider/gateway seams for
the actual request/auth/error contract, and the existing result-read path for
captured evidence. Avoid a Cartesian product of every minor fixture: each SDK
and shipped surface must prove its actual boundary, while pure calculations
receive focused owner tests and representative hosted edge/negative flows.

Existing delivery lanes include `mise run verify:rust-sdk`,
`mise run verify:python-sdk`, `mise run verify:typescript-sdk`, and
`mise run test:gateway:journey`; planning selects the narrowest required portions
from current mise.toml/TESTING.md rather than duplicating all aggregate lanes.
Add exact focused commands only after actual test names exist. Schema/codegen,
format/lint and dependency rules remain the ordinary owner gates.

The mandated clean hot-path benchmark and sanctioned-wait inventory are delivery
evidence for verification/gateway changes, not new user-facing functionality.
Use the applicable full `bench:l1:verification` and `bench:l1:gateway` lanes
with the required same-host history comparison. No tests or benchmarks have been
run as part of this design elaboration. Native mocks prove protocol and judgment
handling, not model accuracy on real-world examples.
