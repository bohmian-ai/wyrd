# Fresh TASK-001 behavior review

**Result: FAIL** — one material sequential-guide defect is proposed below.

## Subject, authority and method

Repository `/Users/stevenforrester/Documents/GitHub/wyrd`; base
`443a79337fe33dfbfe9ccc64e01d10369d469131`; candidate
`1b72ec1774f68a8eefc6f4f25304dfc5aee1248f`.
This is the user's requested fresh full-range acceptance review. No prior
review acceptance or finding conclusions were carried forward or used for
discovery. HEAD was checked after inspection and remains the candidate.

Governing inputs: approved SPEC-positioning revision 5, original TASK-001,
current user direction in `review-scope.md`, root and applicable docs/Python/TS
AGENTS, docs PRODUCT/CONTRIBUTING, maintainer style, workflow, and the Verifier,
Agents/tools/workspaces and in-memory authoring sections of wyrd-design plus
Bifrost durability/visibility. Later-task REQ-009–016 are outside TASK-001 except
where an existing TASK-001 obligation independently requires them. Original
task revision-4 metadata does not override approved revision 5's explicit SDK
authorization. Earlier prescriptions to remove those APIs or add error-handling
scaffolding are not acceptance authority for the current request.

The full Git range and source consumers were inspected. Navigation expanded
from the shared map into LangTabs/LangTab/Tabs/Tab, source highlighting,
Observe/Target projection, result construction, real verification aggregation,
Artifact workspace selection, actual language programs and finance journeys.
No CodeGraph index exists. No implementation/source edits were made.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: agreed opening and Declare/Observe/Verify before infrastructure | README opening; homepage hero, flow and quickstart sections; PRODUCT and Introduction | readme/indexes/shell-links focused source/render checks reported passing by orchestrator | PASS |
| REQ-002: existing finance story, PDF and three first-class languages; synthetic purpose/recipient/destination | finance_service and finance_release in Python/Rust/TS, existing example owner and restored policy PDFs | Rust actual-binary journeys; Python runpy reader; TS actual-process reader | PASS |
| REQ-003 / AC-003: one Service, one Verifier, three independent requirements, one call per release; real gateway policy read | all three declarations and YAML use two assertions plus follows-policy Agent, exact extracted PDF mount, purpose/report/recipient bindings and fs.read grant | Rust graph inspection and provider read assertions; Python/TS read-bearing provider requests; Python YAML judgment parity | PASS |
| REQ-004 / AC-001: visible outcomes/reasons and application-owned decision | typed verify_release/verifyRelease functions and non-pass reason loops; api.py releases only when result.passed and otherwise holds | Rust independent assertion checks; Python/TS independent outcomes; Python API passed/failed/inconclusive requests | PASS under current user instruction to keep observe functions and output simple |
| AC-002 / INV-003: preserve missing/uncertain/error distinctions; no release from non-pass | unchanged server semantics; Rust missing destination yields inconclusive; API branches only on passed; native errors propagate before release_report | Rust uncertainty/missing case; Python permission/refusal tests and API inconclusive case; TS uncertainty case; existing shared typed-error tests inspected | PASS for preserved behavior; no claim of new API error-recovery machinery |
| REQ-005 / AC-004: reused extraction, exact new policy/Verifier/Service identities, old execution attribution | Artifact extract/declaration path; fixed stored extracted file; public Execution.details and query execution_id | Rust policy update actual program changes regulator outcome and inspects exact old/new refs and old execution details; Python/TS unchanged reruns | PASS for implementation; language-specific update proof limitation noted below |
| REQ-006 / AC-005: sequential reader can reproduce displayed original and changed-policy output | Quickstart steps 2–5 embed actual programs and output, but step 3 leaves restricted=false when step 5 promises a pass | existing revision journey never applies the documented step-3 source edit | FAIL — BEH-001 |
| REQ-006: provider setup, grants, exposure and reader-native pinned commands | Quickstart gateway deployment/credential setup, synthetic-data warning, fs.read grants, pinned v0.4.2 file downloads and SDK packages; Setup model-free check | actual-program journeys use local gateway/provider; docs command/link/build and language type/lint checks reported passing | PASS within rev5's release-availability boundary |
| REQ-007 / AC-006: setup/quickstart/learning paths; responsive accessible six-section entry | homepage uses derived nav/current published foundation; fixed CTAs; existing LangTabs and generic Tabs; FinanceFlow and WorkflowStrip | shell SSR checks; recorded browser desktop/narrow, hidden/offscreen and reduced-motion observations; fresh docs build/a11y reported passing | PASS; browser observations are historical, not independently repeated here |
| Locked finance animation: source-backed API, three checks, Gateway model only, response before async capture | +page.server.ts regions source api.py; FinanceFlow source tokens, pass/hold state, named controls and text alternative | API real-server journey; shell rendering both pass and hold; recorded browser observation | PASS |
| Locked four-panel strip and local lifecycle | WorkflowStrip distinct real-time, queued multi-step, review fanout and session accumulation; BeatLoop finished SSR state, effect cleanup, pause, visibility and reduced-motion startup | source inspection plus recorded browser observations; fresh Svelte check and a11y reported passing | PASS |
| REQ-008 / AC-007: inventory, owner handoff, coupled corpus, generated content | task surface dispositions; older packet handoff; PRODUCT, package copy, setup-check rename, Author in code references, generate_llms_txt owner | source ownership and reference inspection; fresh docs parity/generator checks managed by orchestrator | PASS except generated propagation of BEH-001 correction |
| INV-001: no runtime/durable/security boundary changes; only rev5-approved SDK additions | InputSlot schema projection, shared Target::project, Python wrappers/stubs, TS schema-source inputs and result predicates; no server engine or SQL/migration changes | shared model/result regression tests and type tests inspected; narrow language checks reported passing | PASS; maintainer/standards specialists separately assess owner and public parity |
| INV-002: supplied facts, synthetic report/purpose, provider exposure, no autonomous transmission | declarations and docs describe supplied recipient/destination records, synthetic data and policy provider disclosure; only API simulation returns decision JSON | source trace and finance provider assertions | PASS |
| INV-004–006: foundation, no unsupported deployment/discovery platform or invented performance | existing routes/components/toolchain; animation explicitly illustrative; later-task work remains scoped as later-task packet changes | docs structural/a11y/build checks and source inspection | PASS |

## Proposed finding

### BEH-001 — Restore the input facts before the documented policy-update rerun

- Classification: **INCORRECT**.
- Violated obligation: REQ-006 / AC-005 requires a complete runnable onboarding
  workflow whose actual programs reproduce the brief update follow-on. TASK-001
  Scenario 3 requires the expected changed-policy result; docs AGENTS requires
  an executable guide with observable success.
- Exact location: `docs/src/content/docs/get-started/quickstart.md:117`,
  followed by `:143–157`.
- Source-proven path: step 3 tells the reader to edit the program and set
  `destination.restricted` to false after trying recipient authorization.
  Step 4 only performs reads. Step 5 only downloads a revised PDF and reruns
  that edited program. No instruction restores the supplied destination fact.
  The Python/TS/Rust programs each reuse that destination for every release.
  All three declarations require `inputs.destination.restricted == true`.
  `operators.rs:97–101` compares false to true, engine.rs:1258–1264 maps the
  failed comparison to Failed, and engine.rs:1363–1385 makes one Failed under
  default All force the overall verdict Failed. A new policy can make the
  Agent pass regulator requests but cannot make this assertion pass.
- Observable consequence: a reader executing the numbered guide ends step 5
  with the normal external-audit and regulator requests still failed, rather
  than the displayed passed output. They cannot confirm the promised policy
  change from the reported overall verdict and see an extra destination reason.
- Materiality: this affects the ordinary sequential supported quickstart, not
  an unusual malformed input or hypothetical hardening case. It contradicts
  the defining success observation of a required task workflow.
- Shared cause and affected consumers: the source guide owns the instruction;
  all three language programs consume the modified supplied fact. Full agent
  text is generated from that guide. The actual programs and Verifier checks
  are correct and must not be changed to make this failure pass. Rust's revision
  test uses the unmodified binary and Python/TS reruns use original facts, so
  their existing green results do not exercise this sequential document state.
- Smallest testable correction: explicitly restore `authorized` and
  `restricted` to true after the negative-input exercise, before the policy
  update rerun; regenerate the coupled agent text with the existing generator.
  One local guide instruction closes the cause for all languages. No guard,
  helper, reset command, new runtime behavior or test harness is justified.
- Focused closure proof: read the corrected numbered steps to establish restored
  facts, and use the existing finance policy-revision journey's original and
  revised outcomes to confirm that the state now matches the described output.
  Preserve the independent negative-case proof and source-backed snippets.

## Verification, boundaries and limits

This reviewer performed static source/caller/authority validation, not runtime
execution. The orchestrator reports fresh passes for the three exact Rust
finance selectors on repository Postgres, focused docs 21 tests, docs build,
a11y over 32 pages, links/commands, Svelte with zero errors/warnings, and Python
and TS lint/type checks. Python five finance journeys and full docs tests were
running at the last update. The orchestrator's verification report must supply
the final exact commands/results; these messages are not substituted for it.

Python and TS finance journeys do not themselves rerun a substantively revised
policy and inspect its new identities; Python tests explicit-version mutation
refusal and both languages prove ordinary rerun behavior. Rust proves the new
policy selection and old execution path against the shared declaration/server
owners. This is a verification limit, not a claimed source-proven SDK failure
or a demand to redesign tests in this finding.

No added or moved PostgreSQL, Scribe, object-store or Vault wait was found in
changed backend hot-path owners: those owners are unchanged. Input projection
and predicates are client-local operations, Artifact extraction is declaration
preparation, and the motion owner is browser-local. No benchmark exemption is
claimed for future runtime changes.

Not proposed: repeating superseded SDK-scope findings, adding error scaffolding
the current user explicitly removed, competitor research, Task-002–004 corpus
completion, release/tag publication as a task gate, optional UI refactors, or
unrelated pre-existing documentation debt. No new abstraction was needed to
diagnose or correct BEH-001.

Skipped: fresh browser observation, live-model quality measurement, release
publication, and independently rerunning the orchestrator's test commands.
Risk: the sequential guide currently cannot reproduce its policy-update output.
