---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-affected-monorepo-ci
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007]
depends_on: []
---

# Dependency-aware CI and verified release builds

## Outcome and Value

Routine changes receive fast, explainable verification of their affected
packages and consumers. Cross-language contracts and service journeys still
run when relevant; uncertain changes receive the complete gate. Routine
`main` builds package affected deliverables, while release qualification
verifies the full platform set and publication uses the tested artifact
identity. This single outcome maps every obligation in the approved spec.

## Owners, Scope, Consumers, and Prohibited Changes

The current owners are change classification, the `lints-test` and specialized
workflows, repository `mise` verification lanes, the family test runner,
nextest's journey configuration, and the release workflow. Consumers are
contributors reading CI results, the stable required-check result, SDK jobs
on Linux and macOS, service journeys on Linux, and release publication.

Keep the Cargo workspace and mise as the verification boundary. Preserve
repository-wide required format, lint, and live boundary checks, the nightly
and release full gates, the real-cloud credential boundary, and all existing
assertions and environments. Do not add a build system, Wyrd protocol change,
production deployment workflow, or release bypass. Paths below guide
ownership; they are not a private implementation allowlist.

### Required CI routing

Apply these rules to the changed files and the resulting consumer closure;
their order matters. A broader rule wins when a change matches several rows.
The listed commands are existing proof owners, not a limit on tests discovered
through dependency or contract edges.

| Change or affected surface | Required proof |
| --- | --- |
| Global CI/build inputs, mixed Bifrost and other Rust, unknown path or edge, classifier failure, or unresolved added/removed/renamed package | `mise run gate`. Never turn a classifier error or an empty code plan into a successful skip. |
| Classified Rust package | Test the changed package and its transitive reverse dependents across supported normal, test, and feature edges. Select their owning Rust test lanes, including Postgres setup when required; do not run the whole `test:rust` aggregate solely because a leaf Rust package changed. |
| Bifrost-only change | `mise run verify:bifrost`, including its real server and Rust/Python/TypeScript journeys. Keep Bifrost's hosted-runner concurrency bound inside this lane. |
| Wire/schema or shared `wyrd-client` contract | Affected Rust consumers, `mise run codegen:check` where generated contracts can change, Rust/Python/TypeScript SDK jobs on Linux and macOS, and the applicable real-server journeys on Linux. Route changes to served HTTP/OpenAPI contracts through `mise run test:principals:integration`; use `mise run py:test:integration` and `mise run ts:test:integration` for affected language journeys. If applicability cannot be established, take the full gate. |
| Identity or storage server boundary | The affected Rust and SDK consumers plus `mise run test:identity:journey` or `mise run test:storage:matrix`, respectively, on Linux. Preserve the identity-provider and emulator setup owned by those tasks. A Bifrost server boundary also requires its Bifrost journey lane. |
| Python or TypeScript SDK only | Owning language checks and tests on Linux and macOS, including integration when the client/server path is affected; include affected Rust/native consumers. Do not substitute unit tests for required journeys. |
| Docs or UI only | Their existing docs or UI checks; if the same change alters code or global inputs, apply the corresponding broader row. |

Every routine pull request still runs `mise run check` and applicable live
boundary checks (including `check:client-tier` and `check:pyo3-scope` for their
respective edges). Keep the stable `ci-complete` required result, even when
selected jobs differ. Emit changed inputs, selected components, each edge and
lane reason, and the reason for any fallback. The existing real-cloud storage
workflow remains push-to-`main` only and credentialed; no pull-request job or
cache may acquire those credentials.

### Required packaging and publication routing

For a routine `main` push, use the same affected-component closure to select
dry-builds and smoke checks: Python package sdist and all supported wheels
when the Python SDK or a dependency it packages changes; TypeScript native
addon packages for all supported targets when its SDK or native dependency
changes; UI plus server bundles for both Linux targets when the server, UI, or
packaged dependency changes; and applicable Rust crate package checks for
publishable crate changes. A deliverable with an unknown impact edge selects
the complete packaging matrix. A release event always selects every supported
deliverable and platform, regardless of the changed-file set.

Publication must wait for required checks on the release commit and consume
the verified package artifact or image digest from that commit. Record digest,
provenance, and the required signed release manifest for each published
output; enforce the compatibility, migration, rollback, and deployment gates
in the release authority. An artifact rebuilt after verification must be
verified as that exact new digest before publication. The release publisher
must fail closed if identity or commit proof is missing.

## Approach

1. Capture current CI timings and the existing lane inventory before changing
   selection; establish the representative leaf, shared, Bifrost, and global
   baselines available from completed runs.
2. Make affected-package and consumer selection conservative and explainable,
   adding explicit cross-language, generated-contract, and service-journey
   relationships where the Cargo graph cannot express them.
3. Route classified changes through the owning repository-native verification
   lanes; retain complete-gate fallback and stable required-check behavior.
4. Preserve safe hosted-runner concurrency and traces, and expose enough
   timing/cache evidence to assess whether build reuse or distribution helps.
5. Scope routine `main` package dry-runs while retaining full release
   qualification and binding publication to the exact verified artifact and
   commit.

## Ordered Implementation Scenarios

### Scenario 1 — A classified Rust change selects its consumer closure

**Behavior.** A leaf-crate change tests its owner without unrelated families;
a shared-crate change includes transitive consumers and their relevant test
lanes. Added, renamed, and deleted crate paths retain their resolvable
consumer closure or select the full gate when it cannot be resolved.
`REQ-001`, `REQ-003`, `INV-001`, `AC-001`, `AC-002`.

**RED.** Add representative selection regressions to the existing CI
selection check and run `mise run check:ci-selection`. Current generic Rust
routing either selects the complete Rust aggregate or cannot identify
transitive consumers, so the new expected selections must fail.

**GREEN.** Connect changed inputs to the affected test closure and invoke the
repository-native lanes with their existing environment ownership. Rerun
`mise run check:ci-selection` and the earlier selection cases.

**REFACTOR.** Remove redundant path or package rules where one authoritative
relationship already supplies the same coverage; keep selection reasons
visible and the regressions green.

### Scenario 2 — Cross-boundary changes and uncertainty preserve proof

**Behavior.** Wire or shared-client changes select applicable codegen, Rust,
Python, TypeScript, and real-server proof. Relevant Bifrost, identity, and
storage changes retain their journeys. Mixed, global, unclassified, and
classification-error cases fall back to the complete gate; a changed code
path cannot produce a green empty test plan. Server work stays on Linux and
affected SDK coverage stays on Linux and macOS. The `ci-complete` result
reflects every selected job. `REQ-002`–`REQ-005`,
`INV-001`–`INV-003`, `AC-001`, `AC-003`, `AC-004`.

**RED.** Extend the real classifier and workflow selection regressions for
these boundaries and failure cases; run `mise run check:ci-selection`. At least
the cross-language or empty-selection expectations fail against current
path-only routing.

**GREEN.** Route the required checks, journeys, and platform jobs, and report
why each was selected. Preserve the stable aggregate result and the existing
credentialed-cloud isolation. Rerun `mise run check:ci-selection` and Scenario
1's cases.

**REFACTOR.** Consolidate only duplicated classification policy; keep
conservative fallbacks explicit and all scenarios green.

### Scenario 3 — Routine packaging is selective; release qualification is full

**Behavior.** A `main` push dry-builds and smoke-checks each affected
deliverable, not unrelated packages. Unknown impact selects the full set. A
release candidate builds and verifies every supported platform, then
publishes only the artifact bytes or image digest that passed checks for its
commit; missing proof blocks publication. Protected credentials, signed
manifest, compatibility, and deployment-gate requirements remain in force.
`REQ-008`, `REQ-009`,
`INV-004`, `INV-005`, `AC-007`.

**RED.** Add repository-native release-selection and artifact-identity
regressions under the CI selection check; run `mise run check:ci-selection`.
Current `main` dry-runs build the complete matrix, and publication has no
explicit proof tying every output to the tested commit and artifact identity,
so the new assertions must fail.

**GREEN.** Select affected routine package builds, retain a full release
matrix, and require exact tested artifact identity and commit verification at
publication. Rerun `mise run check:ci-selection` and Scenarios 1–2.

**REFACTOR.** Share only durable release-selection policy; remove redundant
builds or checks without weakening the artifact proof.

## Verification and Evidence

**Configuration and static proof.** Concurrency, traces, test inventory,
timing/cache reporting, and release authority are partly workflow obligations,
so they do not need a manufactured RED. Inspect the selected jobs and their
logs, cache restoration and save cost, and packaged-artifact digests. Add
regressions to `mise run check:ci-selection` where routing is executable.
Record before/after measurements for the representative changes; introduce
test distribution or another cache only if those measurements demonstrate a
net gain. This covers `REQ-006`, `REQ-007`, `AC-005`, and `AC-006`.

Run the focused selection check after each scenario. Then run the relevant
format/lint and boundary tasks, `mise run check:ci-selection`, and the smallest
complete affected test and journey lanes through mise. At minimum, exercise
`mise run check`, `mise run codegen:check`, `mise run verify:bifrost`,
`mise run test:identity:journey`, `mise run test:storage:matrix`,
`mise run test:principals:integration`, `mise run py:test:integration`, and
`mise run ts:test:integration` when their corresponding routing cases change.
Because this change
modifies shared CI/build/test infrastructure and prepares release behavior,
run `mise run gate` as the integrated local gate. Inspect the Linux server and
Linux/macOS SDK jobs in CI, the nightly full-gate inventory, and a release
qualification dry-run that exercises the complete supported package matrix.
Use `git diff --check` and inspect the final diff. Do not publish a real
release or deploy as a verification shortcut.

## Acceptance Criteria

- `AC-001`–`AC-004`: the selection regressions cover success, edge, and
  failure routes; logs explain selections; no changed code silently skips
  tests.
- `AC-005`: hosted heavy journeys pass at safe concurrency with traces; the
  complete nightly inventory remains reachable and green.
- `AC-006`: comparable setup, build, cache, and test durations are recorded
  before and after; any added optimization earns its cost.
- `AC-007`: affected `main` packaging and complete release qualification are
  demonstrated; tested artifact identity, commit gate, provenance, and
  existing release protections are evidenced without publishing.

## Expected Write Set and Consumer Closure

Likely surfaces: `.github/scripts/detect-changes.sh` and its selection tests;
`.github/workflows/lints-test.yml`, specialized CI workflows, and
`.github/workflows/release.yml`; `mise.toml` and existing Rust family and
capability runners; nextest configuration if resource scheduling requires it;
and the minimum documentation that explains the final CI behavior. SDK,
server, Bifrost, identity, storage, codegen, and release consumers must be
verified through their owning lanes even when their production source does
not change.

## Material Stop Conditions

Return to the approved specification if correct selection requires a new
public or persisted contract, a different credential or tenant boundary, a
weaker journey or full-gate cadence, a different release identity rule, or a
change to deployment compatibility. Reversible classifier, task, cache, and
workflow mechanics remain implementation choices.

## Authority Links

- `changes/active/affected-monorepo-ci/spec.md` revision 1, approved.
- `AGENTS.md` §§3, 11–12 and `architecture/agent-rules.md`.
- `architecture/references/languages/spec-driven-development.md`,
  `architecture/references/languages/implementation-execution.md`, and
  `architecture/references/languages/testing-workflows.md`.
- `architecture/operations/deployment-and-release.md`,
  `architecture/wyrd-design.md`, and `architecture/bifrost-design.md`.
