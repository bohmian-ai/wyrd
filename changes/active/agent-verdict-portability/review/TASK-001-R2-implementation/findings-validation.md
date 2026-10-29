# Independent findings validation — TASK-001

Recommendation: **FIX_REQUIRED**. Four deduplicated findings remain.

## Subject, independence and inputs

Repository `/home/thorrester/Documents/GitHub/wyrd`; base
`911a8e278d829ab2722fc20ae881b67481dcf9f3`; candidate
`c82414d38561cc3f7e17ac3f3ff52223378abde8`.
HEAD still identified that candidate during validation. The first full review
uses `git diff 911a8e278...c82414d38`; the earlier R1 authority-only BLOCKED
attempt supplies no acceptance or finding closure evidence.

This validator neither implemented the candidate nor authored the discovery
reports. Inputs were `scope.md`, approved specification revision 2, original
TASK-001, `task-review-behavior.md`, `standards-review.md`, `system-review.md`,
and `verification.md`. Root/nested rules, maintainer/Rust/error guidance,
workflow and the owning Prompt/Agent/Verifier architecture govern the claims.
CodeGraph was consulted before independent source navigation.

The validation scope is the proposed defects and their correction boundaries,
not another comprehensive discovery pass. I inspected the actual changed code,
full affected validation/helper bodies, shared render/request/callback owners,
native response adapters and dispatch, Vala consumers, baseline error behavior,
and both live assertion callers. No new independent blocker was discovered.

## Proposal dispositions

| Discovery IDs | Disposition | Retained ID | Reason |
|---|---|---|---|
| BEH-001, SYS-001 | CONFIRMED; one common cause | FIND-TASK-001-1 | Effective request schema and compiled validator have different producers. Supported rendering and request replacement reach the mismatch. |
| BEH-002, SYS-002 | REVISED; one common cause | FIND-TASK-001-2 | No-answer native refusals demonstrably enter repair. Correction is limited to excluding those refusals, preserving terminal/inconclusive behavior with the approved replacement error family; no new public refusal contract is needed. |
| BEH-003 | CONFIRMED | FIND-TASK-001-3 | The required live proof can pass responses that violate the exact schema it sends. |
| REPO-001 | CONFIRMED | FIND-TASK-001-4 | The changed-item rustdoc inventory contains explicit mandatory merge-gate failures. |

No proposal is rejected. Agreement on the first two claims was not used as
proof: the paths below were independently traced. No focused follow-up is
needed because source and approved authority resolve their substance and
correction boundaries; the differing Chat/Anthropic refusal examples are
sibling manifestations of one cause.

## Validated ledger

### FIND-TASK-001-1 — effective request and validator use different schemas

- Discovery IDs: BEH-001, SYS-001. Status: **CONFIRMED**.
- Classification: **INCORRECT**.
- Violated obligation: REQ-004 exact effective schema; REQ-005 existing hooks;
  AC-003. Prompt architecture derives response shape from the native request.
- Exact changed location: `crates/skald/skald-agent/src/loop_runtime.rs:378–383`
  and `:496`, with the stale inputs supplied at `:152` and `:254`.

**Source proof and affected paths.** `skald-prompt/src/prompt.rs:84–86`
delegates rendering to `skald_spec::Prompt::render`. That owner
(`skald-spec/src/prompt.rs:607–632`) clones the Prompt and calls `bind_mut`;
`bind_mut:519–523` substitutes string leaves throughout the native request.
The returned request therefore includes substituted enum/constant values in
its schema, while the original Prompt's cached `response_type` remains
unchanged. `run_prompt` passes the rendered template alongside that original
cache. `run_loop` compiles the cache once, and later checks output against it.
The established `before_model` replacement chain at `loop_runtime.rs:402–428`
can independently change the schema in the native request dispatched at `:433`.
That replacement also never updates the validator. `run_with` and `run_prompt`
share this owner; Vala binds input variables and then calls `run_prompt`
(`vala-verify/src/blocks/agent.rs:282–318,393–400`).

A supported object schema with `answer.enum: ["${expected}"]`, rendered with
`expected=ready`, sends `enum:["ready"]`. A reply `{"answer":"ready"}` is
valid for the sent request and is rejected by the candidate against the literal
placeholder. Repair continues with contradictory schema guidance until the
budget runs out. A schema-changing request callback produces the same error
without variables. Conversely an answer matching only the stale schema can
pass local Agent validation. These are live public Agent workflows, not an
unreachable helper or an exotic additional schema feature.

**Consequence and materiality.** Valid replies can exhaust into inconclusive
results and incur unnecessary calls; local structured acceptance can disagree
with the task's central exact-schema promise.

**Decision-complete correction and burden.** Use the effective outgoing native
request as the authority for the validation associated with that response,
after rendering and request callbacks. Reuse the existing
`skald-spec::response_type_of` derivation owner instead of adding a parallel
provider-specific schema extractor in the Agent or Vala layer. Retain one
compiled validator while its effective schema is unchanged; a changed schema
must refresh the validator and correction fragments. The private mechanics
remain implementation-owned. Deleting validation, banning existing binding or
hook behavior, or fixing Vala alone would not close the shared defect. This is
a bounded owner correction, without a public setting or new orchestration.
Do not alter request routing, tool/session lifetime, budgets or Card identity.

**Focused closure proof.** Extend the existing Agent structured-output owner
with a bound schema enum and a schema-changing `before_model` replacement.
Observe the actual recorded request and verify that its valid answer succeeds;
an answer valid only for the stale schema must be refused/repaired. Exercise
both Agent entry paths where applicable to the shared fix, with call counts
and existing cap/callback/encoding regressions. No full journey aggregate is
needed for this local correction.

### FIND-TASK-001-2 — a no-answer provider refusal triggers model-output repair

- Discovery IDs: BEH-002, SYS-002. Status: **REVISED**.
- Classification: **VIOLATION**.
- Violated obligation: approved spec Invariants and boundaries explicitly
  excludes repair after provider refusal with no usable answer; REQ-005 keeps
  existing failure classification.
- Exact changed location: `crates/skald/skald-agent/src/loop_runtime.rs:489–516`,
  especially the default-empty text and unconditional structured check.

**Source proof and siblings.** Native OpenAI `send_native`
(`skald-providers/src/clients/openai.rs:453–466`) returns the successfully
decoded Chat response. `skald-runtime/src/dispatch.rs:47–58` returns it without
refusal classification. The Chat wire message has a typed `refusal` field
(`skald-spec/src/wire/openai_chat.rs:403–414`); a response with a refusal,
`content:null`, and no tools is retained by `assistant_message`
(`skald-agent/src/request_builder.rs:148–160`). Its text adapter
(`skald-spec/src/adapter.rs:220–225`) yields None. The loop turns None into an
empty string, rejects it as malformed JSON and, with budget left, appends a
correction and calls again. Native Anthropic similarly returns the decoded
message (`clients/anthropic.rs:109–122`); its Refusal finish reason already maps
to `FinishReason::ContentFilter` in the shared adapter at `:336–345`.
Gemini safety endings are sibling adapter values at `:350–365`; any path that
already exits for absent candidates must retain its existing behavior.
Transport/status errors exit at `loop_runtime.rs:452` before repair and are
outside this defect.

At the recorded base, `parse_structured_output` returned the old decode error
immediately for this empty output, without another model call. Baseline Vala
`block_error` treated that non-provider failure as terminal. Candidate Vala
still maps the replacement structured-invalid error terminally; the unwanted
new behavior is the repair before that mapping. Both public Agent entry
points and Vala's invocation consume the shared loop.

**Consequence and materiality.** A genuine refusal now causes additional paid
calls instructing the model to answer again, and a later valid scripted reply
can become a judgment. This directly violates an explicit approved exclusion.

**Decision-complete correction and burden.** At the shared Agent final-response
boundary, exclude explicit native refusals with no usable answer from repair.
Reuse typed response fields and the existing shared response adapter's finish
classification; do not infer refusal from arbitrary malformed/empty text and
do not add a provider compatibility classifier. Preserve immediate terminal
failure and Vala inconclusive behavior through the approved replacement
`StructuredOutputInvalid`/`WYRD_AGENT_422_STRUCTURED_INVALID` projection with
fixed safe diagnostics. No restored decode code or new public refusal code is
required: the approved replacement supersedes the old no-object failure family,
and the explicit refusal exception prevents spending the repair budget.
The spec requires this error for exhaustion but does not say it is exclusive
to exhaustion. Update directly affected documentation if it currently implies
otherwise; do not claim that a refusal consumed all iterations. Preserve HTTP
failure classification, ordinary malformed-answer repair, text Agent behavior,
and no outer block retry. A consumer guard belongs here because the Agent owns
the decision to generate another model call; a Vala-only guard is too late.

**Focused closure proof.** Script refusal-only native responses with budget
greater than one and a valid response queued behind them. Assert one call,
no correction, fixed secret-free terminal error and Vala inconclusive/no-retry
mapping. Cover the distinct established representations needed by the shared
check (explicit Chat refusal and adapter-classified refusal), retaining a
malformed-answer-then-valid case proving repair still works. Reuse existing
provider/journal/Vala seams; no new journey server or refusal code is needed.

### FIND-TASK-001-3 — live compatibility assertion accepts schema-invalid replies

- Discovery ID: BEH-003. Status: **CONFIRMED**.
- Classification: **MISSING**.
- Violated obligation: AC-007 and original task Scenario 6 require validating
  every live reply against the runtime-generated schema.
- Exact changed location:
  `sdks/wyrd-sdk-python/tests/smoke/test_gateway_live_providers.py:103–113`,
  with live callers at `:425` and `:478`.

**Source proof and reachability.** `verdict_format:85–100` correctly generates
the schema via public `Prompt.load(...).render()`. Both newly added smoke
owners send that runtime schema, then call `assert_verdict` without it. The
assertion checks envelope names, outcome/reason and locator key spelling but
neither array type nor locator value/nested closed shape. I independently ran
the unchanged assertion extracted with `ast.parse` under `mise exec -- python3`:
both `citations:{}` and `citations:[{"slot":"answer","locator":{"page":
"not-an-integer"}}]` were accepted. The generated schema requires an array
and an integer page respectively. This reproducer changed no file or provider.

**Consequence and materiality.** All four required modes can report live
success for a schema-invalid response; non-strict OpenAI especially does not
promise server enforcement. Existing Rust structure tests validate generation,
not the actual live replies. This is an explicit required proof gap, not a
request for typed locator/evidence semantic checks.

**Decision-complete correction and burden.** Validate each parsed live reply
against the exact schema already obtained for that request using the installed
Python JSON Schema implementation (`jsonschema` is in the SDK lockfile),
replacing the handwritten partial schema checker. Preserve strict/non-strict
request assertions, native client modes and provider/model evidence. Do not
copy the schema, add a dependency or test harness, or add durable SDK behavior.
The helper/call-site change is small; deleting reply validation or extending a
handwritten parallel schema checker is insufficient.

**Focused closure proof.** A credential-free assertion check must reject the
two invalid examples and accept a valid Verdict against the generated schema.
Then run only the existing four focused live Verdict cases and record each
provider/model/mode and successful schema validation (or explicit missing
evidence). No full live gateway or journey aggregate is required.

### FIND-TASK-001-4 — changed Rust items miss the mandatory rustdoc gate

- Discovery ID: REPO-001. Status: **CONFIRMED**.
- Classification: **VIOLATION**.
- Violated obligation: root AGENTS requires rustdoc on every new/changed Rust
  item and says missing rustdoc blocks merge; `agent_docs/rust.md` includes
  private/test helpers and panic contracts. This is an explicit mandatory
  gate, not a subjective style preference.

I checked the actual diff and surrounding definitions for this bounded
inventory. Paths below are relative to `crates/`:

| Location / item | Required correction |
|---|---|
| `skald/skald-agent/tests/integration/structured_output.rs:32` `registry_returning_text` | Add missing behavioral rustdoc for the changed queued-answer helper. |
| Same file `:84` `format`, `:88` `chat_prompt` | Add missing rustdoc and panic contracts for new fallible-construction helpers using expect. |
| Same file `:138` `encodings`, `:192` `text_of`, `:197` `last_user` | Complete panic contracts for Prompt construction, request serialization and unsupported/missing content respectively. |
| Same file `:224` `structured_output_parses_when_schema_declared` | Document changed test behavior and its assertion/failure panic contract. |
| `skald/skald-agent/tests/integration/callbacks.rs:346–347` `RecordingProvider`, `name` | Document widened owner and new field. |
| Same file `:353` `RecordingProvider::new`, `:399` `Provider::name` | Document changed default-registration and configured-name behavior. |
| `skald/skald-agent/tests/integration/loop_basic.rs:78` `openai_tool_call_response` | Document widened helper used by the new repair proof. |
| `skald/skald-agent/tests/integration/loop_gemini.rs:50` `gemini_text_response` | Document widened helper used by the new repair proof. |
| `skald/skald-agent/tests/integration/session_journal.rs:617,622,626` `RecordingJournal`, `new`, `events` | Document widened recorder, construction and snapshot operations. |
| `skald/skald-spec/src/prompt.rs:2186` `response_schema_edit` | Add missing new-module rustdoc. |
| `vala/vala-verify/tests/integration/blocks_agent.rs:824` `Recording::send` | Add panic contract for the mutex expect. |
| `wyrd-spec/src/verification/execution.rs:1091` nested assertion helper `portable` | Add panic contract for the assertions. |

**Materiality and minimum correction.** These items are changed or newly
consumed proof owners, not unrelated untouched debt. Static source shows the
missing docs even though Clippy passes. Add concise intent/invariant and panic
documentation in the same remediation as the behavioral corrections. Preserve
all assertions and helper behavior; do not delete tests or introduce a new doc
checker. This is documentation-only burden, and no new behavior test is needed.
The function-import placement observation in the standards report is not part
of the retained ledger and creates no remediation obligation.

## Reconciled acceptance and existing-owner assessment

| Obligation | Validated evidence / limitation | Result |
|---|---|---|
| REQ-001 / AC-001 portable generation, canonical values and typed bounds | Discovery source matrix, inline runtime structure/negative corpus and root's exact schema test reruns; no claim of a universal provider guarantee | PASS |
| REQ-002/003 / AC-002 author data/ref semantics and named/inline policy | Existing response-format owner projects before embedding data; authoring tests and root's exact regressions | PASS |
| REQ-004/005 / AC-003 repair, same conversation, exact schema, budgets and exclusions | Ordinary repair/encoding/budget evidence is credible; FIND-1 and FIND-2 leave exact-schema/refusal obligations unsatisfied | FAIL |
| REQ-006 / AC-004 / INV-002 slots and invocation isolation | Vala reuses CallInput.sources, clone/bind/edit owner; stored Card and concurrent requests tested, no-source schema unchanged | PASS |
| REQ-007 / AC-005 / INV-001/003 final guards and safe errors | Action duplicate removed; unchanged typed/data/evidence guards and structured safe-location error ownership remain; no new leaking log path in the correction scope | PASS |
| AC-006 four public real-server surfaces | Complete selected source coverage and recorded focused Postgres journey evidence; wider suites deferred to change review | PASS with supplied execution evidence |
| AC-007 runtime generation plus schema-valid live replies | Generation and request modes are present; FIND-3 prevents accepting the reply-validity proof | FAIL |
| Root mandatory rustdoc rule | FIND-4 inventory verified against source and diff | FAIL |
| Non-goals / ownership / resilience | New compiled validator belongs with the existing Agent loop and installed validator; removed Vala conforms cannot own shared conversation repair. No duplicate runner, persistence owner, schema artifact, author rewrite or external fetch is justified/introduced | PASS outside retained defects |

No expensive-to-reverse product, API, authorization, storage, concurrency or
resource-ownership decision is needed by the corrections. Refusal exclusion is
already approved, and its replacement terminal error can use the accepted code.
Do not reinterpret the narrow remediation as permission to redesign provider
classification or rewrite author schemas. No SPEC_REVISION_REQUIRED is needed.

## Verification and review limits

Root independently executed the 303 owner tests and five exact selectors in
`verification.md`; all passed. I did not run Cargo, change source or call live
providers. I independently reproduced the Python live assertion defect without
credentials or file edits. The packet's wider checks, hosted guards, focused
Postgres journeys and four live passes remain supplied evidence; full journey
aggregates run at integrated change review. Passing tests do not exercise the
two retained runtime failures or supply missing rustdoc/schema assertions.

No prior implementation findings existed to close, and this is not an
unsuccessful remediation round. The next review should cover FIND-1 through
FIND-4 closure, the fix diff and affected callers/consumers; carry forward the
unaffected matrix evidence unless the fix changes it.
