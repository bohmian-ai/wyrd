# Repository standards review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `9c52f887595d4e8a04764d3d2836926e5df70950`
- Candidate tree: `e2731ff2d23fc1a0f4b356ccdd79caf3f6c7b93e`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 8
- Reviewed task inputs: original `TASK-002`, `TASK-004`, and `TASK-003`, plus `TASK-003-R1-close-cumulative-review-findings.md`

The base and candidate commits resolve unambiguously. The repository has no
`.codegraph/` directory, so CodeGraph was not used. The working checkout is at a
later commit and contains unrelated user changes; all source evidence below was
read from the immutable candidate object, not from the checkout.

## Authority coverage

| Changed surface | Applicable authority read | Coverage result |
|---|---|---|
| Repository ownership, dependencies, Rust structure, async behavior, rustdoc, and verification | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/implementation-execution.md`; `architecture/references/languages/spec-driven-development.md` | Covered; rustdoc violations retained as `STD-003`. |
| Public Wyrd, Card, client, SDK, generated-schema, and error surfaces | `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/errors.md`; `pyo3-boundaries.md`; `python-api-and-stubs.md`; `typescript-guide.md` | Covered; the shared-client/SDK topology and generated-contract deletion follow the current owners. |
| Bifrost Gate/Scribe/Oracle/Forge, query, storage, audit, and resource behavior | `architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`; `architecture/v1/00-foundations/permission-check.md`; `architecture/references/domain/vala-architecture.md`; `olap-serving.md`; `datafusion.md`; `iceberg.md`; `telemetry-observations.md`; `analytical-operations-reliability.md` | Covered; the candidate leaves mutually exclusive Oracle audit contracts in applicable authority, retained as `STD-001`. |
| Data-root boot, deployment configuration, readiness, recovery, and storage backend selection | `architecture/operations/README.md`; `deployment-and-release.md`; `reliability-and-recovery.md`; applicable `runbooks.md`; storage and server source/docs | Covered; the locked root/write probe and one typed `WYRD_STORAGE_URL` owner conform, but Oracle readiness/recovery prose still names deleted state (`STD-001`). |
| CI affected-code selection, required aggregation, nightly journeys, storage emulators, live cloud, and performance | `AGENTS.md` §§11–12; `architecture/references/languages/testing-workflows.md`; workflow/scripts/mise definitions and tests | Covered; generic Rust and nightly local matrices conform. The live-cloud workflow has no schedule (`STD-002`). |
| MCP/agent-facing catalog and audit behavior | `architecture/references/languages/agent-harness.md`; Wyrd/Bifrost design; runtime catalog evidence supplied with the task | Covered; stale Oracle WAL/relay guidance participates in `STD-001`; no separate MCP catalog violation found. |

## Rule results

| Applicable rule | Evidence | Result |
|---|---|---|
| Server owns durable behavior; `wyrd-client` is the sole SDK-facing client implementation and language SDKs are thin projections. | `crates/shared/wyrd-client`, `sdks/wyrd-sdk-{rust,python,ts}`, boundary scripts, and the cumulative diff remove the sibling client owners rather than adding another one. | PASS |
| `wyrd-spec` remains foundational, IO-/async-/PyO3-free, and generated public contracts come from owning sources. | Candidate deletes the generator-only Bifrost types and their generated/golden schemas from the source generator and checked outputs; supplied `codegen:check` evidence is green. No new runtime dependency enters `wyrd-spec`. | PASS |
| Public errors use the derive-backed catalog; crate-local errors use typed `thiserror` and must not expose secret-bearing values. | The storage URL failure is crate-local `ConfigParseError::InvalidUrl`; parsing rejects credentials and deliberately reports only a reason, not the supplied URL. No parallel public error catalog was added. | PASS |
| Every Wyrd-owned task set/queue is bounded; Oracle read/tripwire audit work remains non-blocking, tracked, logged, and counted. | `OracleQueryAudit` acquires a pool-sized pending semaphore before `TaskTracker::spawn`, retains the separate quarter-pool connection semaphore, and records overflow through the existing failure metric/log path. | PASS |
| There is one Oracle audit path: direct canonical staging append from a tracked non-blocking task, with no Oracle audit WAL or relay. All applicable architecture and operational guidance must agree. | Top-level rules and implementation use the non-blocking staging append, but security, operations, runbooks, patterns, agent guidance, and implementation guidance still require a local acceptance WAL and relay. | **FAIL (`STD-001`)** |
| All Bifrost local paths derive from one locked root and are usable before role activation. | `BifrostDataRoot::prepare` locks the root and write/sync/remove-probes every managed child before returning; focused root tests are supplied as passing. | PASS |
| Typed configuration is the source of truth; one canonical setting owns storage identity and unsafe/malformed combinations fail startup. | `BackendConfig::from_url` derives backend/location from required `WYRD_STORAGE_URL`, rejects credentials/ports/query/fragment/extra segments, and shares the typed config with storage and Bifrost catalog consumers. Current public storage docs project the same names. | PASS |
| Tenant boundaries, SQL connection owners, and permission scope must remain typed and fail closed. | No remediation change introduces raw pools into domain signatures, manual `TenantConn` tenant filters, widened scopes, or caller-selected tenancy. The upload completion correction remains tenant-transaction scoped and rejects aborted/failed state. | PASS |
| New or materially modified Rust items, including test helpers and tests, require complete rustdoc; panic and async partial-progress/cancellation behavior must be documented when relevant. | The materially added `run_client_server_journey` has no rustdoc, and the new cloud storage tests document only a summary despite using panic-capable assertions/`expect`; several async helpers/tests likewise omit required panic or cancellation/partial-progress sections. | **FAIL (`STD-003`)** |
| Pull requests run affected closure, generic Rust runs Linux `check` + `test:rust`, full-gate changes run `gate`, and required aggregation remains stable. | `.github/workflows/lints-test.yml:61-115` uses the existing `ci` owner and branches without duplicating `test:rust` under `gate`; supplied `check:ci-selection` evidence is green. | PASS |
| Complete non-credentialed correctness runs nightly; live-cloud and performance qualification run in separate scheduled workflows. | `nightly.yml` has a scheduled matrix over `gate` plus six omitted journey owners, and `performance.yml` has its own schedule. `storage-integration-cloud.yml` comments out its only cron and runs only on pushes to `main`. | **FAIL (`STD-002`)** |
| Tests must use the owning runtime and repository-managed environments; no check may be weakened, skipped, ignored, or selected to zero. | Changed Rust/Python/TypeScript tests remain in their owning runtimes; emulator tasks provide explicit backend configuration and fail instead of environment-skipping. No new `allow`/`ignore` or weakened boundary glob was found. | PASS |
| New checks must protect a reachable invariant; retired checks require an explicit unreachable/enforced-elsewhere rationale. | The cumulative change records the removed Oracle-admission self-check's deleted owner and uses existing CI-selection/storage checks rather than adding a second checker for the same property. | PASS |
| No legacy Wyrd vocabulary, compatibility route, alternate SDK owner, or restored retired harness enters the result. | Cumulative deletion/redirect inventory and supplied `check:no-legacy-server-vocab` evidence show no surviving legacy Bifrost owner or compatibility facade. | PASS |

## Material findings

### STD-001 — VIOLATION: applicable authority still requires the deleted Oracle audit WAL and relay

- **Violated rule:** `AGENTS.md` and `architecture/agent-rules.md` require one
  audit write path and expressly forbid another audit WAL or relay. The reference
  router requires every applicable governing authority to agree; approved code
  or a spec cannot silently leave current architecture contradictory.
- **Locations:**
  - `architecture/wyrd-security-posture.md:239-242`
  - `architecture/operations/README.md:25,50`
  - `architecture/operations/reliability-and-recovery.md:148-150`
  - `architecture/operations/runbooks.md:135-139`
  - `architecture/references/architecture/patterns.md:259-262`
  - `architecture/references/doctrine/architecture-constraints.md:116-118`
  - `architecture/references/languages/agent-harness.md:93-96`
  - `architecture/references/languages/implementation-execution.md:224-225`
- **Evidence:** Those sources require a versioned CRC-framed local acceptance
  WAL, fsync-before-rows, and bounded relay. In the same candidate,
  `architecture/agent-rules.md:13`, `AGENTS.md` current decisions, revision-8
  REQ-014/REQ-026A, and `crates/wyrd/wyrd-server/src/oracle/query_audit.rs`
  require a tracked non-blocking direct append to `vala.audit_staging`, with
  failure logged/counted and no WAL or relay. `deployment-and-release.md:114-119`
  also says Oracle has no durable local audit state.
- **Consequence:** Operators and future implementers receive mutually exclusive
  readiness, volume, recovery, and security requirements. Following the
  normative security/operations prose would reintroduce a prohibited durable
  subsystem; following the implementation makes the documented Oracle
  readiness and recovery checks impossible.
- **Testable correction:** Update every listed authority/reference to the
  already-approved revision-8 contract: bounded tracked non-blocking canonical
  staging append, logged/counted failure, no Oracle audit WAL/relay, and no
  WAL/relay readiness or recovery dependency. Then run `mise run docs:check`,
  `mise run check:design-sync`, and an exact repository search proving the
  removed Oracle audit-WAL/relay contract has no live architecture/reference
  occurrence. Do not change the implementation semantics.

### STD-002 — VIOLATION: the live-cloud suite is not scheduled

- **Violated rule:** `architecture/references/languages/testing-workflows.md:51-55`
  requires live-cloud and performance/qualification suites to run on separate
  schedules; revision-8 REQ-034 and AC-008 impose the same repository contract.
- **Location:** `.github/workflows/storage-integration-cloud.yml:10-23`.
- **Evidence:** The prose claims a weekly Monday cron, but the only `schedule`
  stanza is commented out. The workflow runs solely on pushes to `main`.
  `performance.yml:6-9` and `nightly.yml:3-6` demonstrate the required native
  scheduled form.
- **Consequence:** Weeks without a main push receive no real S3/GCS/Azure
  qualification, so credential drift, provider API changes, and IAM breakage can
  remain undetected despite the repository claiming scheduled coverage.
- **Testable correction:** Enable the existing weekly cron in the live-cloud
  workflow and preserve its separate workflow and OIDC permissions. Run
  `mise run check:ci-selection` and inspect the workflow trigger; no new job,
  parser, or permanent check is needed.

### STD-003 — VIOLATION: materially added Rust test workflows lack mandatory rustdoc

- **Violated rule:** `AGENTS.md:657-670` and `architecture/agent-rules.md`
  make complete rustdoc a hard acceptance criterion for every new or materially
  modified Rust item, including private helpers and tests; fallible/panicking
  and async durable workflows must document errors, panics, cancellation, and
  partial progress where relevant.
- **Locations:**
  - `crates/wyrd/wyrd-server/tests/storage_e2e.rs:172` has no rustdoc for the
    large async client→server→client workflow.
  - `crates/wyrd/wyrd-server/tests/storage_e2e.rs:408-430` adds three
    panic-capable async tests without `# Panics`.
  - The same omission appears on materially added storage tests/helpers at
    `crates/wyrd/wyrd-storage/src/settings.rs:369,454,522`,
    `crates/wyrd/wyrd-storage/tests/handle_crud.rs:123-135`, and
    `crates/wyrd/wyrd-storage/tests/integration_{azurite,gcs}.rs:24,23`.
- **Evidence:** These functions contain assertions, `expect`, or call helpers
  that panic on setup/IO/contract failure. The 258-line journey additionally
  performs multipart initiation, upload, completion, download, cleanup, and
  server shutdown but documents neither workflow role nor cancellation/partial
  progress.
- **Consequence:** The candidate violates an explicit hard blocker, and the
  most operationally significant new test workflow does not state which durable
  effects may remain when it is interrupted.
- **Testable correction:** Add concise rustdoc to every new/materially modified
  Rust item in the remediation range, including `# Panics` and async
  cancellation/partial-progress notes where applicable. No helper or abstraction
  is needed. Re-run `mise run fmt`, `mise run lints`, the affected storage tests,
  and source-review the remediation diff against `AGENTS.md` §16.

## Verification evidence and limits

- Independently checked: both commit objects exist; candidate tree is
  `e2731ff2d23fc1a0f4b356ccdd79caf3f6c7b93e`; `git diff --check
  861f8d86cc3f9d7e70fb59489e80f8be62afddbf..9c52f887595d4e8a04764d3d2836926e5df70950`
  passes; the remediation commit range contains no `Co-authored-by` trailer.
- Reviewed supplied evidence: `fmt:check`, `lints`, `codegen:check`,
  `docs:check`, `check:ci-selection`, Bifrost boundary checks, `gate`, the six
  newly scheduled journey owners, and storage emulator/backend tests are
  reported passing. The report states `gate` selected 6,025 tests with zero
  failures.
- No expensive test suite was rerun during this read-only standards wave.
- No candidate-SHA GitHub Actions run URLs or statuses were supplied for S3,
  GCS, and Azure. The implementation record contains only an owner assertion of
  independent validation, and the workflow cannot be manually dispatched.
- The remediation task/evidence commit is later than the immutable candidate;
  it was treated as a supplied review input, not as source proof inside the
  candidate tree.

## Overall result

**FAIL**

The implementation satisfies many repository boundaries, but three explicit
repository standards remain violated. `STD-001` leaves security and operations
authority incompatible with the implemented audit architecture; `STD-002`
leaves required live-cloud scheduling inactive; and `STD-003` violates the
repository's rustdoc hard blocker.
