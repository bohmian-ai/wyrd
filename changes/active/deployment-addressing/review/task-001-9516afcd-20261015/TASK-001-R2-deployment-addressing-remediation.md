---
id: TASK-001-R2
kind: remediation
status: ready
spec: SPEC-deployment-addressing
spec_revision: 6
requirements: [REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-013, INV-004, INV-005, INV-007, AC-004, AC-005, AC-006, AC-008, AC-009, AC-011, AC-012]
depends_on: []
parent_task: TASK-001
remediates: [FIND-001-3, FIND-001-5, FIND-001-8, FIND-001-13, FIND-001-14, FIND-001-15]
---

# Complete deployment-addressing acceptance at the existing owners

## Authority, subject and outcome

Approved authority: `changes/active/deployment-addressing/spec.md`, revision 6;
original task: `changes/active/deployment-addressing/tasks/TASK-001-deployment-addressing.md`.
This task derives from `findings-validation.md` and `verdict.md` in this review
directory. Its predecessor is
`../task-001-ef2fe8bd-20261015/TASK-001-R1-deployment-addressing-remediation.md`.

Original base: `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`.
Previous reviewed candidate: `ef2fe8bda2071f7b32cb6803b7f9f4599f0d4183`.
Current reviewed candidate: `9516afcdc7d3ffb58d5f20e85c4341d0df3cef23`.
The immutable review source is `/tmp/wyrd-task-review-9516afcd`; source locations
below refer to that commit. Revalidate callers on the implementation branch;
preserve concurrent and approved adjacent work. No implementation or test run
was performed by this review.

The outcome is early shared schema/model refusal, consistent local Vertex
refusal, complete TypeScript derived Prompt introspection, an actual downloaded
Agent loader journey, and gateway guides whose setup produces the retained
evidence they promise. Existing successful deployment, permission, media,
capture, batch, authored-preservation and Workflow-removal corrections remain.
Route this ready task directly to `$wyrd-implement`.

## Correct two private instructions without changing the approved spec

R1 incorrectly assumed TypeScript needed no additional projection. REQ-008
and AC-004 already require derived introspection on hydrated objects in all
three SDKs. This task replaces that assumption with a thin projection of the
existing Rust metadata. It does not add authored fields or a new endpoint.

R1 also privately selected Python for finding 14's runtime stage. The owner
requested loading and executing downloaded Agents, without selecting a
language. Python's current local Agent has no gateway-caller binding; adding
one solely for this test is unnecessary. This task replaces that incidental
instruction with the existing public Rust Agent loader and gateway host seam,
inside the same installed-CLI journey. Existing Python projection/refusal and
observed invocation coverage remains valid. Neither correction changes
approved behavior or requires a new public Python runtime API. The source and
authority resolution is preserved in `followup-review.md` and independently
validated in `findings-validation.md`.

## Existing owners and consumer closure

| Capability | Existing owner and inspected consumers | Missing behavior and selected extension |
|---|---|---|
| Local versus server gateway execution | `wyrd-client::PublicWyrdGatewayCaller::call`, direct Rust Agent host binding, observed Run; separate server caller | Enforce actual Vertex-provider refusal at the public caller before transport; preserve server Vertex. |
| Output schema validity | `skald-spec::check_output_schema`, AuthoredPrompt compile, Prompt constructor, builder finalization, PromptSpec registration, Python request rederivation | Replace partial schema-language checking with offline standards meta-validation at the same pure owner. |
| Literal native request target | `skald-spec::Prompt::new`, typed provider requests, native wrapper/Agent runtime and setters | Validate body model and canonical model as one literal identity before variable derivation. |
| Hydrated TS introspection | Shared WyrdState/Prompt runtime; N-API standalone Prompt and hydrated Agent projection; TS PromptCard | Project existing derived data on both paths; no TS derivation or stored duplicate data. |
| Downloaded runtime execution | Installed Python CLI driver, existing Rust client integration binary, Agent loader/resolver and PublicWyrdGatewayCaller | Run the actual loaded runtime Agent for all providers, using downloaded exact refs and normal host auth. |
| Gateway documentation/evidence | Published Connect guides, existing capture-policy command, installed setup and stock-client/framework tests | Apply metadata capture before the first request; read evidence belonging to those requests. |

These existing owners can provide the outcomes. No replacement graph store,
loader, parser, transport, Agent loop, server harness, credential system or
capture writer is justified. Paths identify owners, not a private file/helper
allowlist.

## Diagnoses and selected corrections

### FIND-001-3 — Refuse actual local Vertex calls at the public caller

**Plainly:** a local Rust Agent can still dispatch a Vertex gateway call even
though local Vertex gateway execution must refuse before IO.

REQ-010/011 and AC-008 explicitly defer local Vertex gateway execution while
preserving native and server Vertex. The original Anthropic/Google/Jev target
fixes are present; this is the remaining sibling boundary.

`skald-agent/src/agent.rs:339–383` accepts a host gateway caller and builds
the route registry; `gateway.rs:78–99` keys that registry by the actual Prompt
provider. `wyrd-client/src/gateway_caller.rs:108–128` projects without refusing
Vertex. GenerateContent becomes public Gemini ingress at `:286–294`; gateway
`routing.rs:134–165` permits Vertex provider to Vertex adapter, and
`adapter/mod.rs:219–222` accepts that ingress on Vertex. Thus this reaches
server Vertex dispatch. The refusal in `observe/invoke.rs:93–99` covers only
Run invocation and cannot protect a direct public Agent caller. A Google
Prompt targeting a deployment named `vertex` is valid and distinct.

Enforce the local unsupported rule on **PublicWyrdGatewayCaller**, using the
typed actual provider before native projection/HTTP. Reuse its existing
terminal bad-request/error projection; add no public error code. This guard
belongs here because this caller owns the local/public execution boundary.
Do not forbid Vertex globally on Prompt, Agent or the registry: the separate
ServerWyrdGatewayCaller must retain server/judge Vertex. Preserve arbitrary
deployment names, native Vertex and direct-client conversion.

Proof must include an actual Vertex provider on declared and overridden local
routes, zero gateway/upstream requests, Google to a deployment named `vertex`
still succeeding, and the existing server Vertex behavior. Extend P1; record
any added direct-Agent exact selector rather than inventing a passing name.

### FIND-001-5 — Validate the schema language with the standard implementation

**Plainly:** malformed output schemas still register because the validator
checks only a few schema keywords.

REQ-006/007/008/011, AC-006 and R1-AC-05 require refusal before persistence or
upstream IO. `skald-spec/src/prompt.rs:235–327` accepts unhandled keywords at
307. For example, a nested number schema with `minimum: "wrong"` passes;
numeric `$ref`, negative `minLength`, string `uniqueItems`, and malformed
conditional/dependent subschemas also escape. Portable compilation, native
Prompt construction, builders and Python request rederivation converge here;
PromptSpec deserialization (`wyrd-spec/src/card/prompt/spec.rs:65–78`) then
accepts it. Existing tests cover the implemented subset, so GREEN does not
close the schema-validity contract. The observable failure is a registered
invalid output contract rejected late by the provider.

Replace the partial keyword walk at **the same pure shared acceptance owner**
with standards-backed offline meta-validation. The selected ordinary dependency
update is workspace `jsonschema` 0.18.3 to **0.46.4**, retaining
`default-features = false` and disabled HTTP/file retrieval. The validated
installed version has fallible cached bundled draft-specific meta-validation
and modern reference support; its MSRV is below the workspace requirement.
Use bundled meta-validation for recognized built-in declarations and the
library's normal unversioned semantics otherwise. Unknown/custom meta-schema
declarations receive a local unsupported-schema error; never fetch their URI.
Do not compile/fetch user schema reference targets or install a retrieval
registry. Use the fallible validation API, not a panic-based convenience API.
Keep separate detectable destination checks, including strict OpenAI object
root, field-specific errors, authored values and valid native options. Do not
replace this checker with another private schema-keyword traversal.

The dependency belongs in `skald-spec`, which owns this pure invariant, not
in a broad configuration owner or per-language layer. Adapt the two existing
test-only users of the old JSONSchema API in server
`tests/integration/pg_openapi_contract.rs` and **wyrd-mcp**
`tests/bifrost/mcp/principals.rs`; preserve their instance-validation/UUID
assertions. This is an ordinary dependency/API update, not a new schema store,
schema dialect setting, network resolver or public contract.

Extend P2–P4 with malformed known numeric/string/reference keywords and nested
`$defs`, conditionals and dependent-schema locations in portable/native forms.
Include valid declared-draft, unversioned, nested and local-reference schemas,
unaltered authored JSON and destination checks. Registration must refuse
standalone and inline invalid schemas before durable writes/provider IO.
Run P5–P6 for the two adapted dependency consumers.

### FIND-001-8 — Validate the actual native request model before derivation

**Plainly:** the native constructor validates one model string but can render
a different model stored inside the request.

REQ-005/007/008, INV-004/007 and AC-006 require one literal model identity.
`Prompt::new` (`skald-spec/src/prompt.rs:398–421`) checks only its separate
model argument. A Chat request containing model `{{target}}` with canonical
argument `fixed` succeeds; extraction at 710–727 derives `target`, and binding
at 535–543/634–650 replaces it. The returned body can target another model
while Prompt.model still says `fixed`. The public native wrapper, Agent
`run_prompt`, request builder and native provider sender reach this value;
Responses, Anthropic and Jev body-model slots share the producer. Authored
compilation and normal builders/setters are already corrected and do not
close this public native constructor.

At **shared Prompt construction**, before variable derivation, validate the
effective typed request model with the existing literal validator and refuse
an independently supplied model that conflicts with the canonical model.
Keep model-less GenerateContent, matching literal/slash models and content
variables. Do not silently overwrite an invalid supplied target, remove
arbitrary fields from rendering, or add provider-dispatch guards, aliases or
catalogs. Existing builder/setter consumers must continue through this owner.

Extend P7 with direct constructors for each model-bearing request using a
templated or conflicting body model and a literal canonical argument. Assert
refusal before derivation/IO, matching slash identities accepted and Google
model-less construction preserved. Use the existing public native/Agent test
owner for any necessary cross-boundary regression; record its actual selector.

### FIND-001-13 — Expose existing derived Prompt data in TypeScript

**Plainly:** TypeScript retains Prompt JSON and hashes but cannot expose the
derived variables or output schema that the approved spec promises.

REQ-008/011 and AC-004/009 require derived introspection across all three
SDKs. Shared Prompt already owns variables/media variables/response metadata
(`skald-spec/src/prompt.rs:30–57,398–423,657–702`); WyrdState::prompt and
HydratedAgent::prompt expose it (`wyrd-client/src/state.rs:85–151,986`).
N-API `native/src/cards.rs:441–446` discards the hydrated Prompt and returns
the stored envelope; sibling `hydrated_agent:619–639` does the same. TS
`PromptCard` (`src/index.ts:2360–2373`) adds only ref, so its new four-form
test stops at JSON/hash. That limitation is real, but is not an approved
exception. No storage/hash corruption is alleged.

Extend **existing N-API lifecycle projection and hydrated TS Prompt surface**
with thin read-only metadata from the existing Rust runtime for standalone
lookup and Agent navigation. Include text/media variables and response schema
and name, with a plain-text/null representation consistent with existing
Rust/Python data. Use existing TS camelCase conventions, typed declarations
and documentation; regenerate native declarations from their owner. Preserve
the authored envelope/ref/spec/hash and durable wire schema. Do not derive
templates/schema in TS or add a parser, registry, endpoint or Prompt runtime.

Extend P8 after real registration/GET/hydration to assert derived variables,
schema/name and plain-text behavior for portable/native standalone/inline
navigation; include existing media names through existing fixtures. Preserve
JSON/hash assertions and existing Rust/Python proof. No production accessor
ban from R1 applies to this required thin projection.

### FIND-001-14 — Execute the actual downloaded Agent through its loader

**Plainly:** the new bundle journey constructs a replacement Agent through
Run.invoke, so it does not prove that the Agent loader retained what ran.

This remains the explicitly **human-directed** addition, not a retroactively
discovered original bug. The owner's register → download → load via Agent →
inspect route/provider/model → execute → restart sequence and R1-AC-14 remain
required. The installed driver (`test_local_development.py:602–759`) performs
real setup, download and restart. Its child at 232–280 loads WyrdState and
calls `state.run(alias).invoke`; `observe/invoke.rs:77–110` builds Agent::new
and explicitly reapplies the route. A loader route-loss regression could
therefore leave the entire new test green. This is a missing combined proof,
not a claim that registered Cards are currently corrupt.

Keep the **same installed-CLI driver and fresh-process stage**, and extend the
existing Rust client integration test owner as its runtime client. Its current
dependencies already include WyrdState, Skald Agent/Prompt/tools and the public
gateway caller. Use WyrdState's normal downloaded graph resolution to obtain
the exact pinned downloaded Prompt; supply it under its exact registered ref
to the existing LocalPromptResolver/register_prompt_card in that isolated
process. Load each **disk Agent with the public Agent loader**, whose from_card
must restore the reference, identity and route. Inspect the loaded runtime and
its to_card/to_spec before attaching host dependencies. Do not use original
authoring inputs, fetch a replacement Prompt, reconstruct missing fields,
call Agent::new, set its route, or override its provider registry.

Attach only the existing PublicWyrdGatewayCaller using normal saved WyrdClient
auth and existing Run/Card subject correlation. Execute **that same loaded
runtime instance** through existing run_prompt with its loaded Prompt. That
existing path already supports Jev's one-shot call; add no general Jev loop,
tools or streaming. No new Python host binding, test server, executable
product command, graph loader or transport is required. The current Python
observed-invocation assertions can stay; they cannot substitute for this path.

After existing step 5, run all four onboarded providers (OpenAI, Anthropic,
Google/Gemini, Jev) this way. Preserve distinct deployment names, slash models,
registered/exact Prompt and Agent refs, actual loaded route/provider/model,
expected answer, provider credentials upstream, no Wyrd credential forwarding,
zero native-trap connections and retained rows correlated to these runs.
Keep the real Card-scope403/no-dispatch refusal; a new specifically RBAC
denial is not required. Continue step 6: restart with the same tenant/config
and run the **same downloaded bundles** again without re-register/download,
preserving auth/key reuse, decryption, revocation and cleanup assertions.

P9 remains the exact outer proof. Add and record the actual exact Rust child
selector and its normal build prerequisite in the existing integration binary;
do not invent a passing name or launch another server. A lost route in
Agent::from_card must make this combined stage fail. Supporting loader unit
tests alone cannot replace it.

### FIND-001-15 — Make the documented setup produce its promised evidence

**Plainly:** the setup guide promises recorded calls but omits the capture
policy needed to record them.

REQ-011/AC-009 and R1-DOC-AC-01/02 require executable setup and response plus
retained-evidence proof. The published Connect setup at 45–130 starts a fresh
tenant, uploads credentials/deployment, sends a request and promises a
`vala.gateway.calls` row; its framework page inherits this setup. Neither
applies a capture policy. `gateway/service.rs:967–978` defaults an unset policy
to Disabled, and `invocation.rs:895–898` produces no facts for it. Waiting
cannot recover such a record. The working installed journey explicitly puts
the existing capture-policy file at `test_local_development.py:682–683`, so
it does not mirror the guide's visible setup.

The actual framework inference tests enable metadata capture and check reply
and upstream headers, but do not read retained evidence for Agents SDK and
Pydantic AI calls. This falls short of the explicit docs proof criterion even
though their inference succeeds. Close these related docs/setup proof gaps
together; do not add another production capture owner or one finding per client.

Before the first documented inference, apply the existing metadata-only
capture policy with the existing command:

```json
{ "mode": "metadata", "payload_fields": [] }
```

```sh
wyrd gateway capture-policy put --file capture-policy.json
```

Use the existing walkthrough command/config and explain briefly that recording
depends on it. Keep payload capture off and the server default opt-in. Have the
framework guide inherit this canonical setup rather than create another one.
Extend existing actual-inference tests with normal Bifrost reads and existing
bounded polling for rows correlated to the tested request, verifying expected
deployment/model/outcome/attribution. Unrelated fixture traffic is not proof.
Preserve refusal/no-upstream checks, use controlled HTTP providers, and add
no capture queue, live paid call or new harness.

Run the corrected documented fresh setup through P9 and framework proof P10.
Retain the published stock-client inference/refusal checks; add missing
retained-row assertions for their published success checks where necessary.
Complete the previously requested rendered sidebar/search/links/headings/code
inspection at desktop/narrow widths in light/dark when browser tooling is
available. Record a concrete environment blocker if not; never claim this
unperformed check passed. No layout defect was independently observed in this
review, so no speculative layout rewrite is prescribed. Keep the approved
docsite-rebuild Connect structure, indexes/theme and `docs:check` gate.

## Ordered behavior and proof

Work one scenario at a time. For changed executable behavior, record the
expected behavioral RED, then the minimum GREEN and relevant earlier focused
regressions; refactor only while those remain green. Compile/setup failures
are not behavioral RED. Do not rerun older commits or manufacture retrospective
RED for already-correct behavior. Documentation/declaration regeneration and
verification-only proof need no artificial failing test.

### Scenario 1 — Actual local Vertex refusal (FIND-001-3)

**Behavior.** Actual local Vertex default/override calls refuse before IO;
Google named-target and server Vertex behavior survive.
**RED.** Extend P1 with that actual provider and assert no transport; record
the expected missing refusal, not an unrelated setup error.
**GREEN.** Enforce the selected public-caller boundary, retaining the positive
Google case and server path.
**REFACTOR.** Reuse existing typed provider/error projection only; do not move
the local rule into a globally shared Agent/provider owner.

### Scenario 2 — Complete offline schema validity (FIND-001-5)

**Behavior.** Shared native/portable/builder/registry acceptance rejects
malformed known and nested schema nodes before persistence/IO.
**RED.** Extend P2–P4 with the diagnosed counterexamples and corresponding
positive preservation cases; record accepted-invalid behavior.
**GREEN.** Use the selected offline meta-validation and adapt existing library
consumers; P5–P6 preserve strict advertised-contract/UUID assertions.
**REFACTOR.** Remove the superseded partial keyword language checker; retain
the separate destination-specific validation without duplicating schema logic.

### Scenario 3 — One effective literal native model (FIND-001-8)

**Behavior.** Direct native construction cannot derive/render an independent
request-body target; matching/model-less/content behavior survives.
**RED.** Extend P7 with literal canonical argument plus templated/conflicting
typed native body model and demonstrate the current acceptance gap.
**GREEN.** Enforce the selected construction invariant before derivation and
keep matching slash models and model-less Google positive cases.
**REFACTOR.** Reuse existing typed model handling and literal validation;
do not distribute guards into dispatch consumers.

### Scenario 4 — TS shared-derived introspection (FIND-001-13)

**Behavior.** Standalone and Agent-navigated hydrated Prompts expose actual
shared-derived metadata while authored JSON/hash remain unchanged.
**RED.** Extend P8 to assert variables/media/schema/name through the public
hydrated object; distinguish a missing projection from a server setup failure.
**GREEN.** Project existing Rust data through both current N-API/TS paths and
complete the same four-form journey.
**REFACTOR.** Preserve the current envelope/navigation and remove any duplicate
projection plumbing only where the same existing owner can handle both paths;
never derive templates in TypeScript.

### Scenario 5 — Actual downloaded Agent execution (owner FIND-001-14)

**Behavior.** The same installed journey loads and runs all four downloaded
Agents through the public runtime loader, then repeats those bundles after
restart with preserved identity, routing, auth and retained evidence.
**RED.** Add the real-loader stage to P9 and record any missing behavior it
exposes. If the current actual loader already satisfies it, report an honest
verification-only coverage correction rather than altering working production
code or injecting an artificial failure.
**GREEN.** Use only the selected existing Rust loader/resolver/public-caller
host seam; inspect and execute the loaded instance, never a rebuilt Agent.
**REFACTOR.** Reuse existing subprocess and test owners; retain the direct
setup, denial/native-trap, evidence and restart checks without another harness.

### Scenario 6 — Documented setup and framework evidence (FIND-001-15)

**Behavior.** Canonical setup enables metadata capture before inference and
actual framework calls prove their own retained rows.
**RED.** Extend P9/P10 to exercise the documented setup and correlated retained
success checks; record the missing row/proof honestly, avoiding unrelated
fixture traffic. Already-enabled framework fixtures may provide verification-
only assertions rather than a production RED.
**GREEN.** Correct the prerequisite and missing retained-row assertions using
existing configuration/read/polling owners; no server-default change.
**REFACTOR.** Keep one canonical Connect setup inherited by framework examples.
Documentation/declaration/static checks and the requested rendered inspection
use non-TDD proof; record actual viewport/theme evidence or a concrete blocker.

## Focused commands — planned, not executed by review

Extend these actual owners with the missing assertions. Existing GREEN without
those assertions does not close the finding. Use normal setup/build prerequisites
once, confirm selectors with source/nextest list, and reject zero-test passes.
Record exact new child/regression selectors before executing them. Run Cargo
work sequentially and coordinate with concurrent implementation builds.

```bash
# P1 — local public caller; extend with actual Vertex-provider refusal
mise exec -- cargo nextest run --locked -p wyrd-client --test integration -E 'test(=gateway_caller::public_gateway_call_context_and_errors)'
# P2 — shared schema acceptance
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=authoring::tests::output_schemas_are_validated)'
# P3 — builder parity
mise exec -- cargo nextest run --locked -p wyrd-skald-prompt --lib -E 'test(=builder::builder_tests::builders_refuse_invalid_output_schemas)'
# P4 — real registration refuses invalid standalone/inline before writes
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_card_registration_route::invalid_prompt_authoring_is_refused_before_writes)'
# P5 — adapted JSON Schema dependency consumer, served OpenAPI contract
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_openapi_contract::gateway_ingress_publishes_typed_contracts)'
# P6 — adapted dependency consumer retains strict advertised MCP/UUID validation
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=principals::pg_tests::a_write_tool_is_scoped_at_dispatch_not_merely_hidden)'
# P7 — extend literal-model proof with direct typed native-body mismatch
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=authoring::tests::model_is_literal_configuration)'
# P8 — TypeScript authored preservation plus derived introspection
mise exec -- scripts/postgres/with-test-postgres.sh -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/register-and-hydrate.test.ts -t '^prompt forms round trip as authored$'
# P9 — same installed server, actual downloaded Agent loader and restart stage
mise exec -- scripts/postgres/with-test-postgres.sh -- node scripts/check_ts_package.mjs mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_installed_cli_gateway_setup
# P10 — existing real framework inference plus correlated retained rows
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/gateway/test_client_http_auth.py::test_openai_agents_sdk_runs_through_the_gateway tests/integration/gateway/test_client_http_auth.py::test_pydantic_ai_agent_runs_through_the_gateway
```

The installed CLI/server prerequisites remain the current task's normal
artifact setup, including `mise exec -- cargo build --locked -p wyrd-cli -p
wyrd-server --bins --tests` when rebuilding these artifacts is needed. Build the
actual existing Rust client integration child once using its final exact target;
record the command after its selector exists. Reuse managed Postgres/migration
setup from the owning lanes, including `db:migrate:all:inner` under the wrapper
when needed; do not assume an unprovisioned database.

After focused proof, run only applicable write-set checks: `mise run fmt`,
`mise run lints`, `mise run check:deps`, `mise run codegen:check`;
`mise run ts:format:check`, `mise run ts:lints`, `mise run ts:typecheck`,
`mise run ts:napi:check` for the TS projection; `mise run py:format`,
`mise run py:lints` and `mise run py:typecheck` for changed Python test helpers;
and `mise run docs:check`. Run the touched gateway example's existing owner
check if its executable files change. No new SQL change is selected here;
if a necessary correction touches SQL signatures/queries, retain TenantConn
and run `mise run check:tenant-isolation`. Document changed Rust private/test
items with applicable Errors/Panics/cancellation contracts in the same pass.
No new permanent checker or docs-only review round.

Do not prescribe full journey/family suites or `mise run gate` for remediation.
The full integrated aggregate remains change-review work. On failure, rerun
with tracing and read the trace before diagnosis; record the cause and fix
at that owner without weakening a test, timeout, gate or assertion.

## Acceptance and preserved behavior

| Criterion | Finding | Required observable closure |
|---|---|---|
| R2-AC-01 | FIND-001-3 | Actual local Vertex default/override refuses before IO; Google deployment spelling and native/server Vertex remain supported. |
| R2-AC-02 | FIND-001-5 | Standard malformed output schemas fail at shared construction/registration without writes or user-reference retrieval; valid nested/draft/local-reference/native/builder contracts are preserved. |
| R2-AC-03 | FIND-001-8 | Direct typed native request cannot introduce a separate renderable model; matching literal/slash and model-less generation remain valid. |
| R2-AC-04 | FIND-001-13 | TS standalone/Agent-navigated hydrated Prompts expose actual shared derived metadata; all four authored forms retain server JSON/hash with no stored duplicate fields. |
| R2-AC-05 | FIND-001-14 | Same installed journey loads and runs every downloaded Agent through actual public runtime loader, inspects preserved identities/config, and repeats same bundles after restart with normal auth, headers and correlated evidence. |
| R2-AC-06 | FIND-001-15 | Fresh documented setup enables metadata capture before its first request; actual framework examples read their own retained evidence; Connect guides/indexes/checks and honest rendered verification satisfy R1-DOC-AC-01–03. |

Preserve exact shared delegation/refusal policy, explicit grants beside All,
Open/Granted tenant visibility, pre-LIMIT batch pruning, actual-snapshot batch
authorization, claim release versus uncertain dispatch fences, and actual
admitted provider/deployment/model capture. Retain TenantConn/RLS, credential
sealing/provider/endpoint bindings, SSRF pinning, upstream header isolation,
effective subject authority and SYSTEM attribution, budgets/accounting, and
the existing nonblocking Scribe outbox. No request error may crash the shared
service. Keep existing admitted-snapshot lifecycle and native fail-closed rules.

Retain both authored Prompt forms, eight portable settings, text/image/document
core, media kind/encoding/size/MIME/URI validation, exact authored hashing and
inline Prompt materialization. Keep the required Agent route, whole-invocation
override/isolation, existing loops/tools/sessions/judges and direct-client
translations/explicit fallbacks. Workflow stays removed; independent Eval DAGs
and assertions remain. Approved adjacent changes remain allowed.

Non-goals: no local Vertex support, new scopes, model mapping/catalog/alias,
direct external bypass, portable audio/video, Python gateway host API solely
for the test, TS Agent runtime, hosted Agent jobs, replacement loader/loop/
transport/harness, media fetching, remote schema fetching, audit/capture queue,
default capture-policy change, compatibility layer or deployed-data migration.

## Completion evidence and next review

Append one evidence record per retained finding plus all three docs criteria:
actual source correction, exact focused command and nonempty result,
behavioral RED/GREEN or honest static/verification-only explanation, required
write-set check results and concrete environment limits. Record schema dependency
consumer adaptations and no-network behavior. Record actual loaded-Agent child
selector and demonstrate it uses the loader, not only an adjacent invocation.
Keep historic evidence intact; don't relabel compile-only RED or Card-scope
denial. Visual verification cannot be marked PASS if unperformed.

Return original task, approved r6, both prior verdict/ledgers, R1 and this R2
packet with a new immutable candidate to `$wyrd-task-review`; reassess the full
original-base-to-candidate range. These validated corrections are decision
complete. If a genuinely new material behavior/security/ownership/persistence
choice appears, name the exact conflict; do not turn routine private mechanics
into a new approval requirement. Review does not authorize merge, push or deploy.

## R2 implementation evidence

Integrated candidate on `change/deployment-addressing`. Every focused selector
below was rerun on the merged tree after all three fork merges (`7d9b97d6d`,
`1cfb38e2b`, `e80c288f4`).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-001-3 / R2-AC-01 | `5280c4855`: `PublicWyrdGatewayCaller::call` (`crates/shared/wyrd-client/src/gateway_caller.rs`) refuses `call.provider == ProviderName::Vertex` with `ProviderError::BadRequest` before projection or any IO; rustdoc `# Errors` and module doc updated. `ServerWyrdGatewayCaller` unchanged (server Vertex kept). The Run.invoke check in `observe/invoke.rs` is kept: it still answers an Agent invocation with the earlier `WYRD_AGENT_422_VALIDATION`; the caller guard now covers every other public-caller path. | P1 `mise exec -- cargo nextest run --locked -p wyrd-client --test integration -E 'test(=gateway_caller::public_gateway_call_context_and_errors)'` extended with Vertex-provider calls on a declared (`vertex/tuned/gemini-a`) and an overridden (`gemini/gemini-a`) route plus a zero-transport count. Behavioral RED: call was sent and answered `RemoteProblem{status: 404}` from the mock. GREEN: 1 passed. Google Prompt on deployment `vertex` stays a positive case in the same test. `cargo clippy -p wyrd-client --all-features --all-targets -D warnings` clean. | PASS |
| FIND-001-5 / R2-AC-02 | `e073affe1`: workspace `jsonschema` 0.18.3 → 0.46.4 (`default-features = false`), now a `skald-spec` dependency. `check_output_schema` meta-validates against the bundled meta-schema of the declared draft (unversioned → 2020-12) with error-returning `jsonschema::meta::validate`; the hand-written keyword walk (`check_schema`, `check_subschema`, `SCHEMA_TYPES`) is deleted; strict-OpenAI `type: object` root check kept. No network: a custom/unknown `$schema` is refused at `<field>.$schema` before resolution, user `$ref` targets are never followed, and `cargo tree -p wyrd-skald-spec -e normal` has no reqwest/rustls/hyper/tokio. Dependency consumers adapted with unchanged assertions: `pg_openapi_contract.rs` (`validator_for`), wyrd-mcp `principals.rs` (`options()...build`, still `should_validate_formats(true)` + `uuid`). `workspace-hack` regenerated with `cargo hakari generate`. Two existing error paths became element-precise (`response_format.required.1`, `...properties.a.required.0`). | Behavioral RED: 36 combinations (9 counterexamples incl. `minimum: "wrong"`, `$ref: 5`, nested `$defs`, draft-04 `exclusiveMinimum: 5`, custom `$schema`, × OpenAI/Anthropic/Google portable and Anthropic native) compiled before; builder accepted nested `minimum: "wrong"`; `pa-schema-minimum-prompt` registered 201. GREEN on merged tree: P2 `-p wyrd-skald-spec --lib -E 'test(=authoring::tests::output_schemas_are_validated)'` 1 passed; P3 `-p wyrd-skald-prompt --lib -E 'test(=builder::builder_tests::builders_refuse_invalid_output_schemas)'` 1 passed; P4+P5 (`pg_card_registration_route::invalid_prompt_authoring_is_refused_before_writes`, `pg_openapi_contract::gateway_ingress_publishes_typed_contracts`) 2 passed; P6 `principals::pg_tests::a_write_tool_is_scoped_at_dispatch_not_merely_hidden` 1 passed. `check:workspace-hack` pass. | PASS |
| FIND-001-8 / R2-AC-03 | `bb4ec9ef1`: `Prompt::new` runs `check_model` then refuses a Chat, Responses, Anthropic or Jev body whose model differs from the canonical model (field `body.model`), before schema checks and variable derivation; GenerateContent has no body model. Fixtures that built mismatched Prompts were aligned, not loosened (skald-spec `prompt_media` + one test, wyrd-spec `card/prompt/mod.rs` helper, skald-agent `agent_with_prompt_overrides_prompt_reference`). Example and SDK fixtures checked: no authored native body sets a differing `model`. | Behavioral RED: 12 cases (`{{target}}`, `${target}`, `other-model` vs `org/model-1` × 4 request types) accepted before; templated ones derived a `target` variable. GREEN on merged tree: P7 `-p wyrd-skald-spec --lib -E 'test(=authoring::tests::model_is_literal_configuration)'` 1 passed. Fork lanes: skald-spec lib 94/94; wyrd-spec, wyrd-cards, skald-prompt, skald-agent 998/998; wyrd-client lib, vala-eval, wyrd 365/365; `py:test:unit` 638 passed. | PASS |
| FIND-001-13 / R2-AC-04 | `23f9a8760`: TS `PromptCard` exposes read-only `variables`, `mediaVariables`, `responseSchema`, `responseSchemaName` (mirroring Python), for `state.prompt`/`state.get` and `agent.prompt()`. N-API `hydrated_prompt` (`sdks/wyrd-sdk-ts/native/src/cards.rs`) returns the stored envelope plus data read from the Rust Prompt, never recomputed; envelope, ref, spec and hash unchanged; declarations regenerated by `ts:build`. | Behavioral RED: P8 passed registration/GET/hydration/hash, then `TypeError: hydrated.variables is not iterable`. GREEN on merged tree after `ts:build` + `ts:build:testing`: P8 `vitest run tests/integration/register-and-hydrate.test.ts -t '^prompt forms round trip as authored$'` 1 passed (fork ran the file 12/12 incl. Agent navigation). Limit: no fixture has a `${media:...}` placeholder, so `mediaVariables` is asserted `[]` only. | PASS |
| FIND-001-14 / R2-AC-05 | `31f29df65`, `336b0a1b4`: new Rust child `crates/shared/wyrd-client/tests/integration/local_development.rs` (`#[ignore]`, driven by P9 before and after restart). For openai, anthropic, google, jev it opens the downloaded bundle with `WyrdState`, registers the pinned Prompt with `register_prompt_card`, loads via `Agent::from_card(card, &ToolRegistry::new(), default_prompt_resolver())`, asserts loaded route, `card_ref`, `id`, `to_card` ref, Prompt ref/route/provider/model before attaching only `PublicWyrdGatewayCaller` (Run id + Card UID), runs that instance with `run_prompt`, then polls `vala.gateway.calls` for deployment/model/resolved deployment/card UID/`succeeded`. No `Agent::new`, route setter or registry override. Driver asserts upstream calls `AGENT_CALLS * 2`; Card-scope 403, native-trap, upstream-credential and revocation checks unchanged. | Child selector: `mise exec -- cargo nextest run --locked -p wyrd-client --test integration --run-ignored=all -E 'test(=local_development::cli_gateway_agents)'` (listed by `nextest list`). First run failed on whole-spec equality (see risk below), narrowed to the criterion's ref/identity/route/provider/model; route itself was already preserved, so this is verification coverage, not a production change. Route-loss proof: deleting `.with_execution_route(...)` in `Agent::from_card` (`skald-agent/src/agent.rs`) failed P9 with `openai: loaded route Native`; reverted. GREEN on merged tree: P9 1 passed (23.4 s). | PASS |
| FIND-001-15 / R2-AC-06 | `31ff4f08c`, `c98e094b1`: setup guide step 5 applies `{ "mode": "metadata", "payload_fields": [] }` with `wyrd gateway capture-policy put --file capture-policy.json` before the first request, says a call made before it is never recorded; later steps renumbered. Framework page inherits it (prerequisite link to step 5, `capture-policy get` hint). `test_client_http_auth.py`: Agents SDK and Pydantic AI tests read their own retained rows via the existing `calls` helper (`flush_bifrost` then Bifrost SQL): exactly `[("openai","gpt-4o","succeeded")]` with a non-null `sub_principal_id`. Each test owns a fresh function-scoped `WyrdTestServer`, so rows are correlated to the test's request; no fixture inference exists. Server default unchanged. | P10 `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner; uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/gateway/test_client_http_auth.py::test_openai_agents_sdk_runs_through_the_gateway tests/integration/gateway/test_client_http_auth.py::test_pydantic_ai_agent_runs_through_the_gateway'` 2 passed. Verification-only (fixture already enabled capture); mutation removing the fixture's capture policy fails both with `assert [] == [('openai','gpt-4o','succeeded')]`, which is the guide's original defect. `docs:check`, `py:format`, `py:lints` pass. | PASS |
| R1-DOC-AC-01 | Historic `08b38f742`, `c35a718a8`, `5ca854d44`; R2 `31ff4f08c` adds the capture-policy step so the setup's own recorded-row step is reachable from a fresh tenant. | `docs:check` pass; command mirrors `test_installed_cli_gateway_setup` (which already puts `capture-policy.json`). | PASS |
| R1-DOC-AC-02 | Historic `08b38f742`, `416ddad3b`; R2 `31ff4f08c`: framework examples' recording claim conditioned on the inherited capture policy; Agents SDK and Pydantic AI tests now prove retained evidence. | P10 above, 2 passed. | PASS |
| R1-DOC-AC-03 | Pages unchanged in placement (Connect, orders 1–2, no `draft`). `c98e094b1` is a `human-tech-docs` audit pass on the R2 prose (wording only, no layout change): `metadata` never stores content and requires empty `payload_fields` (per `GatewayCaptureMode`/`GatewayCapturePolicyWrite::validate`), payload capture trade-off, and the `capture-policy get` diagnosis for a permanently empty result. | `docs:check` pass (build, links, Pagefind, a11y light+dark). Rendered inspection performed on the production build (`docs:preview`, port 4317) with Chrome: setup page at 1420px dark and light, and at a 386px viewport (390px iframe; the window manager ignored `resize_window`) light and dark, scrolled to the new step 5; framework page at 386px dark with the step-5 link resolving (200, anchor present). No horizontal page overflow at either width; code blocks scroll inside their box. Limits: themes switched via the `data-theme` attribute, not the toggle; narrow width via iframe, not a device window. | PASS |

**Merged-tree diagnosis (P9).** Symptom: the first merged P9 failed in the
Python child at `test_local_development.py:268`; the Card-scope refusal arrived
as `WYRD_SPEC_502_UPSTREAM_FAILURE` with message `Wyrd gateway answered HTTP
403`. Evidence: rerun with `WYRD_LOG=info` showed the 403 reached the client
but its code was dropped; `sdks/wyrd-sdk-python/python/wyrd/_wyrd.abi3.so` was
built at 17:14, before `b49ea15de` (17:30, "Keep a fieldless Bifrost refusal's
stable code when only the code crosses the wire"). Cause: `uv run` does not
rebuild the native extension, so the Python child ran pre-`b49ea15de` client
code. Fix site: none in source; `mise run py:setup` rebuilt the extension and
P9 and P10 passed. No test, assertion, timeout or retry changed.

**Write-set checks (merged tree).** `mise run fmt`, `lints`, `check:deps`,
`codegen:check`, `check:workspace-hack`, `ts:format:check`, `ts:lints`,
`ts:typecheck`, `ts:napi:check`, `py:format`, `py:lints`, `py:typecheck`,
`docs:check` all exit 0; `git diff --check` clean.

**Non-goals held.** No local Vertex support, default capture-policy change,
remote schema fetching, compatibility layer, new scope, or deployed-data
migration. No full journey suite or `mise run gate` was run.

**Material risk.** `Agent::to_spec` writes the default
`tool_concurrency_cap: Some(8)` into `run_config` where the registered Card has
`None`, so saving a loaded Agent with `to_card`/`to_yaml` changes its spec and
hash. Outside every R2 criterion; not changed here and flagged for decision.
The Run.invoke Vertex check in `observe/invoke.rs` is kept for its earlier
`WYRD_AGENT_422_VALIDATION` answer; the caller guard covers every other path.
