---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 8
supporting_spec: SPEC-authorization-model
supporting_spec_revision: 9
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-011, REQ-012, REQ-014, REQ-020, AC-001, AC-002, AC-007, AC-008, AC-009]
depends_on: [TASK-002, TASK-003, TASK-004, TASK-005A]
---

# Current and historical PR verification sets

## Outcome and activation

A person granted one Repository follows its PR link to the latest review set,
selects an earlier set, opens a member and retained output, then returns with
the same selection. Another Repository contributes no members, counts or facets.
Each member keeps its own status and recorded verdict; a set has no combined
verdict.

Approved revision 8 reconciles REQ-003/020 with current retained-parent authority
and records this task's no-definition presentation in the selected
[resumption design](../design/resume-005-007.md). Before implementation, record
the integrated predecessor outputs and actual dependency proof below;
specification approval does not certify their runtime qualification.

TASK-003 is implemented. Consume its shared discovery queries and URL/filter
owners; do not restart it. TASK-002 supplies the inspector and exact producer
reads; TASK-004 supplies Repository navigation. TASK-005A supplies admitted
generations and complete retained membership, including refusals. Its request
and retention have recorded delivery evidence; the old table-only read blocker
is superseded by authorization-model REQ-006/007 and the current
[retained-parent authority](../../../../architecture/wyrd-security-posture.md#resource-and-retained-data-visibility).
Record the integrated revision and focused dependency results; neither old
proposed headers nor specification approval establish runtime acceptance.

Dependency evidence at base `05fc05138` with this task's changes in the tree:
[D1] the first prerequisite command below, 1 passed; [D2] the second (the
Alice journey), 1 passed before and after it was extended for this task.

## Existing owners and connected implementation

Carry forward the design's “Proposed TASK-005 revision” and its rationale.
Existing `execution_inputs` rows supply provider host/stable repository ID,
PR number, exact Repository policy, PostgreSQL observation time retained as
`recorded_at`, generation, complete member IDs and request/delivery identity.
TASK-005A's explicit review request is fixture input where needed, not a new UI
action. Admission authorization remains independent of reading a set.

Use existing caller-authenticated `bifrost.query` through the shared Node/MCP
executors. Oracle applies parent visibility before expressions, counts and
pagination. Repository Card read includes its PR execution/evidence reads,
including custom roles; no referenced-Verifier read or GitHub configuration
grant is needed. Table permission alone cannot unlock another parent's rows.
Opening a Verifier definition checks that Card independently.

Extend existing shared schemas/loaders/catalog/components, `observe.ts`,
`execution.ts`, `verify-host.ts` and Repository/page owners as appropriate:

- Resolve current from the newest retained observation for that PR, not row
  arrival, SHA alone or the Verify time picker. Explicit generation links never
  fall back to current when absent. Read bounded history with explicit bounds.
- Use frozen member identities and show incomplete retention/truncation
  truthfully. Reuse server-selected result semantics: a queued member's selected
  checkpoint result supplies its verdict, not the newest arbitrary result row.
  The dashboard's time-range/slack assumptions cannot silently cut off old sets.
- Add the approved PR context to Repository activity, Verify filters/groups/
  columns and exact execution links. Keep general verification filters and both
  charts on the same authorized population. No per-member status/detail fanout
  to render a set or list.
- Reuse the inspector on member open. When definition lookup is refused or the
  Card has been deleted, keep its truthful refusal and allow selection of
  readable recorded requirements, invocations and publications. Use existing
  node/producer selection and bounded reads; do not invent authored nodes/edges,
  make a definition grant implicit or hide completed progress solely because
  its definition is unavailable. Occurrence/decision controls belong to 006.
- Retained history uses its recorded identity; a current Repository policy or
  GitHub configuration read must not become a prerequisite for a retained PR
  link. Independent definition/configuration failures do not erase readable
  execution facts.

Route names, query decomposition, bounds, private helpers and component
factoring are implementation-owned. No new server endpoint, SDK wire field,
membership ledger, permission evaluator, migration, provider lookup, rerun
button or separate PR dashboard. Backend retained-status correction is in 007
and is not a predecessor of this task.

## Ordered acceptance scenarios and evidence

Implement each missing scenario test-first. Add assertion locations, exact
selectors, observed RED/GREEN and limitations to these entries; do not create a
second evidence ledger. Already-correct upstream behavior needs focused reuse
proof, not manufactured failures.

1. **Open an actual current set.** Public connection/declaration and PR admission
   produce a multi-Verifier generation and retained pins. A permitted caller
   loads it through the shared Node/MCP loader: exact source/policy/member IDs
   and each member's nullable selected result, status and verdict match the
   accepted records, with no combined verdict and no detail/status fanout.
   **Map:** REQ-003/012/014/020, AC-007/008. **Tier/owner:** shared loader and
   real-server `ui-library.integration.test.ts`. The missing PR-set loader/
   presentation should produce RED.

   - Reuse: retained `execution_inputs` rows (origin `pull_request`) carry the
     frozen source, generation, `members` and the selected policy as
     `workload_card_uid`; the Verify population (`SIGNALS.executions`) already
     selects each execution's status and checkpoint-selected result. Gap: no
     reader grouped those rows into sets. `loadReviewSets`
     (`packages/ui/src/lib/observe.ts`) adds one history query and one member
     query over that same population, through the existing `bifrost.query`
     executors. No new server read, executor or table.
   - Proof: "TASK-005: the current review set of a pull request loads its
     recorded members through the Node and MCP hosts"
     (`src/lib/ui-library.integration.test.ts`). A real connection, declared
     two-Verifier policy (`policies/gated-review.toml`) and
     `reviewPullRequest` admission; `ci-gate` passes on published evidence
     and `code-review` ends inconclusive. Through both executors it asserts
     the accepted generation, source pins, policy UID, request key, frozen
     member order, each member's status/verdict against its exact status
     read and its `result_id` against the retained result row, no `verdict`
     key on the set or the data, and exactly two `bifrost.query` requests.
   - Command [U1]: `mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t '<name>'`.
   - RED: `TypeError: loadReviewSets is not a function`. GREEN: 1 passed.
   - Test corrections before RED/GREEN (test input only): the fixture waited
     on the member that was not parked for `ci`; the public status read has
     no `result_id`, so the expectation now reads the retained result row.
   - Design note: the loader is keyed by Repository Card name and number
     (`{ repository, number, generation? }`), matching the mock's route
     `/repositories/<name>/pulls/<number>`. The name is the retained
     `subject_name`; no Card or GitHub read is needed to open a set. The same
     test also reads the members back through `loadRecordTable` filtered by
     `pull_request = '<name>#7'`, with their `review_set`.
   - Presentation proof (`src/lib/ui-library.test.ts`, [U2]): "a pull
     request shows its review sets, each member with its own status and
     verdict, and no combined verdict" renders `ReviewSets` and checks the
     emitted intents. RED: the component was undefined at render. GREEN.
   - PR title (maintainer decision, 2026-11-23, replacing the "no new SDK wire
     field" constraint for this one field): `PullRequestActivation.title`
     (`crates/wyrd-spec/src/verification/execution.rs`) is optional and kept
     from the GitHub pull request read that admission already makes
     (`app.rs` `PullRequestBody`, `pull_requests.rs` `Observed::of`). It
     is display only: set identity still compares base, head and head
     repository, so a title edit admits nothing. Sources admitted before this
     change have no title, and the page heads them with `#N`. TS wire types
     regenerated; the Python stub `verify/__init__.pyi` updated by hand.
     RED: [U1] scenario 1 test, `title: 'Change'` missing from the set, and
     [U2] "a pull request shows its review sets…", heading without the title.
     GREEN: [U1] 3 passed; [U2]/[P1] 68 passed; Rust
     `components::github::pull_requests::tests::current_provider_state_decides_the_observation`
     1 passed; [D1] 1 passed; `mise run fmt`, `mise run lints`,
     `mise run py:lints`, `mise run codegen:check`, `mise run ui:check` all
     exit 0.
   - GitHub link (maintainer decision, 2026-11-23, same exception): optional
     `PullRequestActivation.full_name`, read from `base.repo.full_name` on
     the same admission read; display only, since a rename keeps
     `repository_id`. `ReviewSets` links `https://{host}/{full_name}/pull/{n}`
     in a new tab; a set without one shows no link. The fake GitHub now
     serves `full_name` from its installation's repository list.
     RED: [U1] scenario 1, `full_name: 'acme/api'` missing; [U2] link
     `acme/api#7 ↗` not found. GREEN: [U1] 3 passed; [U2]/[P1] 68 passed;
     the Rust unit above 1 passed (now asserts `full_name`).
   - Diagnosis: [B1]
     `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(/^pg_github_pull_requests::/)"'`
     failed 1 of 26. Symptom:
     `pull_request_lifecycle_admits_supersedes_and_ends_through_the_webhook`
     (`pg_github_pull_requests.rs:595`) whole-object `assert_eq!`. Evidence:
     served source carries `"title": "Change"` and `"full_name": "acme/policy"`,
     the expectation has neither. Cause: expectation predates the two
     retained display fields. Fix site: that expectation; exact equality kept.
     Diagnostician (codex `gpt-6.1-sol`, medium, read-only): same cause and
     fix site; MCP, CLI and SDK journeys assert single fields or partial
     objects and are unaffected. After the fix [B1] 26 passed.
   - Status: PASS.

2. **Keep current and history exact.** Request a fresh review at the same head,
   then advance the head. Follow current, pin an earlier generation outside the
   Verify time range, refresh with delayed older records, and request an absent
   pinned generation. Current follows the latest retained observation; pinned
   history/execution never switches, and missing pinned history is reported
   rather than replaced. Interrupted retention, refused members and a read/page
   bound never turn a subset into a complete set.
   **Map:** REQ-002/003/012/020, AC-007/008. **Tier/owner:** existing real-server
   UI integration owner consuming actual admission/retention outputs; pure
   selection/bounds branches in `ui-library.test.ts`. Missing history and
   completeness handling should produce RED; reuse backend retention proof.

   - Proof (pure, `src/lib/ui-library.test.ts`): "review sets follow the
     newest observation, keep a pinned set, and report a missing pin instead
     of replacing it" and "a review set is complete only when every frozen
     member is visible, within the history bound". They cover current
     selection, a pin read from its own observation time under an unrelated
     Verify range, a missing pin with no member read, an interrupted set
     (`complete: false`, the unseen member `retained: false`), a refused
     member with a status and no verdict, the history bound (`more`), a pin
     older than the bound, and empty/denied history and denied member reads.
     Command [U2]: `pnpm package && pnpm exec vitest run src/lib/ui-library.test.ts -t 'review set'`.
   - RED: `more` absent and 201 sets returned for a 200-record bound; then
     GREEN 2 passed. The interrupted-set assertions already passed on the
     scenario 1 loader.
   - Proof (real server): "TASK-005: a pull request follows its newest
     review set while a pinned earlier set stays exact" (U1): first review,
     rerequest at the same head, push; then another review while the pin is
     open; then an unretained pin. Passed when first run, because the pin
     and bound were implemented under the pure tests above.
   - Reused backend proof [D1], run at this revision, 1 passed:
     `pg_github_pull_requests::retained_reads_resolve_current_and_history_membership`
     (the first prerequisite command below). It owns delayed older records
     and interrupted handoff at the source; a TypeScript test cannot stall
     Scribe, so "refresh with delayed older records" is not re-driven here.
   - Presentation proof ([U2]): "a review set says when it is the latest
     pin, incomplete, cut short or not retained". Page proof
     (`src/lib/ui-pages.test.ts`, [P1]
     `pnpm exec vitest run src/lib/ui-pages.test.ts -t '<name>'`): "a pull
     request page follows the latest review set, keeps a pinned set in its URL
     and opens a member". RED there was an unresolved import of the route,
     which is a missing-module failure rather than an assertion failure.
   - Browser proof: scenario 5's journey selects the earlier set, reloads and
     stays on it.
   - Status: PASS, with the delayed-records limit above.

3. **Respect Repository grants everywhere.** Two Repositories share a Verifier.
   A family-scoped reader and a minimal custom Card-read role see only their
   Repository's sets/members/counts/pages/Verify facets. An exact-version grant
   covers only its policy UID; table-only permission reveals no hidden managed
   rows. GitHub configuration stays denied. Foreign/inaccessible execution links
   are indistinguishable from unknown; grant/removal affects subsequent
   authoritative reads in the existing signed-in session.
   **Map:** REQ-001/003/011/012/014/020, AC-001/007/008;
   authorization-model REQ-004/006/007. **Tier/owner:** real server → shared
   Node/MCP reads and production BFF in the existing UI integration owners.
   Extend the existing Alice journey's actual grants/admitted reviews rather
   than replacing its assertions. New PR-set consumer mismatches should be RED;
   unchanged source-boundary enforcement may already pass.

   - Proof (real server, Node and MCP, [U1]): "TASK-005: a Repository grant
     reads that Repository's review sets and nothing of another Repository".
     `api` and `web` share `ci-gate`; `api` is reviewed under policy
     versions 0.1.0 and 0.2.0. A family viewer and a custom `cards:read` role
     read both `api` sets, get "no set" for `web`, count only `api#7` in
     the Verify `pull_request` facet, and get
     `WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND` for a `web` member. An
     exact-version viewer sees only the set its policy UID admitted; a pin to
     the other set is reported missing. A `bifrost_query:read` key with no
     Card read gets no rows. Unbind then bind changes the same client's next
     read.
   - Proof (production BFF and browser, [D2]): the Alice journey now opens
     `payments#7` from Repository activity, sees one passed member and no
     `ledger` text, reads `ledger/pulls/9` identically to `nosuch/pulls/9`,
     and gains then loses `ledger`'s set with the ledger grant in the same
     session. Its existing assertions (GitHub configuration denied, foreign
     execution link equal to an unknown one) are unchanged and pass.
   - RED/GREEN: both passed once their test inputs were right; enforcement
     is the existing Oracle boundary and the loader was already built under
     scenarios 1 and 2. Test-input corrections only: `gated-review.toml`
     needed its own version (0.2.0) to coexist with `gated`; the facet read
     needed a time range; the exact-version reader's only visible set is its
     current one.
   - Limitation: the browser journey uses the family viewer. The custom role,
     exact-version and table-only readers are proven through the shared
     loaders, not through a browser session.
   - Status: PASS at the loader and BFF tiers named above.

4. **Inspect without definition access.** The permitted Repository reader has
   no grant to the member's Verifier definition; also exercise retained work
   after a source Card is deleted while its retained parent remains authorized.
   Open the member, select a recorded requirement/producer/output and refresh.
   Authorized facts, bounded progress, previews/downloads and exact selection
   remain usable; graph/YAML stays unavailable and no topology is fabricated.
   **Map:** REQ-002/004/005/006/014, AC-002/008; authorization-model REQ-006.
   **Tier/owner:** component test plus real-server loader and BFF/browser.
   Missing recorded-selection navigation, including completed progress skipped
   because definition lookup failed, should produce RED.

   - Change: `loadExecution` (`packages/ui/src/lib/execution.ts`) reads
     recorded progress whenever the definition is not populated; it still
     skips only for a readable definition with no steps.
     `VerificationExecution` lists the recorded requirements and the steps
     that recorded an invocation under the refusal, as buttons sending the
     existing `execution.select` intent. No node or edge is drawn.
   - Proof (component, [U2]): "an execution whose Verifier definition is
     refused still lets a reader select what was recorded". RED: `Unable to
     find an accessible element with the role "group" and name "Recorded
     without the definition"`. GREEN.
   - Two existing assertions pinned the old skip and now assert the read:
     "execution loader reads each part once and keeps refusals apart"
     (`ui-library.test.ts`) and "a judged execution shows its judgments…"
     (`ui-library.integration.test.ts`, [U1], an `executions:read`-only key
     through Node and MCP). Both failed on the old expectation, then passed.
   - Proof (real server): the scenario 3 grant test opens an `api` member
     as the family and custom readers: definition not populated, recorded
     progress populated, Repository name present. The Alice journey [D2]
     selects `Requirement green`, reloads with it still selected, and finds
     no graph node.
   - FAIL (not proven): work retained after its source Card is deleted; a
     step output preview and download without the definition against a real
     server (the `ci-gate` member has no steps; the component test covers
     the preview with fixtures).
   - Status: selection PASS; the two cases above remain open.

5. **Connect navigation and hosts.** Through the production browser follow
   Repository → PR → older set → member → output → Back. Copy/reload links and
   change the PR while each selection is open. Return context, selected
   generation/execution/producer and existing general discovery behavior remain
   correct. Exercise affected reads in Node/MCP/custom/built-artifact hosts,
   denied/unavailable/empty/partial states, and the approved desktop/mobile,
   light/dark, keyboard/touch and long-content composition.
   **Map:** REQ-001/002/011/012/014, AC-001/007/008/009. **Tier/owner:** existing
   UI host/artifact and production-browser integration owners. Missing links/
   return wiring should produce RED. Supplement the approved mock only for
   definition-unavailable recorded selection; record AC-009 comparison evidence
   and get the maintainer's ruling on remaining mock differences as required by
   [wyrd-ui](../../../../.agents/skills/wyrd-ui/SKILL.md#matching-an-approved-mock).

   - Proof (host units, [U2]/[P1]): "an executions list names the pull
     request of each row and opens its review sets" (RED: `pull_request is
     not a executions column`), "an execution of a pull request names it and
     opens the review set that admitted it", and "Repository activity, Verify
     and an execution each lead to the pull request review sets". The last
     two were written before their production code; their first run was
     after it, so no RED was observed for them.
   - Proof (production browser, [D2]): Repository activity → `payments#7` →
     pinned set → member → recorded selection → reload → Back to the pinned
     set; then, after a rerequested review, latest set marked current →
     earlier set → reload stays → member → its pull request link → the same
     earlier set → Back.
   - Existing discovery behavior: Verify's facet and group lists gained
     `pull_request`; the two pinned lists in `ui-pages.test.ts` were
     extended. `mise run ui:check`: 112 passed, 0 type errors.
   - FAIL (not proven): the built `ui://wyrd/view` artifact and a custom
     host rendering `ReviewSets`; "Copy link" on the pull request page (the
     page has none; the URL carries the selection); changing the pull request
     while a selection is open; light/dark, mobile, keyboard/touch and
     long-content comparison against the mock (AC-009).
   - Links and wording (maintainer decision, 2026-11-23: follow the mock and
     link conventions). `ReviewSets` takes an optional `link` (intent → URL);
     the pull request page supplies it, so set rows, the banner's latest-set
     link, member executions, the Repository and each named Verifier are
     `<a href>`; without `link` (the MCP artifact) they stay buttons that emit
     the same intents. Set occurrence uses the mock's words from recorded facts:
     a request is "rerequested", a head differing from the set before it is
     "new head", the first set of a fully read history is "opened", otherwise
     "pull request event" (GitHub's action is not retained). RED: [U2] rows
     read "requested review"; [P1] "a pull request page follows…" found only
     the crumb link. GREEN: [U2]/[P1] 68 passed; `mise run ui:check` 112
     passed, 0 type errors; [D2] Alice journey 1 passed with set and member
     selectors changed from button to link.
   - Preference review (commit hook, then codex `gpt-6.1-sol` medium for the
     full text): the page's `onaction` now navigates through its `link`, card
     URLs reuse `writeDetail`; an intent the host gives no URL renders as a
     button, never an `<a>` without `href` (asserted in [U2]; restoring the
     old choice fails it); occurrence takes each row's index instead of a
     per-row search.
   - Repository activity Head and Policy (maintainer decision, 2026-11-23):
     the shared executions population gains `head` (`inputs.head.sha`) and
     `policy` (`subject_name@subject_version`), PR origin only, beside
     `pull_request`; both selectable on Verify. Activity reads Time, Pull
     request, Head, Verifier, Policy, Status, Verdict; Head shows 7 characters
     with the full SHA as its title. RED: [U1] scenario 1 `No field named
     head`; [P1] "Repository activity, Verify and an execution…" headers
     lacked Head and Policy. GREEN: [U1] 3 passed; [P1]/[U2] 68 passed;
     `ui:check` 112 passed; [D2] 1 passed. The "Latest per PR / Include
     earlier review sets" switch is deferred (maintainer decision).
   - Diagnosis: [D2] first failed at `wyrd execution status`
     (`ui-pages.integration.test.ts:2849`) with `unknown field \`title\``.
     Evidence: `target/debug/wyrd` built 15:08, `wyrd-spec` change 16:25;
     `PullRequestActivation` is `deny_unknown_fields` like the other wire
     types. Cause: a stale CLI binary; `build:server` does not rebuild it.
     Fix site: none in code; `mise exec -- cargo build --locked -p wyrd-cli
     --bin wyrd`, then [D2] 1 passed. No diagnostician: the fix touches no
     test, timing or code.
   - Status: navigation PASS; the items marked FAIL remain open.

## Verification and handoff

Read `wyrd-ui` before UI edits. Existing owners are `ui-library.test.ts`,
`ui-pages.test.ts`, `ui-library.integration.test.ts` and
`ui-pages.integration.test.ts`; fixtures supplement connected journeys.
Record new focused selectors before RED. Current prerequisite regression commands:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_github_pull_requests::retained_reads_resolve_current_and_history_membership)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t 'Alice is given payments in Settings'
```

Prepare current `ts:build`, `ts:build:testing`, `build:server` and UI build
prerequisites through mise before UI integration. Run focused new/affected
component and integration selectors, `mise run ui:check`, UI build, catalog
regeneration and `mise run codegen:check` for schema/catalog changes. Apply
affected BFF/session owner checks from the UI skill, including
`mise run test:identity:ui` where that boundary changes. Preserve existing
query consolidation and bounded reads; add no benchmark or unrelated gate.

Gallery: `_gallery` has `ReviewSets` cases (latest, pinned, not retained,
each state, narrow). Lanes run: `mise run ui:check` (exit 0),
`mise run codegen:check` (exit 0), `git diff --check` (clean). TDD: the host
link tests in scenario 5 and the browser/grant journeys in scenarios 3 and 5
first ran after their production code; every other new behavior has an
observed RED. Not run: `mise run test:identity:ui` (no BFF/session code
changed).

Planning claims no implementation, runtime proof or independent acceptance.
Full shipped-surface journey lanes belong to integrated change review.
