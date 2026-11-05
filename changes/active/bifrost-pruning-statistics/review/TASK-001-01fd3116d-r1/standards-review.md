# Repository standards review

Result: **FAIL**.

## Immutable subject and coverage

- Wyrd: `d7de12a74c4bdbf93e727cc2383147e34298f689...01fd3116d2501d89317073a2abb547818a23b4a8`.
- DataFusion: `19796a13ed46877cf998013886947b28a6f774b3...9c724e501b613ea1e3de6a7e8d9fb700eed65b25` in `/home/thorrester/Documents/GitHub/datafusion-pruning-statistics`.
- Both HEADs still match those candidates. Reviewed the complete committed ranges, not only the restored historical fork patch. No prior implementation acceptance was carried forward.
- Neither repository has `.codegraph/`; ordinary Git/source navigation applies. Wyrd has no nested `AGENTS.md` on the changed paths. The fork has its own root `AGENTS.md`; Wyrd rustdoc/import policy is not silently imposed on independent fork source.
- Read the recorded scope, approved revision 3, original task and its available verification section, both root contributor authorities, Wyrd `agent_docs/index.md`, `rust.md`, `maintainer-style.md`, `changes/spec-driven-development.md`, `TESTING.md`, command definitions and nextest journey configuration. No other discovery report or implementation completion message was consumed.

| Changed surface | Applicable authority | Coverage / result |
|---|---|---|
| Wyrd `Cargo.toml`, `Cargo.lock` | Root dependency/tier/feature/generated-artifact rules; spec INV-004; task pin-consistency proof | All eleven patch entries point at the same complete fork SHA. Lockfile changes only move resolved DataFusion sources to that SHA; no new dependency, feature, public contract or path patch. PASS on inspected source; regeneration is recorded historical evidence. |
| `oracle/codec.rs::ORACLE_DATAFUSION_REVISION` | Root Rust documentation and dependency rules; Bifrost immutable planning/peer compatibility | Existing meaningful rustdoc preserved. Constant agrees with manifest and lockfile. No codec version or payload change. PASS. |
| `oracle/exec.rs::set_time_bounds` | Root Rust rules; Bifrost source authority, tenant proof, field projection | Only visibility expands to `pub(crate)` for the restored owner-level test. Existing rustdoc explains interval precision, conservative fallback and workflow. No changed pool, SQL, file-proof, schema-projection or IO logic. PASS for changed behavior. |
| `scan_sizing.rs::BatchPruning::keep`, restored test | Root testing and changed-item rustdoc; `agent_docs/rust.md`; Bifrost scan-sizing authority at 1113–1135 | Removes the Wyrd zero-container workaround and retains the DataFusion owner. New test has scenario, invariant and panic docs. The materially changed method's zero-container documentation is stale; new test also introduces prohibited local imports. FAIL: REPO-002/003. |
| `oracle/live.rs` new empty-selection test | Root unit-test, Rust documentation/import rules; Bifrost immutable planning/live-source authority | Inline test uses actual route/pushdown/resolution owners and existing telemetry. Meaningful rustdoc and `# Panics` present. New function-scoped imports violate the explicit import rule. FAIL: REPO-003. |
| `oracle/memtable_source.rs` new partial-statistics test | Root unit-test/rustdoc rules; Rust and maintainer guides; Bifrost scan-sizing and source ownership | Inline test uses recorded Scribe statistics and the source's existing pushdown seam; compares reference rows and exclusion/full-match/sizing state. Rustdoc explains those invariants and panics. No new helper item or production signature. PASS. |
| `wyrd-testing/tests/bifrost/oracle/published.rs` new journey and helper | Root journey/Rust/rustdoc rules; TESTING.md owner layout; existing Oracle lane and `profile.journey`; Bifrost public SDK path and tenant source authority | Real `WyrdTestCluster`, SDK writers/readers, normal tenant fixtures and server paths; existing integration binary, no extra target or harness. Test and helper have meaningful rustdoc, `# Panics`/`# Errors`. Four newly declared constants lack rustdoc; helper introduces local imports. FAIL: REPO-002/003. |
| Change packet `spec.md`, original task, `investigation.md` | Root changes authority; `changes/spec-driven-development.md`; architecture authority map | Kept under tracked active packet, approval revision recorded, evidence separates incident reproduction from historical attribution. No architecture/public contract replacement in the source diff. PASS for standards scope; task acceptance belongs to behavior review. |
| Fork `common/src/pruning.rs` production methods and tests/helpers | Fork `AGENTS.md`; contributor guide “Before Submitting a PR”, review/testing guidance; Wyrd spec external-dependency boundary | Changes remain in existing shared statistics owner. All new production methods and the `exact_bounds`/`unknown_bounds` helpers have rationale docs. `single_column`, `bounds_of` and test functions have no rustdoc, but fork authority does not require Wyrd's universal changed-item rustdoc gate. No new package or manifest. Source placement/doc policy PASS; mandatory delivery verification FAIL: REPO-001. |
| Fork `pruning/src/pruning_predicate.rs` imports, warning capture, test | Same independent fork authorities; applicable benchmark guidance | Test/helper/logger additions stay in existing unit-test module, no new dependency or production API. Warning capture and `warnings_of` document purpose. No Wyrd import/rustdoc rule imposed on fork test items. Mandatory delivery verification FAIL: REPO-001. |

## Rule audit and available verification

| Rule / mandatory check | Evidence | Result |
|---|---|---|
| Generic statistics behavior belongs in DataFusion; server/client dependency boundaries stay intact | Cumulative fork diff changes the existing `PrunableStatistics` owner. Wyrd production diff removes a guard, widens one existing helper and updates the compatibility constant; other changes are tests/pins/packet. No SDK, `wyrd-spec`, PyO3, SQL, handler or public schema surface changes. | PASS |
| Tenant SQL must use `TenantConn`; privileged work requires `OperatorPool`; file tenant proof must precede decoding | No production signatures/fields introduce `PgPool` or any SQL capability. Existing per-file proof code is untouched. Journey obtains separately authenticated tenant readers/writers through existing owners; retained footer refusal test is included in review-time seven-test proof. This conclusion is based on source, not only the tenant-isolation script. | PASS |
| No IO, runtime, background process, migration or durable rewrite introduced by conversion fix | Shared helper is synchronous scalar/array work. Wyrd production ownership and IO paths are unchanged. | PASS |
| Every new/materially modified Wyrd Rust item has meaningful, accurate rustdoc | Audited all changed Wyrd functions/tests/helpers/constants, including local constant items. Gaps below. No new structs, fields, enum variants or traits. | FAIL: REPO-002 |
| Imports at module top; no function-scoped imports except stated exceptions | Newly introduced imports in three functions are neither `use super::*` in a test module nor a rare generic-function `use Trait as _` exception. | FAIL: REPO-003 |
| Tests live with the owning seam; journeys use real SDK/server and managed Postgres | Unit tests are inline. New journey uses existing `oracle` target and matches neighboring ignored journey registration; owning task explicitly selects it with `--run-ignored=all` and managed Postgres. The ignore attribute is not a newly hidden test. | PASS |
| Exact named tests and minimum feature union | Task records exact D1–D4/W1–W3/J1/J2 selectors and counts, RED/GREEN evidence, and appropriate managed journey setup. Review-time `verification.md` independently reruns the seven Wyrd focused tests (all pass), common pruning module (13 pass), and D3 exact test (one passes). | PASS within available proof |
| Wyrd format, lint, dependency, tenancy and storage checks | Task evidence says `fmt`, `lints`, `check:deps`, `check:tenant-isolation`, `check:storage:drift` pass. Definitions in `mise.toml` retain workspace all-target/all-feature Clippy and separate server-feature lint. No gate weakened in cumulative source. | PASS on recorded historical evidence; not rerun by this specialist |
| Fork format, required lint, contributor checks and extended tests | Fork root makes workspace format/lint and extended workspace tests mandatory. Task evidence only claims two-package lint, selected suites, changed-file format; explicitly records unresolved workspace formatting and omitted extended suite. Review-time focused tests do not fill these gaps. | FAIL: REPO-001 |
| Fork relevant local benchmarks, if any | Inspected benchmark guidance and direct pruning benches. Existing primitive/string IN-list benches exercise their own `PruningStatistics` fixtures, not changed `PrunableStatistics` conversion. No directly applicable conversion benchmark was established. Task provides bounded known/mixed/unknown array observations. Do not require a new benchmark harness or unrelated benchmark suite. | PASS for applicability assessment; no benchmark execution claimed |
| Generated artifacts and public surfaces | Only Cargo-regenerated lockfile claimed; no checked-in generated schemas, bindings, stubs or API changes. No codegen task required by this write set. | PASS |
| Diff whitespace | Reviewer ran both complete-range `git diff --check` commands through mise; both exited zero. | PASS |

## Proposed source-local findings

### REPO-001 — Mandatory independent fork delivery checks remain unsatisfied

- Classification: **VIOLATION**.
- Authority: fork `AGENTS.md:12–38` requires following contributor pre-submission checks and, at minimum, clean `cargo fmt --all` plus `cargo clippy --all-targets --all-features -- -D warnings` before committing. Lines 46–52 require the extended workspace test command when this is a code change. Wyrd original task “Owner checks and non-TDD proof” expressly preserves these independent obligations and says its focused list does not waive them.
- Location: dependency candidate `9c724e501`; Wyrd task `TASK-001-dependency-owned-pruning.md`, “Owner checks” implementation evidence (approximately lines 465–469), consumed by `Cargo.toml:276–286`.
- Evidence: evidence says lint was run for `datafusion-common` and `datafusion-pruning`, not the required workspace; `schema_rewriter.rs` has unresolved workspace fmt drift; the full workspace extended test suite was explicitly not run. Selected common/pruning/parquet runs and Wyrd Clippy cannot establish those independent checks. Review-time focused reruns also expressly disclaim that coverage.
- Consequence/materiality: the candidate is offered as a completed, committed external dependency despite explicit mandatory delivery gates remaining red or absent. This is a gate violation, not a conjectured code defect or an optional desire for broader testing. A failure's origin at the base does not waive an actual mandatory check.
- Smallest testable correction: satisfy and record the fork's actual required format, lint/contributor and extended-test commands on the delivered dependency candidate. Fix actual gate failures at their owner without suppressions or weakened checks; then update Wyrd's compatible pin/fingerprint if the delivered SHA changes. Do not add a benchmark harness or substitute Wyrd checks. The correction is verification and any necessary gate repair; it does not authorize unrelated feature work.

### REPO-002 — Changed Wyrd items miss the mandatory rustdoc contract

- Classification: **VIOLATION**.
- Authority: Wyrd root `AGENTS.md` says every new/changed Rust item has rustdoc and missing blocks merge. `agent_docs/rust.md`, “Rustdoc”, extends this explicitly to private/test helpers and requires useful workflow/invariant documentation. It is an explicit gate, not a style preference.
- Locations: `scan_sizing.rs:589` and `published.rs:1773–1777`.
- Evidence: removal of `BatchPruning::keep`'s zero-batch return means zero containers now pass through `PruningPredicate::prune`, which builds/evaluates its statistics predicate; the retained doc still promises “without evaluating the predicate.” Four new Rust constant items (`SPANS`, `OWNER`, `NEIGHBOUR`, `HOUR_NANOS`) in `prove_observe_predicates` have no rustdoc. The ordinary `//` comment before `OWNER` is not rustdoc. Other changed Wyrd tests/helpers and the revision constant were inspected and have meaningful docs, including applicable Errors/Panics sections.
- Consequence/materiality: maintainers are given an obsolete empty-input contract on the production owner, and newly added items fail the explicit changed-item documentation gate. No additional runtime failure is alleged.
- Smallest testable correction: update `keep`'s description to reflect dependency-owned empty handling, and document the four constants' fixture roles or eliminate the unnecessary constant items while preserving fixture behavior. No new helper abstraction is needed. Static item/doc inspection and the existing focused regression suffice; batch this with substantive remediation rather than create a style-only round.

### REPO-003 — New Wyrd tests/helpers violate the explicit module-import rule

- Classification: **VIOLATION**.
- Authority: `agent_docs/rust.md`, “Imports and signatures”: “All `use` statements sit at the top of the module ... No function- or `impl`-scoped `use`.” Its two exceptions do not cover these concrete test/helper functions.
- Locations: `scan_sizing.rs:855–857`, `oracle/live.rs:1611–1614`, `published.rs:1771`.
- Evidence: each listed new function declares concrete imports inside its body. None is the permitted `use super::*` directly inside a `#[cfg(test)] mod tests`, nor a generic-function trait-only exception. Existing top-level test-module imports and journey imports provide the correct existing owner. Old occurrences elsewhere are not additional findings.
- Consequence/materiality: direct breach of an explicit repository implementation constraint. This finding does not claim runtime harm or elevate a preferred import style into a new rule.
- Smallest testable correction: move the newly introduced imports to their owning module import groups and merge with existing imports. Preserve scope through the existing test module; no new module, wrapper, allow, or production import is required. Formatting/lint plus source inspection proves closure.

## Limits

This specialist did not run Cargo builds/tests/lints; the orchestrator owns sequential Cargo verification. Historical RED/GREEN, journey and broad Wyrd-check claims are available task evidence, not independently rerun here. The independent fork mandatory-check gap remains actionable despite passing focused reruns.

Skipped unrelated language/API/deployment gates because the cumulative diff does not change those surfaces. No claim of full fork regression safety is made while its required checks remain incomplete.
