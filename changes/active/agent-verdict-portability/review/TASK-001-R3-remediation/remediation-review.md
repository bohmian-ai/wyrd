# TASK-001-R1 closure discovery — PASS

## Subject, independence and scope

Repository `/home/thorrester/Documents/GitHub/wyrd`; original base
`911a8e278d829ab2722fc20ae881b67481dcf9f3`; prior candidate
`c82414d38561cc3f7e17ac3f3ff52223378abde8`; candidate
`f379283c53f8de823f6295ac273bc62703b8b944`. I reviewed the actual remediation
diff `git diff c82414d38 f379283c5`, all 18 changed paths, and affected source
bodies/callers. HEAD remained the recorded candidate. No source was modified.

This fresh closure reviewer did not implement the candidate. Inputs were
`scope.md`, approved SPEC-agent-verdict-portability revision 2, original TASK-001,
the prior verdict and validated ledger, TASK-001-R1 and its recorded evidence,
root and applicable nested AGENTS, maintainer/Rust/error/surface guidance,
spec-driven workflow, TESTING.md, and owning Prompt/Agent/Verifier architecture.
CodeGraph was consulted before code navigation. The common map was expanded
through render/bind, callback replacement, request derivation, response adapters,
native message extraction, terminal error conversion and Vala retry mapping.

This is a bounded remediation review, not renewed whole-task discovery.
Unchanged portable generation, author data, named-format strictness, slot
isolation, final typed/evidence guards and public journeys carry forward from
the prior source-backed matrix and ledger. I reopened only the request-schema
derivation, refusal boundary, live assertion and changed-item documentation
that the fixes affect. No dependency, shared-state, deployment or persistence
boundary changed, and no separate specialist scope was needed.

## Prior-finding closure

### FIND-TASK-001-1 — CLOSED

`skald-agent/src/loop_runtime.rs:424–432` now derives `ResponseType::of(&request)`
after the before-model replacement chain, immediately before the journaled
dispatch. Both `run_with` and `run_prompt` reach this shared owner. Rendered
variables are already in the template; callback replacements are already in
the request. No cached original response type is supplied through
`RunLoopInputs`. Each answer is therefore checked against the schema of its
actual outgoing request.

`skald-spec/src/prompt.rs:737–787` is the former derivation moved onto
`ResponseType`, rather than a second schema interpreter. Prompt construction
and schema editing use it too. Comparison with the prior function shows the
same Chat/Responses/Anthropic/Gemini extraction and precedence, so existing
schema authoring and native settings are preserved. The loop retains its
compiled `StructuredOutput` when `schema()` equals the newly derived schema,
refreshes it if changed and clears it for text output. This is the existing
invocation-local validator and loop, with no parallel runner or cache owner.

`structured_output_validates_the_bound_schema` exercises `run_prompt`, observes
`enum:["ready"]` in the outgoing request, accepts that value immediately, and
repairs an answer matching only the literal placeholder. The hook-replacement
test exercises `run_with`, rejects an answer matching only the original schema,
accepts the replacement schema's answer, observes the outgoing schema and
correction, and checks the original Prompt's response type is unchanged.
The root independently reran both exact selectors, plus the enclosing lanes.
This proves the diagnosed supported producer-to-consumer mismatch is closed.

### FIND-TASK-001-2 — CLOSED

At `loop_runtime.rs:499–506`, the early terminal branch requires all of:
a structured request, no usable text, and an explicit refusal signal. It
returns `StructuredOutputInvalid` with the fixed diagnostic
`provider refused without an answer` before correction construction or another
iteration. This is the Agent's decision boundary for another model call, so
the guard belongs here rather than in Vala after calls have already occurred.

`ResponseAdapter::refused` (`adapter.rs:171–178`) reads the established typed
Chat refusal field or the adapter's existing `ContentFilter` reduction.
Anthropic Refusal and Gemini safety/blocklist/prohibited-content/SPII finishes
already feed that reduction (`adapter.rs:349–374`); no new provider refusal
taxonomy or transport retry machinery was introduced. Responses whose existing
message extraction fails, such as Google without candidates, still fail there
before this final-answer branch. No-answer Chat/Anthropic paths now stop rather
than entering repair.

The unchanged public error conversion maps this variant to
`WYRD_AGENT_422_STRUCTURED_INVALID`; unchanged `block_error` at
`vala-verify/src/blocks/agent.rs:561–586` preserves the code and classifies it
terminally, so the existing engine inconclusive/no-retry behavior still owns
the result. No old code was restored or new public code added. Updated error,
architecture and reference wording correctly includes an early refusal without
claiming the iteration cap was consumed. Fixed diagnostics never include the
provider refusal text; the test's `s3cr3t` marker is excluded.

`structured_output_does_not_repair_refusals` scripts both explicit Chat and
adapter-classified Anthropic refusals with a valid reply queued behind them
and spare iteration budget. Each stops after one call with the fixed terminal
diagnostic. `check_agent_refusal_is_not_repaired` proves Vala receives a terminal
error containing the stable public code after one call. Root reran both exact
selectors successfully. Existing malformed-then-valid, text-output and provider
failure tests remain in the unchanged surrounding owner and passed in the lanes.

The disclosed lack of a dedicated empty/no-refusal test is a verification
limit, not a retained finding: the new guard requires `refused()`. Empty text
with ordinary Stop and no Chat refusal cannot enter it and reaches the same
`StructuredOutput::check` parse-failure/correction path as other malformed JSON.
The loop budget and correction append/continue code were not otherwise changed.
There is no source-proven regression or missing mandatory per-input test here.

### FIND-TASK-001-3 — CLOSED

Python `assert_verdict` now calls
`jsonschema.Draft202012Validator(schema).validate(json.loads(text))` using the
schema passed by its caller. Both live owners pass the same runtime-generated
schema they send: named non-strict OpenAI uses its response-format schema;
strict OpenAI, Anthropic and Gemini use the local schema sent in their native
request setting. `verdict_format` still generates it through the public Prompt
compiler; no copied schema, dependency or second handwritten validator appears.

The credential-free `test_assert_verdict_validates_against_the_sent_schema`
rejects non-array citations and a string page locator and accepts a valid
Verdict against that generated schema. Root independently reran it successfully.
The remediation's recorded live command reports all four modes passing after
the assertion change, with provider/model/mode evidence. Those live results
are supplied execution evidence, not a rerun by this reviewer. This discharges
the previously absent exact-schema reply assertion without adding semantic
typed-locator or evidence checks to the smoke's scope.

### FIND-TASK-001-4 — CLOSED

I checked the prior inventory against the actual documentation changes:

| Prior inventory / new changed items | Closure evidence |
|---|---|
| `structured_output.rs` registry helper, format, chat_prompt, encodings, text_of, last_user and first valid-answer test | Behavioral rustdoc added; construction/serialization/content/assertion panic contracts supplied where required |
| callbacks RecordingProvider, name field, constructor and Provider::name | Shared recorder/registration behavior documented; the other retained fields are also documented |
| widened `openai_tool_call_response` and `gemini_text_response` | Shared scripted-response role documented |
| RecordingJournal, new, events | Shared in-memory recording, construction and snapshot behavior documented |
| response_schema_edit module | Module intent/invariant documented |
| Vala Recording::send and nested portable assertion helper | Required poisoned-lock/assertion panic contracts added |
| ResponseType::of, ResponseAdapter::refused, StructuredOutput::schema, changed error variants, new tests and sent_schema helper | Meaningful intent/ownership and relevant panic/error contracts present |

The docs preserve all existing implementation and assertions. No new suppressions,
skips, doc checker, helper redesign or unrelated debt cleanup was introduced.
The original mandatory gate is satisfied by static source evidence, not inferred
from green Clippy alone. The prior nonblocking function-import placement
observation is not reopened as remediation work.

## Reconciled acceptance matrix

| Requirement / constraint | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-004/005 / AC-003 exact effective schema and supported hooks | Shared outgoing-request derivation after hooks; same existing derivation owner and validator reuse | Bound-schema and replacement exact tests, enclosing Agent/spec lanes | PASS |
| Refusal non-goal / REQ-005 no repair, fixed terminal error, no outer retry | Signal-gated no-answer branch; adapter reduction; existing public projection/Vala classification | Chat/Anthropic and Vala one-call exact tests; unchanged malformed/text/provider tests | PASS |
| INV-001 / AC-005 safe errors | Fixed refusal diagnostic and unchanged safe structured conversion; no new raw diagnostic log path | Secret marker assertions, existing safe-exhaustion tests in passing lanes | PASS |
| REQ-001 / AC-001 portable structure and typed numeric bounds | Carry forward prior verdict/validation row; remediation changes only assertion-helper rustdoc in schema owner | Prior structure/decode corpus and exact rerun evidence | PASS, carried forward |
| REQ-002/003 / AC-002 author data/references and strictness | Prior source acceptance retained; ResponseType::of preserves extraction logic and does not rewrite requests | Existing spec library tests pass in root's rerun | PASS, carried forward with derivation seam checked |
| REQ-006 / AC-004 / INV-002 slot restrictions and invocation isolation | Existing CallInput.sources/per-call bind/edit remains unchanged; actual sent schema now owns validation | Prior concurrent/Card/no-source proof; Vala seam lane passes | PASS, carried forward |
| REQ-007 / INV-003 final typed/data/evidence/code guards | Guard owners unchanged; no duplicate action validator reintroduced | Prior hosted/typed/data proof; Vala seam lane passes | PASS, carried forward |
| AC-006 real server journeys on four surfaces | Source/public-client coverage unchanged from prior acceptance | Prior supplied focused Rust/Python/TS/MCP Postgres results; full suites reserved for change review | PASS, carried forward |
| AC-007 exact live reply validation | Both live callers pass runtime schema to installed validator | Root offline exact test; supplied updated four-mode live successes | PASS |
| Mandatory changed-item rustdoc | Complete bounded inventory and materially changed remediation items documented | Static source/diff review; supplied format/lint checks | PASS |
| Non-goals, ownership and failure resilience | Same loop/validator/request derivation/response adapter owners; no new settings, persistence or deployment mechanics | Actual 18-file diff and affected caller inspection | PASS |

## Findings, verification and limits

All four prior findings are closed. Proposed new findings: **none**. No
material source-backed correction remains, and no scope expansion or
specification revision is needed. The existing-owner extensions are smaller
and more complete than deleting validation, fixing only Vala, duplicating
schema extraction, or maintaining a handwritten live schema checker.

Root's `verification.md` records 168 distinct passing Rust tests, four exact
Rust selector reruns, one passing offline Python assertion test and a clean
range whitespace check. I ran no Cargo or live commands and changed no source.
Wider lint/typecheck/codegen/docs and all four updated live results remain
recorded implementation evidence. Full journeys and broader live suites were
not rerun. Candidate remained `f379283c53f8de823f6295ac273bc62703b8b944`.

**Overall: PASS**, subject to the required separate independent validation.
