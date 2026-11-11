---
id: TASK-003-R2
status: ready
approved_spec_revision: 5
original_task: TASK-003
findings: [FIND-003-3, FIND-003-6, FIND-003-7, FIND-003-8]
---

# Fix Repository declaration lock ordering and remove superseded code

An exact-version retry of a Pending Repository can deadlock with its activation. Fix the acquisition order at the existing declaration/completion owners, remove obsolete intermediate-version support, and finish the small Rust/SQL rule corrections in the same change. Preserve the working identity, deletion, readiness, recovery, rollback and origin-inference behavior.

Route to `$wyrd-implement`. This task requires no new migration, public behavior, owner or specification decision.

## Acceptance context

- Approved spec: `changes/active/github-verifier-pipelines/spec.md`, revision 5; adjacent `contracts.md`; `design/approved-declaration.md`.
- Original task: `changes/active/github-verifier-pipelines/tasks/003-repository-declaration.md`.
- Original base: `5f13d86aeacf1a7a1ad0c192b093aa6c1b1f79cc`.
- R1 candidate: `ca76d3a50c83e119e917a746db67b5a64b1e8f2b`; current reviewed candidate: `607f5192975159fd4f7235b886c90d2a5979cba0`.
- Keep the original task, R1 verdict/validation/remediation and this directory's amended `verdict.md` and `findings-validation.md` as review context. Source locations below describe the current candidate.
- Apply root/SQL AGENTS, the relevant Rust/maintainer guides and architecture Repository/Registry lifecycle rules. Preserve unrelated dirty files.

The user confirmed that nothing in this change packet has been deployed. Both Repository migrations are new in this feature; supporting data written by an intermediate feature commit is outside scope. FIND-003-4 is closed by the implemented runtime occurrence fence. Its withdrawn legacy-upgrade proposal creates no implementation or testing work.

## 1. Correct the shared lock protocol — FIND-003-3

**Cause and user impact.** `components/cards/service.rs::persist_node` (:1572–1583) takes the Repository family advisory lock before holding a reused Pending Card. Its exact-pin lookup (:1867–1885) can read that root without a row lock. Completion (:2797–2822) takes the root row lock first, then its waiting operation locks and the family lock (`queries/cards/repository.rs:306`).

For a metadata-only root Pending after blob failure, an identical exact retry can hold the family while activation holds the row. Each then waits on the other. PostgreSQL aborts one transaction and the request or recovery receives registry503. This is a lock inversion introduced by the readiness fix, violating supported identical reuse (REQ-003/O-23). Existing reused-Verifier tests do not exercise a member whose own family line is the selection lock.

**Selected correction.** At the existing writer/persist boundary, obtain the reused exact Pending Repository row hold before acquiring its Repository family line; then resolve/revalidate under the canonical family lock. Reuse the existing lookup/hold and version owner. Move the protection rather than retaining a duplicate late hold for that root. Apply the same acquisition rule to all exact Repository reuse paths through this owner.

Keep the required responsibilities together:

- The hold makes uncommitted declaration membership visible before an activation can miss it.
- Persisted exact outcomes determine readiness, including reused members created by another attempt.
- Request completion, already-Active replay and background recovery use the shared atomic completion/selection boundary.
- Selection conflicts terminate the attempt without undoing valid Card activation or registered dependencies.

Preserve non-root Pending holds, external Active-reference checks, operation-before-family settlement, lease fencing and selection in the completion commit. Removing synchronization, adding retry guards or moving selection after commit would leave the underlying invariant unresolved.

**Acceptance.** Add one deterministic real-Postgres regression at the existing declaration/activation owner. Coordinate exact Pending-root retry against final activation, covering writer-first and activator-first acquisition. Both operations finish without deadlock/registry503; the root is Active and selected/reused outcomes are correct. Assert observable outcomes and the corrected ordering. Keep existing reused-member readiness, atomic rollback, recovery and terminal-conflict proof green. Use the existing fixtures and synchronization facilities rather than a general concurrency framework.

## 2. Remove stale intermediate-version support and explanations

This is user-directed scope cleanup accompanying the lock correction, not an additional finding or compatibility project.

- In the unshipped `20261112000000_repository_selection_occurrence.sql`, remove the two legacy backfill UPDATEs and their historical-data comments. The earlier feature migration adds nullable `selected_at` and `selection_expected_uid`, so neither backfill matches pre-feature rows when the complete feature migration set is applied before serving. Retain the occurrence columns and nullity constraint. Retain runtime UID/occurrence comparison, operation-ID occurrences, deselection/deletion clearing, rollback and identical-target behavior. Add no migration, legacy invalidation, provenance recovery or historical-upgrade tests.
- Remove stale backfill descriptions from the affected occurrence documentation.
- Correct `settle_selection`'s outdated explanation that it performs ordinary already-ready write settlement; the writer now settles that before its own commit. Keep the request wrapper and its callers: they expose a stored selection conflict as public409. Shared atomic settlement deliberately records that conflict while allowing valid activation to commit. These responsibilities remain distinct.
- Remove the superseded late root-hold acquisition when moving it under section 1, and update the existing lock-order comments. Do not leave alternative ordering or another readiness guard behind.

**Acceptance.** The complete feature schema applies to a fresh/pre-feature database using the existing migration runner. Runtime rollback/replay tests remain green. No legacy-only backfill, new corrective migration or intermediate-schema test remains. Required membership synchronization and request-side conflict reporting remain present.

## 3. Finish the bounded rule corrections

### FIND-003-6 — accurate changed-item rustdoc

The changed shared SQL delete owner `queries/cards/delete.rs::soft_delete_card_with_expected_kind` (:77–86) lacks `# Errors`. New `settle_waiting_selections` docs (:1196–1198) say operation locks precede the Card row, while activation takes the row first. `hold_pending_member` docs (:220–221) claim its hold precedes every Repository family lock, which the current caller violates.

Add the delete error contract. Document the final actual acquisition order, transaction ownership, progress and retry behavior after section 1. Preserve completed documentation fixes; inspect the final bodies and callers. This closes the explicit changed-item rustdoc gate without a separate style round or documentation-only behavioral tests.

### FIND-003-7 — named SQL decoding

`queries/cards/repository.rs:88–100` decodes current Card UID and occurrence as positional `(Uuid, Uuid)`. New server test readers `card_state` and `policy_state` (`pg_card_declaration_route.rs:5833–5861`) also decode multiple columns positionally. Current ordering works, but violates the mandatory named-column rules.

Use SQLx named row extraction at these existing readers. Preserve CardUid validation, test tuple return contracts, tenant scope, transaction ownership and selection semantics. Existing named projections provide the pattern. No public type, generic mapper or column-order test is needed; leave unrelated scalar readers and old tests alone.

### FIND-003-8 — fallible loader-test setup

`wyrd-loader/src/repository.rs:222–228`, in `origin_remote_decodes_quoted_values_and_any_key_case`, unwraps TempDir creation, directory creation and file writing. This violates the root IO rule in a new closure proof; it is not a production availability defect.

Return the standard fallible test result and propagate those three setup errors with `?`. Keep every origin case/assertion, add the error contract and retain assertion panic documentation. No new fixture helper, dependency or setup-failure test.

**Acceptance.** Source inspection confirms the listed rules, accurate synchronization documentation and unchanged public behavior. Existing focused SQL/server and loader assertions pass. Apply the same rules to items changed by this task; do not expand into old-code cleanup.

## Preserved scope and proof

FIND-003-1 identity admission, FIND-003-2 deletion, FIND-003-4 runtime occurrence fencing and FIND-003-5 origin parsing are closed. Carry their accepted evidence. Retain the shared saga, hashing/version eligibility, exact-pin content conflicts, frozen bindings, empty desired list, rollback, TenantConn/RLS, GitHub association serialization and existing recovery leases. Keep the post-registration negative SDK journey. Task006 still owns accepted-execution/admission seam proof.

The unchanged fresh-key auto-versioned Pending503 limitation is outside this correction. No new table, graph store, worker, queue, lock service, retry framework, public API, dependency, scan-index work, heavy composite uploads or broader Git-config support.

Use `mise tasks ls --hidden` to locate the existing environment/target owners. Record the actual new regression name and exact command when written; do not invent a selector in advance. Required focused proof:

1. The new exact-root declaration/activation regression under repository-managed Postgres.
2. Existing server tests `reused_pending_member_gates_selection_until_reconciled`, `rollback_selects_in_its_own_write_commit` and `overtaken_declaration_records_its_conflict_on_activation`; existing SQL occurrence, reused-member and deletion cases. Their exact selectors and wrapper are recorded in this directory's `verification.md`.
3. The existing migration probe for the complete first-release schema. No historical intermediate-version upgrade matrix.
4. Exact loader selector: `mise exec -- cargo nextest run --locked -p wyrd-loader --lib -E 'test(=repository::tests::origin_remote_decodes_quoted_values_and_any_key_case)'`.
5. Applicable format/lint and tenant-isolation checks for the final write set. Codegen/types only if a projected contract actually changes.

Do not run full journey suites or broad aggregates for this remediation; integrated change review owns those. Add no tests that merely mirror documentation or mapping implementation. Record RED/GREEN for the lock defect and execution evidence honestly.

Next task review checks FIND-003-3/6/7/8 closure, the concrete stale-code removals and affected callers. Carry all other accepted evidence; do not restart Task003 discovery.

## Obligations

| ID | Promise (source) | Planned proof |
|---|---|---|
| R2-LOCK | An exact retry of a Pending Repository root and its final activation both finish without deadlock or registry503, in either acquisition order. The root ends Active and selected, and the retry reuses it. The corrected ordering is asserted: no backend waits on a lock while holding the family line, and in the writer-first order only the writer waits on that line (§1 Acceptance, FIND-003-3, REQ-003/O-23). Cases: writer acquires first; activation acquires first. | New real-Postgres route regression using existing fixtures |
| R2-ORDER | Every exact Repository reuse through `persist_node` holds the reused Pending row before its family line and resolves the row again under that line. The late duplicate root hold is removed, and no alternative ordering, readiness guard or retry guard is added; synchronization is kept and selection stays in its commit. Preserved: non-root Pending holds, external Active-reference checks, operation-before-family settlement, lease fencing, and the four responsibilities of §1 (§1, §2 bullet 4). The four responsibilities are: the hold exposes uncommitted membership; persisted exact outcomes, including reused members, decide readiness; request, already-Active replay and recovery share one atomic completion boundary; and a conflict ends the attempt without undoing activation. | Diff inspection; existing reused-member, rollback, recovery and conflict tests |
| R2-MIG | The unshipped occurrence migration keeps its columns and nullity constraint and loses its legacy backfills and historical comments. The complete feature schema applies through the existing runner. No new migration, legacy invalidation or upgrade test (§2 bullet 1, §2 Acceptance). | Migration probe; SQL occurrence and deletion tests |
| R2-DOC | Changed-item rustdoc is accurate against the final bodies and callers. It covers: the delete owner's `# Errors`; the actual acquisition order, transaction ownership, progress and retry for the writer, activation and settle owners; `settle_selection`'s request-side 409 role, with its wrapper and callers kept; and no backfill wording. Earlier documentation fixes are preserved (§2 bullets 2–3, FIND-003-6). | Source inspection |
| R2-SQLNAMED | `selected_repository` and the test readers `card_state` and `policy_state` decode named columns. CardUid validation, tuple returns, tenant scope and transaction ownership are kept. No public type, generic mapper or column-order test (FIND-003-7). | Source inspection; SQL and route tests |
| R2-LOADER | `origin_remote_decodes_quoted_values_and_any_key_case` returns a fallible result and propagates its three setup errors with `?`. Every case is kept, errors and panics are documented, and there is no new helper, dependency or setup-failure test (FIND-003-8). | Source inspection; exact loader selector |
| R2-KEEP | Items changed by this task follow the same rules (§3 Acceptance). Closed FIND-003-1/2/4/5 behavior and the Preserved-scope list are kept: shared saga, hashing and version eligibility, exact-pin content conflicts, frozen bindings, empty desired list, rollback, TenantConn/RLS, GitHub association serialization, recovery leases, and the post-registration negative SDK journey. Task006 seam proof stays with Task006. Non-goals stay excluded: new table, graph store, worker, queue, lock service, retry framework, public API, dependency, scan index, heavy uploads, broader Git config, and the fresh-key auto-version Pending503 fix (Preserved scope and proof). | Focused runs; diff audit |

## Evidence

The goal named `TASK-003-R2-close-selection-order-and-upgrade.md`; this directory's remediation file is `TASK-003-R2-close-selection-lock-order.md` (mechanical deviation). Commits on `607f51929`:

- `4f7abfbd6`: lock order and regression;
- `17349e19c`: cleanup and rule corrections;
- `5e22593cd`: ordering assertion and settle docs, from the checker pass.

Postgres wrapper, as one literal command:

```sh
mise exec -- env WYRD_REG_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && \
cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E 'test(=pg_cards_declare::repository_selection_fences_the_selection_occurrence) | test(=pg_cards_declare::repository_selection_waits_for_a_reused_pending_member) | test(=pg_cards_declare::soft_delete_clears_only_the_deleted_selection)' && \
cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E 'test(=pg_card_declaration_route::exact_pending_root_retry_and_activation_finish_in_either_order) | test(=pg_card_declaration_route::reused_pending_member_gates_selection_until_reconciled) | test(=pg_card_declaration_route::rollback_selects_in_its_own_write_commit) | test(=pg_card_declaration_route::concurrent_first_declarations_admit_one_repository_identity) | test(=pg_card_declaration_route::overtaken_declaration_records_its_conflict_on_activation)' && \
cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=repository_declaration::pg_tests::failed_readiness_keeps_the_selected_policy) | test(=repository_declaration::pg_tests::deleting_the_selected_root_selects_nothing)'"
```

Results: Wyrd and Vala migration probes 1/1 each; SQL 3/3; server 5/5; SDK 2/2; exit 0.

| Obligation and required cases | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| R2-LOCK: writer-first; activation-first; root Active and selected; retry reuses root; no 503; ordering asserted | `service.rs:1594` holds the pinned Repository before `lock_version_line` (`service.rs:1596`) | `pg_card_declaration_route.rs:6470`. Writer-first round (2.0.0): a gate holds the family line, B holds the root, then A's same-key replay activates. Replay runs before `write`, so it takes no slot lock and its activation really waits on B's hold. Activation-first round (3.0.0): a gate holds A's operation row, A's activation holds the root, then B retries. Asserted before the outcomes: no lock waiter holds an advisory line (`:6563`, read through `line_holders_waiting` `:6421`); writer-first advisory waits equal 1 (`:6569`). Then: both 201, B's root UID equals A's, version count unchanged, root Active and selected. | RED on `607f51929` source. Writer-first: the activation hit `deadlock detected` and answered 503. Activation-first alone, old ordering assertion: B hit `deadlock detected` → 503. Activation-first alone, new ordering assertion: `left: 1, right: 0`, i.e. B waited while holding the family line. GREEN: 1/1. | PASS |
| R2-ORDER: one order; late root hold removed; no new guard; four responsibilities and preserved locks kept | `service.rs:1588–1596` holds early; `service.rs:1606` skips the late hold only for Repository. Auto/scoped Repository dedup resolves only Active or same-operation rows (`version_resolve.rs:74`), so it needs no hold. Repository writers serialize under the GitHub slot lock, so no pinned Repository appears between the hold and the line. Settle, activation, lease, readiness SQL and `recheck_active_card_refs` are unchanged. | Diff `607f51929..HEAD -- crates` | Green: `reused_pending_member_gates_selection_until_reconciled` (reused-member readiness, non-root hold, recovery, atomic rollback); `overtaken_declaration_records_its_conflict_on_activation` (conflict without undo); `rollback_selects_in_its_own_write_commit` (write-commit selection); SQL `repository_selection_waits_for_a_reused_pending_member` | PASS |
| R2-MIG: backfills and comments removed; columns and CHECK kept; schema applies; no new migration | `20261112000000_repository_selection_occurrence.sql` now has only the two ADD COLUMNs and `cards_selection_occurrence` | Migration file | `pg_migration::pg_tests::migrations_apply_and_are_idempotent` and the Vala probe on a fresh DB. Runtime fencing, clearing and rollback: SQL `repository_selection_fences_the_selection_occurrence`, `soft_delete_clears_only_the_deleted_selection`; SDK `deleting_the_selected_root_selects_nothing`, `failed_readiness_keeps_the_selected_policy` | PASS |
| R2-DOC: delete errors; order, ownership, progress, retry; `settle_selection` role; no backfill wording; earlier fixes kept | `delete.rs:83`; `repository.rs` `lock_card_repository_selections` and `hold_pending_member` (hold precedes the root's line); `service.rs:1216` (all three callers, conditional line, rollback and resettle); `service.rs:1175`; `service.rs:2793`; `persist_node` docs; `SelectedRepository.occurrence` | Source | Content review against `settle_repository_selection` (operation lock, then the line only when pending and ready), `commit_card_activation`, and the already-Active caller. `mise run lints` exit 0. | PASS |
| R2-SQLNAMED | `SelectedRepositoryRow` (`repository.rs:82`) keeps `CardUid::from_uuid`; test `CardStateRow` (`pg_card_declaration_route.rs:5831`) keeps tuple returns and `tenant_row` | Source | SQL and route runs above | PASS |
| R2-LOADER | `wyrd-loader/src/repository.rs:219` returns `std::io::Result<()>`, uses `?` three times and has `# Errors` and `# Panics` | Source | `mise exec -- cargo nextest run --locked -p wyrd-loader --lib -E 'test(=repository::tests::origin_remote_decodes_quoted_values_and_any_key_case)'`: 1 passed | PASS |
| R2-KEEP: same rules on changed items; preserved scope; non-goals | New test items carry rustdoc with `# Panics`, use named decoding, and have no `unwrap` on IO. `lock_waits`, `wait_for_lock_waits` and `line_holders_waiting` are small `pg_stat_activity` polling readers in the style of `wait_for_card_status`, not a concurrency framework. Only six source files changed, with no new dependency, public type or migration. | `git diff --stat 607f51929 HEAD -- crates sdks` | Identity (`concurrent_first_declarations_admit_one_repository_identity`), deletion, occurrence, rollback and SDK journeys green. `mise run fmt:check`, `mise run lints` and `mise run check:tenant-isolation` exit 0; `git diff --check` clean. Codegen not run: no projected contract changed. | PASS |

Material risk, outside this task: if a Repository is declared as a non-root Service-bundle peer, the writer takes that Repository's line before holding later reused Pending members. That can reproduce a pre-existing inversion with their activation. Lock order is guaranteed only when the Repository is the root. The fresh-key auto-versioned Pending503 limitation is unchanged.
