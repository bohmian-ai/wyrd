# Tenancy/security domain review

**Result: PASS. Proposed findings: none.**

## Subject and limits

Immutable candidate `8955e75b71ded9d39daf7649a0c985be0803e266`; correction parent `1a66d8a7b4583c9798c2f1573dc6ab1f3eadb027`. Cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`; TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. Reviewed original tasks, spec revision 20 REQ-015/INV-002/INV-006/AC-017, prior R5 verdict, R6 remediation/evidence, cumulative changes and current producer/consumer source. No CodeGraph directory exists. Static review only: no build, test, benchmark, mise, cargo, nextest or commit; supplied execution results are attributed evidence, not rerun results. Only this report is written.

Standing decisions are preserved: FIND-007-3 unchanged; Postgres tenant columns excluded; `WYRD_VALA_500_QUERY_TENANT_INVARIANT` retained; no live-read cap; shutdown does not publish staged residue. The expressly permitted TASK-006 benchmark synchronization delta does not change tenant authority, projection, storage identity or audit ownership; capacity correctness is independently assigned to the concurrency reviewer.

## Authority and source coverage

| Boundary | Governing authority | Source inspected and conclusion |
|---|---|---|
| Authenticated tenant and physical identity | AGENTS.md §§2/3/9/10; agent-rules SQL/audit rules; wyrd-design identity/Bifrost ownership; wyrd-security-posture peer identity and data isolation; bifrost-design physical model | Scribe `parquet_writer.rs:320–412`, `staging_runtime.rs:700–735`, `claim_assembly.rs:235–285`; Forge `managed/executor.rs:180–215,265–300`, `managed/policy.rs:250–278`; footer identity is stamped from the existing tenant/table binding, with seal/binding equality checked before generation encoding. |
| Footer proof before rows | Spec REQ-015/AC-017; bifrost-design:85–97; references/domain/olap-serving.md and datafusion.md | `parquet/footer.rs:35–52,107–127,152–168`; `oracle/exec.rs:978–1055,1115–1149,1423–1465,1550–1580,2915–2995`: duplicate, absent, valueless and foreign values refuse; hot/staged proof precedes reader construction/pruning/decode, published proof precedes handing metadata to the reader. |
| Metadata caches and sibling scan tiers | Security posture tenant-qualified caches; REQ-014/015 | `storage/cache.rs:185–292`, `oracle/follower.rs:530–575,590–625,775–835`, `oracle/mod.rs:3038–3043`: keys include authenticated tenant/table and immutable-object identity; hits and misses both reach the proof, so cached metadata is not a proof bypass. Published loader is mandatory and encrypted key-metadata tasks are refused rather than opened through a bypass. |
| Peer assignment authority | Security posture receiver-owned context; spec INV-002/INV-006 | `oracle/follower.rs:1560–1625`; `wyrd-server/oracle/peer_service.rs:285–328`: assigned tenant equals authenticated tenant and table binding, with role/reservation/digest/fence checks before source IO. Staged metadata keys derive their expected tenant from that assignment; hot keys derive it from signed binding. In-memory rows retain seal-key authority rather than a row-column filter. |
| Remote refusal and terminal mapping | Spec tenant faults fail closed; bifrost-design read terminal contract; languages/errors.md | `peer_service.rs:641–700`; `oracle/dispatcher.rs:1778–1822`; `oracle/live.rs:625–650`; `oracle/exec.rs:1159–1175`; `oracle/mod.rs:4306–4337`: typed refusal becomes private `Aborted`, stays `TenantInvariant` at stream-open and already-open stream classification, is excluded from availability loss, and maps to the stable public tenant error. |
| Audit attribution and cardinality | AGENTS.md canonical nonblocking Oracle audit exception; agent-rules; bifrost-design:569–578 | `oracle/mod.rs:2418–2423,2456–2460,2474–2519`; `wyrd-server/oracle/query_audit.rs:1–170`: both query classes install one leader wrapper before stream polling; first refusal consumes its captured authenticated context once. Canonical server owner appends through tenant connection and `audit::append_on`, with tracked, bounded nonblocking work. Worker tenant proof creates no parallel audit sink. |
| Row-column deletion and docs | REQ-015; current per-file authority; excluded Postgres boundary | `wyrd-spec/vala/managed_columns.rs`, redux schema/tables managed columns, Scribe memtable/execution lanes, deleted provider filter/tripwire and codec changes; current design/docs use footer proof. No restored per-row tenant projection or compatibility fallback found. |

Reference routing also included architecture-constraints.md, spec-driven-development.md and maintainer-style.md for ownership and scope. Current explicit task/spec and Bifrost per-file authority govern the reviewed replacement; fixed maintainer decisions are not reopened by older descriptions.

## Producer-to-consumer assessment

The expected tenant is never taken from the file being checked. Generation encoding compares the frozen seal tenant with the tenant/table binding and stamps its resulting `ArtifactPlan`; staged claim assembly takes `context.binding.tenant`; Forge rewrite properties take `self.binding.tenant`. Oracle binds each file to the authenticated query or signed follower assignment. The shared scan compares that independent value with the footer before the Arrow reader can decode or project rows, including a zero-column aggregate and an otherwise-pruned row group. Footer duplication is refused rather than selecting one value. A missing published loader and encryption metadata are fail-closed paths, not a compatibility route.

For the remote Scribe case, the proof runs when the follower stream is polled. `scribe_stream_error` retains the tenant class, `dispatch_status` emits `Aborted`, and `stream_status_error` preserves it after schema delivery. `is_availability_loss` does not include it. The leader receives a typed refusal through `live_error`, so it neither omits the source as a degraded success nor loses the first-refusal audit attribution.

The latest R6 changes at the four named documentation sites change only Markdown and Rust comments/rustdoc. The diff contains no changed executable statement, assertion, test name, tenant key, enum variant, metric label or binding. Corrected shutdown wording assigns residue retention/restoration to the existing shutdown/restart path and residue publication to explicit flush/ordinary tick; it cannot weaken the reviewed tenant proof.

## Verification evidence assessment

Source-confirmed focused tests cover missing/foreign/duplicate footer metadata, hot-file refusal before rows, missing mandatory published loader, and preserved already-open stream tenant status. `distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads` checks zero-row `QueryTenantInvariant` under aggregate/predicate execution. `distributed::remote_staged_footer_refusal_fails_closed` at `tests/bifrost/oracle/distributed.rs:1860–1937` rewrites a sealed remote run, then checks zero rows, Failed, exact tenant terminal code and one leader `tenant_file` event. Its metric assertion establishes refusal dispatch cardinality, not successful durable audit persistence; the latter is owned by the inspected canonical audit implementation and its existing proof.

TASK-008/R5 packet supplies focused/module/tenant-journey results and exact remote-footer/classifier evidence. No new runtime evidence is required to establish that R6 preserves these boundaries because its task edits are comments only. This review claims static correctness within the authorized scope, not fresh execution or new fault-injection qualification.

## Findings

Empty proposed finding ledger. No new tenancy/security blocker found. R6 changes do not reopen prior tenant-refusal closures or alter any executable tenant-isolation boundary.
