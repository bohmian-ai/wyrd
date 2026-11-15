# Repository standards review

## Subject and result

**FAIL**: one narrowly scoped mandatory rustdoc omission (ST-001).

Root: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Base: `443a79337fe33dfbfe9ccc64e01d10369d469131`.
Candidate: `1b72ec1774f68a8eefc6f4f25304dfc5aee1248f`.
This is a fresh full-range review. No prior review conclusions or acceptance
evidence were read or carried forward. HEAD was independently checked against
the candidate. Only reports are written in the current review directory.

The scope is repository compliance, not task acceptance, alternate product
design, or a request to add error handling to the intentionally short examples.
Candidate specification revision 5 expressly permits the enumerated SDK
authoring and verdict conveniences. Current user direction permitting examples
without error-handling scaffolding takes precedence over older prescriptions.

## Authority coverage

| Changed surface | Applicable authorities read | Source coverage |
|---|---|---|
| Shared Rust authoring and input projection | Root AGENTS Code/Dependencies/Testing; agent_docs index, Rust, errors, maintainer style; wyrd-design Client model and In-memory authoring | authoring/verifier.rs, schema helpers and caller definitions; verify/mod.rs Inputs and Target::project; authoring/observe tests |
| Pure result predicates and setup prompt | Root contract/dependency/rustdoc/agent-surface rules; agent_docs errors and agent-surfaces; wyrd-design Verifier | wyrd-spec verification/execution.rs result methods and predicate test; setup-prompt.md renamed example/query references |
| PyO3, authored stubs and Python tests | Root rules; sdks/wyrd-sdk-python/AGENTS.md placement, six layers, stubs, lifetimes, errors, tests | verify/definitions.rs delegation to InputSlot; verify/mod.rs frozen result construction/getters; verify and observe stubs, public exports; Quickstart/API/YAML/authoring/local-development/ASGI journeys and provider fixtures |
| TypeScript authoring and result projection | Root rules; sdks/wyrd-sdk-ts/AGENTS.md ownership, generated declarations, types, errors, tests | definitions.ts JsonSchemaSource/slot conversion; index.ts result construction, typed reads, Execution.status/wait and Observe.verify; unit/type tests and integration consumers |
| Finance and setup examples | Root code and test rules; Rust guide; wyrd-design Artifact, Agent, Service, Verifier/Agents-tools-workspaces, In-memory authoring; bifrost-design verification-table projections | finance_service/release in three languages, api.py, YAML Cards, original/revised PDF assets, output.txt, stored query; Rust finance/setup tests and SDK actual-program tests; setup-check move and ASGI/example consumers |
| README and docs content/rendering | Root rules; docs/AGENTS.md, docs/CONTRIBUTING.md, docs/PRODUCT.md; architecture authority map; agent-surfaces generated-artifact rules | README source blocks; Introduction/Setup/Quickstart and changed connecting pages; +page server/component, FinanceFlow, WorkflowStrip, BeatLoop, lang state and existing LangTabs; highlighting/region exports, source-parity/SSR tests; llms generator and outputs |
| Manifests, locks, CI selection | Root dependency/platform/generated rules; SDK package rules; current spec's release pins | Workspace versions and internal path versions, narrow example schemars dependency, existing Zod reuse in examples, SDK package/native-platform optional versions, Cargo/uv/pnpm locks and CI detect-change fixture paths |
| Change/design/review packet | Root Changes/Git and changes/spec-driven-development.md | Candidate approved rev5, task's bounded ownership and landing design, design/mockup assets, packet dispositions; these are planning/reference material, not runtime code or architecture replacements |

Architecture authorities were used by owning section. No architecture source
was changed. The SQL directory is untouched; no new or materially changed
production signature/field uses PgPool, OperatorPool or TenantConn in the
changed source. No migration, table, policy or coordination timestamp is added.
No security evaluator, credential flow, permission model or RLS owner changes.
The evidence-domain review separately owns exact grants and policy provenance.

## Applicable rule results

| Rule | Result | Evidence |
|---|---|---|
| Durable behavior remains server-owned; clients project canonical values | PASS | InputSlot helpers derive declaration schemas; Target::project prepares inputs locally. Python delegates schema decomposition to the shared Rust owner; TS adapts structural schema sources. No server workflow/transport/storage fork |
| wyrd-spec IO/async/PyO3-free; SDK dependency boundaries | PASS | Added result methods are const local matches and one unit test. No prohibited dependency added. Example schemars is already installed/workspace-owned and necessary for the taught Rust models |
| New PyO3 remains at Python boundary; exports and authored stubs agree | PASS | New bool getters live in sdk-python/src/verify/mod.rs, all constructors route through new/plain/typed; existing VerificationResult registration/exports remain valid; verify stub declares the getters; shared schema helpers contain no Python types |
| Preserve typed errors and boundary/runtime ownership | PASS | Target::project uses existing input_invalid and WyrdError; wrappers use existing error conversion/native bridge. No new error catalog, ad hoc runtime, per-call transport or hidden constructor IO |
| First-class types and result projection | PASS | TS retains unknown at structural boundary and wire field names; status/wait/direct verify and typedResult attach predicates; predicates stay outside wire JSON. Python result is frozen; shared Rust verdict is authoritative |
| Changed-item Rust documentation | FAIL | New fields/schema fixtures/helpers/predicates have rustdoc, but the new finance_service module has neither outer nor inner module rustdoc: ST-001 |
| Journey tests at the real server/SDK seam | PASS for repository placement and meaningful scenarios | Rust actual binary tests use WyrdTestServer and local provider; Python tests import public wyrd modules with integration mark; TS uses real serverTest. Unit tests separately cover pure schema/result logic. Replacement of the older Python finance test moves its actual-program proof into the promoted Quickstart owner rather than deleting the capability's proof |
| Test changes do not suppress or weaken a gate | PASS on inspected changes | Setup selectors/assertions follow the renamed model-free example; finance proof tests the promoted PDF/three-requirement story. Existing deferred-persistence inspection retry stays test-side. No production retry or weakened permission boundary is added |
| Generated artifacts change through their owner | PASS on inspected source relationship | llms generator changes START/SUMMARY and uses existing CodeFromFile expansion; README parity uses the same source-region machinery; no wire schema/error/napi generated declaration changes occur. Authored .pyi edits are permitted |
| Docs use supported public commands, pinned releases and source-backed examples | PASS within standards scope | Setup/Quickstart use uv/cargo/pnpm and 0.4.2; finance content embeds actual files and output.txt; nested next/error structure retained. Release publication remains a delivery prerequisite, not an additional acceptance requirement |
| UI behavior can be understood without animation/color and has motion controls | PASS on source and reported checks | Figures render a finished SSR state, include text alternatives and labels, and use existing buttons, pause/replay, reduced-motion/offscreen/hidden handling. Shared language store expands through existing owner |
| Platform, package and release alignment | PASS | No target_os/cgroup/proc/sandbox policy change; SDK package and three shipped native platform versions align to 0.4.2; lock changes reflect workspace versioning and installed dependencies |
| No new public nouns, compatibility aliases or SQL/evidence duplication | PASS | Existing Card envelope/kinds/refs used in YAML and code; stored-verdicts.sql queries Bifrost summary; compact result and details remain separate projections |

## ST-001 — New finance_service module lacks rustdoc

- Classification: **VIOLATION**, explicit mandatory documentation gate.
- Location: `examples/finance-release/finance_release.rs:4`, `mod finance_service;`;
  `examples/finance-release/finance_service.rs:1` begins with an import and has no
  inner module documentation.
- Governing obligation: root AGENTS Code requires every new/changed Rust item
  to have rustdoc and states missing rustdoc blocks merge. agent_docs/rust.md
  Rustdoc applies regardless of visibility, including private items.
- Evidence: the module is introduced in this range. The binary's crate-level
  `//!` explains the executable, and the new module's structs and functions are
  individually documented, but neither is documentation attached to this
  module item. This is the live module compiled by the taught Rust program,
  not an unused fixture.
- Consequence/materiality: the new declaration owner omits the explicit
  mandatory Rust documentation requirement. There is no claimed runtime bug;
  this is retained solely because that gate is mandatory.
- Smallest correction: add concise module rustdoc describing the finance
  declaration/input owner's role, preferably an inner `//!` at the top of
  finance_service.rs. Do not add helpers, tests, public APIs or error handling.
  Deleting the module would delete required example behavior; reusing the
  already-written item docs does not attach documentation to the module.
- Correction burden: one short documentation sentence, no runtime change.
  Static inspection of the module's attached docs and the focused Rust format
  check are sufficient closure proof; no new test is needed.

## Wait inventory and hot-path applicability

No added or moved waits on PostgreSQL, Scribe, object storage or Vault occur in
the reviewed implementation. Cumulative Rust production changes are local
declaration-schema transformation, input projection and result predicates;
bindings/TS result decoration are local projection. Manifest changes outside
those owners are version alignment. The changed finance programs continue to
call existing declaration/verify owners; PDF extraction/publication occurs at
declaration through the existing Artifact owner.

VerificationEngine/hosted executors, VerificationControl::execute,
GatewayInvocation/wyrd-gateway runtime, Oracle and Scribe implementation owners
are unchanged. The server path does not gain a catalog statement, outbox wait,
storage read or Vault call. No hot-path benchmark exception is asserted, and no
full bench:l1 run was performed by this reviewer.

## Verification limits

This report independently inspects source and actual full-range diffs; it does
not substitute a green check for rule evidence. The orchestrator reports fresh
fmt:check, Python/TS lint, docs commands/links/Svelte/build/a11y checks, 21
focused docs tests and three Rust finance journeys passing. Typechecking was
still running at the last update. This reviewer launches no Cargo/PG commands
to avoid overlapping the orchestrator's shared target directory.

The full-range whitespace check reports intentional Markdown hard breaks and
an extra blank EOF in finance_service.ts. Neither changes behavior or violates
an explicitly configured lint in the available evidence; no material standards
finding is proposed for those whitespace lines.

Shared-memory search returned maintainer preferences consistent with supplied
instructions, not additional task approval or hidden acceptance requirements.

Skipped: broad runtime suites, independent benchmark/codegen/browser reruns.
Risk: final candidate acceptance requires the remaining independent reports
and validation; this report establishes standards coverage only.
