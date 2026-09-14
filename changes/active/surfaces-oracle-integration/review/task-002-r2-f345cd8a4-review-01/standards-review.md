# Repository Standards Review — TASK-002 R2

## Immutable Review Subject

- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `f345cd8a4fdb5566ae6828ffb4f29d64f7f17599`
- Worktree: `/tmp/wyrd-task-002-r2-review-f345cd8a4`
- Scope: the complete cumulative diff (`382` files), including the intervening authentication, SQL, Vala/Bifrost, generated-contract, SDK, documentation, and build changes.
- Review posture: repository-standards audit only. No implementation file was modified.

## Authority Coverage

| Changed surface | Applicable authority reviewed | Coverage result |
| --- | --- | --- |
| Shared `wyrd-client`, Cards registration/storage relocation, Bifrost facade, Rust SDK | `AGENTS.md` §§2–6, 9–12, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/bifrost-design.md`; `architecture/references/languages/rust-core.md`; `architecture/references/architecture/patterns.md`; `architecture/references/contracts/errors.md`; `architecture/references/languages/testing-workflows.md`; `TESTING.md` | Reviewed ownership, dependency direction, struct-centered shape, async boundaries, public error projection, documentation, imports, and tests. Findings `STD-001` through `STD-003` apply. |
| Python SDK, PyO3 aggregation, package moves, stubs, tests | `AGENTS.md` §§3, 7–8, 11–12, 16; `architecture/references/languages/pyo3-boundaries.md`; `architecture/references/languages/python-api-and-stubs.md`; error and testing references | Reviewed wrapper ownership, feature boundaries, public imports, generated stubs, top-level test shape, and error-detail coverage. Boundary checks passed. |
| TypeScript SDK/N-API declarations, examples, tests | `AGENTS.md` §§2–4, 9–12, 16; `architecture/references/languages/typescript-guide.md`; error and testing references | Reviewed facade projection, native binding placement, declarations, error details, and runtime-specific coverage. Import/signature violations remain (`STD-003`). |
| HTTP server, OpenAPI, MCP/CLI/docs, generated schemas | `AGENTS.md` §§2, 9–12, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/bifrost-design.md`; error and testing references | Reviewed typed errors, `application/problem+json` declarations, client vocabulary, generated artifacts, and docs. No material contract-drift finding. |
| Authentication/OIDC, tenant SQL, system-owner handling | `AGENTS.md` §§2, 9–12, 16; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; Wyrd and Bifrost design authorities | Reviewed fail-closed lookup shape, tenant-scoped access, sentinel use, error handling, and tests. One fallible relocated method remains under-documented (`STD-002`). |
| Vala Oracle/Scribe/Forge, WAL, migrations, reliability tests | `AGENTS.md` §§2–6, 9–12, 16; `architecture/agent-rules.md`; `architecture/bifrost-design.md`; applicable Vala/OLAP/reliability references routed by `architecture/references/README.md` | Reviewed tenant isolation, system-owner exception scope, operational/audit boundaries, SQL changes, recovery behavior, and test structure. A function-scoped import violates the module-top rule (`STD-003`). |
| Cargo manifests, `mise`, checks, release/build workflows | `AGENTS.md` §§4, 11–13, 16; `architecture/references/languages/testing-workflows.md`; `TESTING.md`; `architecture/references/languages/spec-driven-development.md` | Reviewed dependency/profile rules, verification wiring, scope checks, diff hygiene, and commit metadata. Findings `STD-001`, `STD-004`, and `STD-005` apply. |
| Active-change artifacts and review evidence | `AGENTS.md` §§13–14; `architecture/references/languages/spec-driven-development.md`; `$wyrd-task-review` | Reviewed cumulative evidence without reading another task reviewer's conclusions or intended verdict. Cumulative diff hygiene evidence is incorrect (`STD-004`). |

## Per-Rule Results

| Repository rule | Result | Evidence |
| --- | --- | --- |
| Durable behavior stays server-owned; SDKs project the shared Rust client facade | PASS | The Rust, Python, and TypeScript surfaces project `wyrd_client::Bifrost`/shared client behavior; no separate durable client implementation was found. |
| Client-tier dependency and PyO3 ownership boundaries | PASS | `mise run check:client-tier`, `mise run check:pyo3-scope`, and `mise run check:py-wheel-no-testing` passed. `wyrd-spec` remains PyO3-free. |
| Public errors use typed Wyrd errors and machine-readable details | PASS | The remediated Python/TypeScript projections distinguish field/reason, transport, and empty fallback details; the seven Bifrost operations declare problem JSON. |
| Tenant isolation, system-owner exception scope, and audit ownership | PASS | The changed auth/SQL/Vala paths retain typed tenant context and constrain the system-owner WAL exception to the audit-log recovery path. No contradictory path was found. |
| Struct-centered Rust ownership and earned async boundaries | PASS | The new/relocated workflows retain concrete owners; reviewed async functions await filesystem, network, database, or bounded concurrent transfer work. |
| Every new or materially modified Rust item has complete rustdoc; fallible functions document `# Errors`; cancellable workflows document partial progress | **FAIL** | See `STD-002`. The added-line scan is not sufficient for relocated items whose signature lines predate the remediation commits. |
| Types used in fields/signatures/bounds are imported and written as bare names; `use` declarations stay at module scope | **FAIL** | See `STD-003`. Both qualified signature types and a function-scoped `use` remain in cumulative changed code. |
| No per-crate profile blocks | **FAIL** | See `STD-001`. The SDK Python manifest adds an ignored `[profile.release]`. |
| Python/PyO3 public exports, stubs, and runtime-owned tests stay synchronized | PASS | The package and extension aggregation follow the SDK boundary; recorded `codegen:check`, Python typecheck, and unit evidence passes. |
| TypeScript declarations and runtime-owned tests cover the changed projection | PASS | Recorded TypeScript build/typecheck/unit evidence passes and the tests assert transport detail shape. The recorded `Bifrost.connect` empty-URL limitation is outside this remediation's stated contract. |
| Generated contracts and OpenAPI stay source-aligned | PASS | Recorded `codegen:check` passes; static inspection found the seven Bifrost error responses declared as `application/problem+json`. |
| Verification evidence names and runs focused tests without zero-test filters | PASS | The evidence supplies exact `nextest` expressions for the two named Rust proofs and reports both passing. |
| Cumulative source diff is whitespace-clean | **FAIL** | See `STD-004`. `git diff --check <base>..<candidate>` reports two errors. |
| No gate is weakened or bypassed | PASS | No added `#[allow]`, ignored test, deleted assertion, or broadened boundary rule hiding an observed violation was found. |
| Contributor identity is preserved and commits contain no AI co-author trailers | **FAIL** | Commit authors are the configured contributor, but 22 candidate-range commits include prohibited AI `Co-Authored-By` trailers; see `STD-005`. |
| No legacy public Bifrost client vocabulary remains in docs/public SDK surfaces | PASS | Docs use `AsyncBifrost`/`Bifrost.connect`; no public `BifrostQueryClient` reference was found. |

## Material Findings

### STD-002 — VIOLATION: rustdoc remediation does not cover materially relocated fallible workflows

- Violated rule: `AGENTS.md` §16 requires rustdoc for every new or materially modified Rust item, a `# Errors` section on every fallible function, and cancellation/partial-progress documentation where cancellation is relevant. The R2 task expressly includes materially relocated items.
- Evidence:
  - `crates/shared/wyrd-client/src/cards/config.rs:15` — fallible `load` has no `# Errors` section.
  - `crates/shared/wyrd-client/src/cards/saga/build_submission.rs:23` — fallible async `prepare` has no `# Errors` section and does not state its cancellation/partial-progress behavior.
  - `crates/shared/wyrd-client/src/cards/saga/complete.rs:17`, `:33`, `:55`, `:69`, and `:96` — the completion orchestration and helpers return `Result` but their rustdoc omits `# Errors`; the async network workflow also omits cancellation/partial-progress behavior.
  - `crates/shared/wyrd-client/src/cards/saga/hash_artifacts.rs:16` — fallible async `validate_and_stamp` omits `# Errors` and cancellation behavior.
  - `crates/shared/wyrd-client/src/cards/saga/upload.rs:25` — the bounded multi-upload workflow omits `# Errors` and the observable partial-progress/cancellation contract.
  - `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:118` — fallible async `trusted_issuer` has no `# Errors` section.
- Consequence: repository-required API/workflow documentation remains incomplete, and the claimed zero-remaining scan produces a false negative because it keys off added lines rather than the materially relocated item set.
- Required correction: audit every new or materially relocated Rust item in the cumulative task by current symbol, not only by added source lines. Add accurate `# Errors` sections to every fallible function and cancellation/partial-progress text to async workflows where work can have begun before cancellation. Re-run the item audit and record its method and result.

### STD-003 — VIOLATION: cumulative changed Rust still uses qualified signature types and a function-scoped import

- Violated rule: `architecture/agent-rules.md` requires types in fields, signatures, and bounds to be brought into scope and written by bare name; all `use` declarations must be at module scope, with the test exception applying to the test module rather than an individual function.
- Evidence:
  - `crates/shared/wyrd-client/src/bifrost/facade.rs:121` — `Arc<dyn wyrd_queue::BatchSink<wyrd_queue::ClientByteGuard>>` remains qualified in a public signature.
  - `sdks/wyrd-sdk-ts/native/src/lib.rs:89`, `:106`, `:313`, `:338`, `:349`, `:1074`, `:1083`, and `:1091` (representative, not exhaustive) — materially relocated/modified signatures continue to use `serde_json::Value`, `serde::Serialize`, `napi::Result`, `napi::Error`, and `std::fmt::Display` as qualified types.
  - `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs:258` and `crates/wyrd/wyrd-testing/tests/bifrost/oracle/capacity.rs:1390` — changed test signatures retain qualified `wyrd_client::Bifrost` (and, in the former, `tokio::task::JoinHandle`).
  - `crates/vala/vala-bifrost-redux/src/scribe/wal.rs:4373` — `use crate::namespaces::BifrostNamespace;` is declared inside the test function.
- Consequence: the R2 edits closed only the previously named examples, not the cumulative repository rule across the changed set.
- Required correction: inspect all new/materially changed Rust fields, signatures, and bounds in the candidate, add module-top imports (using non-conflicting aliases such as `NapiResult` where helpful), use the imported bare names, and move the WAL import to its test module's import block.

### STD-001 — VIOLATION: SDK Python adds an ineffective forbidden per-package Cargo profile

- Violated rule: `AGENTS.md` §4 states, “Do not add wildcard dependency versions or per-crate profile blocks.”
- Evidence: `sdks/wyrd-sdk-python/Cargo.toml:67` adds `[profile.release]` with LTO, codegen-unit, strip, and debug settings. Cargo reports that profiles in a non-root package are ignored and must be specified at the workspace root.
- Consequence: the manifest violates the repository rule and advertises release behavior that Cargo does not apply. It also creates unnecessary configuration churn for a task whose non-goals exclude new deployment or environment configuration.
- Required correction: delete the package-local profile block. Do not promote it to the workspace profile unless a separately approved workspace-wide requirement earns that behavior.

### STD-004 — VIOLATION: the recorded cumulative diff-hygiene proof is false

- Violated rule: `AGENTS.md` §12 requires the targeted checks to pass and the task's verification evidence to prove the reviewed candidate; the R2 evidence records `git diff --check` as passing.
- Evidence: the explicit immutable range command
  `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..f345cd8a4fdb5566ae6828ffb4f29d64f7f17599`
  reports:
  - `changes/active/surfaces-oracle-integration/review/task-002-f66a33769-review-01/findings-validation.md:183: new blank line at EOF.`
  - `changes/active/surfaces-oracle-integration/review/task-002-r1-4d9d74b34-review-01/verdict.md:88: new blank line at EOF.`
- Consequence: the candidate is not diff-clean, and the evidence likely checked only the empty working-tree diff rather than the cumulative review range.
- Required correction: remove the two extra EOF blank lines and re-run `git diff --check <base>..<new-candidate>` explicitly against the immutable cumulative range.

### STD-005 — VIOLATION: candidate commits contain prohibited AI co-author trailers

- Violated rule: `AGENTS.md` §13 says never add AI co-author trailers.
- Evidence: 22 commits in the cumulative range contain AI `Co-Authored-By` trailers: `f345cd8a4`, `2dc538f81`, `c0a12085c`, `c73bb414e`, `305762044`, `5d21c9dd5`, `afdc3b798`, `3e8b6efb3`, `5cfe7b6b9`, `7fc756f09`, `c0d4dc924`, `35ba64fe3`, `f4ec1161d`, `4d9d74b34`, `2d2e818cd`, `96b531c08`, `da0c13a56`, `06783f749`, `f61cf03ad`, `6c603d141`, `b335fd7df`, and `dfdc60699`. The commit author identity itself is correctly `Thorrester <sjforrester32@gmail.com>`.
- Consequence: the immutable candidate's history directly violates the repository contribution policy.
- Required correction: establish a new immutable candidate whose equivalent commits omit AI `Co-Authored-By` trailers, preserving the configured human author/committer identity and the reviewed source changes.

## Verification Notes

- Independently run and passing:
  - `mise run check:client-tier`
  - `mise run check:pyo3-scope`
  - `mise run check:py-wheel-no-testing`
  - `mise run check:single-into-response-impl`
  - `mise run check:proto-drift`
  - `bash scripts/checks/test-coverage.sh`
- Independently run and failing:
  - `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..f345cd8a4fdb5566ae6828ffb4f29d64f7f17599` (`STD-004`).
- The Cargo-backed checks consistently emitted the non-root-profile warning supporting `STD-001`.
- The recorded focused Rust tests, formatting/lints, Python lanes, TypeScript lanes, code generation, docs check, and other task evidence were reviewed but not all heavy lanes were re-run during this static standards pass.
- The immutable worktree remained clean and at candidate `f345cd8a4fdb5566ae6828ffb4f29d64f7f17599` after review commands.
- No `.codegraph/` index exists in the immutable worktree, so repository navigation used direct source and Git inspection as permitted by `AGENTS.md`.

## Overall Result

**FAIL**

The client-tier, PyO3, generated-contract, typed-error, tenancy, and most verification boundaries conform, but the candidate still violates mandatory rustdoc, Rust import/signature, Cargo profile, cumulative diff-hygiene, and Git metadata rules. These are concrete repository-standard failures and require a new immutable remediation candidate before TASK-002 can pass repository review.
