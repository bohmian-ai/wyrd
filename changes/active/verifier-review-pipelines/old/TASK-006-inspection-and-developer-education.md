---
id: TASK-006
spec: SPEC-verifier-review-pipelines
spec_revision: 18
status: proposed
supplement: SUPPLEMENT-verifier-execution-state-and-durability@2
depends_on: [TASK-001, TASK-002, TASK-003, TASK-004, TASK-005]
req: [REQ-033, REQ-044, REQ-050, REQ-052, REQ-053, REQ-054, REQ-055, REQ-056, REQ-057, REQ-058, REQ-059]
inv: [INV-001, INV-002, INV-010, INV-012, INV-013, INV-014, INV-015, INV-016, INV-017, INV-018, INV-019]
ac: [AC-021, AC-022, AC-037, AC-042, AC-046, AC-047, AC-048, AC-049, AC-050, AC-051]
---

# Execution inspection and developer documentation

## Outcome and Value

Implement the ongoing execution UI and deliver the required in-depth docsite with three runnable code-review examples. The examples and cross-surface integration are verification of Task 001 and Tasks 002–005; UI behavior is new implementation in this task.

## What Changes and What Must Stay True

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
| **Dedicated docsite deliverable** | Write progressive setup/authoring guides, concepts, complete field/error/permission reference and troubleshooting. Teach team definitions reused by two apps, trusted target/local TOML, installation/allowlist/publisher authority, exact PR bundle, output/publications, runtime file/directory/JSON publication, identities/retries, joins/loops/speculation, limits/recovery, API/UI/Bifrost debugging, storage lifecycle/visibility and independent check retry. |
| **Tested teaching materials** | Embed existing journey-tested source via CodeFromFile/LangTabs, equivalent Rust/Python/TS/CLI/MCP workflows, accessible non-Mermaid full control-flow diagram plus text alternative and updated library README/API docs. A fresh reader must complete setup, customize/run a verifier and diagnose a documented partial failure from rendered guides alone. |

## Approach

1. Add execution timeline/read models through existing public APIs and authenticated UI/BFF.
2. Add manifest browsing, safe bounded previews/downloads and explicit authorization/lag states.
3. Verify all three example workflows and combined surface behavior using Task 001 and Tasks 002–005 shipped contracts.
4. Write dedicated progressive guides/reference/troubleshooting and embed tested files/accessible diagram.
5. Run browser, example, rendered-docsite/link/a11y and fresh-reader proof; resolve defects at their original owner.

## Ordered Implementation Scenarios

### 1. Inspect running execution

**Behavior.** Open a fanout/repeat/speculative execution, receive a publication, refresh and restart. Timeline preserves exact identities, attempts, requirements/disposition and accepted content; pending projection is displayed honestly.

**RED.** Production BFF/Playwright test asserts visible states and retained report after reload/restart against a real Wyrd server. Expected RED: the ongoing execution view is absent. Run [proof 1](#proof-1); names are planned until added/listed.

**GREEN.** Project paginated execution APIs through existing UI/auth/client owners; add no UI scheduler.

**REFACTOR.** Reuse current components and typed error/load boundaries. Keep this and earlier scenario tests green.

### 2. Inspect publications safely

**Behavior.** Browse a nested directory and preview text/Markdown/JSON, then test malicious markup, denied/foreign content and discarded work readable under the same execution-read grants. No script runs or unauthorized bytes/downloads appear; valid content retains producer/source provenance.

**RED.** Real browser asserts tree/readback, safe rendering and exact denied/lag states for the table of content cases. Expected RED: the new publication tree/preview/inspection views are absent. Run [proof 2](#proof-2); names are planned until added/listed.

**GREEN.** Use public authorized File/publication APIs and safe preview components; enforce execution-read authorization separately from ordinary admission.

**REFACTOR.** Reuse auth/preview/error handling, no private storage route. Keep this and earlier scenario tests green.

### 3. Bound inspection reads

**Behavior.** Open a large bounded manifest/history without opening files. Only requested pages load; opening one report loads only its bounded preview. Navigation cancels obsolete work and a frozen page cut stays stable while new entries arrive.

**RED.** Browser/network assertions count page/body requests and verify cancellation and no eager tree downloads; avoid host-load/latency claims. Expected RED: the new execution/publication page and lazy-body read path is absent. Run [proof 3](#proof-3); names are planned until added/listed.

**GREEN.** Separate metadata pages from lazy content requests, propagate cancellation and refresh with a new explicit cut.

**REFACTOR.** Use shared client pagination rather than a parallel UI cache. Keep this and earlier scenario tests green.

### 4. Inspect the same execution through recovery and completion

**Behavior.** Open a running execution, begin invocation/publication pages,
complete it and refresh after server restart. Inspect success/error/cancel/timeout,
partial/discarded publications and independently changing Check delivery with the
same identities, grants and original page cut. Delayed/unavailable accepted state
is explicit; safe previews stay bounded and source Card deletion loses no evidence.

**RED.** Add browser request/content assertions through actual Bifrost-backed
state recovery with no PostgreSQL invocation/publication payload dependency.
Run [lifecycle proof](#lifecycle-proof); the selector is planned.

**GREEN.** Reuse existing server API/BFF and timeline/tree/download controls.
Teach the common run-state lifecycle and distinguish queue lease expiry from
execution deadline and Bifrost acknowledgement.

**REFACTOR.** No UI-owned archive detection, source selector, private content path,
or scheduling/recovery logic. Return backend gaps to their owner.

## Acceptance Criteria

| Obligation | Required proof |
|---|---|
| REQ-050; INV-002/015/016; AC-042 | Three browser scenarios prove active/terminal inspection, safe content, inspection/downstream-admission separation, lag and bounded reads. |
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

Before docsite completion, provide rendered guide/concept/reference/troubleshooting URLs, tested source embeds and the full non-Mermaid diagram/text alternative. Check every topic in the implementation table against REQ-033; a link to the specification does not teach the workflow. A fresh reader, given only rendered docs, must connect/allow a repo, adopt/customize a Verifier, run it, find reports/checks and diagnose a documented failed/partial workflow. Capture what the reader did and repair confusing steps. Library README/API instructions must agree with Tasks 004–005 shipped behavior.

```bash
mise run check:examples
mise run docs:check
```

Do not claim the examples implement live finding-triggered successors or the entire CodePal runtime.

Apply only the [shared owner checks](verification.md#owner-checks) for this write set; record real command/results during implementation.

## Material Stop Conditions

No example-only syntax, hardcoded code-review engine/report schema/filename, untested snippet, unsafe download/preview, UI-owned execution or claim of live finding-triggered CodePal behavior. Missing teaching topics or a failed fresh-reader journey blocks completion.

## Authority Links

[Approved spec, revision 18](../spec.md) · [Durability decisions 1–5](../execution-state-and-durability.md) · [Task ownership/dependencies](README.md) ·
[Publication contract](../output-publication.md) · [PR
extraction](../pull-request-extraction.md) ·
[Orchestration](../example-orchestration.md).

[Wyrd design](../../../../architecture/wyrd-design.md) · [Bifrost design](../../../../architecture/bifrost-design.md) · [Security](../../../../architecture/wyrd-security-posture.md) · [Implementation execution](../../../implementation-execution.md) · [Testing](../../../../TESTING.md). Apply nested AGENTS.md at each implementation owner.
