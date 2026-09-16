# Repository Integration Domain Review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `9c52f887595d4e8a04764d3d2836926e5df70950`
- Candidate tree: `e2731ff2d23fc1a0f4b356ccdd79caf3f6c7b93e`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 8
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-003-close-repository-integration.md`
- Remediation task: `changes/active/surfaces-oracle-integration/review/tasks-002-r3-004-003-bbfdf35e-review-01/TASK-003-R1-close-cumulative-review-findings.md`

The repository has no `.codegraph/` directory, so inspection used Git object reads, `rg`/`git grep`, the complete base-to-candidate diff, and GitHub Actions run metadata. The checked-out `HEAD` is not the candidate; all candidate-source references below were read from the immutable commit rather than the working tree.

## Reviewed boundary

Repository integration, affected-code CI selection, stable required-job aggregation, nightly journey ownership, mise task closure, emulator and live-cloud storage workflows, nonzero test selection, and final-candidate verification credibility. The review also assessed the storage-configuration consolidation and three post-remediation gate fixes against the remediation task's scope, constraints, and non-goals.

## Authority and source coverage

| Surface | Governing authority | Source inspected | Result |
|---|---|---|---|
| Pull-request lane selection and aggregation | Spec REQ-033, REQ-035, AC-008; TASK-003; remediation `FIND-TASK-003-2`; `AGENTS.md` §§11-12 | `.github/scripts/detect-changes.sh`, both shell self-tests, `verify-required-jobs.sh`, `.github/workflows/lints-test.yml`, `mise.toml` | PASS |
| Nightly, performance, emulator, and live-cloud separation | Spec REQ-034, REQ-035A, REQ-064, INV-025, AC-008, AC-022; remediation `FIND-TASK-003-3`; testing-workflows reference | `.github/workflows/{nightly,performance,storage-integration-emulator,storage-integration-cloud}.yml`, journey and aggregate tasks in `mise.toml` | FAIL: `RI-R1-02`, `RI-R1-04`, `RI-R1-05` |
| Storage test reachability and nonzero selection | Spec REQ-064, INV-025, AC-022; TASK-003 constraint forbidding empty selection; `AGENTS.md` test taxonomy | `wyrd-storage/Cargo.toml`, all storage integration targets, `test:rust`, `test:storage:*`, and `gate` task closure | FAIL: `RI-R1-02` |
| Remediation scope and minimum correction | Remediation outcome, Ponytail table, constraints, and non-goals; spec REQ-003; implementation-execution material-change boundary; `AGENTS.md` §15 | Complete `bbfdf35e2..9c52f8875` diff, storage settings/factories/docs/tests, post-remediation commits | FAIL: `RI-R1-01`, `RI-R1-03` |
| Final immutable evidence | Spec REQ-064, INV-025, AC-022; TASK-003 verification; remediation broader verification and live-cloud evidence requirements | Remediation evidence record, candidate commit graph, `gh run list` for the branch and cloud workflow | FAIL: `RI-R1-04`, `RI-R1-05` |

## Material proposed findings

### RI-R1-01 — DRIFT: the zero-selection repair became an unapproved production storage-configuration redesign

- **Violated obligation:** The remediation outcome is limited to five named corrections and its non-goals prohibit a new public API or runtime knob. Its Ponytail decision for the CI findings says to reuse existing jobs and mise tasks. Spec REQ-003 requires another human-approved specification revision for a material conflict not already decided, and the implementation-execution authority classifies a public configuration-contract change as material.
- **Location:** `crates/wyrd/wyrd-storage/src/settings.rs:22-202`; `crates/wyrd/wyrd-storage/src/factory/{s3,gcs,azure}.rs`; `docs/src/content/docs/self-hosting/{configuration,index,local-development,storage}.svx`; commit `9c52f8875`.
- **Evidence:** The candidate deletes `WYRD_STORAGE_BACKEND` and every backend-specific bucket/account/container/endpoint setting, removes the explicit S3 path-style setting, and makes the new required `WYRD_STORAGE_URL` plus `WYRD_STORAGE_ENDPOINT_URL` the production boot contract. This changes server configuration, public docs, factories, the Iceberg warehouse derivation, and every storage test. The reported defect was that boolean test gates could skip with success; those gates could be removed and the existing backend settings supplied by the owning mise tasks without changing production configuration. No approved requirement or remediation criterion selects this new URL contract.
- **Observable consequence:** Every existing self-hosted or cloud deployment configured with the candidate's previously documented variables now fails boot for missing `WYRD_STORAGE_URL`; S3-compatible deployments also lose the independent `force_path_style` choice. The cumulative candidate therefore contains a material externally observable change unrelated to closing the five validated findings.
- **Testable correction:** Revert the production configuration/factory/documentation redesign and close zero-selection at the existing test/mise owners: remove the skip gates, provide the existing backend variables in each exact emulator/cloud task, and assert the expected backend. If one URL is desired as product behavior, obtain an approved specification revision before implementing it.

### RI-R1-02 — VIOLATION: two repository-owned storage tests remain unreachable

- **Violated obligation:** Spec REQ-064, INV-025, and AC-022 require every repository-owned local non-credentialed test to execute with nonzero selection; TASK-003 and the remediation task expressly reject empty or omitted selections.
- **Location:** `crates/wyrd/wyrd-storage/Cargo.toml:64-82`; `crates/wyrd/wyrd-storage/tests/handle_crud.rs:111-119`; `crates/wyrd/wyrd-storage/tests/backend_contracts.rs:1-45`; `mise.toml:390-398,623-744`.
- **Evidence:** `handle_crud` has `required-features = ["emulator"]`, so the default-feature family invoked by `test:rust` does not compile or execute `local_handle_crud`. Every explicit handle task selects only one of the three cloud-named tests. The comment at `mise.toml:711-712` incorrectly claims `local_handle_crud` runs through default `cargo nextest run -p wyrd-storage`. `backend_contracts` requires `cloud`, but no mise task or workflow names that target; all-feature storage tasks explicitly select other test binaries. The remediation evidence itself acknowledges both tests as unreachable and calls them pre-existing even though `handle_crud.rs` was materially rewritten by `9c52f8875` and the task forbids baseline omissions.
- **Observable consequence:** `gate`, nightly, emulator CI, and live-cloud CI can all pass without executing these two checked-in tests, so the claimed complete/nonzero storage closure is false.
- **Testable correction:** Add the two exact tests to the existing storage matrix or another existing owning lane with the required feature set, and record their nonzero counts. Do not create another aggregate or checker.

### RI-R1-03 — VIOLATION: a post-remediation Forge change weakens a durability assertion inside an explicit non-goal

- **Violated obligation:** The remediation task excludes rare settlement-cancellation remediation and prohibits weakening an assertion to obtain a green gate. Its broader verification is proof work, not authority to change excluded behavior.
- **Location:** `crates/vala/vala-bifrost-redux/tests/integration/forge/compaction_admission.rs:2576-2588`; commit `c18f0ce07`.
- **Evidence:** The test formerly required every returned attempt error after durable recovery to be the checkpoint shutdown result. The candidate broadens the assertion to also accept the text `refused before commit: Cancelled`. `stop_worker` cancels the shared worker token, but `returned_errors()` is a process-wide list and the assertion does not bind that refusal to the stop or to the recovered attempt. The implementation record expressly says this touches publication-cancellation settlement, which the task lists as a non-goal. The intermediate production change was reverted, but the broader assertion remains.
- **Observable consequence:** The recovery journey can now pass when an attempt reports a publication refusal that the prior proof treated as unexpected; it no longer proves that the recovered operation settled without a publication failure.
- **Testable correction:** Restore the original assertion and make the test deterministically wait for the recovered publication/attempt to settle before stopping the worker, or route any desired change in cancellation semantics through separately authorized work. Keep the proof tied to the exact recovered attempt rather than matching process-wide error text.

### RI-R1-04 — MISSING: the broad local proof is not attached to one immutable corrected candidate

- **Violated obligation:** Spec REQ-064, INV-025, and AC-022 and the remediation task's broader-verification section require `gate`, every named focused lane, and nonzero selections to be recorded against one immutable corrected candidate.
- **Location:** remediation task `Broader verification on 8e266b2b4` and `Storage configuration consolidated onto one variable (9c52f8875)` evidence sections.
- **Evidence:** The only reported successful `mise run gate` ran at commit `8e266b2b4` with uncommitted storage changes. No tree identity for that dirty state was recorded, so it cannot be independently equated to candidate tree `e2731ff2...`. Candidate `9c52f8875` records only format/lint/docs and storage-focused commands. No full gate or complete TASK-003 lane matrix is recorded at the immutable candidate SHA.
- **Observable consequence:** The review cannot establish that the exact candidate, including the 1,012-line storage/configuration change, passes the repository aggregate and all required journeys together.
- **Testable correction:** Run and record `mise run gate`, every TASK-003/remediation focused lane, and the exact nonzero selections on the committed candidate (or a new immutable corrected candidate), followed by the cumulative `git diff --check`.

### RI-R1-05 — MISSING: required candidate-SHA live-cloud GitHub Actions evidence does not exist

- **Violated obligation:** Spec REQ-064 and AC-022, TASK-003 acceptance, and the remediation task require successful candidate-SHA S3, GCS, and Azure jobs with run URLs/statuses.
- **Location:** `.github/workflows/storage-integration-cloud.yml:10-23,49-130`; remediation task `Live-cloud evidence` section.
- **Evidence:** The workflow triggers only on pushes to `main`; its weekly schedule remains commented out and it has no `workflow_dispatch`. `gh run list --branch change/surfaces-oracle-integration` returned no runs. `gh run list --workflow storage-integration-cloud.yml` returned only older failed `main` runs and none at `9c52f8875`. The implementation record substitutes an owner's unspecified independent validation and explicitly states that no candidate-SHA workflow URL exists.
- **Observable consequence:** Real IAM/OIDC, signed URL, endpoint, and provider-specific behavior for the actual candidate remains unproved, and the required evidence is not reproducible or auditable.
- **Testable correction:** Make the existing cloud workflow runnable for the reviewed ref through an authorized mechanism, obtain successful S3/GCS/Azure jobs whose `headSha` is the corrected candidate, and record their URLs and conclusions. Emulator or owner-attested results are not equivalent to the expressly required GitHub Actions proof.

## Confirmed passing behavior

- A generic Rust pull request now enters the existing Linux `ci` job and runs `mise run check` followed by `mise run test:rust`; full-gate changes run only `mise run gate`. The Ubuntu compatibility exclusion avoids duplicate Linux execution.
- The CI-selection self-test inspects that command branch, and the required-job aggregator correctly accepts `success`/`skipped` while rejecting failure, cancellation, missing, and empty results.
- Nightly uses the existing `gate` job as a seven-entry native matrix for `gate` plus the six missing real-server journey owners. Identity and isolated-Postgres checks remain explicit, while production geometry and live-cloud remain in separate workflows. Storage emulators are already reached by `gate -> test:rust -> test:storage:matrix` and are not duplicated in the nightly matrix.
- The Card upload-completion race fix reuses the existing SQL owner, makes completion idempotent only for `pending`/`completed`, preserves the first timestamp, and keeps aborted/failed rows rejected; its focused SQL proof covers both sides.
- The SDK retry test replaces a fixed sleep with bounded observation of the retained retry owner and does not change production behavior.
- `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..9c52f887595d4e8a04764d3d2836926e5df70950` passes, and the candidate contains no removed storage gate-variable references outside change records.

## Verification limits

- This review did not rerun the long Cargo, Postgres, Docker, Python, TypeScript, nightly, performance, or cloud suites. Their reported results were treated as claims until tied to independently inspectable source or external run metadata.
- Static source and task-closure inspection was complete for this domain. GitHub run metadata was queried read-only and confirmed the absence of candidate/branch cloud evidence.
- The working tree contains concurrent review artifacts and unrelated `verified-change-contract` changes. None was used as candidate implementation evidence, and only this report was written.

## Overall result

**FAIL** — the generic Rust and nightly source corrections are present, but the candidate adds an unapproved production storage configuration redesign, leaves two storage tests unexecuted, weakens an excluded Forge cancellation assertion, and lacks both immutable-candidate aggregate proof and the expressly required candidate-SHA live-cloud evidence.
