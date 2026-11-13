# Landing animation concepts

Status: finalized

## Locked task addition

The maintainer approved the final animation and asked to add it to the tasks.
[Task 1](../tasks/TASK-001-positioning-and-finance-onboarding.md) owns production
integration of the final left-to-right composition (direction B), including the
below-hero full-content-width shallow layout, unchanged hero/background,
primary real-time verification, secondary amber Gateway and lime Bifrost,
explicit Python API/Wyrd integration and conventional blank lines/syntax colors.
Other direction selectors remain review artifacts. The API decides after the
verdict; Bifrost capture never delays its response.

The task includes source-backed API code and render/browser/accessibility proof.
The mockup's handwritten sketch is not production source. This is a homepage
presentation refinement within approved revision 3, not new runtime behavior.
No numerical speed, throughput, compression or cost claim is locked. The design
exploration below records prior alternatives; this locked addition supersedes
its selection questions and proposal-only status statements.

An independent comprehension pass accepted the bounded Task 1 amendment: the
composition and semantics are clear, existing owners remain authoritative, and
SSR, tested-example and browser proof cover complementary obligations. Task
shape and whitespace checks passed; production implementation has not begun.

## Current direction after mockup review

The maintainer rejected the initial busy alternatives and subsequent placement
on the hero. Preserve the existing hero background unobstructed; place the
animation in its own section below the cover. The animation still teaches
real-time verification of a finance report request: recipient/destination
assertions and a policy-PDF Agent judgment inform the API’s release/hold
response. Default to a successful, fast-moving request, with the violation
case optional rather than repeatedly presenting Wyrd as a hold gate.

The suggested request → API → response / Verify branch sketch is inspiration,
not a fixed node layout. The maintainer explicitly gives the designer room to
choose the clearest composition. A supplied Restate screenshot establishes the
visual direction: sparse technical outlines, compact labels/code, a few fast
moving signals and open space. No dashboard, inspector or dense record table.
Use a short illustrative cycle, not long pending/progress states. Preserve
real verdict-before-response semantics without claiming zero overhead or
unmeasured backend speed.

References supplied by the maintainer:
[Pydantic Agent Sandboxes](https://pydantic.dev/logfire/agent-sandboxes) and
[Restate](https://restate.dev/), plus the specific
[Restate illustration screenshot](mockups/restate-reference.png). The sites
and supplied screenshot were inspected visually. Keep Wyrd’s own colors,
fonts and existing backdrop; do not copy competitor branding or invent a
comparison of performance/capabilities.

The current review artifact is [the simple landing animation](mockups/simple/index.html).
Its background is a mechanically transpiled copy of the actual CoverScene
source and remains separate from live docs. The earlier concepts below retain
rejected rationale only. The current composition is still under review;
no production animation or new specification requirement has been approved.

## Problem and intended effect

The maintainer wants the landing page to make the impact of real-time
verification immediately visible, with compression/cost savings as a possible
supporting story. The audience is a developer deciding whether to investigate
Wyrd. The successful animation explains what an application learns before it
acts, then leads to the actual runnable example.

These are style-direction/storyboard proposals, not implemented or measured
product demonstrations. The approved positioning, spec revision 3 and current
tasks remain unchanged. No option has been selected.

## Options

### 1. Before the report leaves — recommended hero

Scene: a customer finance report is ready to send. A visible bank-account
identifier sits inside otherwise plausible content. Before the application's
send action, three named requirements appear: recipient authorized, destination
private, sharing policy satisfied. The first two pass; the gateway policy Agent
finds the prohibited identifier using the PDF policy, and policy fails.

Payoff: Wyrd returns failed with the cited evidence; the application changes its
pending action to Hold for review. A second safe report passes all three and
the application releases it. The result connects visibly to the source report
and policy version. Do not depict Wyrd taking over the send operation.

Claim: the application can use verification while deciding whether to proceed.
This is the closest match to the primary quickstart and has the smallest
content/proof gap. A short authored sequence can emphasize the decision without
showing fictitious live timing. Replay and a safe/failing case control could
make it explorable without creating a provider-connected demo.

### 2. The answer looks right. The work is incomplete.

Scene: an agent produces a polished final report. Expand its execution record:
preparation ran, an explicitly recorded required approval was skipped, and a
final answer was produced. Wyrd checks the declared requirement against the
actual workflow evidence and returns failed even though the answer looks good.

Payoff: the skipped requirement and supporting record become the focal point;
the application routes the result for review. A complete execution passes.

Claim: verification covers actions and workflow obligations beyond plausible
output. Requires the actual skipped-step tutorial evidence. A missing/late span
alone must never be animated as proof that an action was skipped. Strong
secondary candidate if differentiating agent verification is the main aim.

### 3. Every decision has a receipt

Scene: a small stream of application requests resolves into passed, failed and
inconclusive outcomes. Follow one request to reveal its exact subject/Verifier
versions, three requirement results and original evidence. The selected failed
case shows the application's hold decision. Evidence details appear as a later
inspection, not a synchronous durable-publication guarantee.

Payoff: the visitor can follow a judgment back to what was checked and why.

Claim: real-time decisions remain attributable and investigable. Broader than
the finance case, but more visually dense and less immediate about the business
consequence. Use a few legible requests rather than an invented throughput
counter. No particle wall or random numbers posing as measured traffic.

### 4. Keep the evidence. Shrink the footprint.

Scene: a labeled dataset of reports, observations and verification evidence
flows into Bifrost. A representation of its stored data becomes smaller while
the evidence count/identity remains visible. Selecting a prior judgment still
retrieves its original evidence. Pair the byte comparison with an inspectable
measurement description once one exists.

Payoff: the cost of retaining useful evidence is made tangible through its
storage footprint, rather than suggesting that compression deletes history.

Claim: measured encoding/storage efficiency for a defined retained dataset.
A numerical demonstration needs the actual input/stored byte boundaries,
retained row/value fidelity, format/settings and reproducible measurement.
Total deployment storage includes more than an encoded data file (for example
WAL, staging, metadata and replication), so label any partial comparison.
Dollar estimates additionally need explicit unit rates, retention and workload
assumptions. Storage-byte reduction is not an equal percentage of total cloud,
provider-token or verification cost savings.

No local `benchmarks/history` directory was present in this checkout, and no
publication-ready compression ratio was established by this inspection.
Do not show a ratio or savings number until supported. An illustrative drawing
must be labeled schematic and must not imply a measured ratio through scale.
This works best below the main real-time-verification story as supporting
infrastructure value, rather than competing for the hero's headline.

### 5. One request. Then the whole service.

The maintainer proposed showing a service receiving thousands of requests and
Wyrd keeping up. Start with the finance decision from Option 1 so the viewer
understands what a verification actually does. Then zoom out: a busy request
stream reaches the service, each depicted verification returns its outcome,
and the application continues or holds according to that result. Keep a small
selection legible while the wider stream establishes the workload context.

Claim: verification can be part of a busy application’s request lifecycle.
This is a proposed performance story, not throughput demonstrated by a looping
animation. Prefer playback derived from a reproducible benchmark at a named
configuration. Show measured requests/second, tail latency and workload/check
composition when supported, with a visible measurement link. Separate request
count from rate and client load from completed verifications. A deterministic
assertion workload must not imply that a PDF/model-judge workload has identical
throughput, latency, provider limits or cost. Depict failures/inconclusive as
judgment outcomes distinct from runtime errors, and do not imply unlimited
capacity, zero queuing or zero overhead.

Recommendation: use this as a second beat of the primary real-time story,
after the close-up decision, rather than a standalone stream of anonymous
particles. Without qualified data, retain an explicitly schematic service-flow
illustration without numerical throughput/latency claims. No load generation
or benchmark execution is authorized by this design conversation.

## Recommendation and implementation direction

Use Option 1 for the hero, potentially followed by Option 5’s scale view once
its benchmark evidence exists. Keep Option 4 as a lower-page supporting module
after its storage evidence exists. Choose Option 2 instead if the next public priority is
workflow correctness; avoid cycling all four unrelated messages in a carousel.

Reuse the page's existing SvelteKit host and the current Evidence Thread brand
palette/type roles. Favor readable HTML/SVG and small CSS/Svelte transitions;
no new animation dependency, WebGL scene or live backend is needed to explain
the story. Keep the exact headline, developer question and CTA in static HTML.
Animate transitions between real narrative states, with a static/reduced-motion
rendering, accessible controls, pause/replay for repeating motion and no
meaning conveyed solely by red/green. Adapt the composition for narrow screens
and suspend motion when offscreen. Do not use scene duration as a latency claim.

The current decorative `CoverScene.svelte` can remain subdued behind or beside
the functional illustration if that preserves legibility; replacing the hero
visual itself is a separate choice because the approved spec preserves it.
The exact placement/composition remains open. No existing UI was changed.

## Evidence and standards

- [Approved specification](../spec.md): real-time finance flow, versioned policy,
  native judgments, application ownership, brand and honest publication proof.
- [Current hero](../../../../docs/src/routes/+page.svelte) and
  [decorative canvas](../../../../docs/src/lib/components/CoverScene.svelte).
- [Active brand authority](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/DESIGN.md)
  and [current docs token projection](../../../../docs/src/styles/wyrd-tokens.css).
- The visual skill's older `docs/src/styles/wyrd.css` reference is absent; its
  font/CRT suggestions conflict with the current brand/editorial rules. Follow
  the current Familjen Grotesk/Fragment Mono, cobalt/periwinkle/lime system;
  no CRT/arcade treatment or invented Card kinds. These are concept directions,
  not a canonical new visual asset.
- [Bifrost authority](../../../../architecture/bifrost-design.md) defines storage
  and visibility; architecture alone does not substantiate compression ratios
  or economic savings.

## Open decisions and proof

Select the hero concept and whether the scale beat and compression module
belong in this rollout. Quantified cost/byte claims require a separate bounded measurement
proposal; no experiment is authorized or performed here. If Option 1 is chosen,
reuse actual quickstart result/evidence shapes and verify the illustration
against executed examples; test that a fresh viewer understands who verifies,
who acts and why the result changes. Verify mobile/reduced-motion/static output
and existing docs performance/accessibility using the current tooling.

## Review mockups

Three animated hero alternatives are available in [the local review page](mockups/index.html):
A — one request, then service scale; B — verification in the busy request path;
C — decisions becoming retained evidence. All follow the existing Wyrd palette
and fonts, support light/dark and reduced motion, and remain separate from the
live docsite. Browser checks and limitations are recorded in
[the mockup README](mockups/README.md). The maintainer rejected this first set as too busy and without the existing
background. The simple revision above supersedes these review alternatives;
the approved spec/tasks are unchanged.

Latest review mockup: the animation is now a separate section below the cover,
using a compact finance-handler illustration connected to three requirement
indicators. It repeats the successful case in an illustrative 2.7-second
cycle; policy violation is an optional review case. The original CoverScene
remains unobstructed. Browser checks and captures are recorded in the mockup
README. Composition remains subject to maintainer review.

## Agreed supporting gateway and Bifrost layer

The maintainer accepted the below-hero animation as a good first draft and
asked how to show records, traces and data reaching Bifrost and Agent calls
going through the gateway. Recommendation: preserve the current main
handler/Verifier/result composition, then add two subordinate signals.

- Put a small “via Gateway” connection/label at the Agent requirement only.
  Model-call pulses briefly traverse it and return; assertions do not. This
  means model/provider calls, not that every Agent tool invocation is routed
  through the model gateway.
- Add one quiet Bifrost rail beneath the figure with a thin/dotted connection
  from the application/verification capture path. Label the flow “records ·
  traces · app data” and distinguish retained verification results/evidence
  where useful. A few subordinate pulses continue after the verdict/response.
  This is configured capture/supplied data through existing SDK/server owners,
  not a claim that all application data is collected automatically.

The API response must not visibly wait for Bifrost publication. Keep the
verification return connection distinct from the asynchronous retention rail;
no storage acknowledgement may become a new drawn critical-path step. Do not
add a storage dashboard, inspector, counters or more explanation panels.
The maintainer agreed to this supporting layer. Claude Code (Fable 5.2, medium)
added it to the local mockup, preserving the 2.7-second illustrative pace and
existing hero/background. Desktop/mobile browser review confirmed the model-call
pulses belong to the Agent alone, capture pulses begin after the response, and
the controls and reduced-motion state still work. Updated captures are in
`mockups/simple/previews/`. Final composition remains subject to review; the
live docsite and approved spec/tasks are unchanged.

## Direction variations requested

The maintainer likes the supporting-layer draft and requested variations of
the same content with different flow directions. Explore top-to-bottom,
left-to-right and right-to-left in a separate local review page, keeping the
accepted `simple/` draft intact. Real-time verification is the primary visual
focus; Gateway and Bifrost are secondary features, with Gateway colored rather
than gray. Use periwinkle for verification, lime for retained evidence, and
amber as the third diagram series for the Gateway/model route. Success and
failure retain their own labeled states. Preserve the hero/background, fast
illustrative pace, finance context and asynchronous retention boundary.

All variants must fill the landing page content width below the hero and stay
shallow (wider than tall). The downward variant must use a short vertical
route within a wide composition rather than a tall stack. These are comparison
mockups, not a selected production composition.

The [direction review page](mockups/directions/index.html) now contains all
three animated variants, authored with Claude Code Fable 5.2 at medium effort.
At the 1240px desktop content width, the diagrams are 269–305px tall.
[Browser evidence and limits](mockups/directions/README.md) cover desktop/mobile,
controls, ordering and reduced motion. On phones the shallow diagram scrolls
internally to retain readable labels. No production direction is selected.

## Comprehension feedback: API ownership and embedded Wyrd call

A second viewer could not tell that the code panel represents the application's
own API with Wyrd verification embedded. `run.observe.verify` alone did not
identify Wyrd. An initial label-only refinement did not satisfy the maintainer.
The maintainer requires an explicit `@api.post(...)` decorator and
`from wyrd import WyrdState`, permitting smaller code text to fit. Show the
state initialization and Run context so the verification call visibly belongs
to that import. Keep the real SDK shape, Your Finance API header, all three
directions, shallow fullwidth format and existing motion order. This remains
an integration sketch with application setup supplied outside the figure.

The code sketch is now implemented in all direction variants with nine lines
at a 9.5-unit code size. The import, state initialization, route decorator,
Run context, verification and release/hold response are visible. Highlights
and links align with the verification line. At 1240px width A is now 301px
tall, B/C remain 305px. Browser review and captures are refreshed. This sketch
omits surrounding application definitions; it is not a runnable tutorial.

The maintainer requested better Python formatting and coloring. Refine the
shared snippet using the existing docsite syntax palette: periwinkle keywords,
lime strings, amber functions and muted punctuation, with readable four-space
indentation and modest separation of import, setup and route-handler blocks.
Keep the nine-line integration sketch and synchronize highlights/links with
any changed baselines. This is a presentation-only refinement.

The Python presentation refinement is implemented across all three directions.
Browser checks confirmed rendered syntax colors, indentation, call/progress
spacing, highlight alignment and mobile containment; captures were refreshed.
Desktop/mobile visual inspection confirms the snippet remains within the
existing panel. Application logic and diagram dimensions are unchanged.

The maintainer requires conventional Python blank lines, not merely small
extra spacing: one blank after the import and two before the top-level decorated
function, with the decorator adjacent to `def`. Displayed line numbers must
reflect those gaps. A modest height increase is acceptable to preserve legibility
and the existing wide composition. Keep animation and links aligned to the
new verification and response lines.

Conventional blank lines are implemented across all three variants with twelve
physical line numbers. At desktop content width A is 333px tall and B/C are
331px. Browser checks confirmed gaps, numbering, call/highlight alignment,
code fit and mobile containment; desktop A/B were inspected and captures
refreshed. Code logic and colors are unchanged.

## Locked lower-page workflow addition

The maintainer also approved the [four-panel workflow section](mockups/workflows/index.html#workflows).
Task 1 now owns both the detailed finance animation below the hero and the
wide, shallow workflow strip toward the bottom: real-time verification,
multi-step queued agent evaluation, multi-reviewer fanout/fan-in code review,
and session information accumulating before a Verifier judges the group.
All four share one desktop row with dashed vertical separators and reflow
readably on tablet/phone. Existing mockup/browser evidence is linked from
the task. This locks presentation; no new runtime implementation is authorized.
