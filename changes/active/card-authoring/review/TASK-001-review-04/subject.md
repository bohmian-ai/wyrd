# TASK-001-R4 remediation review

Repository: /Users/stevenforrester/Documents/GitHub/wyrd.
Original base: 5234961d51798581a026834cfffd5efaadc226b4.
R3 base: a387f2a47a145a867c8a4948966fcbd136af9961.
Prior reviewed candidate / current remediation base: 37b3c2f96b1bd425bf1f0923a1950d0363c40907.
Immutable current candidate: 9be515a10607ba7d930594b1c314757ee05a0424.
Fix diff: `git diff 37b3c2f96b1bd425bf1f0923a1950d0363c40907 9be515a10607ba7d930594b1c314757ee05a0424`.
Cumulative original/R3 context remains available. Two direct follow-on commits, no rebase. Tracked worktree clean; packet locally untracked. No CodeGraph index. Root AGENTS applies to all changed directories; no nested rule in this write set.

Approved spec: changes/active/card-authoring/spec.md revision 4; SHA-256 cd1525e3e6785342871c14423b4e03dc37f20142a594051ad9ba7426f249af85.
R3 task: changes/active/card-authoring/tasks/TASK-001-R3-canonical-authoring-grammar.md; SHA-256 314abf342c8a431e8c82399703a21a694d9b40a0fa8970cabe2f98b196aac7f0.
Current remediation: ../TASK-001-review-03/TASK-001-R4-canonical-admission-and-artifact-roots.md; SHA-256 7bb67857501245e0e0532d8b5f08653447b5f933c0bb01da05222f1b3e718f33.
Original TASK-001 and revision-3 snapshot/prior verdicts/remediations remain preserved. Review-03 verdict, findings-validation, six discovery reports and follow-up supply current acceptance context. Appended implementation evidence is historical claims, not review authority.

## Scope and carry-forward

Check FIND-TASK-001-4 through -11 closure, full current fix diff and affected producer/consumer invariants. Original FIND-1/2/3 are already closed; do not reopen without material new source evidence. Assign a next unused FIND ID only to independently validated genuinely new defects. Do not restart discovery over the full grammar or unrelated server code.

Carry source-backed review-03 accepted rows for unchanged canonical grammar/loader serde, path/native declaration dispatch, same-client state pull/hydration, auth/tenancy/subject binding, native planning/concurrency/null semantics, two-component case matrix and runnable finance behavior. Canonical equality, malformed staging, Artifact graph/root/selection and new editable return conversions are rechecked because their source changed. New docs and exact IO assertion are closure obligations. Architecture prose is checked against these changes, not treated as authority to change approved behavior.

## Shared navigation map

- authoring/mod.rs: Body malformed preservation; Card now owns optional source/extract; Captures one Card list; Card::decode/local; Graph::absorb and same_content canonical comparison; Graph::declaration partition/admission; Declaration.input Option. Follow completed snapshots into native inputs/LocalArtifact descriptors.
- authoring/kinds.rs: Artifact now uses card_kind root adapters, local descriptor fields live on Card, Artifact.file uses converted reference value. Follow ordinary/local/registered selections, metadata, nested schema captures and root admission.
- authoring/verifier.rs: Action.from; AgentAction/AgentCheck/EachCheck returned by existing constructors, field setters and Into conversions into Action/Check/FieldValue. Inspect actual Step/Requirement and homogeneous/mixed collection callers, discriminants/capture preservation without requiring a new API grammar.
- cards/handle.rs: Cards::declare consumes optional graph input or final publication receipt; upload_artifact factors private publish_artifact while preserving public CardRef result, existing preparation/scratch/saga and errors. State code is unchanged and remains an affected receipt/pull consumer.
- authoring/tests.rs: existing six tests extended for all retained finding branches; SDK integration/authoring.rs: public family proof/Into calls, local Artifact root receipt/files and nested schema->payload->Audit registration. No new test binary or dependency.
- finance_release.rs: documentation-only main correction; behavior evidence carried from review-03. Kinds Model/Data/Experiment docs, submit/publish async contracts and exact LoaderIO diagnostic assertion close finite mandatory gates.
- architecture/wyrd-design.md: canonical equality, local references and root receipt updates; compare approved spec and existing registry/reference doctrine.

This is starting navigation, not complete coverage. Expand only for affected owners/consumers or a specific proof gap, recording why accepted coverage is reopened.

## Specialist applicability

| Contract | Selection or evidence for carry-forward |
|---|---|
| Graph identity/equality, current captures and new unified source ownership | Fresh invariant-rev: material representation/partition change from separate local list to one Card list affects shared traversal, deduplication, current references, malformed input and descriptor admission. |
| Artifact root/nested publication, actual receipt, exact references, source/scratch and publication failure | Fresh domain identity-publication: changed root/optional-input and receipt-preserving publication paths require sensitive boundary coverage; inspect only approved demonstrated local-to-local and root flows, not new arbitrary publication contracts. |
| Overall closure, approved field edits and public return conversions, finite rustdoc and proof gates | Fresh ponytail-rev after discovery. Existing Body/staging owners are retained; three thin editable return types implement the already approved representation correction rather than introduce a new owner. Validator checks real changed conversions/callers plus prior maintainer coverage, including any material API regression. No separate maintainer/repo/behavior discovery is required without an unresolved question. |
| Runtime resilience | Prior review-03 system/saga/upload scratch evidence carries at unchanged lower owners. Fresh domain review traces the factored publish_artifact body and receipt/error/cancellation branch, which remains SDK-local and preserves existing server workflow. No new process/shared-service lifecycle requires fresh system discovery. |
| Security/tenancy and exact component subjects | Prior independently validated source coverage carries: no principal, permission, tenant selector, transport or server/subject change. Fresh domain/validator confirm new root dispatch uses the same Cards engine and authenticated submit. |
| Concurrency and input-schema execution | Prior native planner/executor/nullability source+tests carry; no production execution implementation changed. New local/field conversion regressions must still reach those same native contracts. |
| Broader rule coverage | Prior standards authority matrix remains applicable to the same Rust/example/architecture owners. Fresh validator checks all changed items and finite FIND-10/11 closure; no new tier/language/config/generated surface warrants another broad standards review. |

Two fresh discovery roles selected, then one fresh independent ponytail-rev. Validator must independently verify this selection/omission/carry-forward record. Follow-up only for material conflict, an unresolved failing path or separate source validation of a new validator-origin blocker. No intended verdict.

## Verification discipline

Orchestrator coordinates six exact local tests, the public SDK test, the one Artifact journey and focused format/Clippy/docs checks as appropriate. No simultaneous Cargo commands; reviewers request checks rather than run Cargo. No source or packet changes, deliberate mutation or new harness during review. Reports only in this new directory. No full journey aggregate, merge/commit/push/deploy.
