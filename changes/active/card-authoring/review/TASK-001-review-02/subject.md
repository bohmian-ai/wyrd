# TASK-001 remediation review 1

Repository: /Users/stevenforrester/Documents/GitHub/wyrd.
Original base: 5234961d51798581a026834cfffd5efaadc226b4.
Prior candidate: 790da3369c1b0107765db234d72c1ae0c4680446.
Current candidate: c0a06eacdc70249e9c32e44b7670314cf1aefb3a.
Fix diff: `git diff 790da3369c1b0107765db234d72c1ae0c4680446 c0a06eacdc70249e9c32e44b7670314cf1aefb3a`.
Cumulative context: original base through current candidate. Ancestry preserved; three direct follow-on commits. Source and tracked worktree are immutable; packet remains untracked. No .codegraph directory. No nested AGENTS applies to this diff (root rules govern the shared client, Vala engine tests and Rust SDK).

Approved spec: changes/active/card-authoring/spec.md revision 3.
Original task: changes/active/card-authoring/tasks/TASK-001-common-rust-authoring.md.
Prior verdict, ledger and all eight discovery reports: ../TASK-001-review-01/.
Remediation: ../TASK-001-review-01/TASK-001-R1-current-dependencies-and-proof.md (appended implementation evidence is an assertion to verify, not review authority).

Packet SHA-256:
- spec: `73d8be31b64a88eb4d9c99517dca0f1f39bd705e60f8732c91cd47b3233d79fc`.
- original task: `b71ccb16efcc4bbdf28e01bad1c3f41167900c5b93f04525a2a15b5019e7ddc1`.
- remediation with appended evidence: `f2f8ff66706661b45d452030ec0406a952a0cd015e712505ac44aa55f879841b`.

## Scope and navigation

Review FIND-TASK-001-1/2/3 closure, complete fix diff and affected consumers. Carry unchanged source-backed acceptance from prior findings-validation.md and verdict.md; do not restart comprehensive discovery. Prior reports remain preserved.

- Binding: canonical binding plus current Trigger capture; `set_runs_on` and `into_parts`; all Agent/Service attachment and compose callers.
- AgentCall: canonical workspace plus capture per destination; workspace overwrite methods and `into_parts`; Action::agent and Check::agent callers; upstream Dependencies/Graph and downstream Cards/state publication descriptor consumption remain context.
- Local authoring tests: new replacement regression and docs; preserve active conflict, shared Artifact and snapshot behavior.
- Rust SDK integration authoring: new public_paths_author_every_native_family test, existing target, plus documentation changes to helpers/journeys. Trace canonical/public constructions and primitive/required-nullability matrix.
- vala-verify engine tests: supplied_null_follows_the_slot_schema_not_requiredness; no production engine change. Connect authored slot parity to existing conform/Scope omission and explicit-null behavior.
- Cards/state: docs-only cancellation/partial-progress changes; compare unchanged owner bodies for accuracy.
- bifrost/observe.rs, observe/mod.rs and transport/trace.rs: documentation-link repairs required by prescribed cargo doc check; inspect diff to establish no runtime change or unrelated feature.

## Specialist applicability and carried evidence

| Contract | Selection / carry-forward and source reason |
|---|---|
| Replacement capture ownership and affected callers | Fresh ponytail-rev checks closure and exact fix. New fields partition existing local value capture; no shared runtime state, graph validation algorithm, lifecycle or substantial owner replacement. Prior invariant/maintainer reports supply full surrounding owner traces. No separate invariant/maintainer discovery selected absent a material unresolved question. |
| Public primitive and slot proof | Fresh ponytail-rev checks named acceptance gaps, new SDK test and canonical engine test. No new production schema/validation/public type implementation; prior behavior/concurrency coverage remains available. |
| Mandatory rustdoc and changed surfaces | Fresh ponytail-rev checks cumulative documentation closure and all fix-diff items. Prior standards report maps existing authorities; added Vala surface is one inline pure-engine test governed by root/Rust testing rules. No broad cross-tier rule change requiring a fresh repo specialist. |
| Publication failure, scratch lifetime, retry and hydration | Carry prior system/persistence evidence: Cards/state/saga/upload and Graph production bodies unchanged. Slot descriptor producer correction is checked before downstream unchanged publication loop. Documentation accuracy remains in closure scope. No fresh system or persistence specialist needed. |
| Security, permissions, tenancy and exact subject | Carry prior security report: no auth, reference-kind semantics, transport, registry handler, credential or bound-verifier implementation changes. Local current-slot capture correction preserves these consumers. No new domain security review needed. |
| Scheduling/concurrency | Carry prior concurrency report and recorded exact regression/dependent journey. Production planner/engine unchanged; added engine test concerns schema admission only. No new concurrency/domain reviewer needed. |
| Related documentation link repair | Include in fix review because cargo doc closure required it; literal rustdoc changes only, no runtime boundary. |

One fresh ponytail-rev is selected under remediation topology. It must independently validate this applicability/carry-forward record, inspect source and prior proof, and request any required additional specialist. No intended verdict. If it discovers a new blocker, obtain separate focused source validation before retention. A focused follow-up is conditional on a material conflict or new failing path, not automatic.

## Verification coordination

Orchestrator reruns focused local authoring tests, public SDK primitive proof, native supplied-null and omission tests plus focused documentation/format checks as appropriate. No simultaneous Cargo execution. Full journeys and broad aggregates are carried from prior review, not repeated absent a materially changed boundary. Reviewer writes findings-validation.md only, never source, tests, authorities, packet or other reports. No merge/commit/push/deploy.
