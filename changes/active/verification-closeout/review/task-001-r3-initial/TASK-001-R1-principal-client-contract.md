---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-verification-closeout
spec_revision: 2
parent_task: TASK-001
remediates: [FIND-TASK-001-2, FIND-TASK-001-3]
---

# R1: Close principal client declarations and Rust documentation

## Subject and intended outcome

Approved spec: `changes/active/verification-closeout/spec.md` revision 2. Original task: `changes/active/verification-closeout/tasks/TASK-001-r3-principal-roles-and-local-flow.md`. Review: `changes/active/verification-closeout/review/task-001-r3-initial/verdict.md`. Base `c46afdcac`; reviewed candidate `437205debc628538ba6aa4ec828601c7c40145b4`. This task corrects the two independently validated findings while preserving the accepted local and signed-in workflows.

Principal discovery must expose an accurate typed CardRef value to Python and TypeScript callers. Changed Rust declarations must meet the repository rustdoc gate.

## Diagnoses and selected corrections

### FIND-TASK-001-2 — Principal CardRef types do not match the returned value

`PrincipalSummary.card_ref` in `wyrd-spec` is a structured `CardRef`. The shared client returns that wire value. `sdks/wyrd-sdk-ts/wyrd/src/index.ts::PrincipalSummary` declares a loose string record although the SDK already has `CardRef`; `sdks/wyrd-sdk-python/python/wyrd/stubs/principals.pyi` declares `dict[str, str]`, although the Python boundary produces a dictionary whose optional UID can be null. Callers taking a discovered Card-bound principal into another Card operation therefore receive inaccurate type guidance, and TypeScript accepts invalid shapes. The current discovery journeys assert principal identity but do not use its Card reference.

Use the existing TypeScript `CardRef` type for the TypeScript result. In the Python stub source, describe the actual mapping with required Card identity fields and accurate optional/null fields, then regenerate the assembled public stubs through the existing generator. Keep the Python runtime dictionary; declaring its `CardRef` class as the result would misstate runtime behavior. Do not add conversion, transport, or validation machinery to repair a declaration-only defect.

### FIND-TASK-001-3 — Changed Rust items lack mandatory rustdoc

New declarations at `wyrd-cli/src/principal/mod.rs::assignment`, `wyrd-sdk-rust/src/lib.rs::otel`, `wyrd-sdk-python/src/lib.rs::principals`, `wyrd-sdk-ts/native/src/lib.rs::principals`, `wyrd-server/tests/integration/main.rs::pg_principal_roles`, and `wyrd-sql/src/queries/auth/role_assignments.rs::LIST_USER_ROLE_ASSIGNMENTS_SQL` lack item-level rustdoc. The SQL module has sibling new constants with the same issue. `AGENTS.md` §16 and `architecture/agent-rules.md` require substantive documentation for new or materially changed Rust items, including private and test items; the green lints did not check all of them.

Document the changed declarations in place with purpose, workflow role, and any relevant invariant or side effect. Inspect the complete cumulative changed Rust declaration set once for the same omission, including any new corrections in this remediation. Fallible changed functions require accurate `# Errors` sections. No new documentation checker or style-only task is needed.

## Constraints and non-goals

- Preserve tenant isolation, permission/audit ordering, existing not-found behavior, and all accepted gateway, OTLP, verification, and local-login paths.
- Keep durable contracts in `wyrd-spec` and SDK projections in their owning packages. Do not hand-edit generated public `.pyi` output.
- Do not add Role aliases, principal direct permissions, an MCP principal surface, compatibility routes, another refresh path, a Rust gateway adapter, or a second attribution policy layer.
- The user excluded historical migration work from this review. Do not add a forward migration or historical-schema test in this task. External non-UUIDv7 cursor rejection and non-blocking structural notes are also outside this remediation.

## Acceptance and focused proof

| Finding | Acceptance criterion | Focused proof |
|---|---|
| FIND-TASK-001-2 | Both SDK declarations expose the actual CardRef fields and Python's return remains a mapping. | Add a focused typed caller use of `card_ref` in the existing Python and TypeScript principal test/typecheck owners; run their exact named focused commands, `mise run py:typecheck`, the owning TypeScript typecheck, and `mise run codegen:check`. |
| FIND-TASK-001-3 | Every new or materially changed Rust declaration in the cumulative candidate has substantive rustdoc, including private constants and module/test declarations. | Inspect the cumulative Rust diff against `AGENTS.md` §16, then run `mise run fmt` and `mise run lints`. A test mirroring documentation presence is unnecessary. |

Run only the narrowest additional mise lane required by the actual remediation write set, including SDK typechecks. Full user-journey suites and broad aggregate gates run once at change review, not in this bounded remediation.
