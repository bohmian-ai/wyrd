# TASK-002 data and durability domain review

## Immutable subject

- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `fbfc2591a985b288935180098f892aecdf3b8b49`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Review boundary: the five fixed verification-table schemas; authored versus managed columns; publisher/subject/run identity; generic fixed-size-binary conversion; fixed and dynamic table description/routing; lazy built-in materialization; daily partition, Bloom, sensitivity, and existing-retention declarations; and the touched Forge/audit SQL only where it changes persistent correctness, recoverability, or durable progress.

`HEAD` was the candidate before and after inspection. `git diff --quiet fbfc2591a985b288935180098f892aecdf3b8b49 -- ':!changes/active/verified-change-contract/review/TASK-002-r1'` returned success at close, so the reviewed source remained immutable.

## Authority coverage

| Boundary | Authority read | Result |
|---|---|---|
| Repository, SQL, audit, testing, and completion rules | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/references/languages/testing-workflows.md` | PASS |
| Wyrd identity and client/server ownership | `architecture/wyrd-design.md`; approved spec revision 32 | PASS |
| Bifrost table identity, managed envelope, acknowledgement, lazy built-ins, partitioning, Forge, retention, and audit publication | `architecture/bifrost-design.md`; `architecture/references/domain/vala-architecture.md`; `olap-serving.md`; `iceberg.md`; `datafusion.md`; `arrow-analytical-interop.md`; `analytical-operations-reliability.md` | PASS |
| Exact task-local data contract | `architecture/logic/table_schema.md`; `architecture/logic/run_api.md`; TASK-002 | PASS |

CodeGraph was attempted first as required, but this checkout has no `.codegraph/` index; ordinary source inspection was used.

## Source and consumer coverage

| Concern | Source and consumer evidence | Result |
|---|---|---|
| Five exact schemas | The closed built-in registry includes Drift observations, Eval observations, common results, Drift result features, and Eval result items in `crates/vala/vala-bifrost-redux/src/tables/mod.rs:750`. Their owning modules declare the exact authored field order, Arrow types, nullability, payload classification, and sensitive columns from `table_schema.md`. `verification_tables_match_their_approved_schemas` pins all five complete authored contracts and layouts at `tables/mod.rs:898`. | PASS |
| Managed columns and identity split | Every fixed table uses `CorrelationPolicy::Observation`; `ensure_managed_columns` appends nullable `run_id`/`card_uid`, required `principal_id`/`wyrd_request_id`, then the required event, ingest, batch, ordinal, and tenant fields in the approved order at `tables/managed_columns.rs:19`. Scoped runs carry the exact subject `CardRef` plus one invocation `RunId` only as `Correlation` (`wyrd-client/src/observe/mod.rs:89`), and Drift/Eval enqueue through `insert_into` without authored correlation fields (`observe/mod.rs:161`, `observe/mod.rs:206`). Gate/Scribe remains the existing authority that validates the asserted CardRef and stamps resolved subject UID, authenticated publisher principal, request, time, batch, ordinal, and tenant. | PASS |
| Fixed-size binary conversion | The generic schema-driven queue branch handles every described `FixedSizeBinary(width)` at `crates/shared/wyrd-queue/src/batch_builder.rs:286`; `decode_lower_hex` requires exactly `2 * width` lowercase hexadecimal characters and neither pads nor truncates (`batch_builder.rs:320`). Focused tests cover nullable 16-byte trace IDs, 8-byte span IDs, byte-for-byte round trip, uppercase, malformed, non-string, short, and long refusal (`batch_builder.rs:500`). Eval projects the typed IDs as canonical lowercase hex before queue insertion. | PASS |
| Fixed-table startup and lazy built-ins | `StartClaim::complete` describes both fixed input tables and validates their projection types before publishing the one started writer (`wyrd-client/src/observe/lifecycle.rs:155`). `BifrostCatalog::describe_table` resolves a built-in definition and calls `ensure_builtin` before lookup (`vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1279`); `ensure_builtin` uses the same canonical schema/layout registration path as first ingest (`bifrost_catalog.rs:901`). Thus a never-written tenant materializes the server-owned table instead of seeing a false 404, while conflicting existing physical/control metadata still fails validation. | PASS |
| Dynamic description and explicit routing | `Bifrost::writer_table` caches the first server-described user schema by FQN and returns the cache winner to concurrent callers without mutating the active table (`wyrd-client/src/bifrost/facade.rs:304`). `insert_into` passes that immutable FQN/schema to the existing `WriterPool` (`facade.rs:342`). `Observe::record` admits only one-segment `vala.datasets.<name>`, describes before enqueue, and supplies scoped correlation (`observe/mod.rs:248`). Unit coverage proves describe reuse, reserved/system refusal before describe, unknown-table refusal, and multi-table/scope routing; the recorded Rust/Python/TypeScript journeys exercise the real queue/IPC/Gate/Scribe path. | PASS |
| Partition, Bloom, sensitivity, and retention | Each fixed table declares daily `wyrd_event_time` partitioning. Table-specific Blooms are `series`; none for Eval observations; `result_id`/`subject_card_uid`/`binding_id` for results; and `result_id` for both detail tables. The catalog resolves these with the managed `run_id`/`card_uid`/`principal_id` floor and persists the canonical layout into Iceberg properties (`bifrost_catalog.rs:1027`). Sensitive declarations match the authority: Eval `context`/`media`, common result `details`, and Eval item `actual`/`expected`/`message`. No verification-specific TTL, deletion job, retention setting, or alternative maintenance path was added; all five remain on existing Forge/Iceberg retention. | PASS |
| Forge clock-domain durability edits | Production enqueue callers use `ready_at: None`; both enqueue paths stamp `COALESCE($n, statement_timestamp())`, and non-consuming retry also stamps `statement_timestamp()` (`vala-sql/src/queries/forge_tasks.rs:347`, `:843`). Claims compare against the same database clock. Task identity, plan idempotency, claim ownership, attempt budget, and backoff behavior remain unchanged. This removes host/database skew from immediate eligibility without weakening durable fencing or recovery. | PASS |
| Audit publication progress edit | `freeze_publication_range` still serializes the tenant chain head with `FOR UPDATE`, reuses any frozen upper bound, and derives the same deterministic range. The new transaction-local three-second lock timeout recognizes only PostgreSQL `55P03`; timeout aborts/rolls back that short tenant transaction, leaving the watermark, frozen bound, and staging rows unchanged for the next sweep (`vala-sql/src/queries/audit_staging.rs:199`). Per-tenant sweep concurrency means the bounded wait cannot prevent other tenants or the next directory refresh from progressing. No second audit sink or publisher was introduced. | PASS |

## Verification limits

No commands were rerun during this immutable static review. TASK-002 records the exact queue test plus passing `test:shared`, `test:wyrd-sdk`, all nine `verify:bifrost` lanes (including Rust, Python, and TypeScript journeys), Python and TypeScript unit/integration/type checks, codegen, client-tier and PyO3 boundary checks, format, lints, and `git diff --check`, all against `2f9e401d`. That commit is the direct parent of candidate `fbfc2591`; the only candidate delta after it is the TASK-002 evidence record itself. The evidence is therefore applicable to the reviewed implementation.

This task establishes result/detail schemas and their physical recipes but does not yet publish result rows; result-writer identity, same-event-time summary/detail publication, multi-partition query/pruning, and physical result Bloom evidence are explicitly consumed by later TASK-004/TASK-005/TASK-008 work. They are not treated as missing TASK-002 behavior.

## Proposed findings

None.

## Overall result

**PASS** — the candidate satisfies TASK-002's persistent data/schema/durability boundary. The five fixed schemas and physical layouts are exact, subject and publisher identities remain separated at the correct trust boundary, fixed-width IDs stay typed and lossless, lazy/cached description does not create an alternate schema or routing authority, and the adjacent Forge/audit changes preserve durable evidence and recovery semantics.
