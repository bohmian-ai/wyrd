# Repository standards review

## Subject and method

- Base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Prior reviewed candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`
- Candidate: `4a6af5320ab8c236fbe640ff0dc5ddf544bdcde2`
- Candidate tree: `036d647413d0acfd1afdebff2b7057b45ac00609`
- Approved authority: `changes/active/verify-attribution/spec.md`, revision 3
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`
- Remediation task: `changes/active/verify-attribution/review/task-001-827ec9f15/TASK-001-R1-close-trace-continuity-gaps.md`

I inspected the cumulative task diff, each remediation commit, the current
owners and callers named by the navigation map, and both later merges where
they changed task-owned SDK, queue, query, or test surfaces. CodeGraph was used
first for symbol and caller navigation. Unrelated changes brought in from main
were not treated as task findings.

## Authority coverage

| Changed surface | Governing authority read | Result |
| --- | --- | --- |
| Active spec, original task, remediation task, and evidence | `AGENTS.md` §14; `architecture/references/languages/spec-driven-development.md` | PASS for usable approved authority and evidence; lifecycle metadata is stale as noted below |
| Rust client timeline and row projection | `AGENTS.md` §§4–6, 9–12, 16; `architecture/agent-rules.md`; `rust-core.md`; `patterns.md`; `bifrost-design.md`; `olap-serving.md`; `arrow-analytical-interop.md` | PASS for ownership, typed identity, name-based projection, errors, documentation, and test placement |
| Queue admission, sealing, retry ownership, and byte budget | `AGENTS.md` §§4–6, 10–12, 15–16; `architecture/agent-rules.md`; `rust-core.md`; `bifrost-design.md`; `analytical-operations-reliability.md` through the task authority; `testing-workflows.md` | **FAIL**: `STD-R1-001` |
| Telemetry continuation, sampling, tenant routing, and server-span outbox | `AGENTS.md` §§2–6, 9–12, 16; `architecture/wyrd-design.md` Trace context; `architecture/bifrost-design.md` Telemetry; `telemetry-observations.md`; `architecture/wyrd-security-posture.md` | PASS: one server owner, authenticated tenant routing, no raw tenant assertion, one Scribe outbox, non-blocking loss semantics, safe catalog projection |
| Gate producer links and gRPC wire contract | `AGENTS.md` §§2–6, 9–12, 16; `architecture/wyrd-design.md` D4/INV-004; `architecture/bifrost-design.md`; `telemetry-observations.md` | PASS: typed bounded list, links installed at span creation, invalid optional contexts ignored, client fields do not control operator sampling |
| SQL verifier-run persistence and decoder helpers | `AGENTS.md` §§4, 9, 11–12, 16; `architecture/agent-rules.md` SQL capability rules; `rust-core.md` Postgres boundary | PASS: production paths retain `TenantConn`/`OperatorPool` ownership; the remediated `fixed_width` and `stored` helpers each have correct rustdoc and `# Errors` |
| Python Bifrost projection after generator removal | `AGENTS.md` §§7–8, 11–12, 16; `pyo3-boundaries.md`; `python-api-and-stubs.md`; `arrow-analytical-interop.md` | PASS by source: Rust-owned client remains authoritative, PyO3 is thin, public `wyrd.bifrost` exports inline `TypedDict` `SpanRecord`, and the authored public stub re-exports that definition once |
| TypeScript Bifrost projection after the new generator | `AGENTS.md` §§2, 9, 11–12; `typescript-guide.md`; `testing-workflows.md` | PASS by source: `Bifrost.traceTimeline` still projects the native shared client and returns the generated canonical `SpanRecord`; no duplicate transport or durable logic was added |
| Rust, Python, TypeScript, MCP, HTTP/gRPC, and outage proof | `AGENTS.md` §11–12, 16; `architecture/agent-rules.md` test rules; `testing-workflows.md`; `python-api-and-stubs.md`; `typescript-guide.md` | Tests exist at the required tiers and named Rust commands are exact; fresh post-merge runtime evidence is incomplete as recorded below |
| Architecture and public documentation | `AGENTS.md` §2; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/bifrost-design.md` | PASS: complementary request/trace identity, dual export, linked ingest, timeline read, and one Scribe path are current |

## Rule results

| Rule | Evidence | Result |
| --- | --- | --- |
| The configured client byte limit is the one bound for admitted, sealed, in-flight, and retained queue memory, and admitted rows can always use the reserved sealing headroom | `QueueConfig` documents one `max_message_bytes` of headroom at `config.rs:14-16`; `ClientByteBudget::for_config` reserves exactly that at `producer.rs:84-95`; the pre-existing saturation proof is `producer.rs:2033-2068` | **FAIL** (`STD-R1-001`) |
| Context strings are charged at admission and retained sealed contexts remain charged through settlement | `Producer::enqueue_prepared` includes `context_bytes` in the record reservation at `producer.rs:545-568`; `RecordQueue::seal` includes selected contexts in the sealed guard at `queue.rs:561-600` | PASS in isolation; its transition violates the headroom guarantee described in `STD-R1-001` |
| Timeline output is ordered and unique by `(trace_id, span_id)` regardless of duplicate start time | `Bifrost::trace_timeline` sorts then retains through a local `HashSet`; the existing integration fixture places an intervening span between duplicates | PASS |
| Changed Rust items have intent-bearing rustdoc and fallible items have `# Errors` | Remediation items and cumulative changed owners were inspected; `fixed_width` and `stored` now carry separate correct blocks | PASS |
| Tenant SQL capabilities use `TenantConn` or `OperatorPool`; raw `PgPool` stays at test/setup boundaries | Production changed query/server signatures preserve the approved capabilities; `PgPool` in the outage integration test is the fixture's superuser assertion pool, not library tenant access | PASS |
| Python runtime exports, authored stubs, and tests use the public package | `wyrd.bifrost.__init__` and `__init__.pyi` agree; `SpanRecord` is defined once inline in `_client.py`; journeys import `wyrd.bifrost` | PASS by inspection and reported post-merge typecheck |
| TypeScript wire types are generated and the wrapper projects the native client | `card-types.ts::SpanRecord`, `index.ts::Bifrost.traceTimeline`, and N-API declaration remain aligned; post-merge `codegen:check` and typecheck were reported green | PASS by inspection and static evidence |
| Every public SDK surface has a real journey | Rust, Python, and TypeScript action-timeline journeys remain present and assert linked producer/consumer spans | PASS for test presence; execution on the merged candidate is not yet proved |
| Repository checks are not bypassed and named tests use exact selectors | The remediation record corrects the ignored Rust timeline selector with `--run-ignored all`; no `allow`, deleted assertion, or ignored replacement was introduced | PASS |
| `use` statements stay at module top | New `pg_verification_runtime.rs` helpers contain function-scoped imports at lines 3531–3532 and 3564 | Non-blocking standards note; no behavioral consequence |
| Task lifecycle metadata reflects the reviewed authority and phase | Original task remains `status: proposed`, `spec_revision: 2`; remediation remains `status: ready` although its evidence says implemented | Non-blocking workflow note; revision 3 is unambiguous from the approved spec and remediation |
| Commit metadata contains no agent attribution | The five remediation commits contain `Claude-Session:` trailers | Non-blocking history note for this acceptance audit; no implementation or public-contract consequence |

## Material finding

### STD-R1-001 — INCORRECT: producer contexts consume sealing headroom that was never reserved

- **Violated rule and obligation:** `AGENTS.md` §§11–12 and §15 require
  bounded queue behavior and a root-cause correction; `QueueConfig` and
  `ClientByteBudget` promise that admitted rows can always be sealed from the
  one reserved `max_message_bytes` headroom; remediation
  `FIND-TASK-001-1` requires context bytes to transfer from admission to the
  sealed batch without weakening that bound.
- **Location:** `crates/shared/wyrd-queue/src/producer.rs:84-95` and
  `545-568`; `crates/shared/wyrd-queue/src/queue.rs:561-600`.
- **Evidence:** admission can legitimately fill `admission_limit`, exactly as
  the existing `admitted_rows_cannot_take_the_sealing_headroom` test does at
  `producer.rs:2033-2068`. The budget reserves only
  `max_message_bytes` beyond that limit. A traced chunk now asks
  `reserve_sealing(max_message_bytes + trace_bytes)` while its admitted rows
  and their already-charged contexts are still live. Any nonzero selected
  context therefore makes that reservation fail when admission is full. The
  failure restores the same rows, so the next flush repeats the same refusal;
  nothing can settle to create the missing space.
- **Observable consequence:** a valid traced write accepted by `insert`,
  `record`, or `observe` can become permanently unflushable under ordinary
  byte-budget saturation. `flush` and shutdown return backpressure rather
  than sending already-admitted work, contradicting both the queue's sealing
  guarantee and the remediation's bounded-lifecycle outcome.
- **Required testable correction:** keep `ClientByteBudget` as the sole owner
  and transfer the selected contexts' existing charge from the admitted
  record to the sealed batch rather than requiring extra space outside the
  configured sealing headroom. Preserve the current first-32 selection,
  stable retry bytes, public configuration, and refusal semantics. Extend the
  existing sealing-headroom saturation test (or add one adjacent focused test)
  with a non-empty producer context, exhaust admission to its documented
  ceiling, and prove `flush` publishes and terminal settlement returns the
  budget to zero. Do not add another limiter, configuration option, or queue.

## Non-blocking notes

- Move the two new gRPC and cluster imports in
  `pg_verification_runtime.rs:3531-3532,3564` to the module import block when
  this test file is next touched; the current placement violates the explicit
  import-layout rule but changes no behavior and cannot independently block
  under the task-review materiality rule.
- Before final task acceptance, update the original task to revision 3 and
  move both task statuses to the phase the workflow actually reached. These
  stale fields do not make the approved revision or reviewed implementation
  ambiguous.
- Remediation commit bodies carry forbidden `Claude-Session:` attribution.
  This is a concrete `AGENTS.md` §13 violation, but it is recorded as
  non-blocking here because it has no repository-state, runtime, security,
  durability, or public-contract consequence and correcting it would rewrite
  the immutable review subject.

## Verification assessment

Recorded before the two main merges: the three focused remediation tests,
`test:shared`, `test:bifrost:journey:sdk`, and
`test:bifrost:integration:server` passed. Recorded on candidate
`4a6af5320`: format, lints, typecheck, `codegen:check`, and `docs:check`
passed. I did not run Cargo or mise while the shared review checkout had other
reviewers.

The two merges changed the Python `SpanRecord` runtime representation, the
generated TypeScript `SpanRecord`, and task journey helpers. The Python and
TypeScript action-timeline journeys and the three focused remediation tests
have not been rerun on the immutable candidate. Static inspection finds their
imports and shapes aligned, but final acceptance still needs fresh execution
of the exact three remediation tests and the narrow Python/TypeScript timeline
journey lanes selected by the orchestrator. Static typecheck/codegen evidence
does not substitute for those runtime journeys.

## Overall result

**FAIL**

`STD-R1-001` is a reachable queue-progress regression in the remediation for
the prior byte-budget finding. The other repository-owned boundaries inspected
are compliant by source, subject to the explicit post-merge runtime
verification gap above.
