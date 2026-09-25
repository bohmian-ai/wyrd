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

## Implementation Evidence

Commits: `7535b719` (selection), `92c78828` (release routing and identity),
`2410542e` (docs, cache-on-failure), plus this evidence commit.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-001 leaf, shared, wire, Bifrost-only, mixed, global, docs, unknown, renamed, deleted routes | `.github/scripts/select-ci.py`; `detect-changes.sh` (`--no-renames`) | `mise run check:ci-selection`: 52 passed (RED first: 34 of 52 failed against the path-only classifier) | PASS |
| AC-002 transitive consumers in, unrelated families out | `Workspace.closure` (normal/build edges transitive, dev edges one hop); `run-family-tests.sh` `WYRD_TEST_PACKAGES` filter | leaf `vala-drift` → `test:vala` only; `skald-cache` → Skald, Vala, and Wyrd consumers; `WYRD_TEST_PACKAGES=vala-drift mise run test:vala` ran exactly 88 `vala-drift` tests | PASS |
| AC-003 codegen, SDK platforms, server journeys, environment tasks | `ci_lanes` (codegen:check, py/ts integration, ts:napi:check, test:bifrost:gate, check:examples), `rust_client`/`python`/`typescript`/`identity`/`storage` flags; `lints-test.yml` jobs | wire-contract, shared-client, server, auth, storage, SDK-only cases in `check:ci-selection`; workflow-shape assertions (both OS, full gate) | PASS |
| AC-004 logged reasons; classifier errors never green-skip | per-package/lane/fallback reasons to stdout and `$GITHUB_STEP_SUMMARY`; metadata failure, unowned path, and laneless package → full gate; empty family filter exits 1 | `check:ci-selection` failure cases | PASS |
| AC-005 bounded heavy journeys with traces; full nightly inventory | Bifrost concurrency bound unchanged (`test:bifrost:gate` CI threads 2); nightly callable by release | local `mise run gate` exit 0 (1765s); hosted-runner run pending push | PARTIAL |
| AC-006 before/after timings | baseline: `evidence/baseline-ci-timings.md`; per-lane timing table in the `ci` job summary; `cache-on-failure` (baseline: 4 of 5 cache restores missed because red runs skip the save) | after-measurements need hosted runs of this branch | PARTIAL |
| AC-007 affected main packaging, full release matrix, tested artifact identity | `release.yml` `changes` + per-deliverable conditions; `qualify` (nightly gate) and `release-commit` gates; `SHA256SUMS.*` records; `verify-release-artifacts.sh`; `attest-build-provenance` on every publisher; docker image smoke-tested then attested by pushed digest | packaging and digest-verifier cases in `check:ci-selection`; `actionlint` clean; hosted release dry-run pending push | PARTIAL |

Commands: `mise run check:ci-selection`; `bash scripts/checks/test-coverage.sh`;
`uvx --from actionlint-py actionlint .github/workflows/*.yml`; `mise run gate`
(exit 0); `mise run test:principals:integration` (exit 0); `mise run
verify:bifrost` (exit 1, see blockers); `git diff --check`.

Non-goals held: no new build system, no Wyrd protocol or Rust source change,
no deployment workflow, no credential change (`storage-integration-cloud.yml`
untouched; pull-request workflows reference no secrets, pinned by a check).

### Blockers and material risks

1. `mise run verify:bifrost` is red because of Bifrost production defects
   unrelated to CI routing:
   - `wyrd-testing::scribe sustained::scribe_sustained_ingest_oracle_hot_read_journey`
     fails reproducibly in isolation. The lane run read duplicated rows 32–47,
     breaking exactly-once acknowledgement (`sustained.rs:103`). The isolated
     rerun leaked an admission transition: 1043 opened, 1042 closed
     (`sustained.rs:409`).
   - `wyrd-testing::oracle published::published_cache_pruning_and_shutdown_are_production_governed`
     failed under the full lane: "Forge compacted 0 of 3 sealed inputs within
     32 passes". It passed in isolation (21s), so a liveness bound is missed
     under load.
   Both need a Bifrost remediation task; neither is inside this change's write
   set.
2. `wyrd-spec` cannot be packaged: `cargo package -p wyrd-spec` fails on the
   unpublished `skald-spec` dependency. The new `package-crates` check exposes
   this. Crate publication was already broken before this change.
3. A full `wyrd.release/v1` ReleaseManifest (migration, contract, and
   compatibility digests) has no tooling in the repository. This change
   records artifact digests and signed provenance, and it fails publication
   closed on any mismatch. The manifest and deployment gate remain release
   authority work that needs its own specification.
