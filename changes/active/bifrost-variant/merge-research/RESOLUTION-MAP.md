# Merge resolution map: origin/main (8a8813a74) → wyrd/bifrost-variant/TASK-003 (cfe21dc90)

Base 5ab92b003. Main is 875 commits ahead. The dry run (`git merge-tree`) found
148 conflicting paths. This map is research only: nothing has been merged. The
target integration branch is `wyrd/bifrost-variant/integrate-main`, published
at origin/main. Each area's full per-file table is in its group report:
`redux.md`, `server-sql.md`, `testing.md`, `sdks.md`, `shared-spec.md`,
`infra-docs.md` and `silent.md`.

## 1. Rule of thumb

- **Take main** for anything main redesigned:
  - `card_uid` correlation (`card_ref` is removed everywhere, including the `wyrd.card_ref` span attribute, which becomes `wyrd.card_uid`);
  - the audit outbox (`ScribeOutbox`; staging, publisher, `append_audit`, `seq`/hash chain and `AuditUnavailable` are all retired);
  - the single-binary test layouts;
  - the story-named SDK journeys;
  - `PyWyrdClient::resolve` and ambient client options;
  - `sql(query, params, ..)`;
  - `QueryResult::to_bytes`;
  - the TS `TableConfigOptions`;
  - the `.claude/skills` symlink.
- **Take ours** for the Bifrost engine work, which main never touched:
  - Variant columns and typed results;
  - per-table time column (REQ-036), with `wyrd_event_time` removed;
  - `RowPreflight` (main's `BatchBuilder` is gone);
  - WAL group commit, fence and ZSTD;
  - Oracle scan sizing and lock-free memory;
  - `WyrdProblem` query terminal (main's `QueryTerminalErrorCode` is gone);
  - conform plans.
- **Combine** wherever both sides touched the same file. Per-file hunks are listed in the group reports.

| Group | Files | Take main | Take ours | Combine | Other | High risk |
|---|---|---|---|---|---|---|
| redux | 31 | 5 | 3 | 23 | — | 10 |
| server-sql | 31 | 9 | 6 | 14 | 1 recompute | 9 |
| testing | 31 | 4 | 4 | 19 | 7 bench files: decision | 6 |
| shared-spec | 24 | 8 | 8 | 7 | 1 regenerate | 6 |
| sdks | 19 | see `sdks.md` | | | 3 regenerate (stubs, `index.d.ts`/`.d.cts`) | — |
| infra-docs | 12 | 4 | 0 | 7 | 1 regenerate (`Cargo.lock`) | 3 |

## 2. Decisions for the human

**Decided 2026-10-11 (human):** Our Bifrost work is kept wherever it conflicts with main.
- **D1:** keep ours (retention window). No active reads and no 503 barrier.
- **D2:** remove `wyrd_event_time`. REQ-036 supersedes REQ-187.
- **D3:** keep our current benchmarks.
- **D4:** `created_at = decided_at`.
- **D5:** a new compaction fork commit (Arrow 60, no REST catalog/Ring).

**Decided 2026-10-11 (human), items 2.6:**
- Delete `CorrelationPolicy::CodeAxis`, and drop `vala.dev.agent_traces` from REQ-037.
- `register_dataset` keeps ours: it returns `RegisterTableResponse` with the outcome decided under the registration lock. This is internal only; both sides share the wire types.
- Task-Verifier results go in a typed `assertion_result` Variant column, not main's generic `details`.
- In the WAL fence lookup, `scribe_wal_newest_fence` copies main's SECURITY DEFINER operator raise/restore pattern.
- Python `QueryTerminal.error_code` returns the catalog code.
- Eval observation key time uses `created_at`.

The agents' original analysis follows.

The agents' recommendation is shown for each. **[split]** marks a decision where the agents disagree.

1. **Snapshot-expiry model (D1).**
   - Ours: REQ-035 retention window, supersessions, read-only cached `read_cut`.
   - Main: active-read registration, `UnresolvedExpiry`, and a 503 barrier on `acquire_active_cut`.
   - Recommendation: keep ours, port main's lease-bounded expiry (keep the claim on an unknown outcome), drop active reads, and drop the 503 barrier.
   - Precondition: confirm that expiry reconciliation never deletes objects reachable from the current snapshot.
   - Main's `snapshot_expiration` tests get rewritten.
   - `bifrost-design.md` and `forge.svx` follow the decision.
2. **Event time: REQ-036 supersedes main's REQ-187 (client-stamped `wyrd_event_time` plus the admission window).**
   - Drift and eval keep client emit time through `created_at`.
   - Open: should `observe.record` and verify rows carry a client time in the table's declared column, or rely on `wyrd_ingested_at` / `ended_at`?
   - Both specs need a revision note.
   - This also decides whether `WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE` survives.
3. **Capacity benches [split].**
   - infra-docs recommends keeping ours (`bench:bifrost:query-capacity`, `write-capacity`, `client-queue`) as engine microbenches beside main's `bench:capacity`.
   - testing recommends taking main's deletion (main REQ-010) and folding the fixtures into `bench:capacity` later.
   - The choice decides whether `workload.rs` comes back and whether the bins get ported to `release_server`'s `Placement` / `cpu_usec` API.
   - My view: keep ours. Our current perf loop and the 30 MiB/s target are measured on them.
4. **Audit time.**
   - Main's outbox `project_audit_event(event, decided_at)` uses the server's `Utc::now()`.
   - Ours makes `created_at` the audit time column, with Variant `detail`.
   - Recommendation: `created_at = decided_at`, treated as domain event time. Otherwise it conflicts with AGENTS.md §15 ("PostgreSQL owns coordination timestamps").
5. **The compaction fork (prerequisite, external repo).**
   - No `iceberg-compaction` revision has both Arrow 60 (ours 696560602) and no REST catalog / Ring (main 4e9f325, required by main's `check:deps`).
   - A dry merge in the fork is clean (tree ab95e99b).
   - A new fork commit must be pushed before the Wyrd merge can build.
   - The alternative is a time-boxed Ring-ban exception.
6. **Smaller decisions**, each with the agents' recommendation:
   - Delete `CorrelationPolicy::CodeAxis`: yes.
   - Eval observation key time: use `created_at`.
   - `register_dataset` return type: ours' `RegisterOutcome` vs main's `TableUid`. Not wire-visible: both sides define the same `RegisterTableResponse`.
   - Task-Verifier results: main's generic `details` column vs a typed `assertion_result` Variant column. No recommendation.
   - WAL fence lookup: SECURITY DEFINER plus main's operator raise/restore pattern, or `OperatorPool`.
   - Python `QueryTerminal.error_code`: becomes the catalog code (`WYRD_VALA_500_…`). This is a public Python property.
   - Spec REQ-037: drop `vala.dev.agent_traces`, which main deleted.

## 3. Silent conflicts: clean merges that break

**Boot**
- Our migrations `20261009`, `20261010` and `20261011` GRANT to `wyrd_app`, which main removed, so migrate fails.
- `forge_snapshot_supersessions` lacks the `operator_access` policy, so `verify_row_security` refuses boot.
- `scribe_wal_newest_fence` (SECURITY DEFINER) never raises `app.operator`.

**Compile**

| Main file | Uses a symbol ours removed |
|---|---|
| `query/collect.rs` | `QueryTerminalError` / `terminal_error_to_bifrost` |
| `scribe_outbox.rs` | `ObjectStorePutFailed` |
| `forge/snapshot_expiration.rs` | `ActiveReadOwner` |
| `owner_inspection.rs` | `StorageLifecycle` |
| `scribe/describe.rs`, `eval_verification.rs` | `BatchBuilder`, `wyrd_queue::schema` |
| `observe/tests.rs` | `WYRD_EVENT_TIME` |

Signature and API changes:
- `json_schema_to_arrow` moved to `wyrd_types`. Callers: wyrd-client `table.rs`, TS native, Python `record`.
- `superuser_pool()` is now sync.
- `CreateTableRequest.audit` was removed (5 `audit: None` sites).
- The capacity bin uses the old `LocalServer::start(.., Envelope)` and the 3-argument `Capture::start`.

**Semantics**
- Main's stamping of `wyrd_event_time` merges cleanly into wyrd-client `facade.rs`, `handle.rs` and `observe/mod.rs`, `observe_ingest.rs` and `capacity/load.rs`. That is 24 references, plus 4 docs pages.
- The eval observation hook reads `key.event_time` / `receipt_micros`.

**Orphaned tests**
- Ours' `wyrd-client/tests/bifrost/*.rs` and `tests/pg_bifrost_e2e/variant_tables.rs` sit outside main's `tests/integration/` binary, so they compile into nothing and pass silently.
- Ours' mise `--test pg_bifrost_e2e` selector and the CI regex go stale.
- Ours' TS `tests/support/test-server.ts` duplicates main's `serverTest`.

**SDK call shapes**

| Language | What breaks in our tests |
|---|---|
| Python | `_layout_json` arity (3 vs 4); `TableConfig.from_arrow(table, schema)` argument order; positional `sql(q, Model)` must become `model=`; the removed `server_url=` / `credential=` arguments |
| TypeScript | `sql(q, rows)` must become `sql(q, [], rows)`; the layout must move into the options object; the removed `serverUrl` / `grpcUrl` arguments |

**Lints and toolchain**
- Main's `[lints] workspace = true` applies pedantic lints, `unwrap_used` and `allow_attributes_without_reason` to all our new wyrd-testing code.
- Ours' reasonless `#[allow(needless_pass_by_value)]` attributes must go.
- The Rust pin differs: ours 1.98.1, main 1.97.1.

**Coverage to port, not drop**
- The Variant late-failure query stories in Python and TS: the only proof of FIND-003-3.
- The Variant context/media assertions in the observation-surface stories.
- Ours' journey submodules, moved into the integration binaries.

## 4. Procedure, once the decisions are made

1. Push the compaction fork commit (decision 5).
2. On `integrate-main`, run `git merge --no-ff --no-commit origin/wyrd/bifrost-variant/TASK-003`, which merges ours into main.
   - The three forbidden paths are not in the branch tree, so they stay out.
   - Resolve group by group, in this order: shared-spec, redux, server-sql, sdks, testing, infra-docs. Contracts go first because everything compiles against them.
3. Fix the silent conflicts above. Even though git reports no conflict on those files, each one gets an explicit edit.
4. Lockfile:
   - Start from ours' `Cargo.lock`.
   - Run plain `cargo metadata`, then hakari `generate` and `manage-deps`, then `cargo metadata` again.
   - Never run `generate-lockfile`.
5. Regenerate, never hand-merge:
   - `mise run codegen:regen` (py stubs, schemas, error codes);
   - `mise run ts:build` (`index.d.ts` / `.d.cts`);
   - the `vala-sql` `olap_minimal` checksum.
6. Commit as one merge commit. Make follow-up fixes as separate commits, so each resolution can be reviewed.

## 5. Proof lanes

Run in this order:
1. `mise run fmt`, then `lints`.
2. `check:deps`, `check:client-tier`, `check:pyo3-scope`, `codegen:check`.
3. `test:sql`: covers migrations and RLS boot.
4. `test:bifrost`: every tier and language surface, including the Variant journeys.
5. `test:shared`, `test:wyrd`, `test:principals:integration` (OpenAPI).
6. `py:test:unit`, `py:typecheck`, and Python integration.
7. `ts:typecheck`, `ts:test:unit`, `ts:test:integration`.
8. `docs:check`.
9. `mise run gate`: this merge is broad enough to warrant it per AGENTS.md §11.

Then re-bench x1 and x64 OTLP, and profile to confirm that fix 1's
logical-identity reuse now hits once `card_ref` is gone.

**Unknown until compiled:**
- `IngestTransport` signatures;
- `forge_operations.rs` consistency;
- whether the `wyrd_testing::profile` API matches the capacity bin.
