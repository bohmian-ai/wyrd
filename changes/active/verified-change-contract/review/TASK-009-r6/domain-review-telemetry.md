# Telemetry Domain Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `1a4bbff5a26a1462d0f509c4595d52d08fbd25ae`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 46
- Task chain: TASK-009, TASK-009-R1, TASK-009-R2, TASK-009-R3, TASK-009-R4E (superseding TASK-009-R4), and TASK-009-R5
- Domain: Python OpenTelemetry span correlation and its persisted observation identity only

The candidate remained at the named commit during this review.

## Boundary and authority coverage

| Boundary | Governing authority | Source and consumer evidence | Result |
|---|---|---|---|
| Public correlation identity | REQ-151; `architecture/wyrd-design.md` observation identity; `architecture/references/domain/telemetry-observations.md` | `sdks/wyrd-sdk-python/python/wyrd/otel.py:192-193,293-308`; `crates/vala/vala-bifrost-redux/src/tables/signal.rs:270-344`; `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:405-474` | PASS |
| Execution-local scope and nested/async isolation | REQ-151; AC-032; revision-46 token-free decision in `run_api.md` | `otel.py:195-198,293-334`; PyO3 delegation at `sdks/wyrd-sdk-python/src/observe/mod.rs:195-248`; focused cases at `test_observe_surface.py:269-383,640-681` | PASS |
| Active-span and child-span enrichment | REQ-151; AC-032 | Active span is stamped only when recording and without an existing CardRef at `otel.py:305-318`; child spans read the innermost parent-context pair at `otel.py:208-230`; focused proof at `test_observe_surface.py:269-310,658-681` | PASS |
| Global/private provider registration | REQ-151; AC-032; R4E and R5 | Weak identity bookkeeping and terminal outcomes are owned at `otel.py:200-205,245-290`; each attempt owns an inactive processor activated only after normal acceptance at `otel.py:217-230,284-287`; global/private, equal-provider, and accept-then-raise cases are covered at `test_observe_surface.py:392-523` | PASS |
| Fail-open enrichment boundaries | REQ-151; AC-032 | Optional imports and all entry, exit, registration, and processor hooks contain enrichment failures at `otel.py:18-23,220-230,257-290,293-334`; PyO3 swallows only calls into `wyrd.otel` at `src/observe/mod.rs:206-248`; focused explicit-observation proof is at `test_observe_surface.py:542-637` | PASS |
| Eval active-span identity | REQ-151; AC-032 | Python's active OTel span is read at the foreign-runtime boundary and passed through the existing explicit options path at `src/observe/mod.rs:125-154,297-334`; the real journey obtains the active tool span IDs at `test_observe_journey.py:384-402` and proves the persisted trace/Eval join at `:456-474` | PASS |
| Authenticated OTLP persistence | AC-032; telemetry observation authority | The stock private provider exports to authenticated `/v1/traces` at `test_observe_journey.py:365-381`; persisted spans prove asserted CardRef, run ID, authenticated publisher, and resolved Card UID at `:421-454`; existing Vala projection authorizes and projects the final record attributes in `signal.rs:294-375` | PASS |
| Excluded identities and lifecycle effects | REQ-151 non-goals | The processor writes only `wyrd.card_ref` and `wyrd.run_id`; Run entry/exit perform no export, flush, network, or Bifrost lifecycle work. The journey separately flushes the provider, shuts down state, and waits for publication at `test_observe_journey.py:550-562` | PASS |

## Source assessment

The revision-46 scope owner is coherent end to end. One import-time OpenTelemetry context key holds the immutable tuple stack. Entry and exit each attach one replacement context and never use detach tokens or store scope state on `PyRun`. Passing the exact view pair into both boundary calls prevents sibling views from removing each other's correlation, while ordinary `contextvars` propagation isolates concurrent asyncio tasks, including two tasks entering the same immutable Run.

Provider registration now closes the two prior source defects rather than guarding consumers. Outcomes are matched by live referent identity, so equal but distinct providers do not collide. A per-attempt processor stays inert until `add_span_processor` returns normally, so a processor retained by an accept-then-raise provider cannot enrich and the failed provider is not retried. Healthy global and private providers still receive one active processor.

The observation path preserves the trust boundary. Python contributes only the exact CardRef and opaque Run ID. Vala retains the lossless attribute payload, uses the final duplicate correlation attribute, validates the CardRef against signed scope, and leaves tenant, principal, request, and Card UID derivation to the server. The persisted journey exercises the real exporter, authenticated endpoint, custom row join, and exact Eval trace/span join.

## Material findings

None.

## Verification limits

- Independently ran `mise exec -- uv run --project sdks/wyrd-sdk-python python -m pytest -q sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py`: **39 passed**.
- Reviewed the task packet's recorded zero-exit evidence for the authenticated Python integration journey, Python unit/integration lanes, type checking, code generation, PyO3 boundary check, formatting, and lints. This domain pass did not independently rerun the Postgres-backed OTLP journey.
- Raw-thread propagation and improper nesting with a foreign attach that outlives the Wyrd scope remain explicitly excluded/accepted by revision 46; neither is treated as a finding.

## Overall result

**PASS**
