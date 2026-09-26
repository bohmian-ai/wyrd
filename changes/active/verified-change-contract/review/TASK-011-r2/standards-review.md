# TASK-011 r2 repository standards review

## Subject and authority coverage

Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`; base `338f33235f81c30dfe3a570dc26934fe7bb77048`; candidate `3c6fc1880692cc29c4c1ff7fc5fcb72a850bb9d6`. I reviewed the cumulative diff and recorded verification, independently of the other r2 reports. `.codegraph/` is absent. The candidate was HEAD with a clean tree at inspection.

| Changed surface | Governing authority inspected |
|---|---|
| Approved spec, TASK-011, remediation packet and review history | `AGENTS.md` §§11, 14–16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md`; approved revision 38 |
| Vala PSI/SPC algorithms, Arrow input and evidence | `AGENTS.md` §§3–6, 10–11, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/domain/vala-architecture.md`, `domain/drift-monitoring.md`, `domain/arrow-analytical-interop.md`, `languages/rust-core.md`, `languages/testing-workflows.md` |
| `wyrd-spec` Drift Card contract and generated JSON schemas | `AGENTS.md` §§2–4, 9, 11, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`, `languages/rust-core.md`, `languages/testing-workflows.md` |
| Server Drift Oracle query, test server, Postgres fixtures | `AGENTS.md` §§2–6, 9–11, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/bifrost-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/domain/olap-serving.md`, `domain/drift-monitoring.md`, `languages/rust-core.md`, `languages/testing-workflows.md` |
| Rust, Python and TypeScript journeys and N-API/PyO3 test projections | `AGENTS.md` §§7–8, 11, 16; `architecture/agent-rules.md`; `architecture/references/languages/python-api-and-stubs.md`, `languages/typescript-guide.md`, `languages/testing-workflows.md`; `architecture/wyrd-doctrine.mdx` |
| Architecture Drift docs and generated `docs/public/llms-full.txt` | `AGENTS.md` §§1, 2, 11, 14; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; approved specification |

## Rule results

| Rule | Result | Source and verification evidence |
|---|---|---|
| Wyrd-owned server/contract/Vala/client boundaries (`AGENTS.md` §§2–3, 9; `architecture/patterns.md`) | PASS | Drift math remains in `vala-drift`; `SpcProfile` contract lives in `wyrd-spec`; server execution remains in `wyrd-server/src/verification/drift.rs`; SDK edits are journeys or test bindings. No new production dependency or Cargo feature appears in the diff. |
| Tenant-safe audited Bifrost query path (`AGENTS.md` §§2, 9; `bifrost-design.md`) | PASS for repository standards | `DriftEngine` still mints the scoped SYSTEM reader and uses the query service; revised PSI/SPC SQL is a single statement through that path. Domain reviewers assess query semantics and data correctness. |
| Pure contract crate and generated artifacts (`AGENTS.md` §§2, 8–9; `agent-rules.md`) | PASS for code placement | `wyrd-spec` edits are declarative `SpcProfile` validation; schema snapshots match the source-shaped change, and recorded `mise run codegen:check` exited 0. No PyO3/IO was added to `wyrd-spec`. |
| Python and TypeScript runtime ownership (`AGENTS.md` §§7–8, 11; language references) | PASS | Python and TypeScript journeys exercise their own runtime; the new native methods delegate to `wyrd-testing::VerificationFixture`, and TypeScript declaration changes are paired with recorded typecheck and codegen checks. |
| Tier-1 journey coverage (`AGENTS.md` §11; `testing-workflows.md`) | PASS | Rust, Python and TypeScript Drift journeys were changed and their owning `mise` journey lanes are recorded passing. The new language test controls only support those journeys. |
| Documentation and exact focused test commands (`AGENTS.md` §§11, 16) | PASS in part | Task evidence records exact commands and successful results for 25 named Vala, 8 named server, 2 named Rust journey, Python pair and TypeScript focused test; `fmt`, `lints`, `py:format`, `py:lints`, `ts:typecheck`, `codegen:check`, `docs:check`, served OpenAPI gate, and `git diff --check` are recorded passing. The changed `wyrd-spec` tests have no recorded execution; see S3. |
| Rustdoc for new/materially modified items, including test helpers; `# Errors` on every fallible method (`AGENTS.md` §16; `agent-rules.md`) | FAIL | `ColumnRef::collect_f64` and `collect_string` have prose errors but no `# Errors` heading; newly added `psi_score::feature` has no rustdoc at all. See S1. |
| Bare imported types in signatures (`architecture/agent-rules.md`) | FAIL | New `fold_spc` and test-fixture API signatures contain fully qualified paths; see S2. |
| Owning crate test execution (`AGENTS.md` §11; `agent-rules.md`; `testing-workflows.md`) | FAIL | `wyrd-spec/src/card/mod.rs` changes validation tests, while TASK-011's evidence claims the `wyrd-spec` suite but lists no `test:wyrd` or exact `wyrd-spec` test command. See S3. |
| SQL connection ownership and audit mechanics (`architecture/agent-rules.md`) | PASS | New `VerificationFixture::retire_fitted_format` obtains its own tenant connection, executes under its transaction and commits as its owner; it does not accept a caller-owned `&mut TenantConn`. Production Drift reads remain in the existing query/audit service. |

## Material findings

### S1 — Required rustdoc is incomplete

- **Violated rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` require rustdoc for every new or materially modified Rust item, including test helpers, and a `# Errors` section for every fallible function.
- **Locations and evidence:** `crates/vala/vala-drift/src/feature.rs:44–65` adds fallible `ColumnRef::collect_f64` and modifies fallible `collect_string`, documenting errors only as prose without `# Errors`; `crates/vala/vala-drift/src/psi/mod.rs:787–789` adds `psi_score::feature` with no rustdoc or panic note for its `expect`.
- **Consequence:** The implementation misses a hard repository documentation gate even though it compiles; maintainers lack the required explicit failure contract at the changed Arrow conversion boundary.
- **Testable correction:** Add meaningful `# Errors` sections to those two methods and intent/panic rustdoc to the test helper; audit other newly added or materially modified Rust items in the same diff for the same rule. `mise run fmt` and `mise run lints` remain green.

### S2 — Fully qualified types in newly added signatures

- **Violated rule:** `architecture/agent-rules.md` requires types to be imported with module-level `use` statements and written bare in struct fields, function parameters, return types, trait bounds and `where` clauses.
- **Locations and evidence:** `crates/wyrd/wyrd-server/src/verification/drift.rs:578–582` uses `&wyrd_spec::ids::FeatureName` although `FeatureName` is already imported; `crates/wyrd/wyrd-testing/src/server.rs:3474–3479` returns `crate::verification::VerificationFixture` and `crate::verification::VerificationFixtureError`; `sdks/wyrd-sdk-ts/native-testing/src/lib.rs:664` uses `impl std::fmt::Display` and returns `napi::Error` in a new helper.
- **Consequence:** The newly introduced API signatures violate the mandatory module dependency style and hide their owners in a large changed surface.
- **Testable correction:** Use existing or top-level `use` imports and bare type names in these signatures, then run `mise run fmt` and `mise run lints`. No behavior change is needed.

### S3 — Changed `wyrd-spec` tests lack execution evidence

- **Violated rule:** `AGENTS.md` §11 and `architecture/agent-rules.md` require the owning crate family lane or exact focused commands for changed tests, and named test claims must be reproducible. The task's acceptance evidence explicitly claims a `wyrd-spec` suite.
- **Locations and evidence:** `crates/wyrd-spec/src/card/mod.rs:2308–2350` changes SPC validation and unknown-field tests. `changes/active/verified-change-contract/tasks/TASK-011-conventional-psi-spc.md:113,124–170` claims a `wyrd-spec` suite but records only Vala, Bifrost, OpenAPI and codegen lanes plus focused Vala/server/journey tests. `mise.toml:74–82` provides `test:wyrd` as the owning family lane; no result for it or exact `wyrd-spec` tests is recorded.
- **Consequence:** The changed contract validation tests are compile-covered by lint but have no recorded execution; the evidence does not substantiate its own contract-test claim.
- **Testable correction:** Run `mise run test:wyrd` or the exact affected `wyrd-spec` tests through `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=...)'`, and record exact commands/results in TASK-011. Repair any failure without weakening a gate.

## Overall result

**FAIL** — S1, S2 and S3 are bounded repository-standard defects. This report does not decide task acceptance or the statistical/data-domain findings.
