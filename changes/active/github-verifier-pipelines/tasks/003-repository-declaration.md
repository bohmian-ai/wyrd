---
id: TASK-003
status: proposed
approved_spec_revision: 3
requirements: [REQ-002, REQ-003, REQ-016]
acceptance: [AC-002, AC-011]
invariants: [INV-001, INV-002, INV-003, INV-005, INV-007]
depends_on: [TASK-002]
---

# Declare and select the native Repository policy

Carry the six rules of **approved-declaration.md** through the existing declaration
workflow. Public shape is **contracts.md → Repository and declaration**; the old
non-Card proposal in repository-declaration.md is superseded. Successful declare
means a ready exact graph and selected Repository policy, not merely inserted rows.

## Input → owner → selected implementation

The caller supplies `wyrd declare wyrd.toml`, normal YAML Repository or existing
YAML/directory declaration. Minimal TOML selects Verifier paths/registered refs;
origin remote is only a hint, and metadata/spec overrides use the approved tables.
Task 002's real connection supplies host/repository-ID/access verification and
provider name. Outside a checkout, explicit identity supplies the hint instead.

Today `wyrd-config::WyrdConfig` is defaults-only; `wyrd-loader::load` runs local
parse→config→resolve→validate→order→diagnose, and CLI `card.rs::declare_tree`
builds DeclarationInput before invoking `Cards::declare`. Extend that workflow
for an inferred native Repository root and only the selected Verifiers plus
transitive graph. Complete local validation before registration. Server-owned
identity verification resolves the hint to canonical host/positive repository ID;
first registration infers Card name, verified overrides win and provider renames
preserve the established family. Do not move provider credentials/network IO into
the offline loader. `--check` remains local and labels remote checks deferred.

Add Repository to `wyrd-spec`'s ordinary envelope/catalog/spec/reference/binding
owners, using the closed shape in contracts, not a wrapper root or Service
alias. TOML verifier arrays expand into singular VerificationBinding entries.
Read/write/generated projections must accept the same kind. Normal YAML
Repository goes through the same lifecycle and selected-policy semantics.

Reuse the entire shared client `cards/saga` prepare/hash/submit/upload/complete
and server `components/cards/service.rs`, SQL cards/version resolution and
`queries/verification.rs::project_bindings` lifecycle. Preserve normal hashing,
auto-version/reuse and exact-pin conflict. Extend its readiness/activation
coordination: select only after all required graph/artifacts are ready, preserve
the previous selection on failure, and keep registered dependencies. Tie
completion/replay to its declaration attempt and expected prior selection:
identical target may reuse; distinct races conflict; delayed old completion cannot
undo a newer selection. A deliberately new declaration of an old exact version
can roll back. Do not conflate Card Active state with selected policy or redesign
the saga into another transaction system. Exact private coordination layout stays
at existing owners with no new PostgreSQL tables.

A binding list is complete desired state. Empty `verifier = []` selects no future
automatic reviews without disconnecting access; already accepted executions keep
their policy. Amend **wyrd-design.md → Kind catalog / Per-kind specs / Registry
lifecycle / Spec-file authoring / Workspace config / Reference-direction** and
root catalog guidance to authorize Repository, TOML inference and readiness
selection. This task introduces the closed `pull_request` binding/Trigger shape;
task 006 implements its event producer, without arbitrary event-name activation.

## Output and consumer

Return normal declaration receipt with exact Repository root/outcomes and
server-managed selected-version/binding status. Task 006 must resolve these
selected registered bindings, not TOML fetched from a PR. Task 007 establishes
deferred subscriptions from this active binding owner once deferred slots exist.
A success receipt must follow graph readiness and committed selection; earlier
registration/upload acknowledgement alone does not promise activation.

Project Repository/declaration/binding types and operations in shared Rust client,
all three SDKs, CLI, HTTP/MCP and generated wire/error outputs here. Ship authoring,
identity override, offline-check and rollback/empty-list guidance here.

## Proof

**AC-002:** Start with task 002's public connection, declare the minimal TOML via
real SDK/CLI into WyrdTestServer/Postgres, and inspect exact root/outcomes/selected
binding status. Include local and exact registered refs, unrelated YAML, offline
check without a server, unchanged/changed/exact-pin content, failed upload/readiness,
racing distinct/identical activations, delayed prior completion, intentional
rollback and empty desired list. Assert previous selection and successful
dependencies survive failed activation. Ordinary YAML/directory regression and
normal YAML Repository must use the same lifecycle, not a separate fixture path.

**AC-011 (declaration):** A reader follows authoring docs and sees matching typed
receipts/errors/served schemas across SDK, CLI and HTTP/MCP.

Tier 1: corresponding checked-in Card stories in Rust/Python/TypeScript SDKs and
MCP journeys using existing runners in README; tier 2: existing SQL Card/binding
and server declaration route `integration` owners via
`mise run test:cards:integration`; tier 3: loader/config/spec graph and validation
in owning modules via affected shared/Wyrd lanes. These describe new tests,
not invented existing names. Run README contract/codegen/type checks plus
`mise run check:tenant-isolation`. Task 006's seam proof must actually admit
using this receipt's selected binding; a manually seeded binding is insufficient.
