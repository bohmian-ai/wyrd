---
id: SPEC-verdict-response-mapping
revision: 1
status: approved
---

# Native decision answers as verification verdicts

## Problem and outcome

An author wants a Jev or Gemini Decisions Agent to judge a requirement. These
providers return fixed question-answer contracts, while Agent checks currently
require a Prompt that emits Wyrd's Verdict directly. Wyrd already decodes Jev's
answers but loses the typed response before verification consumes it.

Authors will use the ordinary Prompt class to supply shared context and named
questions, then attach a declarative `verdict_transform` to an Agent check.
Wyrd will invoke the provider once, apply that policy to its native answers,
and return the ordinary verification verdict with inspectable evidence.
Gemini Decisions onboarding is included in this change.

For example, after configuring a Jev gateway deployment, an author can build:

```python
from wyrd.prompt import Prompt
from wyrd.prompt.decisions import NoulQuestion
from wyrd.verify import AllTransform, ProbabilityTransform

prompt = Prompt.jev(
    "jev-judge",
    state="${context}",
    questions={
        "promises_refund": NoulQuestion(
            instructions="Does the answer promise a guaranteed refund?",
        ),
        "exposes_personal_data": NoulQuestion(
            instructions="Does the answer expose personal data?",
        ),
    },
)
policy = AllTransform(transforms=[
    ProbabilityTransform(question="promises_refund", expected=False,
                         pass_at=0.9, fail_at=0.1),
    ProbabilityTransform(question="exposes_personal_data", expected=False,
                         pass_at=0.9, fail_at=0.1),
])
```

The author supplies `prompt` to an ordinary Agent and `policy` as the check's
`verdict_transform`, binding `context` through its existing `inputs` map.
Native Noul probabilities 0.02 and 0.03 produce one passed requirement;
0.02 and 0.98 produce failed; 0.02 and 0.5 produce inconclusive. Conversion
requires no user-written parser or additional model invocation. These examples
describe the proposed API, not currently executable release behavior.

## Agreement and authority

This revision records the finalized [design](design/notes.md),
[shared-context and aggregation decisions](design/multi-question-decisions.md),
[SDK/provider interfaces](design/sdk-interfaces.md),
[question models](design/question-models.md), and the eight locked
[observable journeys](design/user-journeys.md). This specification defines the
behavioral obligations; planning references its IDs and turns the journeys into
executable proof without creating additional requirements.

The maintainer resolved the direct-Verdict architecture conflict by locking an
explicit-transform exception. Companion edits to
[Prompt](../../../architecture/wyrd-design.md#prompt),
[Verifier](../../../architecture/wyrd-design.md#verifier), and
[in-memory authoring](../../../architecture/wyrd-design.md#in-memory-authoring)
record that decision, native Decisions support and construction/local-validation
boundaries. Named dynamic response formats remain forbidden for fixed decision
protocols. The existing
[citation rules](../../../architecture/wyrd-design.md#verifier),
[recording contract](../../../architecture/wyrd-design.md#recording-and-eventual-consistency),
[Bifrost output ownership](../../../architecture/bifrost-design.md#storage-format-and-variant),
and [security boundaries](../../../architecture/wyrd-security-posture.md#trust-boundaries)
continue to govern this change. Design locks and these authority edits do not
alone constitute specification approval. Explicit maintainer approval is recorded
below.

## Required behavior

### Authoring a provider request

**REQ-001 — Use the actual Prompt class.** Rust, Python and TypeScript expose
the following constructors returning the ordinary Prompt type. Authors need
neither a custom body nor a separate decision-prompt abstraction. The stored,
hashed and read-back form remains the selected provider's native
`provider`/`model`/`format`/`body` contract; equivalent typed and mapping forms
declare the same semantics and survive graph read-back and redeclaration.

| SDK | Jev | Gemini Decisions |
|---|---|---|
| Rust | `Prompt::jev(model, JevPromptOptions { state, questions })` | `Prompt::gemini_decisions(model, GeminiDecisionsOptions { input, questions })` |
| Python | `Prompt.jev(model, *, state, questions)` | `Prompt.gemini_decisions(model, *, input, questions)` |
| TypeScript | `Prompt.jev(model, {state, questions})` | `Prompt.geminiDecisions(model, {input, questions})` |

**REQ-002 — Author typed questions alongside native mappings.** Expose
`NoulQuestion` (instructions, optional criteria), `ChoiceQuestion` (instructions,
required option-to-description criteria map), and `ScoreQuestion` (instructions,
required ordered criteria list), plus their `DecisionQuestion` union. Each
object supplies its own literal type discriminator; its map key is the question
ID. `NoulCriteria` contains `true_description` and `false_description`.

Python exposes Pydantic question models under `wyrd.prompt.decisions` with
unknown-field refusal. Rust exposes declarable structs, an enum and `From`
conversions for heterogeneous maps. TypeScript exposes concrete question classes
with option-object constructors and readonly discriminators, alongside the
discriminated interfaces. Native mappings, typed values and mixed question maps
are accepted directly without caller serialization. Python model imports remain
scoped to the existing optional Pydantic extra; dictionary-only use does not
require it. No duplicate PyO3 question-model family is added.

Shared helpers cover text instructions/criteria. They lower typed Noul criteria
to Jev's `true`/`false` fields or Google's `true_description`/`false_description`;
omitted criteria stays omitted. Native dictionaries retain the selected
provider's contract and are never inferred to belong to another provider or
silently translated. Choice labels and Score order are preserved. Selected
provider limits apply at construction/declaration through shared validation.

**REQ-003 — Bind shared context into the real native request.** Existing check
input references and Prompt placeholder/media binding supply all questions in
one request. Jev receives literal top-level `model`, string `state` and keyed
`questions` at `/v1/systemone`; the gateway resolves model aliases as usual.
String context is inserted verbatim, including quotes/newlines; other JSON
context becomes JSON text without creating new request fields. Gemini receives
top-level `model`, `input` and keyed `questions`. Its input supports native text,
Content and Content-array forms, including media, through existing media-binding
conventions. No second state injection mechanism or native Jev object-state
extension is introduced.

**REQ-004 — Onboard Gemini Decisions through existing provider conventions.**
`provider: google, format: decisions` uses Google's native request/response
types and upstream `POST /v1beta/decisions` at
`generativelanguage.googleapis.com`. The gateway exposes native `/v1/decisions`
with the `decisions` operation/capability through normal deployment configuration,
authentication, permissions, admission, transport/retry, usage and error owners.
Missing capability, incompatible provider, or unsupported tools/generation
settings refuse before an upstream call. The Agent fixed-output path performs
one invocation without streaming or a tool loop. Native answer fields,
including Google's Noul confidence and distinct usage shape, are preserved;
Google responses are not decoded as Jev responses. Provider safety refusals
remain errors rather than failed claims.

Provider-pattern parity is mandatory for implementation and review. Use Jev's
native contracts and provider path as the reference: define Decisions request,
question and nested request contracts through the existing `wire_contract!`
owned/borrowed-view mechanism, with typed native response/answer/usage contracts
and normal ProviderRequest/ProviderResponse dispatch. Reuse borrowed validation
and the existing gateway RequestJson/body-forwarding path so validation does not
copy request text or force an owned JSON tree or serialize/reparse round trip.
Use existing shared transport, clients, retries and typed response handling;
retain provider-specific Google fields and semantics. Review must cite the
corresponding Jev/existing-provider owners and demonstrate that Decisions uses
these throughput patterns. Functional correctness and aggregate benchmark
results alone do not establish this required parity.

### Turning answers into one judgment

**REQ-005 — Declare the policy on the check.** An optional closed
`verdict_transform` union belongs to Agent checks, retaining the existing
`kind: agent` wire shape and fields. It is stored with the versioned Verifier,
carried through authored/compiled reference handling, and available in the
existing declaration, execution and inspection surfaces. It is not an Agent
action field or provider configuration. Each leaf selects an explicit question
key; dispatch uses the actual native response contract, never map order or
caller-supplied JSON paths. With no transform, the existing canonical Verdict
Prompt requirement and execution semantics remain in force.

**REQ-006 — Map Choice labels explicitly.** A `choice` transform has required
`question` and optional `outcomes`. Without a map, declared option labels must
be canonical `passed`, `failed` or `inconclusive`; copy the selected label.
With a map, cover every declared option with a canonical outcome. For example,
`{compliant: passed, violation: failed, unknown: inconclusive}` maps those
custom labels without interpreting their text. Provider confidence is evidence
and never implicitly overrides the mapping.

**REQ-007 — Apply authored probability thresholds.** A `probability` transform
selects a Noul question and requires boolean `expected`, `pass_at` and `fail_at`,
with finite `0 <= fail_at < pass_at <= 1`. For native probability `p`, let
`q = p` when expected is true and `q = 1-p` otherwise. Return passed when
`q >= pass_at`, failed when `q <= fail_at`, otherwise inconclusive. There are
no implicit thresholds or confidence gates. Reasons identify the original
probability, expected polarity and applied policy.

**REQ-008 — Compare Scores on their authored scale.** A `score` transform
requires `question`, a scalar numeric `ComparisonOperator` and numeric
`expected`. Reuse that operator's existing semantics and wire names; passed
means the comparison holds, failed means it does not. Score uses level indices
`0..N-1` for the resolved question's `N` ordered criteria, allowing fractional
values. Thus `operator: greater_than_or_equals, expected: 2` on three levels
passes at 2 and fails at 1.8. Do not normalize scores or infer uncertainty
intervals from confidence.

**REQ-009 — Aggregate selected questions from one invocation.** An `all`
transform contains a nonempty `transforms` list of leaf policies with unique
question keys. Transform each selected answer in memory: any failed wins,
otherwise any inconclusive wins, otherwise passed. The result remains one
requirement verdict. All selected answers are validated even when another
already fails; unselected ancillary answers do not affect the verdict. Only
flat composition is supported, without nested composites, question dependencies
or batching across separate checks.

**REQ-010 — Separate unusable responses from judgments.** Missing selected
answers, wrong answer tags, undeclared Choice labels, nonfinite numbers and
values outside the selected provider's native contract follow the existing
check-validation-error path with an explicit reason. Jev Noul is bounded to
`0..1`; Google Noul retains its native `0.01..0.99` bounds; Score uses the resolved
`0..N-1` scale. Neither an invalid answer nor a provider/infrastructure failure
becomes an accepted pass or a fabricated provider-authored failed verdict.
Existing retry/error handling remains unchanged. A policy's ordinary uncertainty
interval or explicit inconclusive label is an ordinary inconclusive verdict.

### Validating and inspecting the check

**REQ-011 — Detect and correct known mistakes during authoring.** Typed value
construction checks fields/types and self-contained constraints, including
invalid probability bounds and an empty AllTransform. `verifier.validate()`
checks the assembled locally available graph for missing selected questions,
transform/question incompatibility, incomplete Choice maps and duplicate
selected keys. Errors identify the offending field and correction. Authors can
correct the definition, validate again, then declare and execute; no upstream
request is made for an invalid definition.

Keep incremental Card editing: cross-object checks run on assembled validation,
not after every edit. When registered Agent/Prompt references are unavailable
locally, registration resolves and validates them authoritatively before any
provider invocation. Server validation remains authoritative even after local
success; that exception must not defer locally knowable mistakes to execution.
SDK/API errors use existing structured diagnostic conventions. Shared Rust
owners supply behavioral/cross-field validation rather than independent SDK
rule sets; language models supply typing and serialization.

**REQ-012 — Return and retain an explained canonical result.** The transform
returns the existing Verdict/CheckOutput with a factual generated reason stating
the answer and policy; it does not attribute that explanation to the provider.
It supplies empty citations without fabricating evidence anchors; a valid pass
with no citations stays passed under existing citation rules.

Place selected native answer objects, including available confidence and
probabilities, in existing `CheckOutput.actual`. An aggregate captures selected
answers and each question's verdict/reason, and its overall reason identifies
the selected question outcomes. Existing context-capture/hash/redaction settings
govern retained content. Normal result reads, after asynchronous recording is
visible, expose the permitted capture and existing Agent/Prompt/route provenance;
the exact Verifier version identifies the policy. Keeping only an in-process
response handle does not satisfy recording. Do not repurpose `Verdict.data` or
add result tables, detail variants or a second result envelope.

**REQ-013 — Project one contract through public surfaces.** Rust exposes the
typed `VerdictTransform` enum and declarable payloads without requiring JSON
values. Python exposes `ChoiceTransform`, `ProbabilityTransform`,
`ScoreTransform`, `AllTransform` Pydantic models and the discriminated
`VerdictTransform` union under `wyrd.verify`, scoped to its optional extra.
Models have literal kind defaults and are accepted by constructors/setters
without `model_dump`; canonical dictionaries still work without Pydantic.
TypeScript exports the corresponding discriminated interfaces/union with
snake_case wire fields and accepts ordinary object literals. No duplicate
PyO3 transform-model family is added. SDK exports/stubs, schema/codegen,
HTTP/MCP/CLI owners, deployment capability contracts, examples and documentation
project the same native formats and check policy through their existing paths.

**REQ-014 — Preserve existing workflows.** Direct-Verdict Agent checks retain
their outcomes, citations, declared data and errors. Existing Jev native gateway
calls and Google generation continue to work. Question helpers also build
ordinary Agent action Prompts without requiring a verdict policy; this change
does not promise a different action-output representation. Existing tenant/read
permissions govern captured answers, and denial of the new gateway operation
prevents invocation. No new authorization mechanism is introduced.

## Implementation flow and material constraints

Extend existing owners so authoring, validation and execution share one contract:
`wyrd-spec` owns the transform union and a check-specific wrapper reusing and
flattening `AgentCall`; actions retain `AgentCall`. The existing Rust Prompt
builder owns native question lowering, exposed through SDK boundaries; new
PyO3 belongs only in the Python SDK. Skald owns native requests/responses,
rendering, dispatch and the one-call provider path. Carry the existing
`Arc<ProviderResponse>` from `AgentRun.final_response` into Vala's invocation
result. A concrete Vala `VerdictTransformer` with inherent methods applies
policy and returns existing CheckOutput, preserving the typed native response
without a serialize/reparse round trip or duplicate normalized answer model.
The gateway and existing result owners handle admission and recording as usual.

This approach reuses provider decoding Wyrd already owns and keeps SDKs thin.
Its deliberate cost is explicit acceptance policy and a typed native integration
for each provider. It does not offer arbitrary response transforms, JSON-path
rules, a dynamic adapter registry, new check kinds, new dependencies, migrations,
new storage or new combinators. It does not send dynamic Verdict schemas to
decision APIs. Future providers may reuse the facade when their semantics fit;
they are outside this change.

Conversion/aggregation is synchronous in memory, with no additional model call
or PostgreSQL, Scribe, object-storage or Vault wait. Preserve warm-cache behavior,
shared clients/pools and asynchronous result/audit staging. No new durable
write is added to a handler. Existing security, platform, tenancy, retention,
concurrency and retry contracts remain applicable; this change creates no new
coordination or durability boundary.

### SDK and server allocation review

All new SDK/server provider and authoring logic must preserve the existing
performance and memory-allocation practices, including Prompt construction,
attachment to an Agent/check, captured Card definitions, graph validation and
declaration snapshots, binding and invocation. Reuse shared immutable captures,
lazy Prompt/reference derivation and shared native request views where their
existing owners provide them. Do not eagerly materialize or deep-copy Prompts,
schemas, question maps or captured graphs merely to attach a policy or add a
provider. Avoid repeated language-model/dictionary conversion and intermediate
serialization; necessary SDK/wire/storage conversions remain at their existing
boundaries. Preserve snapshot, mutation and reference semantics while reusing
storage. These constraints extend the existing REQ-001/002/004/005/011/013 owner
and provider-pattern obligations to the complete authored-to-executed flow.

Implementation/review evidence must identify the reused allocation/ownership
paths and any added large copies, allocations or serialization, including SDK
managed and native layers where affected. When a change alters those paths or
inspection cannot establish that existing optimizations are preserved, measure
it with the existing profiling/allocation tools and an equivalent before/after
workload on the same host. Exercise the affected construction/attachment and
graph operations with representative Prompt/question sizes and shared reuse;
report allocation volume/bytes and retained memory where the tool can observe
them, along with the tool's sampling/measurement limits and relevant timing.
Explain necessary extra work and address avoidable regressions before acceptance;
do not infer no regression from functional correctness or invent a numeric
budget. Clean verification/gateway benchmarks remain mandatory separately and
cannot substitute for focused authoring measurements when those are needed.
No new benchmark framework or dependency is authorized.

## Acceptance observations

The eight locked journeys below are the requirement-to-proof mapping. Each uses
real public authoring, registration, invocation and result handling:
SDK → WyrdTestServer + repository Postgres → configured gateway → local native
provider mock → SDK. Only the external provider is mocked; inspect its exact
HTTP request. Cover Rust, Python and TypeScript and the HTTP/MCP/CLI boundaries
that ship the changed contracts. Each boundary needs representative successful,
edge and negative flows, without a Cartesian product of every fixture. Pure
arithmetic/validation tests supplement these connected journeys.

| Journey | Obligations | Situation and expected observation |
|---|---|---|
| J1 — Author a Jev judge | REQ-001–003, 005–006, 012–013 | Author a typed Choice Prompt and transform, declare and run. Observe bound top-level state, correct questions/resolved model, then each canonical outcome including a citation-free pass. Typed, mapping and mixed authoring agree; graph read-back/redeclaration retains meaning. Exercise public Rust/TypeScript types, direct Python models and dictionary-only Python without its extra. |
| J2 — Use Gemini Decisions | REQ-001–004, 012–013 | Configure a normal Google Decisions deployment and repeat the workflow. Observe native upstream endpoint/input, usage/confidence conventions and public native gateway delivery. Missing capability refuses before invocation. Verify typed Noul criteria translation, omission, Choice labels, Score order, and supported native media input through real binding. |
| J3 — Declare answer meaning | REQ-006–008, 010, 012 | Hosted checks exercise custom Choice mapping, both probability polarities, inclusive threshold boundaries and middle values, and Score 2 versus 1.8 on three levels. Verdict/reason follows policy; confidence does not change it. Pure tests enumerate boundaries. |
| J4 — Several questions, one verdict | REQ-003, 009–010, 012 | Bind quoted/newline-containing object context into one request with both questions. Exercise the example's three mixtures and failed-over-inconclusive precedence. Inspect selected answers and question verdicts/reasons. Mixed leaf policies, reordered answers and ancillary answers preserve key-based selection; aggregation adds no invocation. |
| J5 — Inspect a recorded result | REQ-005, 012, 014 | Read after asynchronous recording becomes visible; verdict, reason, selected answers and aggregate breakdown agree with the exact version/policy and provenance. Full capture is inspectable; configured hashing/redaction conceals raw content. Execution does not wait for recording. |
| J6 — Correct a mistake while authoring | REQ-002, 005–009, 011, 013 | Invalid typed fields/bounds/empty composition and locally resolvable graph mismatches fail at their specified authoring boundary with field/remediation. Include selected-provider question/criteria limits. Correct, revalidate, declare and execute. Unavailable registered references are checked authoritatively at registration; invalid definitions never invoke upstream. |
| J7 — Unusable answer or denied call | REQ-004, 009–010, 012, 014 | Return a missing answer, wrong tag, unknown label or invalid numeric value; observe explicit existing check-error behavior even alongside a failed aggregate member. Exercise provider refusals/retry conventions. A denied Google operation never reaches upstream; another tenant cannot inspect captured content. |
| J8 — Existing workflows | REQ-001–002, 005, 014 | Existing direct-Verdict checks keep outcomes/citations/data/errors. Existing Google generation and Jev gateway journeys still work. Typed questions construct an ordinary Agent action without policy. Reuse existing proof where it already establishes these behaviors. |

Delivery uses current owner lanes in `mise.toml`/`TESTING.md`, including relevant
Rust/Python/TypeScript verification and gateway journeys, schema/codegen, and
targeted format/lint checks. Planning supplies actual focused commands when test
names exist. Documentation-only spec authoring does not run runtime checks.

For implementation touching verification/gateway hot paths, review must inventory
every added/moved PostgreSQL, Scribe, object-storage or Vault wait against the
sanctioned waits; none is proposed here. It must also provide full clean
`mise run bench:l1:verification` and `mise run bench:l1:gateway` runs with
verdict VALID, compared with the newest same-experiment, same-host records under
`benchmarks/history/`. Smoke, partial and dirty runs do not qualify. Native mocks
prove protocol and policy behavior, not real-model judgment accuracy.

## Approval and revision history

No material design decisions remain open. The maintainer additionally made
REQ-004 provider-pattern parity a hard review requirement: use Jev native typed
contracts and reuse existing provider throughput practices. This clarifies the
approved existing-provider-conventions constraint; it changes no provider
identity, public behavior, task grouping or ownership boundary. The maintainer
also explicitly requires preservation of existing SDK/server authoring allocation
optimizations, especially Prompt, with focused measurement when necessary; the
allocation-review section clarifies that existing performance constraint.

The maintainer approved revision 1 on
2026-11-18 with “i am aligned. Plan out the tasks into logical groupings.”
`$wyrd-plan` may decompose these obligations using the finalized design and
locked journeys. This records approval of the specification and authorization
to plan; implementation, Git staging and commits remain caller-owned.

| Revision | Date | Status | Change |
|---|---|---|---|
| 1 | 2026-11-18 | approved | Maintainer approved and requested logical task groupings. Records locked native verdict transforms, shared context/multiple questions, typed SDK authoring, Gemini Decisions onboarding and J1–J8 proof, including authoring-time correction. Clarified the maintainer’s mandatory Jev-pattern/throughput parity review requirement under REQ-004; clarified allocation/ownership parity and conditional focused measurement across all SDK/server provider and authoring paths. |
