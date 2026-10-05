# TASK-002-r5 repository standards review

Overall: **PASS**.

Subject: complete cumulative
`0569b79702218600c4f9790f45cc03100d5c6f1c` →
`2d669917c03699876b3c8926f0de5ac88c578c01`, including the original
`TASK-002-cleanup` and the R2, R3, and R4 remediation tasks. The candidate was
`HEAD` when this review began and remained unchanged through report creation.
The worktree contained only the concurrently created untracked R5 review
directory. The repository has no `.codegraph/` directory, so ordinary source
navigation was used.

This is an independent repository-standards audit. It does not decide task
acceptance or perform the structured Ponytail validation. I inspected the
complete cumulative diff, current owning modules and callers, manifests,
language projections, tests, and the final R4 correction. Recorded verification
is treated as supplied evidence; this reviewer did not rerun build, test,
generation, or lint commands.

## Authority and changed-surface coverage

The reference router is `architecture/references/README.md`. The complete
applicable authority set was `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`,
`architecture/wyrd-security-posture.md`, and the routed references named below.
`architecture/bifrost-design.md`, analytical-domain references, and operations
runbooks are not applicable because the candidate changes no Bifrost,
analytical-storage, deployment, backup, recovery, or incident behavior.

| Changed surface and consumers inspected | Governing authority and routed references | Result |
|---|---|---|
| `wyrd-spec` `CardRef` projection and `WorkflowInvalidCardRef`; `wyrd-semver::VersionBlock` and its Card/selector consumers | AGENTS §§2–4, 9, 12, 16; design Card/reference/version/error contracts; doctrine; `doctrine/positioning-and-vocabulary.md`; `languages/{rust-core,errors}.md` | **PASS.** `CardRef` retains one exact version field. `VersionBlock` is still a pure foundational domain value and now enforces its existing `parse` invariant during ordinary Serde construction at `crates/shared/wyrd-semver/src/block.rs:153–166`; schema and serialization shape remain unchanged. |
| `wyrd-loader`; shared `Workflow`, `WorkflowBodies`, `CardGraphHydrator`, and `GraphTraversal`; existing Service graph consumers | AGENTS §§3–6, 9, 11–12, 16; `doctrine/architecture-constraints.md`; `architecture/patterns.md`; `languages/{rust-core,maintainer-style,testing-workflows}.md` | **PASS.** Existing loader, graph owner, and Skald owner are extended rather than duplicated. Stateful workflows remain on cohesive owners, pure validation remains synchronous, and blocking filesystem work uses the installed Tokio blocking pool. |
| Skald Workflow body discovery, Agent/Prompt hydration, declaration-only validation, and execution callers | AGENTS §§3–6, 9–10, 16; design Skald boundary; doctrine/constraints/patterns; Rust/error/maintainer references | **PASS.** Skald remains independent of registry, SQL, tenant, and server owners. Registration does not bind providers, tools, secrets, or execution state. |
| Server `EffectiveSpecs`, `RegistrationWriter`, registration services, and SQL relationship UID fence | AGENTS §§3–6, 9, 11–12, 15–16; agent-rules SQL/RLS/audit rules; design registry lifecycle; security posture; architecture patterns; testing/errors | **PASS.** Durable validation and writes remain server-owned. Borrowed `TenantConn`, caller-owned transaction, existing authorization/audit flow, exact Active UID recheck, authored-submission persistence, and tenant isolation are preserved. No raw pool or manual tenant-filter path entered production code. |
| Shared client and Rust SDK exports, Cargo manifests, lockfile, and workspace feature union | AGENTS §§2–6, 11–12; client model and dependency-cost rules; constraints/patterns; Rust/testing references | **PASS.** `wyrd-client` remains the sole shared client implementation and the Rust SDK remains a thin projection. Client-tier crates gain no SQL, cloud, DataFusion, or server dependency. |
| Python Workflow loading, Cards selector projection, package exports, generated stubs, and Python journeys | AGENTS §§2–4, 7–12, 16; `languages/{pyo3-boundaries,python-api-and-stubs,errors,rust-core,testing-workflows,maintainer-style}.md` | **PASS.** PyO3 stays in the Python SDK, uses the shared runtime/client owners, preserves public imports and stable errors, and is exercised in the Python runtime. No post-R3 Python source or generated surface changed. |
| TypeScript/N-API Workflow and Cards projection, selector parsing, declarations, and Node journeys | AGENTS §§2–6, 9, 11–12, 16; `languages/{typescript-guide,errors,rust-core,testing-workflows,maintainer-style}.md` | **PASS.** The binding remains a thin projection over `wyrd-client`. Raw JSON strings are narrowed once at the foreign-runtime boundary with existing `CardUid`, `SpaceName`, `CardName`, and `VersionBlock` constructors at `sdks/wyrd-sdk-ts/native/src/workflow.rs:53–106`; malformed fields retain the derive-backed Workflow error code and precise field details. The direct workspace `wyrd-semver` dependency is the narrow owner of the exact-version invariant, not a duplicate parser or durable implementation. |
| Rust/Python/TypeScript journeys, server/Postgres seam tests, SQL tests, and shared fixtures | AGENTS §§11–12; agent-rules runtime and test-placement rules; `languages/testing-workflows.md` | **PASS.** External tests span real server/Postgres or an owning foreign runtime and therefore earn their placement. All three first-class SDKs have client→server→client journeys. The R4 change adds an in-owner unit invariant and extends the existing Node journey; it adds no harness, sleep, retry, allowance, or weakened/ignored assertion. |
| Example bundle, docs, READMEs, architecture/doctrine/reference edits, skill mirrors, active spec/tasks, and prior review records | AGENTS §§1–2, 8–12, 14–16; design/doctrine; `languages/{spec-driven-development,implementation-execution,maintainer-style}.md` | **PASS.** Public examples and documentation use Wyrd vocabulary and canonical reference forms. Planning/review material remains under the established change packet. No legacy compatibility surface or permanent source reference to a task/agent was introduced. |

## Applicable-rule results

| Rule | Candidate evidence | Result |
|---|---|---|
| Durable registration is server-owned; shared client composition is in `wyrd-client`; Skald owns runtime behavior; `wyrd-spec` remains foundational | `wyrd-client/src/{workflow,cards/hydrate/*}.rs`; `skald-workflow/src/bodies.rs`; `wyrd-server/src/components/cards/{resolve,service}.rs`; manifests | **PASS** |
| New/materially changed Rust uses cohesive owners, domain types, top-level imports, narrow async, typed errors, and substantive rustdoc | `GraphTraversal`, `WorkflowBodies`, `CardBodyResolver`, `EffectiveSpecs`, `RegistrationWriter`; `VersionBlock` deserialization at `block.rs:153–166`; N-API boundary parser at `workflow.rs:71–106` | **PASS** |
| Invalid exact-version state cannot be constructed through Serde and the wire/schema shape is preserved | `VersionBlock` delegates deserialization to its existing `parse`; `block::tests::serde_preserves_the_exact_version_invariant`; no generated schema diff in R4 | **PASS** |
| Public errors use the derive-backed catalog and project consistently across Rust, HTTP, Python, and TypeScript | `wyrd-spec/src/error.rs`; shared/Python/N-API selector producers; generated TypeScript error-code union | **PASS** |
| Foreign-runtime bindings validate at the edge and delegate transport/durable behavior to `wyrd-client` | Python SDK wrappers; `sdks/wyrd-sdk-ts/native/src/workflow.rs:79–106`; `native/src/cards.rs` | **PASS** |
| Tenant/RLS, audit, caller-owned transaction, and exact relationship identity remain intact | Server resolution/write flow; SQL relationship recheck; Postgres registration and race tests | **PASS** |
| Every shipped SDK loading surface has an owning-runtime user journey and focused negative proof | Rust `workflow_loading.rs`; Python `test_cards_crud.py`; TypeScript `workflow-loading.test.ts:186–222`; server/SQL seam tests | **PASS** |
| Generated contracts, language typing, workspace dependencies, and boundary rules have final-source evidence | R4 implementation record at `TASK-002-R4-close-workflow-selector-validation.md:191–199` records `codegen:check`, `ts:napi:check`, `ts:typecheck`, `check:client-tier`, `check:sdk-client-tier`, `fmt`, and `lints` PASS after the correction | **PASS** (recorded evidence; not independently rerun here) |
| Human standing direction: unsupported bespoke mechanisms are DRIFT and may not be required as remediation | The cumulative candidate adds no new check, scanner, allowlist, feature flag, setting, compatibility option, parser dialect, validator framework, or enforcement file. Serde delegation, domain constructors, workspace dependencies, generated declarations, Tokio `spawn_blocking`, Postgres transactions/locks, and ordinary owning-language tests are established repository or widely used ecosystem mechanisms. | **PASS** |

## Material repository-rule findings

None.

The R4 correction closes the only repository-standard concern visible in its
changed surfaces without creating a second validator or downstream guard:
Serde construction is repaired at the invariant-owning newtype, and the N-API
edge performs field-attributable conversion using existing domain constructors.
The existing server parser remains defense in depth. No independently supported
repository-rule violation remains.

## Verification notes

- `git diff --check 0569b79702218600c4f9790f45cc03100d5c6f1c..2d669917c03699876b3c8926f0de5ac88c578c01` passed during this review.
- The R4 remediation record reports the exact `wyrd-semver` unit test, Rust,
  Python, TypeScript, and server journeys; `test:shared`; `test:wyrd-sdk`;
  TypeScript unit/integration/type/N-API lanes; code generation; client-tier
  checks; formatting; lints; and cumulative diff hygiene as passing for the
  final governed sources.
- The separately preserved `TASK-002-r4/verification.md` is bound to the prior
  reviewed candidate and therefore is not treated as proof of the subsequent
  R4 source correction. The R4 implementation record supplies the correction's
  verification claim; no later source change occurred after those recorded
  commands.
- No build, test, lint, generation, or boundary command was independently run
  by this repository-standards reviewer.
