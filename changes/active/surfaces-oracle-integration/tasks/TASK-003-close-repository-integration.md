---
id: TASK-003
kind: implementation
status: implemented
spec: SPEC-surfaces-oracle-integration
spec_revision: 5
requirements: [REQ-031, REQ-032, REQ-033, REQ-034, REQ-035, REQ-035A, REQ-036, REQ-037, REQ-038, REQ-039, REQ-042, REQ-043, REQ-044, REQ-045, REQ-046, REQ-047, REQ-064, INV-003, INV-011, INV-012, INV-014, INV-015, INV-016, INV-025, AC-001, AC-007, AC-007A, AC-008, AC-009, AC-020, AC-022]
depends_on: [TASK-001, TASK-002, TASK-004]
parent_task:
remediates: []
---

## Objective

Close the integrated repository around the completed data plane, clients, and
single data root: reconcile the greenfield migration baseline, production-
shaped fixtures, generated artifacts, documentation, verification inventory,
and GitHub Actions so the complete revision-5 candidate is demonstrably green.

## Constraints

- This task is final integration closure, not a place to defer behavior from
  the preceding outcomes or to weaken their acceptance.
- Preserve Oracle's isolated Postgres and surviving `wyrd-testing` authority;
  preserve required Surfaces Card, WyrdState, identity, storage, and CLI
  journeys. Do not restore retired benchmark, qualification, typed-read, or
  runtime-emulation layers.
- Generated files come only from reconciled sources. Never hand-merge derived
  outputs or hide a live invariant by weakening a check.
- Pull-request selection, nightly correctness, live-cloud, and performance
  workflows remain distinct as specified. Unaffected skipped jobs are valid;
  selected failures are not.
- Every local non-credentialed lane and gated journey must pass with no
  baseline waiver, empty selection, ignored failure, weakened assertion, or
  lower-tier substitution. Credentialed cloud workflows must pass in GitHub
  Actions.
- Exclude compiled `.node` files from the tree. Do not rewrite history; that is
  a separately authorized prerequisite for entering `main`.
- Keep live Oracle UI integration outside this change.
- There is no such thing as a pre-existing failure anymore. All failures must be explicitly handled within the current execution context.

## Relevant Surface

- Workspace manifests, locks, `mise.toml`, scripts, and test-family inventory
- `.github/workflows` and affected-code/required-check aggregation scripts
- `crates/shared/wyrd-dev-fixtures`, `crates/wyrd/wyrd-testing`, and Postgres
  migration/test owners
- Contract generators and generated OpenAPI, schema, Python, and TypeScript
  artifacts
- Architecture, public documentation, examples, and removal inventories
- `.dev/merge-audits/surfaces-oracle-integration` and the active change packet

Paths are ownership guidance, not a private implementation allowlist.

## Approach

1. Reconcile workspace membership, dependency closure, and the greenfield
   migration baseline around the final Redux and SDK topology.
2. Preserve Oracle's least-privilege isolated database lifecycle, non-owning
   child attach, production-shaped cluster fixtures, and surviving capability
   journey inventory.
3. Reconcile `mise` lanes and affected-code classification so every crate and
   user-facing capability has one credible owner without restoring retired
   test layers.
4. Reconcile GitHub Actions for selected pull-request lanes, stable required
   aggregation, complete nightly-main correctness, and separate live-cloud and
   performance qualification.
5. Regenerate every public artifact from its reconciled source and update
   current architecture/documentation where the approved revision supersedes
   implementation drift.
6. Remove obsolete docs, checks, migrations, aliases, routes, and compiled
   artifacts only where their responsibility is unreachable or enforced by
   the surviving authority.
7. Run the complete local and CI evidence matrix, map every `REQ-*`, `INV-*`,
   and `AC-*` to evidence, and record the immutable inputs and later merge
   readiness without merging to `main`.

## Acceptance Criteria

- Isolated Postgres contract, concurrency, role, inventory, migration, attach,
  and cleanup evidence passes under the required least-privilege roles.
- Every surviving SDK, Forge, Scribe, Oracle, OTLP, server, MCP, Python,
  TypeScript, Card, WyrdState, identity, and storage journey is selected by an
  owning lane; no deliberately deleted harness surface is restored.
- Pull-request classification and required-job aggregation pass success,
  skipped, failure, mixed, global, and unclassified scenarios. Nightly-main
  runs complete non-credentialed correctness; live-cloud and performance remain
  separate workflows.
- Contract regeneration is clean and complete across OpenAPI, protobuf, JSON
  schema, Python stubs, TypeScript declarations, MCP runtime catalog, docs,
  and examples.
- The final tree contains no legacy Bifrost owner, stale public alias, obsolete
  unreleased migration compatibility, CLI-owned database bootstrap, restored
  audit-integrity surface, or tracked native build artifact.
- `mise run gate`, all focused capability lanes, and every gated local journey
  pass without waiver or zero-test selection. The owning GitHub Actions
  live-cloud workflows pass with credentials.
- Final static review maps every revision-5 requirement and invariant to
  credible evidence, confirms every acceptance criterion has passed, and
  records remaining risks and readiness for a separately authorized merge.

## Verification

- `mise run fmt`
- `mise run lints`
- `mise run gate`
- `mise run verify:bifrost`
- `mise run test:bifrost:journey:forge:production-geometry`
- `mise run test:cards:unit`
- `mise run test:cards:integration`
- `mise run test:cli:journey`
- `mise run test:wyrdstate:journey`
- `mise run test:identity:journey`
- `mise run test:postgres:contract`
- `mise run test:postgres:concurrency`
- `mise run test:postgres:roles`
- `mise run test:postgres:inventory`
- `mise run py:test:testing`
- `mise run py:test:integration`
- `git diff --check`

Attach passing GitHub Actions evidence for each credentialed live-cloud
workflow. `verify:bifrost` is run only here, after TASK-001, TASK-002, and
TASK-004. The final `gate` supplies the repository-wide checks already in its
dependency closure; do not repeat them individually. Run Cargo-backed commands
sequentially in the shared checkout.

## Implementation Evidence

Commits: `b9401a34a` (affected-code PR lane selection, tested required-job
aggregation, nightly identity/Postgres lanes), `dadb0125d` (weekly
production-geometry performance workflow), `0e6ace466` (client-tier and SDK
PyO3 cone checks in `gate`; removed `cli:dev-bootstrap` and three empty
orphan test files), `de2cbf133` (Scribe statistics reads no longer erase
pre-insertion active reservations), `957ef9f1d` (harness reads Scribe in-flight
work after drain), `de39d19fd` (Forge refuses an impossible table rewrite
policy at planning), `de1f9844c` (geometry journey judges task ownership after
drain), `5db5cf162` and `62f8a830e` (identity harness registers bound Cards
under their own uid and honours immediate revocation), `55a298885` (retired
the tenant-isolation self-test for the deleted Oracle admission module),
`686636d38` (recovered cleanup settles through the ordinary claim record),
`cd07c2737` (orphan-cleanup demand coalesces onto one nonterminal task per
table), `fb7e7c395` (Scribe round-robin journey places batches on every lane),
`0facbe803` (queue reports a background send's terminal refusal at the next
flush or shutdown), `30751d469` (Oracle audit commits capped to a share of the
Vala pool; frozen-range replay journey tolerates the server's own publisher),
`fe0e20bc5` (theme tokens regenerated; the Claude skill copy is left to
`skills:sync`; UI change detection names the `.agents` theme path),
`288944c6e` (Agent fixture matches the canonical Card envelope), `d880e7871`
(third-party debug info reduced to line tables), `b504d6d5f` (Bifrost Rust unit
tests in one nextest invocation, same 221-test selection), `7cd28db18`
(hakari `workspace-hack` dev-dependency unifies third-party test features,
enforced by `check:workspace-hack`), `19769e24c` (generated output and wire
fixtures stable under the unified feature set), `2bbf390f4` (artifact storage
journeys run the `Server` target), `2feb3756e` (server-starting TypeScript
Oracle journeys declare the sibling timeout).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Isolated Postgres contract, concurrency, role, inventory evidence | `scripts/postgres/*` | `test:postgres:{contract,concurrency,roles,inventory}` | PASS |
| Every surviving journey selected by an owning lane | `mise.toml` lanes; `nightly.yml` identity/postgres jobs | `verify:bifrost`; `test:cards:{unit,integration}`; `test:cli:journey`; `test:wyrdstate:journey`; `py:test:{testing,integration}`; `test:identity:journey`; `test:bifrost:journey:forge:production-geometry` | PASS |
| PR classification and aggregation scenarios; nightly/cloud/perf separate | `.github/scripts/{detect-changes,verify-required-jobs}.sh`; `lints-test.yml`; `nightly.yml`; `performance.yml` | `check:ci-selection` (generic, bifrost-only, mixed, global, unclassified, planning-only, storage; success/skipped/failure/cancelled/empty aggregation) | PASS locally; workflow runs need a push |
| Contract regeneration clean | generators | `codegen:check` inside `gate` | PASS |
| No legacy owner, alias, CLI DB bootstrap, restored audit surface, tracked native artifact | `0e6ace466` | `check:no-legacy-server-vocab`; `git ls-files '*.node'` empty | PASS |
| `gate`, capability lanes, gated journeys pass; live-cloud workflows pass | all owners | see open items | BLOCKED |
| Final static review maps every requirement and invariant | this record | see mapping gaps below | BLOCKED on AC-009 review |

Commands (clean detached worktrees; exit 0 unless noted):

- At `957ef9f1d`: `mise run fmt:check`, `mise run lints`, `mise run verify:bifrost`,
  `mise run test:cards:unit`, `mise run test:cards:integration`,
  `mise run test:cli:journey`, `mise run test:wyrdstate:journey`,
  `mise run test:postgres:contract`, `mise run test:postgres:concurrency`,
  `mise run test:postgres:roles`, `mise run test:postgres:inventory`,
  `mise run py:test:testing`, `mise run py:test:integration` (55 passed),
  `git diff --check`.
- At `de1f9844c`: `mise run test:bifrost:journey:forge:production-geometry`
  (1 passed, 479 s); `cargo clippy --locked -p wyrd-testing --all-features --tests -- -D warnings`.
- At `62f8a830e`: `mise run test:identity:journey` (18/18);
  `cargo clippy --locked -p wyrd-testing --all-features --tests -- -D warnings`.
- `check:tenant-isolation:self` read
  `crates/vala/vala-sql/src/queries/oracle_admission.rs`, deleted by
  `194b37c73`. With explicit approval, `55a298885` retired that self-test and
  its two dead `VALA_OPERATOR_ALLOWLIST` entries; the property it guarded has
  no remaining owner.
- At `cd07c2737`: `mise run gate` failed only in
  `round_robin::scribe_system_and_dynamic_tables_are_round_robin_equal`
  (7 shared lanes of 16 from random batch ids). At `fb7e7c395` the exact
  nextest command for that test, under `with-test-postgres.sh`, passed.
- At `fb7e7c395`: `mise run gate` failed only in
  `test_negative_empty_permissions_denied_rbac_on_write` (Python journey) and
  `audit_publication::frozen_audit_range_replays_once_while_its_tail_waits`.
- At `0facbe803`: `wyrd-queue` lib tests (41/41), with
  `producer::tests::background_terminal_refusal_is_reported_by_the_next_flush_once`
  failing when the fix is removed; `cargo clippy -p wyrd-queue --all-features
  --tests -D warnings`; `mise run test:bifrost:journey:python` (35 passed).
- At `30751d469`: exact nextest for
  `audit_publication::reads_are_served_while_audit_commits_wait_on_the_chain_head`
  fails with `OracleRoleUnavailable` without the pool cap and passes with it;
  `frozen_audit_range_replays_once_while_its_tail_waits` passed 4/4 alone;
  `mise run test:bifrost:journey:server` (10/10); clippy for `wyrd-server` and
  `wyrd-testing` tests.
- At `30751d469`: `mise run gate` passed every Bifrost lane (9/9) and failed
  in `check:tokens`.
- At `fe0e20bc5`: `mise run check:tokens`, `mise run check:skills-sync`,
  `mise run check:ci-selection` (7 passed), `mise run docs:check`.
- From `288944c6e` every `gate` dependency ran as its own `mise run` lane.
  At `19769e24c`: `check`, `check:skills-sync`, every `check:*` lane,
  `codegen:check`, `check:test-coverage`, the `py:*` and `ts:*` lanes,
  `examples:python:datacard`, `test:wyrd` (1907), `test:skald` (410),
  `test:vala`, `test:shared`, `test:sql`, `test:tonic`, and eight of nine
  `test:bifrost` lanes passed.
- At `2bbf390f4`: `mise run test:storage:matrix` (storage e2e 9/9);
  `cargo clippy --locked -p wyrd-server --all-features --test storage_e2e -- -D warnings`.
- At `2feb3756e`: `mise run test:bifrost:journey:typescript` (16/16).

Root causes fixed while running the lanes:

- The Scribe preflight admission counter underflowed because
  `memtable_stats()` overwrote pod-global active bytes that pre-insertion
  reservations still held (`de2cbf133`, with a mutation-verified test).
- Retained audit reaches Forge, so the undeclared 512 MiB `audit_log` table
  received SmallFiles tasks that a 768 MiB threshold made impossible, and
  every worker attempt refused them. The scheduler now derives the worker's
  `ForgeTablePolicy` and withholds only that rewrite (`de39d19fd`).
- Identity journeys drifted from the registry-walked token mint and the
  harness's 5-second revocation cache (`5db5cf162`, `62f8a830e`).
- The Forge recovery drain settled a claim without `record_settled_claim`,
  so the test attempt barrier never opened and
  `forge::production_routes::coordinator_and_worker_delete_only_exact_never_published_generation`
  hit its attempt bound once orphan demand was deduplicated (`686636d38`,
  `cd07c2737`). Earlier contradictory results came from the verify worktree
  and main checkout sharing one `CARGO_TARGET_DIR`: workspace-relative
  dep-info let Cargo reuse a binary built from the other tree's source.
- The Scribe round-robin journey proved lane sharing with random batch ids,
  so the hash sometimes split the lanes; batches are now placed on every lane
  (`fb7e7c395`).
- A row-count, interval, or retry seal in `wyrd-queue` dropped a terminal
  refusal, so a flush after a background send of a denied batch returned Ok
  (`0facbe803`).
- Oracle read-decision audit commits each parked a Vala pool connection while
  waiting on a held `audit_chain_head` lock, starving reader-epoch renewal
  until the Oracle fenced itself. The frozen-range journey reached that state
  when the server's own publisher froze a range after one of its three appends
  (`30751d469`).
- `97eab6c44` changed `palette.json` without regenerating the agent skill and
  docs token files, and `gen-theme.mjs` still wrote a Claude-specific variant
  that contradicted the byte mirror `check:skills-sync` enforces (`fe0e20bc5`).
- Oracle reader leases expired while macOS idle sleep suspended a gate run,
  aborting journeys; lanes now run under `caffeinate -ims`.
- `00fe9fae1` removed the Agent status override, so the fixture still carried a
  status the Card envelope skips (`288944c6e`).
- Hakari unifies `serde_json/preserve_order` into test builds (production
  `wyrd-server` already enables it), so the UI problem-example catalog and one
  OpenAI wire snapshot changed key order; the generator now sorts keys and the
  snapshot was re-recorded with no value change (`19769e24c`). The new crate
  also needed a test family.
- Bound storage servers composed a Forge worker, which refuses staging without
  native `start_after` listing; Azure lacks it, so Azure journeys never became
  ready (`2bbf390f4`). An Azure-backed deployment with a Forge worker still
  refuses to start, as `66bec4bf1` intends.
- Three server-starting TypeScript Oracle journeys used vitest's 5 s default
  and timed out under load (`2feb3756e`).

Open items:

- Credentialed live-cloud workflows (`storage-integration-cloud.yml`) and the
  new `performance.yml` and `nightly.yml` jobs need a push and GitHub secrets.
  No local run can supply that evidence.
- Resolved: spec revision 8 (user-approved 2026-09-14) aligns REQ-026,
  REQ-026A, AC-005, and AC-017 with REQ-014's non-blocking Oracle outbox audit.
- AC-009 needs a final `$wyrd-change-review`.
- FIND-TASK-002-18 (AI trailers on earlier commits) needs explicit
  authorization to rewrite history; this task forbids a rewrite.

Non-goals held: no history rewrite, no tracked `.node` artifact, the live
Oracle UI stays out of scope, and no unrelated working-tree file was committed.
