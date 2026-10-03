# Registry durability and concurrency domain review

## Result and immutable subject

**PASS.** No material proposed findings in this domain.

Reviewed cumulative base `0569b79702218600c4f9790f45cc03100d5c6f1c` to candidate
`8a8282331042c3cc7610478b3d46583dbbf121f8`, using only the candidate archive at
`/tmp/wyrd-task002-r3-8a8282331` and `/tmp/wyrd-task002-r3.diff`. The live dirty
follow-up and its running lanes were excluded. This report does not approve
that follow-up. Approved authority is Revision 12, the original TASK-002,
replacement TASK-002-cleanup, and R1/R2 remediation under that authority.

## Boundary, authority and source coverage

The review traced authored reference and version intent through server
preflight, dependency lifecycle locking, registration persistence, audit,
idempotency and post-commit publication. It expanded into binding and Drift
consumers of the effective-body owner and the existing client graph's Runtime
and Service Bundle consumers. This is the registry domain review, not the
full-task or repository-standards verdict.

Governing authority: AGENTS.md §§2–3, 5, 9, 11–12; architecture/agent-rules.md
transaction ownership, RLS, effective-value and audit rules; the reference
router and its architecture-constraints, patterns, spec-driven-development,
maintainer-style, testing-workflows and errors guidance; wyrd-design.md
Workflow, reference-slot, registration, blob lifecycle, idempotency and version
intent contracts; wyrd-doctrine.mdx declarative Card/runtime separation; and
wyrd-security-posture.md tenant/audit boundaries. Applied Revision 12
resolution/ownership/preservation, REQ-014/028/056–059 and AC-002/006 to this
task's scope. No Bifrost publication mechanics are materially changed here.
No CodeGraph directory exists in the established subject; navigation used rg,
the cumulative diff and snapshot source. Server/SQL manifests and mise lanes
were inspected.

| Obligation / boundary | Implementation and consumer evidence | Proof evidence inspected | Result |
|---|---|---|---|
| Preserve None / Scope / Pin authored version intents | `resolve.rs:430–465` validates Workflow holders projected by existing `graph_ready_submissions`; original submissions continue through `hash_request`, `RegistrationPlan`, `find_submission`, `persist_node` and `resolve_existing` | Version-intent branch of `registers_only_valid_explicit_workflow_graphs` checks omitted → `0.1.0`, Scope `2` → `2.0.0`, exact reload and invalid binding refusals | PASS |
| Separate sibling and external body provenance | `EffectiveSpecs::{new,load,body}` uses distinct maps and matches `Ref::Sibling` / `Ref::Ref`; canonical visitor and `InlineableRef::to_durable` retain the discriminator | Collision registration checks incompatible registered Agent cannot be validated using a submitted sibling at its identity; bind-source unit assertion retained | PASS |
| Validate declarative resolved graph before mutations | `EffectiveSpecs::resolve` invokes bindings, baselines and Workflow validation; Workflow dependencies include transitive registered Agent→Prompt bodies; `Workflow::validate_card_bodies` uses existing Skald lowering/binder validation and intentionally unbinds execution tools | Registration journey covers binding, output, cyclic graph and route refusal for sibling/external sources, no operation/Card writes, built-in tool declarations and Prompt refs | PASS |
| Bind the exact UID whose body preflight validated | `resolve_external` returns saved pairs; `RegistrationWriter::write:1090` passes them unchanged to `recheck_active_card_refs`; SQL requires identity, expected UID and Active state | `refuses_stale_preflight_after_dependency_replacement` establishes replacement after preflight and before recheck, checks refusal and no operation/Workflow/replacement inbound relationship | PASS |
| Exclude lifecycle changes through registration commit | Recheck holds `FOR SHARE`; public delete takes `FOR UPDATE`, checks inbound refs and updates lifecycle in caller transaction; expected-UID selection rejects replacement or disappearance | `relationship_recheck_blocks_target_lifecycle_race` asserts competing update SQLSTATE `55P03`, then progress after lock release | PASS |
| One atomic registration / audit / idempotency boundary | Writer appends allowed audit before dependency recheck/operation reservation; `persist_node` writes nodes, edges, manifests, principals and frozen bindings through the same borrowed TenantConn; operation seed and transaction commit finish together | Existing cards integration target plus source of rollback/replay and route proof; new negatives assert absence of durable registration state | PASS |
| Retain replay and version-line serialization | Lost idempotency race drops writer transaction before bounded replay polling; request hash uses authored submissions; `persist_node` obtains existing `lock_version_line` before `resolve_existing` Pin/None/Scope branches | Existing registration tests and source ownership; R2 evidence names cards integration and tx-coupling checks | PASS |
| Preserve post-commit blob/upload recovery | `initialize_uploads` remains after writer commit; durable seed persists replay inputs, bounded task owner initializes uploads, completion/activation and reconciliation retain existing owners | No new recovery machinery or transaction relocation in cumulative diff; registration assertions reload Active Cards | PASS |
| Preserve sibling consumers and exact client graph reads | Binding Verifier/Trigger/Operator and Drift baseline calls now pass provenance to `EffectiveSpecs::load`; BindingProjector still freezes in registration transaction. `GraphScope::Runtime` follows Agent/Prompt refs without inventories; Bundle retains complete edges/inventories; WorkflowBodies only consumes Active exact bodies | Existing shared regression lane and per-language journeys are recorded; inspected actual server exact/UID/pinning/inactive assertions and shared graph/body source | PASS |

## Prior closure verified from source

**FIND-TASK-002-1:** closed at the effective-body producer. The sibling cache
cannot satisfy external lookup. Binding and baseline sibling consumers share
this corrected owner rather than relying on Workflow-only guards. External
UID binding and sibling UID binding remain separate in the canonical visitor.

**FIND-TASK-002-4:** closed. The saved preflight `(CardRef, CardUid)` pairs
reach the SQL lock fence; they are not reduced to identity-only refs. The
selected UID cannot be replaced with another Active UID at the same identity.
Locks survive every node/binding/relationship write until writer commit or
rollback. The deliberate administrative interleaving in the stale-preflight
test exercises this exact fence, while public lifecycle deletion additionally
protects inbound graph targets.

**FIND-TASK-002-10:** closed. Workflow preflight converts transient graph-ready
holders, not original None/Scope metadata. The existing helper clones
submissions and supplies the graph-only placeholder; sibling identity cache
still admits only actual exact submissions. Hashing/replay and write-time
version allocation consume original metadata. The new test asserts both valid
intents and rejects the resolved invalid binding before bookkeeping or Card
writes; it then reloads the real allocated exact version.

**FIND-TASK-002-9 (registry owner slice):** closed. `EffectiveSpecs::resolve`
owns effective-spec orchestration and `RegistrationWriter { state, caller }`
owns the write. The move preserves audit, UID fence, reservation, topo binding,
version-line lock, projection, commit and upload ordering; `RegistrationPlan`
remains a pure value. No new registry store, durable graph, transport, cache or
second writer was added. Existing owners suffice for the reviewed behavior.

The separate new per-language Native execution closure belongs to other
reviewers; this review inspected the durable graph and registered load proof
without substituting private executor tests for those required public journeys.

## Failure paths and rejected hypotheses

Missing dependencies and declarative validation failure occur before writer
transaction. Replacement/disappearance at recheck, audit append failure,
version/spec conflict and node/binding/relationship failure unwind the
uncommitted TenantConn. A lost idempotency race discards its audit and mutations,
then the public owner records allowed outcomes separately when no registration
committed. Post-commit upload/blob effects remain recoverable using operation
seed, pending lifecycle and existing reconciliation; they were not moved into
preflight or represented as atomic object-store writes.

Investigated whether a registered Agent's transitive Prompt could silently
resolve a replacement UID before preflight. Normal public deletion cannot
create that state: the retained Active Agent's outbound relationship blocks
deleting its Prompt. Arbitrary administrative row/spec corruption is outside
the task; the retained fault-injection test demonstrates the actual changed
cross-transaction UID fence. No speculative corruption-hardening finding is
proposed. Authored UID versus server resolution is existing registration
semantics; this task did not introduce a new registration UID-selector contract.

## Verification limits

Static cumulative diff, full relevant symbol bodies, callers and actual test
assertions were inspected. No build, database lane or runtime reproduction was
started by this reviewer, to avoid competing with root verification. Committed
R2 implementation evidence reports the exact three Workflow registration tests,
SQL lifecycle lock test, shared/cards integration and tx-coupling checks PASS;
those are reported executions, not independently rerun raw logs. The cards
integration aggregate does not select `pg_workflow_registration`, so the named
exact executions remain required evidence. Its known workspace-hack failure is
not waived by this domain PASS and remains for the orchestrator's final proof
assessment.

The reviewed candidate source was not modified. Only this assigned report was
written. Proposed finding ledger: **empty**. Overall domain result: **PASS**.
