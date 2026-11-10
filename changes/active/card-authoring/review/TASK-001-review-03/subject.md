# TASK-001-R3 review under approved revision 4

Repository: /Users/stevenforrester/Documents/GitHub/wyrd.
Original implementation base: 5234961d51798581a026834cfffd5efaadc226b4.
Previously reviewed revision-3 candidate: c0a06eacdc70249e9c32e44b7670314cf1aefb3a.
Immediate revision-4 task base: a387f2a47a145a867c8a4948966fcbd136af9961.
Immutable candidate: 37b3c2f96b1bd425bf1f0923a1950d0363c40907.
Task diff: `git diff a387f2a47a145a867c8a4948966fcbd136af9961 37b3c2f96b1bd425bf1f0923a1950d0363c40907`.
Original cumulative context: original base through candidate. Ancestry is preserved. Intervening 17c4e76d0 documents prior remaining helper panic contracts; b0258d07f/a387f2a47 add/simplify the runnable finance example, now an explicit revision-4 task consumer. No prior review of those commits is implied.

Approved spec: changes/active/card-authoring/spec.md, revision 4, SHA-256 `cd1525e3e6785342871c14423b4e03dc37f20142a594051ad9ba7426f249af85`.
Current task: changes/active/card-authoring/tasks/TASK-001-R3-canonical-authoring-grammar.md, SHA-256 `314abf342c8a431e8c82399703a21a694d9b40a0fa8970cabe2f98b196aac7f0`.
Original task: changes/active/card-authoring/tasks/TASK-001-common-rust-authoring.md, SHA-256 `b71ccb16efcc4bbdf28e01bad1c3f41167900c5b93f04525a2a15b5019e7ddc1`.
Prior approval snapshot: changes/active/card-authoring/spec-revision-3.md. Prior verdicts/findings/reports and remediation tasks remain in review-01 and review-02. The original stable findings are FIND-TASK-001-1/2/3; preserve IDs for true recurrence/remaining closure, assign only genuinely new independently validated findings the next number. The human-directed revision-4 surface change is not itself a review finding.

Tracked worktree is clean; packet is locally untracked, so its hashes fix authority content. No .codegraph exists. Only root AGENTS applies to changed directories; no nested rules under client/loader/Rust SDK/examples/architecture. Implementation and packet must remain immutable throughout review. Reports only are written here; no diffs/patches/archives.

## Round scope and navigation

Review the complete R3 task diff, all approved changed behavior and affected callers, rather than restart unrelated server/domain discovery. Prior acceptance carries only where source-backed evidence remains valid; the authoring representation, validation timing and declaration dispatch are materially changed and need fresh inspection.

- authoring/mod.rs: Segment/Location and field_path; Captures; public FieldValue; Body setters/append/insert/flatten and per-field captures; Card/Graph decode/snapshot/declaration; CardDefinition/CardRef; ArtifactFile/CodeSource; fields macro. Follow all conversions and nested capture/value locations.
- authoring/kinds.rs: named kind constructors/fields; Component, Binding, Artifact local metadata, inline/named references. Compare every approved table against canonical serde fields, including flattened fields, metadata, inputs and invalid/deferred values.
- authoring/verifier.rs: Verifier, InputSlot, Step/Requirement, Action/Check; generic canonical field values and collections; complete native inventory/public surface.
- wyrd-loader lib/parse: parse_card_value -> parse_value -> pre-existing parse_single_envelope -> Spec::from_kind_and_value. Compare complete file load/resolve/validate/order path with authored snapshots and source-reading/diagnostic semantics.
- cards/handle.rs and cards/mod.rs: Declarable and From adapters; Cards::declare dispatch -> submit/saga, declare_from_path and upload_artifact. Trace native/path/Card input compatibility and current local-first publications.
- state.rs: declare/declare_with input dispatch and pull_declared; same-client/options/bundle/cancellation behavior; inspect affected native/Python/TypeScript caller signatures without implementing wrappers.
- client authoring/tests.rs: six local tests including deferred edits and YAML/code errors; SDK integration/authoring.rs: public inventory and five focused journeys including existing path-state regression.
- examples/finance-release/finance_release.rs and existing examples/tests/finance_release.rs: one writer Component, SERVICE-level three-requirement release-check, unchanged exact pass/fail output. This is separate from two-component SDK acceptance story.
- architecture/wyrd-design.md: changed In-memory authoring plus adjoining reference/registry doctrine. Packet Rust sketch has updated import in the untracked packet; inspect against approved syntax as supporting context.

Map is a starting point, not a completeness claim; reviewers expand source/caller coverage independently.

## Specialist applicability and selection

| Affected contract / owners and consumers | Selection / carry-forward rationale |
|---|---|
| Whole R3 acceptance, deferred errors, public grammar, full inventory, entry points and examples | Fresh behavior-rev. New approved semantics and broad input rewrite invalidate prior authoring-only acceptance. Covers task evidence/TDD claims independently. |
| Nested Body/Captures locations, duplicate retention/reset, source descriptors, by-value Graph snapshot and sibling writers | Fresh invariant-rev. Material new local shared-value representation and nested composition require producer-to-consumer inspection; prior stale-capture closure alone is insufficient. |
| Cross-crate/public boundary rules, all changed-item rustdoc including tests/macros, architecture and real example | Fresh repo-rev. Broad common-client/loader/SDK/example rewrite; include prior FIND-3 closure and new changed items in one pass. |
| New FieldValue/Body/Captures/card representation, fields macro and generic adapters versus canonical loader/serde owners | Fresh maintainer-rev. Substantial abstraction/ownership change; compare complete existing workflows rather than leaf-call reuse and assess concrete unearned machinery only. |
| Unified declaration dispatch, deferred source IO, Artifact preparation/scratch, interruption/retained publication, state hydration | Fresh system-rev. Actual owner invocation path changed, even though saga/upload internals remain existing. Follow realistic cancellation/failure/recovery paths. |
| Exact identity/reference intent and source/publication integrity: field captures -> Graph -> loader -> Cards -> registry/storage | Fresh domain identity-publication. Deferred identity/raw values and new normalization/dispatch can affect durable attribution or unintended publication; trace existing server owner seams without expanding server audit. |
| Credentials, RBAC/audit, tenant boundary and subject isolation | Carry source-backed review-01 security report and review-02 reconciliation for unchanged server permission/tenant/run-binding owners. Fresh behavior/system/domain reviewers must check new dispatch uses those same owners/client and no input bypass. No auth/credential/route implementation changed; no new security specialist unless source reveals materially changed authority. |
| Native execution planning, requirements barriers, bounded concurrency and null/schema admission | Carry review-01 concurrency/domain evidence and review-02 slot proof at unchanged canonical planner/engine. Fresh behavior/invariant reviewers check changed strings/fields reach those owners unchanged, and orchestrator reruns exact relevant regressions. No new executor/state change requires another concurrency specialist. |

Six fresh independent discovery roles selected; fresh ponytail-rev follows all reports. None may fill another role. Conditional follow-up only for material conflict/untraced cause, or separate source validation of a new validator finding. Validator must independently confirm specialist applicability and carried coverage. No intended verdict.

## Verification coordination and limits

Task's appended evidence is available as historical execution claims, not presumed truth or approval. Orchestrator reruns six exact local tests, public SDK test, native execution/slot regression and focused real-server/real-example journeys as appropriate. All Cargo execution is coordinated sequentially in this checkout; discovery reviewers must not run Cargo or mutate source. Ask root for necessary focused checks. Do not generate host load or require a full gate.
