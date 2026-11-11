---
id: TASK-003
status: proposed
approved_spec_revision: 5
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

## Obligations

| ID | Source (task line) | Observation | Planned assertion |
|---|---|---|---|
| O-01 | Input → owner, ¶1 | `wyrd declare wyrd.toml` with minimal `[repository.verified_by]` declares an inferred Repository root | CLI/Rust SDK journey declares minimal TOML and receives Repository root receipt |
| O-02 | ¶1 | Normal YAML Repository Card declares through the same lifecycle | journey declares YAML Repository; same selection/binding status |
| O-03 | ¶1 | Existing YAML/directory declaration still works unchanged | existing loader/journey regression plus a YAML Service declare in the Repository journey |
| O-04 | ¶1 | Origin remote is only a hint; server verifies host/repository_id against the tenant connection | integration: hint for an unselected repository is refused `WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE` |
| O-05 | ¶1 | Metadata/spec overrides use `[repository.metadata]`/`[repository.spec]` and win (still verified) | loader unit + journey with explicit name/repository_id |
| O-06 | ¶1 | Outside a checkout explicit identity supplies the hint | loader unit: `--repository` remote outside a checkout loads; missing identity refuses |
| O-07 | ¶2 | Missing/ambiguous identity refuses clearly | loader unit: no origin/no override → typed diagnostic |
| O-08 | ¶2 | TOML resolves only selected Verifiers plus transitive graph; unrelated YAML not declared | loader unit + journey: unrelated YAML beside wyrd.toml absent from outcomes |
| O-09 | ¶2 | Local and exact registered Verifier refs both supported | journey: path ref and registered CardRef in `verifier` array |
| O-10 | ¶2 | Local validation completes before remote registration | CLI test: invalid local Verifier fails with loader error and no server call |
| O-11 | ¶2 | First registration infers Card name from repository name | journey: root name equals normalized repository name |
| O-12 | ¶2 | Provider rename preserves the established family | integration: after connection full_name changes, inferred declare keeps family name |
| O-13 | ¶2 | Explicit name conflicting with the established family / identity refuses (no silent drift) | integration: 409 identity conflict |
| O-14 | ¶2 | Provider credentials/network IO stay out of the offline loader | source proof: loader reads only `.git/config`; check:deps |
| O-15 | ¶2 | `--check` stays local and labels remote checks deferred | CLI unit: `--check` on TOML without server lists `deferred` checks |
| O-16 | ¶3 | Repository is an ordinary Card kind in envelope/catalog/spec/reference/binding owners with the closed contract shape | wyrd-spec unit: decode/round trip, unknown field refused, kind catalog |
| O-17 | ¶3 | TOML `verifier` array expands into singular VerificationBinding entries | loader unit: two verifiers → two bindings sharing runs_on |
| O-18 | ¶3 | Read/write/generated projections accept Repository (Rust/Python/TS kind, schemas) | codegen:check, Python/TS kind tests, served schema test |
| O-19 | ¶4 | Unchanged content reuses version; changed content auto-versions; changed content at an exact pin conflicts | journey: three declares with outcome/version assertions |
| O-20 | ¶4 | Selection only after all required graph/artifacts ready | integration/journey: artifact-bearing dependency pending → Repository not selected until completion |
| O-21 | ¶4 | Failed upload/readiness preserves previous selection; registered dependencies survive | journey: failed declare leaves prior selected version and dependency Cards readable |
| O-22 | ¶4 | Distinct racing activations from the same prior selection: one selects, other conflicts | integration: two attempts same expected prior, second → 409 selection conflict |
| O-23 | ¶4 | Identical racing target reuses the selected result | integration: two attempts same target both succeed |
| O-24 | ¶4 | Delayed old completion/replay cannot undo a newer selection | integration: replay of an earlier attempt after a newer selection does not change selection |
| O-25 | ¶4 | Deliberate new declaration of an old exact version rolls back | journey: re-declare old exact version → selected again |
| O-26 | ¶4 | Card Active state is not selected policy; no new PostgreSQL tables | integration: Active non-selected version reports not selected; migration adds no table |
| O-27 | ¶5 | Binding list is complete desired state; `verifier = []` selects no automatic reviews without disconnecting access | journey: empty list → selected version has no bindings; connection still active |
| O-28 | ¶5 | Closed `pull_request` binding/Trigger shape; no arbitrary event names | spec unit: `kind = "pull_request"` decodes, unknown event refused; server refuses pull_request off a Repository |
| O-29 | ¶5 | Amend wyrd-design.md sections and root catalog guidance | docs diff + docs:check |
| O-30 | Output ¶1 | Receipt returns exact Repository root/outcomes; server-managed selected-version and binding status on the Card | journey: receipt root + `status.verification` binding ids + selection |
| O-31 | Output ¶1 | Success receipt follows graph readiness and committed selection | journey: success only after selection committed (status read after receipt shows selected) |
| O-32 | Output ¶2 | Types/operations projected in shared Rust client, Rust/Python/TS SDKs, CLI, HTTP/MCP, generated wire/error outputs | per-surface journeys; codegen:check; MCP catalog |
| O-33 | Output ¶2 | Authoring, identity override, offline-check, rollback and empty-list guidance | docs guide + docs:check |
| O-34 | Proof AC-011 | Typed receipts/errors/served schemas match across SDK, CLI, HTTP/MCP | journeys assert same codes; served schema contains Repository |
| O-35 | Proof | tenant isolation and contract checks pass | check:tenant-isolation, codegen:check |
