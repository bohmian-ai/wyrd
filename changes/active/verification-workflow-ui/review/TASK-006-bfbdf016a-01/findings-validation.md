# TASK-006 independent findings validation

Subject: base `e6d7d2681e588f118ec7a8e9bbcfab24757a654b` → candidate `bfbdf016a5487cac17e8dc6fbb73cc8ae17a147f`. First review. Approved verification-workflow-ui revision 8 and authorization-model revision 9; original `tasks/TASK-006-waiting-and-approval.md`. Reviewed both discovery reports independently against source, consumers, actual assertions and the approved requirements. No new blocking finding was introduced by validation.

Paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui/` unless qualified. The reconciled coverage matrix is in `verdict.md`.

## FIND-TASK-006-1 — INCORRECT — unresolved decision can be submitted again

**Disposition:** B1 CONFIRMED.

**Authority:** spec REQ-008 and AC-004, specifically “Read back an ambiguous submission before explicit retry”; original scenario 4 requires the same ordering.

**Location:** `src/routes/t/[tenantKey]/verify/executions/[executionId]/+page.svelte:64–90`; `packages/ui/src/lib/components/VerificationExecution.svelte:117–127,273–280`; test `src/lib/ui-pages.test.ts:783–803`.

**Conditions and trigger:** a pending exact occurrence is submitted; the response is lost and the following `invalidateAll()` also rejects. The page correctly reports an unknown outcome and asks for refresh, but sets `approval.decide` to `error`. The component disables decisions only for pending/unavailable action state, and the host guard checks only pending. Both permit a new POST before any successful authoritative readback. The reason or decision can also be changed while the outcome is unresolved.

**Expected / actual:** successful occurrence readback must precede explicit retry; actual controls and host dispatch allow immediate resubmission. The existing page test explicitly asserts Approve enabled after failed readback and sends the next request without an intervening successful refresh. I independently reran that exact test successfully, confirming the asserted behavior is executable. Backend first-decision finality still holds: this finding concerns the promised recovery ordering, not duplicate durable execution or an authorization bypass.

**Smallest correction:** keep the unresolved occurrence non-submittable in the existing host action/refresh owner until authoritative readback succeeds. Preserve a usable refresh path and readable evidence. A successful readback either displays the recorded decision or permits explicit retry when still pending. Do not add automatic write retries, another execution, or a new backend state. Update the existing page assertion; prove the same ordering with controlled reply/read failure around a real decision through the existing integration owner.

## FIND-TASK-006-2 — EVIDENCE_GAP — running peer is demonstrated only by a fixture

**Disposition:** B2 REVISED and retained with narrower closure scope.

**Authority:** spec REQ-008 includes running peers; AC-004 requires a blocked branch with a running peer. Original scenario 1 expressly starts actual public verification creating a missing input, pending approval and running peer. Root AGENTS Testing and `TESTING.md:8–19` disallow a unit-only substitute for user-visible journey behavior.

**Location:** original task scenario 1 and its disclosed limit; `src/lib/ui-library.test.ts:3213`; `src/lib/ui-library.integration.test.ts:2485`; `fixtures/cards/approvals/repeated.yaml`.

**Conditions and missing proof:** the component fixture provides `audit started` alongside deferred input and approval. The actual public Verifier used by the server and browser tests has two approval branches and a deferred `ci` requirement, with all runnable work parked. Neither its authored steps nor the real assertions establish an in-progress peer. The task accurately discloses this. Inspection establishes neither an implementation defect nor any need to change server behavior.

**Consequence:** the required connected state—real running work alongside the two different wait reasons and no final verdict—has no execution evidence. Passing parked-work journeys and a synthetic running row do not close this assigned scenario.

**Smallest correction:** extend one existing focused real-server shared UI journey so public verification actually reaches a pending approval and missing input while a controlled peer remains in progress. Consume the actual status/progress through the existing loader/component and assert the distinct states and absent verdict. Additional definition-denial composition, approval submission during that interval, a new test framework, and backend production changes are not required by this finding. Existing proof already covers definition denial and decisions separately.

## FIND-TASK-006-3 — VIOLATION — approval forwarding launders incoming Origin

**Disposition:** DOM-1 CONFIRMED as an explicit Origin-boundary violation, with exploit claims limited to the demonstrated evidence.

**Authority:** `architecture/wyrd-security-posture.md:708–712` requires every cookie-authenticated mutation to refuse missing/mismatched trusted public Origin and expressly forbids Node replacing either with trusted authority. Original task Inputs/owners requires preserving this Origin/CSRF/session boundary.

**Location:** new `src/routes/t/[tenantKey]/verify/executions/[executionId]/approvals/[approvalId]/+server.ts:16–24`; shared `src/lib/server/auth/browser-sessions.ts:46–52,168–173`; `src/hooks.server.ts:23–34`.

**Conditions and trigger:** a JSON decision POST includes a valid tenant browser cookie but missing or mismatched Origin. The route obtains the session and parses/forwards the body without checking the incoming Origin. Hooks select the tenant session but add no mutation-origin check. `BrowserSession.api` calls shared `call`, which constructs `origin: url.origin` from the BFF request URL. Thus the protected backend receives a trusted Origin regardless of the incoming value.

**Expected / actual and evidence:** reject before the mutation is forwarded; actual route-to-transport source trace replaces the header. The specialist additionally exercised the production Node BFF against a controlled upstream: missing and hostile JSON Origins each returned 200 and reached the sentinel decision operation with the trusted BFF Origin. This is forwarding evidence, not evidence that the sentinel persisted a real decision. Independent validation traced the same route, hooks and shared session transport against the exact architecture text.

**Materiality and limits:** a new permission-sensitive cookie mutation violates a mandatory trust boundary. This does not establish exploitable browser CSRF: ordinary hostile JSON fetches face CORS preflight; the specialist's no-content-type case returned 400 and text/plain is covered by SvelteKit's form-origin check. Those supplemental protections do not satisfy the explicit missing/mismatched-Origin rule. The shared weakness predates this route; the finding concerns this newly shipped mutation's use of it.

**Smallest correction:** enforce the incoming exact public-Origin rule at the existing shared authenticated mutation request/forwarding boundary before trusted forwarding headers are generated. Cover the approval route and affected sibling mutation callers without copying permission logic or redesigning sessions. Preserve read-only requests, bound callback exceptions, tenant cookie selection, backend permission enforcement and matching-Origin decisions. Add production-BFF HTTP proof that missing/mismatched Origin never reaches the approval operation and matching Origin does.

## Validation evidence and limits

- Independently executed `mise exec -- pnpm exec vitest run src/lib/ui-pages.test.ts -t 'TASK-006: an execution page decides one exact approval occurrence once and reads it back'` from the UI directory: exit 0, 1 passed / 22 filtered, confirming the current incorrect retry-order assertion.
- Read the actual component, graph-state, schema, host, real-server and browser assertions. The specialist's fresh managed-Postgres library test passed 1 / 23 filtered and supplies credible permission, identity, replay, expiry and continuation evidence.
- Root fresh `ui:check` passed 117/117 and both svelte-checks; `codegen:check` and range whitespace check passed. These do not negate the retained findings.
- Accepted/deferred mock differences remain excluded exactly as instructed. No independent screenshot recapture; the user supplies the comparison and ruling. No unrelated lifecycle-suite or benchmark requirement was added.
- Behavior and domain expertise are sufficient for this bounded UI change: shared state, failed readback and exact IDs receive primary plus validation coverage; current authorization/session/tenancy receives a separate specialist. No production SQL, storage, lifecycle engine or abstraction redesign occurs, so additional system/invariant/repository/maintainer specialists would not fill a material uncovered boundary.
- TDD: recorded missing-label/emit/host-dispatch RED→GREEN evidence supports compliance; no contrary history is established. Within-commit execution order cannot independently be reconstructed. Preserve this proof limit separately from behavioral FIX_REQUIRED.

No rejected proposal is retained as optional remediation. No source or test implementation was edited.
