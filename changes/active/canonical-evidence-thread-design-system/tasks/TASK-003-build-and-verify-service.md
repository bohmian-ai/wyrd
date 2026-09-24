---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-canonical-evidence-thread-design-system
spec_revision: 2
requirements: [REQ-013, REQ-015, REQ-016, REQ-019, REQ-032, REQ-038, REQ-039, REQ-042, REQ-043, REQ-044, INV-007, INV-008, INV-011, INV-012, AC-006, AC-010, AC-011, AC-012, AC-015]
depends_on: [TASK-002]
parent_task:
remediates: []
---

## Outcome and Value

An AI engineer can follow one coherent service example with two or three Agents, versioned Prompts, telemetry, observation, and verification. A data scientist can train a small model in an external framework, register its exact version and artifacts, and use that version in the Service. Each step leads to an observable result or a precise current capability boundary.

## Owners, Scope, Consumers, and Prohibited Changes

`docs/src/content/docs/` owns these task guides and their directly linked reference explanations. Inspect the real SDK examples, Card contracts, server routes, and journey tests before presenting any step as runnable. Use TASK-002's task-page pattern and navigation. Wyrd does not train models or run arbitrary user services; do not add runtime code, invent registration methods or verifier results, or turn a future capability into a copyable success path. Keep Python, Rust, and TypeScript claims proportional to actual support; the canonical example may use the shortest verified client path.

## Approach

1. Trace the current model, Prompt, Agent, Service, telemetry, Observation, and Verifier contracts and their shipped end-to-end examples.
2. Select one credible service story that can connect exact component versions across the journeys without changing domain vocabulary.
3. Write the shortest supported model training/registration and service declaration paths, each with expected result and verification.
4. Add observation and verification steps only where current user-facing behavior supports them; put unsupported transitions at their decision points with the nearest real reference.
5. Link the journeys contextually from the site and complete their capability-status entries in TASK-002's inventory.

## Proof Strategy

This task authors documentation and examples derived from already-shipped behavior; it does not change executable behavior, so TDD is not applicable. Inspect current SDK, server, schema, and existing user-journey owners; execute or reuse the narrowest repository-managed example/journey proof for each step marked runnable. Static review checks Card shapes, versions, command output, links, disclosure order, and honest capability status. If implementation discovers that a public capability is missing, document the boundary; backend implementation requires a separate approved change.

## Acceptance Criteria

- The reader can identify the exact Service, Agent, Prompt, and Model Card versions and follow the supported path between them without being sent through many tiny prerequisite pages.
- The model guide shows a minimal external training action, supported registration, observable receipt/state, and exact Service reference; it does not imply Wyrd owns training.
- The service guide includes two or three Agents and real telemetry/observation/verification steps when shipped; every unsupported link in the chain is named plainly.
- Expected results and verification appear before variants; links to schemas and reference use current Wyrd vocabulary and typed fields.
- The home/sidebar/search expose these tasks contextually rather than as an up-front feature inventory, and the journey inventory reflects actual runnable/partial/planned status.

## Expected Write Set and Consumer Closure

Likely service/model/observe/verify guides under `docs/src/content/docs/`, with direct updates to existing how-to and concept pages whose current claims conflict with the verified path. Reuse existing examples where correct. If an example file itself needs correction, its owning example package and applicable example gate join the write/verification set; no server or SDK feature work is authorized here.

## Verification and Evidence

- `mise run docs:check` covers generated pages, public commands, links, static build, search, and site accessibility.
- Run the narrowest existing repository-managed model-registration and Card/observation journey or example lanes that correspond to each runnable claim; record their command and observed result in the task evidence. Run `mise run check:examples` if shared example behavior changes.
- Review rendered task pages at desktop/mobile and in both modes; confirm critical unsupported transitions are visible without optional disclosure.

## Material Stop Conditions

Stop if a required runnable example needs a new SDK/server contract, new Card kind, or browser-derived verification truth. A docs page can state the gap but cannot silently implement the missing product behavior.

## Authority Links

- [Approved specification](../spec.md)
- [Repository instructions](../../../../AGENTS.md)
- [Design doctrine](../../../../architecture/wyrd-design.md)
- [Wyrd doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)
