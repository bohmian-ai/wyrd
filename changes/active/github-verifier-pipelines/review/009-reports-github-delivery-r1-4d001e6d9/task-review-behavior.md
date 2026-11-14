# TASK-009 behavior acceptance

Result: FAIL

Subject: complete d07c55158...4d001e6d9ed3daf88bf81995510319373bd8c353 range, approved spec revision 10, original tasks/009-reports-and-github-delivery.md. HEAD was checked at the candidate. Working-tree task index and task 011 were excluded. No source was changed and no Cargo command was run.

## Authority and discovery

Read the task and its obligations/evidence, spec REQ-005/012/013/014/016 and invariants/acceptance, contracts Standard code review report and Errors/inspection/delivery, root and affected nested AGENTS, spec-driven-development workflow, maintainer guide, Rust/errors/agent-surfaces guides, architecture Prompt and queued PR delivery, Bifrost canonical execution_events, and GitHub association security authority. Architecture explicitly selects per-accepted-revision outcomes; that narrower owner resolves the specification's shorter “one outcome per execution” wording for this review.

CodeGraph was used before code-location searches. Independently expanded navigation beyond the seed:

- Provider admission: github/pull_requests.rs::targets, pull_request_bindings and normal Verifier validation. PR activation selects bindings and authorization; it does not require a source-extraction step.
- Output production: vala-verify blocks/engine.rs::reported, own, aggregate and Attempted::settle; verification/results.rs and runner.rs::result; engines.rs::pr_source_extracted. Successful final outputs, rather than invocation history, feed report selection. Failed fanout has no published partial aggregate, so it supplies no retained successful aggregate output.
- Result consumer: service/inspection.rs::presented reads accepted state and selected OutcomeDetails; delivery.rs selects report and extraction; presentation.rs produces Check/review; delivery Write::send and app.rs make provider IO.
- Sibling authority consumer: engines.rs::pr_source_extracted recognizes the server-assigned Extract provenance; delivery's extraction lookup does not.
- Delivery/read lifecycle: runner settlement/park trigger, server startup recovery, SQL current-generation enumeration, allowed access/generation checks, Scribe fenced event, InputResolver checkpoint/event reads, authorized HTTP execution inspection and MCP descriptor/dispatch.
- Connected proof: actual report/extraction/admission integration tests, CLI evidence/approval journey additions, public YAML example consumed by its integration test, Prompt builders and SDK authoring tests, and docs.

The new renderer has a coherent pure transformation role; the GitHub component owns external delivery, while existing queue, accepted-state, Scribe and query owners retain their responsibilities. No replacement engine, new database payload store, helper library or new report selector surface was found. The extraction PointerConflict retry stays at the acquisition seam and also classifies a second conflict as retryable; the task records diagnosis and approval. Its approval provenance is a workflow evidence limit, not independently reconstructed here.

## Acceptance matrix

Verification below means inspected assertions and recorded execution evidence, not a fresh runtime run. Paths without a prefix are under crates/wyrd/wyrd-server/src unless stated otherwise.

| Obligation, including cases omitted by the implementation ledger | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REPORT: closed exact report/Finding/anchor shape, required nullable members and unique IDs | wyrd-spec verification/report.rs types, present, validate | report tests reject missing/extra members, unsupported version, duplicate/empty ID, invalid path/commit/range | PASS |
| Named code_review beside verdict, no custom data, ordinary code envelope | report.rs::resolve_response_format; PromptSpec resolver; skald response_format; Python and TS authoring | builder asserts native summary/findings schema; SDK tests serialize named format; integration uses Python code outputs | PASS |
| Zero/one/multiple envelope selection; consumed earlier report still counts | select_report counts succeeded final outputs, detects ambiguity | pg_github_delivery each_execution_selects... asserts no report summary, draft+final ambiguity and reason | PASS |
| Fanout items, final repeat only, failed attempts/discarded speculation excluded | select_report plus engine reported/aggregate/final invocation semantics | unit selection assertions and real foreach/repeat/speculation integration assertions, including final summary “Final pass”/“Kept” | PASS |
| Selection/shape errors visible with no mandatory source-preparation stage | delivery.rs::render gates Completed on extraction presence | All integration verifier helpers unconditionally prepend an extraction; absent-extraction case is untested and fails by source | FAIL: BEH-009-1 |
| Report severity, ambiguity and malformed shape never replace verdict | select_report/presentation does not mutate accepted outcome; conclusion from Settled | selection journey asserts success/passed even for malformed/ambiguous; incomplete report authored check asserts failure | PASS |
| Report remains normal output, separate from Verdict.data budget; no silently clean truncation | normal OutcomeDetails step output; bounded visibly labels truncated summary | long_summaries_are_cut_visibly asserts length and truncation suffix | PASS |
| RENDER: exact source authority comes from task 005 extraction, not ordinary output resemblance | delivery.rs::render first deserializable output lookup | Existing extraction integration proves real source, but no decoy ordinary-output test; source demonstrates bypass | FAIL: BEH-009-2 |
| Every anchor validated against exact retained commit/source, including evidence anchors | render_findings calls place on locations, evidence and edit | renderer invalid-path/source/range tests cover only head source; retained-before evidence incorrectly refused | FAIL: BEH-009-3 |
| Valid head suggestions replace inclusive declared range; invalid source safely refused, never moved to line 1 | place/comment; no source mutation API | integration asserts exact path, line, RIGHT side and suggestion body; renderer checks missing/binary/range cases and unchanged conclusion | PASS for supported head cases |
| Unlocated findings stay summary-only; ID/severity/confidence shown | render_findings and heading | integration asserts all severities and unlocated content; unit verifies summary outside diff | PASS |
| One COMMENT review for a rendered report, exact commit/marker; summary-only/empty reports included | Presentation::render uses comments.is_empty as presence condition | integration explicitly asserts no review when reports have no inline finding; violates required behavior | FAIL: BEH-009-4 |
| Check stable binding name, external execution ID, reviewed head; verdict/status conclusion mapping | delivery render name; presentation Settled::conclusion | integration asserts name/head/external/conclusion; unit checks terminal mappings | PASS |
| Waiting evidence/approval explanation, no completed-to-in_progress transition | presented builds waiting; Write::Check completed guard | real approval/speculation journey asserts waiting text and same Check ID on completion; monotonic guard inspected | PASS |
| No annotations/APPROVE/REQUEST_CHANGES/auto-resolve/source modification | closed CheckOutput and Review event COMMENT; only Check/review endpoints written | integration absence-of-annotations and COMMENT assertions; source API enumeration | PASS |
| DELIVERY: after settlement/park, background only, retained outputs, no recomputation/verdict wait | runner::run spawn after settle; presented reads OutcomeDetails; recovery reads same | restart journey compares source-read count and POST count; immutable outcome read | PASS |
| Lookup before Check/review creation, also after lost answer, update Check/skip review | Write::send, app pages/create/update | lost-answer journey asserts one Check/review, one Check POST and PATCH, two review attempts including rate refusal | PASS |
| Respect full provider cooldown; bounded retry; refusal visible separately | app::answer parses Retry-After; attempt caps duration | 2-second cooldown asserted; a longer supported cooldown is shortened by source | FAIL: BEH-009-5 |
| Permanent refusal gives failed outcome without verdict change | Halt conversion and deliver record | refusal journey asserts github_refused, passed accepted verdict, one POST and empty provider state | PASS |
| Current access/generation before provider work; supersession/removal stops later attempts | allowed installation token/access and current-generation check | real new-head and access-removal journeys stall/fail provider request and assert skipped plus absence of old later writes | PASS for exercised transition boundary; lookup-to-write interleavings remain a sensitive-domain review concern |
| Canonical outcome fenced per accepted revision, no SQL payload mirror | delivery record/key and recorded committed-batch visibility; inputs event filtering | key identity test; inspection equality; restart without recorded outcome reuses remote state | PASS |
| Restart current PR executions without outcome; unavailable outcome never treated as absence | SQL enumeration/recover and recorded fence/error branch | restart journey proves landed review is found and no source reacquisition; unavailability branches statically inspected | PASS for inspected recovery scope |
| SURFACES: HTTP/MCP delivery outcome safe reason separate from verdict under existing read authorization | service get/retained authorization then recorded; MCP same execution operation | integration asserts HTTP github_delivery equals MCP structuredContent and accepted verdict unaffected | PASS |
| SDK report authoring and generated wire parity; no bespoke SDK/CLI delivery operations | Python static factory/stub, TS format branch/type, Rust factory; generated card-types | source parity and recorded SDK authoring/codegen checks; no new projection method | PASS |
| Public runnable standard-report story, optional convention, severity vs verdict, waiting and retry diagnosis | example YAML; guide; architecture changes | public example test loads published files and asserts authored high finding fails and exact fix appears inline | PASS, subject to renderer findings above |
| AC-011 App connection→declare→admission→evidence→approval→result/delivery via public flows | CLI github_journey Waiting setup and approval scenario, links to existing guides | added assertions read MCP delivered and exact head/execution success Check after existing connection/evidence/approval flow; task records approved language-neutral journey deviation | PASS for recorded approved proof composition |
| No new SQL table/column, report mirror, finalizer, mandatory helper, annotations or automatic merge decisions | complete diff and reachable IO owners | static scope inspection | PASS |

## Findings

### BEH-009-1 — INCORRECT: missing extraction silently suppresses report and errors

Obligation: REQ-012 and architecture queued delivery say one envelope renders and malformed/multiple envelopes are visible errors; no mandatory preparation stage. Task REPORT requires normal code reports and unlocated findings.

Location: components/github/delivery.rs:392–408 and presentation.rs:185–191.

Reachable scenario: bind a normal Verifier containing an ordinary code step producing a valid summary/unlocated report, without an extract action. PR admission targets bindings under ordinary authorization and imposes no extract requirement. The engine accepts the code output and authored requirements determine its verdict. select_report finds One, but no output deserializes as PullRequestExtraction. delivery passes completed=None, so Presentation::report never runs. The Check has an empty summary and github_delivery is recorded delivered with no reason. The same path silently hides malformed and ambiguous report errors. An extraction nested in a successful foreach output is also not found by this top-level lookup.

Consequence/materiality: an explicitly supported optional report is lost, and invalid reports look like successful content delivery. This is not merely missing inline placement: summaries and schema/ambiguity errors need no source anchor at all.

Smallest correction: keep report selection/rendering independent from availability of retained extraction. Use retained source only for validating supplied anchors; render unlocated content normally, and visibly refuse anchored content when authoritative retained source is unavailable. Do not add a mandatory extractor or re-fetch source during delivery. This is a local renderer-input change, with no new infrastructure.

Focused proof: extend the existing connected code-output journey with no extract step, asserting unlocated report text; malformed/multiple envelope reasons; unchanged authored verdict; and visible source error rather than lost report for a located finding without retained source.

### BEH-009-2 — INCORRECT: ordinary outputs can substitute for canonical extracted source

Obligation: REQ-013/task RENDER requires validation against task 005 retained canonical path/commit/range/source, not authored/model-invented source. INV-005 keeps exact source pins.

Location: components/github/delivery.rs:392–395.

Reachable scenario: a preceding ordinary code or custom-schema Agent step returns a JSON object shaped as PullRequestExtraction, with the actual head SHA but fabricated file text/hunks. A subsequent real pull_request extract step and report step succeed. OutcomeDetails preserves plan order. The first deserializable output wins, regardless of action provenance, status or matching accepted activation identity. place then validates the report against fabricated after text/hunks and may send an out-of-source suggestion to GitHub. Conversely, a stale copied extraction can cause valid findings to be suppressed. A code action is allowed to emit arbitrary JSON and schema validation does not certify extraction provenance.

Producer/consumer comparison: the real source producer is GithubConnections::extract_pull_request. The sibling engines.rs::pr_source_extracted already distinguishes its server-assigned CallProvenance::Extract with PULL_REQUEST_EXTRACT_VERSION from other output; delivery ignores that distinction. report selection is a separate envelope convention and must remain available to ordinary code.

Consequence/materiality: a trusted rendering boundary certifies line ranges using invented content, defeating the explicit exact-source guarantee. Retaining ordinary JSON does not make it canonical task 005 source.

Smallest correction: select source only from the accepted successful server pull_request extraction producer, using its existing provenance and accepted activation pins. Preserve ordinary report authorship and retained-only delivery; do not introduce another source acquisition or identity store. Reject unavailable/mismatched canonical source visibly for affected anchors.

Focused proof: insert a plausible ordinary extraction-shaped decoy before a real extraction in the existing journey; make its head path range/hunks differ. Assert the real retained source controls validation and no invalid range becomes an inline suggestion. Also exercise decoy-only output without a real extractor.

### BEH-009-3 — INCORRECT: valid retained-before evidence suppresses a head finding

Obligation: contracts Standard code review report permits anchors at an exact retained commit and inclusive source range; task RENDER explicitly validates every supplied anchor against retained source/diff and unchanged context. evidence anchors are ordinary SourceAnchor values, not head-only locations.

Location: components/github/presentation.rs:285–299 and :339–350.

Reachable scenario: a valid head-side finding points to the changed head line and supports its explanation with an evidence anchor to that file's previous text at review.full_pr.base (the merge base). Extraction retains that before text and exact base commit. Finding::anchors chains evidence after locations; place rejects the evidence solely because its commit differs from pr.head.sha. render_findings marks the entire finding anchor_commit_mismatch and drops its explanation/inline suggestion. The analogous lookup ignores retained unchanged supporting context entirely because it searches only full_pr changed-file after text.

Consequence/materiality: ordinary before/after reasoning in a source review is falsely labeled invalid, and a valid actionable head finding disappears from inline delivery. The public shape includes commit precisely to identify which retained source is referenced; restricting all evidence to changed head files silently narrows it.

Smallest correction: distinguish retained-source validation from GitHub head-side placement. Validate evidence against the matching retained before/after source and commit (and retained context where provided), then decide whether the actual finding/edit can be inline. Keep invalid/unavailable source errors and do not fabricate left-side placement or fetch source. The existing retained extraction already carries both sides; no additional store is justified.

Focused proof: renderer test with a valid head location/edit and valid base-commit evidence must retain the head inline finding; wrong base commit/out-of-range before evidence must still give a safe visible problem. Cover a supplied unchanged-context anchor if that source path is used by the renderer correction.

### BEH-009-4 — MISSING: rendered summary-only reports never create their review

Obligation: architecture queued delivery says “A rendered report adds one COMMENT review” on the reviewed commit carrying the execution marker; REQ-013/task RENDER and docs state that review is delivered alongside the Check. No-report behavior can remain Check-only.

Location: components/github/presentation.rs:192 and delivery.rs::send.

Reachable scenario: a valid complete report has zero findings, or findings only with empty locations/outside-diff locations. Its report is rendered into the Check summary, but comments is empty, so review=None and send skips review lookup/create. The existing pg_github_delivery selection journey explicitly asserts no reviews because no report has an inline finding; the incomplete-report journey likewise asserts the empty report adds none.

Consequence/materiality: supported report shapes omit the promised pinned review/marker entirely, while delivery says delivered. This is an explicit observable contract omission, not a preferred presentation change.

Smallest correction: construct the one COMMENT review when a valid report is rendered, including when its inline comments vector is empty. Retain no review for missing/ambiguous/malformed reports and waiting states; use the existing marker lookup/retry mechanism. No new endpoint or payload store is needed.

Focused proof: update connected empty and unlocated report assertions to require one pinned COMMENT per rendered report, zero inline comments where appropriate, marker present, and retry/restart preserving the same found review.

### BEH-009-5 — INCORRECT: long provider cooldown is shortened

Obligation: REQ-014/task DELIVERY and architecture queued delivery require honoring provider Retry-After while bounding retry attempts.

Location: components/github/delivery.rs:471–472; COOLDOWN_MAX is 15 minutes. app.rs::answer accepts an arbitrary numeric Retry-After duration.

Reachable scenario: GitHub returns 429 or recognized rate-limited 403 with Retry-After: 3600 during Check/review lookup or write. answer returns Cooldown(3600s), but attempt sleeps 900s and issues its next request, 2700 seconds before the provider's requested cooldown expires.

Consequence/materiality: delivery retries while still rate limited, exhausts its bounded attempts prematurely and can amplify a provider refusal. The existing test proves only a 2-second Retry-After and cannot establish the promised behavior.

Smallest correction: never issue another attempt before the provider cooldown. If a bounded delivery cannot wait a supplied duration, terminate visibly without sending early; do not silently shorten it or add a progress ledger. Preserve bounded attempts and access/generation rechecks.

Focused proof: deterministic paused-time or classification/attempt test with Retry-After greater than 15 minutes; assert no provider request occurs before its allowed time. Reuse existing mock/clock mechanisms, without a real long wait.

## Verification limits

The task records focused schema/renderer/SDK checks, 29 PG delivery/extraction cases, five CLI journey cases and format/lint/docs/codegen checks. Their relevant assertions were inspected; this report does not claim to have rerun them. Fresh orchestrated verification also reports report unit 4, renderer/delivery unit 7, PG delivery/extraction 29, CLI GitHub 5, format, codegen, Python typecheck, tenant isolation and lints passing; exact commands and environment limits are in verification.md. No live provider request, broad gate, Cargo command or source edit was performed in this audit. Findings follow deterministic production branches and require focused connected regression proof after correction. No optional refactor or new dependency is requested.
