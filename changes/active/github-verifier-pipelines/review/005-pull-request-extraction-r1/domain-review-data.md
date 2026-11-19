# Domain review: persistent data and tenancy (TASK-005 r1)

Reviewer: `domain-rev` (data/tenancy boundary), independent and read-only.
Subject: base `b262696af`, candidate `79a7f985e` (HEAD verified `79a7f985e`).

## Verdict

**PASS.** No material findings.

## Reviewed boundary

- `crates/wyrd/wyrd-sql/migrations/20261115000000_pull_request_source_extracted.sql`:
  the new column, its CHECK, and the replaced `wyrd.complete_verifier_executions`.
- `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs`: `COMPLETE_SQL`,
  `COMPLETE_MANY_SQL`, `complete`, `complete_many`, and `Completion`.
- `crates/wyrd/wyrd-sql/src/queries/verifier_executions/pull_requests.rs`:
  `BASELINE_SQL`, `PullRequestBaseline`, and `pull_request_baseline`.
- `crates/wyrd/wyrd-server/src/verification/{runner,engines}.rs`: how
  `pr_source_extracted` is derived and passed through `Transition::Complete`,
  both the in-memory completion and the direct `queue.complete` path.
- `crates/wyrd/wyrd-server/src/verification/hosted/blocks.rs`: the extractor
  route and its `CallProvenance::Extract` stamp.
- `crates/wyrd/wyrd-server/src/components/github/extraction.rs`: `history`
  and the error mapping (`connection_failure`, `pipeline_failure`).
- `crates/wyrd/wyrd-server/src/components/github/review_cache.rs`: the
  `CodeReviewMetadata` group commit (`Worker::run` and `commit`, `write`,
  `read`).

## Authority and source coverage

- `crates/wyrd/wyrd-sql/AGENTS.md`: connection types, transaction
  ownership, no manual tenant predicate, `SECURITY DEFINER` rule, and
  migration immutability.
- `AGENTS.md`: the Time and Audit locked decisions.
- Spec revision 8: the REQ-006 baseline paragraph, REQ-017, and INV-001,
  INV-002, INV-004, INV-005, and INV-007.
- Task 005: the "metadata batching and group commit" and "extracted-source
  baseline eligibility" revisions, plus the X4 and X10 Evidence rows.
- `architecture/bifrost-design.md`: the new custody bullet. It calls the
  column coordination, not evidence, which is consistent with INV-002.
- `architecture/operations/deployment-and-release.md`: expand-and-contract
  and overlap rules.
- `scripts/postgres/test-database-setup.sql`: the grants posture.
- `wyrd.current_tenant()` and the `tenant_isolation` policy lineage of
  `wyrd.verifier_executions`.

## Trace results

**Migration immutability.** The range adds exactly one migration (`A`) and
edits no existing migration. Its timestamp sorts after the latest existing
migration (`20261114000000`).

**Column and CHECK.** The column is `NOT NULL DEFAULT false`, so existing
rows stay ineligible, as the task requires. The CHECK is
`NOT pr_source_extracted OR (origin='pull_request' AND status='completed')`.
No statement moves a row out of `completed`:

- `CANCEL_SQL`, `CANCEL_PRINCIPAL_SQL`, `SUPERSEDE_SQL`, `DEACTIVATE_SQL`,
  and `CANCEL_REPOSITORIES_SQL` only change `pending`, `retrying`, or
  `running` rows.
- `RETRY_SQL`, `TERMINATE_SQL`, and `RELEASE_SQL` are fenced on
  `status='running'`.

So the CHECK cannot be tripped by a later transition. The only true value
comes from `engines::pr_source_extracted`: a succeeded step whose provenance
is `Extract { version == PULL_REQUEST_EXTRACT_VERSION }`. That step returns
`Terminal` before acquisition unless the execution carries a
`PullRequestSource`, and the runner sets one only for
`ExecutionInput::PullRequest`. A non-PR origin therefore always records false.

**Replaced function.** Compared with the 3-argument function in `20261022000000`:

- Same `LANGUAGE plpgsql VOLATILE SECURITY INVOKER SET search_path = ''`.
- Same `pg_catalog.` qualification and `#variable_conflict use_column`.
- Same `ORDER BY execution_id` lock order, which stays consistent with
  `renew_verifier_leases`.
- Same lease fence: `lease_token`, `origin <> 'group'`, and `running` or an
  identical completed `result_id`.

Neither version has explicit GRANTs. Both rely on PostgreSQL's default
`EXECUTE` to `PUBLIC` for `wyrd_tenant`, so grants are unchanged. Because the
function is `SECURITY INVOKER`, RLS on `wyrd.verifier_executions` still
applies. The arrays are built from one `completions` iterator in
`complete_many`, so their lengths always match.

**Lease fencing, replay, cancellation, and rollback.**

- `COMPLETE_SQL` and the function write the fact in the same fenced UPDATE
  as `status`, `result_id`, and `settled_at`.
- A cancelled row has no lease and its status is not `running`, so nothing
  is written.
- A replay re-applies only when `result_id` is the same. That value comes
  from the same retained `Completion`, so it cannot differ.
- The SQL tests pin stale lease, rollback, and fenced completion after
  cancellation, for both single and batched completion:
  `stale_or_rolled_back_completion_records_no_extracted_source` and
  `batched_and_single_completion_record_extracted_source_alike`.

**BASELINE_SQL semantics.**

- It reads only `completed` rows with `pr_source_extracted` set.
- It matches the stable repository identity: host, provider `repository_id`,
  and number, which satisfies INV-001.
- It requires a strictly earlier `pr_observed_at`, so sibling executions of
  the same generation are excluded.
- It scopes candidates by Verifier kind and name, prefers the exact
  `verifier_card_uid` (same policy), and otherwise reports `PolicyChanged`.
- It has no manual `data_tenant_id` predicate. RLS scopes both the `current`
  CTE and the candidate join, and
  `pull_request_baseline_is_tenant_isolated` pins this.
- A missing head on a same-policy row is a decode error. `history` maps
  every read error to `Retryable`, never to `FirstReview`, which satisfies
  X4 and INV-004.

**Connection types and transactions.**

- `pull_request_baseline` takes `&mut TenantConn<'_>` and never commits.
- `history` opens one `TenantConn` through `registry_tenant_conn` and
  commits before any GitHub or object-storage IO.
- `CodeReviewMetadata` reaches the database only through
  `ServerPostgres::tenant_conn`.
- Raw `sqlx::PgPool` and `Transaction` appear only in tests.

**Group commit.**

- Writes are sorted, then split per tenant, so one transaction holds exactly
  one tenant.
- A tenant always hashes to the same worker.
- `apply` runs SQL only. There is no object or GitHub IO inside the
  transaction.
- Replies go out after `commit()`.
- Any error answers every write in the batch with that error, and the batch
  is rolled back when the conn drops uncommitted. There is no per-write
  fallback, as the task requires.
- Pending work is bounded (`PENDING`=256 per worker), as are batch size
  (`BATCH`=64) and workers (`WORKERS`=4).
- `recv_many` returns at once with whatever is queued, so a lone write does
  not wait.
- An ambiguous commit is documented in the `# Cancellation` section of
  `write`.
- None of the batched write statements raises an error as part of normal
  operation. The inserts use `ON CONFLICT`, and the conditional publish
  returns a flag. So batching does not turn an expected per-write result
  into a shared failure.

## Non-material observations (no finding)

These were recorded for context. None meets the materiality bar.

- **Rolling deploy.** The migration drops the 3-argument function in place.
  Released `v0.4.1` calls `complete_verifier_executions($1,$2,$3)`, so an
  old replica running alongside this schema would fail batched completion.
  `deployment-and-release.md` allows overlap only when both release
  manifests accept each other; otherwise the rollout fails closed. The doc
  also says no image has been published. `20261024000000` sets the same
  precedent (`DROP FUNCTION wyrd.verification_target`). So no
  supported-use regression is shown.
- **Positional mapping.** `BASELINE_SQL` decodes into a positional tuple
  `(Uuid, Option<String>, bool)`. The crate rule says "Map columns by name".
  The owning file already decodes `COMPLETE_SQL` and others the same way,
  and the columns have distinct types, so a reorder fails to decode rather
  than misreading data. This is pre-existing local drift with no
  consequence shown.
- **Shared workers.** Tenants share the 4 metadata workers by hash. A
  tenant whose batch waits on a row lock or a pool slot delays the other
  tenants on that worker. This is the bounded-worker design the task
  approved. Transaction isolation between tenants holds, and no SLO
  obligation is violated.
- **CHECK validation scan.** `ADD CONSTRAINT ... CHECK` without `NOT VALID`
  scans `wyrd.verifier_executions` under the `ALTER TABLE` lock.
  `20261114000000` does the same, and every existing row is trivially valid
  (`false`).

## Verification limits

- I did not run EXPLAIN on `BASELINE_SQL` under the tenant login with
  realistic history volume.
- The partial index `verifier_executions_pull_request` (`data_tenant_id`,
  host, `repository_id`, number, `pr_observed_at`) matches the query's
  expressions.
- One planner risk is unmeasured. Under forced RLS, the non-leakproof jsonb
  `->>` comparisons may not be promotable to index conditions ahead of the
  policy qual. Index use could then narrow only by `data_tenant_id`. This
  applies equally to the existing `SUPERSEDE_SQL` and `DEACTIVATE_SQL`. It
  is a measurement gap, not a proven failure.
- I did not rerun the Tier 2 journey (`pg_github_pull_request_extraction.rs`).
  Its X4 claims (P/A/B, unreadable history, recovery keeps the fact) were
  checked against source only.

## Tests run

- `timeout 590 scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner >/dev/null 2>&1; mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(/pg_verifier_executions/)"'`
  → 40 passed. This includes all six pull request baseline and
  extracted-source tests.
- The same harness with
  `cargo nextest run --locked -p wyrd-server --test integration -E "test(/pg_code_review_cache/)"`
  → 8 passed. This includes the lone-write, shared-commit-per-tenant, and
  failed-batch tests.

## Proposed findings

None.
