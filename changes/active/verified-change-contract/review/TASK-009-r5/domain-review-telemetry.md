# Telemetry and OpenTelemetry domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `11eb8ed2b64b70903f171335723dcc549b463352`
- Approved authority: `SPEC-verified-change-contract` revision 46
- Task packet: original TASK-009 plus R1, R2, R3, and the human-approved R4E
  remediation that supersedes TASK-009-R4
- Scope: the complete cumulative base-to-candidate telemetry and observation-
  identity boundary. The documented attach-without-detach decision and its
  improper-nesting residual were treated as approved authority, not defects.

There is no `.codegraph/` index in the repository, so direct repository search
and source inspection were used. `HEAD` matched the candidate before this
report was written.

## Boundary and authority coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Run identity and exact Card selection | Revision 46 REQ-123/REQ-151/AC-032; `changes/active/verified-change-contract/architecture/logic/run_api.md`; `crates/shared/wyrd-client/src/state.rs`; `crates/shared/wyrd-client/src/observe/mod.rs`; Python projection and tests | PASS — shared Rust mints one UUIDv7 Run ID, resolves initial and sibling Cards from the hydrated graph, and supplies each immutable view's exact CardRef. Unknown aliases fail before Python scope entry. |
| Import-time OTel binding and private key | REQ-151; R4E required behavior 1; `sdks/wyrd-sdk-python/python/wyrd/otel.py:18-23,192-203`; pinned OTel context implementation | PASS — when OTel imports, one UUID-backed private key is minted at module import. When imports fail, both bindings and the key are absent and Run correlation becomes a no-op without making OTel mandatory. |
| Token-free execution-local stack | REQ-151; revision-46 rationale; `run_api.md:140-228`; `otel.py:272-313`; PyO3 entry/exit at `src/observe/mod.rs:195-249` | PASS — entry and matching exit each attach one new OTel context value, no token or detach call exists, no scope state is stored on `PyRun`, and mismatch/empty/failing exit changes nothing. Nested, await, copied-task, distinct-view, and identical-Run task cases cover the intended execution-local behavior. The approved improper-nesting residual was not reported. |
| Parent-context span propagation | REQ-151; telemetry-observations context propagation; `otel.py:206-236`; pinned `opentelemetry-sdk` 1.42.1 `Span.start` and multi-processor source | PASS — `on_start` reads `_SCOPE_KEY` from `parent_context` (or the current context when `None`) and writes the innermost pair. OTel 1.42.1 passes the selected parent context to processors, and the duck-typed processor implements the SDK lifecycle hooks, including `_on_ending`. |
| Active span non-overwrite and in-scope replacement | Revision 46 REQ-151 and history; `otel.py:272-297`; focused span tests | PASS — entry stamps an active recording span only when its readable attributes do not already contain `wyrd.card_ref`; a nested scope therefore leaves an outer-correlated active span alone. The processor still replaces conflicting initial values on a newly started in-scope span, as required. |
| Provider outcome caching, idempotency, and weak lifetime | REQ-151; R4E required behavior 6; `otel.py:200-268`; provider tests | **FAIL** — the weak, lock-protected cache correctly attempts at most once, preserves provider lifetime, and reports success/ordinary failure consistently. However, the single shared processor handed to an accept-then-raise provider remains active even though the cached terminal outcome is `False`, so the failed provider can still enrich later spans. See `TEL-R5-001`. |
| Optional-boundary failure containment | REQ-151/AC-032; PyO3 delegates; `otel.py`; focused failure tests | PASS except for the failed-provider outcome mismatch above — import absence, unsupported providers, registration exceptions, active-span/processor failures, entry attach failure, exit context-update failure, and mismatched exit do not escape or block explicit observations. User exceptions remain unsuppressed; Card lookup, validation, authorization, and Wyrd writes remain strict. |
| Exact record attributes and server-owned managed identity | `architecture/wyrd-design.md` observation identity; `architecture/references/domain/telemetry-observations.md`; `otel.py:192-219`; `vala-bifrost-redux/src/tables/signal.rs:270-397`; trace projection and Scribe UID resolution | PASS — the client writes only `wyrd.card_ref` and `wyrd.run_id`. Trace projection takes the final record-level string values, checks the CardRef against signed Card scope, and preserves the lossless attributes; Scribe stamps the signed member's Card UID and authenticated publisher rather than trusting a client UID, tenant, principal, or request identity. |
| Persisted join proof and lifecycle barriers | AC-032; `tests/integration/state/test_observe_journey.py:365-474,477-563`; OTLP/HTTP dev dependency and lockfile | PASS — the existing Service journey uses the stock OTel 1.42.1 SDK and OTLP/HTTP exporter against authenticated `/v1/traces`, emits a custom row and Eval row in the same selected-Agent Run scope, and queries persisted span/run/Card UID/publisher, custom-run, and Eval trace/span joins. Provider flush, state shutdown, and server publication are explicit after context exit. |
| Dependency and non-goal boundary | AGENTS dependency ownership; REQ-151; TASK-009/R4E non-goals; `pyproject.toml`; `uv.lock` | PASS — `opentelemetry-api` remains an optional production extra; the OTLP/HTTP exporter is dev-only and lock-pinned with API/SDK to 1.42.1. No wrapper span, server Run, second queue/exporter, implicit flush/shutdown, ambient log/metric promise, or client-managed durable identity was added. |

## Producer-to-consumer trace

1. Shared Rust resolves the selected Card and constructs a Run carrying that
   exact `CardRef` and one invocation `RunId`.
2. `PyRun.__enter__` passes only those two texts to `_enter_run`. The Python
   owner attempts global-provider registration, attaches the pair to the OTel
   execution context stack, and conditionally stamps the already-active span.
3. OTel calls `_RunCorrelationProcessor.on_start` with the span's parent
   context. The processor reads the innermost pair and writes the two exact
   Bifrost attributes. `PyRun.__exit__` passes the same pair and pops only a
   matching top by another attach.
4. Stock OTLP/HTTP export sends the record attributes to the authenticated
   trace endpoint. Trace projection validates their types and signed Card
   scope, Scribe resolves the authoritative Card UID and stamps publisher
   identity, and Bifrost persists the opaque Run ID.
5. The journey proves spans, the caller-owned row, and Eval evidence join only
   after the caller separately flushes the provider, shuts down the state, and
   waits for server publication.

The failing provider path diverges at step 2: the cache stores `False`, then
the foreign provider retains `_PROCESSOR` and raises. The exception returns
`False`, but no state disables the already-retained processor. At step 3 that
provider can still call the shared processor, which has no provider-specific
outcome check and therefore enriches normally.

## Material finding

### `TEL-R5-001` — failed accept-then-raise registration can still enrich spans

- **Classification:** INCORRECT
- **Violated obligation:** REQ-151 says a provider whose registration fails,
  including one that raises after accepting the processor, is never retried and
  “simply gets no enrichment.” R4E also requires a terminal cached `False`
  outcome for that provider while preserving fail-open behavior.
- **Exact location:**
  `sdks/wyrd-sdk-python/python/wyrd/otel.py:206-220,236,239-269`, exercised but
  not closed by
  `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:414-436`.
- **Evidence:** `install_run_correlation` caches `False` before the foreign call
  and passes the global stateless `_PROCESSOR`. The test provider retains that
  processor and then raises; the test asserts one retained processor and a
  `False` result, but never starts a span through it. The retained processor's
  `on_start` reads the current Run stack and stamps attributes unconditionally;
  it cannot distinguish the failed provider outcome. Thus a later span inside
  a Run scope is enriched even though every install call reports terminal
  failure.
- **Observable consequence:** a framework can receive ambient Wyrd Card/Run
  attributes after `install_run_correlation(provider)` reported `False` and
  after the approved contract classified that provider as having no
  enrichment. This makes the public Boolean and terminal failure semantics
  untruthful and can export correlation the caller reasonably treated as
  unavailable.
- **Required testable correction:** keep the current weak, at-most-once outcome
  owner, but ensure a processor retained by a registration call that does not
  return normally remains inert. Activate enrichment only after that provider's
  `add_span_processor` returns successfully; do not retry, introspect the
  provider, add a wrapper/provider registry, or alter successful providers.
  Extend the existing accept-then-raise case to start a span through its
  retained processor while a Run scope is active and assert no Wyrd pair; keep
  the healthy global/private exact-pair tests and weak-lifetime behavior.

## Prior-finding closure in this domain

| Finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` | Actual registration, API-only, import, attach, enrichment, and exit-update failures reach an ordinary explicit Drift error without escaping. | CLOSED |
| `FIND-TASK-009-2` | Native Run subject documentation covers root, initially selected, and sibling-selected views. | CLOSED |
| `FIND-TASK-009-3` / `FIND-TASK-009-4` | Revision 46 supersedes token/detach restoration and lazy-key bookkeeping with one import-time key and attach-only context stack. | SUPERSEDED AND CLOSED |
| `FIND-TASK-009-5` | PyO3 `__exit__` names/defaults agree with the public generated declaration and always return `False`. | CLOSED |
| `FIND-TASK-009-6` | The new Rust selection tests carry their required panic documentation. | CLOSED |
| `FIND-TASK-009-7` | Two coordinated tasks enter the identical immutable Run and prove independent exit and post-exit clearing. | CLOSED |
| `FIND-TASK-009-8` | Registration is now attempted at most once and an accept-then-raise provider retains only one processor. | CLOSED for the duplicate-registration defect; `TEL-R5-001` is the distinct revision-46 no-enrichment outcome gap. |

## Verification limits

- Per the orchestrator's parallel-discovery constraint, this reviewer ran no
  test or build command. Source, pinned OTel 1.42.1 implementation, test bodies,
  lock/manifests, and the successful commands recorded in R4E implementation
  evidence were inspected.
- The candidate records 37 passing focused Python surface tests, the unchanged
  authenticated persisted journey, Python unit/integration/type/codegen and
  boundary lanes, Rust format/lints, and `git diff --check`. This pass did not
  independently reproduce those results.
- Raw-thread propagation, ambient log/metric enrichment, and the documented
  attach-without-detach improper-nesting residual are approved exclusions, not
  verification gaps.

## Overall result

**FAIL**

The normal, nested, async, failure-containment, authenticated ingest, and
persisted-join paths satisfy the telemetry boundary, and R4E closes duplicate
processor registration. The independently traced accept-then-raise path does
not satisfy revision 46's terminal failed-provider no-enrichment contract.
