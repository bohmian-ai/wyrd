# Structured Ponytail validation — TASK-002

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `e165360b1264d3628b13b02c41567c047bf96930`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-002-load-and-register-graphs.md`

`HEAD` resolved to the candidate before validation. The reviewed source was not
modified. CodeGraph was attempted first, but the CLI reported that this checkout
has no usable index, so source and callers were traced directly.

## Proposal validation

| Discovery proposal | Result | Independent source validation |
|---|---|---|
| `INV-REV-001`, `TEN-SEC-001`: sibling/ref provenance collision | **REVISED** into `FIND-TASK-002-1` | Confirmed in `WorkflowGraph`: disk siblings and registry bodies share one identity-only map, while `missing`, `agent`, and both resolvers erase `InlineableRef::{Sibling, Ref}` provenance. The same graph is consumed by server registration, where `EffectiveSpecs::load` also prefers a submitted sibling by identity. The defect therefore affects both authored local loading and composite registration, not only the client path described by discovery. |
| `INV-REV-002`: checked-in bundle lacks versioned Prompt Cards | **CONFIRMED** as `FIND-TASK-002-2` | Revision 11 AC-002 and TASK-002 Scenario 2 require the same checked-in bundle to store exact Agent and Prompt versions. All three example Agents embed anonymous Prompts, and the registration journey asserts four Cards and only three Agent relationships. The separate temporary tooling fixture does not prove the named acceptance bundle. |
| `MNT-001`: duplicate, incomplete keyed-reference slot inventory | **CONFIRMED** as `FIND-TASK-002-3` | `parse.rs` recognizes only `prompt`, `judge_ref`, and Agent-action `target`; the canonical visitor also owns inlineable `runs_on` and each `on_failure[]`. The raw normalization runs before typed deserialization, so those documented keyed forms are not unwrapped and cannot reach the canonical visitor. |
| `DUR-001`: preflight/write UID replacement race | **CONFIRMED** as `FIND-TASK-002-4` | Preflight validates UID/body A and commits. The write transaction discards the preflight UID, resolves the identity again, and binds whichever Active UID B now occupies it. `FOR SHARE` protects B only after selection; it does not prove B is A. The follow-up correctly resolves the conflict with the system review. |
| `STD-TASK-002-001`: incomplete mandatory rustdoc | **REVISED** into `FIND-TASK-002-5` | The reported locations violate the hard documentation rule. Direct diff inspection found additional changed fallible or panic-capable items requiring the same correction, including the new parser test, both test gateway `call` methods, `card_ref`, and the helper/test functions listed below. |
| `STD-TASK-002-002`: import/signature policy violations | **CONFIRMED** as `FIND-TASK-002-6` | The two imports inside `load_explicit_workflow_bundle` are function-scoped, and four changed signatures spell types through module paths rather than module-top imports/aliases. |

The behavior review's empty finding set does not negate reachable source paths.
The system review's claim that the write-time lock closes the preflight/write
race is rejected for the completed identity-replacement interval established by
`service.rs` and the SQL query. No other discovery proposal survives validation,
and none of the retained corrections requires a specification revision.

## Final validated finding ledger

### FIND-TASK-002-1 — REVISED — INCORRECT: graph hydration erases sibling versus external-ref provenance

- **Discovery IDs:** `INV-REV-001`, `TEN-SEC-001`
- **Violated obligation:** Revision 11 REQ-025 and INV-005; TASK-002's packet-local loading seam and Scenario 3; `architecture/wyrd-design.md` “Path resolution rules,” which requires an authored external `Ref` to remain external even when a submitted sibling has the same identity.
- **Exact location:** `crates/shared/wyrd-client/src/workflow_loader.rs:95-102,185-190,221-263,318-324,337-385`; server consumer `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:407-466`.
- **Evidence:** `load_file` inserts every path-loaded Agent and Prompt into `WorkflowGraph::bodies`, keyed only by `CardRefIdentity`. `WorkflowGraph::missing`, `agent`, and `GraphResolver` use `as_card_ref()` plus the same identity key, so a local `Sibling` body satisfies an authored external `Ref` with the same `(kind, space, name, version)`. The external slot skips `WorkflowLoader::read`, including authorization/audit, Active-state, and UID checks. On the server, `EffectiveSpecs::load` likewise checks its sibling-spec map before the external UID map, so registration validation can consume the sibling body for both slots even though persistence later binds the external slot to the registered UID.
- **Observable consequence:** a bundle can declare one local sibling and one external dependency with the same named identity but different bodies. Local loading executes the disk body for the nominal external slot, including without a client. Composite registration can validate the sibling body and then durably bind the external slot to a different registered body, accepting a graph that the validator never examined.
- **Decision-complete minimal correction:** keep the existing `InlineableRef::{Sibling, Ref}` discriminator authoritative through the shared `WorkflowGraph` body lookup and through the server's `EffectiveSpecs` body source. A sibling slot may consume only the submitted/path-loaded body; an external slot may consume only the result associated with its exact Cards/tenant-registry read. Do not add another loader, resolver trait, or downstream guard, and do not infer source from identity. Apply the correction at the shared graph/body owner and its existing server adapter so Agent and transitive Prompt consumers inherit it.
- **Focused closure proof:** extend the existing `wyrd-client` loader test with a sibling and external Agent or Prompt sharing a named identity but carrying observably different bodies: without a client the external slot must return `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY`; through the existing real client/server journey it must read and use the registered body while the sibling retains the local body. Add the corresponding composite-registration collision case and prove an incompatible external body is refused with no operation, Card, or relationship committed.

### FIND-TASK-002-2 — CONFIRMED — MISSING: the acceptance bundle has no versioned Prompt Cards or Prompt relationships

- **Discovery IDs:** `INV-REV-002`
- **Violated obligation:** Revision 11 REQ-028 and AC-002; TASK-002 Scenario 2, which requires the same checked-in code-review bundle to register exact Agent and Prompt versions and relationships in dependency order.
- **Exact location:** `examples/workflows/code-review/agents/security.yaml:8-23`, `examples/workflows/code-review/agents/correctness.yaml:8-23`, `examples/workflows/code-review/agents/final-reviewer.yaml:8-30`; `crates/wyrd/wyrd-server/tests/pg_workflow_registration.rs:318-387`.
- **Evidence:** each checked-in Agent uses `prompt.inline`, so the bundle contains only three Agent Cards plus one Workflow. The journey explicitly asserts four outcomes and three Workflow-to-Agent relationships. Its separate temporary tooling bundle proves one path-loaded Prompt, but it is not the acceptance artifact named by AC-002.
- **Observable consequence:** the shipped code-review bundle has no Prompt Card identity, version, UID, or Agent-to-Prompt relationship to preserve or inspect, so AC-002 is unproved and unsatisfied for that bundle.
- **Decision-complete minimal correction:** reuse the existing loader `path` projection and native Prompt Card schema: move the three existing Prompt bodies into exact-version Prompt Card files in the checked-in code-review bundle and have each Agent reference its Prompt by path. Keep the Prompt request bodies and binder variables unchanged; add no alternate example or fixture-only contract.
- **Focused closure proof:** update the existing loader, hydration, and registration journeys for the same checked-in bundle. The registration journey must assert all seven Active outcomes, UID-bearing Agent-to-Prompt refs, direct Workflow-to-Agent and Agent-to-Prompt relationship rows, exact `1.0.0` versions, and unchanged local execution/binding results.

### FIND-TASK-002-3 — CONFIRMED — VIOLATION: keyed-form normalization duplicates and omits canonical reference slots

- **Discovery IDs:** `MNT-001`
- **Violated obligation:** `architecture/wyrd-design.md` “Reference-slot inventory” single-source rule and TASK-002 Scenario 1 REFACTOR, which require loader projection to reuse the canonical inventory rather than maintain a Workflow-specific field list.
- **Exact location:** `crates/shared/wyrd-loader/src/parse.rs:236-370`; canonical owner `crates/wyrd-spec/src/refs/mod.rs:34-45,128-158,488-527`.
- **Evidence:** `is_inlineable_slot` hard-codes `prompt`, `judge_ref`, and Agent-action `target`. `ReferenceSlotVisitor` additionally owns `verified_by[].runs_on` (`InlineableTrigger`) and every `verified_by[].on_failure[]` (`InlineableOperator`). Because keyed-form unwrapping precedes typed `Spec` deserialization, `runs_on: { ref|path|inline: ... }` and keyed list elements under `on_failure` remain wrapped and fail before the canonical visitor can project them.
- **Observable consequence:** documented authoring syntax works or fails according to an incomplete second field-name inventory, and adding a canonical reference slot requires a hidden second edit in the loader.
- **Decision-complete minimal correction:** make raw keyed-form recognition project from the existing canonical reference-slot owner in `wyrd-spec`, including whether a slot is inlineable and whether it occurs in a list, then have `wyrd-loader` consume that projection before typed deserialization. Delete the three-name inventory in `parse.rs`; do not create a second parser or another independently maintained list.
- **Focused closure proof:** add one focused loader test covering keyed `runs_on` and keyed `on_failure[]` forms, including a list element, while retaining the checked-in Workflow `target` and Agent `prompt` proof. The canonical reference inventory/completeness test must remain the single place whose expansion makes a new slot participate.

### FIND-TASK-002-4 — CONFIRMED — INCORRECT: write-time authority can bind a replacement UID/body that was not validated

- **Discovery IDs:** `DUR-001`; corroborated and resolved by `followup-review.md`
- **Violated obligation:** Revision 11 REQ-014 and REQ-028; TASK-002 Scenario 2's requirement that only the resolved graph that passed validation be accepted and persisted atomically.
- **Exact location:** `crates/wyrd/wyrd-server/src/components/cards/service.rs:995-1004,1055-1104`; `crates/wyrd/wyrd-sql/src/queries/cards/relationships.rs:17-54`; validation producer `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:388-466`.
- **Evidence:** preflight resolves and validates external UID/body A, then commits its transaction. `write_registration` maps the saved `(CardRef, CardUid)` pairs back to identity-only `CardRef`s and replaces them with `recheck_active_card_refs` results. That query filters by named identity and Active status but not the expected UID. A completed delete of A followed by registration of Active UID B at the same identity is permitted before the write transaction; the write locks and binds B without rerunning graph validation.
- **Observable consequence:** registration can succeed and persist relationships to Agent/Prompt body B even though only A supplied resolved Prompt-binding and route-dialect validation. A later registered load can reject the accepted graph. SQL atomicity remains intact; semantic validation-to-persistence identity does not.
- **Decision-complete minimal correction:** preserve the existing preflight `(CardRef, CardUid)` pairs as the input to the existing write-time authority. `recheck_active_card_refs` must lock the row matching both the exact identity and expected UID and require that row to remain Active; it must refuse rather than substitute a replacement. Keep the current preflight validator, `FOR SHARE` lifecycle protection, transaction order, and unresolved-dependency error path; add no cross-transaction lock or duplicate validator.
- **Focused closure proof:** extend the existing Postgres registration coverage to preflight against UID A, complete deletion and activation of UID B at the same identity before write-time recheck, and prove the stale plan is refused with no registration operation, Workflow row, or relationship. A fresh request must validate B and then reject it if incompatible or accept it if valid. Retain the existing post-selection lifecycle-lock proof.

### FIND-TASK-002-5 — REVISED — VIOLATION: changed Rust items omit mandatory behavior, error, panic, or cancellation rustdoc

- **Discovery IDs:** `STD-TASK-002-001`
- **Violated obligation:** `AGENTS.md` §16 and `architecture/agent-rules.md` mandatory Rust documentation rule; missing rustdoc is explicitly `BLOCK_BEFORE_MERGE`.
- **Exact location:**
  - `crates/shared/wyrd-loader/src/parse.rs:236-241,360-371,403-418` and `crates/shared/wyrd-loader/src/lib.rs:223-305`;
  - `crates/shared/wyrd-client/src/workflow_loader.rs:60-177,337-387,462-500,508-524,526-615`;
  - `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:46-82,388-441`;
  - `crates/wyrd/wyrd-server/tests/pg_workflow_registration.rs:46-79,143-153,239-257,308-313,525-529`.
- **Evidence:** changed fallible helpers `materialize_inline_files` and both resolver methods lack required `# Errors`; `is_inlineable_body` is materially changed and undocumented. New panic-capable parser/loader/client/server tests and helpers lack `# Panics`, including `parse_rejects_combined_reference_forms`, `load_explicit_workflow_bundle`, `chat_text`, `edited_bundle`, `card_ref`, `operation_count`, `card_count`, and both PG journey tests. Both new test gateway `call` implementations return `Result` (and the server one can panic while serializing) without complete error/panic documentation. The changed async loader/read and server Workflow-validation paths describe errors but omit cancellation/partial-progress behavior even though they compose filesystem, HTTP, or SQL reads.
- **Observable consequence:** the candidate violates a hard repository acceptance rule and leaves maintainers without item-local contracts for failure, cancellation, partial reads, and intentional fixture panics.
- **Decision-complete minimal correction:** add only the missing substantive rustdoc to the changed items. State the actual keyed-form traversal role, error conditions, fixture invariants that panic, and that cancellation may stop after completed reads but performs no registration or durable write; for the server preflight, state that cancellation occurs before Card persistence. Do not refactor code or add lint suppression.
- **Focused closure proof:** inspect every added/materially changed Rust item against §16, then run the existing format and lint lanes plus the already named focused loader, client, and PG tests. No new test harness is needed because this is a static contract correction.

### FIND-TASK-002-6 — CONFIRMED — VIOLATION: changed imports and signatures hide module dependencies

- **Discovery IDs:** `STD-TASK-002-002`
- **Violated obligation:** `architecture/agent-rules.md` requires module-top `use` declarations and bare imported names in signatures.
- **Exact location:** `crates/shared/wyrd-loader/src/lib.rs:210-236`; `crates/shared/wyrd-client/src/workflow_loader.rs:374-376,437-453,508-510`; `crates/wyrd/wyrd-server/tests/pg_workflow_registration.rs:4-37,102-105`.
- **Evidence:** `load_explicit_workflow_bundle` contains function-scoped imports for `WorkflowAction` and `InlineableRef`; `code_review_workflow` returns `std::path::PathBuf`; the Prompt resolver signature names `skald_spec::Prompt`; and both edited-bundle helpers return `tempfile::TempDir` instead of module-top imported or aliased names.
- **Observable consequence:** the changed modules no longer expose their dependency/type ownership in one import manifest, directly violating the repository's required Rust shape.
- **Decision-complete minimal correction:** move the two local imports to the loader test module's import block; import `PathBuf` and `TempDir` in their owning modules; alias the native Skald Prompt type at module scope to distinguish it from `skald_prompt::Prompt`; use those bare names in signatures. No behavior or dependency change is needed.
- **Focused closure proof:** `git diff --check`, format, and lints pass, and direct source inspection finds no function-scoped import or qualified type in a changed signature.

## Validation outcome

- **Validated retained findings:** `FIND-TASK-002-1` through `FIND-TASK-002-6`.
- **Rejected proposals:** none.
- **Specification revision:** not required. Every correction is bounded by already-approved behavior and existing owners.
- **Validation status:** complete; the candidate requires remediation before TASK-002 can pass.
