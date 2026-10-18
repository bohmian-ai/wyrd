---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-agent-readiness
spec_revision: 2
requirements: [REQ-001, REQ-007, REQ-008, REQ-009, REQ-010, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, AC-001, AC-007, AC-008, AC-009, AC-010, AC-011]
depends_on: [TASK-002]
---

# Teach and prove the complete agent onboarding workflow

## Outcome and Value

An unfamiliar coding agent can follow docs or MCP `start` from a running
local server to a declared application, SDK observation and real synchronous
and asynchronous judgments. Docs, embedded examples, agent references and
MCP guidance teach the final contracts already delivered by TASK-001/002.

Execute with `$wyrd-implement`. This dependency is semantic and protects
overlapping files: the guide must use merged get and final declaration/pull
names, while setup/docs must describe actual JSON readiness/errors. No UI tree
changes are expected here; the docs site's Svelte source is a separate owner.
Sibling standalone example repositories remain outside scope.

## Owners, Scope, Consumers, and Prohibited Changes

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| MCP first contact | `WyrdMcpHandler::get_info`, `mcp/guide.rs::{descriptor,response}`, `wyrd_spec::mcp::GuideTopic` | Real MCP discovery journey enumerates four topics and executes guide examples; selected-host proxy forwards instructions | Instructions/guide begin with investigation of an existing Service | Add the approved `start` topic and instructions, retaining scoped verification and investigation | No onboarding service or new declaration/upload/check tool |
| Docs and adaptation | `docs/src/content/docs` setup/quickstart/Declare/Observe/Verify/error pages | Existing docs content/link/command checks; local-development and support-desk journeys | Missing visible telemetry setup, inconsistent names, unclear buffering/activation statements and unverified outputs | Repair one executable own-app narrative, link actual public SDK helpers and capture observed outputs | Existing docs/SDK/example owners already provide the workflow |
| Snippets and machine reference | `generate_llms_txt.py::{sidebar_pages,render_pages_index,render_llms_txt,render_llms_full}`; `highlight.js::embedCodeFromFile`; docs content/router | Docs content/highlight tests; `docs:generate/check/build` | Full reference is another index; embeds hide source bodies from a plain-text reader | Generate actual page/snippet bodies with canonical start navigation and deploy-base-aware links | Extend existing content/render contract; no independent Markdown/reference product |
| SDK runtime recipe | `WyrdState`, Run/Observe; `examples/support-desk-app` initialization/answer/check/submit; canonical `examples/support-desk` | Three runtime-owned real-server support-desk stories and Python local-development story | Quickstart embeds a different app; Rust/TS copies need clear runnability; own-app telemetry/Model guidance incomplete | Align documented/tested recipe, visible initialization and adaptation; validate each language's public operations | No new SDK engine, framework patching or duplicate app bootstrap |
| Foreground setup guidance | Existing setup page, `DevServer` readiness and saved-client resolution | TASK-002 JSON readiness/host proof; three-target authority | Agent may let the foreground server's shell lifetime end | Explain person-owned terminal/background job and wait-for-readiness, then agent connects | Docs-only AR-6; no lifecycle feature |

Keep MCP's roles precise: teach, read Cards/evidence and run verification;
CLI declares/pulls; SDKs instrument the application. Local check never contacts
a server and there is no MCP check tool. Declaration does not return a secret
or substitute for workload credential issuance. No arbitrary local path is
server-readable evidence. Existing evidence planning is not a byte upload.

Teach Agent, Prompt and heavy Model components without treating gateway model
strings as Model Cards or bypassing predeclaration/artifact rules. Use one
consistent runnable Service, table, aliases, bindings and slots across the
main path, with explicit adaptation guidance for another app. Explain actual
package acquisition/extras, telemetry/framework hooks, card views, custom
schema, flush/shutdown/readback, synchronous versus queued/window semantics,
delayed visibility, terminal errors and existing abrupt-loss guarantees.

Do not invent new durable concepts, scheduler activation, SDK behavior or
acknowledgment guarantees. Keep the error catalog source-derived. No detached
mode, new stop/process manager, UI feature, compatibility aliases, arbitrary
code runtime or sibling repo edits. Preserve other contributors' changes.

## Approach

1. Pin final-name MCP first-contact and generator failures; identify the
   published recipe's concrete runtime/static proof before changing its prose.
2. Add `start` and server instructions through existing typed guide ownership,
   with executable links/commands and current catalog/permission guidance.
3. Align setup and Declare/Observe/Verify/error/SDK examples around one
   consistent recipe and its own-app adaptation, using existing capabilities.
4. Generate full page/embedded source text, retain compact index discovery and
   make onboarding links usable from the locally served build as well.
5. Execute the quickstart with local provider fixtures, preserve the three
   SDK/MCP journeys and capture honest output/evidence before changing claims.
6. Run focused docs/contracts/runtime proof and append acceptance evidence.

## Ordered Implementation Scenarios

This is mixed work. Guide topic/catalog and generator logic require TDD.
Docs prose, static authority alignment and already-working SDK operations use
the non-manufactured proof described below. Do not add a generic docs-command
interpreter, additional application framework or duplicate example engine.

### Scenario 1 — MCP first contact teaches the final complete workflow

**Behavior.** Initialize instructions lead to `wyrd.guide {"topic":"start"}`.
Authenticated readers receive bounded, tenant-free workflow guidance. Guide
topics and examples use final names, disclose required CLI/SDK/auth steps,
retain investigation and grant no permissions (REQ-001/007, AC-001/007).

**RED.** Extend P1's actual MCP discovery journey to request start, validate
its schema/examples and inspect initialization instructions. Assert absence
of declaration/upload/check tools, retained merged get and unchanged write
gating. Baseline topic enumeration/refusal and instructions expose the gap.

**GREEN.** Add the topic/response and initialization guidance through the
current owner. Use CLI/SDK examples as guidance, not fictitious MCP tools.
Pass P1 and P2's installed-proxy instruction forwarding checks.

**REFACTOR.** Share current guide/example/descriptor helpers where applicable;
do not create an onboarding server or grant authority through discovery.

### Scenario 2 — The full machine reference contains the actual recipe

**Behavior.** Compact llms discovery points to start; full reference contains
published page bodies and actual embedded code/YAML, in navigable order, with
working deploy-base-aware links and no unresolved embed markers. Source edits
update output through generation (REQ-009, AC-009).

**RED.** Add focused assertions at the existing generator/content test owner
for a real onboarding page, a selected source region/range and a missing or
invalid embed. The baseline full output is only an index. Use P3/P4 as the
confirmed current docs test/build entry points; record exact new test
selectors after collection. Fail on omitted bodies, not on formatting trivia.

**GREEN.** Extend current generation using the owning content/snippet rules;
regenerate from sources and verify start, code bodies and links. Align link
resolution with local served docs required by the live lane, preserving the
deployment base. Pass focused assertions and earlier guide proof.

**REFACTOR.** Reuse the existing source/extraction contract instead of keeping
a second hand-maintained recipe or parallel content parser/rendering system.

### Scenario 3 — The documented application produces attributable evidence

**Behavior.** The published quickstart actually executes: declaration/check,
pull/load, telemetry and custom rows, synchronous verdict and asynchronous
terminal result. Another-app adaptation and all three SDKs' operations are
discoverable. Unverified output is replaced only by captured execution
(REQ-001/008/009, AC-001/008/009).

**RED.** P5 is a planned addition to the inspected Python support-desk
integration owner for the newer published quickstart app/recipe. Exercise its
real code and commands with local provider fixtures and server readback,
including missing/invalid binding/input and insufficient privilege. Do not
mistake the older canonical example's success for quickstart coverage. If an
operation already works, keep its regression proof rather than manufacture
RED; omissions of visible snippets/authoring instructions require static proof.

**GREEN.** Correct the actual in-repository recipe and consumer/example seams
needed for P5; show complete initialization in the docs, retain canonical
journeys P6-P8, and verify the Rust/TS snippets against their public SDKs.
Read retained evidence and record real expected outputs. Pass earlier guide
and generation proofs; do not change SDK/server semantics to fit the prose.

**REFACTOR.** Delete duplicate/conflicting snippets and embed their owning
source where practical. Keep one recipe with explicit language/run destinations,
not three divergent descriptions or dependency on sibling repo edits.

## Acceptance Criteria

| Criterion | Spec obligations | Proof |
|---|---|---|
| Docs/MCP alone teach final Declare -> Observe -> Verify on an own app | REQ-001/007, AC-001/007 | P1/P2/P5, step/credential/link source review and actual connected catalog |
| Visible recipes cover telemetry, custom rows, sync/async and correct semantics in all SDKs | REQ-008, AC-008 | P5-P8, executable snippet checks and retained negative flows |
| llms full contains generated bodies/embeds; errors/help/output claims agree | REQ-009, AC-009 | P3/P4/P5, actual generated output and captured evidence |
| Setup teaches a person-owned foreground/background job and readiness, no new lifetime | REQ-001/010, AC-010 contribution | Setup/MCP source review against TASK-002's actual output; no detached/stop instructions |
| Owners/auth/durability remain; deterministic proof supports the later live lane | INV-001-007, AC-011 contribution | Focused regressions, source review and non-goal closure |

## Expected Write Set and Consumer Closure

- Server MCP instructions/guide/catalog and pure guide topic schema plus
  runtime MCP discovery/proxy consumers/tests; no new write capability.
- Main setup/quickstart/Declare/Observe/Verify/concept/SDK/error guidance;
  `docs/scripts/generate_llms_txt.py`, source extraction/render integration
  where necessary, generated agent/API references and docs tests.
- `examples/support-desk-app`, canonical `examples/support-desk`, public SDK
  documentation/authored stub documentation and owning runtime journey tests
  required for recipe continuity; existing packages remain the implementation.
- Relevant docs and exact-test registration in mise if needed. Do not add
  the live lane yet; TASK-004 owns it. No `wyrd-ui` tree or sibling repos.

## Verification and Evidence

P5 is planned under the existing inspected Python module, runner and marker;
confirm collection once authored. P1/P2/P6-P8 exist in current owners. A renamed
exact selector after TASK-001 must be collected, run and recorded equivalently.
Build native SDK/CLI/testing binaries with their existing prerequisites before
runtime proof. Local fixtures replace only external providers, not Wyrd client
or server paths. No provider or coding-agent credentials are needed here.

| Proof | Exact focused command |
|---|---|
| P1 guide catalog and real examples | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=discovery::pg_tests::agent_discovers_mcp_guide_and_sql_examples)'` |
| P2 installed selected-host instruction forwarding | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_selected_mcp_host_reads)'` |
| P3 current content/embed unit owners | `mise exec -- pnpm --dir docs exec vitest run src/lib/content.test.ts src/lib/mdsvex/highlight.test.ts` |
| P4 owning generated-doc/build/link/command checks | `mise run docs:check` |
| P5 planned published quickstart journey | `mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_support_desk.py::test_published_quickstart_declares_observes_and_verifies` |
| P6 retained Python app/evidence journey | `mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_support_desk.py::test_support_desk_answers_verifies_and_explains_every_request` |
| P7 retained Rust app/evidence journey | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=support_desk::support_desk_answers_verifies_and_explains_every_request)'` |
| P8 retained TypeScript app/evidence journey | `mise exec -- scripts/postgres/with-test-postgres.sh -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/support-desk.test.ts -t '^support desk answers, verifies, and explains every request$'` |

P6-P8 are focused tests for changed shared example contracts, not permission
to run full SDK suites. Confirm the quickstart's language snippets compile or
execute as their documentation claims, without treating reference copies as
complete sibling projects. Record exact additional selectors if new assertions
are split into smaller tests. Do not require a generic snippet runner.

Non-TDD proof: inspect full public text for consistent aliases/slots and the
person-owned server prerequisite; compare static SDK docs to current stubs;
read actual rendered/generated pages and captured Wyrd outputs. Preserve
uncertainty where proof is unavailable. `docs:check` owns docs generation and
its format/link/build checks; generated artifacts are source-driven.

Run `mise run fmt`, `mise run lints`, `mise run codegen:check` for MCP/schema
work and `mise run check:examples` for shared example changes. Use the owning
Python/TS format/lint/type checks for changed sources. No broad aggregate or
complete journey suites here; integrated change review runs them once.
Append acceptance/non-goal evidence, exact command results and observed output
provenance. Finish with `git diff --check` and tracked/untracked diff review.
Do not stage, commit, edit the spec or plan review remediation.

## Material Stop Conditions

Stop if the recipe needs a new runtime/API/credential or Model declaration
contract, MCP write/check capability, altered schedule/acknowledgment/visibility
guarantee, platform prerequisite or sibling-repository change. Report an
unavailable real package/artifact as an evidence blocker; do not claim it is
published. Existing SDK functionality may be documented and locally adapted
without reopening settled owner decisions. Rendering/extraction/test placement
are reversible choices within existing owners.

## Authority Links

- [Approved spec revision 2](../spec.md), [TASK-001](TASK-001-declare-get-pull.md),
  [TASK-002](TASK-002-machine-readable-local-tooling.md).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md),
  [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-doctrine.mdx).
- [Router](../../../../architecture/references/README.md), [spec workflow](../../../../architecture/references/languages/spec-driven-development.md),
  [execution](../../../../architecture/references/languages/implementation-execution.md), [testing](../../../../architecture/references/languages/testing-workflows.md).
- [Agent harness](../../../../architecture/references/languages/agent-harness.md), [errors](../../../../architecture/references/languages/errors.md),
  [telemetry](../../../../architecture/references/domain/telemetry-observations.md), [evaluation](../../../../architecture/references/domain/evaluation.md),
  [security](../../../../architecture/wyrd-security-posture.md), [Bifrost](../../../../architecture/bifrost-design.md).

## Implementation Evidence

Commits: `d31002a61`, `47ac6e1b0`, `ec58c4fd9`, `aa3841193`, `fb89cb4c7`,
`effa77458` (the user asked for frequent commits, which overrides "do not
commit").

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Docs/MCP alone teach final Declare -> Observe -> Verify on an own app | `GuideTopic::Start` (`crates/wyrd-spec/src/mcp.rs`); `start` guide steps and initialization instructions (`crates/wyrd/wyrd-server/src/mcp/{guide,mod}.rs`); quickstart "Adapt it to your own app"; `reference/mcp-tools.md`, `connect/connect-an-agent-through-mcp.md` | P1 asserts instructions name `wyrd.guide {"topic":"start"}`, start steps carry setup/declare/get/pull/observe/verify/readback/Model Card text, and no declare/pull/check/upload/get_by_ref tool exists; P2 and `mcp_proxy_discovers_and_reads_with_shared_auth` assert forwarded instructions through the CLI proxy; P5 runs the documented path | PASS |
| Visible recipes cover telemetry, custom rows, sync/async and correct semantics in all SDKs | Quickstart embeds `init` (`desk.py`) and `lifespan` (`app.py`) regions, adds `wyrd declare ../service --check --format json`, sync `verify` vs staged `submit`, transient not-found, terminal statuses, buffered/delayed evidence; Rust `lib.rs` compiles as `examples/rust` `[lib] support_desk_app`; TypeScript copy is a pnpm workspace package type-checked by `ts:typecheck`; README states this | P5 (declare check, viewer refusal `WYRD_PERMISSION_403_DENIED_RBAC`, both sync verdicts, `missing order_id` failure, `WYRD_SDK_404_UNKNOWN_VERIFIER`, `WYRD_VERIFICATION_400_INPUT_INVALID`, queued review to completed with line citations, post-shutdown evidence rows); P6-P8; `check:examples`; `ts:typecheck` | PASS |
| llms full contains generated bodies/embeds; errors/help/output claims agree | `docs/src/lib/mdsvex/page-text.js` (+ test), `docs/scripts/page_text.mjs`, `generate_llms_txt.py` (base-relative links, Start here, bodies); `embedCodeFromFile` pluggable renderer; CLI remediation names `pip install -U wyrd-ai` / `npm install -g wyrd-ai@latest`; MCP install help no longer leaks a Rust type name; quickstart outputs captured from P5, not derived | P3 (42 vitest tests incl. bodies, embeds, no component markers, base links); P4 `docs:check` exit 0 (generate drift, build, links, a11y); `codegen:check` | PASS |
| Setup teaches a person-owned foreground/background job and readiness, no new lifetime | `get-started/setup.md`: foreground lifetime owned by the person, background job with `--format json` readiness line and failure semantics, `kill -INT` stop, MCP install `--format json` then reload, agent never starts/stops the server | Source review against TASK-002's readiness JSON fields; no detached or stop API instruction added | PASS |
| Owners/auth/durability remain; deterministic proof supports the later live lane | No SDK or server semantic change; guide is read-only; local provider fixture replaces only the external model | P1, P2, P5-P8; `check:deps` (axum in examples crate) | PASS |

Reuse/gap: the guide topic extends the existing `wyrd.guide` owner;
llms-full reuses the build-time `CodeFromFile` expansion through a renderer
parameter rather than a second embed parser; P5 extends the existing
support-desk integration module and its local upstream. The Rust and
TypeScript reference copies reuse the existing `examples/rust` crate and
the SDK pnpm workspace instead of new projects.

Commands (all exit 0 on the final tree):

- P1 `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=discovery::pg_tests::agent_discovers_mcp_guide_and_sql_examples)'` — 1 passed
- P2 `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_selected_mcp_host_reads)'` — 1 passed; also `-E 'test(=mcp_proxy_discovers_and_reads_with_shared_auth)'` — 1 passed (after `cargo build --locked -p wyrd-cli -p wyrd-server --bins`)
- P3 `mise exec -- pnpm --dir docs exec vitest run src/lib/content.test.ts src/lib/mdsvex/highlight.test.ts src/lib/mdsvex/page-text.test.ts src/lib/indexes.test.ts` — 4 files, 42 passed
- P4 `mise run docs:check` — exit 0, 32 pages a11y OK
- P5 `... pytest -q -m integration tests/integration/test_support_desk.py::test_published_quickstart_declares_observes_and_verifies` — 1 passed (needs `mise run build:server` for the code-check sandbox)
- P6 `...::test_support_desk_answers_verifies_and_explains_every_request` — 1 passed
- P7 `... cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=support_desk::support_desk_answers_verifies_and_explains_every_request)'` — 1 passed
- P8 `... pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/support-desk.test.ts -t '^support desk answers, verifies, and explains every request$'` — 1 passed
- `mise run fmt`, `lints`, `codegen:check`, `check:examples`, `check:deps`, `ts:typecheck`, `py:lints`, `py:typecheck`; `git diff --check` — all exit 0

Observed output provenance: quickstart replies, the transient
`WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND` read, the completed review and the
evidence rows were printed by a P5 run on this tree. Answer and letter text
come from the local provider fixture and are labelled as such.

Non-goals held: no MCP write/check/upload tool (P1 asserts), no detached
mode or stop API, no `wyrd-ui` change, no sibling repository edit, no SDK or
server semantic change, no compatibility alias, no live lane.

Limits and risks:

- Evidence blocker for a published walkthrough: every published artifact
  (PyPI/npm `wyrd-ai`, GitHub release, `bohmian/wyrd` images) is 0.1.1, which
  predates declare/get/pull and JSON setup output, and a 0.2.0 CLI refuses a
  0.1.1 server (`WYRD_CLI_409_SERVER_VERSION_INCOMPATIBLE`). Setup and the
  quickstart state this; neither claims 0.2.0 is published. A release is
  required before the docs run from published packages.
- The quickstart keeps `status: partial`: it ran against a local model
  fixture only; a real provider run belongs to TASK-004's live lane.
- Proof ran on Linux x86_64 only.
