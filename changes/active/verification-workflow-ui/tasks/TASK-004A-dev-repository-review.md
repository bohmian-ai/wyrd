---
id: TASK-004A
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 5
requirements: [REQ-010, REQ-011, REQ-013, REQ-016, REQ-017, AC-006, AC-009, AC-011]
depends_on: [TASK-004]
---

# Exercise repository registration and PR review in dev:full

## Outcome and activation

Supplement implemented TASK-004 with a usable local development journey:
`mise run dev:full` seeds a reusable code-review Verifier; the developer connects
GitHub once; the development harness registers the owned demo Repository through
the existing Cards API. Opening a subsequent non-draft PR produces a completed
execution, `wyrd/code-review` Check and COMMENT review, visible through the
existing Repositories and Verify pages. No second declaration command or
checkout switching is required.

As part of this task's integration verification, actually connect the live
`bohmian-ai/wyrd-example-py` repository to the running dev tenant and exercise
PRs against it. Shipping bootstrap code and instructions alone does not finish
this task. Model calls stay mocked; GitHub connection, webhook delivery,
repository extraction, execution and GitHub writes use the real integration.

This task implements revision 5's verdict/PR citation correction under
REQ-016/017 and AC-011, then supplies development inputs and connected proof for
AC-009 and TASK-004's Repository/connection surfaces. It does not change
connection semantics: connecting GitHub creates no Card. The dev harness makes
a separate authenticated declaration after access exists. Preserve REQ-010/011's
provider identity and selected policy, and REQ-013's connection/registration
separation. The [development design supplement](../design/dev-repository-review.md)
records the selected flow, approved correction and investigated existing owners.

TASK-004 supplies the implemented Repository/settings views; TASK-003 supplies
Verify and the existing execution inspector supplies execution reads. Reuse the
integrated connection, declaration, hosted runtime, extraction and GitHub delivery
owners. Source presence or queue admission alone is not completion proof.

## Inputs, ownership and connected implementation

| Owner | Change and consumer |
|---|---|
| `bohmian-ai/wyrd-example-py` | Add `review/reviewer.yaml`, `review/code-review.yaml`, and root `wyrd.toml`. Wyrd's pinned checkout consumes that upstream commit; ordinary users can also declare the example config. |
| Wyrd `scripts/dev/example-app.sh` | Advance the base pin and seed the review Verifier after the existing `openai` deployment is created, using the prepared production SDK/CLI environment. |
| Wyrd `dev:full` harness | Supervise the post-connect API registration alongside the serving stack; use the current dev principal and existing client authentication/declaration handling. |
| Existing Wyrd/Vala verification owners | Preserve valid check verdicts, explicitly validate supplied citations, and propagate the retained pinned PR source through the actual review/check path. Correct the shipped PR example consumed by the demo. |
| Wyrd MockServer fixtures | Supply deterministic final code-review reports through the actual gateway route without matching support-desk requests. |
| Wyrd `CONTRIBUTING.md` | Commit the App, relay and developer journey instructions, including waiting/registered messages and fresh-restart behavior. |

### Correct verdict validation and PR source lineage first

The existing `vala-verify` engine's `Judged::cited` and Agent executor currently
turn an uncited pass into inconclusive; both paths must satisfy REQ-016. Extend
the existing check validation/evidence owners, preserving valid returned verdicts
and rejecting malformed output or invalid supplied citations with an explicit
validation reason. Remove silent citation filtering as an acceptance mechanism.
A check error continues through the existing error/aggregation path, including
its inconclusive error result where applicable; distinguish that explicit
validation failure from accepting and rewriting a valid verdict. Preserve
existing declared-data validation and do not weaken evidence validation.

The hosted pull-request extraction already obtains exact accepted source through
`GithubConnections::extract_pull_request`. Make that retained extraction an
engine-owned original citable source, reusing the existing source-slot and typed
locator mechanism and `TenantEvidence` custody/validation. Its identity must
remain bound to the accepted provider repository and pinned comparison/head
commits. Carry source reachability through extraction output, review Agent
inputs/output and workspace, and the final code check, including existing
retained recovery. Document the engine-owned source slot for check authors;
ensure callers cannot replace it or collide with it. Source-slot naming and
private representation are implementation-owned; do not add a second citation
schema, arbitrary-step-output trust, a new storage owner or an extra external
read to validate extraction already held for the execution.

Update the shipped `examples/review-a-pull-request/{verifier,reviewer}.yaml` and
its guide as needed, then derive the demo files below from that corrected example.
The final requirement binds the reviewed PR evidence as well as the report and
emits a valid citation to the original PR source, including clean reports with
no findings. Keep incomplete coverage inconclusive, critical/high findings
failed and clean complete reports passed. Requirement citations are separate
from `CodeReviewReport` finding anchors; its closed schema stays unchanged.

Do not implement against the old architecture rule that downgraded uncited
Agent passes: revision 5's approved authority amendment replaces it. This narrow
runtime correction is now in scope; unrelated runtime redesign is not.

### Portable demo files and pinned input

Adapt `examples/review-a-pull-request/{reviewer,verifier}.yaml` into the demo
files. Keep reusable `code-reviewer:1.0.0` and `code-review:1.0.0`, deployment
`openai`, model `gpt-4o`, `response_format: code_review`, the `Pull request:`
message prefix, extraction workspace and fs.read declaration. Neither Card names
a repository. Preserve the requirement: incomplete coverage is inconclusive;
critical/high findings fail; otherwise pass.

Root `wyrd.toml` uses the local Verifier path:

```toml
[repository.verified_by]
verifier = ["./review/code-review.yaml"]
runs_on = { kind = "pull_request" }
```

Record the actual upstream commit before advancing the pin. Upstream publication
is a real delivery dependency, not a fabricated task output; commit/push remain
maintainer-authorized Git actions. Preserve existing dirty-checkout protection
and the support-desk application. Do not add development bootstrap logic to the
demo application's deploy script.

### Automatic API registration

After normal startup seeding, observe `GET /v1/integrations/github` through the
existing authenticated client while the app and startup batch continue running.
When the active connection covers `bohmian-ai/wyrd-example-py`, take its configured
host and stable provider repository ID from that exact entry. Submit a Repository
through `POST /v1/cards`, binding the exact already-seeded Verifier with
`runs_on.kind = pull_request`. The API continues to validate access and select
the Card using ordinary declaration behavior. Do not infer identity from a
working directory or hard-code its numeric GitHub ID.

Verify the selected Card and exact binding before announcing registration.
Use existing authentication/declaration completion facilities, not a second
credential-exchange or publication implementation. Keep the small dev-only
bootstrap within existing process supervision; stop it with the stack and exit
after success or a visible failure. Waiting for connection must not block app
serving or the batch. Absent optional GitHub configuration leaves support-desk
use available. An inaccessible demo repository must never register another
repository or report false success. Do not add a persistent reconciler, new
endpoint, callback mutation or `dev:repository` command.

### Connect the live demo repository during integration

Use the developer-provided GitHub App and webhook relay with an installation
that includes `bohmian-ai/wyrd-example-py`. Complete the existing browser
installation/OAuth connection for the dev tenant; user interaction and secrets
remain within that normal flow. Let the running harness perform its separate
Repository declaration and confirm the selected Card and exact binding before
opening PRs. Do not replace this with a fabricated connection row, manually
inserted Card, or provider substitute for the live acceptance run.

This task delivers a working connected demo session, so the maintainer can open
further PRs against the same repository and observe the pipeline while the stack
runs. Fresh restarts still require reconnection; no persistence is added. If
App configuration, installation access or browser authorization is unavailable,
record that precise unfinished dependency and continue independent work, but
do not report the live integration as passed.

### Model fixtures and developer instructions

Add review-specific gpt-4o expectations above all existing priorities (currently
40), matching user messages beginning `Pull request:`. Ordinary titles yield a
complete empty report. Titles beginning `[wyrd-demo-fail]` yield a complete report
with one explicitly scripted high-severity finding; this matcher wins over the
generic review matcher and targets the title in PR metadata, not arbitrary
source text. Validate against the actual rendered prompt.

Both responses are final assistant messages with JSON report content,
`finish_reason: stop`, and no tool calls. Follow `CodeReviewReport` exactly:
include every required member; finding IDs are unique and non-empty;
`locations: []`, `evidence: []`, and `remediation: null`. Clearly identify
scripted development data in the report. Findings appear in the Check summary;
the COMMENT review need not have inline comments. Do not invent source anchors
or introduce dynamic mock templates.

Document the existing GitHub App permissions/events and `WYRD_GITHUB_*` settings
using placeholders and secret files, linking the existing Connect GitHub guide.
Set `WYRD_GITHUB_PUBLIC_URL=http://127.0.0.1:8080`, keep browser setup/callback
URLs on loopback, and forward webhook deliveries through the relay to
`/v1/integrations/github/webhook`. Explain browser sign-in/connection, manual
return from the JSON callback, automatic registration, PR scenarios, inspection,
and reconnection after each fresh database start. No secrets are committed.

## Acceptance scenarios and evidence

The first three scenarios exercise AC-011 under REQ-016/017. The remaining
scenarios exercise AC-009 over REQ-010/011/013 and the applicable AC-006
connection/declaration path. Add actual assertion locations, exact focused
commands, observed RED/GREEN results and limitations beneath each scenario;
keep expectations unchanged. Existing already-correct behavior uses existing
proof rather than manufactured RED failures. The evidence already recorded below
is retained from revision 4. It does not prove the new REQ-016/017 correction;
rerun affected assertions against the corrected runtime and authoring files.

1. **Preserve valid Agent and code check decisions.** Run each executor path
   with schema-valid passed, failed and inconclusive results, including passed
   with an empty citations list and with valid reachable citations. Assert that
   each accepted requirement keeps the authored outcome and the unchanged
   `decide` rule aggregates it. Include a direct/public real-server verification
   path so executor and engine behavior agree. Today uncited passes are rewritten,
   providing the missing-behavior RED. This tests REQ-016, not a new minimum
   citation count.
2. **Reject invalid results explicitly.** Supply malformed output and citations
   to an unreachable slot, a nonexistent location, and another execution/tenant's
   source. Assert an explicit validation reason through the existing check-error
   path, no silently dropped citation and no accepted/relabelled authored verdict.
   Keep existing provider/infrastructure failure and aggregation behavior. Test
   the engine/Agent/evidence validation owners and public server boundary; use
   local providers for repeatability. Current filtering supplies RED where it
   silently accepts or alters such results. These cases exercise REQ-016/017's
   original-source validation boundary without accepting arbitrary step output.
3. **Cite the pinned PR through the shipped example.** Admit a queued PR with
   the runtime enabled and run the actual shipped example through extraction,
   review Agent, workspace and final code check. Read its retained requirement
   citation: it resolves into that execution's pinned PR evidence, including the
   exact provider repository and comparison/head commits. A complete clean report
   has no findings yet passes with that valid citation; high findings fail and
   incomplete coverage stays inconclusive. Exercise the current retained/recovery
   path and confirm it preserves the same pinned source and citation rather than
   rebasing onto a later PR head. Forged/colliding caller input cannot replace
   the source. Today extraction has no original citable slot and the example
   returns an uncited pass. Use SDK → WyrdTestServer + repository Postgres → SDK
   journey proof with existing GitHub doubles and gateway fixtures; do not
   manufacture a caller input to bypass the missing source lineage.
4. **Seed the actual example policy.** Start setup against the real dev server
   with no GitHub connection. Load package input from the pinned demo checkout,
   create the gateway deployment and declare the review Verifier. Public Card
   reads return the reusable Agent/Verifier with the expected exact references;
   no Repository has been declared. This fails today because startup has no
   review policy. Use real-server/client proof, not only YAML parsing.

   *Evidence (PASS).* Journey `pg_dev_repository_review::dev_harness_registers_the_demo_and_reviews_its_pull_requests` in
   `crates/wyrd/wyrd-server/tests/integration/pg_dev_repository_review.rs`
   (command C1) runs `example-app.sh prepare` then the real `setup` with the
   production wheel against a bound server with no connection. It asserts
   `Agent/code-reviewer@1.0.0` (route `openai`, `tool_names: [fs.read]`), its
   Prompt (`gpt-4o`, `code_review`, `Pull request:` prefix), the Verifier's
   exact inbound reference to that Agent uid, and no Repository Card. TDD:
   VIOLATED — `setup`'s declaration preceded the journey; no RED recorded.

5. **Register after connection without another command.** Start the actual
   bootstrap unconnected; app serving and the startup batch remain usable.
   Establish connection coverage for the canonical demo identity. The harness
   separately declares and selects its Repository with the seeded Verifier;
   the existing Repository read/UI consumer sees the returned exact policy.
   An absent/unconfigured connection or one excluding the demo produces no
   false registration or substitute repository. Repeat an unchanged declaration
   through the ordinary API and observe reuse, then stop/restart the dev stack:
   its supervised work stops and the fresh run seeds/waits again. Today no
   automatic post-connect declaration exists. Exercise the real bootstrap path
   against public server operations and repository Postgres; provider transport
   can use the existing local GitHub substitute.

   *Evidence (PASS, one limit).* Same journey: `register` prints
   `waiting: GitHub connection is not_connected`; after a connection without
   the demo it prints `does not cover bohmian-ai/wyrd-example-py` and no
   Repository exists; after reconnecting with the demo it exits 0 printing the
   selected `Repository/wyrd-example-py@0.1.0`. The served Card carries the
   connection's host and repository id and exactly the seeded Verifier uid on
   `pull_request`, `selected: true`. A second `register` (ordinary Cards API
   declaration) reports the same version and leaves one Repository. No GitHub
   App: `pg_dev_repository_review::dev_harness_leaves_review_off_without_a_github_app`
   (C2) — exits 0, `review off`, no Card. `dev:full` runs `register` in its own
   process group, kills it in `stop()`, and never awaits it (`mise.toml`), so
   it blocks neither serving nor the batch; the lifecycle journey
   (`development environment lifecycle`, C3) asserts the summary line.
   Limit: stopping a *waiting* `register` with the stack, and a fresh restart
   waiting again, are proven only by inspection and the live run (now scenario 8);
   no test drives `dev:full` with a GitHub App configured. C3 PASS.
   TDD: VIOLATED — script before journey.

6. **Complete the passing review.** After scenario 5, admit an ordinary PR through
   the existing webhook path. Run real extraction → Agent → code requirement
   through gateway/MockServer and the enabled hosted runtime. Inspect a terminal
   passed execution, delivered GitHub status, success Check named
   `wyrd/code-review` and a COMMENT review. Recorded provider requests show the
   review fixture supplied a final report without requesting fs.read. The actual
   Repository/activity and Verify views expose this execution. Today no review
   mock/bound demo policy completes that connected path; a permanently queued
   execution fails this scenario.

   *Evidence (FAIL under revision 4; corrective scope approved in revision 5).* Journey `pg_dev_repository_review::dev_harness_registers_the_demo_and_reviews_its_pull_requests`, last block: the
   ordinary PR completes, delivery is `delivered`, one `wyrd/code-review`
   Check carries the scripted summary, one inline-free `COMMENT` review lands,
   and MockServer recorded one final turn with no `tool` message. But the
   verdict is `inconclusive` and the Check `action_required`, not
   `passed`/`success`: `vala-verify` `Judged::cited` (`engine.rs`) turns a
   `passed` code check with no citation of an original slot into
   `inconclusive`, and a pull-request Verifier has no original slot — the
   `extract` step has no `from`, so `Bindings::sources()` is empty for the
   Agent and the requirement. The example's (and
   `examples/review-a-pull-request`'s) "otherwise pass" branch therefore
   cannot pass; only its failing branch had a journey
   (`pg_github_delivery::the_review_example_fails_on_a_high_finding_and_comments_its_fix`).
   This was outside revision 4; REQ-016/017 now assign the correction to this
   task. This historical failure remains unclosed until rerun. The journey
   asserts the required `passed`/`success` and is red at that line.

7. **Complete the failing review and isolate the matcher.** A PR with the failing
   title follows the same real path and finishes failed with delivered failure
   Check, COMMENT review and the scripted high finding in the Check summary,
   without inline anchors. The marker in source context with an ordinary title
   does not select the failing report. The existing support-desk batch still
   receives its own fixtures. This fails today because review expectations do
   not exist. Use the real rendered model requests and public execution/delivery
   observations, not disconnected response/rendering fixtures alone.

   *Evidence (PASS).* Same journey: the `[wyrd-demo-fail]` PR finishes
   `failed`, delivered, Check `failure` whose summary holds "Scripted
   high-severity finding (development data)", one `COMMENT` review with no
   comments. An ordinary title whose changed source contains
   `"title":"[wyrd-demo-fail]` gets the ordinary report and is not `failed`.
   The recorded rendered prompt is compact JSON with a nested
   `repository.card` object. RED observed: the first matcher, which allowed one
   nesting level, let that PR through as the ordinary report; it now anchors
   on the compact metadata line before `"title":"[wyrd-demo-fail]`. Support-desk
   fixtures are unchanged (priorities 10–40). TDD: matcher fix PASS, initial
   fixtures VIOLATED.

8. **Connect the live demo and prove the complete pipeline.** With the actual
   developer App installed on `bohmian-ai/wyrd-example-py` and the webhook relay
   running, start `dev:full`, complete the normal browser connection and let the
   harness register/select the Repository. Open actual passing and failing-title
   PRs against that repository after registration. For each, follow GitHub's
   webhook through extraction, the real hosted runtime and mocked model response
   to the terminal verdict, delivered `wyrd/code-review` Check and COMMENT review.
   Open the matching Repository and execution in the existing Repositories and
   Verify pages. Record the PR URL/head commit, Repository Card reference,
   execution ID and Check/review links with the observed verdict and delivery
   result here. Keep the connected stack usable for subsequent developer PRs.
   Local provider substitutes and successful admission cannot satisfy this
   live-App/relay/browser proof. Missing access leaves this scenario unfinished;
   instructions alone are not completion. Preserve the current exclusion of
   TASK-005–007 UI features; inspect delivery through GitHub/public operations
   where those Wyrd UI projections are not yet implemented.

   *Evidence (NOT RUN).* Needs the developer App (configured in
   `mise.local.toml`), a running webhook relay, the browser connection, and
   PRs on `bohmian-ai/wyrd-example-py`. The pin `9a33805` is not yet pushed
   upstream. Live run pending maintainer action; not passed.

   Commands:
   - C1: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_dev_repository_review::dev_harness_registers_the_demo_and_reviews_its_pull_requests)"'` — historical run fails at the passing-review verdict assertion (now scenario 6).
   - C2: same with `-E "test(=pg_dev_repository_review::dev_harness_leaves_review_off_without_a_github_app)"` — PASS.
   - C3: `mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t 'development environment lifecycle'` — PASS (99.8 s; support-desk batch passed alongside).
   - `cargo clippy --locked -p wyrd-server --test integration --all-features -- -D warnings`, `cargo fmt`, `ruff format/check scripts/dev/review_repository.py`, `bash -n` on `example-app.sh` and the `dev:full` body, `git diff --check` — clean.
   - Upstream: `wyrd declare --check review/code-review.yaml` and `wyrd.toml` in the demo checkout — valid.

## Verification and boundaries

Use the existing pure engine/check/evidence tests for the narrow regression,
plus real-server verification and queued-PR journeys for the delivered flow.
An existing regression to revise for the approved semantics is
`blocks::engine::tests::hosted_check_passes_need_reachable_citations`, run with:

```bash
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::hosted_check_passes_need_reachable_citations)'
```

Rename it to describe verdict preservation when changing its assertions, and
record the resulting exact command before RED. The former downgrade expectation
is intentionally superseded by REQ-016; do not weaken unrelated source validation.
For new tests record actual focused commands rather than inventing names here.

Use existing real-server GitHub test owners with repository-managed Postgres,
including the connection, PR admission, extraction and delivery modules in the
server's single integration target. New connected-runtime proof belongs with the
existing runtime-capable WyrdTestServer/SDK journey owner; enable the actual
runtime and exercise the development bootstrap's real inputs. Keep test helper
placement implementation-owned. Tests need no production App credentials where
the existing provider substitute suffices. Record new focused selectors before
RED; do not invent test names in this task.

Existing runners are Cargo nextest through the managed Postgres wrapper and
Vitest through the UI's existing real-server lanes. Use exact focused selectors
for changed scenarios. For the existing UI journey owners, the command shape is:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t '<actual scenario name>'
```

Build prerequisites are the existing `ts:build`, `ts:build:testing`,
`build:server` and UI build, as in TASK-004. Run only format/lint and boundary
checks appropriate to the actual diff. Shell changes need syntax checks; any
non-trivial bootstrap logic needs focused behavioral proof. Follow the demo
repository's own check commands for upstream files. Full journey aggregates
remain integrated change-review work. Run `mise run fmt`, `mise run lints` and
relevant dependency/tenant/codegen checks for the actual Rust/contract diff;
never hand-edit generated artifacts. Python/TypeScript projections use the
existing citation shape; verify affected language-lifetime/consumer paths at
their owning tier if their implementation changes.

This correction touches the verification hot path. List every wait on PostgreSQL,
Scribe, object storage or Vault added or moved, matching each to a sanctioned
wait; introducing a new unsanctioned wait is outside this spec. A task/change
review may return PASS only with a full clean `mise run bench:l1:verification`
run, verdict VALID, compared with the newest record for the same experiment and
host under `benchmarks/history/`. Smoke, `--steps` and `--allow-dirty` do not count.
Record benchmark evidence when the runtime correction is verified; no benchmark
was run while authoring this revision.

No new UI features, callback navigation, Repository status columns, PR review-set
navigation (TASK-005), migrations, new storage or unrelated SDK/runtime behavior
are assigned here. Only REQ-016/017's existing-runtime correction is added.
Preserve existing authentication, tenant isolation and GitHub access checks.
A defect requiring material changes beyond REQ-016/017 returns to its owning
design/specification; it is not permission to expand this development supplement. Private watcher language/layout and
matcher syntax remain implementation-owned.
