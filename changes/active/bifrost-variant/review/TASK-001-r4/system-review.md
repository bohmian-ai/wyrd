# TASK-001 System-Resilience Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `a6429060fb011aafa4335f2f736c70adab231739`
- Candidate tree: `64177d67141993ec18e63b43dc227dbc31d70950`
- Approved authority: `SPEC-bifrost-variant` revision 11 and original `TASK-001`
- Scope: cumulative runtime and deployment effects only; no prior reviewer conclusion was used.

The candidate commit remained available with the supplied tree throughout this review. The checkout contains later review-process commits, so candidate source was resolved commit-scoped with `git show` and `git diff` where that distinction mattered.

## Deployed-Path Evidence

| Changed runtime path | Deployed ownership and propagation | Affected capabilities |
|---|---|---|
| Built-in ingest and Variant validation | Gate reaches Scribe through the existing ingest seam. Scribe resolves the table and database receipt instant, acquires bounded memory/admission ownership, then decodes and runs the table-owned validator before constructing `AdmittedAppend` or dispatching to a shard (`scribe/ingress.rs:408-450`, `552-627`; `scribe/execution_lanes.rs:533-577`, `580-647`). A refusal drops local leases and reaches Gate as the catalogued `ContractViolation`; no WAL append, batch fence, shard handoff, or acknowledgement occurs. | OTLP traces/logs/metrics, verification results, gateway captures, agent traces, audit publication, and other changed built-ins. |
| Interactive and analytical query execution | The one `OracleVariantSql` owner is installed into ordinary leader sessions, analytical leader sessions, and distributed worker sessions before planning/execution (`oracle/mod.rs:3097-3111`; `oracle/analytical.rs:538-545`, `7684-7694`). A UDF or scan error is mapped at the stream boundary and becomes a failed terminal after rows already framed (`oracle/query_stream.rs:470-525`). | HTTP, gRPC, MCP, CLI, scheduled/internal verification reads, and Rust/Python/TypeScript SDK queries. |
| Distributed error transport | Catalogued execution failures are wrapped as tagged `BifrostError` JSON in `DataFusionError::External`; the coordinator walks local source chains or reconstructs the tagged worker error and otherwise falls back to the generic execution error (`oracle/mod.rs:4233-4308`). No message-family parser or Variant-only branch remains. | Late failures, follower failures, tenant-footer refusals, resource failures, and stable error consumers across every query surface. |
| Query terminal and client acceptance | A failed query cancels request and stream tokens, joins distributed descendants, drains failed partition children, settles analytical ownership, releases active reads, and only then emits its terminal (`oracle/query_stream.rs:647-715`, `718-805`). The shared client collector retains batches privately and returns no result if the terminal or transport fails (`wyrd-client/src/bifrost/query.rs:775-825`); Python and TypeScript project that shared client behavior. | Prevention of successful partial results, prompt follower cleanup, running-query retirement, and safe client retry decisions. |
| Iceberg v3 creation and Forge maintenance | New and existing registered physical tables are required to be v3 (`catalog/bifrost_catalog.rs:1016-1059`, `1077-1100`). Forge now permits manifest maintenance for v3 under the existing table lease and stop token (`forge/gc.rs:223-299`). Managed rewrites use the pinned compaction core's hidden-metadata projection and pre-write non-null/type validation; Wyrd publishes only the returned five-field handoff (`forge/worker.rs:6548-6610`). | Every built-in and user table, repeated compaction, manifest rewrite, snapshot expiry, and orphan cleanup. |
| Bloom sizing | Scribe and Forge use the same writer recipe; Bloom enablement is per allowlisted column while parquet-rs derives capacity from the row-group ceiling and folds on write (`parquet/writer_properties.rs:54-83`, `101-145`). | Ingest and rewrite memory/footer cost plus hot and published query pruning. |

## Failure-Path and Recovery Assessment

| Failure or interruption | Observed boundary and recovery behavior | Assessment |
|---|---|---|
| Malformed, oversized, too-deep, or out-of-range built-in Variant | Validation occurs in the bounded ingress CPU lane before shard dispatch, WAL, durable batch fencing, or ACK. Existing memory and admission guards are RAII-owned and are released on error or cancellation. Retrying corrected input starts a new ordinary append; no partial durable state exists. | PASS |
| Postgres unavailable while Scribe reads the admission instant or registration | The request fails before payload dispatch and durable acceptance (`scribe/ingress.rs:408-450`). The shared server remains up; only writes requiring the unavailable control plane fail. Recovery is dependency restoration plus normal client retry with the stable batch ID. | PASS |
| Client cancellation before and after Scribe dispatch | Before dispatch, local reservations drop. After dispatch, the shard owns the append and its durable ACK semantics (`scribe/ingress.rs:535-551`). This preserves the existing ambiguity boundary and stable-batch retry/dedup contract. | PASS |
| Oracle timeout, cancellation, execution error, peer loss, or client body drop | Cancellation stops further batch exposure, failed terminals cancel both ownership trees, and settlement joins descendants before releasing admission and active reads (`oracle/query_stream.rs:647-715`, `747-805`). Dropping the stream revokes analytical ownership before releasing read protection (`oracle/query_stream.rs:530-561`). A component failure therefore fails the query, not the shared server process or unrelated queries. | PASS |
| Worker-side catalogued failure after rows | The candidate transports the complete catalog problem across the lossy peer error hop, emits a failed terminal, and shared collectors discard their retained rows. Unknown/malformed external text remains the generic execution failure. This keeps retry classification stable without treating untrusted text as a catalog identity. | PASS |
| Oracle process/pod replacement | Query state, admission, active reads, and spill are process-local; interruption terminates the in-flight query. Durable table state remains in Postgres/object storage, and a replacement reconstructs Variant SQL registration through each session constructor. No new durable Oracle state or restart dependency was introduced. | PASS |
| Forge cancellation or lease loss during a v3 rewrite | The pinned managed core validates both lineage columns before writing each v3 batch, drains writer tasks, records possible outputs, and returns cancellation/failure rather than a commit-ready handoff. Wyrd's existing fenced publication/reconciliation path remains the only visibility transition. Retry replans from the durable current snapshot; an object PUT alone is not accepted as publication. | PASS |
| Forge process crash or uncertain catalog outcome | TASK-001 does not add another publication identity or retry loop. Existing task, lease, operation, output-generation, and reconciliation identities remain unchanged; the v3 lineage values are data carried inside candidate output files and become authoritative only through the existing fenced commit. | PASS |
| Manifest rewrite/GC on v3 | The prior v3 skip is removed only after the pinned Iceberg/compaction changes and production-path test establish retained `first_row_id`/hidden lineage across repeated rewrites and maintenance. Maintenance still runs per table under lease, checks revocation before commit, logs one-table failure, and continues the pass. | PASS |
| Rolling overlap with v2 data | The active specification explicitly declares no shipped tables/data and no migration or mixed-version rollout. The candidate fails closed on any non-v3 physical table rather than interpreting it. Because v3 creation and lineage-safe rewrite land in the same candidate, no task-authorized intermediate version can create v3 data with the old rewrite behavior. | PASS within the approved deployment boundary |

## Recovery and Proof Assessment

Available evidence is proportionate to the changed failure surfaces:

- The real Postgres/object-store Forge journey `v3_row_lineage_survives_repeated_rewrite` exercises built-in and user-table creation, production scheduling, repeated rewrites, interleaved fresh appends, manifest rewrite, snapshot expiry, and v3 GC while comparing each logical row's `_row_id` and `_last_updated_sequence_number` after every step.
- The pinned compaction-core test `rewrite_preserves_v3_row_lineage` exercises repeated v3 compaction and fresh appends, and the focused lineage validator rejects missing, null, and mistyped hidden columns before output is commit-eligible.
- `published::variant_sql_registry_covers_every_session` covers interactive and analytical session construction, follower execution, pre-stream and late Variant failures, a worker tenant refusal, no returned partial collected result, and graph settlement.
- Focused query-stream, tonic conversion, and shared-client tests cover complete failed-terminal problem transport and reconstruction. Rust, Python, TypeScript, and MCP journeys cover the public terminal projections.
- Task evidence records passing formatting/lint, code generation, documentation, V1-V15 focused journeys, both fork tests, touched crate lanes, and exact late-terminal selectors on the final implementation candidate. The later `iceberg-compaction` repin to `2b65fa189f2d05002acc6e59515a071a63777970` was included in the final V10/V13 rerun evidence.

Residual proof limits do not constitute a reachable TASK-001 regression: there is no destructive pod-kill test during the exact lineage write loop, and the worker tenant-refusal journey permits either pre-stream or late arrival because footer validation may fail on the first poll. The existing Forge uncertain-publication and Oracle cancellation/peer-loss suites cover those lifecycle owners; the changed lineage and error identities are independently exercised at their actual boundaries.

## Material Findings

No material system-resilience findings.

The candidate keeps failures scoped to the request, query, plan, or table that owns them; preserves bounded admission and structured cancellation; does not introduce a second durable authority or retry identity; fails closed on unsupported physical format and untrusted error text; and has recovery proof for the runtime paths materially changed by TASK-001.

## Overall Verdict

**PASS**
