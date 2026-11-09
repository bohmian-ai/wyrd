# TASK-001 first review

Repository: /Users/stevenforrester/Documents/GitHub/wyrd
Base: 5234961d51798581a026834cfffd5efaadc226b4
Candidate: 790da3369c1b0107765db234d72c1ae0c4680446
Range: `git diff 5234961d51798581a026834cfffd5efaadc226b4...790da3369c1b0107765db234d72c1ae0c4680446`
Approved authority: changes/active/card-authoring/spec.md revision 3.
Original task: changes/active/card-authoring/tasks/TASK-001-common-rust-authoring.md.
First review; no prior acceptance evidence carried forward. Packet is untracked locally; implementation range is committed. Preserve packet and reviewed source unchanged. No .codegraph directory is present.

Packet SHA-256 at review start:
- spec.md: `73d8be31b64a88eb4d9c99517dca0f1f39bd705e60f8732c91cd47b3233d79fc`
- TASK-001-common-rust-authoring.md: `b71ccb16efcc4bbdf28e01bad1c3f41167900c5b93f04525a2a15b5019e7ddc1`

## Navigation map

- authoring/mod.rs: Definition, Dependencies, Graph, CardDefinition::declaration, Linked, Artifact, Binding, Service and Agent constructors/editors; produces canonical references and loader AuthoredCards.
- authoring/verifier.rs: Step, Requirement, Action, Check, AgentCall, CodeCall; canonical conversion and captured dependencies; consumed by Verifier and server execution.
- wyrd-loader/src/lib.rs::declaration_input_from_cards: new in-memory entry point; compare load/build_declaration_input, validate::validate_tree, order::order_cards/submission_for and reference inventory.
- wyrd-client/src/cards/handle.rs::declare_authored: existing upload_artifact then declare; trace ArtifactUpload::prepare and declaration engine, request capture and errors.
- wyrd-client/src/state.rs::declare_authored(_with), pull_declared: shares declare_with pull/hydrate tail.
- wyrd-spec/src/card/verifier/reference.rs: input/step_output/at helpers; canonical parser and consumers.
- wyrd-sdk-rust/src/lib.rs: public exports; integration/authoring.rs: four real-server journeys and shared local provider/story.
- authoring/tests.rs: three local tests including full primitive inventory and file parity; fixtures/cards/card_authoring: YAML, PDF and Python source.
- architecture/wyrd-design.md: new in-memory authoring section; compare existing client, Verifier, registry lifecycle and file-authoring contracts.

This map is a starting point, not complete coverage. Every reviewer expands it against actual source and callers.

## Specialist applicability

| Contract / invariant | Owners, producers and consumers | Selected review and reason |
|---|---|---|
| All TASK-001 obligations and non-goals | authoring -> loader -> Cards/state -> SDK execution | behavior-rev: comprehensive adversarial acceptance |
| Captured children, identity deduplication, sibling/external intent, canonical graph validation | Dependencies/Graph/Linked -> loader reference inventory and ordering -> registry | invariant-rev: new recursive value capture and graph lowering |
| Cross-tier rules, rustdoc, fixtures and architecture | spec/client/loader/SDK/test exports | repo-rev: changed public contracts and multiple crate owners |
| Ownership, reuse versus duplicate workflow, abstraction surface | authoring Graph and wrappers, existing loader and Cards/state | maintainer-rev: substantial new authoring module; independently compare existing owners |
| Publication failure, cancellation, scratch lifetime, state pull and recovery | Cards::declare_authored -> upload_artifact -> declare -> state hydration | system-rev: explicit retained-publication contract and multi-call runtime path |
| Permissions, tenant binding, exact-subject component boundaries | authenticated Cards/state -> existing registry permission checks; run binding lookup | domain security: preserved security/tenancy and refusal obligations |
| Immutable versions, independent Artifact source maps, durable publication and parity | Artifact preparation -> registry/storage -> graph declarations | domain persistence: explicit staged-persistence obligations |
| Explicit/inferred dependency stages, shared executor concurrency | authored Step/Reference -> VerifierSpec::plan -> vala engine | domain concurrency: task requires existing concurrency and dependent outputs |

All specialist families requested by the user are selected. Six discovery agents can run concurrently; remaining domain reviews start when slots free. A fresh independent ponytail-rev validates all reports afterward. No agent serves multiple roles. Follow-up is conditional on a material conflict or unresolved path.

## Verification evidence and limits

Task Evidence section records RED/GREEN and final commands/results; reviewers inspect that evidence directly, without relying on a completion summary. Available commands include three local authoring tests, four SDK journeys, concurrency regression, fmt/lints/check:deps/check:docs/docs:check. Orchestrator coordinates any rerun; no simultaneous Cargo commands in this checkout. Reviewers must not change implementation, tests, fixtures, authorities or packet. Reports only go into this directory. No merge, commit or push authorized.
