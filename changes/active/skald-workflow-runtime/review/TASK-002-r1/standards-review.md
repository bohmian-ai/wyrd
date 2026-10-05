# Repository standards review — TASK-002

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `e165360b1264d3628b13b02c41567c047bf96930`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-002-load-and-register-graphs.md`
- Scope: repository-rule compliance only. Task acceptance is reviewed independently.

CodeGraph was not used because the repository has no `.codegraph/` directory.

## Authority coverage

| Changed surface | Applicable authority inspected | Coverage and rule result |
|---|---|---|
| Active task packet and implementation evidence | `AGENTS.md` §§11–16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md` | **PASS.** The task identifies approved Revision 11, exact immutable inputs, focused selectors, broader lanes, boundary gates, and implementation evidence. No source file embeds task or agent history. |
| `wyrd-client` manifest, lockfile, module/export surface | `AGENTS.md` §§2–6, 9, 15–16; `architecture/references/architecture/patterns.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/languages/rust-core.md`; `architecture/wyrd-design.md` client model; `architecture/wyrd-doctrine.mdx` | **PASS.** `WorkflowLoader` is a dependency-owning client handle; async is confined to filesystem/HTTP composition, pure graph validation remains synchronous, Skald has no upward client/server edge, and all added dependencies already exist in the workspace. The client cone adds no SQL, cloud, DataFusion, Iceberg, server, or new third-party dependency. |
| `wyrd-client::workflow_loader` implementation and unit journey | `AGENTS.md` §§4–6, 9, 11, 16; `architecture/agent-rules.md`; `architecture/references/languages/{rust-core,maintainer-style,errors,testing-workflows}.md` | **FAIL.** Ownership, stable Wyrd errors, exact Card reads, and test placement comply. Mandatory documentation is incomplete (`STD-TASK-002-001`) and one signature uses a fully qualified type instead of an imported bare name (`STD-TASK-002-002`). |
| `wyrd-loader` parsing and pure validation | `AGENTS.md` §§3–6, 11, 16; `architecture/agent-rules.md`; `architecture/references/languages/{rust-core,maintainer-style,testing-workflows}.md`; `architecture/wyrd-design.md` §Spec-file authoring and reference-slot inventory | **FAIL.** The parsing/validation work remains synchronous, IO stays in the loader, and the checked-in bundle has direct regression proof. Two materially modified helpers lack required rustdoc, and the new test adds function-scoped imports and a fully qualified signature type (`STD-TASK-002-001`, `STD-TASK-002-002`). |
| Skald hydration-seam documentation | `AGENTS.md` §§2–6, 16; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/languages/{rust-core,maintainer-style}.md`; `architecture/wyrd-design.md` §Workflow | **PASS.** The change is documentation-only, accurately keeps Skald free of loader/client/registry IO, and leaves the existing runtime owner and resolver seam intact. |
| Server effective-spec resolution and Workflow registration validation | `AGENTS.md` §§3–6, 9, 11, 15–16; `architecture/agent-rules.md`; `architecture/references/architecture/patterns.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/languages/{rust-core,errors,testing-workflows}.md`; `architecture/wyrd-security-posture.md`; `architecture/wyrd-design.md` §§Registry lifecycle, Spec-file authoring | **FAIL.** Server composition remains the durable acceptance owner; reads use caller-owned `TenantConn`, rely on RLS, select Active exact references, and commit no write inside the callee. The new async Workflow-validation path lacks the required cancellation documentation (`STD-TASK-002-001`). No audit decision, URL fetch, secret, generated contract, or public handler changed. |
| New `pg_workflow_registration` client→server tests and test target | `AGENTS.md` §11 and §16; `architecture/agent-rules.md`; `architecture/references/languages/{testing-workflows,maintainer-style,rust-core}.md`; `architecture/wyrd-security-posture.md` tenant isolation | **FAIL.** A separate `pg_*` target is earned: it starts a real server, uses repository-managed Postgres, crosses the real HTTP client boundary, and checks tenant invisibility and durable state. Its Rust documentation and signature imports remain incomplete (`STD-TASK-002-001`, `STD-TASK-002-002`). Test setup uses the fixture's explicit privileged pool only to create foreign-tenant state; production paths remain RLS-scoped. |
| Checked-in Workflow/Agent/input example bundle and `examples/README.md` | `AGENTS.md` §§2, 9, 11–12; `architecture/wyrd-design.md` §§Workflow, Spec-file authoring, Reference forms; `architecture/wyrd-doctrine.mdx` | **PASS.** The files use Wyrd-native Card envelopes and canonical keyed `path`/`inline` forms, contain no credentials, and are exercised directly by loader, hydration, and registration tests. |
| Errors, generated artifacts, schemas, stubs, and check integrity | `AGENTS.md` §§4, 8–12; `architecture/agent-rules.md`; `architecture/references/languages/errors.md`; `architecture/references/languages/testing-workflows.md` | **PASS.** The implementation reuses derive-backed `WyrdError` variants and does not add parallel error metadata. No generated artifact or schema source changed, no check/test was weakened, ignored, allowed, or deleted, and the task records `codegen:check`, client/PyO3/transaction boundary checks, format, lints, exact tests, and `git diff --check` as green. These command results were available in the immutable task evidence and were not independently rerun in this standards pass. |

## Material repository-rule findings

### STD-TASK-002-001 — Mandatory Rust documentation is incomplete

- **Violated rules:** `AGENTS.md` §16 and `architecture/agent-rules.md` require substantive rustdoc for every new or materially modified Rust item, including private helpers and tests; every fallible item needs `# Errors`, panic-capable items need `# Panics`, and applicable async IO needs cancellation/partial-progress behavior.
- **Locations and evidence:**
  - `crates/shared/wyrd-loader/src/parse.rs:236` materially changes `materialize_inline_files` to normalize keyed reference forms but the fallible helper has no rustdoc or `# Errors`; `is_inlineable_body` at line 360 is also materially changed and undocumented.
  - `crates/shared/wyrd-client/src/workflow_loader.rs:340` and `:376` add fallible resolver methods without `# Errors`.
  - The public async IO operations at `workflow_loader.rs:74` and `:122`, their private IO stages at `:142` and `:160`, and server graph reads at `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:407` do not document cancellation/partial-progress behavior.
  - New panic-capable helpers and tests omit `# Panics`, including `workflow_loader.rs:487`, `:510`, and `:531`; `crates/shared/wyrd-loader/src/lib.rs:234`; and `crates/wyrd/wyrd-server/tests/pg_workflow_registration.rs:49`, `:239`, `:249`, `:313`, and `:529`.
- **Consequence:** The candidate violates a hard `BLOCK_BEFORE_MERGE` repository rule, and maintainers cannot determine from the owning items whether cancellation leaves durable state, which resolver failures propagate, or which test helpers intentionally panic on fixture invariants.
- **Smallest testable correction:** Add substantive rustdoc only to the new/materially modified items: describe keyed-form traversal for the loader helpers; add accurate `# Errors` to fallible resolver methods; state that loader/server read cancellation performs no registration or durable write and may stop after completed reads; and add concise `# Panics` sections to panic-capable test items. Do not refactor behavior. Verify with format, lints, and the existing focused tests.

### STD-TASK-002-002 — Imports and signature types do not follow the mandatory module dependency-manifest rule

- **Violated rule:** `architecture/agent-rules.md` requires every `use` at module top and bare imported type names in signatures; the test-module exception permits the test module's own top-level import block, not function-scoped imports.
- **Locations and evidence:**
  - `crates/shared/wyrd-loader/src/lib.rs:235-236` introduces two function-scoped `use` statements; line 224 spells `std::path::PathBuf` in the signature instead of importing `PathBuf` in the test module.
  - `crates/shared/wyrd-client/src/workflow_loader.rs:376` spells `skald_spec::Prompt` in a signature rather than importing or aliasing the native Prompt type.
  - `workflow_loader.rs:510` and `crates/wyrd/wyrd-server/tests/pg_workflow_registration.rs:105` spell `tempfile::TempDir` in signatures despite each module having a top-level import block (the server test can import `TempDir`; the client test already imports `tempfile` only by path).
- **Consequence:** These modules hide part of their dependency surface inside an operation and make signatures inconsistent with the repository's required owner/readability convention.
- **Smallest testable correction:** Move `WorkflowAction`, `InlineableRef`, `PathBuf`, and `TempDir` into their module-level import blocks; alias the two Prompt types at module scope and use the bare alias in the resolver signature. Run format and lints; no behavioral test change is needed.

## Applicable rule results

| Rule | Result | Evidence |
|---|---|---|
| Struct-centered ownership and no speculative abstraction | PASS | `WorkflowLoader` owns optional `Cards` plus the caller tool resolver; `WorkflowGraph` owns pure graph state; `EffectiveSpecs` owns tenant-scoped resolved bodies. Existing Skald resolver traits are reused. |
| Async only for IO/composition | PASS | Loader filesystem/HTTP and server SQL methods await real IO; parsing, keyed-form normalization, graph inventory, hydration, and validation stay synchronous. |
| Server owns durable validation and client performs no hidden write | PASS | Registration resolves and validates before the write transaction; local/registered loading only reads and hydrates. |
| Tenant isolation and transaction ownership | PASS | Production graph resolution accepts `&mut TenantConn<'_>`, uses existing RLS queries, and never commits or rolls back. The caller opens and commits the read transaction. |
| Stable public errors | PASS | Existing derive-backed registry/workflow errors are reused; no hand-written code/status/catalog path is added. |
| Client-tier dependency cost and Skald direction | PASS | No forbidden client dependency or Skald→client/server edge appears in manifests or source; no new Cargo feature or third-party dependency is introduced. |
| Test taxonomy and target placement | PASS | Pure cases are inline unit tests; the real HTTP/Postgres journey is in an earned `pg_*` integration target and uses `WyrdTestServer`. |
| Generated/codegen and public-contract handling | PASS | No generated source or wire schema changed; the recorded codegen check is appropriate. |
| Check/test integrity | PASS | No `allow`, ignore, deletion, weakened assertion, sleep, boundary-glob change, or invented test environment appears in the diff. |
| Mandatory Rust documentation | **FAIL** | `STD-TASK-002-001`. |
| Module-top imports and bare signature types | **FAIL** | `STD-TASK-002-002`. |

## Verification limits

- Inspected the complete base-to-candidate diff, manifests and lockfile, changed source, direct owners/callers, test target registration, example inputs, repository task definitions, and the task's recorded verification evidence.
- `git diff --check` for the immutable range produced no output.
- No Cargo or mise command was rerun during this independent pass; the task records the focused tests, `test:shared`, `test:skald`, `test:cards:integration`, `codegen:check`, boundary checks, format, and lints as passing. Those automated checks do not enforce the private-item documentation and import-policy failures above.

## Overall result

**FAIL** — `STD-TASK-002-001` and `STD-TASK-002-002` are bounded repository-rule violations. The reviewed ownership, dependency, tenancy, transaction, error, generated-contract, test-tier, and check-integrity rules otherwise pass.
