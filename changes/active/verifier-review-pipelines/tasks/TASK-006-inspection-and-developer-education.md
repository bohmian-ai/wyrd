---
id: TASK-006
spec: SPEC-verifier-review-pipelines
spec_revision: 24
status: proposed
contract_status: complete
supplement: SUPPLEMENT-verifier-execution-state-and-durability@2
depends_on: [TASK-001-R5, TASK-002, TASK-003, TASK-004, TASK-005]
req: [REQ-033, REQ-044, REQ-050, REQ-052, REQ-053, REQ-054, REQ-055, REQ-056, REQ-057, REQ-058, REQ-059]
inv: [INV-001, INV-002, INV-010, INV-012, INV-013, INV-014, INV-015, INV-016, INV-017, INV-018, INV-019]
ac: [AC-021, AC-022, AC-037, AC-042, AC-046, AC-047, AC-048, AC-049, AC-050, AC-051]
---

# Execution inspection and developer documentation

## Outcome and Value

Implement the ongoing execution UI and deliver the required in-depth docsite with three runnable code-review examples. The examples and cross-surface integration are verification of Task 001 and Tasks 002–005; UI behavior is new implementation in this task.

## What Changes and What Must Stay True

[Replacement TASK-004](TASK-004-installable-github-app.md) owns the complete
install-Wyrd connection UI and server workflow. This task consumes it and teaches
the combined experience; it must not introduce a second connection screen,
OAuth implementation, tenant association or customer App-registration procedure.
SaaS teams install the existing Wyrd App and select repositories. Self-hosted
operators configure their own App once; their teams then follow the same steps.
Separate the short team guide from operator registration, exact callbacks,
secret configuration and rotation. Neither guide asks a tenant to upload keys.

Task 001 owns execution state and recovery; the UI uses the same existing
APIs for active and terminal work. Teach direct/submit entry paths, in-memory
state, shared-writer WAL acknowledgement, authoritative recovery, queue lease
renewal versus the execution deadline, and independent delivery retries. No UI
backend selection, active-to-archive switch or source-specific lookup is added.

| Implement | Required implementation logic |
|---|---|
| **Execution view** | Use existing authenticated UI/BFF/client to show paginated steps, child/iteration/attempt identity, requirements, partial/failed/skipped states, speculative pending/retained/discarded/blocked disposition, reasons, limits, projection progress and content availability. Refresh/restart must preserve accepted work. |
| **Publication inspection** | Browse actual manifest paths/directories and authorized content, with bounded safe Markdown/text/JSON previews and downloads. Show producer/original-source lineage and explicit lag/denial. Pending/discarded content requires verifier:read and never becomes ordinary downstream evidence; no raw object-store or anonymous source path. |
| **Bounded UI reads** | Fetch list pages separately from file bodies, preview/download on demand, cancel obsolete requests and preserve stable pagination cuts. No full repository/tree download to paint a list, per-item query loop or UI-owned execution state machine. |
| **Canonical examples** | Promote the three organized alternatives (repeated sweeps, conditional specialists, speculative review) against shipped schemas/runtime. Keep inline Prompts, workspace-based file reading, one pull_request extraction and ordinary named final report requirement. Add explanatory array-fanout/sequential-refinement examples; no invented platform report/finding schema or scheduler. |
| **Dedicated docsite deliverable** | Write progressive setup/authoring guides, concepts, complete field/error/permission reference and troubleshooting. Teach install/select/connect onboarding from either entry point, team definitions reused by two repositories, trusted target/local TOML, installation/allowlist/publisher authority, exact PR bundle, output/publications, runtime file/directory/JSON publication, identities/retries, joins/loops/speculation, limits/recovery, API/UI/Bifrost debugging, storage lifecycle/visibility and independent check retry. |
| **Tested teaching materials** | Embed existing journey-tested source via CodeFromFile/LangTabs, equivalent Rust/Python/TS/CLI/MCP workflows, accessible non-Mermaid full control-flow diagram plus text alternative and updated library README/API docs. A fresh reader must complete setup, customize/run a verifier and diagnose a documented partial failure from rendered guides alone. |

## Approach

1. Add execution timeline/read models through existing public APIs and authenticated UI/BFF.
2. Add manifest browsing, safe bounded previews/downloads and explicit authorization/lag states.
3. Verify all three example workflows and combined surface behavior using Task 001 and Tasks 002–005 shipped contracts.
4. Write dedicated progressive guides/reference/troubleshooting and embed tested files/accessible diagram.
5. Run browser, example, rendered-docsite/link/a11y and fresh-reader proof; resolve defects at their original owner.

## Ordered Implementation Scenarios

These are user journeys. Browser safety/availability/read-bound cases are part
of inspection; documentation and already-shipped example verification do not
require manufactured runtime RED tests.

### Scenario 1 — Inspect a running or failed verification and its publications

**Behavior.** An engineer opens an execution, follows concurrent items/iterations,
finds a partial or failed producer, browses its publications, previews a report
and downloads the exact retained content. Reload, completion and server restart
preserve accepted identity and page cuts. Authorized debugging of discarded work
never makes it ordinary downstream evidence.

| Test case within this journey | Expected observation and focused proof |
| --- | --- |
| Running/settled/failed/skipped items, attempts, requirements and speculative disposition | Honest progress/reasons/provenance and retained publications across refresh. [Progress cases](#proof-1). |
| Nested tree, text/Markdown/JSON, malicious markup, denied/foreign/discarded content | Safe rendering, exact authorized bytes/downloads, no scripts or unauthorized disclosure. [Content cases](#proof-2). |
| Large bounded pages, opening one body, navigating away and entries arriving during paging | Only requested metadata/preview loads, obsolete requests cancel and original cut membership is stable. [Read-bound cases](#proof-3). |
| Success/error/cancel/timeout, source Card deletion, delayed/unavailable accepted state and independent Check updates after restart | Same identities/grants/exact content; explicit unavailability rather than deletion or a UI backend switch. [Recovery cases](#lifecycle-proof). |

**RED.** Extend the [production BFF/Playwright inspection journey](#proof-1) against
a real Wyrd server, with the content/read-bound/recovery cases above. Assert what
the engineer sees and the network requests made. The ongoing execution and
publication inspection views do not yet complete this journey.

**GREEN.** Use existing authenticated UI/BFF/client APIs for paginated progress,
manifest browsing, safe lazy previews/downloads and typed problems. Task 001 owns
state/recovery and Tasks 002–005 own their feature data; return missing backend
behavior to those owners rather than implementing it in the UI.

**REFACTOR.** Reuse current components, pagination and cancellation. No UI-owned
scheduler, recovery state, private content route, archive detection or source selector.

### Scenario 2 — Follow the documentation to configure, run and troubleshoot verification

**Behavior.** A new team member, using only rendered documentation, installs
the existing Wyrd App on GitHub, selects repositories, signs into Wyrd and
confirms the tenant connection. They adopt or customize a reusable Verifier,
open a PR, find its report
and GitHub Check, and diagnoses a documented partial failure. A team can reuse
exact Verifier versions across two applications. Teach the three canonical
configurations plus array fanout/sequential refinement with the shipped contracts.
A self-hosted operator follows the separate App configuration guide, then the
same team journey succeeds. Customer onboarding must not require creating an
App, entering App secrets, manually relaying installation IDs/state, or composing
administration requests. GitHub-first and Wyrd-first paths use TASK-004's same
verified connection and normal existing Wyrd onboarding.

**RED.** TDD is not applicable to documentation or verification of already-shipped
behavior. Use the [canonical examples and combined-surface regressions](#example-and-combined-surface-proof-verification-only)
and the [rendered-guide/fresh-reader proof](#dedicated-docsite-proof-static-and-fresh-reader).
Missing topics, untested snippets or an unsuccessful reader exercise block this
journey; do not invent another runtime feature or manufacture a failing test.

**GREEN.** Deliver progressive setup/authoring guides, concepts, full field/error/
permission reference and troubleshooting. Embed tested source using existing
CodeFromFile/LangTabs and the accessible full diagram/text alternative. Explain
state/retry, queue leases versus deadlines, publication custody/visibility and
independent Check delivery consistently with Tasks 001–005 and the library docs.

**REFACTOR.** Remove duplicated or stale explanations and reuse example/source
embeds across languages. Repair gaps in rendered guides and rerun the reader
exercise; capability defects return to their existing task owners.

## Acceptance Criteria

| Obligation | Required proof |
|---|---|
| REQ-050; INV-002/015/016; AC-042 | The inspection journey and its browser cases prove active/terminal state, safe content, admission separation, explicit unavailability and bounded reads. |
| REQ-044; INV-001/012–014; AC-037 | Example proof matrix below runs all three alternatives on clean/defect/insufficient evidence with actual extraction/tool calls and ordinary final verdict/check. |
| REQ-032 integrated closure; AC-021 | Combined-surface regression below verifies the same final story; Task 001 and Tasks 002–005 remain responsible for every operation/negative case they introduce. |
| REQ-033; AC-022 | Docsite checklist/rendered-guide proof below is required delivery, including developer setup/customization/debugging and complete references, not YAML parsing only. |
| INV-010 | Existing CI must select changed capability journeys on macOS arm64/Linux x86_64/Linux arm64, with the same self-hosted/SaaS code. Do not introduce platform-specific/private runtime paths. |
| Revision-18 durability obligations in metadata; DUR consumer seam | Lifecycle proof closes this capability against the shared Bifrost state foundation with unchanged identities, bounds, current permissions and exact evidence. Task 001 owns loading, acceptance, replay and old-consumer replacement. |

## Implementation Reference

Existing UI integration target, BFF execution calls, shared Execution contracts and Task
001 accepted-state reader provide the seam. Extend current lifecycle journey and rendered
docs; no separate archive endpoint/store is needed.

| Inspected owner / consumers | Reuse and precise gap |
|---|---|
| UI ui-pages.integration.test.ts; tenant routing/session/problem; node SDK client | Current production BFF/Playwright harness boots a real Wyrd server and signs in normally. Extend these views/read paths; do not access SQL/object storage from a private UI service. |
| proposed-examples; SDK verify_in_real_time/verifier_building_blocks; MCP verification | Use existing declaration/provider/journey patterns and the same checked-in example files. No test-only authoring parser, copied orchestrator or language-specific pipeline. |
| docs/AGENTS.md; build-and-verify/verify; Understand/reference pages; CodeFromFile/LangTabs | Existing page shapes, source embedding, generation/link/a11y checks supply docs delivery. Read docs CONTRIBUTING/PRODUCT at implementation and use wyrd-ui skill for UI work. |
| Library README/API rustdoc | Explain extensions at the upstream owner without claiming Wyrd tenant policy or scheduling belongs in the library. Tasks 004–005 already own corresponding API documentation changes; this task verifies teaching consistency. |

## Verification and Evidence

Proof numbers identify supporting tests, not additional implementation scenarios.
All new names below are **planned**. Existing packages, targets, runners and Postgres setup are listed in [shared verification](verification.md). After adding a test, list/confirm its actual selector before RED; zero selected tests is not proof. The RED paragraph states the missing assertion, not a claim that a test has run. Use focused owner tests plus real public journeys; no credentialed provider or synthetic host load.

### Proof 1

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd crates/wyrd/wyrd-server/wyrd-ui && WYRD_UI_INTEGRATION=1 pnpm exec vitest run src/lib/ui-pages.integration.test.ts -t "^ui preserves running invocation progress$"'
```

### Proof 2

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd crates/wyrd/wyrd-server/wyrd-ui && WYRD_UI_INTEGRATION=1 pnpm exec vitest run src/lib/ui-pages.integration.test.ts -t "^ui publication preview respects content authority$"'
```

### Proof 3

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd crates/wyrd/wyrd-server/wyrd-ui && WYRD_UI_INTEGRATION=1 pnpm exec vitest run src/lib/ui-pages.integration.test.ts -t "^ui loads only requested publication pages$"'
```

### Lifecycle proof

New selector is **planned**. Use the existing target/setup below, add/list the actual
test before RED, and record a nonzero selection plus the missing-behavior failure and
closure result. Extend the task's existing Python/TS/MCP public-closure case table with
its recovery outcomes rather than adding full suites; Task 006 uses existing
combined-surface cases. No runtime proof is claimed during planning.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd crates/wyrd/wyrd-server/wyrd-ui && WYRD_UI_INTEGRATION=1 pnpm exec vitest run src/lib/ui-pages.integration.test.ts -t "^ui preserves execution inspection across state recovery$"'
```

### Example and combined-surface proof (verification-only)

Task 001 and Tasks 002–005 implement these runtime contracts. Run their integrated examples as regression/configuration proof; no manufactured RED is required merely to verify already-correct machinery. Missing behavior returns to its existing owner, not example-specific code.

Planned `code_review_pipelines::canonical_examples_return_supported_verdicts` uses the same checked-in files on every surface:

| Example | Variations and assertions |
|---|---|
| repeated-sweeps | Three private calls with original indices, identical evidence, one consolidation and required final report; retries do not become a fourth reviewer. |
| conditional-specialists | Neither/either/both specialists selected; false branches skip, failures remain partial/failed, consolidation runs with honest manifests. |
| speculative-review | Agreement/disagreement/invalid decision; extra pass ready before/after decision; retain/discard/block and final evidence reflect the resolved condition. |
| All three | Clean supported evidence → passed; substantiated introduced defect → failed; insufficient evidence → inconclusive. Require the named report and matching exact-head check. No client schedules steps or moves reports. |

Planned `code_review_pipelines::review_journey_agrees_across_surfaces` and mirrored Python/TS/MCP cases verify shared team Verifier adoption by two applications, local declaration, queued execution, progress/report/check read and denied/lag behavior. Each feature task already owns its operation coverage; this is one combined regression, not a duplicate implementation task.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=code_review_pipelines::canonical_examples_return_supported_verdicts)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=code_review_pipelines::review_journey_agrees_across_surfaces)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_code_review_pipelines.py::test_review_journey_agrees_across_surfaces'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/code-review-pipelines.test.ts -t "^review_journey_agrees_across_surfaces$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all --test-threads=1 -E "test(=verification::pg_tests::review_journey_agrees_across_surfaces)"'
```

### Dedicated docsite proof (static and fresh-reader)

Before docsite completion, provide rendered guide/concept/reference/troubleshooting URLs, tested source embeds and the full non-Mermaid diagram/text alternative. Check every topic in the implementation table against REQ-033; a link to the specification does not teach the workflow. A fresh reader, given only rendered docs, must install the deployment's existing App, select repos and complete verified Wyrd tenant connection, adopt/customize a Verifier, run it, find reports/checks and diagnose a documented failed/partial workflow. Capture what the reader did and repair confusing steps. Library README/API instructions must agree with Tasks 004–005 shipped behavior and remain standalone: generic client/review APIs, never Wyrd onboarding routes or tenant policy.

```bash
mise run check:examples
mise run docs:check
```

Do not claim the examples implement live finding-triggered successors or the entire CodePal runtime.

Apply only the [shared owner checks](verification.md#owner-checks) for this write set; record real command/results during implementation.

## Material Stop Conditions

No example-only syntax, hardcoded code-review engine/report schema/filename, untested snippet, unsafe download/preview, UI-owned execution or claim of live finding-triggered CodePal behavior. Missing teaching topics or a failed fresh-reader journey blocks completion.

## Authority Links

[Approved GitHub amendment revision 24 and existing approved inspection baseline](../spec.md) · [Durability decisions 1–5](../execution-state-and-durability.md) · [Task ownership/dependencies](README.md) ·
[Publication contract](../output-publication.md) · [PR
extraction](../pull-request-extraction.md) ·
[Orchestration](../example-orchestration.md).

[Wyrd design](../../../../architecture/wyrd-design.md) · [Bifrost design](../../../../architecture/bifrost-design.md) · [Security](../../../../architecture/wyrd-security-posture.md) · [Implementation execution](../../../implementation-execution.md) · [Testing](../../../../TESTING.md). Apply nested AGENTS.md at each implementation owner.
