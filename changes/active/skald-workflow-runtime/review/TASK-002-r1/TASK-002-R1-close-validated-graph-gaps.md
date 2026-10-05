---
id: TASK-002-R1
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 11
parent_task: TASK-002
findings: [FIND-TASK-002-1, FIND-TASK-002-2, FIND-TASK-002-3, FIND-TASK-002-4, FIND-TASK-002-5, FIND-TASK-002-6]
base: 0569b79702218600c4f9790f45cc03100d5c6f1c
reviewed_candidate: e165360b1264d3628b13b02c41567c047bf96930
---

# Close validated graph loading and registration gaps

Implementation skill: `$wyrd-implement`.

## Authority and immutable inputs

- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 11.
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-002-load-and-register-graphs.md`.
- Review verdict: `changes/active/skald-workflow-runtime/review/TASK-002-r1/verdict.md`.
- Validated ledger: `changes/active/skald-workflow-runtime/review/TASK-002-r1/findings-validation.md`.
- Original base: `0569b79702218600c4f9790f45cc03100d5c6f1c`.
- Reviewed cumulative candidate: `e165360b1264d3628b13b02c41567c047bf96930`.

Implement all six findings as one bounded remediation. Reassess the complete
base-to-remediated-candidate range in the next review; do not treat this as a
standalone feature or revise approved behavior.

## Issue diagnoses and required outcomes

### FIND-TASK-002-1 — Preserve authored reference provenance

Revision 11 REQ-025 and INV-005 require a local sibling body and an authored
external `ref` to retain different source intent. The candidate stores disk
siblings and registry-fetched bodies in one `WorkflowGraph` map keyed only by
`CardRefIdentity`. `missing`, Agent lookup, and both Skald resolvers consequently
allow a sibling body to satisfy an external slot with the same kind, space,
name, and version. The external read, authorization/audit, Active-state check,
and UID assertion never occur. The server effective-spec loader has the same
source-preference defect, so registration may validate the sibling body and
later bind the external UID.

The outcome is that `InlineableRef::Sibling` consumes only the submitted or
path-loaded body and `InlineableRef::Ref` consumes only the body returned by
its exact client or tenant-registry read, even when their named identities are
equal. Preserve the existing `InlineableRef` discriminator through the shared
graph/body owner and the existing server adapter. Do not infer provenance from
identity, add a new resolver trait, duplicate the loader, or add guards at each
consumer.

### FIND-TASK-002-2 — Make the acceptance bundle version its Prompts

REQ-028, AC-002, and TASK-002 Scenario 2 require the same checked-in code-review
bundle to register exact Workflow, Agent, and Prompt versions and relationships.
The three Agent files currently embed anonymous inline Prompts, so the bundle
has four Cards, no Prompt Card identities, and no Agent-to-Prompt relationships.
The temporary tooling fixture proves a different graph and does not close this
acceptance obligation.

Reuse the existing native Prompt Card schema and loader `path` projection.
Move the three existing Prompt bodies into exact-version Prompt Card files in
the checked-in bundle and point each Agent at its Prompt through the existing
path form. Preserve request bodies, variables, models, and execution results;
do not add an alternate example or a fixture-only representation.

### FIND-TASK-002-3 — Restore one canonical reference-slot inventory

The Wyrd design requires one canonical reference-slot inventory. The candidate
adds a raw keyed-form recognizer in `wyrd-loader::parse` that hard-codes only
`prompt`, `judge_ref`, and Agent-action `target`. The existing canonical
`ReferenceSlotVisitor` also owns inlineable `verified_by[].runs_on` and every
`verified_by[].on_failure[]` element. Because normalization precedes typed
deserialization, those documented keyed forms remain wrapped and fail before
the canonical visitor can process them.

Delete the new three-name inventory. Make raw keyed-form recognition consume a
projection from the existing canonical reference-slot owner, including
inlineability and list-element shape, so adding a canonical slot does not need
a second loader edit. Keep typed deserialization, containing-file path rules,
and the canonical visitor as the authority; do not introduce another parser or
independent slot list.

### FIND-TASK-002-4 — Persist only the graph validated in preflight

REQ-014 and REQ-028 require registration to persist the resolved graph that
passed validation. Preflight currently resolves and validates UID/body A and
then commits. The write transaction reduces saved pairs to identity-only refs,
queries the current Active row, and replaces the saved pairs. If A is deleted
and UID B becomes Active at the same identity between transactions, the write
locks, binds, hashes, and persists B without validating B's body. The current
`FOR SHARE` lock prevents changes after B is selected; it does not prove B is A.

Keep the existing preflight validator, transaction ordering, and write-time
lifecycle lock. Pass the preflight `(CardRef, CardUid)` authority into the
existing write-time recheck, lock only the row matching both exact identity and
expected UID, and require it to remain Active. Refuse through the existing
unresolved-dependency path rather than substitute a replacement. Do not hold a
cross-transaction lock or duplicate resolved-graph validation in the write
path; a fresh request may preflight the replacement graph.

### FIND-TASK-002-5 — Complete mandatory Rust item contracts

`AGENTS.md` §16 and `architecture/agent-rules.md` make substantive rustdoc a
hard acceptance rule for every added or materially changed Rust item. The
candidate omits required behavior, `# Errors`, `# Panics`, or
cancellation/partial-progress documentation across changed loader parse/tests,
`workflow_loader`, server Workflow validation, and the new Postgres journey.
The validated locations are enumerated in `findings-validation.md` under
`FIND-TASK-002-5`.

Add only the missing item-local contracts. Describe keyed-form traversal and
real error conditions, name fixture invariants behind intentional panics, and
state that cancellation may occur after completed filesystem/HTTP/SQL reads
but performs no registration or durable Card write at these preflight
boundaries. Do not refactor behavior or add lint/check suppression.

### FIND-TASK-002-6 — Restore module dependency manifests

`architecture/agent-rules.md` requires imports at module scope and bare imported
or aliased types in signatures. The changed loader test has function-scoped
imports, while changed loader/client/server-test signatures spell `PathBuf`,
native Prompt, or `TempDir` through module paths.

Move the two local imports to the owning test module import block, import
`PathBuf` and `TempDir` at module scope, alias the native Skald Prompt type so it
remains distinct from the runtime Prompt type, and use the bare names in
changed signatures. This is a shape-only correction with no behavior or
dependency change.

## Constraints and preserved behavior

- Keep `wyrd-spec` synchronous, IO-free, async-free, and PyO3-free.
- Keep `WorkflowLoader` as the one client composition and Skald's existing
  resolver seam as the one runtime hydration path.
- Keep Cards exact reads, tenant-qualified `TenantConn` resolution, RLS,
  canonical authorization/audit, Active lifecycle checks, and caller-owned
  transaction boundaries.
- Preserve registration atomicity, UID-bearing durable refs, canonical hashing,
  relationship derivation, stable error codes, and idempotency behavior.
- Preserve caller-supplied local tools and declarative registration of Native
  and the two built-in tool names.
- Preserve Prompt request bodies, variables, binder ownership, local outputs,
  and registered-local execution behavior.
- Keep all new workspace edges within the already-approved owner boundaries;
  add no third-party dependency or Cargo feature.
- Follow the required struct-centered Rust style and keep pure work synchronous.

## Explicit non-goals

- No server Workflow-run execution, CLI Workflow commands, Python/TypeScript/MCP
  Workflow surface, or TASK-004/TASK-005 behavior.
- No WyrdState Workflow root, Skald registry IO, second YAML dialect, second
  executor, second Prompt renderer, or alternate resolver abstraction.
- No registration-time workflow execution, provider/tool dispatch, secret
  resolution, endpoint resolution, or server suitability policy.
- No compatibility alias, migration layer, broad loader rewrite, cross-
  transaction lock, duplicated write-time graph validator, new lint suppression,
  or unrelated documentation cleanup.

## Acceptance criteria mapped to findings

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-002-1` | A supported bundle may contain a sibling and external Agent or Prompt with the same named identity; the sibling uses only its submitted body, the external slot always performs the exact registry read, no-client loading refuses, and composite registration validates the body it will bind. |
| `FIND-TASK-002-2` | The checked-in code-review bundle contains seven exact-version Cards; registration persists UID-bearing Workflow-to-Agent and Agent-to-Prompt refs/relationships at version `1.0.0`; local and registered-local execution results remain unchanged. |
| `FIND-TASK-002-3` | Keyed `runs_on` and keyed `on_failure[]` elements normalize through the canonical reference-slot projection, while existing Workflow target and Agent Prompt forms remain green; no second field-name inventory remains. |
| `FIND-TASK-002-4` | A stale preflight plan for UID A refuses if UID B replaces it at the same identity before the write transaction, commits no registration operation/Card/relationship, and a fresh request validates B before accepting it. |
| `FIND-TASK-002-5` | Every added or materially changed Rust item in the cumulative TASK-002 diff has substantive behavior documentation and applicable `# Errors`, `# Panics`, and cancellation/partial-progress sections. |
| `FIND-TASK-002-6` | Changed modules contain no function-scoped imports outside the documented exception and no qualified type paths in changed signatures. |

## Focused proof and broader verification

Add the smallest tests that fail on the validated gaps:

1. Extend the existing client loader coverage with a same-identity sibling and
   external ref. Prove the no-client refusal and, through the existing real
   client/server boundary, distinct local and registered bodies.
2. Extend the existing composite-registration journey with the same provenance
   collision and prove incompatible external content leaves no durable state.
3. Update the existing checked-in-bundle loader, hydration, and registration
   assertions for its three exact Prompt Cards and seven registered outcomes.
4. Add one focused loader case for keyed `runs_on` and list-element
   `on_failure[]` normalization through the canonical inventory.
5. Add one Postgres interleaving case for preflight UID A replaced by Active UID
   B before write-time recheck; preserve the existing post-selection lock test.

Run every newly or specifically named test with its exact `mise exec -- cargo
nextest` selector and the repository-managed Postgres wrapper where required.
Then run the original TASK-002 focused tests and the smallest broader task lanes:

- `mise run test:shared`
- `mise run test:skald`
- `mise run test:cards:integration`
- the complete `pg_workflow_registration` target because its new cases are not
  selected by the existing Cards integration lane
- `mise run codegen:check`
- `mise run check:client-tier`
- `mise run check:pyo3-scope`
- `mise run check:registry-tx-coupling`
- `mise run fmt`
- `mise run lints`
- `git diff --check`

Use `CARGO_TARGET_DIR=/home/thorrester/Documents/GitHub/wyrd/target` for every
Cargo and mise invocation, as required by the review caller.
