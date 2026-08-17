---
name: wyrd-plan-v3
description: Produce decision-complete Wyrd implementation plans and executable task packets with a digest-bound execution manifest. Use for planning Wyrd features, refactors, migrations, contracts, SDKs, server, CLI, MCP, storage, Vala, or cross-language work that must be ready for delegated implementation.
---

# Wyrd Plan V3

Create one self-contained plan directory containing `plan.md`, numbered
`tasks/<NN>-<slug>.md` packets, and `execution-manifest.json`. Do not invoke or
depend on any v1 or v2 planning or rehearsal skill.

Run this root planning invocation only as `gpt-5.6-sol` with `low` reasoning.
If the active root does not satisfy that identity, stop before investigation.

## Authorities and investigation

Read `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`,
and `architecture/wyrd-doctrine.mdx`. Inspect current source through CodeGraph
first when indexed, then manifests, tests, `mise.toml`, and generated surfaces.
Record the repository origin, exact revision, and evidence digest. Resolve every
material unknown before approval; mark the plan `Draft` when authority is
missing.

## Canonical artifacts

Write `plan.md` in the validator's canonical heading order: Objective; Current
state and evidence; Requirements; Non-goals; Constraints; Architecture and
design decisions; Domain and data contracts; Interfaces and function contracts;
Control flow and pseudocode; Failure and edge-case matrix; Milestones; Task
inventory; Global acceptance criteria; Verification strategy; Closeout
verification; Risks, migration, and rollout; Execution handoff.

Write one task packet per cohesive outcome. Each packet must state:

- exact allowed write paths and target symbols;
- why every path and symbol changes;
- concrete typed contracts, signatures, ownership, visibility, and errors;
- ordered control flow and IO/transaction boundaries;
- lifecycle, cancellation, concurrency, retry, recovery, and partial-progress
  behavior, or an explicit evidence-backed statement that each is inapplicable;
- requirement and decision IDs, dependencies, prohibited scope, and escalation
  boundaries;
- `AC<N>` criteria mapped to a named test and one exact command;
- command preflight evidence including the honest selected count under the
  existing/new test rules below.

Use the complete ordered task headings enforced by the validator: Objective;
User and operator value; Current behavior and evidence; Required changes;
Non-goals; Allowed scope; Prohibited changes; Target paths, symbols, callers,
and consumers; Required types and interfaces; Implementation guidance; Control
flow and pseudocode; Failure and edge-case matrix; Allocation and lifecycle
contract; Acceptance criteria; Required tests; Required features; Focused
verification; Concurrency and integration locks; Commands explicitly excluded;
Stop and escalate if; Completion evidence.

In the target section, use the canonical table columns `Path`, `Path kind`,
`Symbol`, `Symbol kind`, `Why`, `Callers`, and `Consumers`. Mirror every
manifest path/symbol/why exactly. Name direct callers and consumers; when none
exist, cite the repository evidence that proves none. New or cross-owner symbols
require a typed signature block. Stateful tasks require at least one exact
failure-matrix row and a complete allocation/lifecycle contract.

Never use “update as needed”, “wire it up”, “add tests”, “follow existing
patterns”, TBD, TODO, or equivalent placeholders in an approved packet. A
precedent is useful only when its exact path and symbol are named and the
required delta is fixed.

Keep every normative product and implementation contract in the canonical plan
and packets. Create `execution-manifest.json` using
[`references/execution-manifest.md`](references/execution-manifest.md). The
manifest carries scheduling/proof identity only: IDs, digest bindings, DAG,
categorized locks, write identity, proof evidence references, and delegated
role configuration. It must not duplicate normative requirements, decisions,
contracts, lifecycle semantics, or acceptance text. All delegated roles use
exactly `gpt-5.6-sol` with `low` reasoning.

## Decomposition and proof

Prefer vertical tasks that leave the repository coherent. A task must never
need an uncommitted peer implementation: express the producer as a dependency
and lock its consumed contract. Parallel tasks require disjoint write sets and
must not share generated artifacts, migrations, manifests, lockfiles, or mutable
fixtures. Serialize all Cargo-backed verification.

Map every requirement to at least one task and every acceptance criterion to a
named test. Use the highest Wyrd test tier required by `AGENTS.md`. New public
behavior requires a real client-to-server journey. Postgres tests must use an
established repository fixture or mise-owned setup; task packets must not
construct raw pools or invent database lifecycle.

Preflight each exact command locally and serially against the pinned revision.
Capture command, output, exit status, and selected count in a digest-bound
evidence JSON file. Existing tests require a positive executed selection. A
newly planned test declares `test_kind: new`, its exact path/name and expected
post-implementation count, and preflights the existing package/target with
`--no-run` and selected count zero; never fabricate a positive count. The
manifest's zero-to-two Cargo lanes constrain execution
only; they never authorize parallel preflight.

## Validation and approval

Compute SHA-256 digests after all three artifacts are final, place plan/task
digests in the manifest, then run:

```bash
python3 .agents/skills/wyrd-plan-v3/scripts/validate_v3_artifacts.py <plan-directory> --repository-root <repository-root>
```

Run cold rehearsal only after structural validation. Store its canonical
`rehearsal.json` beside but outside the manifest's digested artifact set. A plan becomes `Approved`
and tasks become `Ready` only when the current digest-bound rehearsal returns
`PASS`. Any material contract, semantic lock, task dependency, write set,
acceptance proof, or source revision change invalidates that verdict.
