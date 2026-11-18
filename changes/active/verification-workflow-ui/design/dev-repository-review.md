# Dev repository review

Status: finalized — verdict and pinned PR citation correction agreed

## Problem

`mise run dev:full` exercises the canonical support-desk application, but leaves
repository integration and pull-request review unexercised. Startup has no
review Verifier, and connecting GitHub deliberately creates no Repository Card.
A developer therefore cannot complete the review journey from the dev stack.

## Desired outcome

Startup declares a reusable code-review Verifier. After one GitHub connection
and automatic post-connect API declaration by the dev harness, the owned demo repository is registered
and bound to that Verifier. Opening a non-draft pull request then reaches a
terminal execution, a `wyrd/code-review` GitHub Check, and a `COMMENT` review.
The developer can inspect the Repository and execution using the existing
Repositories and Verify pages, without requiring the deferred UI improvements.

The maintainer's TASK-004A revision requires actually connecting this live demo
repository during integration and opening real PRs to prove the whole pipeline.
Bootstrap implementation, local GitHub substitutes and setup instructions alone
are insufficient. The connected dev session remains available for further PRs;
fresh restarts retain the existing reconnection requirement.

## Constraints and non-goals

- Use `bohmian-ai/wyrd-example-py`. The maintainer selected the owned demo repo
  and authorized necessary changes there; no fork configuration is needed.
  Interpret the reply's `wyrd-example-y` as the demo repo named in the handoff.
- Preserve fresh databases, the pinned example checkout, existing dirty-tree
  protection, support-desk setup and workload, and ordinary declaration semantics.
- GitHub connection and Repository declaration remain separate. No automatic
  Card creation during connection, browser automation, or persistent dev database.
- Model responses are scripted; extraction, execution, verdict calculation,
  GitHub delivery, and UI reads use real Wyrd behavior.
- No migrations, permission changes, runtime redesign or UI feature work.
  Approved spec revision 5 adds only the verdict/source-citation correction below;
  implementation and its verification remain TASK-004A work.

## Proposed implementation

### 1. Publish review authoring files in the demo repository

Add `review/reviewer.yaml` and `review/code-review.yaml`, adapted from
[`examples/review-a-pull-request`](../../../../examples/review-a-pull-request).
Keep `code-reviewer:1.0.0`, `code-review:1.0.0`, `gpt-4o`, deployment `openai`,
`response_format: code_review`, the `Pull request:` user-message prefix, and the
extract → Agent → code requirement flow. Neither Card identifies a repository.
Retain the existing `fs.read` and extraction workspace configuration; the mock
returns a final answer directly, so it does not need to exercise tools.

The requirement keeps the example's behavior: incomplete coverage is
inconclusive; critical/high findings fail; otherwise pass. The verdict belongs
to this requirement, not to the mock or GitHub renderer.

Add root `wyrd.toml`:

```toml
[repository.verified_by]
verifier = ["./review/code-review.yaml"]
runs_on = { kind = "pull_request" }
```

Use the file reference so both standalone Verifier declaration and Repository
declaration hydrate the same Agent and Verifier graph. Do not add a separate
Repository YAML or copy provider IDs into configuration. Publish the upstream
commit before changing the base pin in `scripts/dev/example-app.sh`.

### 2. Seed the reusable Verifier during ordinary startup

Extend `example-app.sh setup` to declare `review/code-review.yaml` using its
existing `uv run --no-sync wyrd` environment, after the `openai` deployment
exists. Do not declare `wyrd.toml` here: a fresh database cannot yet have the
interactive connection. Keep setup failure visible through the existing
`dev:full` failure handling.

### 3. Register through the existing API after connection

Revised recommendation in response to maintainer feedback: `dev:full` owns the
dev-only bootstrap and sends an authenticated Repository declaration request to
`POST /v1/cards` after the GitHub connection covers the canonical demo repository.
No second terminal command, change of working directory, or `dev:repository`
task is needed. Connection itself still creates no Card: this is a separate
API operation performed by the development harness with the dev principal.

Use the existing connection read operation to observe readiness while the stack
continues serving. Resolve the canonical demo repository from that connection's
repository list, then submit a Repository Card with its verified provider
identity and a `pull_request` binding to the exact already-seeded code-review
Verifier reference. The API validates access and selects the Repository through
ordinary declaration behavior. No local Git inference is necessary for this
known demo repository. Use the existing authentication/client facilities;
do not invent a new endpoint or duplicate registration in the GitHub callback.

Keep this small watcher within the existing supervised dev process lifecycle.
Waiting for a connection is normal startup progress, not a reason to block
support-desk serving or the initial batch. Make registration success and errors
visible. Missing GitHub configuration must leave the existing support-desk dev
flow usable. Stop the watcher with the stack and stop it after successful
registration; do not turn it into a persistent reconciler. Implementation must
verify the declaration receipt and selected Card rather than equating an
accepted request with a ready Repository. Authentication, the ordinary declaration
completion path, and existing error handling remain authoritative.

### 4. Add deterministic summary-only review responses

Add review-specific MockServer expectations for `gpt-4o` requests whose user
message starts `Pull request:`, with priorities above all current entries
(current maximum 40). Reuse the existing JSON matcher and response envelope.
Do not intercept support-desk traffic.

Recommend two canned outcomes: ordinary PRs return a complete empty report;
PR titles beginning `[wyrd-demo-fail]` return a complete report containing one
explicitly scripted high-severity finding. Match the title field in the `pr`
metadata portion of the user message, not an arbitrary occurrence in source
context. Give the failing matcher precedence over the generic review matcher.
The title is present in extraction and is already supplied to the Agent.
Validate the actual rendered prompt and matcher during implementation.

Both responses contain all required report members, return assistant content
as a JSON string, `finish_reason: stop`, and no tool calls. Findings have unique
non-empty IDs, `locations: []`, `evidence: []`, and `remediation: null`. Summaries
and the canned finding explicitly identify scripted development data. This
exercises success and failure without inventing source anchors or implementing
response templates. Both outcomes produce a Check and a non-inline COMMENT
review; findings appear in the Check summary. Inline comments are outside this
proposal's smoke coverage.

### 5. Commit the developer setup journey

Extend `CONTRIBUTING.md` near `dev:full`, linking the existing Connect GitHub
and review guides for shared details. Include the actual dev addresses and
commands rather than requiring the ignored `mise.local.toml`:

1. Configure a developer-owned GitHub App installed on the canonical demo repo,
   with Contents read, Pull requests write, Checks write and the documented events.
2. Set the existing `WYRD_GITHUB_APP_ID`, `WYRD_GITHUB_APP_SLUG`,
   `WYRD_GITHUB_OAUTH_CLIENT_ID`, secret-file variables, and
   `WYRD_GITHUB_PUBLIC_URL=http://127.0.0.1:8080`. Commit placeholders only.
3. Set callback and setup URLs to that loopback server's existing integration
   routes. Sign into the UI at `127.0.0.1`, consistently avoiding `localhost`.
4. Forward only webhook deliveries to
   `http://127.0.0.1:8080/v1/integrations/github/webhook` using a documented
   webhook relay; put the relay URL in the App's Webhook URL. Keep browser URLs
   on loopback. GitHub documents Smee for this local-development use:
   [testing webhooks](https://docs.github.com/en/webhooks/testing-and-troubleshooting-webhooks/testing-webhooks).
   Reuse an available relay; no new Wyrd tunnel manager or dependency is needed.
5. Run `mise run dev:full`, sign in as `development`, connect the tenant, and
   let the dev harness register the Repository through the API. The callback
   currently ends on JSON; return to the UI manually. Reconnect after each fresh
   start; the harness seeds and registers again.
6. Open a small non-draft PR targeting the demo repository, then inspect the
   Check, review, Repository, and execution. Use a separate failing-title PR to
   exercise the negative conclusion. Do not infer success from queue admission.

### Ownership and command locations

| Owner | Files and responsibility |
| --- | --- |
| `bohmian-ai/wyrd-example-py` | Review Agent and Verifier YAML, root `wyrd.toml`: portable authoring files for ordinary users. No Wyrd dev bootstrap logic. |
| Wyrd | Pinned checkout and startup Verifier seeding in `scripts/dev/example-app.sh`; `dev:full` orchestration and supervised post-connect API registration in the existing development harness. |
| Wyrd | MockServer expectations and committed App/relay/developer instructions. |
| Existing Wyrd server | Connection, access validation, declaration, selection, execution, extraction, verdict, and GitHub delivery. No new endpoint or callback behavior. |

The proposed developer journey is: configure App and relay → run
`mise run dev:full` from Wyrd → connect GitHub in the browser → observe the
harness's successful registration → open a PR against the demo repository.
The demo's `wyrd.toml` remains useful for the normal declaration journey, but
this known-repository bootstrap does not need to read it or enter that checkout.
Keep its binding aligned with the harness's pinned Verifier reference.

## Evidence and rationale

Verified by source inspection, not by running the dev stack:

- [`example-app.sh`](../../../../scripts/dev/example-app.sh) fixes origin and
  commit, protects dirty worktrees, creates `openai`, and declares fixtures.
  [`mise.toml`](../../../../mise.toml) owns the supervised startup and batch.
- [`Repository authority`](../../../../architecture/wyrd-design.md#repository)
  resolves origin against connection access; connection creates no Cards under
  [GitHub App association](../../../../architecture/wyrd-design.md#github-app-association).
  [Repository selection](../../../../architecture/wyrd-design.md#repository-selection)
  already defines replay and failure behavior; no custom registration state.
- [`boot/mod.rs`](../../../../crates/wyrd/wyrd-server/src/boot/mod.rs) composes
  the verification runtime when verification and queued roles are enabled.
  [`verification/mod.rs`](../../../../crates/wyrd/wyrd-server/src/verification/mod.rs)
  composes its runner with an operator pool, hosted blocks, and reachable Scribe.
  [`app/server.rs`](../../../../crates/wyrd/wyrd-server/src/app/server.rs) starts
  GitHub report/delivery recovery when GitHub and queued roles are enabled.
  These are composition paths, not proof of successful dev execution.
- [`report.rs`](../../../../crates/wyrd-spec/src/verification/report.rs) and
  [`presentation.rs`](../../../../crates/wyrd/wyrd-server/src/components/github/presentation.rs)
  support empty locations and a completed report's COMMENT review without inline
  findings. [`extraction.rs`](../../../../crates/wyrd/wyrd-server/src/components/github/extraction.rs)
  includes the PR title in Agent input.
- [`Connect GitHub`](../../../../docs/src/content/docs/operate/connect-github.md)
  and [`server.sh`](../../../../examples/connect-github/server.sh) already own
  App settings. Document only the dev-specific address and relay differences.

## Verdict and PR citation correction

The maintainer explicitly agreed to preserve valid check verdicts, make pinned
PR extraction citable, and correct the shipped example/demo rather than changing
the passing PR expectation. Specification revision 5 records the agreement and
TASK-004A implements it before its dev bootstrap acceptance scenarios.

### Current defect and existing owners

Before the correction, the example's final Python check returns passed with
`citations: []`. `vala-verify/src/blocks/engine.rs::judge` sends Agent and code
checks through `Judged::cited`, which filters citations and downgrades an uncited
pass. `AgentExecutor` also performs its own downgrade; `TenantEvidence::cited`
filters invalid source/locator citations. All participating owners must implement
the same corrected policy, not just bypass one engine branch.

The hosted `pull_request` extractor obtains pinned PR source from the accepted
queued execution. It produces a step output without original-source lineage;
mounting that output does not create a citable original. The defect is source
representation, not missing PR content. The earlier architecture paragraph
required citations for passing Agent checks while the engine also enforced it
for code; revision 5 replaces that rule in the owning architecture before code.
Source inspection established these paths; no runtime reproduction was run in
this design session.

### Selected behavior and connected implementation

A schema-valid Agent/code verdict, including an uncited pass, keeps its outcome.
Malformed output and invalid supplied citations are explicit validation failures
through the existing check-error path; no silent filtering or accepted-verdict
rewrite. Existing aggregation and provider/infrastructure error handling remain.
This preserves a check's decision without accepting invalid evidence. An authored
requirement can expressly demand evidence; the engine adds no implicit count rule.

Reuse the existing source-slot/typed-locator mechanism for an engine-owned
original source backed by the retained PR extraction. It identifies the exact
provider repository and pinned comparison/head commits. Downstream extraction
output, Agent input/output, workspace and final check retain reachability to it.
Use existing evidence custody and retained recovery; do not add another evidence
store, arbitrary-step-output trust or external reread. The slot must be documented
for authors, distinct from caller slots and impossible for callers to replace;
its name and private representation are implementation-owned. Existing JSON
pointer locators can cite the retained extraction without a new citation schema.

Correct the shipped review example and guide, then derive the upstream demo
from it. Bind the pinned PR evidence alongside the report in the final code
check and emit its valid source citation for the review, including a clean one.
The report's finding locations remain independent: zero findings requires no
invented source line. Keep complete/no-high-or-critical passed, high/critical
failed and incomplete inconclusive; GitHub conclusions continue to follow those
verdicts. No `CodeReviewReport` fields or rendering contract change.

### Proof and limits

Prove valid Agent/code verdict preservation with and without citations, explicit
invalid-output/source/locator rejection, and unchanged aggregation. Run the actual
shipped example through the hosted queued runtime: inspect the exact retained
PR citation through the final result, recover it through the current retained
path, and reject substitution from another execution/tenant or caller slot.
Keep TASK-004A's live canonical-demo PR, Check/review and existing UI proof.
The engine correction touches a verification hot path, so review requires a
sanctioned-wait inventory and a full clean VALID verification L1 benchmark
compared with the newest same-host/experiment history. These are implementation
proof requirements, not results already obtained.

## Decisions and alternatives

Agreed: use the owned canonical demo repository; change it as necessary.

Selected for the requested TASK-004A supplement: post-connect API registration inside the
dev harness; preserve fresh DBs; file-backed ordinary example binding; final summary-only mocks with ordinary-pass and
failing-title outcomes. A persistent DB adds lifecycle and sealing-key concerns
without solving an agreed need. The separate wrapper task and manual declaration step were withdrawn after
maintainer feedback; neither is needed for the requested dev bootstrap.
Dynamic anchors would be warranted if inline review demonstration becomes a
required outcome; static fake SHAs would not provide that proof.

## Standards conflicts

The maintainer chose to replace the prior passed-Agent citation downgrade rule.
Specification revision 5 and the owning Verifier citation paragraph now record
verdict preservation and explicit validation, plus pinned PR source citations.
No runtime code was changed in this documentation revision. Other architecture,
tenancy, evidence custody and hot-path constraints remain in force.

## Open questions

- Declaration against the actual production wheel and dev gateway, extraction
  completion, and Agent turn count remain untested. Expect one final model turn
  with no fs.read call; verify this from MockServer's recorded requests.
- Confirm the failing-title matcher against rendered Agent input. This is a
  private fixture mechanic; if it cannot be done reliably with existing matchers,
  report the limitation before introducing templates or a scenario controller.

## Handoff

[Specification revision 5](../spec.md) records the explicitly agreed correction.
[TASK-004A](../tasks/TASK-004A-dev-repository-review.md) owns its engine/evidence
regressions, shipped example and demo updates, development bootstrap and live
GitHub journey. Existing TASK-004 stays implemented; 005–007 gain no new scope.
No runtime implementation, live connection or benchmark was performed during
this revision. Private source-slot naming/layout and test helpers remain with
the implementer; published citation shape and exact pinned provenance must
satisfy the approved contract.
