# TASK-001-R5 review subject

Repository: /Users/stevenforrester/Documents/GitHub/wyrd.
Original base: 5234961d51798581a026834cfffd5efaadc226b4.
Prior candidate / remediation base: 9be515a10607ba7d930594b1c314757ee05a0424.
Candidate: c24f5bae7d720d8a64f01d8127ec00c91367f48e (direct child).
Fix diff: `git diff 9be515a10 c24f5bae7`.
Cumulative context: `git diff 5234961d5 c24f5bae7`.
Source tracked worktree clean; change packet and unrelated positioning packet
are untracked. No CodeGraph index. Root AGENTS applies; no nested AGENTS in
these five changed-file owners. No source mutations are authorized by review.

## Authority snapshots

Approved specification is revision 5. Exact revision 4 remains archived.

- spec.md: SHA-256 454409515b38bcc50962f2edc225140800e4ea0a1ec3fe26f03042bbbca08d0d
- tasks/TASK-001-common-rust-authoring.md: SHA-256 b71ccb16efcc4bbdf28e01bad1c3f41167900c5b93f04525a2a15b5019e7ddc1
- tasks/TASK-001-R3-canonical-authoring-grammar.md: SHA-256 314abf342c8a431e8c82399703a21a694d9b40a0fa8970cabe2f98b196aac7f0
- tasks/TASK-001-R5-constructor-fields-once.md: SHA-256 bba5e86e7144c4d6e5edcc84340bd177a680537e6832c577512128d55553be7c
- review/TASK-001-review-03/TASK-001-R4-canonical-admission-and-artifact-roots.md: SHA-256 7bb67857501245e0e0532d8b5f08653447b5f933c0bb01da05222f1b3e718f33

## Scope and carried evidence

This is remediation of a human-rejected API, not a new comprehensive task
review. Check fixed-constructor-field method removal, ordinary Action/Check
returns, deleted receiver exports/conversions, remaining builders, all five
changed files, public test rewrite, rustdoc and affected callers.
FIND-9's former duplicate-setter target is superseded by revision 5; do not
reintroduce it. Carry closure of FIND-1–8 and FIND-10–11 from review-04
findings-validation.md and its invariant/domain reports, with review-03 accepted
behavior and standards coverage referenced there. Reopen only a specific changed
path or missing proof. Current task evidence includes compiler RED and focused
GREEN claims; those claims are inputs, not conclusions.

## Shared navigation map

- authoring/mod.rs: card_methods! removes name/version edits and reexport list
  drops three receiver types; retain metadata and read-only CardRef accessors.
- authoring/kinds.rs: Component and Binding fields! remove alias/ref/verifier;
  constructors and remaining spec fields retain existing Body/capture path.
- authoring/verifier.rs: constructors write directly through existing Body;
  new private InputSlot::listing reuses Body::set_list after deleting field
  setters. Action::agent/Check::agent/Check::each return Self; removed receivers
  previously forwarded the same body. Inspect every constructor and fields!
  invocation for fixed fields versus retained fields.
- Rust SDK integration/authoring.rs: explicit type assignments, mixed Check
  array without into, obsolete operand edits replaced by direct construction;
  retain native-family/operator coverage, shared Agent and mounted policy proof.
- Architecture In-memory authoring describes constructor-only fields. Affected
  consumers: existing six client tests, SDK inventory, Artifact journey, finance
  example, Rust SDK authoring reexports and packet sketches.

## Specialist applicability and roles

| Contract / owner / consumers | Selection and rationale |
|---|---|
| Constructor-only public grammar and ordinary returns; constructors -> Body -> Step/Requirement -> declaration | One fresh remediation ponytail discovery reviewer checks deletion, API semantics, changed private listing and affected consumers. No new ownership or abstraction layer: three wrappers are deleted and canonical Body reused. A separate maintainer role is unnecessary; bounded maintenance/rustdoc obligations stay in discovery and validation. |
| Capture, equality, malformed values, loader and Artifact ordering/receipts | Carry source-backed review-04 invariant and identity-publication reports, refreshed through the changed constructors and current tests. No changed graph store, capture algorithm, validator, publication or declaration owner; no fresh invariant/domain role unless tracing disproves this. |
| Failure/cancellation, persistence, credentials/tenancy and concurrency | Prior review-03 coverage and review-04 validation carry for unchanged owners. No server/SQL/transport/state/saga/executor changes; constructor body assembly is the affected producer only. No new domain/system discovery needed. |
| Repository standards and other languages | Five-file Rust/architecture removal only; existing owner rules plus changed-item rustdoc reviewed by discovery and validator. No generated artifact, manifest, PyO3/N-API or broad cross-tier change; no separate repo reviewer. Python/TS implementations remain outside this task. |
| Independent source/materiality/coverage validation | Always a separate fresh ponytail validator after discovery, even if empty proposals. It independently checks omissions, affected paths and carried proof. |

Two fresh agents in distinct roles: remediation discovery then independent
validation. No intended verdict provided. Follow-up only if a material conflict,
untraced failure or new validation finding requires separate source validation.
