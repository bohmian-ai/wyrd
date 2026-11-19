---
id: TASK-004B-R2
kind: remediation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 6
supporting_spec: SPEC-wyrd-ui-main-integration
supporting_spec_revision: 28
requirements: [REQ-001, REQ-002, REQ-003, REQ-009, REQ-014, REQ-018, AC-001, AC-008]
parent_task: TASK-004B
depends_on: [TASK-004B-R1]
remediates: [FIND-TASK-004B-4, FIND-TASK-004B-7]
---

# Complete cancellation and preserve one validation per refresh

An engineer changing filters or leaving Observe/Verify must stop obsolete
supporting work, including requests that have not returned headers and Verify's
Card-name read. An Observe refresh must validate its custom predicate once.
While new supporting data loads, the page must identify that pending state
without disguising old counts as results for the new population.

This is the separate follow-on remediation from R1 re-review. Preserve R1 and
its evidence. FIND-1, FIND-2, FIND-3, FIND-5 and FIND-6 are closed within their
recorded evidence; do not restart them without an affected-path reason.

## Authority and inputs

- Approved SPEC-verification-workflow-ui revision 6; supporting UI revision 28.
- Original task: `../../tasks/TASK-004B-ui-query-consolidation.md`.
- Prior remediation: `../TASK-004B-2026-11-18-01/TASK-004B-R1-query-lifecycle-and-proof.md`,
  including its maintainer-approved Revision 1 browser acceptance.
- Original reviewed candidate: `7e2b3f6fab8c81fb85769eee37d0c7fd6b633146`.
- R1 reviewed HEAD: `79a4ef66190e572c47aa93351e7362329f7bc73f` plus the
  uncommitted tests/evidence identified and fingerprinted in `verdict.md`.
- Independent diagnoses: `findings-validation.md` and `lifecycle-validation.md`.

Record the actual implementation baseline before starting. Do not discard the
uncommitted R1 tests or assume HEAD contains them. Git actions remain caller-owned.
The original scenario 5 and approved loading acceptance settle behavior; this
task adds no new product promise.

## Diagnoses and smallest correction boundaries

**FIND-TASK-004B-4, still open:** `packages/ui/src/lib/node.ts:63` waits for
`bifrost.stream` before registering its abort listener. The same executor's
Card branch at lines 56–59 ignores cancellation. Verify includes that lookup
in `src/lib/server/support.ts:35–39`. Existing browser proof forwards SQL
headers before holding bodies, so it covers only the already-working phase.

Complete cancellation through the existing host/executor/transport owners for
pending SQL startup and Card lookup, preserving ordinary stream cancellation,
typed failures, connection/resource cleanup, host credentials and independent
successful content. A server deadline or ignored eventual result does not stop
obsolete work. Do not add a separate query API or engine control. If an actual
material contract decision is required, surface it under the existing spec
workflow rather than weakening this requirement or inventing a new boundary.

**FIND-TASK-004B-7, fix-induced regression:** Observe's list and support route
construct separate LoadContexts. `loadRecordTable` and `loadFacetRail` therefore
each call `predicateFor` for the same nonblank custom predicate; the WeakMap
only shares within a context. SSR still shares correctly. Verify does not
support this custom-SQL input and is outside FIND-7.

Restore one applicable validation across the private Observe refresh composition
while keeping standalone loaders/endpoints safe. Reuse the existing predicate
validation owner and preserve tenant/table authorization. Never accept a browser
claim of prior validation or introduce a cross-request query-result cache.
Keep the accepted list-only invalidation and population read counts.

**Outstanding approved Revision 1 acceptance:** Streamed preserves old support
while its successor runs but exposes no pending indication; Observe omits its
overview entirely while initial support is absent. Make loading explicit using
the existing section/render owners and house UI patterns. Previous content may
remain for continuity and Back restoration, but the reader must be able to tell
it is pending replacement. Keep the list usable, loading honest, errors explicit
and no false zeros. This is completion of approved behavior, not a new finding
ID or permission to redesign the dashboard.

The separately recorded dark full-page background difference still needs a
maintainer ruling. Do not count “ruling requested” as approval or invent a
styling fix outside the approved scope. Prepare concrete before/after evidence
for the stated visual decisions; apply any further ruling only when given.

## Ordered observable closure

Use existing browser/BFF integration owners and conventional assertions.
Record actual selectors before changes; capture behavior-failing RED where
applicable, without reconstructing historical RED for R1.

| Finding or approved acceptance | Scenario and pass criteria | Existing proof owner |
|---|---|---|
| FIND-4: startup | Hold a real supporting SQL request before response headers. After the primary list renders, supersede its population or leave the page. Observe the upstream request/query cancel and release its owned work before the normal query deadline. Preserve the existing post-header abort case and no late overwrite. | `ui-pages.integration.test.ts`, production-BFF/executor/transport; focused lower-tier cancellation assertions where needed |
| FIND-4: Card names | Hold Verify's actual Card-name request while independent list/query results can arrive; perform real supersession/departure. Observe that upstream lookup cancel/close rather than merely suppressing its eventual display. Preserve bounded Card lookup and label fallback behavior. | Existing Verify dashboard journey and host executor seams |
| FIND-7 | With nonblank valid custom SQL, exercise the combined Observe list-plus-support client refresh and assert exactly one applicable validation. Prove standalone list/support requests remain safe and invalid/denied predicates issue no dependent reads. List-only choices still issue no support refresh. | `ui-pages.test.ts` request assertions plus focused production-BFF browser/integration path |
| Explicit loading | Hold initial and replacement support on Observe and Verify. Initial sections identify loading; replacement facets/charts distinguish retained old content from current results. The list remains usable. Support success clears pending state; failure shows an explicit section error without false counts or erasing the list. Preserve Back height/scroll. | Existing component tests and real browser lifecycle journeys |
| Visual acceptance | Capture loading, settled, error and Back-restored states in light/dark at the recorded viewport; compare against approved appearance and record actual maintainer rulings for remaining differences, including the reported background issue. | Existing capture owner via `WYRD_UI_CAPTURE_DIR` |

For startup/Card cancellation, reuse the existing test-local transport fixture
where its boundary fits, extending it only as needed to hold the previously
untested stages. Do not substitute synthetic Request aborts, post-header-only
holds, screenshots or a deadline timeout for the required upstream observation.
No synthetic host load, broad harness or new diagnostic API is required.

## Execution, evidence and preservation

Use the existing mise runners recorded in R1. Resolve focused selectors from
the actual owners; run the affected UI unit/component and browser integration
selectors, `ui:check`, and only the SDK/transport checks required by the real
write set. If Rust changes, run targeted owner tests and format/lints; serialize
Cargo commands. Apply applicable hot-path evidence rules if production hot-path
behavior changes. The earlier hosted benchmark exception is not a blanket waiver.
Do not rerun full unrelated journey matrices merely to close these bounded gaps.

Record exact commands/results, tested commit plus relevant uncommitted state,
per-operation cancellation observations, validation counts, and capture locations.
Use an existing configured capture directory; never hardcode a machine/session
path into source. Preserve screenshots needed for review in a documented stable
artifact location before temporary files disappear. Artifact retention does not
authorize a new external upload or publishing action.

Prior review reports and R1 evidence remain immutable history. Append new R2
evidence here and preserve all original and R1 TDD violations separately from
behavioral closure. Do not mark incomplete behavior PASS because a limitation is
documented. No migration, dependency, server storage/auth change, generalized
cache, UI redesign, or reopened analytical/EXPLAIN work is requested. Do not
commit, push or launch review without the maintainer's instruction.

## R2 evidence

**Baseline.** HEAD `79a4ef661` plus the uncommitted R1 tests and evidence
fingerprinted in `verdict.md`, committed unchanged as `41f2419c8` before any
R2 change.

### FIND-TASK-004B-7 — maintainer ruling (2026-11-19)

A client Observe refresh is two host requests: the list, then its support.
Each validates its own custom `where_sql` before any dependent read; neither
trusts the other or the browser, and no validation outlives its request. The
maintainer accepted this as the intended boundary: one validation per host
request (SSR, one request, still validates once). The extra cost is one
`bifrost.validate_predicate` round trip per filtered client population
refresh, with no PostgreSQL statement. Sharing it would need a browser claim,
a sealed host attestation or a cross-request memo, each a new trust
mechanism that the task excludes. No code change. This ruling replaces the
"exactly one applicable validation" pass criterion for the client refresh;
the standalone, refused-predicate and list-only rows still need proof.

### Closure

Tested commit `b5f8a322a` (R2 commits `450ab04fe`, `f968142b0`, `14ae3b9fe`,
`33fd49980`, `b5f8a322a` over baseline `41f2419c8`); no relevant uncommitted
state.

| Row | Status | Proof |
|---|---|---|
| FIND-4: startup | PASS | `supportGate` gained `hold('head')`, which keeps the server's response head back. Observe journey (`Observe reads support once per population…`): with the list rendered, Zoom in twice, then leave for Verify; every held grouped SQL request `ended` `abandoned` before any head (3 + 3). The existing post-header hold, its abandonment and the no-late-overwrite assertions are unchanged and pass. Owner fix: TS SDK `Bifrost.stream(..., { signal })` with native `NativeAbort`, which drops the pending Rust future and so the HTTP request; `node.ts` passes its signal. Lower tier: `sdks/wyrd-sdk-ts/wyrd/tests/unit/abort-pending.test.ts` holds `/v1/query` on a silent loopback server and observes its socket close on abort. |
| FIND-4: Card names | PASS | Verify dashboard journey: with Verify's real `cards.list` (`GET /v1/cards`) and discovery SQL both held before headers, filter `failed` then `completed`, then leave for Observe; each pair `ended` `abandoned` (`['cards','sql']` × 2). Later steps keep label fallback and the bounded 200-Verifier lookup. SDK: `Cards.list(request, { signal })`, same unit test; an already-aborted signal sends nothing. |
| FIND-7 | PASS under ruling | Ruling above. `ui-pages.test.ts` `each Observe request validates its custom predicate once before reading…`: with valid nonblank `where_sql`, the client list request and the support request each issue exactly one `bifrost.validate_predicate`, first; an invalid (`400`) or denied (`403`) predicate stops both with only the validation call. Standalone loaders keep their own validation (`observe signal planning and interactions`). The list-only no-support-refresh test (`a client navigation reads only its list…`) is unchanged and passes. |
| Explicit loading | PASS | `Streamed.pending` is true while the newest read is for a population other than the one shown, or Back is showing a remembered value; it is false for a Live reread of the shown population (`streamed.test.ts`, 2 new tests). `src/lib/Updating.svelte` dims retained facets and charts, sets `aria-busy`, and overlays a `role=status` "Updating" label without changing height. The list bar counts (Observe) and total (Verify) are marked too, and Verify's list neither counts nor pages against the old total. Observe's overview and timeline show loading or error EmptyStates until their first support. Browser assertions: Updating on facets and overview/charts while held, list usable, cleared after release, initial "Reading the overview", per-chart errors with no counts on failure, nothing busy during a held Live reread, and Back height and scroll unchanged. |
| Visual acceptance | Evidence prepared; rulings pending | See captures below. |

**RED observed:** browser `expected [0,0] to deeply equal [1,1]` (no Updating marker) on both boards; Observe startup outcome `['open','open','open']` and Verify `['open','open']` before the SDK signal; counts `aria-busy` `null`; Verify `Show N more` count `1`; `pending` `undefined`, then `true` for a Live reread. SDK mutation: with abort forwarding disabled, both abort tests time out.

**Removed proof:** the `Bifrost.running()` "server no longer runs it" check was vacuous, because held queries were already absent from `running()` while held. Only the closed upstream request is claimed, not server-side work release.

**Commands** (Cargo serialized):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 WYRD_UI_CAPTURE_DIR=<captures> pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t 'Observe reads support once per population|the Verify dashboard filters|authenticated traces and spans|authenticated logs investigation|authenticated metrics investigation|observe boards match approved canvas'   # 6 passed
pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/streamed.test.ts src/lib/ui-pages.test.ts   # 25 passed
mise run ui:check                                   # 0 errors; 95 tests
mise exec -- cargo fmt -p wyrd-sdk-ts; mise exec -- cargo clippy --locked -p wyrd-sdk-ts --all-targets -- -D warnings   # clean
mise run ts:napi:check; mise run ts:format:check; mise run ts:lints; mise run ts:typecheck   # pass
mise run ts:test:unit                               # 91 passed
git diff --check                                    # clean
```

**Hot path:** none touched. The change is UI host, TS SDK and native binding client-side cancellation; no server, Oracle, Scribe or PostgreSQL wait is added or moved.

**Captures:** `captures/` beside this file (untracked, 1440×900, full page,
`lifecycle-{observe,verify}-{loading,updating,settled,support-error,back}-{light,dark}.png`,
20 files), written via `WYRD_UI_CAPTURE_DIR` on the command line. Rulings
requested, none given yet:
1. The Updating treatment: 50% dim plus an overlaid dashed "● Updating" label in the house EmptyState vocabulary. On the facet rail the label overlaps the "Click a value to add it to WHERE" hint.
2. Initial overview and timeline loading as a dashed EmptyState inside the panel.
3. The dark full-page background still ends at the 900 px viewport (`lifecycle-*-dark.png`); existing styling, not changed here.

**TDD:** the original and R1 violations stand as recorded, including R1's malformed-JSON test-after-fix. R2: the FIND-4 SDK unit test was written after the native/TS signal. Its strength is shown by mutation, but test-first was not followed (the browser RED came first). The FIND-7 host test is characterization of already-correct behavior. Loading, counts and Live-pending were test-first.
