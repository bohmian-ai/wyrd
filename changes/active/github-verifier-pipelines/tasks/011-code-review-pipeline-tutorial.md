---
id: TASK-011
status: proposed
approved_spec_revision: 9
requirements: [REQ-016]
acceptance: [AC-011]
invariants: [INV-001, INV-005, INV-006, INV-007]
depends_on: [TASK-009]
---

# Build your own code review pipeline tutorial

A developer follows one docsite guide from connecting GitHub to declaring their
own review policy, authoring its Verifier and seeing its Check and PR comments.
The page explains each action and its observable result, including how to find
and resolve a wait and distinguish a verdict from a delivery outcome.

## Contract sources and scope

Contract: spec revision 9 **REQ-016 / AC-011**; contracts → Repository and
declaration / GitHub connection / Pull request extraction / Standard code review
report / Deferred input publication / Human approval / Errors, inspection and
delivery. The tutorial explains existing REQ-001–014 behavior; it does not own
another implementation or acceptance suite for those requirements.

Carry forward **approved-report-delivery.md → Worked example / Output convention /
GitHub consumption**, **approved-code-review-support.md → Approved pull_request
extraction contract / Model input and automatic behavior**, and
**approved-deferred-inputs.md** for evidence and approval. Spec revisions 8–9 and
the shared execution-expiration amendment supersede conflicting older design
text: no report helpers or Check annotations, no separately supplied repository
guidance, no approval-specific expiration and no exactly-once delivery promise.

Task 009's **SURFACES / AC-011** row retains ownership of the runnable public
standard-report example, report/delivery topic docs, introduced operations and
connected runtime journeys. This task adds their single teaching page and entry
links; it does not transfer or duplicate those obligations.

## Inputs, existing owners and dependency output

Task 009 supplies the runnable public version of
`design/worked-examples/standard-report/`, its actual public example path under
`examples/`, tested commands/output and the declaration→extraction→review→
requirement→Check/comment journey. Its dependencies supply the served connection,
declaration, admission, extraction, publication and approval operations. Wait for
009's runnable example and surface proof before publishing this page; the design
copy still says it is not runnable and is not a public code source.

Reuse the topic guides already owned by tasks 002–008:

- `operate/connect-github.md` for operator setup, tenant connection and access.
- `build-and-verify/declare/declare-a-repository.md` for TOML, selected policy,
  binding/execution discovery, eligible PRs and supersession.
- `reference/verifier-card.md` and `reference/extraction-output.md` for extraction
  and ordinary Prompt/workspace bindings.
- `build-and-verify/verify/publish-deferred-evidence.md` and
  `build-and-verify/verify/approve-a-pull-request.md` for tested later-input and
  approval authoring, inspection and decision commands.

The existing docsite owns guide structure, code embedding and discovery. Follow
`docs/AGENTS.md` and `docs/CONTRIBUTING.md`: frontmatter supplies navigation;
`CodeFromFile` embeds journey-tested public assets; generated indexes come from
`mise run docs:generate`. No navigation component or new docs machinery is needed.

## Teaching flow and output

Write one guide, **Build a code review pipeline**, under Build and verify →
Verify. Start with the outcome and explicit prerequisites: running configured
server, credential/permissions, selected GitHub repository and model deployment.
Link the operator's App registration/configuration steps instead of copying that
reference; walk the developer through tenant connection and its access check.
GitHub's installation/authorization browser flow is a prerequisite, not a Wyrd UI
workflow.

Teach the public standard-report example in order, with files, commands and
success observations at each meaningful step:

- Author `wyrd.toml` with `[repository.verified_by]` and `pull_request` activation;
  check, declare and confirm the selected exact Repository policy.
- Explain the Verifier's `extractor: pull_request` step, pinned source and ordinary
  `pr`/`changes`/`context` inputs into its Agent/Prompt placeholders and workspace.
  Teach where to customize instructions and the model route. Inspect extraction
  as ordinary output through existing HTTP and MCP execution details only.
- Explain `response_format: code_review`, the one final report envelope, coverage,
  findings and exact-source suggestions. Show how the example's ordinary
  requirement chooses its verdict: incomplete coverage is inconclusive;
  critical/high findings fail; otherwise it passes. Severity/confidence describe
  findings, not verdict; an empty report alone does not prove a complete review.
- Use the existing tested deferred-input and approval authoring assets to explain
  extending the policy, declaring it before the next PR admission, inspecting
  the pending occurrence and approving/rejecting it with normal permission.
  Explicitly require approval if it must gate success. Publish matching evidence
  when a declared deferred input blocks progress. Both continue the same execution
  and share its original expiration; neither receipt means GitHub delivery finished.
- Open an eligible PR, discover its execution from the binding and follow the
  reviewed commit through status/result and delivery inspection to the Check and
  `COMMENT` review with inline findings/suggestions. A waiting Check remains
  `in_progress`; a suggestion does not edit source or approve a merge.

Keep this the complete journey page; link topic/reference pages for exhaustive
fields, variants and errors. Use the tested commands and assets from 009 and the
earlier guides, not copied design YAML or an independently maintained example.
Use existing language tabs where SDK operations differ, respecting the extraction
inspection exception. End with a compact `If it fails` table and `Next` action.
Diagnosis must let the reader distinguish missing evidence/pending approval from
failed, skipped or pending delivery and find the safe reason. Explain that
lookup-before-create retries use retained output without rerunning review, honor
cooldowns and may rarely duplicate after a lost answer; supersession/access loss
skips later writes. Link detailed topic diagnosis rather than enumerating faults.

## Write set

Paths below are relative to `docs/`:

- `src/content/docs/build-and-verify/verify/build-a-code-review-pipeline.md` — the
  single end-to-end guide, with normal title/pillar/section/topic/order metadata.
- Existing connection, declaration, publication and approval guides above, plus
  task 009's report/delivery topic page if needed — narrow links to this journey,
  retaining their topic ownership and procedures.
- `public/llms.txt`, `public/llms-full.txt` — regenerate from content, never edit
  by hand. Generated API pages should not change for this documentation task.

The public example, fixtures, production code, SDKs, schemas, architecture and
approved spec/contracts remain outside this task's write set. If the actual
example or served operations cannot support the agreed tutorial, report the
concrete predecessor gap; do not invent an operation or silently expand scope.
A material need beyond the approved contract is `SPEC_REVISION_REQUIRED`.

## Obligations and observable proof

| ID | Approved obligation and observation | Proof owner and method |
|---|---|---|
| TUTORIAL (REQ-016, AC-011) | A fresh reader builds and customizes the standard-report policy, declares it, follows an admitted PR through extraction/review and observes the pinned Check/comment; can resolve a documented wait and inspect delivery separately from verdict, without private orchestration | Documentation reader pass against the rendered page, executing its commands/files against 009's runnable public example and real WyrdTestServer/Postgres journey setup with local GitHub/model transports. Reuse 009's connected proof and earlier evidence/approval assets; record commands, observed outputs and any blocked prerequisite. Content review compares fields, permissions, errors and HTTP/MCP extraction reads with actual served operations and revision 9 contracts. |

Run `mise run docs:generate`, review generated changes, then `mise run docs:check`
for content, command/link, build and accessibility proof. The docs gate does not
execute the full example journey: separately rerun the focused journey commands
recorded by 009 for the public example and the evidence/approval steps used here,
with their existing managed setup. Record actual test paths/selectors from those
owners; do not invent names or add a new runtime suite. A reader must be able to
follow the page's sequence as written, not merely run disconnected fixture tests.
Keep an unproven page `draft: true` until that proof succeeds. Finish with
`git diff --check` and a focused content/diff review.

## Non-goals

No tutorial implementation during planning. No second runnable example, new
review strategy, report helper library, new operation/permission, UI workflow,
CI-provider integration, automatic source modification, GitHub approval/request
changes or stronger delivery/recovery guarantee. Task 009 and predecessor tasks
retain runtime implementation, topic docs and their existing acceptance proof.
