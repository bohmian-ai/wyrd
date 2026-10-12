Review ID: wyrd-ui-live-data-plan-20261012
Target: changes/active/wyrd-ui-live-data/spec.md revision 3, route-audit.md, tasks/001–018
Gate: Plan
Verdict: Revise
Architecture axis: PASS
Executability axis: FAIL
Critical: 0
Major: 1
Task coverage: All implementation tasks
Rehearsal: FAIL

## Evidence and scope

Independent read-only review of all eighteen proposed tasks and their approved specification/route audit. Reviewed artifacts were not changed. This report is stored in the active packet at the caller's explicitly requested location.

Authorities used: AGENTS.md and architecture/agent-rules.md govern ownership, tenancy, tests and reuse; architecture/wyrd-design.md and wyrd-doctrine.mdx govern exact Card identity, server-owned human federation and the shared SDK boundary; architecture/bifrost-design.md governs canonical analytical ownership, bounded queries, best-effort visibility and terminal-safe results; architecture/wyrd-security-posture.md governs session secrecy, verified tenancy, authorization and non-blocking audit. The spec-driven-development, implementation-execution and testing-workflows references govern approved scope, material stop conditions and focused versus journey proof. The reviewer rubric, execution-readiness, production-risk and public-workflow references governed this Plan gate.

Source evidence inspected includes mounted build_router/cards_router/verification routes; registry read contracts and metadata field resolver; UI hooks, server-only WyrdClient, page loads, contextual-state helpers, Card projection and test owners; canonical verification/Eval/Drift schemas; mise identity dispatch and codegen; production identity host; UI manifest/Vitest configuration; SDK native client/platform packaging; official Dockerfile and image startup script. The fresh system_rev agent independently rehearsed generation, identity/public routing and analytical deployment seams.

## Findings

### PR-001 — Major: Shipping native SDK integration lacks runtime-specific proof

- **Location:** tasks/009-service-workspace.md:107–131, with production runtime work explicitly required at approach step 3 (line 52) and Expected Write Set (line 102).
- **Type:** INVALID_VERIFICATION_RECIPE.
- **Obligations:** REQ-003, REQ-009, REQ-018; AC-008 and AC-018; task 009's explicit SDK/native/production-image consumer closure.
- **Issue:** The task introduces the existing native TypeScript SDK into the production BFF and requires image/runtime closure, but its concrete checks execute the host-built BFF, component tests and pnpm check/build. None proves an authenticated analytical read through the shipped image's BFF with its installed/pruned native package and actual endpoint configuration.
- **Reachable consequence:** The host page gate can pass while the image lacks a compatible native binary or its BFF cannot connect to the configured gRPC endpoint. Deployment then fails at SDK import or analytical connection; all subsequent live telemetry/verification pages depend on that path.
- **Repository evidence:** identity_ui_e2e.rs::start_bff (113–140) runs host node with the UI working directory. docker/official/Dockerfile:14–33 installs from the UI manifest/lock, prunes and copies modules from a Node slim builder into Ubuntu 24.04. sdks/wyrd-sdk-ts/wyrd/package.json declares platform-specific optional native dependencies. sdks/wyrd-sdk-ts/native/src/client.rs::NativeWyrdClient::connect_bifrost reuses the shared Bifrost connection. scripts/server/test-startup-prod.sh:100–105 checks readiness/OpenAPI and unauthenticated root HTML; it never loads an authenticated analytical UI page. mise.toml already owns test:server:startup:prod.
- **Required artifact edit:** Extend task 009's scenario/acceptance and verification contract with one authenticated analytical BFF read under the official image's production Node/runtime dependency layout. Reuse existing image/startup ownership; explicitly prove native load, correct configured connection, actual terminal-safe rows and no fixture fallback, and list its narrow repository-native command. A bounded filtered extension of the existing startup proof is sufficient. No new endpoint, task family or specification revision is required.

## Architecture and execution readiness

The selected architecture passes: existing Card list/UID/reference/version APIs own registry reads; canonical POST /v1/query through the shared SDK owns telemetry and retained verification; existing binding/run reads own known control status. SvelteKit server loads directly call the server-only facade. A separate /ui API family would duplicate those owners without a source-proven missing capability.

The deployed path is browser/CLI → one public nginx origin → Rust auth/API or Node BFF → authenticated Rust registry/query services. Rust brokers Keycloak. The CLI constructs device/token URLs from its configured Wyrd origin; it does not use discovery or exchange provider tokens itself. The missing published discovery forwarding is configuration owned by task 002, independently of CLI authentication.

The current hooks revalidate protected requests, including data navigation/actions, and browser tokens remain private to BrowserSessions. Tasks preserve per-request authority and disallow URL-selected tenants or presentation scope as authorization. Query failure is contained at the affected load/request; no task introduces a process-wide fatal-query policy. Failed/degraded terminals cannot masquerade as empty complete results; resource release and bounded deadlines are explicitly owned by task 009. Registry/auth sibling capabilities can remain available during analytical failure. Existing outbox visibility and audit semantics are retained rather than strengthened.

Independent system_rev reviewed these interruption/recovery paths and found the same image proof gap. Local provider, query-schema, permission and session negative branches are specified, and combined/browser gates cover recovery without fixture fallback. Production image/native recovery cannot yet be accepted because its owning task omits the runtime-specific smoke (PR-001).

## Adversarial analysis

- **Counter-hypothesis: UI needs its own server endpoint family.** Disproved for approved scope by mounted registry operations, canonical query/control surfaces and existing SvelteKit routes. No new durable UI API is necessary.
- **Counter-hypothesis: service_name can scope all verification evidence.** Disproved by ResultsTable's subject/owner/binding/Verifier identities and absence of a service_name field. The route audit and tasks explicitly require exact authorized attribution and prohibit assigning unattributed direct results to a selected Service. Names remain telemetry resource filters.
- **Counter-hypothesis: mock totals and newest sorting can be copied.** Disproved by ListCardsResponse's items/next_cursor without global totals, and canonical keyset selection. Task 004 explicitly requires truthful unknown totals and forbids page-local global sorting/count claims.
- **Counter-hypothesis: result inventory can imply unfinished runs or healthy baseline state.** Disproved by completed-only ResultsTable and separate binding/run control reads. Tasks 015–017 preserve that separation, skipped/null evidence and real baseline readiness.
- **Counter-hypothesis: the filtered identity command runs every journey.** Disproved by mise run_ui forwarding WYRD_UI_FILTER and checking exactly one selected case. Task 002 owns discovery of the new browser cases; later packets separate lightweight focused iteration from live acceptance. Existing lane naming and setup cost are bounded mechanics.
- **Counter-hypothesis: host build proves production native packaging.** Not disproved; current Docker and startup consumers establish PR-001.

Non-gating handoff notes:
1. Planned test names are future selectors, not existing passing tests. Task 002 must close unique discovery/forwarding before page gates can run; current packets state this.
2. Existing Card/Observe test modules often start a mock Vite server in beforeAll. Keep the proposed focused projection checks genuinely focused; moving private test placement or preserving inherited setup is implementation-owned and does not weaken browser acceptance.
3. Every page's settled-pagination assertions must use a bounded settled dataset; do not imply snapshot consistency across ongoing ingestion. The approved spec already states this boundary.

## Traceability and task coverage

All REQ-001–025, INV-001–011 and AC-001–018 have packet owners. TASK-018's broad closure mapping is supplemented by concrete earlier page owners; it does not substitute for individual gates.

Task compilation dimensions: O = outcome/scope; I = owners/impact; C = material interfaces; F = consequential control flow/failure; A = acceptance; T = test design/tier; V = verification; D = adaptation/stops; R = cold rehearsal. P means PASS and F means FAIL. Each row was reviewed; none was sampled out.

| Task | O | I | C | F | A | T | V | D | R | Cold first steps / remaining choice |
|---|---|---|---|---|---|---|---|---|---|---|
| 001 | P | P | P | P | P | P | P | P | P | Serve the composed contract using existing Rust host; add generation/drift probe and type-check consumers. Generator/source-export mechanics are reversible; no new wire owner. |
| 002 | P | P | P | P | P | P | P | P | P | Extend existing identity lifetime and unique selector forwarding; add real browser/proxy and fresh-human CLI assertions before repairing demonstrated auth gaps. Browser library and trust/setup placement are reversible. |
| 003 | P | P | P | P | P | P | P | P | P | Connect current session to registry-backed home and await load; test empty/populated and truthful partial operational sections. Presentation of unknown totals is bounded by truthfulness. |
| 004 | P | P | P | P | P | P | P | P | P | Translate URL metadata filters to existing ListCardsRequest and cursor; test multi-page exclusions and resets. UI cursor mechanics are private. |
| 005 | P | P | P | P | P | P | P | P | P | Extend shared exact Card reads, finite kind resolution and by-reference selected versions; assert missing/denied identity. Probe ordering/parallelism is private and bounded. |
| 006 | P | P | P | P | P | P | P | P | P | Exercise real Data declaration through task 005's shared owner; add only missing Data mapping. Already-correct generic behavior receives regression proof. |
| 007 | P | P | P | P | P | P | P | P | P | Exercise actual Agent/Prompt relationship and selected versions through shared shell. No new graph owner; formatting remains private. |
| 008 | P | P | P | P | P | P | P | P | P | Exercise exact Prompt declaration with safe denied/missing cases and secret checks. Generic mapping may already satisfy it; no artificial code churn. |
| 009 | P | P | P | P | P | P | F | P | F | Connect private session/current SDK and schema-safe canonical queries; exercise composition and faults. Host path starts coherently; deployed-runtime proof remains missing (PR-001). |
| 010 | P | P | P | P | P | P | P | P | P | Compose bounded scoped overview queries using analytical seam; verify empty/degraded/failed states and destination scope. Summary helper placement is private. |
| 011 | P | P | P | P | P | P | P | P | P | Project real spans to trace grain, scoped matching and deterministic continuation; test settled inclusion/exclusion. Exact SQL formulation is private. |
| 012 | P | P | P | P | P | P | P | P | P | Pass scope to actual trace/span reads; project nested events/links and preserve list context. Formatting/correlation helpers are private. |
| 013 | P | P | P | P | P | P | P | P | P | Project actual records under equivalent filter/count/page scope; preserve duplicate occurrences and optional correlation. Selection key is presentation state, not persistence. |
| 014 | P | P | P | P | P | P | P | P | P | Query actual metric/resource/unit/type/temporality series; assert truthful missing values and supported measures. No new metric engine. |
| 015 | P | P | P | P | P | P | P | P | P | Populate real direct and binding results; use exact attribution and native permission/query contracts. Result-grain selection and continuation implementation are private. |
| 016 | P | P | P | P | P | P | P | P | P | Join the exact result/task evidence or direct assertion_result; preserve skip/null/control-state distinctions and links. No invented task table. |
| 017 | P | P | P | P | P | P | P | P | P | Generate actual fit/comparison evidence and read report/features/control baseline; test unhealthy edge states. Public workloads/runtime are already owned. |
| 018 | P | P | P | P | P | P | P | P | P | Compose accepted Home owners and run combined/isolation/browser loading story. Integration corrections remain in shared owners; no second harness. |

Rehearsal identities: this independent plan_readiness_review agent self-rehearsed all eighteen tasks; fresh system_rev independently repeated material generation/auth/native deployment seams (001/002/009/018). Inputs were only the approved packet, normal repository authorities, current source/manifests/test runners and relevant existing tests. No intended verdict was supplied to the system reviewer. Located evidence and first test/implementation steps are recorded above. The only remaining material proof decision is which existing-runtime test extension closes PR-001. No reviewed task revisions were made.

## Static limitations

No runtime, browser, schema-generation, database, SDK or image test was executed in this read-only review. Planned browser cases and generated HTTP artifacts do not yet exist, so their passing results are not claimed. Validation confirms this report's structure, not implementation correctness. The review establishes source-backed startability except for the identified deployed-runtime proof gap.

