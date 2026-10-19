# Persistent declaration and bundle integrity review

Result: **PASS**. Proposed material finding IDs: **none**.

## Subject and reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
- Base: `1b962bb00`; candidate: `98d8ef46e61ea6aa5dae90d59572467a256a3ef3`.
- Reviewed cumulative TASK-001–003 against approved `changes/active/agent-readiness/spec.md`, revision 2, their original task files, and their recorded verification evidence. TASK-004 is excluded.
- Boundary: persisted declaration operations, request/replay identity, transactional Card/manifest/principal/binding publication, reconciliation consumers, audit projections, graph bundle publication, and runtime admission of complete versus metadata-only bundles.
- HEAD remained the recorded candidate, and the reviewed production paths had no working-tree difference from it at the final identity check. Only this report was written.

## Authority coverage

| Authority | Application |
|---|---|
| `AGENTS.md` §§2–3, 9, 11–12, 15–16; `architecture/agent-rules.md` | Rust server ownership, pure shared contracts, caller-owned `TenantConn` transactions, RLS, one non-blocking audit path, verification and durable-state rules |
| `architecture/wyrd-design.md`, particularly Registry lifecycle and reference binding | Composite declaration, exact UID references, Pending/Active transitions, post-commit storage work, idempotency and expiry |
| `architecture/wyrd-doctrine.mdx` | Declarative Cards; server-owned durable behavior; SDKs project one contract |
| `architecture/references/README.md`; `doctrine/architecture-constraints.md`; `architecture/patterns.md` | Reference routing, single owner reuse, storage/registry and audit composition |
| `languages/spec-driven-development.md`; `languages/maintainer-style.md` | Acceptance authority, immutable review, testable material findings, owning workflows |
| `domain/telemetry-observations.md`; `domain/analytical-operations-reliability.md` | Attribution and acknowledgment distinctions for the renamed audit/readback projections; no new analytical durability claim |
| Approved spec REQ-003/004, INV-001–005, AC-003/005; original TASK-001 scenarios 2–4 | In-place pre-release replacement, unchanged replay/lifecycle semantics, verified complete publication, metadata-only refusal, no compatibility conversion |

The schema is explicitly pre-release and the specification authorizes editing its owning migrations in place. An upgrade migration or historical-data reader would contradict the approved scope. This review therefore does not demand either.

## Source and consumer coverage

| Path/owner | Traced sources and consumers | Assessment |
|---|---|---|
| Pure declaration wire state | `wyrd-spec/src/registry/{submission,enums,ids,mod}.rs`; shared client saga and progress; server routes/mapping/service; SDK receipt projections and fixtures | Renamed types use the same fields and lifecycle states. `Declared` serializes as `declared`; `IdempotentNoop` and `Deduplicated` remain separate. No alternate old-value reader was found. |
| Durable SQL schema | Migrations `20260601000001`, `000006`, `000014`, `000019`, `000020`; `wyrd-sql/src/queries/cards/{declare,lifecycle,version_resolve,mod}.rs` | Operation table, FK, column, unique key, index, RLS policies and reconciliation default agree on the replacement vocabulary. The unique replay key remains tenant + subject + actor + caller key. |
| Declaration transaction and replay | `service.rs::declare_card`, `replay`, `replay_operation`, `DeclarationWriter::write`, `persist_node`, `replay_seed`; SQL lookup/reservation/commit functions; relationship/auth/binding projection callers | Existing transaction ownership and post-commit lifecycle sequencing are retained. All reservation/Card/manifest/relationship/principal/binding/seed writes remain in the writer's one tenant transaction. |
| Post-commit lifecycle | Upload initialization, manifest persistence, completion/activation, abort, reconciliation claim/lease/failure helpers and worker consumers | No renamed SQL field or reconciliation-kind consumer is stranded. Artifact verification and immutable blob persistence still precede activation; failure and cleanup states remain independently represented. |
| Audit | `wyrd-spec/src/vala/audit_detail.rs`; Card route authorization and operation strings; authenticated replay and retained-audit tests | `CardDeclaration`, `Declare`, `card_declaration` and `declare` replace only declaration vocabulary. Audit `Created` stays `Created`. Routes still stage authorization decisions through the existing audit owner; no SQL audit transaction or second writer was introduced. |
| Bundle producer/publication | `cards/handle.rs`; old `hydrate/*` to `pull/{mod,graph,bundle,workspace}.rs` cumulative rename; `cards/download.rs` and shared storage transfer | Existing graph resolver, authenticated artifact downloader and rollback-capable workspace are reused. Metadata is written after all required Card/artifact materialization. A failed transfer returns before publish; a failed promotion attempts restoration and surfaces restoration failure. |
| Runtime bundle reader | `wyrd-spec/src/registry/bundle.rs`; `wyrd-client/src/state.rs::BundleReader`; Python `state/{mod,registry,hydrator}.rs`; Model/Data holder loading; TS public/native Cards and state consumers; Rust SDK consumers | Writer and reader agree on `wyrd/bundle/v1`, `mode`, and `metadata`/`complete`. Runtime admission rejects metadata-only and the retired header. Shared reader continues validating count consistency, exact identities, aliases, inventory equality, graph closure, confined paths, sizes and SHA-256 bytes before publishing its immutable index. Python still loads artifacts through the existing state hydration boundary. |
| Public proofs and fixtures | CLI `card_lifecycle.rs`; SQL `pg_cards_declare.rs`; server `pg_card_declaration_route.rs`; three SDK declare/pull journeys; Python bundle fixtures and CRUD/runtime tests; state and workspace unit tests | Existing real-boundary proof is retained under final names, including corruption refusal, metadata-only rejection, replay attribution and offline runtime loading. |

Repository searches across `crates/` and `sdks/` found no remaining production consumers of `card_registration_operations`, `registration_operation_id`, `RECONCILE_KIND_REGISTRATION`, `CardRegistration*`, `HydratedBundleManifest`, `HydrationMode`, or `card.registration`; the retired bundle discriminator occurs only in its explicit rejection test. Runtime hydration concepts remain intentionally separate.

## Boundary reasoning and acceptance

| Obligation | Source evidence | Verification evidence inspected | Result |
|---|---|---|---|
| Rename persisted operations and their constraints without changing identity/tenancy | Migration `000014`; SQL `declare.rs:158`, `:231`, `:437`; `version_resolve.rs`; lifecycle claim projection | `consolidated_declaration_migration_has_locked_shape`, `subject_actor_creation_and_replay`; reported P3 and boundary checks | PASS |
| Preserve atomic reservation and Card graph writes | `service.rs:1078` opens and commits the connection; `persist_node` at `:1177` composes writes without committing; conflicting reservation drops its transaction before waiting for replay | SQL transaction tests, non-active/cross-tenant no-write tests, authenticated replay tests | PASS |
| Store replay inputs without expiring upload authority | `DeclarationReplaySeed` at `submission.rs:172`; `replay_seed` at `service.rs:1530`; SQL serialization at `declare.rs:437`; `initialize_uploads` rebuilds upload plans from stable seed paths | Source trace distinguishes stored seed from complete HTTP response; authenticated replay verifies stable UID and `idempotent_noop` | PASS |
| Preserve independent lifecycle outcomes and audit durability contract | `replay_operation` changes only write outcome classification; completion reloads current Card state; route authorization precedes service work; audit outbox owner unchanged | Retained-audit assertions count each received declaration decision; lifecycle and denial/conflict cases remain | PASS |
| Complete pull publishes verified bytes; failed pull does not become success | `pull/mod.rs:78`, `bundle.rs:68`, shared verified downloader, `workspace.rs:87` | CLI digest/size/path corruption cases refuse publication; workspace rollback tests; SDK graph journeys | PASS |
| Metadata-only and old formats cannot enter runtime state | `state.rs:1218`, `:1277`, `:1511`; coordinated manifest DTO/fixture changes | `rejects_metadata_only_bundle_as_unhydrated`, `rejects_retired_bundle_api_version`, real CLI metadata bundle → state refusal | PASS |
| Reuse existing owners instead of creating competing persistence/traversal/loading workflows | Base-to-candidate rename comparison for SQL query module, graph/bundle/workspace modules, existing declaration saga and shared BundleReader | Source comparison and call tracing | PASS |

The stored seed is not the final HTTP response: it intentionally omits upload plans, initially records Pending outcomes, and drives later upload/completion work. The renamed enum writes `declared` to fresh seeds; replay changes its in-memory outcome classification to `idempotent_noop`, while completion consults durable Card state. This preserves the existing protocol rather than claiming a single atomic SQL/object-storage transaction.

The bundle writer still delegates byte verification to the shared download/storage path and publishes only after its writes complete. The state reader independently validates on-disk payloads before making typed runtime objects available. Neither the rename nor the removal of per-kind eager `get` introduces an alternate materializer or weakens this admission boundary.

## Findings and proof limits

No material proposed findings. No correction or additional mechanism is requested.

This is a static domain review of the immutable cumulative range, with detailed inspection of the sources and consumers above. Task evidence was treated as reported execution evidence, not independently rerun results. No Cargo, database, SDK or broad verification suite was executed, and no live filesystem-crash or deployment-upgrade experiment was performed. In particular, the named SQL attribution test does not itself assert the serialized committed seed JSON; that projection was established by tracing `replay_seed` → `commit_declaration_operation` → the renamed serde enum and replay decoder, rather than misidentifying HTTP replay output as stored-response bytes.

Unrelated CLI process supervision, documentation rendering, MCP trust policy and whole-repository style acceptance remain with their respective independent reviewers. No new crash-durability, concurrent local destination-writer, mixed-version, migration, or end-to-end exactly-once guarantee is inferred.
