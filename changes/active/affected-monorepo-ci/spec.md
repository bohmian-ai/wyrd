---
id: SPEC-affected-monorepo-ci
revision: 1
status: approved
---

# Dependency-aware monorepo CI and release builds

## Objective and user value

Give contributors fast, trustworthy feedback as Wyrd adds crates, SDKs, and
services. A routine change should run the checks that can detect its effects,
including tests in consumers of changed code, without paying for unrelated
families. Broad verification remains available where selective proof is unsafe
and before a release. Release builds should spend time on changed deliverables
while retaining Wyrd's immutable, verifiable release contract.

## Current behavior

- Change detection selects jobs by path. A generic Rust change runs workspace
  checks and the complete Rust family and storage-emulator aggregate. Bifrost
  has a dedicated capability route; global, mixed, and unclassified changes
  run the complete repository gate.
- Rust, Python, and TypeScript SDK jobs cover Linux and macOS where their
  current workflow selects them. The full gate also runs nightly. Credentialed
  real-cloud storage tests run separately after pushes to `main`.
- The release workflow builds all platform packages on every `main` push and
  publishes selected artifacts when a release is published.

## Scope and definitions

An **affected component** is a changed workspace package or deliverable plus
the components that consume its contracts or behavior. Rust dependency edges
are only part of this relationship: generated schemas, language bindings,
wire contracts, runtime integration, and service journeys also connect
components. An **unknown change** is one whose impact cannot be classified
with enough confidence to choose a smaller verification set.

This change covers pull-request and `main` CI selection, Rust test execution,
cross-language and service verification routing, cache and timing visibility,
and selection of routine release dry-run builds. It also covers proof that
published release artifacts are the artifacts that passed their package
checks. It does not add a new build system, change Wyrd product contracts,
create a production deployment workflow, or alter the existing credential
boundary for real-cloud tests.

## Requirements

- **REQ-001 — Affected closure.** For a classified Rust change, CI selects
  tests for the changed workspace packages and their transitive consumers.
  Selection accounts for supported target and feature configurations, test
  dependencies, added or removed packages, and renamed or deleted paths. If
  the dependency relationship cannot be determined, CI takes a conservative
  broader route.
- **REQ-002 — Cross-boundary coverage.** Selection includes relevant generated
  contract checks, first-class SDK tests, service integration tests, and real
  client-to-server journeys when a change can affect those surfaces. In
  particular, shared client or wire-contract changes cannot be treated as
  Rust-only; server behavior that affects Bifrost, identity, or storage cannot
  be proved solely by unit tests in its owning crate.
- **REQ-003 — Verification depth.** A routine pull request runs the affected
  checks and tests plus repository-wide format, lint, and live boundary checks
  required by `AGENTS.md`. The complete non-credentialed gate runs nightly,
  for releases, and for global, mixed, or unknown changes. Selective CI never
  replaces that full-regression cadence.
- **REQ-004 — Platform and environment ownership.** Server and real-server
  journeys run on Linux. First-class client and SDK coverage runs on Linux and
  macOS for affected client contracts. Tests retain their repository-managed
  Postgres, identity-provider, and storage-emulator setup. Credentialed cloud
  tests remain outside pull requests and continue to require the existing
  real-cloud credential boundary.
- **REQ-005 — Explainable selection.** Each CI run exposes the changed inputs,
  affected components, reasons for transitive or cross-boundary selection,
  selected verification lanes, and any full-gate fallback. A changed code path
  cannot quietly yield an empty test selection. Classification failure fails
  closed to broad verification rather than skipping proof.
- **REQ-006 — Bounded heavy tests.** Resource-heavy journeys retain safe
  concurrency on hosted runners without globally serializing unrelated tests.
  Their traces and terminal failure evidence remain available. Splitting or
  distributing test execution must preserve the exact test set and required
  environment; test omission is not a speed optimization.
- **REQ-007 — Measured build reuse.** CI reports enough timing and cache
  information to distinguish setup, compilation, cache transfer, and test
  execution cost. Cached outputs must be scoped to compatible toolchains,
  targets, and build inputs. Additional cache layers or test distribution are
  accepted only when measured improvement outweighs their cost and complexity.
- **REQ-008 — Selective routine packaging.** A `main` push dry-builds and
  smoke-checks the deliverables affected by that change. A release candidate
  builds and verifies the complete supported platform set. An uncertain
  deliverable relationship takes the complete packaging route.
- **REQ-009 — Release identity.** Publication uses the same tested, immutable
  artifact bytes or image digest for the release commit. Required checks for
  that commit precede publication. Published artifacts retain the signed
  release-manifest, digest, provenance, compatibility, and deployment-gate
  obligations in `architecture/operations/deployment-and-release.md`.

## Invariants and material boundaries

- **INV-001 — No false negative selection.** Optimizing for time or cost never
  skips a consumer, language surface, service journey, or contract check that
  can detect a regression from the change. Unknown impact broadens selection.
- **INV-002 — Test meaning is unchanged.** Selection, caching, and sharding do
  not weaken assertions, remove tests, replace real journeys with in-process
  substitutes, or conceal terminal traces.
- **INV-003 — One authority for each contract.** CI/CD changes do not move
  durable behavior into SDKs, introduce Wyrd protocol variants, or alter
  Bifrost server ownership. Rust, Python, and TypeScript continue to project
  the same wire contracts.
- **INV-004 — Trusted publication.** A release cannot publish unverified or
  rebuilt-different bytes, and pull-request code cannot obtain release or
  real-cloud credentials through selective routing or cache reuse.
- **INV-005 — Existing release compatibility.** Selection never bypasses the
  migration, version-skew, rollback, or deployment checks required by the
  deployment-and-release authority.

The expensive-to-reverse decisions are to use affected dependency closure
with conservative fallback as the pull-request model; to retain a complete
nightly and release gate; to keep first-class SDK platform parity; and to
publish only tested immutable release artifacts. The repository's existing
Cargo workspace, mise verification lanes, and stable required-check result
remain the delivery boundary. No particular selector implementation, cache
product, test partition count, or private workflow layout is prescribed.

## Acceptance criteria and evidence

- **AC-001.** Classification examples prove leaf-crate, shared-crate,
  wire-contract, Bifrost-only, mixed, global, docs-only, unknown, renamed, and
  deleted-file changes select the required lanes or conservative fallback.
- **AC-002.** An affected package's transitive consumers are included, while
  an unrelated family is absent from a routine classified Rust change.
- **AC-003.** Cross-boundary examples prove the applicable codegen, SDK
  platforms, server journeys, and environment-owning tasks are selected.
- **AC-004.** CI logs show selection reasons and a nonempty runnable test set
  for changed code; classifier errors cannot produce a green skipped run.
- **AC-005.** A normal hosted-runner execution proves heavy journeys complete
  with bounded concurrency and useful failure traces; the full nightly gate
  proves the complete test inventory still runs.
- **AC-006.** Before and after CI measurements report setup, compile, cache,
  and test durations for representative leaf, shared, Bifrost, and global
  changes. Any added build-reuse or distribution layer has evidence of net
  benefit.
- **AC-007.** A changed-deliverable `main` push builds the affected package
  set; release qualification builds the complete platform set; publication
  records the digest and provenance of the exact tested artifact. Release
  checks are tied to the same commit.

## Open material decisions

None identified for this revision. Implementation may choose a conservative
larger affected set when dependency or contract edges are uncertain.

## Revision history and authority

- **Revision 1, approved (2026-09-24):** Captures the accepted monorepo CI/CD
  direction for affected dependency closure, full-regression cadence, bounded
  journeys, measured reuse, and immutable release artifacts. Approved by the
  user before task planning.

Authority: `AGENTS.md` §§3, 11–12;
`architecture/agent-rules.md`;
`architecture/wyrd-design.md`;
`architecture/wyrd-doctrine.mdx`;
`architecture/bifrost-design.md`;
`architecture/operations/deployment-and-release.md`;
`architecture/references/languages/testing-workflows.md`.
