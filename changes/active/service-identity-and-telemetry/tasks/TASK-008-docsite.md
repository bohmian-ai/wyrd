---
id: TASK-008
kind: implementation
status: proposed
spec: SPEC-service-identity-and-telemetry
spec_revision: 12
depends_on: [TASK-001, TASK-002, TASK-003, TASK-004, TASK-005, TASK-006, TASK-007]
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-020, REQ-021, REQ-022, REQ-023, REQ-024, REQ-025, REQ-026, REQ-027, REQ-028, REQ-029, REQ-030, REQ-031, REQ-032, REQ-033, REQ-034, REQ-035, REQ-036, REQ-037, REQ-038, REQ-039, REQ-040, REQ-041, REQ-042, REQ-043, REQ-044, REQ-045, REQ-046, REQ-047, REQ-048]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011, AC-012, AC-013, AC-014, AC-015, AC-016, AC-017, AC-018, AC-019, AC-020, AC-021]
proof: static documentation and existing-example regression
---

# Document the shipped identity, evidence, and table contracts

## Outcome and Value

Publish one coherent developer and agent documentation path for the new
baseline. A reader can configure one stable Service account for two releases,
understand when an execution root is optional, and query evidence by its actual
workload, subject, and account. References describe the shipped signal schemas
and editable table layouts rather than preserving old catalog or space forms.

This is the **last implementation task**, after all seven dependencies have
settled their runtime, contracts, SDKs, runnable examples, and owning architecture
amendments. Finish this task and its documentation proof before integrated
change review. It does not postpone missing runtime work or architecture
decisions until documentation closeout.

## What Changes and What Must Stay True

Update the owning guides, concepts, references, embedded examples, navigation,
and generated machine-readable documentation against published contracts and
the dependencies' evidence. Keep guides focused on completing a reader's job;
put full fields, limits, permissions, and failure conditions in references.

Cover stable accounts and optional execution roots (including authorized
rootless Users), flat Card references, complete signal schemas, the single
`bifrost` catalog, shared correlation, configurable layouts and Blooms, atomic
revisioned edits and evolution, visible-trace verification/retry, and fresh
v0.3.0 setup/reset. The supplements supply the exact contracts; the acceptance
table below maps them to documentation proof.

Use already proved examples from the dependencies. No new runtime API,
application logic, generator, migration, compatibility alias or performance
promise belongs here. Do not execute a destructive reset or hide required
coverage behind `draft: true`. Missing behavior, architecture amendments or
example tests return to their owning dependency.

## Approach

1. Check dependency evidence and the amended architecture against the approved
   packet. Build a small page-to-contract checklist using the acceptance table
   below; expose any shipped disagreement before publishing it.
2. Revise existing setup, declaration, authentication, telemetry, Bifrost,
   verification, and operations pages. Add reference sections/pages only where
   the current owners cannot hold the required exhaustive contract clearly.
3. Embed the dependencies' runnable examples with `CodeFromFile`; update
   displayed output only from captured evidence. Keep language order Python,
   Rust, TypeScript and use the reader's public tools in published commands.
4. Refresh API/error references and both `llms` outputs through existing
   generators. Check discoverability, links, page types, and published/draft
   state against the resulting content tree.
5. Run the documentation proof and any affected example regression, inspect
   rendered pages and machine-readable output, and record concise evidence for
   integrated review.

## Proof Strategy

TDD is not applicable: this task changes prose, static references, embeds and
generated artifacts using behavior already tested by the dependencies. Compare
references to runtime schemas, resolved table descriptions, public errors and
the supplements, including complete nested types. Read procedures literally;
inspect rendered pages and machine text for matching commands and meanings.
A successful build alone cannot establish contract accuracy.

Reuse dependency example evidence unless embedded source or displayed output
changed. Executable example/generator logic changes require their owning
behavior test and explicit Behavior/RED/GREEN/REFACTOR plan; return that work
to the responsible dependency rather than expanding this static task.

## Acceptance Criteria

These mappings own documentation closure only; runtime proof remains with
the dependencies.

| Obligations | Observable documentation and proof |
| --- | --- |
| REQ-001–017, 033; INV-001–007; AC-001–005, 011 | Account/auth guides and references agree with shipped creation, restriction, credential and grant contracts. A reader distinguishes optional execution identity from permissions, knows restricted missing/false context is refused, and sees no registration-created principal or implicit scheduled authority. Check against auth supplement and dependency journey evidence. |
| REQ-018–020, 029, 032; AC-006, 010 | Declaration, selectors, config and embedded output use flat references on every documented surface. Search and inspect changed pages, examples and generated text for space and substitute namespaces; no obsolete identity syntax survives. |
| REQ-021–024, 028, 034, 042; INV-008; AC-007, 012 | Signal references match complete shipped schemas, nested types and production rules. Seven managed fields are distinguished from promoted resources; forged managed/derived values are described as refused. Compare to live/schema-owner evidence without inventing a second stored schema. |
| REQ-025–026, 035–039, 044, 047–048; AC-008, 013–014, 016, 018, 021 | All nine system defaults and dynamic-table rules are accessible. Configuration reference states exact permissions, transform/type limits, revisioned atomic edits, clearing Blooms, and evolution guarantees. Query examples separate ownership, subjects, resource filtering and account identity. Compare against resolved table descriptions and approved storage ledger. |
| REQ-014–017, 040, 043, 045–046; AC-005, 014, 017, 019–020 | Verification pages explain full authorized visible trace evidence, attempt-local reuse, direct/queued differences, bounded availability retry and late-arrival limits. Result ownership remains distinct from source-span attribution; no retry-until-pass or unsupported latency/completeness claim appears. |
| REQ-027–032, 039; AC-009–010, 014 | Setup/reset instructions explicitly describe the fresh baseline and loss from optional recreation. Public examples, table permissions and SQL use `bifrost.*`; no old catalog aliases, token aliases or conversion path are taught. No reset is executed. |
| REQ-041; AC-015; all remaining cross-surface closure | This task follows TASK-001–007 and precedes integrated review. Generated references, navigation and machine text agree with pages; documentation checks and affected example proof pass. A local generated-drift limitation is recorded for resolution on the integrated candidate; it does not waive the gate. No required page is withheld merely to obtain a green build. |

## Implementation Reference

Paths below are relative to `docs/src/content/docs/` unless qualified.
Extend these inspected owners rather than creating parallel guidance:

- `understand/identity-and-authorization.md`, Principals/Credentials/Roles,
  teaches release-created principals and space. Reconcile it with
  `operate/grant-and-inspect-permissions.md`, `reference/mcp-tools.md`, and
  `reference/agent-tools-and-authorization.md` using the shipped auth contract.
- `get-started/setup.md`, Manual setup, owns installation;
  `build-and-verify/declare/define-a-service.md` and
  `understand/cards-versions-and-relationships.md` own declaration/identity.
  Their quickstart/integration consumers need consistent syntax and output.
- `understand/bifrost.md` owns catalog/visibility concepts. The draft
  `build-and-verify/observe/write-and-query-custom-data-with-bifrost.md`
  contains old catalog names, immutable-layout claims and handwritten SDK
  blocks. Reconcile these with shipped APIs and link full schema/layout
  references; preserve correct durability/visibility explanations.
- `build-and-verify/verify/`, `understand/how-verification-works.md`,
  `reference/verification-runtime-limits.md`, and
  `operate/configure-verification-runtime.md` own evidence and timeout guidance.
- `examples/README.md` maps `examples/quickstart/` to real-server journeys in
  all three SDKs. `examples/tests/quickstart.rs` checks repeated Rust output.
  Dependency tasks own example behavior; reuse their sources via `CodeFromFile`.
- `docs/scripts/generate_api_docs.py::error_catalog`, `render_errors`, and
  `render_openapi` derive errors and link live OpenAPI. Extend their source
  contracts, never hand-edit generated pages or commit a second OpenAPI copy.
- `docs/scripts/generate_llms_txt.py::sidebar_pages`, `page_bodies`,
  `render_llms_txt`, and `render_llms_full` derive indexes with site embeds.
  `docs/src/lib/content.ts::validateFrontmatter` and existing content/index,
  navigation/embed/shell-link tests enforce structure;
  `wyrd-spec/examples/gen_schemas.rs::contracts` supplies `docs:test` metadata.
  Reuse these owners without introducing a second schema or renderer.

## Verification and Evidence

Run from repository root through `mise`; serialize Cargo-backed operations.

| Check | Command and expected evidence |
| --- | --- |
| Regenerate published references and machine text | `mise run docs:generate`. Inspect generated API references, `docs/public/llms.txt` and `docs/public/llms-full.txt`; regenerate twice and confirm no second content delta. Never hand-edit generated output. |
| Owning documentation gate | `mise run docs:check`. Covers install, generation/drift, contract metadata/site tests, command/link checks, build and accessibility. Inspect rendered changed pages and their machine-text counterparts as well. This task currently does **not** execute SDK example journeys. |
| Local generated-drift limitation | The existing gate compares generated files to Git and rejects untracked outputs, so intentional uncommitted generated changes can fail its drift step. Record that limitation without staging or committing for the check. Verify generation is repeatable and the local diff is expected, then run `mise run docs:test`, `mise run docs:check:commands`, `mise run docs:linkcheck`, `mise run docs:build`, and `mise run docs:a11y` separately to prove the remaining components locally. This is interim evidence, not a waived drift check; the integrated candidate must pass the unchanged gate. |
| Changed runnable examples only | `mise run check:examples` builds Rust examples; `mise run test:examples` runs their real-server journeys with repository Postgres. Neither claims Python/TypeScript journey coverage. Reuse the exact focused Python/TypeScript example commands and setup recorded by the dependency task for any changed embed source; record actual selectors and observed output rather than running full SDK suites here. |

Record dependency example evidence used by each changed guide and any actual
rerun. No runtime test is newly named or invented by this task. If a changed
example lacks an exact owned runner, return it to the dependency owner before
publishing. Integrated cross-surface suites run once at change review, not as
a broad gate repeated here.

## Material Stop Conditions

- A dependency has not delivered a required runtime, SDK, executable-example,
  error/schema, or architecture amendment: return the gap to that owner.
- Shipped behavior contradicts an approved contract: correct the owning
  implementation; a change to approved semantics requires spec revision.
- A documented operation cannot be completed using the public API or its
  example lacks real-server proof: do not fabricate an API, output or test.
- A reset instruction would silently discard state, imply historical
  migration support, or require destructive action to finish this task.

## Authority Links

- [Approved revision 12](../spec.md), including the separate final-docsite requirement.
- [Authentication contracts](../supplements/auth-system.md).
- [Complete telemetry schemas](../supplements/telemetry-schemas.md).
- [Storage, correlation, alteration and trace decisions](../supplements/table-storage-and-correlation.md).
- [Architecture authority map](../../../../architecture/README.md): Wyrd design Runtime identity, Reference forms and Workspace config; security posture Principal and credential lifecycle, Authorization and policy; Bifrost Table and row identity, Durability and visibility, and Public surface. Dependencies amend these owners before this task.
- [Docs rules](../../../../docs/AGENTS.md), [contributor workflow](../../../../docs/CONTRIBUTING.md), [product audience](../../../../docs/PRODUCT.md).
- [Spec-driven development](../../../spec-driven-development.md), [implementation execution](../../../implementation-execution.md), and [testing map](../../../../TESTING.md).
