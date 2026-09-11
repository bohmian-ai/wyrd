---
id: TASK-003
kind: implementation
status: proposed
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
