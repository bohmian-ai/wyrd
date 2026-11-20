---
id: TASK-004B-R1
kind: remediation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 6
supporting_spec: SPEC-wyrd-ui-main-integration
supporting_spec_revision: 28
requirements: [REQ-001, REQ-002, REQ-003, REQ-009, REQ-014, REQ-018, AC-001, AC-008, AC-012]
depends_on: [TASK-004B]
parent_task: TASK-004B
remediates: [FIND-TASK-004B-1, FIND-TASK-004B-2, FIND-TASK-004B-3, FIND-TASK-004B-4, FIND-TASK-004B-5, FIND-TASK-004B-6]
---

# Preserve consolidated results and finish the refresh lifecycle

After correction, an engineer can page and refresh Observe/Verify without
repeating unrelated support work or leaving obsolete queries running, while
the shared statistical reads preserve existing displayed values and trends.
Focused real-server tests establish the outstanding analytical and EXPLAIN
boundaries. This task does not authorize implementation automatically; return
it to the caller for scheduling under `$wyrd-implement`.

## Authority and immutable input

Approved contract: `changes/active/verification-workflow-ui/spec.md` revision 6,
with supporting UI spec revision 28 and their existing retained component
requirements. Original task: `tasks/TASK-004B-ui-query-consolidation.md`.
Read this directory's [verdict](verdict.md) and
[validated diagnoses](findings-validation.md), which supply exact source
locations and full finding records.

- Base: `2ee8bddcab3fff94a75ba35fbcdc4a91a9b9f49f`.
- Reviewed candidate: `7e2b3f6fab8c81fb85769eee37d0c7fd6b633146`.
- Retain original task/spec/reports and compare remediation directly against
  this candidate, with the cumulative original range available.

The selected consolidation design and `design/query-verification.md` sections
1, 3 and 4 already require these behaviors and proof; no new product contract
is introduced. Applicable repository testing/architecture rules remain binding.

## Diagnoses and correction boundaries

**FIND-TASK-004B-1 — sparse stats.** The shared count FILTER groups buckets
from a wider population than the original per-stat WHERE. `loadStatStrip`
passes extra zero rows to the unchanged StatStrip sparkline; the diagnostic
comparison removes them and hides a visible change. Correct bucket membership
at the existing `packages/ui/src/lib/observe.ts` stat query/projection owner.
Preserve the original filtered count series, actual range totals and shared
reads; other stat types must retain meaningful recorded zeros. Consumers are
Observe and shared StatStrip users, including Home/Card pages. Do not globally
drop zeros or redesign the component to conceal a changed result.

**FIND-TASK-004B-2 — invalidation.** Observe's layout and Verify's server load
track list-only URL inputs and start support unconditionally on every load.
Partition those existing BFF compositions by list inputs versus population
inputs so paging/sort/shown columns rerun only their consumers. Keep portable
loaders authoritative and credentials server-owned. Range/filter, explicit
refresh, Live and authorization changes must still refresh affected support;
detail/drawer navigation must retain its existing behavior. This does not
authorize a cross-request result cache or a new domain endpoint.

**FIND-TASK-004B-3 — Live settlement.** The page scheduler awaits SvelteKit
`invalidate`, which resolves on the initial data chunk before nested support
settles. Retain the page-owned scheduler and list-first streaming, but wait for
the actual refresh's support settlement before another automatic refresh.
Both success and failure must settle scheduling; disabling Live or leaving the
page stops future ticks. Reuse the existing section/load lifecycle rather than
introducing another orchestration layer unrelated to it.

**FIND-TASK-004B-4 — cancellation.** Passing the server Request signal proves
only forwarding: actual SPA navigation does not abort that signal in the pinned
framework/adapter path. Connect support work to a real browser/BFF cancellation
lifecycle for both supersession and departure. Reuse current abort-capable
loaders/executors/transports and existing host ownership; stop pending upstream
reads, not merely their display. Preserve typed failures, latest-result guards,
host-only credentials, SSR completion and successful independent content. A
manual abort on a synthetic event is insufficient proof.

**FIND-TASK-004B-5 — populated aggregation proof.** Existing real fixtures are
too small to exercise independent facet cutoffs and contain no gateway calls
for populated token/currency equivalence. Extend the existing authorized
real-server fixture/test owners with bounded data that exercises those cases.
Compare complete typed loader results with original SQL and independent facts,
including independent facet ranking/cutoff and separate currency values. Reuse
existing Node/MCP host and ingest/gateway fixture mechanisms. This gap requires
tests/evidence only unless they expose an implementation defect.

**FIND-TASK-004B-6 — explained-query negative proof.** Extend the existing
public Oracle/query journey owners using current table/tenant authorization,
deadline/cancellation and governed resource/output-limit fixtures. Test the
relevant EXPLAIN forms through the public operation, asserting no unauthorized
plan disclosure, correct typed failure and cleanup rather than successful
partial plans. Keep shipped-client positive proof; add focused adapter failure
assertions where needed to prove projection. No new runtime control, endpoint,
harness or production rewrite is justified solely by missing proof.

## Ordered closure scenarios

Record exact selectors before production fixes and execute behavior-failing
RED where applicable; do not reconstruct historical RED for the original task.
Use existing owners, conventional setup/action/assertions and real boundaries.

| Finding / existing obligation | Observable closure and narrowest credible proof |
|---|---|
| 1 / REQ-014, scenario 2 and verification §3 | Real Logs buckets with ERROR in A and only INFO in B, plus the corresponding filtered span-count behavior: complete per-stat series matches original SQL and resulting trend points. Valid zeros in other stats and range totals survive; selected shared-read count remains. Existing `ui-library.integration.test.ts` plus focused StatStrip/component assertion. Remove the comparison's masking normalization where it conceals this difference. **Evidence — PASS:** `ui-library.integration.test.ts` › `consolidated Observe bundles return the original results; EXPLAIN ANALYZE compares their work` (640f14375). RED: `errors buckets: expected [ [ '2026-10-19T03:00:00Z', 1 ], …(1) ] to deeply equal [ [ '2026-10-19T03:00:00Z', 1 ] ]`: the shared FILTER read added a `[05:00, 0]` bucket. Fixed at `statQueries`/`loadStatStrip` (`packages/ui/src/lib/observe.ts`); the masking normalization in `ui-library.test.ts` was removed. GREEN then, and again on the final tree. Limitation: the SQL expectation in `ui-pages.test.ts` (line 89) was not run with this fix; it was caught and updated in df44f7296. |
| 2 / REQ-018 consolidation, scenario 5 and verification §4 | Observe and Verify paging, sorting and shown columns issue only affected list reads. Production-BFF navigation verifies range/filter still refresh support and detail navigation preserves the board. Existing `ui-pages.test.ts` request assertions and `ui-pages.integration.test.ts` browser owner. **Evidence — PASS:** unit `ui-pages.test.ts` › `a client navigation reads only its list, and support reads the population whatever the list shows`, RED `expected Promise{…} to be undefined`. Browser: `Observe reads support once per population, stops it when superseded or left, and Live waits for it` (Older page, header sort, Columns) and `the Verify dashboard filters, groups, brushes, pages, opens an execution and returns to the same list` (Show 7 more, header sort, Group by session; Verify has no Columns control). Each asserts zero `/support` requests, an unchanged supporting-query count, and unchanged facet text and charts. A range change (Observe zoom) and a filter change (Verify facet) each make exactly one `/support` read, of 3 (Observe traces) or 1 (Verify) supporting queries. Opening an execution and returning keeps the board (existing Verify journey assertions). |
| 3 / scenario 5 and verification §4 | Let initial invalidation complete while support remains held beyond multiple Live tick opportunities: only one automatic refresh is active; releasing/failing support permits the next; disabling/leaving stops ticks. Exercise both actual streamed page compositions, preserving primary render. Existing component/browser owners; a mock holding invalidate itself is insufficient. **Evidence — PASS:** unit `ui-pages.test.ts` › `Live starts no refresh until the last refresh has read its support, and stops when the page goes`, RED `expected "vi.fn()" to be called 1 times, but got 3 times`. Browser `liveWaitsForSupport` runs on both boards with Playwright's clock: after the first tick, support is held for 25 s (two more tick opportunities) with 1 list refresh and no new supporting query. A failed support read shows its error, and the next tick then makes refresh 2; a released one allows refresh 3. Pausing, and resuming then leaving, each run 60 s with no further reads (5 list reads in all). The upstream outcomes are `failed`/`finished` only, so Live stopped nothing. Browser RED before the fix: no supporting query after a refresh. Live's effect tracked `current.live` through the state object every refresh replaces, so its cleanup stopped the refresh before it read support. Fixed in 79a4ef661 by deriving `live`. The unit test's mocked `invalidate` did not reproduce this. |
| 4 / scenario 5 and verification §4 | Hold upstream support, allow the list to render, then perform real browser navigation/supersession. Observe upstream read cancellation and no late overwrite; repeat departure from both relevant route compositions. Existing production-BFF browser/executor seams, without synthetic host load or new engine controls. **Evidence — PASS:** unit `ui-pages.test.ts` › `a support read stops its upstream reads when the browser closes the connection, and leaves the connection as it found it`. Browser, both boards, real navigation through the production BFF. Supersession (Observe zoom then zoom; Verify `failed` then `completed`) and departure (the Destinations link from the Observe layout and from the Verify page) leave every held upstream answer `abandoned`: the BFF closed it before its end, and none is `finished`. Superseded queries are closed unread, so no late result reaches the page. A late arrival is still covered by the newest-load guard (`streamed.test.ts` first test). Limitations: the SDK's `bifrost.stream()` takes no signal before the response head, so the gate forwards the head and the cancellation is observed at the stream. Before the head, the server's query deadline bounds the work. The gate holds SQL queries only; Verify's Card-name `cards.list` read passes through unheld. |
| 5 / REQ-014, scenario 2 and verification §3 | Multiple facets independently exceed their ten-plus-overflow cutoff with relevant tied/null/native values. Populated token/non-token calls and multiple currencies produce original-equivalent complete results and independent expected totals, with no currency combination. Existing real-server UI integration owner and original SQL catalog. **Evidence — PASS (tests only):** facet cutoffs in the row-1 test (640f14375). `ui-library.integration.test.ts` › `gateway usage stats count tokens apart from calls and never add currencies` (982e8fe00). Failures while writing it were expectation corrections (`'24'` vs `'24.000000000000'`, cost bucket shape), not product defects. GREEN on the final tree. |
| 6 / REQ-018, AC-012, scenario 0 and verification §1 | Explained denied-table/foreign-tenant reads disclose no unauthorized plan data; governed deadline, cancellation, resource and plan-output refusals remain typed, terminal-safe and cleaned up. Existing `wyrd-testing` Oracle public-query journeys; focused shipped adapters where error propagation needs proof. **Evidence — PASS (tests only):** e948bebd0, `wyrd-testing --test oracle`: `published::explain_reports_the_selected_plan_with_select_authority`, `published::planning_read_stall_ends_at_query_deadline`, `capacity::memory_refusal_preserves_oracle_health_and_next_query`, `mcp::pg_tests::agent_runs_three_table_analytical_query_through_mcp`, all PASS. Denied-table and foreign-tenant codes are asserted with no plan disclosed; the cancel code is `WYRD_VALA_500_QUERY_EXECUTION_FAILED`. Failures while writing them were fixture corrections (role scope, planning-stall placement). No Rust has changed since. |

## Runners and affected checks

Prepare only existing build prerequisites from the original task when required
by changed artifacts. Existing runner families are:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts src/lib/ui-pages.test.ts src/lib/streamed.test.ts
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts src/lib/ui-pages.integration.test.ts -t '<actual focused selector>'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=<actual exact selector>)"'
```

Selectors above are deliberate placeholders, not claims that tests exist or
ran; resolve them from the actual test owner before execution. Run focused
new/changed selectors and relevant existing regressions, then `mise run ui:check`
for UI changes. Use the original `ts:build`, `ts:build:testing`, `build:server`
and UI prerequisites only as needed. If Rust changes, run `fmt`, `lints` and
targeted owner checks; if adapters/projections change, run their applicable SDK
checks and focused journeys. Run Cargo-backed commands sequentially. Regenerate
contracts only if the source contract changes, through the existing generator.
Full journey matrices and broad gates belong to integrated change review.

**Executed on the final tree (79a4ef661 plus uncommitted test-only edits to `ui-pages.integration.test.ts`, `ui-pages.test.ts` and `streamed.test.ts`):**

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/ui-pages.test.ts src/lib/streamed.test.ts src/lib/ui-library.test.ts   # 61 passed
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 WYRD_UI_CAPTURE_DIR=<dir> pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t 'Observe reads support once per population|the Verify dashboard filters|authenticated traces and spans|authenticated logs investigation|authenticated metrics investigation|observe boards match approved canvas'   # 6 passed
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t 'consolidated Observe bundles|gateway'   # 2 passed
mise run ui:check   # 0 type errors, 93 tests passed
git diff --check    # clean
```

Rust: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner >/dev/null && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all --no-fail-fast -E "test(=<selector>)"'` for each row-6 selector at e948bebd0, then `mise run fmt` and `mise run lints`.

Mechanical deviations:
- Support for client navigations and Live comes from the host-private GET routes `observe/support` and `verify/support`, which call the same loaders. The maintainer chose this design during implementation.
- No TS test seam can hold a query, so the browser journeys use a test-local loopback proxy (`supportGate` in `ui-pages.integration.test.ts`) between the BFF and the server. The browser→BFF→executor→server path stays real.
- The Verify lifecycle assertions extend the existing Verify dashboard journey, which owns the seeded executions.

TDD: FIND-2/3/4 unit RED preceded the production change, and browser RED preceded the Live-timer fix. One violation: the `Streamed.read` fix that fails a successful non-JSON answer (made at review-hook request in df44f7296) preceded its test. `streamed.test.ts` › `a read whose successful answer is not JSON fails the section instead of clearing it` was added afterwards; reverting the fix makes it fail.

## Preserved scope and handoff

Preserve accepted query consolidation, execution populations/permissions,
selected EXPLAIN paths, metric math/lazy keys, Card/session duplicate removal,
Back restoration and first-response behavior. No migrations, new dependencies,
domain APIs, backend storage/auth semantics, mock redesign or blanket cache.
Do not revisit accepted hosted capacity code without a demonstrated affected
defect. The recorded no-L1 override remains a disclosed performance limit for
the reviewed correction, not authority to waive gates on newly expanded hot-path
work. If further hot-path behavior changes, follow its applicable benchmark and
wait rules or return the material decision to the maintainer.

Record candidate-bound proof and limitations in this task when implemented;
preserve original TDD history and immutable review reports. Do not stage,
commit, push, implement another task, or launch acceptance review unless asked.

## Revision 1 — Approved browser regression acceptance (2026-11-19)

The maintainer approved this addition while remediation implementation was
already underway. Apply it to the current implementation and its closure
evidence; do not restart completed work or rewrite the original review. This
section makes the existing preservation and lifecycle proof explicit without
adding product behavior. It supplements the ordered closure scenarios above;
populated aggregation and EXPLAIN proof remain separate acceptance cases.

Use automated browser tests against the existing production BFF fixtures for
Observe and Verify. Assertions must establish both the displayed experience and
the upstream request lifecycle. Screenshots document visible states; a video is
not required and cannot replace executable assertions.

| Scenario / finding | Required pass criteria |
|---|---|
| Paging, sorting and columns / FIND-TASK-004B-2 | The list updates correctly; unchanged charts and facets remain visible; no supporting queries repeat. Range/filter changes still refresh their affected sections. Exercise each control where the dashboard exposes it. **PASS:** see closure row 2. Supporting queries per population: Observe 3, Verify 1. Paging, sorting, columns and grouping add 0. |
| Delayed supporting query / retained scenario 5 behavior | The list renders while support remains pending. Loading is explicit. Support success or failure settles without removing the successful list or displaying false zeros. **PASS, ruling requested:** the list renders while support is held, on both boards. On a fresh mount (Back to a population that never settled) the facets show `Loading · Counting the facets`, and Verify's charts show `Reading the timelines`. A failure shows `WYRD_SPEC_502_UPSTREAM_FAILURE` in each section, keeping the list and showing no counts (0 facet buttons). On a mounted page, the previous population's sections stay until the new ones settle (retained scenario 5) with no busy marker. Observe's trace overview band is absent while loading instead of showing a loading state; this has been the case since 278543e39 and was not changed here. |
| Live refresh / FIND-TASK-004B-3 | Allow the list and initial invalidation to finish while support remains blocked across two refresh opportunities. No second refresh starts until support settles. Verify that both support success and failure release scheduling, and disabling Live or leaving stops future ticks. **PASS:** see closure row 3. |
| Navigation and supersession / FIND-TASK-004B-4 | Navigate away or change filters while support is blocked. Observe cancellation at the upstream executor/transport and verify late results cannot overwrite the new view. Prove both departure and supersession through actual browser navigation. **PASS:** see closure row 4. |
| Back navigation / retained REQ-002 and scenario 5 behavior | Restore the same filters, range, selected state and scroll position without losing page height while support reloads. **PASS:** Observe: filter, range and scroll are restored at a 1440×600 viewport, page height ≥ before, while exactly 3 supporting queries are held. Verify: shown rows (57), filter, range and scroll are restored after opening an execution and returning, while 1 query is held. Remembered support keeps the facets visible during the reload. Observe's open-span selection is covered by `authenticated traces and spans`, not this journey. |
| Sparse statistics / FIND-TASK-004B-1 | Complete returned series and rendered trend points match the original query, including buckets with no matching errors. Preserve meaningful recorded zeros for other stat types. **PASS:** see closure row 1. |

Hold work through existing test seams so pending, success, failure and
cancellation states are deterministic. Keep the real browser-to-BFF-to-executor
path in the lifecycle tests. Do not substitute a manually aborted fabricated
Request, a mocked invalidate promise that includes all support, screenshots
alone, or synthetic host load for those observations. Reuse the existing test
owners and focused runners above; no new harness is requested.

Record the following with the remediation evidence:

- Tested commit identity, any relevant uncommitted changes, exact commands and
  selectors, assertion results and artifact locations. Identify the final
  candidate to which passing evidence applies.
- Per-scenario request counts and upstream cancellation observations sufficient
  to distinguish a stopped query from an ignored late result.
- Representative loading, settled, support-error and Back-restored screenshots
  at a recorded fixed viewport in light and dark mode. Compare them with the
  approved appearance and describe any visible differences. Any intentional
  visible difference requires the maintainer's ruling before acceptance.

Record evidence as the work proceeds. Existing valid proof may be reused where
it exercises these actual boundaries; this revision does not require fabricated
historical RED or duplicate runs of unchanged, adequately covered behavior.

**Screenshots:** full page at a 1440×900 viewport, light and dark, written when `WYRD_UI_CAPTURE_DIR` is set: `lifecycle-{observe,verify}-{loading,settled,support-error,back}-{light,dark}.png` (16 files, from the 6-test run above). Compared with `references/original-canvas/png/Verify.png` and with the passing `observe boards match approved canvas` journey, the settled boards keep the approved layout: WHERE bar, charts band, Group by, facet rail and table. The canvas has no loading or error states, so these need the maintainer's ruling:
1. Observe's overview band is absent while loading (existing behavior).
2. Mounted pages keep the previous sections during a population change with no busy marker.
3. In dark full-page captures, the shell background ends at the 900 px viewport height (existing styling).
No other intentional visible change was made.
