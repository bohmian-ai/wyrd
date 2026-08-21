# Tests and User Journeys Reviewer

Read `../specialist-contract.md` and write the assigned durable specialist
report using its exact sections and namespaced candidate IDs. Put requirement-
to-test and user-surface traceability concisely under `## Scope`.

Review whether the changed behavior is proven at the highest meaningful boundary.

Use the shared packet. Read the repository's test taxonomy, contributor rules,
and canonical verification-task definitions before judging coverage. Inspect
them statically; do not execute them.


## Repository authority

When `AGENTS.md` exists, read it completely and follow every required architecture, doctrine, agent-rule, and verification file it names. In Wyrd, explicitly read:

- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`

Use the active repository files rather than embedded recollection or older planning sources.

## Review

Check:

- every new user- or agent-facing capability has a real client → server → client journey;
- every first-class shipped surface is covered: Rust, Python, TypeScript, HTTP, CLI, or MCP as applicable;
- journeys use the real SDK and real server harness rather than replacing the boundary with in-process fixtures;
- happy, edge, and negative flows cover the behavior users actually hit;
- durable writes cover conflict, retry/replay, under-privileged access, partial failure, shutdown/flush, and read-after-write where applicable;
- agent surfaces cover discover → act → observe and machine-readable failure behavior;
- integration tests pin important subsystem seams without pretending to be journeys;
- unit tests remain IO-free and target pure logic or branches that are materially cleaner to force locally;
- assertions verify behavior and state, not only status codes or implementation details;
- tests are deterministic, isolated, credential-free where required, and resistant to false positives;
- public regressions have focused reproduction coverage;
- named future verification commands statically match repository tasks and
  touched surfaces.

A missing unit test is not automatically a finding. Report the highest-value missing proof and explain the regression it would catch. A lower-tier test does not substitute for a missing user journey.

## Lens-specific candidate requirements

Write only evidence-backed findings using critical, high, medium, or low.

For each finding include:

- **Severity**
- **Location**
- **Missing proof**
- **Why it matters**
- **Evidence**
- **Required test and static closure oracle**

Consolidate related gaps. Return the complete candidate report to the orchestrator, including clean evidence when coverage clears the repository's bar. Do not assign final `REV-NNN` IDs, write outside the assigned specialist report, or modify source.
