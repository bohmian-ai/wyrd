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
under their own uid and honours immediate revocation).

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
- `mise run gate`: fails only in `check:tenant-isolation:self`, whose
  `test_oracle_admission_module_is_an_operator_pool_owner` reads
  `crates/vala/vala-sql/src/queries/oracle_admission.rs`, deleted by
  `194b37c73`. Retiring that self-test and its two dead
  `VALA_OPERATOR_ALLOWLIST` entries awaits explicit approval, because it
  removes a check.

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

Open items:

- Credentialed live-cloud workflows (`storage-integration-cloud.yml`) and the
  new `performance.yml` and `nightly.yml` jobs need a push and GitHub secrets.
  No local run can supply that evidence.
- REQ-026, REQ-026A, and AC-017 still require an Oracle WAL-first relay and
  checkpoint, which the approved REQ-014 removed. This needs a spec revision
  before a final review can map them.
- AC-009 needs a final `$wyrd-change-review`.
- FIND-TASK-002-18 (AI trailers on earlier commits) needs explicit
  authorization to rewrite history; this task forbids a rewrite.
- The uncommitted working-tree Forge task edits make
  `forge::production_routes::coordinator_and_worker_delete_only_exact_never_published_generation`
  time out. They are not part of this change and were not committed.

Non-goals held: no history rewrite, no tracked `.node` artifact, the live
Oracle UI stays out of scope, and no unrelated working-tree file was committed.
