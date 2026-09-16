# Wave 1 Task Implementation Review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Cumulative base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `9c52f887595d4e8a04764d3d2836926e5df70950`
- Candidate tree: `e2731ff2d23fc1a0f4b356ccdd79caf3f6c7b93e`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 8
- Original tasks: `TASK-002-converge-client-and-sdks.md`, `TASK-004-unify-bifrost-data-root.md`, and `TASK-003-close-repository-integration.md`
- Remediation authority: `review/tasks-002-r3-004-003-bbfdf35e-review-01/TASK-003-R1-close-cumulative-review-findings.md` through **Broader verification and final evidence**

The candidate commit and tree remained fixed throughout review. The checkout's
`HEAD` already contained a later evidence-only commit, so candidate source was
read from the immutable Git object. The unrelated dirty
`changes/active/verified-change-contract/` worktree state was not used as
evidence and was not modified. The repository has no `.codegraph/` directory.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-002-19`: remove the four zero-runtime Bifrost types, two removed MCP names, and six stale schema families without replacement | The obsolete types and generator entries are deleted from `crates/wyrd-spec/src/vala/api.rs` and `examples/gen_schemas.rs`; all twelve generated/golden JSON files are deleted; the docs inventory is regenerated | Exact candidate search over `crates`, `sdks`, `docs`, and `tests` found no removed name; independently rerun `bifrost_wire_tests` passed 11/11; available evidence records the focused MCP test, `codegen:check`, and `docs:check` passing | PASS |
| `FIND-TASK-004-1`: the existing locked data-root owner must write/sync/remove-probe every managed directory before composition | `crates/wyrd/wyrd-server/src/boot/data_root.rs:82-136,158-172` probes the WAL root and all three managed children while holding the root lock and returns the existing typed unusable-root error | Independently rerun `boot::data_root::tests` passed 4/4, including `prepare_rejects_an_unwritable_managed_child` | PASS |
| `FIND-TASK-003-1`: total pending Oracle audit ownership is pool-bounded before spawn; overflow stays non-blocking, scrubbed, and counted while admitted work drains | `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:40-108` owns separate pool-share and total-pending semaphores, uses `try_acquire_owned` before `TaskTracker::spawn`, and reuses `record_commit_failure` | `wyrd-testing/tests/bifrost/server/audit_publication.rs:343-396` drives twice the pool bound and checks the bound, overflow metric, spare connection, and drain; available evidence records the exact Postgres journey passing | PASS |
| `FIND-TASK-003-2`: generic Rust PRs run `check` then `test:rust`; full-gate changes run `gate` once; Linux remains excluded from `rust-compat` | `.github/workflows/lints-test.yml:61-115,161-181`; `.github/scripts/tests/test-detect-changes.sh:149-166` | Independently rerun `mise run check:ci-selection`: classifier 32/32 and aggregator 7/7 passed | PASS |
| `FIND-TASK-003-3`: nightly reaches `gate` plus the six missing real-server journey owners without folding in cloud or performance | `.github/workflows/nightly.yml:14-71` uses the prescribed seven-entry matrix; identity and isolated Postgres jobs remain separate | Static matrix closure is exact; available evidence records each six added mise owners passing | PASS |
| Preserve the converged `wyrd-client`/three-SDK ownership, exactly three runtime MCP tools, the one Bifrost data root, the canonical audit staging path/publisher, tenant isolation, and required aggregation | The remediation changes stay in the existing spec, root, audit, and workflow owners; no second client, root, audit path, queue worker, or compatibility facade was added | Prior cumulative review evidence plus candidate source inspection; no regression in these owners found | PASS |
| TASK-003 / REQ-064 / AC-022: every surviving local non-credentialed test has a reachable owning lane with nonzero selection | `handle_crud` is feature-gated in `wyrd-storage/Cargo.toml:80-82`, while its local test is at `tests/handle_crud.rs:111-119`; `backend_contracts` is separately feature-gated at manifest lines 64-66 | No mise lane selects `local_handle_crud` or the `backend_contracts` target; the supplied evidence expressly records both as unreachable | FAIL — `TASK-REV-R1-01` |
| Remediation scope and non-goals: make the five bounded corrections with no new public configuration decision or unrelated refactor | Commit `9c52f8875` replaces the existing storage environment contract with `WYRD_STORAGE_URL`/`WYRD_STORAGE_ENDPOINT_URL` across production parsing, factories, tests, workflow, and public docs | The five retained findings require no storage configuration redesign; removal of test early-return gates and exact lane selection can close zero-test execution using the existing backend settings | FAIL — `TASK-REV-R1-02` |
| Remediation broader proof: all required checks, aggregate, focused capabilities, and gated journeys are recorded against one immutable corrected candidate | The candidate is `9c52f8875`; the recorded broad matrix is labeled as run on `8e266b2b4` with uncommitted storage changes, while only a smaller storage/check subset is recorded on `9c52f8875` | Independent review reran cumulative `git diff --check`, CI selection, data-root tests, and wire tests, but no candidate-SHA `gate`, complete TASK-003 journey matrix, lints, codegen, or docs run is available | FAIL — `TASK-REV-R1-03` |
| REQ-064 / AC-022 and remediation lines 273-278: successful candidate-SHA S3, GCS, and Azure GitHub Actions jobs have attached URLs and statuses | `.github/workflows/storage-integration-cloud.yml` still triggers only on pushes to `main`; the supplied evidence states that no candidate-SHA workflow URL exists | An owner's independent validation assertion is not GitHub Actions evidence and supplies neither immutable run identity nor job status | FAIL — `TASK-REV-R1-04` |
| Preserve prior accepted exceptions and explicit non-goals | The temporary Forge production change was reverted; the retained Forge and client changes are test corrections. No new remediation commit has an AI co-author trailer; no merge, push, release, deployment, history rewrite, or final change review occurred | Commit-message inspection and cumulative diff inspection | PASS |

## Proposed findings

### `TASK-REV-R1-01` — MISSING: two surviving storage tests have no owning lane

- **Violated obligation:** TASK-003 requires every surviving storage unit,
  integration, and journey test to be selected by an owning lane and expressly
  forbids a zero-test selection or a pre-existing-failure waiver. REQ-064 and
  AC-022 require the same closure.
- **Exact location:** `crates/wyrd/wyrd-storage/Cargo.toml:64-66,80-82`;
  `crates/wyrd/wyrd-storage/tests/handle_crud.rs:111-119`; `mise.toml:623-739`.
- **Evidence:** `handle_crud` requires the `emulator` feature, so the default
  `test:wyrd` family skips the whole target. The emulator and cloud tasks invoke
  only the S3, GCS, and Azure exact expressions, never `local_handle_crud`.
  `backend_contracts` requires `cloud`, but no mise task invokes that target.
  The candidate's own evidence at remediation lines 402-408 confirms both are
  unreachable and attempts to waive them as pre-existing, contrary to the task.
- **Observable consequence:** `mise run gate` can pass without executing two
  checked-in storage tests, so its reported test count is not the complete local
  non-credentialed suite required for acceptance.
- **Required testable correction:** add `local_handle_crud` to one existing
  emulator-backed `handle_crud` exact selection. Delete the redundant
  `backend_contracts` target and file: the existing Azurite-owned
  `integration_azurite::azure_abort_on_nonexistent_blob_returns_error` already
  proves the same dispatch/error behavior against the repository-managed
  backend. Rerun the owning storage lane and show both that the local test is
  selected and the redundant orphan target is absent.

### `TASK-REV-R1-02` — DRIFT: test closure was expanded into an unauthorized public storage configuration replacement

- **Violated obligation:** TASK-003-R1 is a decision-complete remediation for
  five findings and prohibits new runtime/configuration scope; TASK-003 is
  repository closeout, not authority to replace an already documented server
  configuration contract. The Ponytail requirement is the smallest correction
  that removes zero-test passes.
- **Exact location:** `crates/wyrd/wyrd-storage/src/settings.rs:21-192` and the
  corresponding factory, server/storage test, workflow, mise, and public-doc
  changes in commit `9c52f8875`.
- **Evidence:** the candidate deletes `WYRD_STORAGE_BACKEND`, local-root,
  bucket/account/container, and provider endpoint variables and replaces them
  with required `WYRD_STORAGE_URL` plus `WYRD_STORAGE_ENDPOINT_URL`. None of
  `FIND-TASK-002-19`, `FIND-TASK-004-1`, or `FIND-TASK-003-1..3` requires that
  public behavior. The stated defect was tests returning success when their
  gate variables were absent; deleting those early returns and using the
  existing exact mise owners closes it without changing production parsing.
- **Observable consequence:** every deployment using the candidate's prior
  documented storage variables now fails boot, while a large production and
  documentation diff enters an acceptance task that supplied no approved
  product/configuration decision for it.
- **Required testable correction:** remove the storage URL consolidation and
  retain the existing production storage settings contract. Keep only the
  minimum test/lane changes needed to eliminate conditional early-success and
  prove nonzero backend selection. If one-URL storage configuration is desired,
  route it through a separately approved specification rather than this
  remediation.

### `TASK-REV-R1-03` — MISSING: the required matrix was not proved on the immutable final candidate

- **Violated obligation:** TASK-003-R1 lines 259-278 require all broader checks,
  `gate`, focused TASK-003 capabilities, and gated journeys to be recorded
  against one immutable corrected candidate. REQ-064 and AC-022 prohibit
  substituting partial or unidentifiable working-tree evidence.
- **Exact location:** remediation evidence lines 302-348.
- **Evidence:** the broad matrix, including `gate`, is explicitly recorded at
  commit `8e266b2b4` while the storage consolidation existed only as
  uncommitted working-tree content. Commit `9c52f8875` then adds 21 storage,
  workflow, mise, and documentation paths. Its recorded candidate-specific
  runs cover storage lanes and only three checks, not the required broad matrix.
  A claim that an unrecorded dirty tree was identical is not an immutable tree
  identity and cannot establish what `gate` tested.
- **Observable consequence:** acceptance cannot tie the claimed 6,025-test
  aggregate or the full journey matrix to the reviewed candidate tree; a
  candidate-only regression can therefore be reported green.
- **Required testable correction:** after source remediation, run and record
  every command in TASK-003-R1's broader-verification section against the final
  commit/tree, with nonzero exact selections for every named focused and gated
  journey.

### `TASK-REV-R1-04` — MISSING: mandatory candidate-SHA live-cloud evidence does not exist

- **Violated obligation:** REQ-064, AC-022, TASK-003, and TASK-003-R1 lines
  273-278 require passing S3, GCS, and Azure jobs in the owning GitHub Actions
  workflow, attached by run URL and status for the immutable candidate.
- **Exact location:** `.github/workflows/storage-integration-cloud.yml:1-130`;
  remediation evidence lines 394-400.
- **Evidence:** the evidence explicitly states that the workflow still runs
  only after a push to `main`, has no `workflow_dispatch`, and produced no
  candidate-SHA URL. The replacement is only an assertion that the owner
  validated the providers independently.
- **Observable consequence:** there is no auditable proof that the candidate's
  real-provider credentials, signing, addressing, and multipart paths pass;
  emulator or informal external runs cannot satisfy the specified gate.
- **Required testable correction:** make the existing live-cloud workflow
  runnable for the review candidate without merging it, run its existing S3,
  GCS, and Azure jobs at the final candidate SHA, and attach each immutable run
  URL and passing job status. No new storage abstraction or test suite is
  needed.

## Prior-finding closure

| Stable finding | Result |
|---|---|
| `FIND-TASK-002-1` through `FIND-TASK-002-18` | Remain closed under the prior validated dispositions and explicit owner exceptions; no cumulative regression found |
| `FIND-TASK-002-19` | CLOSED by deletion and focused proof |
| `FIND-TASK-004-1` | CLOSED by the locked managed-directory probes and focused proof |
| `FIND-TASK-003-1` | CLOSED by bounded pre-spawn ownership and the available journey proof |
| `FIND-TASK-003-2` | CLOSED by the Linux CI owner and independently rerun selection proof |
| `FIND-TASK-003-3` | CLOSED by the nightly matrix and available journey evidence |

## Verification notes and limits

- Independently passed: cumulative `git diff --check`; `mise run
  check:ci-selection` (32 classifier and 7 aggregator cases); all four
  `boot::data_root::tests`; and all eleven `bifrost_wire_tests`.
- Source-inspected: the complete cumulative diff inventory; the full
  remediation range `bbfdf35e2..9c52f8875`; all five prescribed correction
  owners and their focused proofs; storage settings/factories/tests/mise lanes;
  nightly and PR workflows; prior reports; original tasks; and spec revision 8.
- Not independently rerun: the Postgres-backed Oracle audit journey, focused
  MCP test, `gate`, lints, codegen, docs, all TASK-003 journey lanes, or any
  credentialed cloud job. Their supplied results were treated as available
  evidence only where tied to a stable commit or exact source path.
- No candidate-SHA GitHub Actions live-cloud URL/status was supplied. Current
  workflow triggers do not permit a review-branch manual dispatch.

## Overall result

**FAIL**

The five retained source findings are implemented, but the cumulative candidate
does not satisfy TASK-003-R1 exactly: two surviving storage tests remain
unreachable, the last commit introduces an unnecessary breaking storage
configuration redesign, and neither the required immutable final-candidate
local matrix nor candidate-SHA live-cloud evidence exists.
