# Telemetry and observation-identity domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `d01307b8c37b47488115743c94b624a29d66e4be`
- Reviewed range: the complete cumulative base-to-candidate diff, including the
  original TASK-009 implementation and the R1/R2 remediations.
- Candidate immutability: `git rev-parse HEAD` returned the candidate before
  review and again after report preparation.

## Reviewed boundary

Python `Run` selection and synchronous scope entry/exit; execution-local
OpenTelemetry context; active-span enrichment; global and private provider
processor registration; optional-integration failure containment; OTLP/HTTP
export; record-level `wyrd.card_ref` and `wyrd.run_id` extraction; signed Card
scope authorization; Scribe Card UID resolution and publisher stamping; and the
persisted trace/custom/Eval joins required by AC-032.

## Authority and source coverage

| Authority or source | Coverage |
|---|---|
| `AGENTS.md`; `architecture/agent-rules.md`; spec-driven development and maintainer-style references | Review immutability, SDK ownership, optional Python boundary, exact proof obligations, and user-journey priority |
| Approved spec revision 45: REQ-123, REQ-151, INV-007, INV-012, AC-032 | Exact Run/Card identity, fail-open span correlation, server-owned managed identity, and required persisted proof |
| Original TASK-009 and R1/R2 remediation tasks | Original scenarios plus prior failure-containment, detach restoration, first-use serialization, and boundary-parity closures |
| `changes/active/verified-change-contract/architecture/logic/run_api.md` | One client invocation ID, immutable Card views, explicit observation correlation, and existing Bifrost path |
| `architecture/wyrd-design.md` observation identity; `architecture/bifrost-design.md` table/row identity; `architecture/references/domain/telemetry-observations.md` | Exact OTLP attribute names, per-record Card authorization, opaque Run ID, and server-derived tenant/publisher/Card UID |
| `crates/shared/wyrd-client/src/state.rs`; `crates/shared/wyrd-client/src/observe/mod.rs` | Local initial Card selection, UUIDv7 Run identity, immutable sibling views, and explicit observation correlation |
| `sdks/wyrd-sdk-python/src/state/mod.rs`; `src/observe/mod.rs`; `python/wyrd/otel.py` | PyO3 projection, active Eval span IDs, context-manager calls, processor registration, context restoration, and optional failure containment |
| `crates/vala/vala-bifrost-redux/src/tables/signal.rs`; trace projection; `scribe/execution_lanes.rs` | Final record-attribute extraction, signed-scope authorization, exact signed UID resolution, and server stamping of `principal_id` |
| Python unit and integration tests | Healthy, nested, failure, provider, concurrency, OTLP/HTTP, and persisted join evidence |

## End-to-end assessment

The runtime path preserves the approved identity boundary. Shared Rust owns the
one Run ID and exact hydrated `CardRef`; Python injects only their text values.
`_RunCorrelationProcessor` reads execution-local parent context without starting
a wrapper span or owning provider/exporter lifecycle. OTLP trace projection
extracts the final record-level attributes and checks Card identity against the
authenticated signed scope. Scribe independently resolves only the signed scope
member's UID and stamps the authenticated principal, so a client UID is not
trusted and no tenant, principal, Card UID, or request identity moved into the
SDK.

The real journey uses the stock OTLP/HTTP exporter against authenticated
`POST /v1/traces`, then separately flushes the provider, shuts down the state,
and publishes before querying. It proves both framework spans retain the exact
asserted CardRef in their lossless payload and persist the expected Run ID,
server-resolved Card UID, and one non-null publisher. Its custom row joins those
spans by Run ID, and its Eval row joins the exact active tool span by trace/span
ID while retaining the same Run and Card identity. No second queue, wrapper
span, server Run, required OpenTelemetry dependency, or client-managed durable
identity was introduced.

R1's runtime defects are closed in the existing owner: lazy key creation is
serialized under the existing registration lock, and exit records/restores the
prior execution-local value when detach raises or silently fails. R2 proves an
actual raising registration attempt cannot block explicit Drift emission and
aligns the public `__exit__` runtime/stub boundary.

## Material finding

### `TEL-R3-001` — AC-032's same-Run concurrent-task proof is absent

- **Classification:** MISSING
- **Violated obligation:** AC-032 requires async evidence for “concurrent tasks
  using the same immutable Run”; REQ-151 specifically prohibits storing one
  shared attach token on that immutable Run.
- **Exact location:**
  `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:315-345`,
  `test_scope_survives_await_and_isolates_concurrent_tasks`.
- **Evidence:** the two concurrent tasks enter `views["model"]` and
  `views["backup"]`. Those are two distinct `PyRun` objects returned by separate
  `for_card` calls. The test therefore remains green for an implementation that
  incorrectly stores one token on each Run object; it never has two tasks enter
  the identical `run` instance. No other Python test concurrently enters the
  same Run object. The production `ContextVar` implementation appears correct,
  but source plausibility cannot replace the proof class explicitly required by
  AC-032.
- **Observable consequence:** a future regression to per-Run token storage
  could make one task's exit detach another task's scope, exporting an
  uncorrelated or stale-correlated span, while every current test still passes.
  Such a span would fail the required trace/custom/Eval join or be attributed to
  the wrong scope.
- **Required testable correction:** in the existing focused Python test file,
  add or adapt one async case so two coordinated concurrent tasks both enter the
  exact same `run` object, their exits interleave, and each task still emits a
  correctly correlated span after the other task exits; assert a span after
  both scopes has no correlation. Keep the current sibling-Card isolation case.
  No production change, new harness, dependency, or provider abstraction is
  indicated.

## Verification and limits

- Independently ran
  `mise exec -- uv run --project sdks/wyrd-sdk-python python -m pytest -q sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py`:
  **34 passed**.
- Source inspection corroborates the recorded successful authenticated journey
  and broad gates in the task/remediation evidence, but this domain pass did not
  rerun the Postgres-backed journey or the full multi-language gate set.
- The integration journey is credible for persisted identity and join behavior;
  the blocking gap is the explicitly required same-object concurrent proof, not
  a demonstrated production runtime defect.

## Overall result

**FAIL** — telemetry identity, authorization, managed identity, lifecycle, and
persisted joins are implemented coherently, but AC-032's explicit concurrent
use of the same immutable Run lacks the required focused evidence.
