---
id: SPEC-canonical-evidence-thread-design-system
revision: 2
status: approved
---

# Wyrd developer docsite and canonical Evidence Thread design system

## Objective and user value

Build the Wyrd developer docsite around the real tasks a developer needs to
complete, and establish Evidence Thread as the one canonical production design
system for that docsite and the authenticated workbench. The docsite is the
primary delivery outcome. The shared visual system supports comprehension and
continuity between reading and operating.

The system is for developers, data scientists, and AI engineers who move
between documentation, terminals, IDEs, CI, and operational tooling. It must
feel recognizably Wyrd, stay calm during long technical sessions, make exact
system evidence easy to scan, and let a user move from a task to proof of its
outcome without learning two visual languages.

Evidence Thread expresses Wyrd's product truth: an exact Declaration is
connected to observed behavior, a versioned Verifier, its Judgment, and the
retained Evidence that makes the result attributable. This is scientific
provenance, not crime-scene, policing, or compliance theater.

## Scope

- Replace the current documentation landing, primary navigation, and task
  pathways with a coherent developer docsite covering Wyrd's purpose, local
  setup, self-hosted team deployment, access management, OIDC, service and
  model authoring, observation and verification, container/Kubernetes
  operation, and API/CI integration.
- Publish complete, verifiable procedures for supported local and self-hosted
  workflows, and clear capability status and next steps wherever the requested
  workflow is not yet shipped. Include S3, GCS, and Azure as object-store
  deployment choices where the current server supports them.
- Replace the current production Wyrd visual theme with the Evidence Thread
  identity in the canonical brand authority.
- Project the same semantic tokens, type roles, geometry, state language,
  interaction feedback, and evidence grammar into both `docs/` and the Wyrd
  workbench.
- Adopt the Wyrd mark already used by the repository README as the canonical
  Wyrd logo and favicon geometry.
- Provide equal-quality light and dark modes, responsive behavior, and the
  accessibility contract for both surfaces.
- Build the documentation shell, task pages, code presentation, and
  progressive-disclosure behavior needed by the developer documentation.
- Define the operating density and visual hierarchy for existing workbench
  routes and reusable components.
- Retire the superseded arcade, warm-paper, Bohmian-monogram, thick-border,
  and hard-offset-shadow treatments from Wyrd docs and product UI.

## Non-goals

- Rewriting unrelated legacy or reference articles solely to make every page
  look new. The named developer journeys, their navigation, and their directly
  needed reference links are in scope.
- Implementing missing server, SDK, CLI, container-release, Kubernetes,
  authentication, or CI/CD capabilities to make a documentation example work.
- Documenting SaaS operation or a separate proprietary enterprise edition in
  this local/self-hosted release.
- Changing the documentation's durable content contracts, generated API
  contracts, existing public URLs, or search engine. New task pages may add
  URLs while existing paths keep working.
- Changing workbench domain routes, BFF contracts, authentication, tenant
  behavior, server APIs, Card contracts, or durable Wyrd behavior.
- Redesigning or implementing the public Bohmian, Mitari, or Wyrd marketing
  websites. Evidence Thread may be extended there by a later approved change.
- Creating a general-purpose design-tool platform, theme marketplace, user-
  authored theme system, or new A2UI/runtime composition protocol.
- Requiring the documentation and workbench to share one compiled component
  package. They share one design contract; each application may retain the
  narrow implementation appropriate to its existing toolchain.
- Adding decorative dashboards, animation, evidence labels, serial numbers,
  or custody marks where no real Wyrd data gives them meaning.

## Definitions

- **Evidence Thread**: Wyrd's visual and interaction system based on scientific
  provenance, archival finding aids, and laboratory accession records.
- **Canonical token source**: the one machine-readable production source from
  which application-specific theme projections are derived.
- **Surface projection**: generated or mapped tokens consumed by one
  application without becoming a second authored palette.
- **Evidence thread**: a visual connector used only when the interface can show
  a real causal path among a Declaration, Observation, Verifier, Judgment, or
  retained Evidence.
- **Read surface**: developer documentation, optimized for comprehension,
  completion of a task, and fast retrieval.
- **Operate surface**: the workbench, optimized for dense inspection,
  comparison, and action.

## Required behavior

### One canonical identity and token system

- **REQ-001**: `crates/wyrd/wyrd-server/wyrd-ui/brand` MUST remain the
  canonical production owner for Wyrd brand assets, semantic design tokens,
  contrast obligations, and the rendered design reference. Documentation and
  workbench theme files MUST be projections of that authority rather than
  independently authored palettes.
- **REQ-002**: The exact geometry in `docs/src/assets/wyrd-mark.svg` MUST become
  the canonical Wyrd mark for workbench and documentation lockups, favicons,
  empty states, and brand references. Palette-aware renderings MAY recolor its
  wings, center spine, or outline, but MUST NOT redraw or replace its geometry.
  A parent-company mark or evidence-thread diagram MUST NOT impersonate the
  Wyrd mark.
- **REQ-003**: Wyrd prose MUST continue to use the proper product name `Wyrd`.
  A stylized lockup MAY use a typographic wordmark, but the mark remains the
  persistent product identifier.
- **REQ-004**: Familjen Grotesk MUST provide interface, navigation, heading,
  and documentation-reading roles. Fragment Mono MUST be limited to code,
  commands, identifiers, timestamps, measurements, compact metadata, and other
  genuinely machine-shaped values. Both faces MUST be shipped with the
  applications; production rendering MUST NOT depend on a request to a
  third-party font service.
- **REQ-005**: The canonical palette MUST expose the following semantic roles
  and mode values. Implementation-specific token names MAY differ, but every
  consumer MUST map to these roles rather than restating literals.

| Semantic role | Light | Dark | Meaning |
|---|---:|---:|---|
| Canvas | `#F3F6F5` | `#0E1413` | application/page field |
| Surface | `#FFFFFF` | `#131A19` | primary panel and chrome |
| Surface secondary | `#E7EEEC` | `#19211F` | subordinate regions and controls |
| Surface hover | `#EDF2F0` | `#1F2826` | neutral interaction feedback |
| Ink | `#10201D` | `#E3E9E7` | primary text and strongest rule |
| Muted ink | `#53645F` | `#A3AEAB` | supporting text and metadata |
| Rule | `#9AACAA` | `#434E4C` | structural dividers and control borders |
| Rule soft | `#D7E1DE` | `#222A28` | row and subordinate dividers |
| Declare / primary | `#5036D5` | `#A894FF` | Declaration identity, links, selection, primary action |
| Declare soft | `#EAE6FF` | `#241F46` | selected or contextual declaration field |
| Observe | `#007563` | `#67B891` | observed facts and Observation identity |
| Observe soft | `#BDF5E7` | `#15352F` | selected or contextual observation field |
| Retained Evidence | `#D7F33F` | `#B8D14A` | retained-evidence signal and logo spine |
| Success | `#187B45` | `#50DA83` | successful state, with non-color cue |
| Warning | `#A65F00` | `#FFB455` | caution state, with non-color cue |
| Failure | `#B52828` | `#FF715E` | failed judgment or error, with non-color cue |
| Failure soft | `#F9E8E7` | `#3B1D1B` | failure context field |
| Code surface | `#0E1716` | `#050908` | code and terminal blocks |
| Code ink | `#DCF5EE` | `#DCF5EE` | default code text |

- **REQ-006**: Indigo MUST identify Declaration and the primary action; green
  MUST identify observed facts; red and amber MUST retain failure and warning
  meaning; lime MUST be reserved for retained Evidence, the Wyrd mark's center
  spine, and direct interaction feedback that signifies evidence capture or
  retention. These colors MUST NOT become decorative page washes.
- **REQ-007**: The normal documentation and workbench canvas MUST remain a
  neutral mineral field. Neither surface may use an entire-screen indigo,
  lime, gradient, or dark-neon field as its default content background.
- **REQ-008**: Persistent surfaces MUST use a flat, ruled grammar: primarily
  one-pixel structural rules, restrained approximately two-pixel corners, and
  no decorative elevation. Hard-offset shadows, glassmorphism, glow, scanlines,
  CRT effects, pixel treatments, faux terminal chrome, and generic AI gradients
  MUST NOT appear. Functional overlays MAY use a restrained scrim or soft
  separation needed to preserve context.
- **REQ-009**: Common primitives shared in meaning across both surfaces—links,
  buttons, inputs, search, tabs, disclosures, status, panels, tables, code,
  copy feedback, focus, selection, loading, empty, authorization, and error
  states—MUST follow the same token roles and interaction language. Surface-
  specific layout MUST NOT fork their meaning.

### Theme behavior

- **REQ-010**: Light and dark MUST be two complete renderings of the same
  semantic system and component structure. Neither mode is a fallback,
  inversion, or partially themed variant.
- **REQ-011**: On a first visit without an explicit choice, each application
  MUST follow the user's system color preference. An explicit light or dark
  choice MUST persist for later visits to that application, apply before first
  meaningful paint, and correctly set native control color scheme without a
  visible wrong-theme flash.
- **REQ-012**: Theme selection is local presentation state only. It MUST NOT
  change tenant, principal, domain state, server behavior, or authorization.

### Developer documentation

- **REQ-013**: Documentation navigation MUST be organized around tasks and
  outcomes. It MUST NOT use audience-role labels, internal crates, deployment
  processes, or an exhaustive product inventory as the primary wayfinding
  model. The first navigation level MUST stay small and orient the reader to
  a useful next action; deeper tasks and reference detail become discoverable
  in context. Declare → Observe → Verify is the conceptual spine, not a rigid
  three-bucket navigation taxonomy.
- **REQ-014**: Documentation pages MUST expose a stable Wyrd header, visible
  search, persistent section navigation on wide screens, a restrained on-page
  table of contents when useful, a theme control, and direct access to
  reference material. Narrow screens MUST replace persistent navigation with
  an accessible control without hiding the page or its current location.
- **REQ-015**: A task page MUST lead with its outcome, required prerequisites,
  and shortest supported path. It MUST show the expected result and a concrete
  way to verify success before expanding into alternatives, advanced options,
  rationale, troubleshooting, or exhaustive reference. The primary path MUST
  ask the reader to make only decisions needed to complete that task.
- **REQ-016**: Progressive disclosure MUST reduce ceremony without hiding a
  required prerequisite, security boundary, destructive-action warning,
  unsupported capability, or failure recovery needed to complete the task
  safely. When a choice matters, the page MUST introduce it at the point of
  use with a clear default or selection rule; optional variants and exhaustive
  configuration belong in secondary sections or reference.
- **REQ-017**: Documentation code blocks MUST provide legible syntax
  highlighting, a language or file label when meaningful, copy control with
  immediate success feedback, keyboard access, and horizontal overflow rather
  than compressed code. Code and terminal surfaces MUST use the canonical dark
  code field in both application modes and preserve at least WCAG AA contrast.
- **REQ-018**: Documentation typography MUST target moderate reading density:
  approximately 44px desktop page titles, 17px leads, and 15px task prose.
  Mobile reading text MUST not fall below 16px. These sizes are role targets,
  not a requirement that every heading or label share one size.
- **REQ-019**: Likely next tasks MUST be few and contextual. Walls of equal
  cards, role-selection gateways, decorative diagrams, giant setup checklists,
  and fragmented chains of tiny prerequisite pages MUST NOT replace direct
  task completion. Readers MUST be able to discover deeper material without
  having to consume it before their first successful result.

### Complete developer task journeys

- **REQ-033**: The docs home and introductory path MUST explain what Wyrd is,
  why a developer would use it, and how Declare → Observe → Verify connects a
  versioned declaration to measured behavior, a Verifier judgment, and retained
  evidence. The reader MUST be able to start the local path or go directly to
  task and reference navigation without passing through a role gateway.
- **REQ-034**: The local-development journey MUST take a new contributor from
  repository prerequisites to a running server and client connection, show a
  health/readiness check, and state which conveniences are development-only.
  It MUST use current repository commands and identify the shortest supported
  path before optional setup detail.
- **REQ-035**: The self-hosted team journey MUST explain the currently
  supported server/container deployment shape, durable Postgres and object
  storage, required secrets and network boundaries, readiness, and how a team
  connects. It MUST distinguish the self-hosted enterprise topology from a
  separate edition. S3, GCS, and Azure choices MUST connect to their real
  configuration and credential behavior; an unshipped image, chart, or release
  workflow MUST be labeled unavailable rather than supplied as a fake command.
- **REQ-036**: The access-management journey MUST show the supported path to
  create or select a principal, issue and rotate or replace credentials, grant
  additional permissions to an existing principal, and verify the resulting
  access, including an under-privileged failure. It MUST explain the platform
  versus tenant authority boundary and avoid exposing real secrets in examples.
- **REQ-037**: The OIDC journey MUST show the supported human sign-in setup
  with Okta as a concrete provider example, including redirect URI, issuer and
  client configuration, claim/group mapping, role verification, and failure
  checks. It MUST distinguish human OIDC from workload federation and mark any
  provider-specific step that cannot be validated from Wyrd's own contract.
- **REQ-038**: The service-authoring journey MUST follow one coherent example
  with two or three Agents, versioned Prompts, a Service declaration, and
  telemetry linked to the exact declared components. It MUST show how the
  reader observes and verifies the result through supported interfaces. Each
  unshipped segment MUST be explicitly separated from runnable instructions.
- **REQ-039**: The model journey MUST show a minimal reproducible model-training
  step in a supported external framework, register its Model Card and artifacts
  through a supported client path, then show how a Service refers to that exact
  model version. The page MUST include an observable registration result and a
  way to verify the reference or state; Wyrd MUST NOT be described as the
  model-training runtime.
- **REQ-040**: The container operations journey MUST explain the current
  Kubernetes-first role-targeted topology for `wyrd-server`, Scribe, Oracle,
  and Forge, including one logical user-facing endpoint, HTTP versus gRPC
  routing, role activation, persistent versus scratch storage, health,
  readiness, TLS/auth boundaries, and container configuration. It MUST also
  explain the deployment contract for non-Kubernetes containers without
  claiming a finished chart or image that is not published.
- **REQ-041**: The automation journey MUST make the live OpenAPI document at
  `GET /openapi.json`, API and schema reference, authentication, stable errors,
  CLI, and MCP discoverable from task navigation. It MUST show one supported
  CI/CD integration path or, if that exact automation path has not shipped,
  state the supported API building blocks and the remaining gap. Swagger-style
  tooling MAY consume the OpenAPI document; a hosted Swagger UI MUST NOT be
  implied unless it exists.
- **REQ-042**: Every named journey MUST have a clear status of runnable,
  partially available, or planned based on current code, tests, published
  artifacts, and owning contracts. Runnable steps MUST use complete copyable
  commands or files and an expected result plus a verification action. Partly
  available journeys MUST put the supported portion first, name the missing
  capability at the decision point, and link to the closest real reference.
  Planned material MUST NOT masquerade as a completed task page in primary
  navigation or search.
- **REQ-043**: One representative service example SHOULD link the service,
  model, telemetry, verification, access, and deployment journeys where the
  current contracts permit it, so readers can build progressively without
  learning a new fictional domain on every page. Its declarations, field
  names, route names, and output MUST be checked against shipped Wyrd contracts.
- **REQ-044**: Documentation MUST remain useful as a headless developer
  surface: direct task links, visible search, generated schema/OpenAPI links,
  stable error guidance, and machine-readable indexes MUST lead to the same
  supported contracts. No procedure may require the workbench as its only
  completion path.

### Developer workbench

- **REQ-020**: The workbench MUST use an IDE-adjacent operating density: an
  approximately 30px page title, 14px body text, 11–12px metadata, 40px desktop
  controls, and approximately 52px evidence rows. Functional text MUST NOT be
  smaller than 11px.
- **REQ-021**: Every workbench page MUST establish one dominant current task or
  inspection region. Supporting context, raw identifiers, implementation
  detail, and provenance MUST remain subordinate until they are relevant or
  explicitly requested.
- **REQ-022**: The evidence-thread motif MAY connect real Declaration,
  Observation, Verifier, Judgment, principal, timestamp, and retained Evidence
  records. It MUST use canonical Wyrd vocabulary and real data, remain readable
  without the connector, and MUST NOT invent a new domain object or replace
  normal navigation.
- **REQ-023**: Existing workbench routes and workflows MUST retain their
  approved information architecture and domain behavior. Evidence Thread
  changes presentation and interaction hierarchy; it MUST NOT make the browser
  derive status, thresholds, judgments, lineage, authorization, or other
  server-owned truth.
- **REQ-024**: Status and stage MUST be distinct. A Declaration or Observation
  keeps its domain identity while success, warning, failure, running, or
  unavailable state is communicated through text or iconography as well as
  color.

### Responsive behavior, accessibility, and motion

- **REQ-025**: Documentation MUST remain fully usable from a 320px viewport.
  The workbench remains laptop/monitor-first but MUST preserve essential
  information and actions on narrow screens through stacking, accessible
  navigation, and controlled overflow rather than clipped or crushed data.
- **REQ-026**: Interactive targets on narrow screens MUST be at least 44px in
  either dimension where the target is not otherwise enlarged by spacing.
  Dense desktop controls MAY use the 40px operating target.
- **REQ-027**: Both surfaces MUST meet WCAG 2.2 AA for supported content and
  interaction. All intended foreground/background token pairs MUST be checked
  in both modes. Keyboard operation, visible focus, headings and landmarks,
  accessible names, reduced motion, zoom, and non-color state cues are part of
  the design contract.
- **REQ-028**: Routine feedback SHOULD complete in roughly 160ms and MUST not
  delay the underlying action. Evidence Thread MAY use one restrained custody-
  thread reveal where it explains a real causal sequence; decorative autoplay,
  looping motion, parallax, and motion required to understand state MUST NOT be
  used. Reduced-motion preference MUST remove nonessential movement.

### Migration and compatibility

- **REQ-029**: Documentation and workbench adoption MUST finish with one live
  Evidence Thread system. Superseded palettes, font roles, mark assets, arcade
  styling, thick brutalist geometry, and hard-shadow component variants MUST
  not remain as parallel production themes or per-route exceptions.
- **REQ-030**: Existing machine-readable component contracts, built component
  implementations, registry entries, and rendered design references MUST
  remain mutually consistent after migration. A token or variant removed by
  Evidence Thread MUST be migrated or removed rather than retained through an
  undocumented compatibility alias.
- **REQ-031**: Generated or projected theme artifacts MUST be reproducible from
  the canonical source, and drift MUST fail the repository's existing token
  check. No consumer may hand-edit a generated projection.
- **REQ-032**: Documentation and workbench copy, examples, labels, and evidence
  displays MUST project current Wyrd doctrine and typed server contracts. The
  visual redesign MUST NOT revive legacy names or imply capabilities that have
  not shipped.

## Externally observable behavior

| Situation | Required result |
|---|---|
| A first-time visitor opens docs or workbench in a dark-preferred system | The first meaningful paint is the complete Evidence Thread dark mode with no light-theme flash. |
| A user explicitly selects light mode and returns later | The same application opens in the Evidence Thread light mode with equivalent content and interaction. |
| A developer opens a task guide | The shortest supported path, expected output, and verification step are visible before advanced variants; search and task navigation remain obvious. |
| A developer copies a code sample | Copy is keyboard-operable and gives immediate textual or icon feedback; the code remains readable and horizontally scrollable. |
| A failed verification is inspected in the workbench | Declaration, observed value, exact Verifier, failed Judgment, and retained Evidence are distinguishable; failure is not encoded by red alone. |
| A user opens either surface on a narrow viewport | Navigation remains reachable, primary actions meet touch sizing, prose remains legible, and dense data stacks or scrolls without losing meaning. |
| Fonts are unavailable from the public internet | The intended bundled fonts still load; docs and workbench do not contact a third-party font host. |
| A user moves between docs and workbench | Logo, typography, palette semantics, geometry, focus, state, and evidence grammar remain recognizably one system while density adapts to reading versus operation. |
| A new developer asks what Wyrd does and how to start | The docs explain Declare → Observe → Verify in Wyrd vocabulary and lead directly to a verified local setup path. |
| A team operator needs a self-hosted deployment | The docs identify the supported container and storage configuration, one logical endpoint, required security and durability boundaries, and any release artifact that is not yet available. |
| A team operator needs to manage access or Okta sign-in | The docs give the supported credential, grant, and OIDC path with verification and failure guidance, and label unsupported provider-specific steps. |
| An AI engineer builds a service or a data scientist registers a model | The docs connect exact Card versions, Agents, Prompts, telemetry, observation, and verification through runnable supported steps and explicit capability gaps. |
| A DevOps engineer needs Kubernetes routing or CI/CD API integration | The docs expose current role topology, HTTP/gRPC entrypoints, configuration, live OpenAPI, and supported automation building blocks without inventing a published chart or hosted Swagger UI. |

## Invariants and prohibited outcomes

- **INV-001**: There is one authored semantic token authority for Wyrd docs
  and workbench, not two synchronized palettes.
- **INV-002**: The README Wyrd mark geometry is the Wyrd product identity; a
  Bohmian/Mitari mark, wordmark treatment, or evidence-thread illustration
  cannot replace it in Wyrd product chrome or favicons.
- **INV-003**: Lime always communicates retained Evidence or direct evidence-
  retention feedback; it is not a general accent, background wash, or hover
  color.
- **INV-004**: No judgment, status, stage, chart series, threshold, or
  interaction state relies on color alone.
- **INV-005**: Light and dark modes expose the same information, hierarchy,
  actions, and accessibility semantics.
- **INV-006**: The design system never becomes a source of domain truth or a
  reason to duplicate server-owned calculations and contracts in the browser.
- **INV-007**: Primary documentation navigation stays task-first and does not
  become role-first, crate-first, or product-inventory-first.
- **INV-008**: Critical prerequisites, security guidance, destructive warnings,
  and unsupported-state labels are never hidden behind optional disclosure.
- **INV-009**: Production brand assets and fonts require no runtime dependency
  on a third-party service.
- **INV-010**: Evidence Thread remains scientific and technical. Police tape,
  fingerprints, legal seals, surveillance motifs, fantasy manuscripts,
  decorative accession numbers, and compliance theater are prohibited.
- **INV-011**: A developer journey is never declared runnable merely because
  an architecture document or design mock describes it; executable examples
  require a shipped implementation or artifact and a credible verification
  path.
- **INV-012**: Primary docs navigation remains centered on local and
  self-hosted user tasks. Unsupported tasks are discoverable through explicit
  status and reference links, not presented as successful procedures.

## Material decisions and required boundaries

1. **Canonical owner**: the existing workbench `brand/` directory remains the
   production authority because it already owns machine-readable tokens,
   component contracts, generation, and token drift checks. Documentation is a
   consumer of generated projections, not a peer palette owner.
2. **Surface adaptation**: documentation and workbench share identity,
   semantics, and component states but retain different density and layout
   composition for reading and operating.
3. **Asset delivery**: logo and fonts are repository/distribution assets.
   Runtime third-party font loading is not an accepted dependency for local or
   self-hosted Wyrd.
4. **Public marketing**: the broader Evidence Thread exploration remains
   compatible with later public-site adoption, but public marketing delivery
   is outside this change.
5. **Existing UI specification**: when this specification is approved, it
   narrowly supersedes the visual particulars in
   `SPEC-wyrd-ui-foundation` `REQ-066`—the old palette, five-pixel geometry,
   hard shadows, and old mark treatment. It preserves that requirement's
   `brand/` ownership, contrast enforcement, and component-contract authority.
   It also supersedes only the old geometry implied by `REQ-081`; the same-
   structure and equal light/dark obligations remain in force. All other
   approved UI-foundation behavior remains unchanged.
6. **No compatibility skin**: migration completes on Evidence Thread. The old
   arcade and hard-shadow themes do not remain selectable or silently retained
   as a second visual system.
7. **Documentation is the main deliverable**: this revision extends the
   approved visual foundation to the requested complete developer task map.
   Every named journey receives an honest answer; a missing product capability
   is documented as a gap, not silently implemented or invented by the docs.
8. **Deployment framing**: local and self-hosted deployments are the only
   procedural scope. Kubernetes is the primary container example, with the
   same logical server boundary explained for other container platforms.
   Enterprise means a self-hosted team topology, not a separate product.

## Acceptance obligations

- **AC-001**: Canonical-source and drift evidence proves that both production
  applications receive their semantic tokens from the same brand authority,
  including a passing `mise run check:tokens` result and no independently
  authored consumer palette.
- **AC-002**: Reviewable desktop and narrow-viewport renderings cover a
  representative documentation home/task page and a representative workbench
  evidence/verification page in both light and dark modes. The matrix proves
  shared identity, surface-specific density, responsive behavior, and the
  absence of full-screen accent fields and superseded styling.
- **AC-003**: Accessibility evidence covers automated contrast for every used
  token pair in both modes plus keyboard navigation, visible focus, landmarks,
  headings, theme control, search, disclosures, code copy, status cues, 200%
  zoom, narrow-screen operation, and reduced motion. Documentation evidence
  includes a passing `mise run docs:check` result.
- **AC-004**: Theme tests prove system-preference initialization, persisted
  explicit choice, correct pre-paint application, native color scheme, and
  information parity for both applications.
- **AC-005**: Static and built-asset evidence proves that Familjen Grotesk,
  Fragment Mono, and every Wyrd logo/favicon use are shipped locally; no
  production page requests Google Fonts or another third-party brand asset.
- **AC-006**: Documentation review proves that primary navigation and every
  runnable named task journey are task-first, progressively disclosed, and show
  expected output plus verification without hiding critical prerequisites or
  warnings.
- **AC-007**: Workbench component and route evidence proves canonical styling
  for normal, hover, focus, active, selected, disabled, loading, empty,
  unauthorized, warning, failure, and success states without browser-derived
  domain truth or color-only meaning.
- **AC-008**: Component-contract, registry-parity, Svelte type/build, and
  focused component tests pass for the touched workbench surface. Generated
  docs assets, links, search output, code rendering, Svelte checks, and the
  production static build pass for the touched documentation surface.
- **AC-009**: Migration evidence shows no production reference to the retired
  arcade/pixel fonts, Bohmian Wyrd chrome mark, old warm-paper palette,
  five-pixel hard-shadow geometry, or runtime Google Fonts import remains in
  docs or workbench.
- **AC-010**: A journey inventory maps each of REQ-033–041 to its owning
  task page or reference, current runnable/partial/planned status, source or
  test evidence, expected result, and verification action. Primary navigation
  has a direct route to every supported journey without role selection.
- **AC-011**: The local server, access management, OIDC/Okta, service/telemetry,
  model registration, self-hosting/storage, container routing, and API/CI
  journeys are each reviewed from the named user's starting question through
  the supported result or an explicit capability boundary. At least the
  locally supported paths are executed or verified against current source,
  tests, and repository-managed environments; no fake commands or unstated
  prerequisites remain.
- **AC-012**: The docs home and task-first navigation lead to a coherent
  Declare → Observe → Verify learning path and directly to OpenAPI, schemas,
  CLI, MCP, and error reference. Generated docs, links, search output,
  machine-readable indexes, and the static site pass `mise run docs:check`.
- **AC-013**: The Kubernetes/container guidance is checked against the current
  role-target, gateway, storage, and deployment authorities. Any published
  image/manifest command is backed by an actual artifact and can be verified;
  absent packaging is identified plainly with the operator's next supported
  step.
- **AC-014**: The revised docs and workbench are inspected together in light
  and dark mode at desktop and narrow widths, proving one visual language
  across the complete docs journey and representative workbench evidence view.
- **AC-015**: A first-time developer review confirms that the home and local
  quickstart expose one obvious starting action, one short successful path,
  and a small set of contextual next tasks. Readers can find the team,
  service, model, and API journeys through navigation or search without an
  up-front feature inventory or unnecessary prerequisite reading.

## Open material decisions

None. The user has made the complete developer docsite the primary outcome.
Evidence Thread remains the approved shared visual system. The requested
journeys are fixed; current implementation evidence determines whether each
can be documented as runnable or must show its precise capability boundary.

## Revision history

- **Revision 2 — 2026-09-24 — approved**: Corrected the omitted primary outcome:
  build the developer docsite across the user's named local, self-hosted,
  identity, service, model, Kubernetes, and API journeys. Added requirements
  and acceptance for truthful task coverage, runnable verification, clear
  unsupported-state handling, and progressively disclosed journeys that keep
  the first successful path short. Explicitly approved by the user on
  2026-09-24; tasks written for revision 1 do not cover the expanded scope.
- **Revision 1 — 2026-09-24 — approved**: Captured the locked Evidence Thread
  direction as one production design contract for developer documentation and
  the workbench; fixed its semantic palette, type, mark, density, theming,
  progressive disclosure, accessibility, canonical token ownership, migration
  boundary, and narrow supersession of the previous UI visual theme. Explicitly
  approved by the user on 2026-09-24.

## Material authority and evidence links

- [`AGENTS.md`](../../../AGENTS.md)
- [`architecture/agent-rules.md`](../../../architecture/agent-rules.md)
- [`architecture/wyrd-design.md`](../../../architecture/wyrd-design.md)
- [`architecture/wyrd-doctrine.mdx`](../../../architecture/wyrd-doctrine.mdx)
- [`architecture/references/doctrine/architecture-constraints.md`](../../../architecture/references/doctrine/architecture-constraints.md)
- [`architecture/references/doctrine/positioning-and-vocabulary.md`](../../../architecture/references/doctrine/positioning-and-vocabulary.md)
- [`architecture/references/architecture/patterns.md`](../../../architecture/references/architecture/patterns.md)
- [`architecture/references/languages/spec-driven-development.md`](../../../architecture/references/languages/spec-driven-development.md)
- [`architecture/references/languages/testing-workflows.md`](../../../architecture/references/languages/testing-workflows.md)
- [`changes/active/wyrd-ui-foundation/spec.md`](../wyrd-ui-foundation/spec.md)
- [`crates/wyrd/wyrd-server/wyrd-ui/brand/DESIGN.md`](../../../crates/wyrd/wyrd-server/wyrd-ui/brand/DESIGN.md)
- [Evidence Thread design decision](../../../.dev/docs-refactor/brand-variations/decision.json)
- [Evidence Thread research direction](../../../.dev/docs-refactor/research-and-direction.md)
- [Evidence Thread docs/workbench matrix](../../../.dev/docs-refactor/brand-variations/matrices/evidence-thread.html)
