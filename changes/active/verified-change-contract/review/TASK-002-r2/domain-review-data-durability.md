# TASK-002 R2 data and durability domain review

## Immutable subject

- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `a000c201ae86f584fd5b80349f375e087902fd78`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Prior review and remediation: `changes/active/verified-change-contract/review/TASK-002-r1/`
- Review boundary: the five verification-table schemas and physical recipes; fixed-table startup compatibility; generic fixed-width ID decoding; dynamic-table describe/cache/producer convergence; observation publisher-versus-subject identity; shutdown durability and ambiguous retry; and audit-publication lock-timeout transaction behavior.

`HEAD` resolved to the candidate before inspection. No candidate source was edited; this report is the only path owned by this reviewer.

## Authority coverage

| Boundary | Authority read | Result |
|---|---|---|
| Repository, SQL transaction, audit, Rust documentation, testing, and completion rules | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/references/languages/testing-workflows.md` | PASS |
| Wyrd client/server, Card-scope, observation, and publisher identity | `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; approved spec revision 32 | PASS |
| Bifrost schema, managed envelope, WAL acknowledgement, lazy built-ins, daily partitions, Bloom filters, retention, and audit-history recovery | `architecture/bifrost-design.md`; `architecture/references/domain/olap-serving.md`; `iceberg.md`; `arrow-analytical-interop.md`; `analytical-operations-reliability.md`; `architecture/operations/reliability-and-recovery.md` | PASS |
| Exact task-local schema and lifecycle contract | `architecture/logic/table_schema.md`; `architecture/logic/run_api.md`; TASK-002; R1 verdict, validation ledger, data review, and remediation task | PASS |

This checkout has no `.codegraph/` directory, so the repository-required CodeGraph path was unavailable and ordinary source/caller inspection was used.

## Source and consumer coverage

| Concern | Cumulative source and caller evidence | Result |
|---|---|---|
| Five exact schemas | The closed built-in registry contains `vala.drift.observations`, `vala.eval.observations`, `vala.verification.results`, `vala.drift.result_features`, and `vala.eval.result_items` in `crates/vala/vala-bifrost-redux/src/tables/mod.rs:750-762`. Their five owning table definitions match `table_schema.md` in field order, Arrow type, and nullability. `verification_tables_match_their_approved_schemas` pins every authored field and the table-specific Bloom declarations at `tables/mod.rs:899-1069`. | PASS |
| Managed envelope, layout, sensitivity, and retention | All five definitions use `CorrelationPolicy::Observation`; `ensure_managed_columns` appends nullable `run_id`/`card_uid`, non-null publisher/request identity, UTC-microsecond event/ingest time, fixed 16-byte batch identity, row ordinal, and tenant in the approved order (`tables/managed_columns.rs:19-63`). Every table declares daily `wyrd_event_time`; table-specific Blooms are `series`, none, result/subject/binding, and `result_id` for each detail table. `PhysicalLayout::resolve` adds only the managed Bloom floor. Sensitive columns are Eval `context`/`media`, result `details`, and Eval-item `actual`/`expected`/`message`. No verification-specific TTL, cleanup path, or second physical authority was added. | PASS |
| Exact fixed-table startup | `StartClaim::complete` describes Drift and Eval before it can publish `Started` (`wyrd-client/src/observe/lifecycle.rs:166-204`). Both projections call the shared `require_projection`, which compares the complete authored sequence by count, ordinal name, datatype, and nullability and excludes managed columns (`observe/mod.rs:305-345`; declarations in `observe/drift.rs:30-56` and `observe/eval.rs:88-113`). Lazy server describe materializes the canonical built-in through `ensure_builtin` and returns the built-in declaration rather than inventing a client schema (`catalog/bifrost_catalog.rs:1274-1331`). | PASS |
| Fixed-width decoding | The generic queue builder handles any described `FixedSizeBinary(width)` and decodes only exact-width lowercase hex (`wyrd-queue/src/batch_builder.rs:297-349`). It neither pads nor truncates; uppercase, prefixes, malformed characters, non-string values, and wrong lengths fail schema conversion. The focused tests cover nullable 16-byte trace IDs and 8-byte span IDs, byte-for-byte round trip, and malformed/width refusals (`batch_builder.rs:608-675`). Eval projects the typed IDs as canonical hex before this generic path. | PASS |
| Dynamic-table describe cache and producer gate | `Bifrost::writer_table` checks the existing map, acquires one owner-local async miss gate, rechecks, describes once, and inserts into the same map (`bifrost/facade.rs:279-345`). It never mutates the active table. `Observe::record_value` refuses non-`vala.datasets.<name>` destinations before IO, then uses that cached immutable destination (`observe/mod.rs:244-260`). `WriterPool` remains the only producer cache. The barrier-controlled test races eight writes over two FQNs and observes one describe and one producer per FQN (`observe/tests.rs:873-921`). The deliberately global miss gate is the remediation's approved minimal ceiling, not a durability defect. | PASS |
| Publisher and observed-subject identity | Scoped runs construct `Correlation { card_ref: subject, run_id: invocation }` and projected user rows contain neither (`observe/mod.rs:42-109,153-212`). The queue appends only `card_ref` and `run_id` correlation (`wyrd-queue/src/batch_builder.rs:101-179`). Scribe validates every asserted CardRef against the authenticated signed scope before stamping (`scribe/execution_lanes.rs:715-753`), resolves the exact signed member UID rather than trusting a client UID or substituting the principal's root (`:1120-1184`), and separately stamps `principal.id` into non-null `principal_id` (`:819-890`). The identity test covers absent/null correlation, root and secondary scope members, forged UID, malformed, UID-less, and out-of-scope values (`:4007-4107`). | PASS |
| Unknown/denied dynamic describe boundary | Each real Rust, Python, and TypeScript journey now exercises an unknown dynamic table and asserts `WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND`. `denied_describe_is_audited_before_admission` drives the real server with a principal lacking `bifrost_table:read`, asserts the stable denial and one canonical denied describe audit row, and proves the facade cached no table and created no producer (`wyrd-client/tests/pg_bifrost_e2e.rs:2577-2616`). This closes the data-boundary portion of prior FIND-12 without adding another admission path. | PASS |
| Shutdown durability and ambiguous retry | Every successful lifecycle shutdown is terminal; a concurrent start can publish only from `Starting`, and claim drop cannot reopen `Closed` (`observe/lifecycle.rs:121-218`). A failed drain leaves the same `Arc<StartedBifrost>` installed. The state-level ambiguity test enqueues a real projected row through the existing producer, forces a retryable ambiguous sink result, retries shutdown on the same state, proves every attempt and the final receipt use the retained batch ID, then proves writes and restart are closed (`observe/tests.rs:525-597`). Queue admission is still explicitly not a Scribe acknowledgement; successful shutdown/flush is the client durability barrier. | PASS |
| Audit lock-timeout transaction behavior | `freeze_publication_range` sets a three-second transaction-local `lock_timeout`, takes the tenant chain head `FOR UPDATE`, and now propagates PostgreSQL `55P03` through `SqlError` instead of returning `Ok(None)` from an aborted caller-owned transaction (`vala-sql/src/queries/audit_staging.rs:199-287`). `AuditPublisher::freeze` maps that error to `AuditPublicationError::Staging`; the `TenantConn` rolls back on drop, so no commit is attempted and the next sweep retries unchanged state (`wyrd-server/src/audit/publication.rs:340-368`). The Postgres test holds the head past timeout, observes the explicit bounded error, proves no bound was frozen, then freezes the same owed range from a fresh transaction (`vala-sql/tests/pg_audit_staging.rs:274-327`). Watermark, frozen-bound reuse, staging retirement, and the sole Scribe publisher path are unchanged. | PASS |
| Table-module documentation | The Drift, Eval, and Verification table modules and the newly added result/observation files carry intent-bearing module rustdoc; the two previously undocumented `VerificationContract` fields are documented. No suppression or structural refactor was used. | PASS |

## Prior-finding closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-2` | Shared exact ordered-schema comparison plus reordered, extra, retyped, and renullabled startup cases; canonical Drift and Eval startup remains covered. | CLOSED |
| `FIND-TASK-002-6` | Existing Bifrost cache now has one check-lock-recheck miss gate; concurrent same-FQN callers perform one describe and share the existing pooled producer. | CLOSED |
| `FIND-TASK-002-8` | State-level ambiguous shutdown proof preserves and settles the identical retained batch, then observes terminal closure. | CLOSED |
| `FIND-TASK-002-9` | Lock timeout is an explicit SQL/staging error from the aborted tenant transaction; fresh retry sees unchanged durable range state. | CLOSED |
| `FIND-TASK-002-10` | The exact table module/item documentation gaps named by R1 are filled without suppressions. | CLOSED |
| Relevant `FIND-TASK-002-7` | The lifecycle state machine now makes every successful shutdown terminal and fences start/shutdown publication. | CLOSED |
| Relevant `FIND-TASK-002-12` | Real unknown and denied describes fail before cache/producer admission; denial uses the canonical audit path. | CLOSED |

## Verification limits

No test command was rerun during this immutable Wave 1 review. The task records the focused exact-schema, concurrent-describe, terminal-shutdown, ambiguous-retry, and Postgres lock-timeout tests, plus the real denied-describe journey, as passing at `a96820fa`. It also records passing `verify:bifrost` (all nine lanes), `test:shared`, `test:wyrd-sdk`, Python and TypeScript unit/integration/type lanes, codegen, client/PyO3 boundaries, format, lints, and `git diff --check`. Candidate `a000c201` changes only the TASK-002 evidence record after `a96820fa`, so that execution evidence applies to the reviewed source. Static inspection independently confirmed each recorded closure against the cumulative candidate.

TASK-002 establishes the five schemas, physical recipes, observation write path, and lifecycle boundaries. Result/detail publication, same-event-time multi-table writes, physical result Bloom evidence, and multi-partition result pruning remain consumers of later tasks under the approved task decomposition; they are not represented as already implemented here and are not TASK-002 findings.

## Proposed findings

None. No reachable task-scoped data integrity, persistence, durability, schema, or recovery defect remains. No optional hardening or speculative redesign is proposed.

## Overall result

**PASS** — the cumulative candidate satisfies TASK-002's data and durability boundary, and the relevant R1 findings are closed. The five schemas retain one authority and exact physical shape; fixed and dynamic description paths fail closed without creating a second cache or producer owner; fixed-width identifiers remain typed and lossless; publisher and subject identities remain distinct at Scribe's trusted stamping boundary; ambiguous client shutdown preserves batch identity; and audit lock timeout now reports the true aborted-transaction outcome while preserving retryable publication state.
