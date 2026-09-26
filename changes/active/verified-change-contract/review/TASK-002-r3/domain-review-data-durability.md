# TASK-002 R3 data and durability domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `04f73570397d5123eb767abafa60d016c37de1db`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Prior review inputs: `review/TASK-002-r1/`, `review/TASK-002-r2/`, and the R2 remediation task
- Reviewed boundary: persistent table schemas and physical recipes, JSON-to-Arrow queue conversion, fixed-width Eval identities, schema description/cache use, Gate/Scribe fingerprint fencing, stored-row readback, client drain semantics, and persistent-data aspects of the test hooks added by R2 remediation.

`HEAD` resolved to the candidate before inspection and again before this report was written. No candidate source was modified; this report is the reviewer's only write.

This checkout has no `.codegraph/` directory, so the repository's CodeGraph route was unavailable and ordinary source inspection was used.

## Authority coverage

| Boundary | Authority | Result |
|---|---|---|
| Repository, SQL, audit, testing, and task-review rules | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md` | PASS |
| Bifrost physical identity, managed columns, acknowledgement, shutdown, recovery, and schema fencing | `architecture/bifrost-design.md`; `architecture/references/domain/olap-serving.md`; `architecture/references/domain/analytical-operations-reliability.md`; `architecture/operations/reliability-and-recovery.md` | PASS |
| Arrow schema, nullability, fixed-width binary representation, IPC, and fail-closed conversion | `architecture/references/domain/arrow-analytical-interop.md` | PASS |
| Exact change contract | approved spec revision 32; `architecture/logic/table_schema.md`; TASK-002; R1/R2 verdicts and remediation tasks | PASS |

## Source and verification coverage

| Concern | Source and consumer evidence | Result |
|---|---|---|
| Five fixed verification tables | The closed built-in registry in `crates/vala/vala-bifrost-redux/src/tables/mod.rs` includes Drift observations, Eval observations, common verification results, Drift result features, and Eval result items. Their owning modules match `table_schema.md` in authored column order, Arrow type, nullability, daily partitioning, table-specific Bloom intent, and sensitive-column classification. `verification_tables_match_their_approved_schemas` independently pins all five definitions and the catalog-resolved managed Bloom floor. | PASS |
| Managed envelope and identity split | All five tables retain `CorrelationPolicy::Observation`, so the catalog appends the managed run, subject, publisher, request, event, ingest, batch, ordinal, and tenant columns rather than accepting them as payload. `Run::correlation` supplies the invocation `RunId` and exact scoped subject `CardRef`; the existing Gate/Scribe path validates that scope and stamps publisher and subject separately. The observation table tests prohibit the retired Verifier reference and duplicate correlation columns. | PASS |
| Eval `FixedSizeBinary(16)/(8)` conversion | `crates/shared/wyrd-queue/src/batch_builder.rs` handles described `FixedSizeBinary(width)` generically. `decode_lower_hex` requires exactly twice the byte width and lowercase hexadecimal only, while Arrow's fixed-size builder preserves nulls and exact bytes. The three focused queue tests cover successful trace/span conversion and IPC round trip plus short, long, uppercase, prefixed, non-string, and malformed refusal; they were rerun in this review and all three passed. | PASS |
| Fixed-table startup compatibility | `StartClaim::complete` describes both fixed observation tables before publishing `Started`, and the shared projection validator compares the full authored field count, order, name, datatype, and nullability. The R2 Rust, Python, and TypeScript journeys make each fixed-table describe fail through the real audited server path, observe the stable failure, restore it, and subsequently start the same state. No failure path caches an invented schema or admits a row. | PASS |
| Describe caching and routing | `Bifrost::writer_table` retains each server-described FQN and user schema for the writer lifetime, uses the existing single miss gate with a cache recheck, and hands insertion to the existing `WriterPool`; it does not mutate the active table or add a producer owner. Each SDK journey writes the same dynamic table twice, observes one server-side allowed describe, and verifies that Drift/Eval emissions do not add fixed-table describes. | PASS |
| Stale-schema fence | `assert_stale_writer_is_fenced` in `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs` connects a second writer carrying an extra column, admits its row only to the local queue, and proves both flush and replacement registration fail with `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH`. The subsequent real query returns only the original rows, so the check covers both stable refusal and absence of stale data rather than only error mapping. | PASS |
| Eval persistence and readback | The real Rust, Python, and TypeScript journeys enqueue session/media plus non-null trace/span identities through their public SDK boundary, shut down the state as the client drain barrier, flush server-owned Scribe state, and query the exact persisted fields. Rust proves explicit IDs; Python retains absent, active, and explicit cases; TypeScript proves an active OpenTelemetry context crossing N-API. Each journey also proves a span without a trace and malformed media add no row. | PASS |
| Durability semantics | Observation calls remain bounded local enqueue operations and are not described as Scribe acknowledgements. Successful state shutdown drains the retained writer; an ambiguous failure retains the same writer and batch identity for retry. The R2 work does not introduce an event-level commit, second queue, new durable state, or alternate recovery path. | PASS |
| R2 test-server probes | `table_describe_count` reads the canonical staged audit decisions for the fixture tenant, and the journeys explicitly disable publication so retirement cannot make the counter shrink. `fail_table_describe` installs a tenant- and resource-specific trigger which makes the canonical audit append fail closed; `restore_table_describe` removes it. These are test-only probes and do not alter production schema, admission, or durability behavior. | PASS |
| Adjacent Oracle test fixes | The post-remediation Oracle commits change test wait calculations only. They neither modify persistent Bifrost state nor weaken a TASK-002 data fence, schema contract, or durability boundary. | PASS |

## Prior-finding closure

| Finding | Data-boundary closure | Result |
|---|---|---|
| `FIND-TASK-002-2` | The fixed startup validator compares the complete ordered authored schema, and the five Vala definitions remain pinned to the approved authority. | CLOSED |
| `FIND-TASK-002-6` | First-use describes converge through the existing owner cache and miss gate; later writes reuse the same schema and `WriterPool` producer. | CLOSED |
| `FIND-TASK-002-8` | Ambiguous shutdown retains and retries the same batch on the same state, while successful shutdown is the terminal client drain boundary. | CLOSED |
| `FIND-TASK-002-9` | Audit publication lock timeout propagates from the aborted transaction, leaving durable range state unchanged for a fresh retry. | CLOSED |
| `FIND-TASK-002-12` | Real unknown and denied describes fail before cache and producer admission through the canonical server boundary. | CLOSED |
| `FIND-TASK-002-13` | All three SDK journeys now prove persisted session/media and exact fixed-width trace/span identity, plus row-count-preserving refusal of invalid Eval input. | CLOSED |
| `FIND-TASK-002-14` | All three SDK journeys now prove fixed-table startup refusal and cached describe reuse; the existing real-server Rust journey proves stale-schema flush and registration refusal with no stale row stored. | CLOSED |

## Verification limits

This Wave 1 review reran only the three exact `wyrd-queue` fixed-size-binary tests and `git diff --check`; both passed. It did not rerun Postgres, server, or three-language journey lanes. The task records `verify:bifrost` 9/9, the shared and Rust SDK lanes, all Python and TypeScript unit/integration/type/binding lanes, codegen and boundary checks, formatting, and lints as passing through `4fc251ce`; the later `1f00ce91` server-integration run passed 58/58, and `e6adf997` records its two exact Oracle tests plus Clippy rather than another full Bifrost aggregate. Static inspection confirms those later commits are test-only and do not invalidate the data-path evidence.

TASK-002 defines the result/detail table schemas but does not yet publish Verifier result rows. Same-event-time multi-table result persistence, result/detail partial acknowledgement, and result-query pruning remain obligations of later tasks under the approved decomposition and are not represented as completed here.

## Proposed findings

None. No reachable task-scoped schema, Arrow encoding, fingerprint, persistence, or durability defect remains.

## Overall result

**PASS** — the cumulative candidate satisfies TASK-002's persistent data and durability obligations. The five schemas retain one server-owned authority, fixed-width identities survive the public SDK-to-Scribe-to-query path without coercion, cached routing reuses the existing writer and queue, stale declarations are fenced before storage, and shutdown remains the explicit client durability barrier.
