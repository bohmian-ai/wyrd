# Independent remediation validation — TASK-001-R1

Recommendation: **PASS**. The independently validated residual ledger is empty.

## Immutable subject and independence

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Original base: `911a8e278d829ab2722fc20ae881b67481dcf9f3`.
- Prior candidate: `c82414d38561cc3f7e17ac3f3ff52223378abde8`.
- Candidate: `f379283c53f8de823f6295ac273bc62703b8b944`.
- Actual reviewed fix diff: `git diff c82414d38 f379283c5`, all 18 paths.
- Cumulative acceptance context: `git diff 911a8e278...f379283c5`.

This fresh validator did not implement the candidate or author its closure
discovery report. I read the approved revision 2 specification, original
TASK-001, prior verdict and validated findings, TASK-001-R1 including its
completion evidence, `scope.md`, `remediation-review.md` and `verification.md`.
Root/nested AGENTS, workflow, maintainer/Rust/error guidance and the owning
Prompt/Agent/Verifier architecture govern the decision. CodeGraph was consulted
before source navigation. HEAD remained the candidate; source was not changed.

The scope is prior-finding closure and affected paths, not a repeated full
audit. I independently inspected the changed implementations and full affected
bodies, request derivation/binding/hooks, native response extraction and finish
classification, terminal conversion/Vala classification, live assertion callers
and the bounded documentation inventory. The existing source-backed acceptance
of unaffected areas carries forward as identified below. No new blocker was
discovered, so no separate validation of a validator-origin claim is needed.

## Stable finding dispositions

| Stable ID | Prior discovery IDs / validated status | Current disposition |
|---|---|---|
| FIND-TASK-001-1 | BEH-001, SYS-001 / CONFIRMED | CLOSED: validation and dispatch now share the outgoing schema authority. |
| FIND-TASK-001-2 | BEH-002, SYS-002 / REVISED | CLOSED: explicit no-answer refusals terminate before output repair. |
| FIND-TASK-001-3 | BEH-003 / CONFIRMED | CLOSED: each live reply is checked against its sent runtime schema. |
| FIND-TASK-001-4 | REPO-001 / CONFIRMED | CLOSED: mandatory documentation inventory is complete. |

No finding is rejected or renumbered. The prior defects were material; their
source states have been removed by the selected corrections. There are no new
proposed findings and no remaining correction to prescribe.

## Independent closure evidence

### FIND-TASK-001-1 — outgoing schema is the shared authority

`skald-agent/src/loop_runtime.rs:424–432` derives `ResponseType::of(&request)`
after the complete `before_model` chain and before the same request is journaled
and passed to dispatch. Render substitutions are already present in the request
template. The stale original response type was removed from `RunLoopInputs` at
both `run_with` and `run_prompt` call sites. Thus both supported entry points
and Vala's `run_prompt` consumer use the corrected shared loop.

`skald-spec/src/prompt.rs:737–787` moves the existing extraction into
`ResponseType::of`; comparison with the prior function confirms preserved
Chat/Responses/Anthropic/Gemini extraction and Gemini field precedence. Prompt
construction and response-schema editing call that same owner. No parallel
provider schema parser was added. The loop reuses `StructuredOutput` only while
`schema()` equals the outgoing schema, refreshes for a changed schema, and
clears structured state for a text request. Correction fragments therefore
belong to the schema used to judge that response.

The bound-enum test accepts the rendered `ready` value and repairs a literal
placeholder answer, inspecting the actual provider request. The replacement
test rejects the old-schema answer, accepts the hook's schema answer and checks
the Prompt's original response type is unchanged. They cover `run_prompt` and
`run_with` respectively. Root independently reran both exact selectors and the
surrounding spec/Agent lanes successfully. The correction addresses the producer
of stale validation state rather than adding a Vala consumer guard.

### FIND-TASK-001-2 — refusal cannot initiate another repair call

At `loop_runtime.rs:499–506`, the early branch requires a structured request,
empty/whitespace answer text and `response.adapter().refused()`. It returns the
accepted `StructuredOutputInvalid` family with one fixed diagnostic before
correction construction or the next iteration. `ResponseAdapter::refused`
(`adapter.rs:171–178`) reads the existing typed Chat refusal field or existing
`ContentFilter` finish reduction. Anthropic refusal and Gemini safety endings
already map to that atom; this extends the established adapter rather than
introducing another provider taxonomy.

The public error conversion is unchanged and still preserves
`WYRD_AGENT_422_STRUCTURED_INVALID`. Vala's unchanged `block_error` at
`vala-verify/src/blocks/agent.rs:561–586` preserves its code and returns
`BlockError::Terminal`; the existing engine owns inconclusive/no-outer-retry
behavior. HTTP/provider failures still return before structured checking.
Text Agents do not enter the structured guard. The guard belongs to the Agent
because it owns whether another model call is made; a later Vala check would
not prevent the unwanted calls.

The Chat and Anthropic refusal test queues a valid response behind each refusal
with spare budget and asserts one call and fixed secret-free diagnostics. The
Vala test separately asserts the stable code and terminal result after one
provider call. Root reran their exact selectors successfully. Error metadata,
architecture and reference text now describe early refusal without claiming
budget exhaustion; generated error/reference projections match those changes.
No new public code or restored decode code was introduced.

The missing dedicated empty-without-refusal test is a disclosed proof limit,
not a material residual defect. A normal stop with no Chat refusal makes
`refused()` false, so an empty answer reaches the same `StructuredOutput::check`
JSON parse failure and bounded correction branch as other malformed output.
The new guard cannot bypass that branch without its refusal signal; existing
malformed-then-valid recovery tests remain intact. The remediation requires
preserving ordinary repair, not a separate test for every malformed string.

### FIND-TASK-001-3 — live replies receive exact-schema validation

Python `assert_verdict` now uses
`jsonschema.Draft202012Validator(schema).validate(json.loads(text))`. The named
OpenAI caller passes the schema from the same response format sent to the
client; the strict OpenAI, Anthropic and Gemini caller passes the same local
schema object placed in its native request setting. That schema is still built
at runtime through the public Prompt compiler. There is no copied schema,
additional dependency or handwritten parallel schema checker.

The new offline test accepts a valid Verdict and rejects both diagnosed invalid
cases: non-array citations and a string-valued page. Root's exact pytest selector
passed without credentials. Recorded updated live evidence reports all four
modes passing with OpenAI `gpt-6-luna` strict/non-strict, Anthropic
`claude-haiku-5-5` output_config and Gemini `gemini-3.8-flash`
response_json_schema. Those runs were not reproduced by this validator; their
recorded execution plus the inspected exact-schema assertion closes the prior
proof gap. Typed locator/evidence semantics were not added to the smoke scope.

### FIND-TASK-001-4 — required changed-item rustdoc is present

I checked the actual fix diff and surrounding definitions against the complete
prior inventory, including the required panic contracts:

- `structured_output.rs`: registry helper, format/chat Prompt builders,
  encodings, text serialization, last user extraction and first valid-answer
  test; new sent-schema helper and bound/hook/refusal tests.
- `callbacks.rs`: RecordingProvider, its name/new fields and changed
  constructor/name behavior; widened tool-call/Gemini helpers in their owners.
- `session_journal.rs`: RecordingJournal, its field, constructor and snapshot.
- `prompt.rs`: response_schema_edit module and moved `ResponseType::of` owner;
  `adapter.rs`: refused; `structured.rs`: schema accessor and changed docs.
- Vala Recording::send's mutex panic contract and new refusal test; nested
  portable assertion helper's panic contract; changed public/local error docs.

These items now state their workflow role and relevant construction,
serialization, content, lock or assertion panic conditions. All existing
assertions remain. No suppression, doc-check framework or unrelated helper
redesign was added. Static source establishes closure of the mandatory gate;
green Clippy alone was not treated as its proof. Unchanged documentation debt
and the prior nonblocking import-placement observation are outside this round.

## Reconciled acceptance and carried-forward evidence

| Obligation | Source / proof basis | Result |
|---|---|---|
| REQ-004/005 / AC-003 exact effective schema, hooks and repair | Independent closure of FIND-1; exact tests and surrounding Agent/spec lanes in verification.md | PASS |
| Refusal exclusion / REQ-005 terminal classification | Independent closure of FIND-2; signal-gated shared boundary, unchanged Vala mapping and exact refusal tests | PASS |
| INV-001 / AC-005 safe error content | Fixed refusal diagnostic, existing safe structured conversion and retained exhaustion/redaction proofs | PASS |
| AC-007 generated schema and validated live replies | Independent closure of FIND-3; exact offline test and supplied updated four-mode live results | PASS |
| Mandatory changed-item rustdoc | Independent inventory inspection above and supplied format/lint evidence | PASS |
| REQ-001 / AC-001 generated portability and typed bounds | Prior R2 validated matrix and schema proof carry forward; schema owner changes here are assertion documentation only | PASS, carried forward |
| REQ-002/003 / AC-002 authored data/local refs and strictness | Prior R2 source acceptance carries forward; moved derivation checked for equivalence; existing spec library rerun passed | PASS, carried forward with changed seam checked |
| REQ-006 / AC-004 / INV-002 slots and invocation isolation | Prior source/test evidence retained; input-source/bind/edit ownership unchanged, actual outgoing schema now validates; Vala seam rerun passed | PASS, carried forward |
| REQ-007 / INV-003 final typed/data/evidence/code guards | Prior acceptance retained; owning guards unchanged; Vala seam rerun passed | PASS, carried forward |
| AC-006 Rust/Python/TS/MCP real-server journeys | Prior source-backed four-surface coverage and supplied focused Postgres journey results unchanged | PASS, carried forward |
| Non-goals, ownership and resilience | Actual 18-path diff extends existing loop/derivation/adapter/installed validator, without new runner/settings/persistence/deployment or resource owner | PASS |

The changed dependencies and authority do not invalidate these carried-forward
rows. No previously accepted unchanged area was reopened without cause.
Corrections preserve adjacent behavior and are the smallest complete extensions
of existing owners; deleting validation, duplicating schema extraction or adding
downstream guards would not preserve the required task.

## Final ledger, follow-up and limits

**Validated residual ledger: `[]`.** All four stable findings are closed;
there are no independently retained new findings or remediation recommendations.
No material conflict or untraced failing path requires focused follow-up. No
new public/architecture/ownership/persistence decision requires spec revision.
This is the first remediation review, so no convergence assessment is required.

Root's verification.md records 168 distinct passing Rust tests, four exact Rust
selector reruns and one passing offline Python test, plus clean range whitespace.
I ran no Cargo or live commands and changed only this report. Broader
format/lint/typecheck/codegen/docs and four live successes remain supplied
evidence. Full journey suites and provider calls were not rerun here. The
empty-no-refusal dedicated-test limit is explicitly assessed above and does
not prevent acceptance.

**Recommendation: PASS.**
