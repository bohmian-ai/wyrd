# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

The primary users are developers, data scientists, and AI engineers building,
registering, observing, and integrating AI systems with Wyrd.

Platform and DevOps leads are operational users responsible for self-hosted
deployment, identity, authorization, credentials, networking, and team access.

## Product Purpose

Wyrd is open-source verification and assurance infrastructure for AI systems.
It lets teams declare exact, versioned system components and expectations,
observe real behavior, verify that behavior, and retain attributable evidence.

The developer documentation succeeds when a user can understand Wyrd and
complete a real local or self-hosted workflow without reconstructing the path
from architecture documents, source code, or disconnected reference pages.

## Positioning

Wyrd evaluates observed AI-system behavior against declared expectations for
exact component versions. It preserves the lineage and evidence behind those
judgments without becoming the user's application runtime, training loop,
workflow engine, or arbitrary code-execution framework.

## Operating Context

The documentation supports these durable user workflows:

- Understand what Wyrd is and where it fits in an AI system.
- Run the Wyrd server for local development.
- Deploy and configure Wyrd for a self-hosted enterprise team.
- Provision, update, rotate, and revoke credentials and permissions.
- Add permissions to an existing principal.
- Configure OIDC, using Okta as the primary worked example.
- Declare a service composed of agents, prompts, models, and telemetry.
- Train and register a model, then reference its exact version from a service.
- Deploy Wyrd on Kubernetes with independently operated server and Bifrost
  components while exposing one developer-facing endpoint.
- Use OpenAPI, HTTP, CLI, SDK, and machine-facing contracts in CI/CD workflows.

The current documentation scope is local development and self-hosted operation.
Hosted Wyrd SaaS workflows are out of scope until explicitly added.

## Capabilities and Constraints

- Wyrd uses a language-agnostic client/server model. Durable behavior belongs
  to the Rust server; clients project the server contract.
- Rust, Python, and TypeScript are first-class client languages. HTTP, CLI,
  MCP, generated schemas, and stable errors are primary headless surfaces.
- Cards and Specs declare exact components. Runs and Observations record
  measured behavior. Verifiers produce judgments. Lineage and Bifrost retain
  and query evidence. Policy governs, Audit records accountability, and
  Operators react.
- The memorable product model is Declare → Observe → Verify. Documentation may
  explain bindings, retained evidence, governance, and reaction where needed.
- User-facing guidance is organized around complete developer tasks rather
  than internal crate or service ownership.
- Operational documentation may expose server, Scribe, Oracle, and Forge when
  operators must configure their deployment topology. These names do not
  become separate public client APIs.
- A published task must describe an executable workflow and show how the user
  confirms success. Planned behavior does not occupy primary task navigation.
- The existing SvelteKit, mdsvex, Pagefind, Shiki, and static-site toolchain is
  the implementation foundation unless a later approved decision replaces it.

## Brand Commitments

- Wyrd uses the approved brand authority under
  `crates/wyrd/wyrd-server/wyrd-ui/brand`.
- Documentation must remain recognizably Wyrd rather than a generic developer
  portal.
- Restrained editorial or technical-manual nostalgia is acceptable. Arcade,
  CRT, pixel-font, scanline, glow, and terminal-cosplay treatments are not.
- The product voice is direct, technically precise, calm, and honest about
  prerequisites, trust boundaries, and capability maturity.

## Evidence on Hand

- Protocol and product authority: `architecture/wyrd-design.md` and
  `architecture/wyrd-doctrine.mdx`.
- Repository contribution and contract rules: `AGENTS.md` and
  `architecture/agent-rules.md`.
- Approved brand assets and constraints:
  `crates/wyrd/wyrd-server/wyrd-ui/brand`.
- Existing documentation implementation and content: `docs/`.
- Real server, CLI, SDK, Bifrost, authentication, storage, telemetry, query,
  and deployment-contract tests throughout the repository.
- No customer testimonials, adoption claims, performance claims, or production
  deployment claims may be invented when evidence is absent.

## Product Principles

1. Lead with the user's job, not Wyrd's internal inventory.
2. Teach through complete, executable journeys with visible successful results.
3. Reuse one coherent system example across roles, languages, and interfaces.
4. Keep concepts concise and put exhaustive machinery in searchable reference.
5. Project one contract consistently across human and agent-facing surfaces.

## Accessibility & Inclusion

The documentation must meet WCAG 2.2 AA. Navigation, search, tabs, disclosures,
code samples, diagrams, status, responsive layouts, keyboard interaction, and
focus behavior must remain understandable without relying on color alone.
