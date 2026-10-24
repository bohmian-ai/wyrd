# TASK-003 immutable review subject

- Repository: `/Users/stevenforrester/Documents/GitHub/schema-change-task-3`
- Base: `ca2ac4a37` (resolve with Git); candidate: `d1d02cced07582a8f817d3f5651e7895b96d046d`.
- First review; complete `git diff ca2ac4a37...d1d02cced`, 328 files. No prior TASK-003 acceptance evidence carried forward.
- Approved authority: `changes/active/service-identity-and-telemetry/spec.md`, revision 16, and its three supplements. Original task: `tasks/TASK-003-system-schemas-and-attribution.md` relative to the change packet.
- Initial worktree clean; no `.codegraph/` directory. Source must remain unchanged; only reports may be written here.
- Read repository and applicable nested rules: root, Python SDK, TS SDK, docs, SQL. Also `changes/spec-driven-development.md`, `agent_docs/index.md`, relevant guides, architecture authority map and owning sections, and `TESTING.md`.

## Common navigation map

This is a starting map; expand against the actual diff and callers. It is not a coverage conclusion.

| Path/owner | Caller/consumer and proof starting points |
|---|---|
| Redux catalog, namespaces, TableRef; spec permission_scope | SDK/HTTP/MCP/CLI queries, grants, SQL metadata; catalog scenario |
| Redux tables/managed_columns, tables/signal, nine table ledgers and projections; spec vala managed/API | Native and OTLP ingress, describe and layout tests; telemetry-schemas supplement |
| Redux gate/{attribution,auth,mod}, scribe/{ingress,preprocess,persistence,replay,wal} | Server HTTP OTLP and gRPC capture_peer; public attribution/refusal and replay proofs |
| Server scribe_outbox, audit, verification results/groups/drift; registry row_dimensions | Per-row origins, direct/queued verification, audit and peer delivery; server verification_runtime tests |
| Server components/gateway/{ingress,invocation,batches,capture,caller,routes} | Provider attempts, verification callers, public adapters; gateway compatible/native/peer and Python/TS gateway journeys |
| Redux oracle, parquet, forge and vala-sql batch commits/file list | Hot/recovered/published/compacted canonical rows; distributed lifecycle test and origin tests |
| Shared client bifrost/observe, queue, telemetry; SDK Rust/Python/TS | Public declarations, generated parity, native validation, buffer origins; SDK OTel and query journeys |
| Architecture, runtime examples, docs literals and mise | Catalog closure, mandatory gates, no new docsite feature |

## Reviewer selection and scope

- behavior-rev: comprehensive task acceptance and supported caller paths across all seven scenarios.
- invariant-rev: complex shared row identity, schema/projection and producer-to-sink lifecycle invariants.
- repo-rev: broad cross-tier/language mandatory repository rules, including changed-item rustdoc and nested authorities.
- maintainer-rev: changed attribution/context owners and abstraction reuse, duplicate workflow investigation.
- domain-rev-security: public ingress trust, graph/tenant permissions, provider header boundaries.
- domain-rev-persistent-data: persisted schemas, layout, atomicity/idempotency and exact lifecycle values.
- system-rev: outbox/replay/peer failure propagation and recovery; scheduled once a discovery slot is free.
- Fresh ponytail-rev: independent validation after all selected discovery reports; never an implementation author or discovery reviewer.

Each agent owns only its assigned report. No source edits, commits or broad test suites. Use existing mise tasks and narrowly focused checks if needed. Task implementation evidence is available verification evidence, not an acceptance conclusion. Independently challenge its diagnoses and source claims. Reports must state verification actually run versus recorded evidence. Discovery reviewers do not receive other reviewers' conclusions. Finding materiality and correction burden follow the invoked task-review skill.

## User clarification during review

The user explicitly requires a clear ownership flow across Bifrost, gateway and verification: resolve originating execution context once through one shared set of logic, then carry accepted values through projection, batching, hashing, delivery and retries. Subject account, actor account, executing Service workload and observed Card remain distinct. Delivery workers do not become owners. This is recorded separately from independently discovered findings and does not receive a retroactive FIND ID. Independent validation must reconcile the correction with existing shared ownership selection and accepted-context owners, avoiding several local field replacements or a new parallel ownership engine. Review remains read-only for source.
