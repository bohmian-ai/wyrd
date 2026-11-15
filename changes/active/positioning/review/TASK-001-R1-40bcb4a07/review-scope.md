# TASK-001 first review: immutable subject and coverage

- Root: `/Users/stevenforrester/Documents/GitHub/wyrd`.
- Base: `443a79337fe33dfbfe9ccc64e01d10369d469131`.
- Candidate: `40bcb4a072dcbb33bee734f9b1c3ff0f63cf0718` (`docs`).
- Range: `git diff 443a79337...40bcb4a07`; all 130 changed paths included, including packet/design assets and the older packet disposition.
- Approved authority: candidate `changes/active/positioning/spec.md`, SPEC-positioning rev 4; original candidate `changes/active/positioning/tasks/TASK-001-positioning-and-finance-onboarding.md` and its approved landing design. Read these with `git show 40bcb4a07:<path>` to freeze inputs.
- Round: first independent task review; no carried-forward acceptance evidence.
- Dirty files at entry: positioning design notes, verification-foundations, and TASK-001 through TASK-004. These user edits are outside the committed subject; inspect differences for context only, do not overwrite or treat them as candidate implementation. The task's current uncommitted addition foregrounds verdict-before-response; it does not amend the spec's PDF or public API constraints.
- CodeGraph: `.codegraph/` absent at entry; use Git and rg navigation.
- Review artifacts only may be written under this new directory. No source edits, commits, or pushes.

## Shared starting navigation map

| Changed surface / contract | Owners, producers, consumers and proof |
|---|---|
| Positioning and first-reader workflow | README; docs PRODUCT; Introduction, Setup, Quickstart; docs routes +page.server.ts and +page.svelte; CodeFromFile/mdsvex highlight and generated llms indexes; docs readme, indexes, shell-links, page-text tests; docs CONTRIBUTING/AGENTS |
| Finance authoring, policy selection and release decision | examples/finance-release/finance_service.{py,rs,ts}, finance_release.{py,rs,ts}, api.py; WyrdState/verify, Cards declaration, Agent workspace Artifact/file grants; examples/tests/finance_release.rs; SDK quickstart/finance/ASGI/authoring journeys; Author in code embedded consumers |
| Typed authoring and input projection | wyrd-client authoring/verifier.rs InputSlot helpers and verify/mod.rs Inputs/Target::project; Python src/verify definitions/mod + authored stubs; TS src/definitions.ts and src/index.ts; authoring, observe and integration tests |
| Result predicates | wyrd-spec verification/execution.rs VerificationResult; Python result wrapper/stubs; TS result construction and type tests; SDK consumers/examples |
| Motion and source rendering | FinanceFlow.svelte, WorkflowStrip.svelte, beat-loop.svelte.ts; existing homepage/CoverScene/source highlighting owners; shell-links tests; approved landing-animation and direction-B/workflow mockups |
| Coupled example paths and package surfaces | setup-check rename, CLI setup-prompt, package descriptions/manifests/locks, local-development/ASGI/declaration tests, .github detect-changes test, docs command checker/generator |

Every reviewer must expand this map from actual source and inspect all applicable authorities, surrounding callers, tests and full function bodies for its scope. Do not consume the user's implementation summary as acceptance evidence. Candidate task Execution Evidence is available verification history, not independent proof.

## Specialist applicability recorded before discovery

| Contract/invariant | Selected role and reason / omission evidence |
|---|---|
| Complete REQ-001–008, INV-001–006, AC-001–007 acceptance, negative flows and native verdict/error semantics | behavior-rev: required first review; full range and supported reader-to-result paths |
| Rust shared client/spec, PyO3 + stubs, TS wrappers/types, docs and tooling rules | repo-rev: broad cross-tier coverage required by actual diff across crates/shared, wyrd-spec, three SDKs, docs and examples; mandatory rustdoc and existing test integrity included |
| Schema-to-slot public authoring/projection, result wrapper shapes and shared motion owner | maintainer-rev: substantial new InputSlot model/fields abstraction, TS schema interpretation, Python model handling, shared BeatLoop and new components require existing-owner comparison and caller tracing |
| Policy source, selected Artifact versions, workspace file access, model-provider exposure | domain-rev (evidence): selected for required end-to-end PDF extraction and immutable provenance contract, exact declarations and permissions. The policy asset replacement and public inspection proof materially affect this trust/evidence boundary even though storage engine files are unchanged |
| Shared mutable state and lifecycle | invariant-rev omitted: changed client projection is local value transformation; new BeatLoop is component-local presentation lifecycle; no new server cache, registry, durable coordination or shared mutable lifecycle. behavior-rev covers local lifecycle, error semantics and all producer-consumer paths; evidence domain reviewer covers version attribution |
| Process/service failure propagation and recovery | system-rev omitted: diff has no server executor, process supervisor, deploy, transport or recovery implementation; shared client input projection affects request preparation only. behavior-rev traces native request/execution errors, release/hold and component cleanup; escalate if actual callers reveal material service recovery changes |
| Security/RBAC, tenancy, persistent engine concurrency | separate specialists omitted: no permission evaluator, RLS/SQL, credential, storage engine or concurrency-owner changes in full diff. evidence specialist traces policy grants/provider exposure and existing storage boundary; escalate if materially changed behavior is found |

Discovery reports: task-review-behavior.md, standards-review.md, maintainer-review.md, domain-review-evidence.md. A fresh ponytail-rev must validate all reports and applicability after discovery. Follow-up only for material conflict/untraced cause; separate validation required for any new validator finding.

## Verification limits at start

Candidate task records historical execution evidence, including earlier candidate IDs. Review must distinguish that evidence from the final candidate and missing checks. No implementation summary claims are assumed verified. No full aggregate gate or hot-path benchmark is assumed passed. Any hot-path impact requires sanctioned-wait inventory and full valid comparative benchmark under personal repository rules before PASS.
