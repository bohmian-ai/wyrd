---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-agent-runtime-composition
spec_revision: 3
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6, FIND-TASK-001-7, FIND-TASK-001-8]
---

# Close the bounded SDK deletion gaps

## Authority and immutable review subject

Use `$wyrd-implement` directly. This is one remediation of the approved deletion,
not a new design plan or a return to public SDK Agent execution.

- Approved [specification revision 3](../../spec.md).
- Original [TASK-001](../../tasks/TASK-001-delete-sdk-agent-runtime.md), as committed at base.
- Base: `06a368906187be20364a2d959da311151964b617`.
- Reviewed candidate: `497dd3acc8fd06567a0e0ff01baf599f9af1711b`.
- [Verdict](verdict.md), [validated ledger](findings-validation.md), and
  [smoke coverage follow-up](followup-review.md).

The cumulative candidate removed the main SDK runtime and Run invocation
correctly, but consumer migration, companion deletion and retained proof are
incomplete. Correct only the independently validated causes below, preserving
all other approved behavior. Read full owners and sibling consumers before
editing; paths identify affected surfaces rather than a private allowlist.

## Diagnosis and selected correction

### FIND-TASK-001-1 — Preserve Python gateway request subjects

**Diagnosis.** `sdks/wyrd-sdk-python/tests/integration/test_gateway_inference.py:49–55`
constructs authenticated model requests with no Run/Card headers. Its route
journey drives four such requests but projects deployment only, so null subject
capture passes. The server intentionally treats absent optional headers as no
subject. REQ-016/AC-009 require these migrated calls to retain their attribution;
a separate lone-header test cannot supply it.

**Correction.** Use the intended existing Run for each alias at this producer,
attach `wyrd-run-id` and `wyrd-card-uid`, and compare the actual accepted calls'
Run/Card/principal/deployment in the same journey. Reuse existing Run identity
and the Python support-desk/local-development header pattern. Preserve grant
denial and no-extra-upstream assertions. Do not change ingress or infer subject
identity in an auth adapter.

### FIND-TASK-001-2 — Consume the actual TS gateway Prompts

**Diagnosis.** `sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-inference.test.ts:401–403`
reads model only and substitutes a constant `hi` request for both hydrated
aliases. Its portable messages contain `{{question}}`; its native
`body.messages` contain `${question}`. The fixed mock response and model-only
assertion allow both real inputs to be ignored, contrary to consumer closure
under REQ-013/016 and AC-009.

**Correction.** Have this existing application/test caller use precisely those
two authored message forms and insert the question literally. Give their
expected content a distinguishable assertion at the recording upstream, across
both aliases/deployments. Preserve auth, model, headers and denial coverage.
Reuse existing local binding patterns; do not add a generic renderer, settings
compiler, or new SDK operation.

### FIND-TASK-001-3 — Finish deleting the mock callback constructor

**Diagnosis.** `crates/skald/skald-prompt/src/python/response.rs:40–49` still
registers public `ProviderResponse.text`. Its authored declaration/docs at
`sdks/wyrd-sdk-python/python/wyrd/prompt/__init__.pyi:263–278` instruct returning
it from deleted `after_model_callback`. This deterministic mock constructor has
no surviving lifecycle consumer and violates REQ-013/AC-007/011.

**Correction.** Delete this Python-visible constructor, its authored declaration
and callback-only docs, and directly unused private construction glue. Keep
Rust-native provider mocks/internal runtime uses and the surviving typed
response/request/message views. Do not replace it with a constructor, parser,
test-only production escape hatch, or blanket provider-view cleanup.

### FIND-TASK-001-4 — Preserve existing authored-Prompt smoke contracts

**Diagnosis.** The deleted `sdks/wyrd-sdk-python/tests/smoke/test_prompt_live_providers.py`
at base tests actual authored Prompt output reaching live providers. Its
12 portable non-loop cases and five native cases cover a surviving compiler
boundary. Retained unit tests prove local lowering and retained gateway smoke
sends handwritten bodies; neither proves this seam. Deleting the execution
vehicle does not satisfy the task's sole-contract test-retirement rule.

**Correction.** Migrate only those pre-existing cases to test/application-owned
native requests, using actual public Prompt loader/render/native output and
existing installed clients/HTTP and smoke credential/model helpers. Insert the
former fixed run input in the test-owned request as necessary. Retain the prior
nonempty-text, string `answer` JSON and typed Jev-answer checks. Remove the
retired local-tool loop and AgentRun result/iteration/callback assertions.
Keep direct-provider lowering isolation, existing credential skips, opt-in
marker and fast-lane separation. Correct stale native-route-Agent wording in
`mise.toml`'s existing smoke description and retirement evidence. No new
provider/case, live gate, compiler/runtime fix or revision-2 sweep hardening.

### FIND-TASK-001-5 — Document the actual Rust IO side effects

**Diagnosis.** `examples/support-desk/rust/support_desk.rs` `answer`/`serve` and
`examples/support-desk-app/rust/lib.rs` `answer`/`ask`/`review` materially own
new IO without required cancellation/partial-progress rustdoc. A dispatched
model call may complete before its later ticket/verification record; losing a
reply proves no rollback. `serve` promises flushing on return despite `?`
early exits. Handler error docs omit new application failure categories.
This violates the explicit AGENTS §16 gate.

**Correction.** Batch accurate rustdoc on those existing owners/callers:
uncertain remote completion, subsequent evidence that may be absent, lack of
automatic replay/deduplication, successful-completion-only flushing, and actual
safe error categories after finding 8. Describe source behavior; add no new
retry, cleanup, durability, cancellation or availability guarantee.

### FIND-TASK-001-6 — Retain finite native request deadlines

**Diagnosis.** New Rust requests at `examples/support-desk/rust/support_desk.rs:190–199`
and `examples/support-desk-app/rust/lib.rs:136–146` have no total/read deadline;
pinned reqwest defaults are unbounded. TS `support-desk.ts:129–144` sends no
abort signal. The removed call bounded send/body with ten minutes. A stalled
network hop or incomplete response can prevent evidence and workflow progress;
the server's provider deadline cannot release that client wait.

**Correction.** Apply the former ten-minute total budget directly with the
installed reqwest request timeout at both Rust sites and native
`AbortSignal.timeout` on the TS authenticated fetch. Keep the bound active
through body consumption, using the existing adapter's request-init forwarding.
Do not add a timer engine, retry, public timeout option, SDK transport, test knob,
new dependency, or shared-server policy. Python already has a finite stock
client deadline and needs no unrelated rewrite.

### FIND-TASK-001-7 — Insert TS questions literally

**Diagnosis.** `examples/support-desk/typescript/support-desk.ts:140` passes
question text as JavaScript's replacement-string argument. `$&`, `$$` and other
replacement tokens change the actual request, while observations may retain
the original question. The documented/exported application helper accepts such
text; former Prompt rendering inserted it literally.

**Correction.** Use the standard replacement callback returning the question.
Keep this application's exact existing template scope. Extend the existing
support-desk journey's actual helper invocation with replacement-token input
and assert exact upstream prefixed text, preserving correlation/refusal proof.
No general templating feature or standalone expression-only substitute test.

### FIND-TASK-001-8 — Restore safe reference-app error projection

**Diagnosis.** `examples/support-desk-app/rust/lib.rs:147–150` copies a native
provider error object into `ApiError`, serialized at `:403–406`; newly introduced
`ApiError::internal(Display)` also exposes native render/transport/JSON details.
The gateway strips credentials but deliberately preserves other provider
content. The prior caller used fixed/catalog-safe text. This is a proven
public-response path in an in-scope reference app, not an observed secret leak
or permission defect in the server.

**Correction.** Keep projection at the app boundary. Preserve refusal status
and recognized Wyrd code while deriving trusted message/title from existing
`WyrdError` catalog metadata (`codes`/`from_code`/`title` pattern in the deleted
caller); use fixed status text for unknown/uncoded upstream failure and omit
arbitrary provider fields. A recognized code never makes its supplied message
trusted. Newly introduced native IO/render/decoder failure paths receive fixed
safe application text rather than `Display`; retain useful static invariant
messages. Do not modify unrelated existing `From<WyrdError>` behavior, native
gateway passthrough, shared errors or authorization. No general error API or
parallel catalog. The reference app has no local crate/harness: do not create
one or claim runtime/compile proof that does not exist.

## Constraints and non-goals

Preserve Agent/Prompt Cards, hydration, public Prompt authoring, internal
Skald/Vala/server execution, existing framework integrations, caller identity,
tenant/subject authorization, audit, upstream secret/header boundaries and
native gateway semantics. Public Agent and Run invocation stay deleted.
No new public execution/rendering API, compatibility shim, generic transport,
parser, validator, runtime owner, persistence, dependency, feature, timer or
retry framework. No revision-2 work. Correct bounded docs together with code;
never weaken checks or delete surviving assertions to pass.

## Acceptance and focused proof

| Finding | Closure outcome |
|---|---|
| 1 | Python migrated calls' exact Run/Card/caller/deployment are asserted; missing headers fail; denial stays upstream-free. |
| 2 | Both actual TS authored forms drive distinct asserted upstream content; hard-coded `hi` cannot pass. |
| 3 | Mock callback constructor absent from public/native method and authored typing; Prompt rendering and shared views remain. |
| 4 | Prior non-loop authored-Prompt provider contracts retained with actual lowered output, correct markers/skips and meaningful native response checks. |
| 5 | All named changed Rust owners/callers document real interruption/partial progress/errors without inventing stronger guarantees. |
| 6 | Three application request sites use native finite total bounds covering body consumption; no replay or new public configuration. |
| 7 | Actual TS helper transmits literal replacement-token questions and retains correlation. |
| 8 | Public reference-app response contains only safe catalog/fixed text and permitted code/status for native/provider/local failures. |

For changed executable gaps, record focused failing assertions before the
minimal correction, then passing output and affected regressions. Deletion,
native timeout-option wiring, documentation and reference-only projection may
use source/static/dependency-contract proof as specified in the validated
ledger; do not manufacture historical RED or add harnesses to force it.

Run at repository root unless stated. Native prerequisites are `mise run
py:setup`, `mise run ts:build`, and `mise run ts:build:testing` when needed.
Run Cargo-backed work sequentially. Rerun failures with tracing before diagnosis.

```bash
# Finding 1: exact existing Python gateway migration journey
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_gateway_inference.py::test_agent_prompt_routes'
# Findings 2, 6, 7: exact existing TS gateway/support-desk journeys
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/gateway-inference.test.ts tests/integration/support-desk.test.ts -t "^(agent prompt routes|support desk answers, verifies, and explains every request)$"'
# Findings 5/6: exact existing compiled Rust support-desk regression
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=support_desk::support_desk_answers_verifies_and_explains_every_request)"'
```

Finding 3: a one-time native/public attribute probe plus `mise run py:test:unit`,
`mise run py:lints`, and `mise run py:typecheck` proves absence and preserved
Prompt behavior. Add no permanent name-ban check.

Finding 4 restores existing owners/selectors confirmed from base. Use the
following same exact nodes for collection and available-key execution; retain
17 non-loop parameter cases (equivalent grouping only with recorded equivalent
proof). The old local-tool-loop node is intentionally excluded.

```bash
# Collection requires no provider call and does not establish live acceptance.
mise exec -- bash -lc 'cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest --collect-only -q -m smoke tests/smoke/test_prompt_live_providers.py::test_portable_text tests/smoke/test_prompt_live_providers.py::test_portable_system_and_history tests/smoke/test_prompt_live_providers.py::test_portable_max_output_tokens tests/smoke/test_prompt_live_providers.py::test_portable_response_format tests/smoke/test_prompt_live_providers.py::test_native_chat_completion tests/smoke/test_prompt_live_providers.py::test_native_responses tests/smoke/test_prompt_live_providers.py::test_native_messages tests/smoke/test_prompt_live_providers.py::test_native_generate_content tests/smoke/test_prompt_live_providers.py::test_native_system_one'
# Preserve the existing smoke lifecycle; unavailable keys remain explicit skips.
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -rs -m smoke tests/smoke/test_prompt_live_providers.py::test_portable_text tests/smoke/test_prompt_live_providers.py::test_portable_system_and_history tests/smoke/test_prompt_live_providers.py::test_portable_max_output_tokens tests/smoke/test_prompt_live_providers.py::test_portable_response_format tests/smoke/test_prompt_live_providers.py::test_native_chat_completion tests/smoke/test_prompt_live_providers.py::test_native_responses tests/smoke/test_prompt_live_providers.py::test_native_messages tests/smoke/test_prompt_live_providers.py::test_native_generate_content tests/smoke/test_prompt_live_providers.py::test_native_system_one'
```

Findings 5/6 receive complete source inspection at each changed owner: docs
match actual operation order; reqwest's existing request timeout covers the
finished response body; the authenticated fetch passes the abort signal through
body reading. Retain the focused healthy/refusal journeys above. No ten-minute
stall suite, new timer dependency or public test timeout knob is required.

Finding 8 receives source-level proof for an uncoded provider error with a
sensitive marker, recognized code plus untrusted message/extra fields, normal
stable permission refusal, and native transport/JSON error. Trace each through
the complete projection to `IntoResponse`; only allowed safe fields reach it.
Reuse derive-backed catalog metadata. Record reference-app runtime proof as
unavailable rather than adding a crate or harness.

For the actual write set run `mise run fmt`, `mise run lints`,
`mise run py:format`, `mise run py:lints`, `mise run py:typecheck`,
`mise run py:test:unit`, `mise run ts:format:check`, `mise run ts:lints`,
`mise run ts:typecheck`, and `mise run ts:test:unit` once, avoiding repeated
component checks. Record TS unit execution explicitly; the candidate evidence
did not. Run `mise run check:examples` for changed examples and the owning
smoke task-description command check if applicable; `mise run docs:check` only
if docs-site source is changed. Dependency/codegen/NAPI checks are conditional
on an actual dependency/generated/wire change; none is required to introduce
new machinery. Do not run full user-journey suites or a repository aggregate
for this remediation. Finish with `git diff --check` and full actual-diff audit.

## Evidence and next review

Record each FIND ID's diagnosis, minimal correction, exact failing/passing or
static proof, final commands/features and residual credential/reference limits.
Do not rewrite approved scope, weaken assertions, claim skips as live passes,
or invent original RED evidence. The next independent task review uses the
complete original base-to-new-candidate range, original task, this remediation
and prior verdict, and must source-validate closure of all eight IDs.

## Implementation Evidence

Commits `8705c7ba0`..HEAD on `change/agent-runtime-composition`, on top of
reviewed candidate `497dd3acc`.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-1 | `test_gateway_inference.py` `answer` takes the Run and sends `wyrd-run-id`/`wyrd-card-uid` (the support-desk pattern: `state.card_ref(run.alias).uid`). `test_agent_prompt_routes` opens one Run per alias and deployment and compares `run_id, card_uid, sub_principal_id, deployment` of the four captured calls. Denial and no-extra-upstream assertions kept | RED with headers removed: `At index 0 diff: {'run_id': None, 'card_uid': None, ...} != {'run_id': '01a1470f-…', 'card_uid': '01a1470f-…', ...}`. GREEN: exact selector `1 passed` | PASS |
| FIND-TASK-001-2 | TS `askAgent` sends the alias's authored messages: portable `spec.messages` or native `spec.body.messages`, with `{{question}}`/`${question}` replaced literally by one regex. Each alias and deployment gets a distinct question, and the recording upstream's `messages` are asserted | RED with the constant `hi` request: four `- "agent on openai"` / `+ "hi"` diffs. GREEN: `agent prompt routes` passed | PASS |
| FIND-TASK-001-3 | Deleted `PyProviderResponse::text_py` and its only glue (`from_native`, `from_arc`, `native`; no other caller) plus the `ProviderResponse.text` stub and callback docs. Rust `ProviderResponse::text` and the typed views are kept | One-time probe after `py:setup`: `hasattr(wyrd.prompt.ProviderResponse, "text")` → `False` (root also `False`), and `Prompt.render()` still works. `py:test:unit` 597 passed, `py:lints`, `py:typecheck` | PASS |
| FIND-TASK-001-4 | Restored `tests/smoke/test_prompt_live_providers.py`: the 12 portable non-loop cases and 5 native cases. Each renders with public `Prompt.render`, appends the former fixed `INPUT` turn in the test-owned body (Jev has none), and posts with installed `httpx` to the provider's own endpoint, so no gateway is involved. Same `credential`/`model` skips and `smoke` marker; checks for non-empty text, string `answer` JSON, and typed Jev answers. Loop case and AgentRun assertions not restored. `mise.toml` smoke description and TASK-001 retirement evidence corrected | Collection: `17 tests collected`. Execution: `12 passed, 5 skipped` (live OpenAI, Gemini and Jev; `SKIPPED [5] … ANTHROPIC_API_KEY is set`). The Anthropic cases are explicit skips, not live passes | PASS (Anthropic unverified live) |
| FIND-TASK-001-5 | Rustdoc on `support_desk.rs` `answer`/`serve` and `support-desk-app` `answer`/`ask`/`review`: the outcome is uncertain once a call is dispatched, later ticket/check records may be missing, there is no replay, dedup or rollback, `serve` flushes only on full success, and the new error categories are listed. No behavior added | Source inspection against operation order. `fmt`, `lints`, Rust support-desk journey `1 passed` | PASS |
| FIND-TASK-001-6 | Rust: `.timeout(ANSWER_TIMEOUT)` (10 min) on both reqwest requests. reqwest's `RequestBuilder::timeout` runs from connect through the end of the response body, and both sites read the body (`.json()`) inside it. TS: `signal: AbortSignal.timeout(ANSWER_TIMEOUT_MS)`; `httpAuth` forwards `{ ...init, headers }` to `fetch`, and an aborted signal also aborts body reading. Python unchanged (stock client deadline) | Source and dependency-contract inspection. Healthy and refusal journeys still pass: Rust support desk, TS `support desk …` and `agent prompt routes` | PASS |
| FIND-TASK-001-7 | `support-desk.ts` replaces through a callback `() => asked`. The journey calls the exported `answer` with `Is $& or $$ or $' my order?` and asserts the upstream body contains the exact `"Answer the customer: …"` text; correlation and refusal proof are kept | RED: received `Answer the customer: Is {{question}} or $ or  my order?`. GREEN: journey passed | PASS |
| FIND-TASK-001-8 | `support-desk-app` `ApiError::internal` takes only `&'static str`. Render, serialize, transport/timeout and decode failures map to fixed text. A new `ApiError::refusal` keeps the status and an `error.code` only when it is in `WyrdError::codes()`; the message is the catalog `title()` via `from_code`, or fixed `the Wyrd gateway answered HTTP <n>`. Provider messages and fields are dropped. `From<WyrdError>` is unchanged | Source trace through `IntoResponse` (only `(status, Json(body))`): (a) uncoded `{"error":{"code":"invalid_api_key","message":"<marker>"}}` → `{"message":"the Wyrd gateway answered HTTP 401"}`; (b) `WYRD_PERMISSION_403_DENIED_RBAC` with an untrusted message and extra fields → `{"code", "message": title or fixed}`, extras dropped; (c) a normal 403 RBAC refusal → the same as (b); (d) send error → 500 `the Wyrd gateway call failed`; undecodable success body → 500 `the gateway reply could not be read`; undecodable refusal body → fixed status text. Runtime/compile proof unavailable: the reference app has no crate or harness. `fmt` and `docs:check` (the quickstart includes its regions) pass | PASS (source only) |

Commands run once over the write set: `fmt`, `lints`, `py:format`,
`py:lints`, `py:typecheck`, `py:test:unit` (597 passed), `ts:format:check`,
`ts:lints`, `ts:typecheck`, `ts:test:unit` (9 files, 71 tests passed),
`check:examples`, `docs:check`, plus the exact focused journeys and smoke
commands above, and `git diff --check`. No dependency, generated or wire
change, so the deps, codegen and NAPI checks were not run.

Non-goals held: no public execution or render API, shim, transport, timer,
retry, dependency or feature. No revision-2 work.
