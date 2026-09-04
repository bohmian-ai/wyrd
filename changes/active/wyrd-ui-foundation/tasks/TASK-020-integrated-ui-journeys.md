---
id: TASK-020
title: Integrated UI route and workflow closure
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-060, REQ-066, REQ-068, REQ-069, REQ-070, REQ-075, REQ-076, REQ-080, REQ-081, REQ-082, REQ-083, REQ-084, REQ-090, REQ-092, REQ-093, REQ-094, REQ-095, REQ-096, REQ-097, REQ-098, REQ-099, REQ-100, REQ-101, REQ-102, REQ-103, REQ-104, REQ-105, REQ-106, REQ-127, REQ-128, REQ-129, REQ-130, REQ-131, REQ-132, INV-001, INV-002, INV-008, INV-009, INV-012, INV-013, INV-014, INV-015, INV-016, INV-017, INV-018, INV-019, INV-020, INV-021, INV-022, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011]
depends_on: [TASK-004, TASK-005, TASK-007, TASK-008, TASK-009, TASK-010, TASK-011, TASK-012, TASK-013, TASK-014, TASK-015, TASK-016, TASK-017, TASK-019]
parent_task:
remediates: []
---

# Outcome and value

Prove the mock-backed product works as one tenant-safe UI—not isolated screens—
and close every specification obligation with route, interaction, visual,
session, component-reuse, and deployment evidence.

# Owner and write set

- Own cross-feature journey fixtures/scripts and the smallest corrections at an
  already approved seam. Feature redesign returns to its owning task/spec.
- Exercise the production adapter-node build through real page loads/actions and
  the Nginx/container boundary with an opaque local session.
- Audit every canonical route, direct link, URL filter/local view, cross-area
  handoff, error boundary, theme, and responsive pattern against cumulative
  TASK-001 remediation mocks.
- Produce the final component-to-consumer/catalog-boundary evidence required by
  AC-010 and the distinct-workspace evidence required by AC-011.

# Integrated journeys

1. Authenticate, resolve/switch tenant safely, and use Home with no credential
   leakage.
2. Create/resume and review a multi-subject Change Request through verification,
   discussion, timeline, and read-only source drilldown.
3. Investigate Traces → Metrics → Logs with stable filters; open trace detail;
   continue from Service, Eval, and Drift links and clear inherited scope.
4. Search Cards and visit Service, Experiment, Model, Agent, Prompt, Drift, Eval,
   Data, Operator, Trigger, Workflow, Verifier, and generic fallback through the
   same route host with exact identity and restorable local state.
5. Transfer Observe context into Query; run/cancel mocked reads and inspect
   success/error/details/history without client-side authorization.
6. Repeat representative journeys in light/dark and desktop/mobile containers,
   checking keyboard flow, accessible names, overflow, and non-color meaning.
7. Reach the same surfaces through the one production gateway with target-aware
   health and safe HTTP/gRPC/error/stream behavior.

# Red-Green-Refactor

Add one cross-feature journey at a time and confirm failure at the seam. Correct
the owning seam only, then rerun all earlier journeys; do not create a second
integration abstraction or browser-test dependency.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/journeys/WyrdUiJourney.test.ts
mise exec -- node crates/wyrd/wyrd-server/wyrd-ui/scripts/route-journey.mjs
mise exec -- bash scripts/checks/gateway-topology.sh all
mise exec -- bash scripts/checks/gateway-topology.sh split
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
mise run docker:build
```

`route-journey.mjs` starts the built adapter-node app on an ephemeral local port,
drives the cookie plus real load/action HTTP boundaries, and always terminates
the child process. Add no browser automation dependency.

# Evidence and stop conditions

Record route/state matrices, exact command results, component reuse, paired
responsive captures, and gateway smoke evidence. Stop on any missing durable
backend contract, security-boundary conflict, or mock-derived product truth and
return it to `$wyrd-spec`; do not weaken a gate or conceal a gap.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
