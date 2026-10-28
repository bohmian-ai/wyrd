---
id: SPEC-agent-verdict-portability
revision: 2
status: approved
---

# Portable Verdicts and bounded structured-output repair

Agent judges should reach the model with a provider-compatible Verdict schema,
and a model that returns invalid structured output should receive a chance to
correct it within its existing Agent budget. Today OpenAI and Anthropic can
refuse the generated schema, while a schema-invalid JSON object can pass through
Skald and fail terminally in Vala after the conversation ends.

For example, a judge returning `{"outcome":"FAIL","reason":"policy violated",
"citations":[]}` currently ends inconclusive immediately. With this change,
the same run tells the judge that `outcome` must be `passed`, `failed`, or
`inconclusive`. A corrected, otherwise valid `failed` Verdict is accepted. If
the judge never corrects its reply, the requirement remains inconclusive with
a stable error code; invalid output never becomes a judgment of the subject.

This change extends the existing owners: Wyrd generates Verdict schemas, Skald
compiles Prompts and owns Agent conversations, and Vala owns verification and
evidence validation. It adds no retry setting or second orchestration layer.
Repair increases token use and latency within the existing iteration and time
limits. Arbitrary author schemas remain subject to provider restrictions.

## Validated findings and limits of the evidence

Research performed on 2026-10-28 against the current working tree:

| Claim | Assessment |
|---|---|
| Generated schema contains problematic constructs | Confirmed in `Verdict::response_format` and the supplied original schema: outcome uses described `oneOf`/`const` alternatives, locators use `oneOf`, numeric schemas carry `minimum`/`format`, and references have description siblings. |
| OpenAI named formats are always strict | True at committed baseline. The existing uncommitted `authoring.rs` change makes portable Chat and native Chat/Responses named formats non-strict while preserving inline-schema behavior. All 101 `wyrd-skald-spec` library tests passed during this research, including the exact named-format regression. |
| OpenAI Luna probes support non-strict named formats | Supplied live results: `gpt-6-luna` accepted the same portable plain Verdict schema in strict mode that passed on `gpt-4o`, Anthropic Sonnet 5.5 and Gemini 3.8 Flash. A separate strict-mode request with an author `data` schema containing an optional field returned HTTP 400. Plain-envelope portability therefore does not make arbitrary author schemas strict-compatible; this is the reason REQ-003 keeps all named formats non-strict, with or without `data`. These calls were not independently rerun here. |
| No-source citation restrictions are portable | Supplied live results contradict this: an empty slot enum made strict `gpt-6-luna` hit its output limit, Anthropic reject the request because enums must be non-empty, and Gemini 3.8 Flash return a citation anyway. `citations.maxItems: 0` worked on Luna and Gemini but Anthropic rejected `maxItems`. A non-empty reachable-slot enum worked on all three. REQ-006 therefore leaves citation schemas unchanged when no source is reachable and relies on final evidence validation. |
| Slot description encourages `inputs.order` | The ambiguous description is confirmed. The reported provider frequencies are supplied probe evidence, not independently reproduced here. Additionally, `SlotName` decoding rejects dotted identifiers: schema acceptance alone does not establish typed Verdict acceptance. |
| Invalid output cannot be repaired | Confirmed: `parse_structured_output` parses an object only; Vala actions use `conforms`, checks use `verdict_from`, and `block_error` treats decode errors as terminal despite the gateway classifier considering them retryable. |
| Validation diagnostics differ | Confirmed: `conforms` formats the validator error and instance path; `verdict_from` exposes only the schema path. Saved `probe_errors.txt` confirms nested union errors contain useful detail absent from the summary string. |
| Tests never validate structured output | Too broad. Skald has structured-output tests, and live Prompt/gateway tests already cover generic schemas. They do not establish compatibility of the generated Verdict or correction after schema-invalid JSON. Existing building-block journeys include negative schema cases as well as valid scripted judges. |
| One portable schema passed three providers | The saved `p_openai_v2.json`, `p_anthropic_v2.json`, and `p_gemini_v2.json` contain structurally identical schemas. The reported successful live calls and model-specific counts are supplied evidence; no live calls were rerun for this draft. Custom `data` was not covered by those probes. |

The scratchpad files were inspected under
`/tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-docs/1f0fa5f0-d6d9-4e45-a989-4fd9780eab38/scratchpad/`.
They are transient research evidence, not build inputs or repository fixtures.

Official provider documentation supports using a restricted schema projection:
[OpenAI](https://developers.openai.com/api/docs/guides/structured-outputs)
requires all properties to be required and closed objects in strict mode;
[Anthropic](https://platform.claude.com/docs/en/build-with-claude/structured-outputs)
documents unsupported numeric constraints and schema complexity limits;
[Gemini](https://ai.google.dev/gemini-api/docs/structured-output)
documents a schema subset including enums and `anyOf`, and recommends application
validation. These documents support the approach, not the supplied per-model
success rates or a universal compatibility guarantee.

## Required behavior

- **REQ-001 — Portable generated envelope.** `Verdict::response_format` produces
  the structural schema below for plain Verdicts. Outcome is a string enum;
  locator is the same five disjoint closed shapes under `anyOf`; generated
  schema nodes contain no `oneOf`, `const`, `minimum`, `format`, or siblings of
  `$ref`. Preserve canonical Rust types and their rustdoc. Titles and
  descriptions on non-reference nodes may remain. This is a provider-facing
  projection, not a change to serialized Verdict values.
- **REQ-002 — Preserve author schemas.** An expanded Verdict format still
  requires `data` matching its authored object schema. Apply portability changes
  before embedding that resource; preserve authored constraints, local reference
  semantics and the existing private resource identity. Unsupported author
  keywords may still cause a provider refusal and an inconclusive check.
- **REQ-003 — OpenAI policy.** Every named format uses `strict: false` on OpenAI
  Chat Completions and Responses, for portable and native authoring where
  supported. Portable inline schemas retain strict mode; explicitly authored
  native settings retain their existing behavior. Do not make strictness depend
  on whether Verdict declares `data`.
- **REQ-004 — Validate and repair in Skald.** Every structured-output Agent
  conversation validates each proposed final answer against the exact effective
  schema sent in that invocation. Malformed JSON, non-object output and schema
  violations receive correction turns in the same conversation. Retain the
  offending assistant reply and send a correction describing up to ten
  violations, their locations, broken rules and relevant schema fragments.
  Union failures must explain allowed shapes, not just repeat the generic union
  failure. The correction may reference only that reply and its schema, not
  fetch or introduce other evidence. Bound correction text to 16 KiB of UTF-8;
  clearly mark omitted diagnostics or clipped schema fragments.
- **REQ-005 — Existing budget and failure contract.** Every repair model call
  spends an iteration from `run_config.max_iterations`, uses the effective
  execution route, and receives ordinary iteration/model spans and journal
  events. Existing deadlines, cancellation, hooks, tool restrictions and failure
  classification still apply. Zero iterations makes no provider call and keeps
  its existing max-iterations error. A final invalid answer with no remaining
  iteration returns `AgentError::StructuredOutputInvalid { agent, violations }`;
  its public projection is HTTP status 422 and stable code
  `WYRD_AGENT_422_STRUCTURED_INVALID`, with bounded schema-location diagnostics.
  Exhaustion after tool-only turns keeps the existing max-iterations behavior.
  Vala reports the new failure as terminal/inconclusive, preserving its stable
  code in the requirement reason and spending no outer block retry on it.
- **REQ-006 — Invocation-specific citation slots.** Before invoking an Agent
  check with reachable evidence, restrict `citations[].slot` to original evidence
  source slots reachable through that call's inputs and workspace. Use bare slot
  names, including fixed attachment slots. Restrict the effective schema both in
  the provider request and local validator. With no reachable source, leave the
  citation schema unchanged: do not add an empty slot enum or forbid citations
  in the schema. Final evidence validation still makes a pass without a valid
  citation inconclusive. Wyrd retains
  evidence/locator validation and the rule that a pass needs a valid citation.
  Clarify `SlotName` rustdoc with a bare-name example and no `inputs.` prefix.
- **REQ-007 — Retain final verification guards.** Vala actions rely on Skald's
  schema validation rather than `conforms`. `verdict_from` remains for Agent and
  code checks, preserving typed decoding, numeric bounds, data size/depth
  limits, and secret-free refusals. Code checks have no model repair. A reply
  that satisfies the portable schema but fails typed decoding or evidence
  validation remains inconclusive; extending repair to these semantic failures
  is outside this change.

### Plain Verdict structural target

This is the exact structural target supplied by the maintainer. The full tested
payload also retains `$schema`, title and definition descriptions; those
annotations are permitted, except beside `$ref`. Do not commit a generated
schema artifact: tests compare runtime-generated structure with this contract.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "properties": {
    "outcome": { "$ref": "#/$defs/VerificationVerdict" },
    "reason": { "type": "string" },
    "citations": { "type": "array", "items": { "$ref": "#/$defs/Citation" } }
  },
  "additionalProperties": false,
  "required": ["outcome", "reason", "citations"],
  "$defs": {
    "VerificationVerdict": { "type": "string", "enum": ["passed", "failed", "inconclusive"] },
    "Citation": {
      "type": "object",
      "properties": {
        "slot": { "$ref": "#/$defs/SlotName" },
        "locator": { "$ref": "#/$defs/Locator" }
      },
      "additionalProperties": false,
      "required": ["slot", "locator"]
    },
    "SlotName": { "type": "string" },
    "Locator": {
      "anyOf": [
        { "type": "object", "properties": { "page": { "type": "integer" } }, "required": ["page"], "additionalProperties": false },
        { "type": "object", "properties": { "slide": { "type": "integer" } }, "required": ["slide"], "additionalProperties": false },
        {
          "type": "object",
          "properties": { "lines": {
            "type": "object",
            "properties": { "start": { "type": "integer" }, "end": { "type": "integer" } },
            "required": ["start", "end"], "additionalProperties": false
          } },
          "required": ["lines"], "additionalProperties": false
        },
        { "type": "object", "properties": { "json_pointer": { "type": "string" } }, "required": ["json_pointer"], "additionalProperties": false },
        {
          "type": "object",
          "properties": { "region": {
            "type": "object",
            "properties": {
              "page": { "type": "integer" }, "x": { "type": "number" }, "y": { "type": "number" },
              "width": { "type": "number" }, "height": { "type": "number" }
            },
            "required": ["page", "x", "y", "width", "height"], "additionalProperties": false
          } },
          "required": ["region"], "additionalProperties": false
        }
      ]
    }
  }
}
```

## Invariants and boundaries

- **INV-001 — No error-content leakage.** Stored/displayed validation errors,
  public details and Vala reasons expose schema locations or fixed diagnostics,
  never reply values, arbitrary reply dictionary keys or raw parser text.
  Detailed correction text is provider conversation content, governed by the
  existing sensitive prompt/response capture policy; it must not be copied into
  error events or ordinary logs. This does not prohibit existing authorized
  sensitive conversation storage.
- **INV-002 — Invocation isolation.** Narrowing never mutates registered Prompt
  or Agent Cards, authored serialization, hashes, or another concurrent call.
  Skald remains unaware of the name `verdict`; Wyrd supplies the resolved schema.
- **INV-003 — No weakened acceptance.** Outcome values, closed objects, mutually
  exclusive locator shapes, author data constraints, typed bounds and evidence
  checks remain enforced at their owning boundary. Removing numeric annotations
  moves some rejection to decoding; it does not authorize invalid locators.

Scope includes generated envelope, named-format lowering, bounded Agent repair,
Vala integration, error catalog/projections, regression journeys and live
Verdict smoke coverage. No new public retry setting, author-schema rewriting,
provider compatibility analyzer, external schema fetching, deployment changes,
automatic repair of code-check output, or Bifrost storage changes are included.
Existing provider refusals, tool-only iteration exhaustion and timeouts retain
their existing classifications. Provider refusal must not trigger model-output
repair when no usable answer was returned.

The new public error variant/code and the shared Agent iteration semantics are
the expensive-to-reverse decisions. Record these, the portability projection,
and invocation-specific citation behavior in the owning Prompt, Agent and
Verifier sections of the architecture during implementation. No new tenant,
authorization or persistence boundary is introduced.

## Acceptance and evidence

| ID / obligations | Situation and required observation |
|---|---|
| AC-001 / REQ-001, INV-003 | Runtime schema matches the structural target modulo permitted annotations. Every locator shape and outcome round-trips; missing/extra fields, wrong outcomes and multiple locator keys fail. Negative and overflowing unsigned values fail final decoding even if the portable schema accepts them. |
| AC-002 / REQ-002, REQ-003 | Plain/expanded named formats lower correctly across portable Chat and native Chat/Responses. Inline strict behavior stays intact. Custom optional fields and local references retain their authored meaning; generated normalization never edits their keywords. |
| AC-003 / REQ-004, REQ-005 | Scripted providers return malformed JSON, a non-object, an invalid enum, invalid union shapes and multiple violations before a corrected answer. The same Agent run repairs them; requests contain bounded actionable feedback and spans account for each call. Permanent invalidity terminates at the configured cap with the new stable code. Zero/one-iteration, mixed tool/repair, timeout and cancellation cases remain bounded. Cover Chat, Responses, Anthropic and Gemini conversation encodings. |
| AC-004 / REQ-006, INV-002 | With reachable evidence, judges citing a prefixed, unrelated or scratch slot receive repair; original input and fixed attachment slots succeed. Parallel invocations with different source sets send and validate different slot restrictions without mutating stored cards. With no reachable source, the citation schema stays unchanged and final evidence validation prevents a pass without a valid citation. |
| AC-005 / REQ-007, INV-001, INV-003 | Unique reply values and attacker-chosen dictionary keys never appear in returned/stored errors or ordinary logs, including parser and nested union failures. Agent actions no longer leak validator output; code checks still reject malformed Verdicts and excessive data without repair. |
| AC-006 / REQ-004–REQ-007 | Real SDK → WyrdTestServer + repository Postgres → SDK journeys demonstrate invalid-then-valid recovery and permanent-invalid inconclusive results. Extend the existing Rust, Python, TypeScript and MCP/HTTP verification owners with focused success/failure assertions on their shipped surfaces; public code projections agree. |
| AC-007 / REQ-001–REQ-003 | Opt-in live lane sends the runtime-generated plain schema through native OpenAI strict mode, Anthropic and Gemini and validates each reply. Separately exercise named OpenAI non-strict production lowering. No hand-copied probe schema may substitute. Report provider/model/request mode and explicit skips; all three providers must run successfully before claiming live compatibility. Custom-data portability remains a documented limit. |

## Approval and revision history

The maintainer explicitly approved revision 2 as a whole on 2026-10-28,
including the 16 KiB diagnostic bound, error name and tool-only exhaustion rule.
No open material decision is deferred to implementation.

- Revision 2, 2026-10-28: leave the citation schema unchanged for checks with no
  reachable source; add supplied Luna portability, optional-data strict-mode
  rejection and no-source probe results; record the three accepted decisions.
- Revision 1, 2026-10-28: draft based on repository tracing, provider documentation,
  supplied probes and the maintainer's exact portable schema.

Authority: [Doctrine and Prompt/Agent contracts](../../../architecture/wyrd-design.md#prompt),
[Verifier](../../../architecture/wyrd-design.md#verifier),
[workflow](../../spec-driven-development.md),
[error policy](../../../agent_docs/errors.md),
[agent surfaces](../../../agent_docs/agent-surfaces.md).
