# Domain Review: Repository Integration

## Reviewed Boundary

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `bbfdf35e26212b2a831bda5e31e1ef4433e41900`
- Candidate tree: `58f5c2acfe8da01f3e4b58363f38ce761626df35`
- Scope: TASK-003 repository closeout, including the cumulative TASK-002/R3 and TASK-004 results where they affect workspace membership, CI selection and aggregation, nightly/cloud/performance separation, generated artifacts, native build artifacts, and verification credibility.

## Authority and Source Coverage

| Boundary | Governing authority | Source/evidence inspected | Result |
|---|---|---|---|
| PR affected-code selection and required aggregation | Spec REQ-033, REQ-035, AC-008; TASK-003 acceptance criteria | `.github/scripts/detect-changes.sh`, both shell self-tests, `.github/workflows/lints-test.yml`, identity and storage emulator workflows | **FAIL**: DRI-01 |
| Nightly correctness and workflow separation | Spec REQ-034, REQ-035A, REQ-064, INV-025, AC-008, AC-022 | `.github/workflows/nightly.yml`, `performance.yml`, `storage-integration-cloud.yml`, `mise.toml`, test-family scripts | **FAIL**: DRI-02 |
| Generated-artifact provenance and drift | Spec REQ-036, INV-012, AC-020; AGENTS.md generated-artifact rules | `codegen:{regen,check}`, `ts:napi:check`, `check:proto-drift`, gate dependency list, generators and tracked descriptor | **FAIL**: DRI-05 |
| Workspace/build graph and native artifacts | Spec REQ-045, INV-016, INV-023, AC-020 | workspace manifests/lock, `cargo metadata --locked --no-deps`, tree inventory, TypeScript package/build tasks | **PASS**: one `vala-bifrost-redux` engine package; SDK packages are workspace members; no tracked `*.node` file |
| Required completion evidence | Spec REQ-064, INV-025, AC-009, AC-022; TASK-003 verification and acceptance criteria | TASK-003 implementation evidence, commit history, GitHub Actions run inventory | **FAIL**: DRI-03, DRI-04, DRI-06 |
| Check/waiver integrity | AGENTS.md and agent-rules prohibition on weakened checks; spec REQ-037, INV-014, INV-025 | cumulative script/check changes, especially `55a298885` | **PASS**: the deleted Oracle-admission self-test named a deleted owner; surviving admission-table RLS checks remain. No material waiver was found in this domain. |

## Test Coverage Analysis

### Current Coverage

- `.github/scripts/tests/test-detect-changes.sh` passes 31 static classifier assertions, including generic Rust, Bifrost-only, mixed, global, unclassified, planning-only, and storage examples. It proves emitted flags, not that the resulting workflow matrix executes the intended commands.
- `.github/scripts/tests/test-verify-required-jobs.sh` passes seven success/skipped/failure/cancelled/missing/empty aggregation cases. It proves the shell aggregator, not the job graph feeding it.
- `nightly.yml` explicitly owns `mise run gate`, the identity journey, and the four isolated-Postgres checks. `performance.yml` separately owns Forge production geometry. The cloud workflow is separate.
- TASK-003 records passing focused lanes across several intermediate commits and records local fixes for every observed failure. It does not record one successful final aggregate at the reviewed candidate.
- `codegen:check` regenerates and hashes OpenAPI, JSON schemas, the generated TypeScript error-code file, and public Python stubs; `ts:napi:check` checks the generated N-API declaration.
- Static review commands passed: both CI shell self-tests; `git diff --check 861f8d86c..bbfdf35e`; `cargo metadata --locked --no-deps`; candidate tree inventory contains no tracked `*.node`.

### Gaps

#### DRI-01 — INCORRECT: generic Rust pull requests select no Rust test execution

- **Violated obligation:** REQ-033 requires PR verification lanes selected from affected code and dependency closure; TASK-003 requires every crate and surviving journey to have a credible owning lane.
- **Location:** `.github/workflows/lints-test.yml:40-59,61-109,155-175`; `.github/scripts/tests/test-detect-changes.sh` `generic Rust runs focused lanes` case.
- **Evidence:** for a PR, `matrix-plan` emits only `os=["ubuntu-24.04"]`. `rust-compat` then excludes `ubuntu-24.04`, leaving no Rust-test matrix entry. The `ci` job runs `mise run check` when `full_gate=false`; `mise.toml:62-68` shows that command is formatting plus Clippy, not tests. The classifier self-test asserts only `rust=true bifrost_only=false full_gate=false`, so it cannot detect the job-graph omission.
- **Observable consequence:** an ordinary change such as `crates/skald/skald-agent/src/lib.rs` can make crate tests fail while the stable `ci complete` check receives only successful/skipped upstream results.
- **Required testable correction:** keep the focused PR policy, but ensure one selected Linux job runs the owning Rust test lane for generic Rust changes. Add a workflow-topology assertion that the generic-Rust PR scenario resolves to a nonempty Rust-test execution, then run `mise run check:ci-selection`.

#### DRI-02 — MISSING: nightly is not the complete non-credentialed correctness suite

- **Violated obligation:** REQ-034; REQ-064; INV-025; AC-008; AC-022.
- **Location:** `.github/workflows/nightly.yml:14-95`; `mise.toml:1361-1412` (`gate`); focused journey owners at `mise.toml:102-135,918-952,1299-1318`.
- **Evidence:** nightly invokes only `gate`, identity, and four Postgres checks. `gate` includes default Rust families, Bifrost, Python unit, and TypeScript unit tasks, but not the separately gated `test:cli:journey`, `test:wyrdstate:journey`, `py:test:integration` (including Python Cards/WyrdState), or full `ts:test:integration` (including the new `cards-state.test.ts`). Default family and unit invocations do not run ignored/integration-marked journeys. The existing CI self-tests do not inspect nightly task closure.
- **Observable consequence:** nightly can be green while required real-server CLI, WyrdState, Python, or TypeScript Card/state workflows are broken, contrary to the explicit rule that default exclusion is not passing evidence.
- **Required testable correction:** add the existing focused journey owners to nightly correctness (directly or through one existing aggregate that actually contains them) and add a static workflow-closure test proving each required non-credentialed lane is selected exactly once. Do not move performance or live-cloud work into nightly.

#### DRI-03 — MISSING: no successful final `mise run gate` or complete candidate-local lane matrix is recorded

- **Violated obligation:** REQ-064, INV-025, AC-022, and TASK-003 lines 95-125 require a passing broad gate, all focused lanes, and every gated journey on the final integrated candidate.
- **Location:** `changes/active/surfaces-oracle-integration/tasks/TASK-003-close-repository-integration.md:158-215,267-270`.
- **Evidence:** the task's own matrix marks this criterion `BLOCKED`. Every recorded `mise run gate` failed: at `cd07c2737`, at `fb7e7c395`, and at `30751d469`. At `19769e24c`, individual dependencies were run, but eight of nine Bifrost lanes passed; later commits record only the storage matrix and one TypeScript Bifrost journey. There is no successful `mise run gate` at `bbfdf35e` and no final-candidate record showing every named focused lane after the last fixes.
- **Observable consequence:** the reviewed candidate has no credible proof that the aggregate scheduling/feature union and all required journeys pass together; INV-025 makes an unexecuted required lane a completion failure.
- **Required testable correction:** on the immutable corrected candidate, run and record `mise run gate`, every focused command named by TASK-003, and `git diff --check 861f8d86c..<candidate>`, with nonzero test counts for each selected test/journey lane.

#### DRI-04 — MISSING: required credentialed live-cloud GitHub Actions evidence does not exist

- **Violated obligation:** REQ-064 and AC-022 explicitly require passing credentialed live-cloud workflows; TASK-003 lines 95-97 and 122 require attached passing GitHub Actions evidence.
- **Location:** `.github/workflows/storage-integration-cloud.yml`; TASK-003 open items at lines 267-270.
- **Evidence:** `gh run list --branch change/surfaces-oracle-integration ...` returned no runs. `gh run list --workflow storage-integration-cloud.yml` returned only older default-branch failures, none for candidate `bbfdf35e`. The task itself says the credentialed workflows need a push and secrets. The cloud workflow also has its weekly schedule commented out (`storage-integration-cloud.yml:18-23`), so no scheduled candidate proof can arise before activation or a qualifying main push.
- **Observable consequence:** real IAM/OIDC, signed-URL, provider endpoint, and provider-specific failure behavior for S3, GCS, and Azure remains unproved. The approved specification treats this as a completion failure, not a review limitation.
- **Required testable correction:** make the reviewed candidate reachable by the authorized GitHub workflow mechanism, provision the named secrets, obtain successful S3, GCS, and Azure jobs from `storage-integration-cloud.yml`, and attach the run URLs/SHAs to TASK-003 evidence. Do not replace them with emulator results.

#### DRI-05 — INCORRECT: protobuf drift has no owner in the claimed aggregate/codegen closure

- **Violated obligation:** REQ-036, INV-012, TASK-003's requirement that regeneration be clean across protobuf, and its statement that the final gate supplies repository-wide checks in its dependency closure.
- **Location:** `mise.toml:1194-1238,1346-1349,1361-1412`; tracked `crates/wyrd/wyrd-tonic/proto/wyrd.v1.bin`.
- **Evidence:** `codegen:regen` and its checksum inventory do not regenerate or snapshot the protobuf descriptor. The dedicated `check:proto-drift` exists, but no task depends on it: repository search finds only its task definition. It is absent from `gate`, `check`, `test:tonic`, and GitHub workflows. TASK-003 provides no exact successful `mise run check:proto-drift` evidence at the candidate.
- **Observable consequence:** a stale checked-in `wyrd.v1.bin` reflection descriptor can pass `codegen:check`, `mise run gate`, nightly, and PR CI while runtime reflection/tool consumers see a contract different from the compiled `.proto` service.
- **Required testable correction:** put the existing `check:proto-drift` in the smallest existing generated/aggregate owner that satisfies TASK-003 (without another checker), add a closure assertion, and record its successful execution at the corrected candidate.

#### DRI-06 — MISSING: the required final requirement/invariant evidence map is absent

- **Violated obligation:** AC-009 and TASK-003 lines 98-100 require final static review mapping every requirement and invariant to credible evidence before merge proposal.
- **Location:** `.dev/merge-audits/surfaces-oracle-integration/{review-ledger.json,merge-inventory.md}` and TASK-003 implementation-evidence table.
- **Evidence:** the merge ledger maps conflict dispositions and the TASK-003 table maps seven task-level criteria. Neither is a final revision-8 matrix covering every `REQ-*` and `INV-*`. TASK-003 itself marks this row `BLOCKED on AC-009 review` and names the future `$wyrd-change-review` as open work.
- **Observable consequence:** there is no independent end-to-end proof that later spec revisions and cumulative remediation preserved every locked obligation before merge readiness is claimed.
- **Required testable correction:** after bounded task findings and required runs are closed, execute the required `$wyrd-change-review` against the immutable cumulative candidate and preserve its complete REQ/INV evidence mapping and verdict. Do not treat the implementation record as its own independent review.

### Recommended Verification

- `mise run check:ci-selection` — prove classifier and required-result cases, extended to exercise the actual job/task closure rather than flags alone.
- `mise run check:proto-drift` — prove the checked-in descriptor matches the authoritative `.proto`.
- `mise run gate` — required final aggregate; record the candidate SHA and all nonzero selections.
- `mise run test:cli:journey`, `mise run test:wyrdstate:journey`, `mise run py:test:integration`, `mise run ts:test:integration` — close the currently absent nightly/final real-server journey evidence.
- Every remaining focused command listed in TASK-003, followed by `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..<candidate>` — prove the final candidate rather than intermediate commits.
- Successful S3, GCS, and Azure jobs from `.github/workflows/storage-integration-cloud.yml` with run URLs and matching candidate SHA — required credentialed integration proof.

### Residual Risk

- Heavy Cargo, Postgres, Docker, language-runtime, and cloud suites were not re-run during this read-only review. The report distinguishes locally reproducible static results from implementation-record claims.
- GitHub-hosted nightly and performance definitions are not present on the current default branch, and no Actions run exists for the candidate branch. Their YAML shape is reviewable; hosted execution remains unproved.
- The present unrelated untracked directory `changes/active/verified-change-contract/architecture/verifier/` was not modified and is outside the reviewed candidate.

## Verification Limits and Immutability

- Candidate identity was checked before review: commit `bbfdf35e26212b2a831bda5e31e1ef4433e41900`, tree `58f5c2acfe8da01f3e4b58363f38ce761626df35`.
- The reviewed source was not edited. Only this assigned report was created.
- Absence of external evidence is classified as task failure because REQ-064 and AC-022 explicitly make passing live-cloud GitHub Actions a completion obligation.

## Overall Result

**FAIL** — six material, reachable repository-integration gaps remain. The immutable subject and governing authority were available, so this is not `BLOCKED`.
