# Maintainer review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/verify-attribution`
- Base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`
- Candidate tree: `9df6ae0e34c646e6ef2c32a1c030e2a800683644`
- Result: **FAIL**

The current worktree has the same tree as the immutable candidate. This review
used the complete base-to-candidate diff and did not modify reviewed source.

## Changed-surface coverage

| Surface | Symbols and callers inspected | Maintenance assessment |
|---|---|---|
| Process telemetry and request routing | `context::{inbound_context, serve_request, continued_context}`, `export::{ServerSampler, OperatorExport, TenantRoutes, TenantExport, TenantSpanSink}`, HTTP and gRPC `request_span`, authentication route binding, `AppState`/`Bifrost` sink ownership | The stateful route registry, per-server sink, sampler wrapper, and processor wrappers each own a distinct required invariant. The sole production sink implementation is behind a trait because `wyrd-telemetry` cannot depend on `wyrd-server`, and tests inject the same seam. No unearned public option, factory, or second route opener remains. Rustdoc explains ownership, identity binding, filtering, and non-blocking staging. |
| Scribe staging and bounded admission | `ServerSpanSink`, `ScribeWrite::ServerSpans`, `project_spans`, `ScribeTable::Spans`, permit lifecycle, state composition, outage tests | The fixed permit pool is local to the new high-rate producer and rides with the staged write across retries. It reuses the one existing outbox and projector. Layout and naming make the ownership path discoverable. |
| Queued-write producer context | `transport::trace`, `Producer::enqueue_prepared`, queue `Row`, `producer_traces`, `SealedBatch::producer_traces`, gRPC request construction, proto `ProducerTraceContext`, Gate `producer_links` and ingest-span creation | Context is captured once at admission, bounded and deduplicated once at seal, and reused on retry. The queue extends its existing retained batch owner rather than adding another retry state. Names and docs expose the frame-grain invariant directly. |
| Trace timeline | `Bifrost::trace_timeline`, blocking projection, `bifrost::rows` decoder, server Eval decoder caller, Python sync/async projection, TypeScript projection, generated `SpanRecord` declarations, focused and journey tests | The implementation reuses the authenticated query path and canonical `SpanRecord`. Moving the existing decoder to one shared owner deletes the parallel server decoder. Public names, parameter and result types, and generated shapes agree across the three SDKs. |
| Foreign-runtime propagation | Python `trace::{active_trace_context, block_on, traced}`, Node `trace::{set_trace_source, spawn, traced}`, all changed native binding entry points, `start_telemetry` propagator setup | The shared helpers isolate the unavoidable Python/Node runtime handoff. Although many Node methods now clone their handle into `trace::spawn`, there remains one propagation mechanism and no new per-capability abstraction. The repeated boundary edits are required because Node context must be read on the JavaScript thread before the Rust future is spawned. |
| Deferred verifier attribution and gateway ancestry | verifier-run migration and query mapping, `RunRequest`/`ClaimedRun`, runner link, Eval fan-out instrumentation, gateway capture parent selection, SQL/integration tests | The context rides the existing unshipped run row and existing queue types. Gateway capture and Eval reuse current owners; no compatibility path or second execution model was added. One documentation regression in this surface is the material finding below. |
| Architecture, generated artifacts, and tests | Wyrd/Bifrost design text, spec r3, schema generator and generated Python/TypeScript declarations, Rust/Python/TypeScript/MCP journeys, shared test ancestry/timeline helpers | Architecture text describes the implemented single rule. Test helpers were centralized only where multiple journeys use them. Generated declarations visibly contain the canonical span type and timeline return shape; recorded task evidence reports `codegen:check`, format, lint, typecheck, dependency, tenancy, and owner lanes green. |

## Material findings

### MAINT-001 — `stored` lost its rustdoc when `fixed_width` was inserted

- Location: `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:2143-2158`
- Governing rule: `architecture/agent-rules.md` requires substantive rustdoc
  for every new or materially modified Rust item, including private helpers,
  and treats missing rustdoc as `BLOCK_BEFORE_MERGE`.
- Evidence: the pre-existing documentation for `stored` now immediately
  precedes the new `fixed_width` documentation with no item boundary. Rust
  attaches the combined block to `fixed_width`, giving that helper two
  unrelated `# Errors` sections. `pub(crate) fn stored` at line 2158 now has
  no rustdoc at all.
- Maintenance cost: generated/source documentation falsely says
  `fixed_width` wraps an arbitrary input error, while maintainers reading the
  broadly reused `stored` mapper lose its error contract. The placement also
  obscures which invariant the new database-width guard owns.
- Smallest correction: keep only the “Convert stored identifier bytes…” block
  immediately above `fixed_width`, then restore the original “Decode a stored
  value…” block immediately above `stored`. No code, test, or abstraction
  change is needed.
- Closure proof: inspect the two adjacent helpers and confirm each has exactly
  one matching intent and `# Errors` block; rerun the already-required Rust
  formatting/lint lane with the remediation.

## Non-blocking calibration

- `TenantRoutes::start` uses `Span::exported_data` to inspect links at the
  processor boundary, which clones span data once for routed request spans.
  OpenTelemetry exposes the needed link set there through that snapshot; the
  cost is localized, documented with a `ponytail:` upgrade condition, and does
  not introduce a second routing mechanism. Given the approved linked-ingest
  rule, this is simple enough and is not a finding.
- The task frontmatter still names spec revision 2 while the candidate and
  implementation evidence use approved revision 3. This is left to the task
  acceptance/standards reviewers because it concerns review authority, not the
  maintainability of an implementation symbol.

## Verification limits

No Cargo, mise, code-generation, Python, or TypeScript commands were rerun in
this parallel discovery pass. The review inspected candidate source, callers,
generated declarations, tests, and the verification evidence recorded in the
task. No additional focused runtime command is needed for MAINT-001.

