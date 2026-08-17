---
name: wyrd-review-and-plan-v3
description: Run the terminal v3 static review of one committed Wyrd target against one committed base, validate specialist findings against source, publish a consolidated review, and route confirmed remediation only through wyrd-plan-v3. Use for final, holistic, merge-readiness, or post-plan implementation review; never for working-tree or per-task review.
---

# Wyrd Review And Plan V3

Run as a Sol-low root orchestrator. Review exactly one immutable committed
target SHA against one immutable committed base SHA, optionally using an
approved Wyrd plan as intent. Never modify product source or run project tests,
builds, formatters, linters, generators, migrations, services, or release
commands. State prominently: `Static analysis: no runtime verification
performed.` Never invoke v1 or v2 review or planning skills.

## Establish the review

Resolve and record base and target SHAs before dispatch. When invoked by the
v3 controller, also resolve the supplied hash-chained controller-state event
identity and require every specialist result to echo it exactly; reject review
evidence bound to another snapshot. Reject a dirty or moving target. Read
`AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, applicable
architecture references, manifests, generated-contract owners, and the
caller-supplied intent. Use CodeGraph first when `.codegraph/` exists.

Derive the impact graph from the committed diff: changed owners, callers,
consumers, public projections, persistence and lifecycle paths, tests,
manifests, generated artifacts, deployment surfaces, and release gates. Treat
recorded verification as untrusted evidence to audit semantically, never as
permission to rerun it.

## Dispatch exactly five specialists

Dispatch exactly five independent fresh Sol-low specialists. Each receives the
same immutable base and target SHAs, impact graph, authorities, intent, and a
disjoint primary domain. Include the exact controller-state event hash when
supplied. All are read-only and return candidate findings plus clean coverage,
the strongest counterexample attempted, and the same state identity.

1. `correctness-lifecycle`: behavior, state transitions, concurrency,
   cancellation, recovery, cleanup, and performance-critical lifecycle paths.
2. `security-tenancy-audit-errors`: authn/authz, RLS and tenant isolation,
   secrets, audit commit boundaries, stable public errors, unsafe inputs, and
   abuse paths.
3. `architecture-contracts`: Wyrd doctrine, ownership, struct-centered Rust,
   SDK/HTTP/MCP/CLI/UI projections, durable contracts, dependencies, and
   unnecessary complexity.
4. `tests-verification`: requirement and acceptance traceability, journey and
   negative coverage, assertion strength, lane correctness, and whether
   recorded commands prove the exact candidate SHA.
5. `build-generated-deployment-release`: manifests, feature cones, lockfiles,
   generated drift, packaging, migrations, topology, configuration, CI, and
   release compatibility.

Do not combine, omit, or add baseline roles. Specialists may overlap evidence
when a cross-cutting invariant requires it, but each candidate finding names
its primary owner and affected consumers. They do not assign final IDs, write
the consolidated review, invoke planning, or communicate with the user.

## Validate and consolidate

The root independently validates every candidate against source at the target
SHA. Reconstruct the current flow, identify the concrete failure scenario,
inspect callers, consumers, tests, and local precedent, and reject
preference-only, speculative, duplicate, stale, or unproved claims. Deduplicate
only findings with the same root cause and correction.

For every confirmed finding record:

- severity and stable `REV-NNN` identifier;
- violated authority, requirement, or invariant;
- exact source evidence and affected owners/consumers;
- reproducible failure scenario and user, agent, security, or operational
  consequence;
- bounded required outcome, without prescribing private helper shapes;
- exact regression assertion and verification gate that proves closure.

Record clean coverage for every specialist and material static-analysis
limits. A missing specialist, unresolved SHA, unreadable mandatory authority,
or unvalidated candidate blocks merge-readiness. A clean review requires zero
confirmed required findings, not merely zero specialist reports.

## Persist and route

Write one authoritative review artifact in the caller-selected review
directory. Include resolved SHAs, intent, impact graph, five-role coverage,
validated findings, rejected-candidate dispositions, static limits, and final
verdict. Do not mutate the reviewed branch or intent artifact.

When confirmed required findings remain, invoke only `$wyrd-plan-v3` to create
or revise canonical remediation planning. Give it the consolidated findings
and exact committed SHAs. Do not create a review-private task format and do not
continue into implementation. Do not plan clean, informational, deferred, or
authority-blocked findings.

Return the consolidated review and any canonical v3 remediation plan. The
terminal verdict is `CLEAN`, `REMEDIATION_REQUIRED`, or `REVIEW_BLOCKED`.
