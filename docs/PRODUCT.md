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

Wyrd is the only real-time verification system built for your applications. It
answers the developer's question: "You built the system. How do you know it's
behaving correctly?" Before an application acts, it asks Wyrd whether the
action meets the requirements its developers declared; Wyrd returns a verdict
for each requirement, and the application decides. Wyrd is open-source
building blocks for that verification: Declare. Observe. Verify.

The developer documentation succeeds when a user can understand Wyrd, run the
finance release Quickstart, and learn to build their own verification without
reconstructing the path from architecture documents, source code, or
disconnected reference pages.

## Positioning

Public entry points (README, homepage, Introduction) open with the exact
headline and question above, then show working proof before infrastructure:
an application verifying a report release against three requirements, one of
them a model judging a policy PDF, and releasing only on a passed verdict.
Gateway, Bifrost, and deployment detail follow that proof.

Wyrd verifies that an AI system behaves as declared, whether it is an
individual LLM, a deployed agent service, or a traditional HTTP service, before
and after deployment. Every verdict records what was checked, which standard
applied, and the exact versions that ran. The application owns the decision a
verdict informs; Wyrd never sends or blocks anything itself. Wyrd does not
become the user's application runtime, training loop, workflow engine, or
arbitrary code-execution framework.

## Operating Context

The documentation supports these durable user workflows:

- Understand what Wyrd is and where it fits in an AI system.
- Verify an application action in real time and act on the verdict.
- Run the Wyrd server for local development.
- Deploy and configure Wyrd for a self-hosted enterprise team.
- Provision, update, rotate, and revoke credentials and permissions.
- Add permissions to an existing principal.
- Configure OIDC, using Okta as the primary worked example.
- Declare a service as a set of versioned components.
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
- The product model is Declare → Observe → Verify: declare a service, observe
  its behavior, and verify it behaves as intended. Documentation may
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

- Protocol and product authority: `architecture/wyrd-design.md`.
- Repository contribution and contract rules: `AGENTS.md` and
  `agent_docs/`.
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
3. Teach each task with a small, independently runnable example under
   `examples/`, extending the Quickstart Service where it fits. Full
   applications are optional further reading, never a prerequisite.
4. Keep concepts concise and put exhaustive machinery in searchable reference.
5. Project one contract consistently across human and agent-facing surfaces.

## Accessibility & Inclusion

The documentation must meet WCAG 2.2 AA. Navigation, search, tabs, disclosures,
code samples, diagrams, status, responsive layouts, keyboard interaction, and
focus behavior must remain understandable without relying on color alone.
