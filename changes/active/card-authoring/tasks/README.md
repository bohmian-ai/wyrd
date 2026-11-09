# Current authoring task handoff

Specification revision 6 is approved by explicit maintainer direction.

Human-directed remediation: [TASK-001-R6](TASK-001-R6-evidence-and-input-bindings.md)
removes optional evidence slots, accepts bare Rust schemas and distinguishes
Verifier.add_input from Action/Check.bind_input across SDKs. It follows approved
revision 6 and the updated architecture. No migration work is in scope.

| Scope | Current owner / status |
|---|---|
| Remove duplicate constructor-field edits and the three extra receiver types; preserve other R4 fixes | [TASK-001-R5](TASK-001-R5-constructor-fields-once.md), ready for `$wyrd-implement`. |
| Common Rust grammar and subsequent R4 corrections | TASK-001-R3 and review/TASK-001-review-03/TASK-001-R4 record prior implementation and evidence; constructor-setter requirements are superseded by revision 5. |
| Python kwargs/builders, Pydantic/lifetimes/stubs and journey | REQ-009 / AC-005; projections must follow revision 6 and R6 remediation. |
| TypeScript generated options, methods, Node bindings and journey | REQ-010 / AC-006; projections must follow revision 6 and R6 remediation. |

R5 is historical human-directed remediation of the rejected API. Revision 6
supersedes its retention of optional slots and Action/Check add_input. It retains
Verifier.add_input, add_step, add_requirement,
add_component and other edits outside fixed constructor arguments. No Model
verified_by or replacement API is added.

Existing task and review records remain historical evidence, not authority to
reintroduce removed operations. Earlier specifications are preserved alongside
the current spec. Implementation and review of R5 remain pending.
