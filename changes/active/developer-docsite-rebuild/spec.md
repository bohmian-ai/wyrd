---
id: SPEC-developer-docsite-rebuild
revision: 2
status: approved
---

# Rebuild the Wyrd developer documentation

## Objective and user value

Give a developer a short path from understanding Wyrd to a verified result,
then let them complete a specific task or look up a contract without reading
the whole site. Rebuild the documentation content from an empty content tree,
one useful, truthful slice at a time.

This specification replaces the documentation scope of
`SPEC-developer-docsite` when approved. It does not reopen the brand, styling,
or theme established by that change's approved `TASK-001`.

## Scope and delivery boundary

- The current landing page remains the visual starting point. Its words,
  examples, claims, and links may change to match published documentation and
  supported Wyrd behavior.
- The deleted files under `docs/src/content/docs/` remain deleted. New content
  is authored from current Wyrd contracts and runnable workflows; historical
  articles are not restored as the starting corpus.
- The content architecture covers local development, self-hosted operation,
  first-class SDKs, HTTP, CLI, MCP, and relevant integrations. Individual
  sections and pages appear as their content becomes publishable.
- This change owns documentation content, navigation, presentation within the
  existing docsite, and documentation verification. It does not change Wyrd
  server, SDK, CLI, gateway, authentication, or verification behavior to make a
  documentation promise true.

## Required experience

### Proposed site structure

This is the target content map, not a requirement to publish empty pages. Each
line names a distinct page; indented headings under **Build and verify** are
sidebar topics. The section, topic, and page are the deepest navigation path.
Publish each page only when its stated user path or reference is accurate.

```text
Get started
  Introduction — what Wyrd verifies and where it fits
  Quickstart — declare, observe, and see a first verdict
  Set up Wyrd — run a local server and connect a client

Build and verify
  Declare
    Define a service and its components
    Create and register Cards
    Bind a Verifier to a service component
  Observe
    Record a run and observations
    Collect traces, logs, and metrics
    Write and query custom data with Bifrost
  Verify
    Run a verification and inspect its evidence
    Evaluate behavior
    Monitor drift
    Respond to a failed verification

Connect
  Use an SDK
  Use the CLI in automation
  Connect an agent through MCP
  Set up the LLM gateway
  Make gateway requests from a framework

Operate
  Deploy Wyrd for a team
  Configure OIDC sign-in
  Grant and inspect permissions
  Delegate access
  Troubleshoot a deployment

Understand
  Cards, versions, and relationships
  How verification works — bindings, Triggers, Verifiers, and Operators
  Runs, observations, and retained evidence
  Bifrost architecture
  Identity and authorization
  Wyrd system architecture

Reference
  Card schemas
  HTTP API and OpenAPI
  Rust, Python, and TypeScript SDK APIs
  MCP tools
  CLI commands
  Error codes
  Configuration and environment variables
  Documented limits
```

The **Verify** topic may later gain distinct guides for Change Request, Code
Review, workflows, and live verification. Those names are candidates, not
published promises: their procedures and exact labels depend on supported
Wyrd behavior. Gateway and framework guides likewise appear only when their
end-to-end paths are supported. The content map is an editorial destination;
the visible sidebar is always the subset that has earned publication.

### Main-content structure

| Page | Required main-content order |
|---|---|
| Landing | One-line value proposition; Declare → Observe → Verify visual; a clear quickstart action; a few task/reference entry cards. The current branded composition remains the basis. |
| Introduction | What Wyrd does; the service-verification loop; where Wyrd stops and the user's runtime starts; the first action. |
| Quickstart | Outcome and prerequisites; one complete copyable path; expected output and visible verdict; how to verify it; next task. |
| Task guide | Outcome; prerequisites; code or commands; expected result; common failure and recovery; concise links to explanation and reference. |
| Explanation | The mental model and rationale; one diagram or example where useful; consequences and boundaries; links to tasks and exact contracts. |
| Reference | Source-backed definitions, fields, commands, parameters, errors, and limits with stable anchors; no narrative prerequisite chain. |

The quickstart is one guided learning experience. The **Build and verify**,
**Connect**, and **Operate** pages are how-to guides. **Understand** holds
explanations; **Reference** holds lookup material. These roles govern page
content even when a reader enters through search instead of the sidebar.

### Navigation and content types

- **REQ-001**: The eventual primary navigation MUST use these reader-facing
  destinations: **Get started**, **Build and verify**, **Connect**, **Operate**,
  **Understand**, and **Reference**. Empty destinations MUST stay out of the
  sidebar until they contain a useful published page. The proposed site
  structure above is the target inventory and placement. The navigation MUST
  remain no deeper than section → topic → page.
- **REQ-002**: **Get started** MUST explain what Wyrd is, provide a quickstart
  for a first verified result, and provide the setup needed to follow it. The
  quickstart MUST be the clearest entry point from the landing page.
- **REQ-003**: **Build and verify** MUST organize task guides around **Declare
  → Observe → Verify**. Declare covers service/components, exact Card versions,
  verifier bindings, and registration. Observe covers runs, observations,
  telemetry, and custom data through Bifrost. Verify covers executing checks,
  reading verdicts and evidence, evaluation, drift, and reactions to failure
  where those workflows are supported.
- **REQ-004**: **Connect** MUST hold supported integration tasks, including SDK,
  HTTP/CLI/MCP automation, framework use, and LLM gateway use as each becomes
  documentable. **Operate** MUST hold deployment, identity, authorization,
  OIDC, delegated access, and troubleshooting tasks. A command used to finish
  a task belongs in that task guide; the exhaustive CLI command catalog belongs
  in Reference.
- **REQ-005**: **Understand** MUST explain concepts, rationale, trust
  boundaries, and architecture. **Reference** MUST provide lookup material for
  Card contracts, APIs, SDKs, MCP, CLI, errors, configuration, environment
  variables, and documented limits. Bifrost architecture belongs in
  Understand; writing and querying custom data belongs in a task guide.
- **REQ-006**: Each page MUST have one primary job: guided learning, completing
  a task, explaining a concept, or looking up a contract. Cross-links MAY
  connect those page types; a task guide MUST NOT bury its procedure in a long
  architecture explanation, and a reference page MUST NOT be the only account
  of how to complete a user task.
- **REQ-007**: Sidebar labels and page titles MUST describe the reader's goal
  or the contract being looked up. Internal component names MAY appear where
  they are necessary to operate or understand the system, but MUST NOT replace
  the reader's task as the primary navigation model.

### First success and page content

- **REQ-008**: The landing page MUST state Wyrd's verification value in one
  sentence, show the Declare → Observe → Verify relationship, and offer direct
  paths to first success, task guides, and reference. Every visible action and
  example MUST link to an existing page or supported action. It MUST NOT claim
  a completion time or successful verdict that has not been measured with its
  documented prerequisites.
- **REQ-009**: The quickstart MUST use one coherent, complete example: prepare
  a supported local environment, declare a service and its verification
  binding, record relevant behavior, then retrieve a visible verdict and its
  evidence. It MUST show copyable inputs, expected output, a success check,
  cleanup when needed, and the next useful task. Aim for a measured path under
  five minutes; if current Wyrd capabilities cannot support the full loop, do
  not publish an illustrative or partially runnable procedure as the
  quickstart.
- **REQ-010**: A published task guide MUST lead with the outcome and shortest
  supported procedure, show prerequisites and complete code or commands, and
  say how to confirm success. It MUST cover common caller failures, relevant
  authorization failures, recovery, and links to exact contracts without
  turning into an exhaustive reference page. Include imports and realistic
  values in code examples; use placeholders only where an actual secret or
  deployment-specific value is required.
- **REQ-011**: Use one consistent service example across the core journey
  where current contracts allow it. Rust, Python, and TypeScript examples
  SHOULD be presented together when they express the same supported workflow;
  a language with a missing capability MUST be identified honestly instead
  of receiving invented parity.
- **REQ-012**: Specialized subjects such as Change Request, Code Review,
  workflows, and continuous or scheduled verification MAY become task guides
  only when their actual user path can be demonstrated. Their concepts may be
  explained separately. No unshipped feature, placeholder procedure, or
  planned product sits in primary task navigation or presents as runnable.
- **REQ-013**: Public reference material MUST follow owning Wyrd contracts.
  Where source-generated OpenAPI, schemas, SDK declarations, or CLI metadata
  exist, published reference MUST derive from or link to that source and have
  drift checks. The live `GET /openapi.json` contract MUST be discoverable; a
  hosted API explorer MUST NOT be implied unless one exists.

### Progressive publication and reading experience

- **REQ-014**: The site MUST remain coherent after each incremental content
  addition: navigation contains only useful published destinations; landing,
  sidebar, search, related links, and machine-readable indexes do not direct
  readers to missing pages. A page with a partial workflow identifies the
  supported portion and the gap at the point of use. Planned material does not
  appear as an ordinary task result.
- **REQ-015**: Search MUST index published content and provide a useful route
  from a query to the matching page. Code blocks MUST support accessible copy
  controls; headings MUST have stable anchor links; visible update dates MUST
  reflect a real content change rather than a build timestamp. Long code and
  tables MUST scroll within narrow screens. Reading width, hierarchy, both
  themes, and responsive navigation MUST remain comfortable and accessible.
- **REQ-016**: Documentation MUST be usable without the workbench. The same
  supported Wyrd contract MUST be recognizable across SDK, CLI, HTTP, MCP,
  generated reference, and prose. Procedures MUST NOT invent Wyrd Card kinds,
  aliases, routes, fields, status, relationships, or authentication behavior.

## Invariants and non-goals

- **INV-001**: Preserve the current approved Wyrd brand, styling, mark,
  typography, semantic tokens, light/dark theme behavior, and landing-page
  visual language from `TASK-001`. The canonical brand owner remains
  `crates/wyrd/wyrd-server/wyrd-ui/brand`; `docs/` remains its consumer.
  Content-driven wording or layout adjustments MUST use that visual system.
- **INV-002**: The removed historical content pages stay deleted. Do not
  recreate them wholesale, retain obsolete prose for URL compatibility, or
  add legacy vocabulary or compatibility routes. New pages earn publication
  through current contracts and evidence.
- **INV-003**: Wyrd's active protocol and doctrine govern product claims.
  Cards are declarations; Runs and Observations are measured facts; Verifiers
  produce judgments; Bifrost retains/query evidence; Policy governs, Audit
  records accountability, and Operators react. Documentation does not make
  the UI or any one language the source of truth.
- **INV-004**: Do not expose real secrets, suggest cross-tenant access, omit a
  required permission, or imply that a rejected request succeeded. Examples
  and troubleshooting MUST honor the documented identity, authorization,
  tenant, and audit boundaries.
- **INV-005**: Keep the existing SvelteKit, mdsvex, Pagefind, Shiki, and
  static-site foundation unless a later approved specification changes it.
  New content does not require a new documentation platform or search service.

Out of scope: restoring the prior article set; redesigning the brand or
workbench; implementing missing Wyrd capabilities; documenting hosted SaaS
operation; guaranteeing every eventual page is published in the first slice;
and preserving old content URLs as a separate compatibility product.

## Acceptance and evidence

- **AC-001**: At every published stage, a reader can enter from the landing
  page, reach every visible navigation destination, use search for published
  pages, and encounter no internal dead links or false runnable claims.
  Evidence: built-site link/search checks and a manual landing-to-page pass.
- **AC-002**: The first published quickstart can be run from its stated
  prerequisites through a visible verification result using exact documented
  commands; its measured completion time and expected output match the page.
  Evidence: a fresh-environment walkthrough and command checks.
- **AC-003**: Each later task page has a supported success path and at least
  the material failure path for that task. Examples match current contracts;
  generated reference does not drift. Evidence: source comparison, runnable
  walkthroughs where applicable, and repository documentation checks.
- **AC-004**: The navigation separates task, explanation, and lookup needs,
  follows the proposed site structure, stays within three levels, and excludes
  empty, planned, or misleading task entries. Evidence: content inventory and
  rendered desktop/mobile sidebar.
- **AC-005**: Light and dark pages, code controls, heading links, search,
  tables, and narrow-screen navigation retain the approved visual system and
  accessibility contract. Evidence: both-theme visual and keyboard review,
  plus `mise run docs:check` for touched documentation behavior.
- **AC-006**: The 64 previously deleted content files are not restored as a
  corpus, the current landing page remains, and no Wyrd product behavior is
  changed to support a documentation example. Evidence: final change diff.

## Expensive-to-reverse decisions

The proposed site structure, six reader-facing destinations, Declare → Observe
→ Verify task spine, separation of page purposes, source-owned references, and
the existing brand and docsite platform are the durable boundaries of this
rebuild. Exact page slugs, prose, example domain, and private site
implementation remain implementation choices. Old article URLs are not
preserved by default; any specific externally important redirect requires
separate evidence and an explicit decision.

## Open material decisions

None. Revision 2 was approved by the user on 2026-10-01. The old change
packet is retired separately because it also contains brand, workbench, and
client-behavior work outside this specification.

## Revision history

- **Revision 2 — 2026-10-01 — approved by the user:** Add the proposed
  page-by-page sidebar and main-content patterns. Keep publication incremental
  and gated by supported Wyrd behavior.
- **Revision 1 — 2026-10-01 — draft:** Replace the old broad docsite change's
  documentation scope with an incremental content rebuild. Keep the deleted
  content deleted, retain the landing page and approved Task 001 visual system,
  and make a truthful first verification the front door.

## Authority and context

- [`AGENTS.md`](../../../AGENTS.md)
- [`architecture/agent-rules.md`](../../../architecture/agent-rules.md)
- [`architecture/wyrd-design.md`](../../../architecture/wyrd-design.md)
- [`architecture/wyrd-doctrine.mdx`](../../../architecture/wyrd-doctrine.mdx)
- [`architecture/bifrost-design.md`](../../../architecture/bifrost-design.md)
- [`architecture/references/languages/spec-driven-development.md`](../../../architecture/references/languages/spec-driven-development.md)
- [Canonical brand design](../../../crates/wyrd/wyrd-server/wyrd-ui/brand/DESIGN.md)
- [Existing documentation product brief](../../../docs/PRODUCT.md)
