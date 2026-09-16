# Wave 1 Task Implementation Review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `bbfdf35e26212b2a831bda5e31e1ef4433e41900`
- Candidate tree: `58f5c2acfe8da01f3e4b58363f38ce761626df35`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 8
- Cumulative obligations: `TASK-002-converge-client-and-sdks.md`; the R2 verdict and `TASK-002-R3-close-r2-review-findings.md`; `TASK-004-unify-bifrost-data-root.md`; and `TASK-003-close-repository-integration.md`

The complete base-to-candidate range was inspected. `HEAD` and its tree were the supplied candidate identities before review and remained so after review. The pre-existing untracked `changes/active/verified-change-contract/architecture/verifier/` directory was not read as candidate evidence and was not modified. No source file was edited.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK-002: one shared `wyrd-client` implementation and thin Rust, Python, and TypeScript SDK roots | `crates/shared/wyrd-client/src/lib.rs:1-24`; `sdks/wyrd-sdk-rust/src/lib.rs:1-12`; SDK manifests depend on `wyrd-client` | Recorded TASK-002 Rust/Python/TypeScript build, typecheck, unit, integration, boundary, and journey evidence | PASS |
| TASK-002 / REQ-019: `Bifrost` owns table, ingestion, query, streaming, and lifecycle behavior; query and raw gRPC mechanics are not ordinary sibling clients | `crates/shared/wyrd-client/src/bifrost/mod.rs:1-67`; `facade.rs:36-175`; `QueryClient` remains private and raw transport is exposed only under `test-support`, which no SDK enables | Compile-fail documentation plus recorded SDK journeys and client-tier checks | PASS |
| TASK-002: preserved Card registration/loading and `WyrdState` workflows through affected surfaces | `crates/shared/wyrd-client/src/cards/`, `state.rs`; three SDK projections and real-server journey targets | Recorded Card, CLI, Rust SDK, Python Card, and Python WyrdState lanes | PASS |
| TASK-002: exact MCP catalog, CLI behavior, generated Bifrost table/query contracts, and catalog-backed cross-language errors | MCP discovery test, CLI facade use, route annotations, generated OpenAPI/error-code outputs | Recorded focused MCP/CLI/OpenAPI/error-contract checks and `codegen:check` | PASS |
| TASK-002: five Postgres-backed Card/CLI/WyrdState lanes own isolated lifecycle and do not restore deleted setup tasks | `mise.toml:102-137,930-952`; no live reference to `setup:postgres` or `setup:db-roles` outside the inventory prohibition | Recorded five lane passes and `test:postgres:inventory` | PASS |
| Prior `FIND-TASK-002-7`: fallible/cancellable relocated workflows have governed rustdoc | R3 documentation/import commits and `#![deny(missing_docs)]`; sampled changed owners include accurate `# Errors` and cancellation text | Recorded cumulative scan, lint, and codegen evidence | PASS |
| Prior `FIND-TASK-002-8`: governed types use module-top imports | R3 import correction in client/auth/native/test owners | Recorded cumulative scan, lint, TS build/typecheck, and boundary checks | PASS |
| Prior `FIND-TASK-002-15`: cumulative diff hygiene | Review artifacts no longer have the reported terminal whitespace | `git diff --check 861f8d86..bbfdf35e` exited 0 with no output during this review | PASS |
| Prior `FIND-TASK-002-16`: shutdown refuses later direct sends and waits for every admitted direct Arrow send | `crates/shared/wyrd-client/src/bifrost/handle.rs:188-230,267-315,383-422` | Four-test focused nextest run passed, including `shutdown_waits_for_admitted_direct_write` | PASS |
| Prior `FIND-TASK-002-17`: completed failed-drain settlement is at most once while cancellation of the settlement future remains resumable | `crates/shared/wyrd-client/src/bifrost/query.rs:898-933` fixes re-entry only; it marks `Settled` before its first await and has no cancellation rollback | Existing focused test passes only the completed-call/twice case (`query.rs:1396-1436`); no test cancels `settle()` itself | FAIL — `TASK-REV-01` |
| Prior `FIND-TASK-002-18`: commit trailers | 22 historical trailers remain; candidate records the owner's explicit 2026-09-14 acceptance without rewriting history | Commit-log inspection confirms the accepted state | PASS under explicit owner authority |
| TASK-002 constraints/non-goals: no duplicate durable client behavior, legacy reads/audit/bootstrap/live UI, new lifecycle owner, or Bifrost aggregate during task proof | Shared owners and removal/static inventory; R3 uses existing `WriterPool` and `QueryResultStream` | Recorded boundary/removal checks; static source inspection | PASS |
| TASK-004 / REQ-055: one default/overridden root derives every managed Scribe and Oracle path; Forge gets no local child | `crates/wyrd/wyrd-server/src/boot/data_root.rs:59-128`; `config.rs:2241-2249`; boot passes derived WAL/spill roots at `boot/mod.rs:439-487,641-652,1001-1019` | Root/config focused tests and recorded deploy/resource checks | PASS |
| TASK-004: unusable/invalid/unwritable/contended roots fail before any selected role activates | Root is prepared before external dependency and role construction (`boot/mod.rs:434-439`), and lock contention/uncreatable roots are rejected; however `data_root.rs:78-108` only creates directories and write-opens the root lock file | Tests cover a file-backed root and contention (`data_root.rs:162-189`) but no pre-existing unwritable managed child | FAIL — `TASK-REV-02` |
| TASK-004: restart retains acknowledged Scribe authority and replicas cannot share a writable WAL identity | Root lock lifetime and stable node identity at `data_root.rs:47-57`, `boot/mod.rs:444-454`; per-node deployment/harness roots | Recorded restart, process-pool, readiness, Scribe, and lock tests | PASS |
| TASK-004 constraints/non-goals: removed WAL env has no alias; Forge has no spill; no second root or weakened memory/readiness owner | `config.rs:2241-2249`; `data_root.rs:72-77,159`; deploy manifests and resource-owner composition | Config, deploy, resource-governance, and unwrap checks recorded | PASS |
| TASK-003 / REQ-031: isolated Postgres contract, roles, attach, cleanup, and inventory | `scripts/postgres/*`, SQL/dev-fixture owners, canonical outer/inner mise lanes | Recorded contract/concurrency/roles/inventory passes | PASS |
| TASK-003 / REQ-032, REQ-035A, REQ-064: every surviving Bifrost and non-Bifrost journey has a credible owning lane and local evidence | `mise.toml` defines Bifrost, Cards, CLI, WyrdState, identity, storage, Python, TypeScript, and Postgres lanes | Candidate records successful local runs, including `gate`, `verify:bifrost`, focused journeys, identity, Postgres, Python, TS, and storage | PASS for local evidence |
| TASK-003 / REQ-033, REQ-035: PR affected-code selection and stable aggregation cover generic, Bifrost-only, mixed, global, unclassified, success, skipped, failure, cancellation, and empty input | `.github/scripts/detect-changes.sh:49-110`; `verify-required-jobs.sh:1-17`; `lints-test.yml:15-329` | `mise run check:ci-selection` passed 31 classifier and 7 aggregation cases during this review | PASS |
| TASK-003 / REQ-034, AC-008: nightly `main` runs the complete non-credentialed correctness suite, while live-cloud and performance remain separate | `.github/workflows/nightly.yml:14-95` runs `gate`, identity, and Postgres checks; `mise.toml:1361-1412` shows `gate` omits several gated journeys explicitly required by TASK-003 | No workflow execution evidence; static scheduling proves the omitted lanes are not reached | FAIL — `TASK-REV-03` |
| TASK-003: generated artifacts are source-derived and clean across OpenAPI/schema/stubs/declarations/docs/examples/MCP | Generators and reconciled artifacts in the cumulative diff | Recorded `codegen:check`, docs, TS/Python, examples, and MCP tests | PASS |
| TASK-003 / REQ-042-046, AC-020: greenfield migrations; no legacy engine/alias, CLI bootstrap, audit-integrity resurrection, or tracked `.node`; no history rewrite | No `crates/vala/vala-bifrost`; no tracked `*.node`; current CLI/migration/static inventories | Static searches plus recorded repository checks | PASS |
| TASK-003 / INV-025, AC-022: every required local lane and credentialed live-cloud workflow has passing evidence | Local results are recorded, but TASK-003 itself states the credentialed storage workflows still require a push and GitHub secrets (`TASK-003:95-97,122-126,165`) | No GitHub Actions run URL/status or other passing credentialed evidence is attached | FAIL — `TASK-REV-04` |
| TASK-003 non-goals: no merge/push/deploy/history rewrite/live Oracle UI; no restored retired benchmark/qualification/runtime-emulation surface | Candidate remains on the dedicated integration branch; static inventory and diff | Candidate/tree identity and branch checked | PASS |

## Proposed findings

### `TASK-REV-01` — INCORRECT: cancelling `QueryResultStream::settle` permanently suppresses settlement

- Violated obligation: `TASK-002-R3` requires preserving resumability when the settlement future itself is cancelled while making only a *completed* settlement terminal (`TASK-002-R3-close-r2-review-findings.md:96-107`). REQ-056 also requires dropped callers to trigger bounded settlement.
- Exact location: `crates/shared/wyrd-client/src/bifrost/query.rs:912-932`; incomplete proof at `query.rs:1396-1436`.
- Evidence: `settle()` replaces `Healthy`/`Broken` with `Settled` at line 913 before awaiting cancellation, body drain, or status retirement. If the caller drops this future at any await, the stream remains `Settled`; the next `settle()` returns at line 915 without resuming the unfinished work. The added test awaits the first call to completion and therefore cannot exercise this reachable cancellation path. Python and TypeScript wrappers own the same `QueryResultStream`, so runtime cancellation/drop can reach it.
- Observable consequence: a cancelled settlement can leave the server query running until server cleanup while the client falsely records that it owes no further cancel/drain/status work.
- Required testable correction: keep settlement resumable until its cancel/drain/status cycle actually completes, then transition terminally to `Settled`; preserve the existing one owner/state machine and completed-call at-most-once behavior. Add a deterministic test that parks settlement after it begins, cancels/drops that future, resumes `settle()`, and proves cleanup completes without duplicate completed cycles.

### `TASK-REV-02` — MISSING: boot does not validate that every managed child directory is writable

- Violated obligation: TASK-004 requires every derived path to be usable before Scribe or Oracle activation and explicitly requires an unwritable root to fail with no partial activation (`TASK-004-unify-bifrost-data-root.md:25-26,50-51,62-64`).
- Exact location: `crates/wyrd/wyrd-server/src/boot/data_root.rs:71-108`; incomplete tests at `data_root.rs:162-189`.
- Evidence: `prepare()` calls `create_dir_all` for existing managed directories, then write-opens only `<root>/.lock`. A pre-existing read-only `scribe-stage`, `scribe-output-scratch`, or `oracle-spill` directory passes this preparation. Resource registration merely inspects metadata/readability (and only reconciles existing Scribe scratch entries), so empty read-only children can survive pre-role validation. Existing tests cover a non-directory root and lock contention, not child writeability.
- Observable consequence: the server can reserve/activate Bifrost roles and advertise readiness before the first staged member, Scribe scratch, or Oracle spill creation reveals the unusable configured volume.
- Required testable correction: in the existing `BifrostDataRoot::prepare` owner, prove create/remove write capability for every managed directory before returning the prepared handle, clean up probes on both success and error, and return the existing structured unusable error. Add a focused Unix permission test (and an appropriate platform-specific equivalent or gate) showing a pre-existing unwritable managed child prevents composition before role activation.

### `TASK-REV-03` — MISSING: nightly does not schedule the complete non-credentialed suite

- Violated obligation: REQ-034 and AC-008 require the complete non-credentialed correctness suite nightly; REQ-064 says gated/ignored journeys must be invoked through their owning mise lanes. TASK-003 repeats this at `TASK-003-close-repository-integration.md:65-67,85-88`.
- Exact location: `.github/workflows/nightly.yml:14-95`; aggregate definition `mise.toml:1361-1412`.
- Evidence: nightly runs only `mise run gate`, `test:identity:journey`, and four Postgres checks. `gate` includes the Bifrost journeys but not the separately required `test:cards:integration`, `test:cli:journey`, `test:wyrdstate:journey`, `py:test:testing`, `py:test:integration`, or the complete non-Bifrost TypeScript integration lane. Those tests are gated/real-server journeys and their default family/unit selection is not substitute proof.
- Observable consequence: regressions in preserved Card, CLI, WyrdState, Python testing/integration, and non-Bifrost TypeScript journeys can remain indefinitely green on nightly `main`.
- Required testable correction: reuse the existing owning mise lanes from TASK-003 in the nightly workflow (directly or through the smallest existing aggregate that truly contains them); do not duplicate their commands or fold live-cloud/performance into nightly. Extend the CI-selection/workflow check to prove every required nightly lane is reachable.

### `TASK-REV-04` — MISSING: required credentialed live-cloud pass evidence is absent

- Violated obligation: TASK-003 requires credentialed cloud workflows to pass and attach their GitHub Actions evidence (`TASK-003-close-repository-integration.md:33-36,95-97,122-126`); REQ-064/INV-025/AC-022 prohibit treating an unexecuted required workflow as completion.
- Exact location: `changes/active/surfaces-oracle-integration/tasks/TASK-003-close-repository-integration.md:158-166` and its recorded open item for credentialed workflows.
- Evidence: the candidate explicitly records the live-cloud workflow as still needing a push and GitHub secrets and provides no immutable run URL or passing status. Local emulator/storage evidence cannot substitute for credentialed S3/GCS/Azure workflows.
- Observable consequence: the integration is marked implemented without proof that the reconciled SDK/server/storage paths work against their required real cloud backends.
- Required testable correction: run each owning credentialed live-cloud GitHub Actions workflow at this exact candidate tree (or an immutable tree-equivalent candidate after source remediation) with repository secrets, attach the run URLs and passing job statuses, and keep failures open rather than waiving them. No source abstraction or local mock is needed.

## Prior-finding closure

| Prior finding | Result |
|---|---|
| `FIND-TASK-002-1`, `-2`, `-3`, `-4`, `-5`, `-6`, `-9`, `-10`, `-11`, `-12`, `-13`, `-14` | CLOSED by the prior validated source/contracts work; no regression found in the cumulative candidate |
| `FIND-TASK-002-7` | CLOSED by the R3 documentation pass and governed lint evidence |
| `FIND-TASK-002-8` | CLOSED by the R3 module-top import pass and build/lint evidence |
| `FIND-TASK-002-15` | CLOSED; exact cumulative `git diff --check` is clean |
| `FIND-TASK-002-16` | CLOSED; shared-owner direct-send admission/shutdown ordering and focused proof pass |
| `FIND-TASK-002-17` | PARTIAL / REOPENED as `TASK-REV-01`; completed re-entry is fixed, but explicitly preserved future-cancellation resumability is still incorrect |
| `FIND-TASK-002-18` | CLOSED under the owner's explicit 2026-09-14 acceptance of the existing historical trailers without a history rewrite |

## Verification notes and limits

- Passed during this review: exact cumulative `git diff --check`; `mise run check:ci-selection` (31 classifier cases and 7 required-job aggregation cases); and the exact four-test `wyrd-client` nextest selection covering both R3 lifecycle fixes (4/4 passed, 183 skipped by the exact expression).
- Source-inspected: the complete base-to-candidate diff inventory, task/spec authorities, shared client/SDK topology, lifecycle implementations and callers, data-root composition, resource registration, workflow selection, nightly/performance workflows, legacy/native-artifact inventory, manifests, and relevant mise lanes.
- The broad `gate`, all capability/journey lanes, lints, codegen, docs, and cloud workflows were not rerun by this Wave-1 reviewer. Candidate-recorded local results were treated as available evidence but not as proof against the source defects above.
- GitHub secrets and workflow execution are unavailable in this workspace. Consequently no credentialed live-cloud, scheduled nightly, or scheduled performance run was independently executed or verified. TASK-REV-04 is a required evidence gap, not a speculative source defect.
- The accepted historical AI trailers remain present; this review did not rewrite history.
- No finding requires a new product, public API, architecture, security, compatibility, concurrency-semantics, resource-ownership, or persistent-data decision. Each correction stays in an existing owner or evidence lane.

## Overall result

**FAIL**

The candidate closes the direct-write race and completed settlement re-entry, preserves the converged client/SDK and single-root architecture, and carries substantial local verification. It does not yet satisfy the cumulative tasks because one R3 cancellation guarantee remains incorrect, the root preflight does not prove every child writable, nightly omits required gated correctness lanes, and required credentialed workflow pass evidence is absent.
