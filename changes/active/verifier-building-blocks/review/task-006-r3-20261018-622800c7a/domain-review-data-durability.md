# TASK-006 independent data/durability review

**Result: PASS.** No material data/durability finding established in the cumulative task range. This is bounded task acceptance in this domain, not final integrated qualification or retrospective acceptance of TASK-005.

## Immutable subject and boundary

Read source and Git only in `/tmp/wyrd-task006-review-622800c7a`, candidate `622800c7ad18c8b28cb5a2ea84cd568e2a085cb6`, exclusive base `b323327f4cbb690628303d3a6b5249a67af50c9b`. Reviewed the full cumulative diff, including initial implementation f711c6834; latest R2 assets do not replace that audit. No `.codegraph` exists in S. Wrote only this report in R. No builds, migrations, generators, runtime tests, source edits, commits, or peer-review conclusions used. Prior committed verdicts/validation/remediation were closure hypotheses and scope evidence, checked against current source.

Boundary: retired analytical table identities from registration/discovery through describe, SQL planning and native ingest; tenant-bound catalog projection; Forge durable task/claim identity and forward constraint retirement; current verification/drift evidence owners and unrelated namespaces/tenants.

## Authority and source coverage

| Authority | Applicable obligation and inspection |
|---|---|
| AGENTS.md §§2–3, 9–12; architecture/agent-rules.md | Server owns durability; typed SQL capabilities and RLS remain the boundary; do not bypass checks or add compatibility; journey proof and scoped verification. |
| architecture/wyrd-design.md Bifrost/verification boundaries; architecture/wyrd-doctrine.mdx runtime/public surfaces | One SDK/server analytical contract and tenant-qualified physical identity; no independent historical store or replacement runtime. |
| architecture/bifrost-design.md table identity, public surface, Forge scheduling/lease and maintenance boundaries | Closed logical identities, public writes cannot target current server-managed evidence, catalog/physical tenant binding, existing Forge and Scribe owners retained. |
| architecture/references/README.md; languages/spec-driven-development.md, maintainer-style.md, testing-workflows.md; domain/olap-serving.md, analytical-operations-reliability.md | Approved requirements govern implementation; fail closed before IO for unrepresentable tables; preserve durable authority, scoped proof and explicit limits. |
| Approved specification r11 REQ-018–022, INV-008/009, AC-012/013/015; original TASK-006; committed R1/R2 and evidence | Unreleased retirement, claims-before-task deletion, other namespace/current evidence preservation, ordinary unknown-table refusal and no history/conversion/object cleanup. Removed scanner obligations stay withdrawn. |

Actual source expansion beyond the supplied map:

- `namespaces.rs`, `catalog/table_ref.rs`, `catalog/bifrost_catalog.rs` create/register/ensure/list/describe/provider boundaries; `vala-sql/src/queries/olap_catalog.rs` tenant-bound raw-row producer.
- `gate/mod.rs` native dispatch, `resolve_fqn`, write authorization and current result-table reservation; `oracle/mod.rs` SQL relation resolution; server `bifrost/{convert,service}.rs` registration/list/describe consumers.
- All cumulative migration changes and historical Forge task, verification/gateway allowlist, expiration-claim FK/RLS, and prior demand-drop migrations; SQL migration owner `vala-sql/src/lib.rs` and `wyrd-sql/src/lib.rs::MigrationLease::apply`.
- `vala-sql/src/row_types/forge_tasks.rs::ForgeTaskTableIdentity`, durable task/debt queries, Forge `identity.rs`, scheduler promotion and peer-wire conversion callers.
- Current `wyrd-server/src/verification/results.rs`, `scribe_outbox.rs`, SQL execution and baseline owners, plus base consumers of ResultItemsTable to distinguish a registration from a live writer.
- CLI authenticated table-refusal test; Rust/Python/TypeScript retired-contract journeys; MCP stale-registration/catalog/refusal journey; migration fresh/upgrade tests and committed exact execution records; mise SQL/Bifrost task definitions.

## Behavior and invariant matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Retired namespace/tables have no current registration or alias | `namespaces.rs:8–78` removes Eval from enum/ALL/parsers; `tables/mod.rs:1180–1199` removes ResultItems inventory; old table module deleted. No surviving production Eval namespace or table owner in inspected Bifrost/SQL/server boundaries. | Existing inventory refusal test adds result_items/observations; recorded P1b/P4–P7. | PASS |
| Stale catalog entry cannot leak discovery or regain access | `BifrostCatalog::list_tables` (`:1396–1407`) filters raw rows through the existing closed namespace before projection. SQL producer takes TenantConn; transaction remains caller-owned and commits at handle boundary. Describe validates namespace in server service before loading catalog; Oracle resolves relations through TableRef::parse_fqn before providers. | MCP `query.rs:1148–1253` inserts an actual tenant-bound stale result_items registration, asserts it is not listed, compares describe/query against absent namespace, then reads a current table. Recorded P7 GREEN. | PASS |
| Retired writes refuse before Scribe/durable mutation | Gate native dispatch (`:1024–1028`) resolves the FQN before authorization and Scribe dispatch; closed enum cannot carry Eval through typed internal paths. Public registration also resolves namespace and admits only datasets. Current result/input/drift detail and gateway-call reservation remains enforced. | P4–P6 cover writer discovery refusal against absent namespace; Gate source independently proves native refusal. Writer discovery tests alone are not a raw native-write journey. | PASS within stated proof boundary |
| Forward development upgrade removes only obsolete tasks and dependent claims | New `20261016000000_retire_eval_namespace.sql:7–15` deletes claims selected by globally unique retired task ID, then only tasks whose namespace is exactly vala.eval, then narrows the same named CHECK. Other allowed values are identical to previous gateway constraint, including verification/dev/datasets/gateway. | `pg_migration.rs:93–140` restores prior CHECK, seeds retired/current tasks, executes exact migration, checks current survives and future retired insert fails. Recorded P3 and migration module GREEN. | PASS |
| No missing demand-table operation or edited applied migration | Unchanged `20261003000100_drop_forge_planning_demands.sql` already drops demand table before new retirement migration. Cumulative migration diff adds only the new migration. Claim FK in `20260910000025...:333` has no task cascade, explaining deletion order. | Fresh-migration constraint assertion; cumulative diff inspection. | PASS |
| Migration reaches all tenants through its authorized owner | `MigrationLease::apply` sets the operator session before embedded migrator execution; existing task and claim policies admit that owner under forced RLS. No tenant transaction, application raw-pool path, policy relaxation or new bypass introduced. | Source ownership/RLS inspection; existing fixture uses production migration lifecycle before upgrade scenario. | PASS |
| Current/other-tenant evidence and maintenance state preserved | New DELETE predicates affect only retired tasks and their dependent claims. No catalog/evidence/object deletion. Current verification results, inputs, drift details, execution/baseline SQL, outbox write ordering/retry identity retain their owners; SQL changes there are TaskId import moves. | P3 preserves a current task with a distinct tenant ID; P4–P6 query current verification table; MCP/CLI retain current table controls; recorded P9/P10 retained engine/background proof. | PASS task regression scope |
| No new compatibility/history/conversion/GC workflow | No table view, namespace alias, remap, archive reader or object-store deletion added. List filter reuses BifrostNamespace at the exposure boundary; it does not invent a second identity authority. | Cumulative source diff and caller inspection. | PASS |

## Reachability and preservation assessment

The changed SQL allowlist is consumed by durable task projection and Forge peer conversion, while table binding independently requires the same current Bifrost enum. New retired tasks cannot be produced through either current path. Dependent expiration claims must be removed first because the existing task foreign key does not cascade; remaining operation, catalog and evidence records are untouched. Historical demand retirement is already ordered earlier in the migration chain.

I also followed the apparent stale-hot-object concern: `ForgeTaskQueries::tables_owing_promotion` validates every unpromoted file_list identity, so an artificially injected retired file_list row could fail a sweep. Base-source consumers of ResultItemsTable are its inventory/reservation/tests; no current production result-items writer was found. Public writes were reserved and caller registration is dataset-only. The actual current result producer writes verification/drift destinations. A stale development *catalog registration* therefore does not establish a reachable required hot-object producer. This does not justify introducing historical filtering, recovery or cleanup excluded by REQ-021.

## Verification limits and material findings

No material proposed findings; ledger is empty.

Supplied committed evidence records exact P1b and P3–P7 GREEN, migration module 3 GREEN, supporting permissions/engine/contract checks, and retained P9/P10. These are recorded executions, not fresh runs by this reviewer. Source inspection confirms assertions and boundaries; no broad SQL/Bifrost/Verifier aggregate was rerun. Those aggregates remain integrated change-review work.

P3 does not seed expiration claims or assert retained claims; deletion order and preservation are supported by the exact predicates/FK, not dynamic branch coverage. P3's different tenant IDs support scoped deletion but do not themselves test ordinary-role RLS. SDK writer discovery can reject before native ingest; raw Gate behavior is source-proved here. SQL refusal intentionally uses the same QUERY_INVALID_SQL result as a never-existing namespace. P10 proves background handoff/client reconnection, not server crash recovery. P2 lacks historical RED, a separate credential-proof limitation. Nothing here grants TASK-005 an independent PASS.

Final read-only checks: HEAD remained `622800c7ad18c8b28cb5a2ea84cd568e2a085cb6`; `git status --porcelain` empty; cumulative `git diff --check` passed. No source state changed.
