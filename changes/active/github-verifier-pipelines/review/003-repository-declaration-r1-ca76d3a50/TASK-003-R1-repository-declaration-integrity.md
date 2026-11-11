---
id: TASK-003-R1
status: ready
approved_spec_revision: 5
original_task: TASK-003
findings: [FIND-003-1, FIND-003-2, FIND-003-3, FIND-003-4, FIND-003-5, FIND-003-6]
---

# Complete Repository declaration integrity and recovery

Route directly to `$wyrd-implement`. This packet corrects six independently validated findings within approved behavior. No specification decision is pending. It is one remediation task, not a new design or replacement declaration system.

## Subject and authority

- Approved specification: `changes/active/github-verifier-pipelines/spec.md`, revision 5; Repository public shapes in adjacent `contracts.md`; approved behavior in `design/approved-declaration.md`.
- Original task: `changes/active/github-verifier-pipelines/tasks/003-repository-declaration.md`.
- Original base: `5f13d86aeacf1a7a1ad0c192b093aa6c1b1f79cc`.
- Reviewed candidate: `ca76d3a50c83e119e917a746db67b5a64b1e8f2b` on `task/github-verifier-pipelines/TASK-002-github-app-connection`.
- Review: `changes/active/github-verifier-pipelines/review/003-repository-declaration-r1-ca76d3a50/`; source-validated findings and full reconciled matrix are in `findings-validation.md`. Preserve original task, candidate and reports as acceptance context.
- Governing owners: `architecture/wyrd-design.md` Repository, Registry lifecycle and Repository selection; security Postgres tenant binding/GitHub association; root/nested AGENTS and applicable topic guides. Production remains immutable during review; this packet authorizes the corrective implementation through the normal skill.

## FIND-003-1: enforce family identity in the admission transaction

**Diagnosis.** `components/cards/service.rs:779–807` checks both provider-ID→family and family→provider-ID mappings, then commits that preflight transaction. `DeclarationWriter::write:1343–1449` later persists without rechecking; `persist_node:1480` locks only kind/name. SQL `queries/cards/repository.rs:29–67` reads an existing mapping but supplies no cross-family serialization. Two valid first declarations can both observe no mapping. Same ID with different explicit names creates two families; different connected IDs with the same name can create different versions of one family with different external identities. A later selection conflict does not undo those committed rows. Sequential rename/refusal tests cannot prove this concurrent invariant.

This violates REQ-002, O-12/O-13 and the stable Repository family rule. Concurrent CI declarations are supported use; the consequence is ambiguous policy identity, not merely an inconvenient error message.

**Correction outcome and recommendation.** Serialize canonical identity admission in the existing declaration write transaction. Reuse the tenant GitHub slot lock `lock_github_installation` in `queries/github_installations.rs:206–214`, before version-line locks, and retain it through commit. Recheck both final canonical mapping directions under that lock; conflicting admission returns the existing identity-conflict error and rolls back that attempt. Provider resolution stays outside this transaction. Use the verified identity; preserve existing-family inference and rename behavior. If no connection slot remains, use the existing connection refusal boundary rather than proceeding unlocked. This is one existing lock and write-owner correction; another unlocked preflight, name-only lock, new table, or consumer ambiguity handling cannot close both races.

**Acceptance / focused proof.** Coordinate two real-Postgres declarations so both have reached preflight before either writes. Cover same ID/different names and different IDs/same name. Exactly one mapping persists; the loser receives the identity-conflict code and leaves no conflicting nodes. Same ID/same name normal versioning and inferred reuse after rename continue to work.

## FIND-003-2: clear selection in ordinary soft deletion

**Diagnosis.** Migration `20261111000000_repository_cards.sql:25–28` allows non-null `selected_at` only for Active Repository rows. The shared mutation `queries/cards/delete.rs:129–147` changes status to deleted without clearing selection. Both ordinary UID/kind and exact-ref delete paths reach `soft_delete_card_with_expected_kind`. A selected root has no inbound references and passes normal checks, then PostgreSQL rejects the UPDATE. The root remains selected and readable instead of being deleted.

Repository is an ordinary Card (O-16/O-32); no special deletion prohibition was approved. This breaks every normally selected root's public deletion path.

**Correction outcome and recommendation.** Clear `selected_at` and the occurrence marker introduced by FIND-003-4 atomically in the existing shared soft-delete UPDATE. Retain the integrity constraint, row lock, inbound-reference refusal, tenant scope, cleanup scheduling and idempotence. Preserve the existing checks preventing selection of a deleted target. Do not fall back to an old version, disconnect access or add a Repository-specific endpoint. One mutation owner already serves both addressing forms.

**Acceptance / focused proof.** A focused SDK journey declares/selects a Repository, deletes its exact root, observes ordinary absence/deleted behavior and no selected version, and confirms the GitHub connection and registered Verifier remain intact. Verify deleting an unselected version preserves another selected version at the shared owner. Retain existing ordinary Card deletion proof; no duplicate end-to-end workflow for each addressing form is required.

## FIND-003-3: make exact graph readiness and selection one recoverable completion boundary

**Diagnosis.** SQL `queries/cards/repository.rs:155–160` tests readiness only for Cards inserted by the current operation. Exact-pin reuse at server `resolve_existing:1768–1794` can reuse a Pending local sibling owned by an earlier operation. Submitted siblings pass through `EffectiveSpecs` and binding freeze, while external-reference Active validation does not cover those local siblings. The complete exact outcome set is already persisted in `DeclarationReplaySeed` (`service.rs:1892–1917`); insertion ownership is not declaration membership.

The supported failing path is metadata-only. Attempt A inserts exact Verifier z-review; its blob fails after registration and it remains Pending. B submits unchanged local z-review plus fresh a-review and a Repository binding both. B commits, then is interrupted before inline completion. Per-Card reconciliation can activate B's a-review and root while z-review is backed off. Same-key B replay retains seed Pending statuses and visits a-review before z-review. Completing already-Active a-review calls selection; the target and all B-owned rows are Active, so B selects while reused z-review is still Pending. Its next completion may fail after the prior policy was replaced. See `followup-review.md` for exact producer/replay/reconciler references.

A second symptom has the same completion owner: `commit_card_activation:2630–2669` commits Active and clears recovery work before request-only `settle_selection:1106–1130` runs. `reconcile_card_claim:2177–2186` calls inner completion without selection. Recovery after blob failure, or a process interruption after activation commit, can leave every Card Active/idle while its selection remains pending indefinitely. Original-key manual replay remains possible, but does not satisfy the declared server recovery boundary.

This violates O-20/O-21/O-31/O-38, REQ-003 and architecture `Repository selection:2419–2426`. Existing SQL tests model Pending rows owned by the current operation, while journeys fail only before registration. They omit the expressly required post-registration failure proof. The original proposed artifact-bearing composite reproduction is invalid because existing validation rejects such composites; do not implement it or relax that guard.

**Correction outcome and recommendation.** Use the persisted replay-seed outcome UIDs as exact submitted membership, resolving their current lifecycle states, including reused local members. Preserve external-reference admission validation and the Repository target check. At the existing activation/completion transaction owner, resolve and settle affected pending Repository declarations when all exact members are ready. Derive operation identity from persisted state, not the caller transport key or Card creation owner. Include operations waiting on a Card inserted by another attempt and the reused/already-Active completion branch.

Persist the selection decision with the completion/reconciliation transition. The existing background recovery path must use the same boundary so final completion cannot clear its rediscovery obligation while leaving selection pending. Preserve per-Card activation when the graph remains incomplete, blob/manifest-before-Active checks, leases/fencing, normal Card recovery, tenant isolation and valid registered dependencies. A selection conflict terminates that attempt; it does not undo dependencies or crash a shared process. Reuse operation rows/seed and SQL family locking. Do not add a graph store, worker, queue, table, reparent Cards, reject supported exact-pin reuse or add another post-commit request-only guard.

**Acceptance / focused proof.**

1. Produce A's Pending metadata Verifier through blob failure, commit B with that reused member plus fresh sibling/root, recover B-owned rows, and replay B. Old selection remains until the reused exact member becomes Active; only then may B select.
2. Existing background reconciliation after post-registration blob failure completes the same persisted attempt and selects or records the required conflict without a new declaration. Inject a precommit failure to prove final completion and selection cannot split.
3. Add the bounded real SDK→WyrdTestServer/Postgres→SDK negative case required by O-21/O-38: fail required readiness after registration, assert typed failure, unchanged old policy and readable successfully registered dependencies. Use the existing test storage owner; only a narrowly scoped test failure seam is justified if necessary. Pin/missing-ref errors or manually Pending SQL rows alone do not satisfy this proof.

These assertions belong to the same lifecycle correction. Do not introduce a general failure framework or demand a full suite of crash scenarios.

## FIND-003-4: fence a selection occurrence, not just its version

**Diagnosis.** Writer `service.rs:1436–1438` and SQL `repository.rs:96–108,194–215` record/compare only the prior selected UID. Select A; start delayed X expecting A; select B; intentionally newly declare exact A as rollback; then let X settle for the first time. Current UID again equals A, so X silently overwrites the newer rollback. Terminal replay protection does not cover an attempt still pending. This composes two explicitly supported operations, violating O-24 while O-25 must remain available.

**Correction outcome and recommendation.** Add a private selection-occurrence UUID on existing selected Card rows and expected-occurrence UUID on existing operation rows through an additive migration. On an actual selection change, use the selecting operation's unique ID as its occurrence. Capture UID and occurrence under existing family serialization and compare both during CAS. Identical-current-target reuse leaves the occurrence unchanged; terminal replay stays terminal. Clear the marker alongside `selected_at` on deselection/deletion. Backfill existing selected rows with unique private occurrences. Do not use timestamps as unique event IDs or add a new public contract/table. This is the minimal private fence at the current selection owner; a version UID cannot distinguish two selections of A.

**Acceptance / focused proof.** SQL sequence A → pending X expecting A → B → deliberately new A → first X completion yields stable selection conflict and leaves A selected. Repeated X stays conflicted. Existing identical-target reuse, intentional rollback and terminal replay remain valid. Coordinate the setter with FIND-003-3 and soft deletion rather than adding independent machinery.

## FIND-003-5: interpret normal local origin config

**Diagnosis.** `wyrd-loader/src/repository.rs:147–154` compares only lowercase `url` and passes a raw quoted value to the URL parser. Git accepts both `url = "https://github.com/acme/widget.git"` and `URL = https://github.com/acme/widget.git` in the normal origin section. The candidate instead returns no origin, causing `repository_root:42–58` to refuse minimal TOML as identity-missing. The maintainer used local Git as an oracle; source validation confirmed the rejection. This breaks the advertised default checkout path (O-01/O-04/O-07).

**Correction outcome and recommendation.** Decode quoted values and case-insensitive variable names in the existing filesystem-only origin reader before `RepositoryRemote::parse`. Preserve worktree discovery, offline behavior and explicit remote argument grammar. No new dependency, runtime Git executable requirement, alternative loader or quote permissiveness in the URL parser. The correction is limited to the proven normal config forms, not every Git configuration feature.

**Acceptance / focused proof.** Existing origin parameter cases cover quoted/lowercase and unquoted/uppercase inputs producing the same hint. A minimal TOML `load` assertion proves these inputs reach inferred Repository construction rather than identity-missing. No database or new journey harness is needed.

## FIND-003-6: complete changed-item rustdoc in this remediation

**Diagnosis.** The root mandatory merge gate covers private items and test helpers. Independently verified gaps:

- `wyrd-sql/src/queries/cards/repository.rs:128–135,235–240`: new projection structs and fields lack docs.
- `wyrd-loader/src/lib.rs:469–493`: new helpers omit their panic contracts.
- Server `components/cards/service.rs:2108–2129`: changed completion lacks Errors/progress/retry documentation; new `pg_tests` module at :3361 lacks module docs.
- Rust journey `repository_declaration.rs:97–108`: helper omits its filesystem panic contract.
- Spec `card/mod.rs:2778–2784`: changed count test docs say 15/14 while assertions say 16/15.

**Correction outcome and recommendation.** Document those changed items and fields, named panic paths and test module; correct the counts. Document completion's final corrected transaction, error, cancellation/progress and retry behavior after the lifecycle/fence changes. Do not freeze the current defect in comments or document unrelated old code. These bounded documentation edits are batched here; no style-only remediation round or new behavioral test is justified.

**Acceptance / focused proof.** Source inspection confirms each listed item and newly changed remediation item meets applicable rustdoc rules and matches its body/callers. Run applicable formatting and lint checks with the behavioral work.

## Preserved behavior and non-goals

Keep ordinary loader parse/config/resolve/validate/order, shared client saga, hashing/version reuse and exact-pin conflict, flat Card receipts, frozen binding projection, TenantConn/RLS, connection access, normal dependency retention, Active versus selected state, rollback and empty-list behavior. Retain all three client projections; do not introduce new durable SDK logic. Accepted-execution retention and task006 selected-binding admission remain the downstream seam, not new execution machinery in this task.

No new PostgreSQL table, declaration registry, background worker, broker, public API, provider credentials, heavy composite upload support, source commit, UI or arbitrary event activation. Preserve the reviewed migration and add the needed occurrence migration rather than rewriting applied schema history. Preserve unrelated dirty files and user work. Do not fabricate historical RED evidence; apply the required test-first discipline to these corrections.

## Narrow verification and handoff

The implementer must locate the existing owning tasks with `mise tasks ls --hidden`, confirm actual test names, and record exact focused commands after adding each scenario. Use the repository-managed Postgres wrapper and normal test owners. Run only the new/affected SQL selection/identity/delete cases, server completion/reconciler cases, and bounded SDK negative/deletion scenarios—not full journey suites or broad aggregates. The existing `test:cards:integration` task shows the environment and target owners; narrow its known selectors for remediation proof. Pure parser tests run in `wyrd-loader --lib` through exact nextest selectors. Apply required format/lint checks to the write set and tenant-isolation checking for server/SQL changes; add codegen/types only if a projected contract actually changes.

Map closure evidence explicitly: atomic mapping races to FIND-003-1; public/shared deletion to FIND-003-2; reused-member readiness, recovery commit and negative journey to FIND-003-3; A→B→A fence to FIND-003-4; legal-origin load cases to FIND-003-5; source documentation inspection to FIND-003-6. Full surface journeys and broad required gates run once at integrated change review.

A later `$wyrd-task-review` compares this candidate to the remediation candidate for closure and affected paths, carrying forward the original task and source-backed unaffected acceptance evidence. Do not restart whole-task discovery merely because this is a new round.

## Obligations

| ID | Promise (contract source) | Required cases | Planned proof |
|---|---|---|---|
| R1-ID | Repository family identity is admitted atomically in the declaration write transaction under the tenant GitHub slot lock, both mapping directions rechecked (FIND-003-1; REQ-002, O-12/O-13) | two first declarations both preflighted before either writes: same id/different names; different ids/same name; loser gets `WYRD_REGISTRY_409_REPOSITORY_IDENTITY_CONFLICT` and writes no nodes; no slot → existing not-connected refusal; same id/same name versioning and inferred reuse after rename still work | real-Postgres server test; existing identity journey |
| R1-DEL | Ordinary soft deletion clears selection and its occurrence atomically in the shared delete (FIND-003-2; O-16/O-32) | selected root deleted through the SDK exact ref → not served, no version selected, no fallback; GitHub connection and Verifier intact; deleting an unselected version keeps another selected; existing delete refusals/idempotence retained | SDK journey; SQL shared-owner test; existing delete tests |
| R1-READY | Selection readiness uses the persisted replay-seed exact membership, including reused Pending local members (FIND-003-3 #1; O-20/O-21/O-31/O-38, REQ-003) | A leaves Pending z-review after post-registration blob failure; B reuses it with fresh a-review/root; B's own Cards recovered and same-key replay keep the old selection until z-review is Active; only then B selects | SQL test; server integration test |
| R1-REC | Selection is decided in the same activation/reconciliation transaction, including the already-Active branch and operations waiting on another attempt's Card; a conflict ends only that attempt (FIND-003-3 #2) | background reconciler selects with no new declaration; injected precommit failure leaves activation and selection uncommitted together (Card stays Pending, recovery retained); final replay answers success | server integration test |
| R1-NEG | Bounded SDK negative case: readiness fails after registration (FIND-003-3 #3; O-21/O-38) | typed failure; old policy still selected; registered dependency readable; existing test storage owner only | Rust SDK journey |
| R1-FENCE | Selection CAS compares version and occurrence; occurrence = selecting operation id, unchanged on identical reuse, cleared with `selected_at`, backfilled uniquely; additive migration (FIND-003-4; O-24/O-25) | A → pending X expecting A → B → new A → X conflicts and A stays selected; repeated X stays conflicted; identical-target reuse, rollback, terminal replay remain valid | SQL test; existing rollback journey |
| R1-ORIGIN | Filesystem origin reader decodes quoted values and case-insensitive `url` (FIND-005; O-01/O-04/O-07) | quoted/lowercase and unquoted/uppercase give the same hint; minimal TOML `load` infers the Repository instead of identity-missing | loader unit tests |
| R1-DOC | Changed items carry required rustdoc (FIND-003-6) | repository.rs projection structs/fields; loader helper panics; completion final transaction/errors/cancellation/retry; server `pg_tests` module doc; journey helper panic; spec count docs 16/15 | source inspection; fmt/lints |
| R1-KEEP | Preserved behavior and non-goals (Preserved behavior section) | no new table/worker/queue/public API/dependency; applied migrations untouched; tenant isolation; unrelated dirty files preserved | diff audit; `check:tenant-isolation` |

## Evidence

Commits on top of `ca76d3a50`: `6d90b66b5`, `676355d84`, `65629f28a`, `5199ac919`, `66499a90e`, `eee23b32e` (plus the earlier SQL/fence commits in that range). Postgres runs use `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && …'` with `WYRD_REG_E2E=1`.

| Obligation and required cases | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| R1-ID: both orders preflighted then written; loser 409 with zero rows; same id/name 2.0.0 versions; no slot refuses | `service.rs:853` `admit_repository_identities` locks `lock_github_installation` (`:872`) and admits both directions, called at writer start `:1451`; preflight shares `admit_repository_family` | `service.rs:3716`; rename reuse `repository_declaration.rs` `repository_identity_is_verified_by_the_connection` | `cargo nextest run -p wyrd-server --lib -E 'test(/^components::cards::service::pg_tests::/)'` 2/2 PASS. RED before fix: second write succeeded | PASS |
| R1-DEL: SDK exact delete of selected root, 404 after, old not selected, `selected_version` None, connection Active, Verifier readable; unselected delete keeps selection | `delete.rs:133–148` clears `selected_at`, `selection_occurrence` in the shared UPDATE | `repository_declaration.rs:433`; `pg_cards_declare.rs:1725` | `-p wyrd-ai -P journey --run-ignored=all -E 'test(/^repository_declaration::/)'` 6/6 PASS; `pg_cards_declare::` 14/14 PASS. RED: journey "the selected root deletes: RegistryUnavailable"; SQL CHECK violation | PASS |
| R1-READY: reused Pending member gates root recovery and same-key replay; selects only once Active | `repository.rs:227–260` readiness over seed outcome UIDs; `repository.rs:182` locks operations whose seed contains the Card | `pg_card_declaration_route.rs:5917`; `pg_cards_declare.rs:1661` | `-E 'test(=pg_card_declaration_route::reused_pending_member_gates_selection_until_reconciled)'` PASS. RED with candidate owners: "B's replay cannot select while reused z-review is Pending" left `2.0.0` right `1.0.0`; SQL RED Selected vs Pending | PASS |
| R1-REC: reconciler selects without declaration; injected `BEFORE UPDATE OF selection` failure rolls activation back (z Pending, attempt recorded, old selected); replay 201 | `commit_card_activation` `service.rs:2776` locks waiting `:2786` and settles `:2816` in its transaction; Active branch `:2452–2456`; conflicts logged not raised (`settle_waiting_selections` `:1216`) | `pg_card_declaration_route.rs:5917` | same test PASS; candidate reconcile path never settled (source `reconcile_card_claim` → inner completion only) | PASS |
| R1-NEG: typed `WYRD_STORAGE_500_BACKEND`; old policy selected; answer-check readable | storage broken via the test server's own root (`server.rs:4098` `local_storage_root`), refused root's blob alone kept blocked | `repository_declaration.rs:364` | journey PASS. Missing-proof coverage of behavior the candidate met; no RED expected | PASS |
| R1-FENCE: A→X→B→A' → X conflict twice, A' kept; reuse unchanged; backfill unique | migration `20261112000000_repository_selection_occurrence.sql`; CAS on (uid, occurrence) `repository.rs:280–305`; writer records expected pair | `pg_cards_declare.rs:1572`; rollback in `toml_policy_is_selected_only_once_declared` | `pg_cards_declare::` 14/14 PASS; journey PASS. Process exception: RED not runnable after API change to `SelectedRepository` | PASS |
| R1-ORIGIN: quoted and `URL =` forms; minimal TOML infers `widget` | `repository.rs:150–160` | `wyrd-loader/src/repository.rs:216`; `lib.rs:564` | `cargo nextest run -p wyrd-loader --lib -E 'test(/origin_remote_decodes_quoted_values_and_any_key_case|toml_infers_repository_from_quoted_or_uppercase_origin/)'` 2/2 PASS. RED: `None`; `IDENTITY_MISSING` | PASS |
| R1-DOC: listed items documented, counts 16/15 | `repository.rs:67–78,152–167,328–337`; loader helpers `# Panics`; `complete_card`/`complete_card_inner`/`commit_card_activation` docs; `pg_tests` module doc `service.rs:3510`; `declare_toml` `# Panics`; `card/mod.rs` count doc | listed lines | source inspection; `mise run fmt`, `mise run lints` exit 0 | PASS |
| R1-KEEP: no new table/worker/API/dependency; migrations additive; tenant scope | one additive migration; one test accessor `local_storage_root`; no Cargo changes | `git diff --stat ca76d3a50..HEAD` (12 files, all in write set) | `mise run check:tenant-isolation` exit 0; `git diff --check` clean; unrelated dirty files untouched | PASS |

Process exceptions: R1-FENCE RED could not run against the UID-only API; the server-side F3 production change preceded its integration test (SQL RED preceded it; the integration RED was then taken by restoring the candidate owners).

Material risk outside this packet: re-declaring identical auto-versioned content under a fresh idempotency key while a prior attempt's root is still Pending collides on `cards_identity_unique` and answers `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE`, because `version_resolve.rs:65` `latest_stable_in_line` excludes other operations' Pending rows. It is pre-existing registry behavior for every Card kind; same-key replay and the reconciler recover. It is not changed here.
