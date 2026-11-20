# TASK-004B lifecycle review

Reviewed base `2ee8bddcab3fff94a75ba35fbcdc4a91a9b9f49f` to candidate
`7e2b3f6fab8c81fb85769eee37d0c7fd6b633146`. HEAD matched the candidate.
This independent review covers UI loading, cancellation, refresh sequencing,
request memoization and remembered sections. It does not review analytical SQL
semantics, public EXPLAIN or hosted resource admission.

Authority: approved verification-workflow-ui revision 6 REQ-002/003/014;
TASK-004B scenario 5; its selected design
`design/ui-query-consolidation.md:134–152`; supporting UI revision 28;
AGENTS.md, spec-driven development, maintainer style, TESTING and the UI
server/client boundary. The selected design explicitly requires list-only
invalidation, coalesced Live and cancellation of superseded reads; these are
assigned task behavior, not proposed improvements.

## Navigation and coverage

CodeGraph was used first for `Streamed`, `predicateFor`, Card readers and
`WyrdView`; route source, callers and tests were then traced. The pinned
framework in `wyrd-ui/pnpm-lock.yaml:38` is `@sveltejs/kit` 2.70.3, matching
the installed dependency source used below. Framework internals are cited as
evidence of this candidate's actual behavior, not new product authority.

| Boundary | Implementation and evidence | Assessment |
|---|---|---|
| Primary content before support; complete first HTML | Observe `+layout.server.ts:50–61`, Verify `+page.server.ts:65–77`; `ui-pages.test.ts:965` holds support and checks client load returns while SSR waits | Credible implementation and focused assertion |
| Late results/failures cannot replace a newer section | `streamed.svelte.ts:43–63` monotonic sequence; `streamed.test.ts:5` resolves old value and fails newest | Credible implementation; no independent rerun here |
| Remembered Back sections | Page-module maps; `streamed.svelte.ts:50–72` first-follow restore by complete URL, ten-entry limit; `streamed.test.ts:25` same-address remount and different-tenant miss; task records real browser Back journey | No concrete defect established; effects do not run during SSR, so module maps are populated in browser effects rather than shared SSR render state |
| Card and predicate memo lifetime | WeakMaps keyed by context; Card route `cards/detail/+page.server.ts:43–44` creates a fresh context; Observe/Verify loads do likewise; WyrdView supplies new per-load context | No demonstrated supported caller with a long-lived reused context; hypothetical future misuse is not retained |
| Tenant layout enumeration | `+layout.server.ts:12` untracks only origin; session metadata remains server-authorized; `browser-sessions.test.ts:239` checks enumeration and lack of URL tracking | Credible bounded optimization; sign-out/switch forms use ordinary document POST navigation, resetting browser module memory |
| List-only invalidation | URL state reads sort, columns, cursors and shown; every server-load invocation constructs support | Fails: INV-1 |
| Live waits for full refresh | timers await `invalidate()` only; framework resolves that after initial streamed chunk | Fails: INV-2 |
| Navigation cancels outstanding support | routes pass `request.signal`; no browser owner cancels streamed SvelteKit fetch, and adapter signal is not response-disconnect signal | Fails: INV-3 |

## Proposed blocking findings (independent validation required)

### INV-1 — INCORRECT: list-only changes rerun supporting analytical work

- Requirement: TASK-004B scenario 5 and selected design at
  `design/ui-query-consolidation.md:147–150` require cursor/sort/columns reload
  only their consumers; REQ-003/014 preserve bounded shared read composition.
- Locations: Observe `src/routes/t/[tenantKey]/observe/+layout.server.ts:38–61`;
  Verify `src/routes/t/[tenantKey]/verify/+page.server.ts:51–77`.
- Starting conditions/trigger: open a populated Observe or Verify dashboard,
  then change a list sort/column or request the next page (Verify changes
  `shown`). `observe-host.ts:89–115` reads these search parameters and
  `verify/+page.server.ts:58` reads `shown`; they therefore invalidate their
  server load. Observe's `listParams` excludes only detail parameters, not
  list-only parameters (`observe-host.ts:156`).
- Expected: list rereads, while unchanged supporting population/aggregate/name
  consumers remain settled and issue no read.
- Actual: each invocation unconditionally starts the entire `support`
  Promise.all. Traces repeats facets and both overview scans; Verify repeats
  discovery and Card-name enumeration. This is observable extra work during
  ordinary paging and sorting, despite the task claiming narrow invalidation.
  SvelteKit `client.js:1087–1113` confirms tracked search-parameter changes
  rerun their owning server node; there is no separate support owner here.
- Evidence: source trace above. The test at `ui-pages.test.ts:900` checks the
  sort label, not absence of supporting requests. The new load-context memos
  cannot suppress reads across separate invocations.
- Smallest correction boundary: partition BFF loading/invalidation by the
  existing list inputs versus population inputs while retaining portable
  loaders and host-owned credentials. Preserve full refresh on range/filter,
  Live and real authorization changes; do not add a cross-request result cache.
  Add a focused request-count assertion for both boards' list-only navigation,
  plus a production-browser navigation assertion that range/filter still
  refresh supporting sections and opening detail does not refresh the board.

### INV-2 — INCORRECT: Live coalesces the initial chunk, not the complete refresh

- Requirement: TASK-004B scenario 5, design `ui-query-consolidation.md:150`
  (coalesce while a refresh is in flight).
- Locations: Observe `+layout.svelte:84–92`; Verify `+page.svelte:71–79`.
- Starting conditions/trigger: Live is enabled; the list answers promptly but
  an aggregate or Card-name support read takes more than ten seconds.
- Expected: no second refresh starts until all reads from the prior refresh
  settle; a slow supporting read cannot cause overlapping automatic work.
- Actual: `invalidate(...).finally(...)` starts the next timer after the list
  chunk is applied. In the pinned SvelteKit client, `load_data` resolves on
  the initial `data` chunk (`client.js:3110–3218`), while deferred support
  promises settle from later chunks. `_invalidate` awaits `load_route` and
  remote-query resource promises (`client.js:417–499`), not nested server-load
  support promises. Thus every ten seconds a new support aggregate can start
  while its predecessor is still running. Ignoring the old result in Streamed
  does not stop that work.
- Evidence: the actual framework call path above. `ui-pages.test.ts:992`
  mocks `invalidate` with a promise held for the entire imagined refresh;
  it cannot expose the actual initial-chunk completion boundary.
- Smallest correction boundary: keep the existing page-owned Live scheduler,
  but gate its next tick on the current load's complete support settlement as
  well as initial invalidation. Preserve primary-content streaming. Test a
  real initial-chunk completion with support held past two tick periods:
  only one automatic refresh may remain active, and another occurs after
  release. Test Observe and Verify's production composition, not solely an
  artificially held invalidate mock.

### INV-3 — INCORRECT: navigation does not cancel the streamed server reads

- Requirement: TASK-004B scenario 5 and design
  `ui-query-consolidation.md:150–151` require superseded reads to cancel.
- Locations: Observe `+layout.server.ts:42`; Verify `+page.server.ts:57,68`;
  `ui-pages.test.ts:937–963` is the insufficient cancellation assertion.
- Starting conditions/trigger: a client refresh has returned its list but
  support remains pending; the reader navigates to another page/filter.
- Expected: pending support stops through the supported host's actual
  cancellation path. Discarding obsolete presentation is insufficient.
- Actual: passing SvelteKit's `request.signal` only forwards an abort if that
  signal is aborted. The pinned framework fetches `__data.json` with `{}`,
  no AbortSignal (`client.js:3122–3123`), and its stream reader continues
  consuming deferred chunks independently of the navigation token. Neither
  changed page has navigation cancellation wiring. Additionally adapter-node's
  `getRequest` aborts only an incoming request that closes before its `end`
  (`src/exports/node/index.js:138–147`); it does not turn a later streamed
  response disconnect into this request abort.
- Evidence: test `ui-pages.test.ts:937` manually constructs a Request with an
  AbortController and explicitly calls `leave.abort()`. It proves downstream
  forwarding, not that browser navigation supplies that abort. A focused
  executable check against installed Kit's actual `getRequest` created a Node
  HTTP server, completed a GET and wrote an initial response chunk, then
  disconnected the client while the response remained open. Through
  `mise exec -- node --input-type=module` (stdin script, UI working directory),
  output was `{"afterClientDisconnect":{"requestSignalAborted":false}}`;
  the matching assertion passed (exit 0). No application source was changed.
- Consequence: obsolete aggregates continue consuming query resources after
  leaving or replacing the view; together with INV-2 this can accumulate work.
- Smallest correction boundary: connect support work to a real host-owned
  cancellation lifecycle across browser/BFF/transport, reusing existing
  AbortSignal-capable loader/executor paths. Account for both supersession
  and leaving the page; preserve credential isolation and typed failures.
  Add a production-BFF browser test that holds support, performs actual
  navigation, and observes the upstream read being cancelled. A manual abort
  of a synthetic loader event cannot close this finding.

## Verification and process limits

No Cargo, broad UI suite, benchmark or full browser journey was run by this
specialist. Root independently owns focused UI test execution; the candidate
task's recorded production-browser results are inspected evidence, not fresh
runs. Only the adapter boundary self-check above was executed here. The three
findings are distinct absent lifecycle promises; helpers/signatures and exact
control flow remain implementation-owned.

Streamed failure and remembered-section tests were written with their fixes;
this does not establish test-first RED. The task records that process limit;
the passing assertions do not erase it. No historical RED reconstruction or
production churn is requested.
