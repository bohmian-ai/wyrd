# TASK-001 r3 retry — repository-standards review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/verified-change-contract`
- Base: `5293546f33b3a5fd9de529098e23ea70d472c412`
- Cumulative candidate: `9d7b6266206f15136f306b66c06571066fc6bd13`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-001-verifier-contract-and-registration.md`
- Prior review/remediation: `review/TASK-001-r1/`, `review/TASK-001-r2/`, and `review/TASK-001-r2/TASK-001-R2-verifier-contract-closure.md`

`HEAD` was the candidate at both the start and end of this review. The only
worktree entries were untracked review directories created for the current and
abandoned review attempts; no reviewed source changed. The repository has no
`.codegraph/` index, so normal source inspection was used as instructed.

## Authority coverage

| Changed surface | Applicable authority | Source and evidence inspected | Result |
|---|---|---|---|
| Card vocabulary, Verifier envelope, Drift/Eval implementations, binding/Trigger/Operator contracts, canonical reference traversal | `AGENTS.md` §§2–6, 9, 12, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `references/doctrine/positioning-and-vocabulary.md`; `references/architecture/patterns.md`; `references/languages/{rust-core,errors}.md`; `references/domain/{evaluation,drift-monitoring}.md` | `wyrd-spec` Card, envelope, reference, graph, error, and schema owners; cumulative diff; strict-decoding and binding-validation tests | PASS |
| Loader, composite ordering, shared client state, CLI and card projections | `AGENTS.md` §§2–6, 9, 11–12, 15–16; `agent-rules.md`; `wyrd-design.md`; `references/languages/{implementation-execution,testing-workflows,errors}.md` | `wyrd-loader`, `wyrd-client`, `wyrd-cards`, CLI source, fixtures, and journeys | PASS |
| Server registration, reference resolution, authorization, no-write refusals, relationship/status derivation | `AGENTS.md` §§3–6, 9, 11–12, 15–16; `agent-rules.md`; `wyrd-security-posture.md`; `references/architecture/patterns.md`; `references/languages/{errors,testing-workflows}.md` | `components/cards/{resolve,service}.rs`, route tests, effective-spec owner, stable denial mappings, tenant-scoped registry paths | PASS |
| Persistent Card-kind migration, retired Vala tables/APIs, Bifrost table catalog and restart/Oracle proof | `AGENTS.md` §§2–6, 10–12, 15–16; `agent-rules.md`; `bifrost-design.md`; `wyrd-security-posture.md`; `references/domain/{evaluation,drift-monitoring,analytical-operations-reliability}.md` | append-only migrations, removed table/query owners, `wyrd-testing` cluster and Oracle journey changes, recorded focused and aggregate results | PASS |
| Rust/Python/TypeScript SDK projections and PyO3 boundary | `AGENTS.md` §§2–8, 11–12, 16; `references/languages/{rust-core,pyo3-boundaries,python-api-and-stubs,typescript-guide}.md` | Python native state projection, generated/public stubs and tests, generated TS error catalog, client-tier and PyO3 checks | PASS |
| UI projection and mocks | `AGENTS.md` §§2–3, 9, 11–12; `wyrd-design.md`; `wyrd-doctrine.mdx` | changed UI kind union, Service definition projection, mocks, and component test | PASS |
| OpenAPI, JSON Schema, stubs, error catalog, generated docs and source ownership | `AGENTS.md` §§2, 8–12, 16; `agent-rules.md`; reference-router authority hierarchy; generated-artifact rules | schema generators, `WyrdApiDoc`, recursive Verifier reference-closure test, generated artifacts, recorded `codegen:check` and `docs:check` | PASS |
| Rust structure, imports, async boundaries, documentation and panic contracts | `AGENTS.md` §§4–6, 15–16; `agent-rules.md`; `references/languages/rust-core.md` | cumulative Rust diff, round-1 closure, round-2 changed items, `EffectiveSpecs`, duplicate validation, OpenAPI owners, test helpers and test rustdoc | PASS |
| Manifests, lockfile, CI change routing, test-family/check retirement, verification execution | `AGENTS.md` §§1, 4, 11–12, 15–16; `agent-rules.md`; `references/languages/{spec-driven-development,implementation-execution,testing-workflows}.md` | root/crate manifests, `Cargo.lock`, `mise.toml`, `scripts/test-families.sh`, `.github/scripts/detect-changes.sh`, remediation evidence | **FAIL — RS3-2** |
| Active change packet, diff scope, commits and contributor identity | `AGENTS.md` §§12–14; `references/languages/{spec-driven-development,implementation-execution}.md`; Git identity rules | full `5293546f3..9d7b62662` commit and file range, task/review packets, commit bodies and author identities | **FAIL — RS3-1** |

## Applicable rule results

| Repository rule | Result | Exact evidence |
|---|---|---|
| One 15-kind Wyrd Card catalog; Drift/Eval only as closed Verifier implementations; no compatibility registration path | PASS | `CardKind::Verifier`, `Spec::Verifier`, strict rejection tests for retired kinds, synchronized design/doctrine/docs/schemas. Remaining legacy wording explicitly excluded by the remediation packet belongs to TASK-002 or untouched design assets, not a new candidate path. |
| Pure foundational `wyrd-spec`, typed contracts, stable derive-backed errors | PASS | No IO, async, SQL, PyO3, or server dependency entered `wyrd-spec`; duplicate/ref/activation errors remain catalog variants and server responses assert stable codes. |
| One canonical reference visitor and server-owned durable registration behavior | PASS | `Spec::binding_sites` and the reference visitor feed loader, composition, resolution and relationships; `EffectiveSpecs` owns dependency-backed server validation rather than threading dependencies through free workflows. |
| Struct-centered Rust, module-scoped imports, bare signature types, earned async | PASS | Prior import and owner findings remain closed. New round-2 behavior is pure synchronous validation or directly awaits Postgres/Oracle IO. No new trait, dependency, feature, or speculative helper was added. |
| SQL tenancy, transaction ownership, audit and security boundaries | PASS | Registration resolution stays on tenant-scoped owners; no raw pool enters production library signatures, no `TenantConn` callee commits, and direct pool queries added by remediation are test-only liveness probes. Permission denial retains the canonical audit/error path and no-write proof. |
| Migration and retired-resource discipline | PASS | The candidate appends Card-kind and table-drop migrations, deletes obsolete owners/targets/checks, and does not edit migration history or replace a live boundary check with a name ban. |
| Generated-artifact ownership and reference closure | PASS | `VerifierImplementation::schemas` delegates to `DriftSpec`'s existing schema dependency owner; `WyrdApiDoc` registers the three new roots; `openapi.yaml` defines `TriggerActivation`, `VerificationBinding`, `VerifierImplementation`, and the reachable Drift closure. The recursive focused test and reported `codegen:check` cover this candidate-owned graph. The defined-but-unreferenced `DriftSpec` component is harmless output of the pre-existing schema owner, not a second model. |
| Test placement and integrity | PASS | Pure duplicate/strict-shape cases are unit tests; authenticated decode/auth/RLS/no-write behavior is in the real Postgres-backed route test; CLI/Python public journeys remain present. Pointer-address assertions were deleted, replacement pools are exercised through real queries, and Oracle settlement preserves the exact six-field zero invariant under a bounded poll. No assertion, gate, or failure was waived. |
| Rustdoc, `# Errors`, `# Panics`, and cancellation/partial-progress documentation | PASS | Round-2 adds panic contracts to every ledger-named panicking helper/test, documents the new duplicate tests, OpenAPI walker/test, constants, and `VerifierImplementation` schema methods, and retains `# Errors` on the modified fallible Oracle journey owner. No placeholder documentation was found in the remediation diff. |
| First-class language and generated projection rules | PASS | Python behavior stays a thin SDK-owned projection with public imports/stubs/tests; TypeScript receives only the generated error-code change required by retired catalog entries; no SDK duplicates registry or transport behavior. Recorded round-1 Python/TypeScript lanes plus round-2 no-change evidence cover these projections. |
| Relevant format, lint, owner, Bifrost, codegen, docs, and boundary gates | PASS for outcomes | The record reports green `fmt`, `lints`, `test:shared`, `test:cards:integration`, `test:wyrd`, all nine `test:bifrost` lanes, `codegen:check`, `docs:check`, `check:client-tier`, `check:pyo3-scope`, and `git diff --check`; repeated restart and Oracle runs used `--retries 0`. Command-form compliance separately fails under RS3-2. |
| Contributor identity and no AI attribution trailers | **FAIL** | RS3-1. All commit author identities are correct, but in-range commit `5f14f3c32` contains a prohibited AI co-author trailer and an unrelated process-rule edit. |
| Exact focused verification through the repository-pinned toolchain | **FAIL** | RS3-2. Two named remediation proofs and the Postgres route proof are recorded with raw `cargo nextest` rather than the mandatory `mise exec -- cargo nextest` form. |

## Prior standards-finding closure

| Prior finding | Closure |
|---|---|
| Round-1 SR-1 through SR-8 | Remain closed by the cumulative implementation and round-1 remediation: stale targets/checks and old public paths are deleted; the canonical owner, imports, stable errors, vocabulary, and production-item rustdoc remain corrected. |
| Round-2 SR2-1 allocator-address flake | CLOSED. `PostgresPoolIdentity` and both pointer comparisons are deleted. The two restart tests exercise replacement application/Vala pools and preserve their lifecycle assertions; three no-retry runs and the aggregate are recorded green. |
| Round-2 SR2-2 missing panic contracts | CLOSED. The remediation documents all ledger-named test/helper panic conditions and the new round-2 tests and owners. |

## Material findings

### RS3-1 — VIOLATION — the cumulative task candidate contains prohibited AI attribution and unrelated process work

- **Violated rules:** `AGENTS.md` §13 says never add AI co-author trailers; the task-review acceptance boundary and `implementation-execution.md` final-diff audit require every changed file and commit to belong to the task or its verification need.
- **Exact location:** commit `5f14f3c325cc8c45081fe9ad0543e7794b4a4e0f`; `AGENTS.md:515-527` in the cumulative base-to-candidate diff.
- **Evidence:** the commit body contains `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>` and a `Claude-Session` trailer. Its only source change adds the repository-wide “pre-existing is not a waiver” process rule. The handoff itself identifies this as a process fix rather than TASK-001 work, but the required cumulative subject `5293546f3..9d7b62662` includes it.
- **Consequence:** the candidate violates the repository's contributor-identity policy and is not an isolated TASK-001 implementation range. A later commit cannot remove a trailer from the history being reviewed, and treating the unrelated rule as review authority does not make it task implementation.
- **Testable correction:** present a new immutable cumulative TASK-001 candidate whose original-base range excludes the unrelated `5f14f3c32` source commit and contains no AI co-author/session trailer, while preserving the accepted TASK-001 and remediation content. Verify with `git diff --quiet 5293546f3..<candidate> -- AGENTS.md` and a full-range commit-body scan returning no AI attribution trailer. Branch/history mechanics remain caller-owned; do not alter Git identity configuration.

### RS3-2 — VIOLATION — three named focused proofs were not recorded through `mise exec --`

- **Violated rules:** `AGENTS.md` §11, `architecture/references/languages/spec-driven-development.md` “Test command precision,” and `implementation-execution.md` “Focused verification” require every specifically named Rust test to be run and recorded through an exact `mise exec -- cargo nextest run --locked ... -E 'test(=...)'` command, including the repository-managed setup wrapper when needed. The R2 remediation repeats that requirement at lines 176–181.
- **Exact location:** `changes/active/verified-change-contract/review/TASK-001-r2/TASK-001-R2-verifier-contract-closure.md:219,222-223`.
- **Evidence:** the route proof at line 219 invokes raw `cargo nextest` inside the Postgres wrapper after a `mise run` migration; the two restart proofs at line 222 and the Oracle proof at line 223 likewise record raw `cargo nextest`. The composition and OpenAPI focused proofs correctly use `mise exec --`, demonstrating the expected form. Broader `mise run` lanes establish substantial behavior but do not satisfy the explicit named-test command requirement.
- **Consequence:** the evidence does not prove those exact repeated commands used the repository-pinned Cargo/nextest toolchain, and the committed execution record claims PASS despite omitting a mandatory proof form.
- **Testable correction:** rerun only the three named focused proof groups through the same environment/setup wrappers with `mise exec -- cargo nextest run --locked`, retaining their exact package/target/features/test expressions and `--retries 0` repetition where required; append the exact commands and results to the remediation evidence. The already-green broader lanes need not be rerun unless the candidate source changes.

## Verification limits

- This was a static repository-standards audit. I did not start overlapping Cargo work in the shared checkout. I relied on the committed record and supplied handoff for expensive runtime results and independently ran `git diff --check`.
- The packet's implementation-range sentence still says `6f59da966..b612e263e` on top of `5f14f3c32`, while the immutable candidate also includes `90f28c8f2` and the rustdoc-only `9d7b62662`. The supplied handoff names `9d7b62662` and reports the final lanes green; source inspection found no behavior change after `b612e263e`. This bookkeeping mismatch is not a separate finding from RS3-1/RS3-2.
- Python and TypeScript were unchanged by round-2 remediation. Their cumulative TASK-001 projections retain the round-1 recorded unit/type/journey evidence; round 2 appropriately did not rebuild them.

## Overall result

**FAIL.** The seven implementation findings from round 2 are closed, including
the flaky restart proof, asynchronous Oracle settlement, Rust documentation,
duplicate identity handling, and OpenAPI reference closure. Repository
acceptance still fails because the immutable cumulative candidate includes a
prohibited AI co-author trailer plus unrelated process work (RS3-1), and three
specifically named proofs were not recorded through the mandatory
repository-pinned `mise exec --` form (RS3-2).
