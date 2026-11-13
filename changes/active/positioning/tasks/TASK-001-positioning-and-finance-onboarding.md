---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-positioning
spec_revision: 3
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007]
depends_on: []
approved_design: ../design/landing-animation.md
---

# Lead public entry points with a runnable finance verification

## Outcome and Value

A visitor sees the agreed real-time positioning, understands why an application
would call Wyrd, and can run the existing finance example as the primary
quickstart. The application receives three requirement results and uses their
combined verdict to release or hold a synthetic report. README and homepage
show that proof before infrastructure detail.

The current finance programs already declare a Service, three requirements and
a gateway policy Agent. Adapt those programs instead of writing another demo.
Replace their bank-account assertion with destination controls; the policy
Agent must read the policy PDF extraction and judge the prohibited identifier.
Add the TypeScript equivalent and explicit release/hold handling.

## What Changes and What Must Stay True

Own public positioning, Introduction/Setup/Quickstart, the existing finance
programs/assets, mechanically coupled snippets, package/example entry copy and
reader-facing output. Bind one Verifier with recipient, policy and destination
requirements to one Service. No mandatory step graph, concurrency proof or new
runtime behavior. Reuse supported SDK authoring/declaration and gateway/tool
permissions. Only passed permits release; uncertainty and typed errors hold.

Preserve immutable policy attribution, synthetic evidence, existing routes,
brand, six sections and accessibility. State that recipient/destination facts
are supplied records and the application owns the decision. Include actual
provider setup, data exposure and supported release pins. The added provider
cost/setup must not be hidden by scripted tests.

Keep a compact public-surface disposition in execution evidence; Task 2 extends
it for educational pages and Task 4 verifies complete discovery coverage. Apply
this task's bounded ownership handoff in the older docsite packet using the
approved spec's transfer; do not revise its unrelated decisions. Task 3 records
its tutorial-specific handoff when taking over those pages.

Task 2 owns the new learning pages/FAQ. Task 4 owns shared metadata, semantic
rendering and sitemap repairs. Here provide positioning text, frontmatter,
links and homepage presentation; do not preempt those shared repairs. Link only
to currently published learning content until Task 2's foundation is proven.
The initial public launch must also include that foundation and FAQ, and wait
for Task 4's required discovery repairs/checks. Completing this task alone
does not satisfy the initial publication set or the rollout.

### Locked homepage animation addition

The maintainer approved the finance animation as part of this task. Ship one
left-to-right composition based on the final [direction B mockup](../design/mockups/directions/index.html#b)
and [captures/limits](../design/mockups/directions/README.md); the other directions
and review selectors are exploration artifacts, not three production options.

- Place it below the existing hero, spanning the landing page content width as
  a wide, shallow band. Preserve the existing hero and CoverScene background.
  The reviewed desktop diagram is about 1240 × 331px; adapt responsively without
  turning it into a tall architecture dashboard or overflowing the page.
- Make real-time verification the primary subject in periwinkle. Gateway is
  an actual secondary feature in amber; Bifrost/evidence is secondary in lime.
  Reuse current brand tokens, fonts and docsite syntax colors.
- Show “Your Finance API” with `from wyrd import WyrdState`, state setup,
  `@api.post(...)`, the Run context and verification call, followed by the
  application's release/hold response. Preserve conventional Python formatting:
  one blank after the import, two before the top-level decorated function,
  decorator adjacent to `def`, four-space indents and matching line numbers.
  Keep the final syntax coloring and readable code size. Replace the mockup's
  handwritten sketch with a source-backed excerpt from a journey-tested API
  example using existing integration support; this is not a new SDK API or a
  second standalone finance quickstart.
- Show one Verifier with three requirements: recipient assertion, policy-PDF
  Agent judgment and destination assertion. Only the Agent's model/provider
  call traverses Gateway. The verdict returns before the API releases or holds.
  Records, traces and supplied/configured application data reach Bifrost through
  a clearly separate asynchronous capture path; response never waits for its
  publication. Do not imply every Agent tool call uses Gateway or all application
  data is collected automatically.
- Preserve the fast, illustrative all-pass loop and readable resting state
  (reviewed at about 2.7 seconds). Failure must show the application's hold
  branch; it may remain a review/test case rather than a shipped selector.
  Provide accessible text/static output, reduced motion, a way to pause looping
  motion, and stop animation offscreen or in a hidden tab. Keep complete meaning
  in initial HTML; motion timing is not measured verification/model/storage
  latency. No throughput, compression or savings claims are approved.

The mockup locks the story, hierarchy and formatting, not its duplicated HTML,
SVG templates or timer implementation. Reuse existing Svelte/component/source
owners and keep new code local to the homepage presentation.

### Locked lower-page workflow strip

The maintainer also approved the [four-panel workflow mockup](../design/mockups/workflows/index.html#workflows)
and its [browser evidence](../design/mockups/workflows/README.md). Add this second
wide, shallow section toward the bottom of the landing page, complementing the
detailed finance animation. Use “One verification engine. Build your workflow.”
with four animations in one desktop row, separated by dashed vertical rules:

| Panel | Required visual story |
|---|---|
| Real-time verification | Request reaches the user's API, Wyrd verifies, the verdict returns, the API decides. |
| Agent evaluation | Plan → retrieve/tool → compose, with evidence from multiple steps accumulating into a run record; submit/queued evaluation then produces a verdict. |
| Code review | A change fans out to multiple agent reviewers (correctness, security, tests); parallel results fan in to a combined verification result. |
| Session evaluation | Turns, tool calls and events visibly accumulate as session information, then the grouped information passes through a Verifier to a session verdict. |

Preserve the reviewed quiet geometry, periwinkle verification, amber agent work,
lime evidence and labeled success, with coordinated fast motion and a readable
finished state. The reviewed desktop diagrams are about 203px tall. Keep titles,
one short description per panel and clear multi-step/fanout/accumulation shapes;
do not repeat four code editors or the detailed Gateway/Bifrost diagram. Reflow
to two columns/tablet and one/phone, retaining readable labels and no page
horizontal overflow. Apply the same pause, reduced-motion/static alternatives,
initial HTML and offscreen/hidden suspension as the finance animation.

These are ways users build on Wyrd's engine; no turnkey code-review product,
new agent orchestration runtime or backend implementation is added by this task.
Do not add roadmap/support disclaimers to the approved visual. Reuse native
homepage styling and shared motion behavior where appropriate; the prototype's
copied hero, review note and duplicated timer are not production requirements.

## Approach

1. Inventory affected entry surfaces and revalidate the existing finance
   declarations, gateway policy reader, actual-program tests and consumers.
2. Execute the four scenarios below; extend existing public SDK examples and
   journey support with the smallest missing behavior.
3. Replace the primary onboarding story, captured output, setup and embedded
   source together. Preserve useful integrity/refusal coverage.
4. Align README/homepage, package introductions and `docs/PRODUCT.md` around
   the exact headline/question and the demonstrated application decision.
5. Integrate the agreed below-hero animation using the tested API excerpt and
   add the approved lower-page four-workflow row; verify both sections' visual
   hierarchy, ordering, accessibility and motion behavior.
6. Regenerate coupled agent text, run focused parity/render checks, and walk
   through the rendered guide at the documented release with its real provider.

## Ordered Implementation Scenarios

### Scenario 1 — Verify and act on three distinct requirements

**Behavior.** The actual Python, Rust and TypeScript programs declare a Service
with one three-requirement Verifier. Each proposed release uses one synchronous
call. Allowed release passes; prohibited identifier, unauthorized recipient and
inadequate destination independently fail the intended requirement and hold.
The gateway policy Agent reads the selected extraction before judging policy;
assertions judge supplied recipient/destination facts. Print reasons/results
and an application decision, not three separate verification calls. An
unchanged replay preserves exact registered identities.

**RED.** Extend the existing Rust finance program journey and the Python/TS
quickstart journeys to run the promoted programs with these observations. The
old programs lack destination checks/decisions and their always-approving local
policy response does not prove policy use. See [Rust story](#rust-story),
[Python story](#python-story) and [TypeScript story](#typescript-story).
Confirm the failure concerns missing behavior, not missing provider setup.

**GREEN.** Adapt the existing programs/PDF, declaration and local provider
support; add TypeScript parity. Update copied assets and expected output.
Prove the declared graph, actual policy read and independent result branches.
Keep existing authority, immutable-content and broken-reference assertions.

**REFACTOR.** Keep one authoritative finance story and source-backed snippets.
Remove duplicate onboarding orchestration or route old consumers through the
promoted program; retain the model-free assertion example only as supplemental
material where useful. Keep all three actual-program journeys green.

### Scenario 2 — Hold on inconclusive and request/execution errors

**Behavior.** Missing evidence and provider uncertainty follow current native
semantics; an inconclusive judgment is not failed or passed. Invalid inputs and
provider failures that are typed errors stay errors. All such paths hold and
never simulate release; visible reasons and stable error codes explain why.
Independent requirements retain their actual outcomes.

**RED.** Add the planned Rust finance uncertainty journey and extend each SDK's
focused story for its decision/error branch. See [uncertainty](#uncertainty).
Fail because the example's current decision handling is absent/inadequate.
Do not redefine a runtime error as an inconclusive fixture expectation.

**GREEN.** Add minimal idiomatic application handling and local provider cases;
keep the public outcome/error contract intact and rerun Scenario 1.

**REFACTOR.** Keep the decision visible in each language's example. Remove
redundant result conversions; introduce no SDK wrapper or new verdict.

### Scenario 3 — Reuse policy and preserve version attribution

**Behavior.** Two requests reuse the uploaded policy extraction. A new policy
and declaration select new exact policy/Verifier/binding versions and produce
the expected changed-policy result. The old declaration still selects its old
policy. Public results/details identify the exact versions and cited evidence;
labels printed without actual selected-policy proof do not satisfy this.

**RED.** Add the planned Rust policy-revision journey, reusing the actual
program/declaration seam, and extend the Python/TS focused stories for reuse
and exact identities. See [policy revision](#policy-revision). Fail for the
missing example/revision behavior while retaining existing server contracts.

**GREEN.** Reuse local Artifact publication, fixed workspace selection and
server version selection. Document the brief policy-update follow-on and
rerun previous scenarios. No manual parent-version algorithm or mutable
latest-policy pointer is needed.

**REFACTOR.** Remove per-request upload/extraction and duplicated publication
logic. Preserve immutable history and independent SDK examples.

### Scenario 4 — Understand embedded verification and the workflows it enables

**Behavior.** A homepage visitor recognizes their API calling Wyrd, sees three
requirements judged and the verdict returned, then sees the API decide. Gateway
serves only the Agent model call; Bifrost capture happens after the response.
The full-width shallow diagram remains understandable without animation or
JavaScript and usable on narrow screens. Python import/decorator, conventional
spacing, token colors and source-backed logic make the embedding explicit.
Farther down the page, four distinct compact animations show real-time checks,
multi-step queued agent evaluation, parallel agent-review fanout/fan-in, and
session accumulation followed by verification. Desktop uses one row with dashed
vertical separators; tablet/phone reflow preserves readability.

**RED.** Extend the existing server-rendered homepage regression with the planned
[homepage animation proof](#homepage-animation-proof), showing the missing
embedded API story, four workflow stories, accessible static content and
source-backed excerpt.
Observe the initial missing animation in the browser for ordering, pause,
replay if shipped, reduced motion and offscreen/hidden behavior. Do not pretend
an SSR assertion proves browser timing or motion accessibility.

**GREEN.** Extend the existing homepage/component and tested API example owners
minimally, using the agreed mockup as visual reference. Supply static/text
alternatives and animation states for both sections together. Prove pass/hold rendering and the
asynchronous capture boundary without making backend calls from the animation.
Run the render regression and record actual desktop/mobile/browser observations.

**REFACTOR.** Reuse current source embedding, syntax highlighting, tokens and
component lifecycle. Remove prototype direction selectors, copied hero/font
assets and duplicate content/timers. Keep the tested API source authoritative
and prior finance/onboarding proof green.

## Acceptance Criteria

| Obligations | Required observation |
|---|---|
| REQ-002–004; AC-001–003; INV-001–003 | Scenarios 1–2 execute each actual program against a real server, expose the three branches and policy read, and release only on passed. Negative/error paths hold without new runtime behavior. |
| REQ-005; AC-004; INV-003 | Scenario 3 proves real policy reuse, changed selected versions and unchanged historical selection through public inspection. |
| REQ-006; AC-005; INV-002, INV-004–005 | Rendered released-package setup and provider walkthrough reproduce the example, visible output and follow-on; no-model instructions and stale install pins are corrected. |
| REQ-001, REQ-007; AC-006; INV-004 | Entry pages use the fixed claim/question, concise Python proof with other-language links and usable CTA/learning links before infrastructure. Brand, routes and keyboard/mobile behavior remain intact. |
| REQ-007; AC-006; INV-001, INV-004–006 | Scenario 4 and the locked design addition establish an explicit embedded API example, primary real-time verification, secondary Gateway/Bifrost, response-before-capture ordering, conventional Python formatting, the four distinct lower-page workflows, static accessibility and responsive motion controls. |
| REQ-008; AC-007; INV-001, INV-004–006 | Public-surface dispositions, bounded overlap handoff, editorial/package alignment and coupled source/generated consumers agree. No marketing rewrite of technical authorities. |

## Implementation Reference

Paths identify inspected owners and gaps, not an allowlist.

- **Finance:** `examples/finance-release/finance_release.py` builds `policy`,
  `policy_judge`, `release_check`, `service` and calls `WyrdState.declare`;
  `finance_release.rs::main` / `release_check` mirror it. Existing
  `examples/tests/finance_release.rs::finance_release_passes_and_fails` runs
  the actual binary from a reader directory, but its local provider always
  approves. Missing: destination evidence, policy-dependent failure proof,
  release/hold, TypeScript and primary-doc consumers. Extend this workflow.
- **Publication/policy:** `Cards::upload_artifact`, `Artifact::from_path`,
  `AgentCall.workspace` and `VerificationEngine::attach` already own stored
  extraction and fixed selection. Rust `authoring.rs::judge_policy`, Python
  `cards/test_authoring_journey.py`, and TS `authoring.test.ts` script an actual
  `fs.read` and cited policy judgment. Reuse these complete patterns/assets;
  their two-Verifier story is not a replacement quickstart.
- **Caller/result:** `Observe::verify`, `VerificationControl::execute` and
  `Execution::details` own execution/inspection. Current `verify` namespaces
  expose typed composition. Preserve the distinction between compact returned
  results and separately retained evidence; no persistence redesign.
- **Actual-program consumers:** `examples/tests/quickstart.rs`, Python
  `test_docsite_quickstart.py::run_quickstart`, and TS
  `declare-and-pull.test.ts` explicitly copy old files/assert old output.
  Redirect their primary-reader proof to the promoted example or justify
  supplemental-only coverage; do not keep misleading names/instructions.
- **Entry UI:** `docs/src/routes/+page.server.ts::load` inlines old quickstart
  files and constructs an old failure; `+page.svelte` renders the hero/strip/
  example/section doors. `readme.test.ts` compares marked README source and
  diagram parity; `indexes.test.ts` pins old embedded snippets. Replace those
  story assumptions from captured results, preserving generic parity rules.
- **Animation reuse:** `+page.svelte` already composes `CoverScene`,
  `CodeBlock`, language tabs and the homepage sections. `shell-links.test.ts`
  imports/SSR-renders `Landing` with existing runtime stubs. Docsite
  `--code-keyword`, `--code-string`, `--code-number` and `--code-muted` tokens
  already own syntax colors. Gap: the agreed embedded API animation and source
  excerpt; extend these owners instead of copying the mockup site or adding a
  highlighting/motion library. `examples/integrate-fastapi/app.py` already shows
  the route/Wyrd integration seam; reconcile the finance excerpt with supported
  synchronous request handling and lifecycle, and prove the displayed source.
- **Editorial:** Get started content, Author in code embeds, public SDK
  README/package descriptions, `examples/README.md` and `docs/PRODUCT.md` are
  existing surfaces. `generate_llms_txt.py::START` / `SUMMARY` still name the
  assertion story; update its source strings and regenerate rather than edit
  output. No new content-fetch layer or second finance-example owner.

## Verification and Evidence

Run repository commands sequentially. Existing owning setup is
`mise run build:server`, `mise run py:setup`, `mise run ts:build`,
`mise run ts:build:testing`, and `mise run docs:install` for the surfaces used.
These are build/setup prerequisites, not runtime proof. Rust examples package
`wyrd-rust-examples` has the existing `finance_release` integration target;
Python uses pytest integration and TS uses package Vitest with local fixtures.
The wrapper provisions Postgres; the test owns the local provider.

Planned selectors below do not exist yet. Confirm each actual selector and
nonempty collection after adding it, before RED. Existing selectors were read
from source; equivalent stronger ownership/selector repairs are implementation
choices and must be recorded.

### Rust story

Existing actual-program test; extend it to the new story:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-rust-examples --test finance_release -P journey --run-ignored=all -E 'test(=finance_release_passes_and_fails)'"
```

### Uncertainty

Planned test in that same existing target:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-rust-examples --test finance_release -P journey --run-ignored=all -E 'test(=finance_release_holds_uncertain_releases)'"
```

### Policy revision

Planned test in that same existing target:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-rust-examples --test finance_release -P journey --run-ignored=all -E 'test(=finance_release_policy_versions_remain_attributable)'"
```

### Python story

Existing quickstart entry tests; update their actual-program/reader source:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_docsite_quickstart.py::test_docsite_quickstart_verifies_and_repeats tests/integration/test_docsite_quickstart.py::test_docsite_quickstart_refuses_insufficient_authority tests/integration/test_docsite_quickstart.py::test_docsite_quickstart_refuses_changed_content_and_broken_references"
```

### TypeScript story

Existing exact titles; preserve their public example and refusal coverage:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/declare-and-pull.test.ts -t '^(runs the Quickstart script from a directory holding only its Cards|declares a ready verification state)$'"
```

### Homepage animation proof

Planned observable homepage-render regression in the existing SSR test owner;
confirm its collected title before RED:

```bash
mise exec -- pnpm --dir docs exec vitest run src/routes/shell-links.test.ts -t 'homepage shows embedded finance verification$'
```

The displayed API source requires the actual-program journey proof for the
surface it presents. Reuse the existing finance and FastAPI integration test
owners, confirm the focused selector from source before RED, and record it in
execution evidence; do not substitute a snapshot of handwritten code.

Use the existing docs preview and browser tools to observe desktop and narrow
viewports, keyboard/pause, reduced-motion/static rendering, default pass and
hold states, verdict-before-response and delayed asynchronous capture, plus
hidden-tab/offscreen suspension. Confirm Gateway model-call scope, code spacing,
line numbers, syntax colors, no clipping/page overflow and unchanged hero.
For the lower strip, observe all four desktop panels and dashed separators,
ordered agent steps feeding queued evaluation, concurrent reviewer branches
converging, and session rows accumulating before the Verifier runs. Confirm
readable tablet/phone reflow and that both sections pause appropriately.
Record viewport/actions/results and changed asset cost. Illustrative timing
must not become a backend speed claim. Task 4 owns final shared HTML/discovery
closure; this task owns the animation's initial HTML and text alternative.

### Editorial, snippet and reader proof

No manufactured RED for prose/frontmatter. Regenerate with
`mise run docs:generate`, then use the existing focused regression files:

```bash
mise exec -- pnpm --dir docs exec vitest run src/lib/readme.test.ts src/lib/indexes.test.ts src/routes/shell-links.test.ts
mise run docs:check:commands
mise run docs:linkcheck
mise run docs:svelte:check
mise run docs:build
mise run docs:a11y
```

Record the released CLI/SDK pins, gateway/model/tool setup, actual captured
results, responsive/keyboard observations and a fresh-reader walkthrough.
Scripted providers do not substitute for the configured live-provider guide.
README/editorial changes preserve existing semantic behavior; the homepage
animation has new executable presentation behavior and follows Scenario 4. Use the
existing source/diagram generators for any changed derived visual.

Apply `mise run fmt` / `mise run lints`, `mise run py:format` /
`mise run py:lints` / `mise run py:typecheck`, and `mise run ts:format` /
`mise run ts:lints` / `mise run ts:typecheck` only for the languages changed.
Finish with `mise exec -- git diff --check`. Broad SDK/example suites and the
complete docs gate run once on the integrated candidate, not per task loop.

## Material Stop Conditions

Return to spec if the story needs new public/runtime/security/persistence
behavior, a dependency, or cannot use real PDF evidence through the existing
gateway. Keep the guide unpublished if a release/provider prerequisite cannot
be met. Correct private path/test-selector drift locally without weakening
proof. Do not touch backend hot paths; no benchmark exception is granted.

## Authority Links

- [Approved specification, revision 3](../spec.md).
- [Repository rules](../../../../AGENTS.md), [workflow](../../../spec-driven-development.md),
  [execution](../../../implementation-execution.md), and [testing](../../../../TESTING.md).
- [Docs rules](../../../../docs/AGENTS.md), [editorial owner](../../../../docs/PRODUCT.md),
  and [docs contribution rules](../../../../docs/CONTRIBUTING.md).
- [Wyrd authority](../../../../architecture/wyrd-design.md),
  [Bifrost evidence](../../../../architecture/bifrost-design.md), and
  [security boundaries](../../../../architecture/wyrd-security-posture.md).

## Execution Evidence

Candidate: `c86541477`, `da37471cf`, `695c86746`, `0ec297793` on `docs`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002–004; AC-001–003; INV-001–003 | `examples/finance-release/finance_release.{py,rs,ts}`: one Service, one Verifier `release-check` with `recipient-authorized` (assertion), `follows-policy` (Agent via `openai` reading the PDF extraction), `destination-restricted` (assertion); one `verify` per release from `releases.json`; only `passed` releases. Python inputs are Pydantic models. `api.py` is the FastAPI handler the homepage shows | Rust `finance_release_passes_and_fails`, `finance_release_holds_uncertain_releases`; Python `test_docsite_quickstart_verifies_and_repeats`, `test_docsite_quickstart_refuses_insufficient_authority`, `test_docsite_quickstart_refuses_changed_content_and_broken_references`, `test_finance_api_releases_only_passed_reports` (released, held, uncertain held); TS `runs the finance release Quickstart from a reader's directory`, `holds every finance release that does not pass` | PASS |
| REQ-005; AC-004; INV-003 | Changed policy content redeclares `release-check@0.1.1` mounting the new policy; `0.1.0` still mounts the old one; Service `verified_by` names the selected version. Restoring old content assigns `0.1.2`, not `0.1.0` (server versioning), so the test does not assert a restore | Rust `finance_release_policy_versions_remain_attributable` | PASS |
| REQ-006; AC-005; INV-002, INV-004–005 | Setup ends with the model-free setup check (`examples/setup-check`, `### 3. Check it end to end`); Quickstart embeds the finance programs, provider setup and Pydantic extra (`uv add "wyrd-ai[pydantic]==0.4.1"`), step 6 shows `0.1.1` | `setup_check_verifies_and_repeats`; TS `runs the setup check from a directory holding only its Cards`; `test_local_development.py` (guided and manual setup per language); `docs:check:commands` | PASS for tested source. Live-provider and released-pin walkthrough NOT performed: see limits |
| REQ-001, REQ-007; AC-006; INV-004 | README, homepage hero, Introduction lead with "The only real-time verification system built for your applications." and the question; the requirements and release regions and their output come before infrastructure; other languages linked | `readme.test.ts` (region parity), `indexes.test.ts`, `shell-links.test.ts`, `page-text.test.ts`; `docs:linkcheck`; `docs:a11y` (32 pages, AA light and dark) | PASS |
| REQ-007; AC-006; INV-001, INV-004–006 | `FinanceFlow.svelte` (12 numbered lines token-rendered from `api.py` regions `import` and `handler`; three requirement rows; Gateway linked only to the Agent row; async capture to Bifrost; release or hold), `WorkflowStrip.svelte` (four panels), shared `BeatLoop` (server renders the finished beat; pause/play; reduced motion; offscreen and hidden suspension) | `shell-links.test.ts` `homepage shows embedded finance verification`, `holds the report when the policy check fails`; browser observations below | PASS |
| REQ-008; AC-007; INV-001, INV-004–006 | Inventory below; handoff appended to `docsite-incremental-reset/dispositions.md`; `docs/PRODUCT.md`, package descriptions, `generate_llms_txt.py` START/SUMMARY updated; `docs:generate` regenerated `llms.txt`/`llms-full.txt` | `docs:generate` leaves no diff after commit; `docs:build` | PASS |

Commands, run sequentially on the candidate:

| Command | Result |
|---|---|
| `cargo nextest run -p wyrd-rust-examples --test finance_release --test setup_check -P journey --run-ignored=all` (with test Postgres) | 4 passed |
| `pytest -m integration test_docsite_quickstart.py test_asgi_integration.py test_local_development.py` | 22 passed, 3 skipped: `test_cli_bootstrap_saved_config` and `test_cli_gateway_agents` run as subprocesses of the passing installed-CLI tests and skip at top level by design; `test_standalone_examples_use_saved_local_setup` needs `wyrd-example-{py,ts,rs}` beside the repo, not checked out here |
| `vitest run tests/integration/finance-release.test.ts tests/integration/declare-and-pull.test.ts` | 17 passed |
| `pnpm --dir docs exec vitest run src/lib/readme.test.ts src/lib/indexes.test.ts src/routes/shell-links.test.ts` | 18 passed |
| `docs:svelte:check`, `docs:check:commands`, `docs:linkcheck`, `docs:build`, `docs:a11y` | pass |
| `py:format`, `py:lints`, `py:typecheck`, `ts:format:check`, `ts:lints`, `ts:typecheck` | pass (`py:typecheck` needed `PERMITTED: dict[str, Any]`, `0ec297793`) |
| `cargo fmt -p wyrd-rust-examples --check`; `cargo clippy -p wyrd-rust-examples --all-targets -D warnings` | pass |
| humanizer lint on changed pages and README; `git diff --check` | clean |

Selector repairs: the TS title `runs the Quickstart script from a directory
holding only its Cards` is now `runs the setup check from a directory holding
only its Cards`; `examples/tests/quickstart.rs` is `setup_check.rs`
(`setup_check_verifies_and_repeats`); `examples/quickstart` is
`examples/setup-check`. Planned Rust and homepage selectors exist as named.

Browser observations (dev server, Chrome): at 1440 wide the hero is unchanged;
the figure shows 12 numbered lines with conventional blank lines and token
colors, no clipping, the call marker on line 9 (moved to x=470 so it clears
the code), Gateway on the Agent row only, async capture below. At 390 and 800
wide the page has no horizontal overflow; the figure scrolls sideways with its
hint; the strip reflows to one and two columns. Pause freezes both figures,
Play resumes, and scrolling either offscreen stops its beats. No console
errors. Hidden tab (`document.hidden` true plus `visibilitychange`): both
figures froze for 2.5 s and resumed when visible. Reduced motion (fresh
document whose `matchMedia` reports `prefers-reduced-motion: reduce`): both
hydrated without autoplay, showed the finished pass state with Play buttons,
and Play ran one loop. Asset cost: three source
files (`FinanceFlow.svelte`, `WorkflowStrip.svelte`, `beat-loop.svelte.ts`),
inline SVG, no images, fonts or dependencies. Timing is labelled
illustrative.

### Public-surface dispositions

| Surface | Disposition | Canonical destination |
|---|---|---|
| `README.md` | Rewrite | Headline, question, finance regions and output, then infrastructure |
| Homepage `+page.svelte` / `+page.server.ts` | Rewrite | Hero claim; finance figure; Release-only-what-passes tabs; workflow strip |
| `get-started/introduction.md` | Rewrite | Finance example as the landing example |
| `get-started/setup.md` | Targeted alignment | Ends with the model-free setup check |
| `get-started/quickstart.md` | Replace | `examples/finance-release` |
| `build-and-verify/declare/author-in-code.md` | Retain | Embeds `finance-release/service/*` and the `run-config` region, both unchanged and building |
| `connect/integrate-asgi.md`, `verify/verifier-recipes.md`, `understand/cards-versions-and-relationships.md`, `understand/how-verification-works.md` | Targeted alignment | Model-free links point at the setup check |
| `examples/README.md`, `examples/integrate-fastapi/app.py` | Targeted alignment | setup-check and finance-release rows |
| `docs/PRODUCT.md` | Rewrite (purpose, positioning) | Editorial owner |
| Python `pyproject.toml`, Rust SDK `Cargo.toml`, TS `package.json` and `README.md` | Targeted alignment | One description pattern |
| `docs/public/llms.txt`, `llms-full.txt` | Generated | `generate_llms_txt.py` |
| `crates/wyrd-spec/src/setup-prompt.md` | Targeted alignment | Points at the setup check |
| Architecture, contributor rules, history | Retain | Not marketing copy |

Limits and open items:

- **Release pin gap.** Setup and Quickstart pin `0.4.1`, which predates code
  authoring, so the finance Quickstart does not run on the published release.
  Following the older packet's precedent, the page stays published on this
  branch and merge waits for the release that ships code authoring; then
  repin and walk it through.
- **Live-provider walkthrough not performed.** Journeys use a scripted local
  provider; no fresh reader followed the rendered guide with a real `openai`
  deployment. Required before merge (AC-005).
- No runtime, SDK contract, migration or hot-path change. Task 2 extends the
  inventory for education pages; Task 4 owns shared metadata and discovery.
