# Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-forge`
- Original implementation base: `c1508b375ba21a517f03ed6dd4d680dab4c3d12c`
- Remediation base: `e8d3cca13ccb799ec6dc5c69d09da3de40bffba9`
- Candidate: `7fcb45fc15ef2a43e8249af2a3dc7721fb55d517`
- Cumulative range: `c1508b375..7fcb45fc1`
- Remediation range: `e8d3cca13..7fcb45fc1`
- The checked-out branch had advanced to `5ab92b003`; every source citation and
  static check in this report used the candidate object through `git show`,
  commit-scoped diffs, or an archive of `7fcb45fc1`.
- Scope: repository standards only; task behavior is not adjudicated here.

## Authority coverage

| Changed surface | Governing authority read and applied | Coverage |
|---|---|---|
| Forge leader, scheduler, worker, promotion, maintenance, cleanup, Oracle active reads, Scribe publication, and DataFusion/Iceberg work | `AGENTS.md` §§4–6, 9–12; `architecture/agent-rules.md`; `architecture/bifrost-design.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/{rust-core,maintainer-style,testing-workflows}.md`; `architecture/references/domain/{vala-architecture,olap-serving,iceberg,datafusion,analytical-operations-reliability,arrow-analytical-interop}.md` | Complete for repository standards |
| SQL queries and migrations, tenant RLS, leader singleton, active-read and table-maintenance authority | `AGENTS.md` §§9, 11–12; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md` | Complete |
| Server boot and supervision, gRPC peer adapter, metrics, private/public process lifecycle | `AGENTS.md` §§5–6, 9, 12; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/references/{architecture/patterns,languages/rust-core,languages/maintainer-style}.md` | Complete |
| Rust public/private contracts, protobuf conversion, shared client and Rust SDK | `AGENTS.md` §§4–6, 9, 12; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/languages/{rust-core,errors,maintainer-style}.md` | Complete |
| Python/PyO3 package, exports, docstrings, generated stubs and tests | `AGENTS.md` §§7–8, 11–12; `architecture/references/languages/{pyo3-boundaries,python-api-and-stubs,testing-workflows,maintainer-style}.md`; original TASK-006 documentation contract | Complete |
| TypeScript/N-API source, declarations and tests | `AGENTS.md` §§2–3, 8, 11–12; `architecture/references/languages/{typescript-guide,testing-workflows}.md` | Complete |
| Arrow/Parquet schemas, managed columns, filtering and physical projections | `architecture/bifrost-design.md`; `architecture/agent-rules.md` field-by-name rule; `architecture/references/domain/{arrow-analytical-interop,telemetry-observations,iceberg,datafusion}.md` | Complete |
| Rust/Python/TypeScript journeys, SQL/integration tests, capacity harness and verification commands | `AGENTS.md` §§11–12; `architecture/agent-rules.md`; `architecture/references/languages/{spec-driven-development,implementation-execution,testing-workflows}.md` | Complete |
| Active spec/task/review artifacts and cumulative candidate scope | `architecture/references/languages/spec-driven-development.md`; `wyrd-task-review` immutable-subject and PASS rules; approved spec revision 11 | Complete |
| Cargo manifests/lockfile, `mise.toml`, generated JSON/protobuf/declarations, scripts and docs | `AGENTS.md` §§1, 4, 8, 11–12; `architecture/agent-rules.md`; `architecture/references/README.md` | Complete |

## Rule results

| Repository rule | Candidate evidence | Result |
|---|---|---|
| Immutable review covers the complete cumulative range | Candidate source was read from commit `7fcb45fc1`; the cumulative inventory contains 323 files and the remediation inventory contains 166 files | PASS |
| Prior rustdoc finding: every new/materially changed Rust item is documented with applicable errors, panics, cancellation and partial-progress behavior | The reviewer-named sites now carry substantive contracts, including `catalog/bifrost_catalog.rs::ensure_builtin`, `catalog/wire.rs::reject_reserved_field_names`, `forge/expire.rs::run_snapshot_expiry_for_table_inner`, `forge/orphan_gc.rs::load_maintenance_protection_inner`, `oracle/query_stream.rs::build_frames`, the Forge peer methods, test items and constants. The remediation records an empty changed-item source scan and final all-target Clippy pass | PASS |
| Prior import/signature finding: module-scope imports and bare names | Candidate inspection confirms the named local imports were moved to module/test-module blocks and the named qualified signatures were shortened. The only retained function-local imports found are `FutureExt as _` and `StreamExt as _` in one generic function, the rule's explicit narrow trait-import exception | PASS |
| Struct-centered ownership and concrete types for stateful lifecycles | Forge, Oracle, Scribe, table authority and SQL workflows use concrete owners, but the newly added restart lifecycle does not; see `STD-RR-003` | **FAIL** |
| Async only at IO or intentional async-composition boundaries | Changed async production paths await SQL, object storage, network, task, cancellation or stream lifecycle operations; no changed pure helper was found to be async solely for signature uniformity | PASS |
| No unjustified `#[allow]`, unsafe non-test `unwrap`, or gate weakening | Fresh candidate-archive runs of `check_unwrap_audit.py` and `check_clippy_allow.py` passed. No added `#[allow]` was found. The three `mocks-scope` entries are verified test-only files backed by dev-dependencies and the check's documented per-file exception mechanism | PASS |
| Tenant SQL uses `TenantConn`/`OperatorPool`, preserves caller transaction ownership and RLS | Fresh candidate-archive `check_tenant_isolation.py` passed; no widened pool signature or tenant transaction completion was found | PASS |
| PostgreSQL owns coordination time | Durable leader, claim, active-read and maintenance timing is evaluated by PostgreSQL; host `Instant`/Tokio time in the changed production paths governs only process-local renewal, backoff, timeout and measurement | PASS |
| Python tests remain top-level; public typing/runtime tests use public projections | Changed Python test functions are module-level. Nested functions are callback values, not nested tests. Imports use public `wyrd` projections | PASS |
| Deleted Python Args-format test did not weaken an approved property | The retired test required `name (type):` prose. TASK-006 explicitly prohibits duplicating signature types as filler and says no permanent text-mirroring linter is required; annotations plus `py:typecheck` own the type property. Its deletion is diagnosed in TASK-006-R1 | PASS |
| PyO3 stays at its approved boundary; `wyrd-spec` remains PyO3/IO/async free | The candidate adds no PyO3 dependency or runtime behavior to `wyrd-spec`; wrappers remain in the Python SDK or approved existing owner modules | PASS |
| Generated schema/stub/declaration parity | TASK-006-R1 records generation from hand-authored sources and passing `codegen:check`; the final aggregate includes codegen. No generated-only source of truth was found | PASS (recorded final-gate evidence) |
| Sanctioned mock allowlist does not hide production use | `wyrd-client/src/storage/upload/tests.rs` is included under `#[cfg(test)]`; `wyrd-client/src/bifrost/grpc.rs` uses wiremock only in its `#[cfg(test)] mod tests`; `wyrd-server/tests/pg_verification_routes.rs` is an external test; both Cargo manifests declare wiremock only under `[dev-dependencies]` | PASS |
| Task artifacts identify their approved authority and do not contradict it | Every implementation/remediation task is `status: review`, but the index still declares revision 10 as the only authority and TASK-004 has no required `spec_revision`; see `STD-RR-002` | **FAIL** |
| Candidate contains only the reviewed packet | Commit `406464a59` adds the unrelated `changes/active/bifrost-variant/spec.md` inside the immutable cumulative/remediation range; see `STD-RR-001` | **FAIL** |
| Clean diff and broad final verification | Fresh `git diff --check c1508b375..7fcb45fc1` passed. TASK-PACKET-R1 records the 16/16 release benchmark and `mise run gate` passing at `30d31ebac`; the only later code-neutral commits add the unrelated draft spec and record that evidence | PASS for the Forge code; unrelated-scope finding remains |

## Material findings

### STD-RR-001 — Unrelated Bifrost Variant specification is inside the immutable candidate

- **Classification:** DRIFT.
- **Rule:** The task-review completion rule requires the cumulative immutable
  diff and returns PASS only when no unrelated change entered that diff.
- **Location:** `changes/active/bifrost-variant/spec.md:1-686`, introduced by
  `406464a59bea7eb2b73c739e74d98ac7b357b22e` between the passing gate commit and
  the reviewed candidate.
- **Evidence:** The file defines a separate `SPEC-bifrost-variant` change about
  Variant/Struct storage, Iceberg v3, shredding and query operators. None of the
  Forge concurrent-planning spec, original tasks, prior findings, or
  remediation tasks authorizes it; the implementer also labels it another
  session's draft.
- **Consequence:** The candidate cannot be accepted as the immutable result of
  this task packet because approval would also approve an unreviewed, unrelated
  product specification.
- **Testable correction:** Remove commit `406464a59` (or otherwise produce a
  new candidate whose cumulative range omits this file) without changing the
  Forge implementation, then confirm the cumulative name-status inventory and
  `git diff --check` are clean.

### STD-RR-002 — The task index contradicts revision 11 and TASK-004 omits required authority metadata

- **Classification:** VIOLATION.
- **Rule:** `architecture/references/languages/spec-driven-development.md`
  requires each task to identify its approved spec revision; active task
  artifacts must preserve unambiguous authority. Revision 11 is the approved
  behavior authority and supersedes conflicting older task prose.
- **Locations:**
  - `changes/active/forge-concurrent-planning/tasks/README.md:3,14` says the
    revision-10 spec is approved and is the *only* Forge/Oracle authority.
  - `changes/active/forge-concurrent-planning/tasks/TASK-004-compaction-defaults-and-type.md:1-7`
    has no `spec_revision` field, although it was created against approved
    revision 6 (`3ffbd1832`).
- **Evidence:** `spec.md:1-5` identifies approved revision 11, and its revision
  history records revision 11's material active-read scope change. The other
  task headers retain their derivation revision; all task and remediation
  statuses are otherwise correctly `review`.
- **Consequence:** A fresh implementer or reviewer following the task index is
  explicitly directed to superseded authority and cannot establish which
  approved contract TASK-004 was derived from.
- **Testable correction:** Change the index to name revision 11 as the current
  authority while preserving the historical revisions on individual tasks,
  and add `spec_revision: 6` to TASK-004. Re-scan every task/remediation header
  to confirm `status: review`, `spec`, and `spec_revision` are present and
  consistent with its derivation.

### STD-RR-003 — Restart supervision is a boxed free lifecycle instead of a concrete owner

- **Classification:** VIOLATION.
- **Rule:** `AGENTS.md` §5 and `architecture/agent-rules.md` require one clear
  concrete struct for stateful capabilities and multi-step lifecycles; module
  free functions are limited to stateless deterministic helpers. `Box<dyn
  Trait>` is reserved for intentional runtime extensibility.
- **Location:** `crates/wyrd/wyrd-server/src/app/supervise.rs:78-181` adds
  `restarting_worker<B, F, E, BE, R>`; its call sites are
  `app/mod.rs:126` and `app/server.rs:632,653`.
- **Evidence:** The free function owns a mutable worker instance, restart
  backoff, cancellation token, worker identity, constructor and restart
  callback across a loop, then erases that lifecycle behind
  `Pin<Box<dyn Future<Output = TaskExit> + Send>>`. This is stateful orchestration,
  not a deterministic conversion, and there is no runtime-pluggable future
  implementation to justify the trait object. The focused test at
  `supervise.rs:623-682` proves behavior but not the required structural shape.
- **Consequence:** The restart policy's state and invariants have no discoverable
  owner, and callers receive an opaque future rather than a type on which the
  lifecycle can be understood and safely changed; this is the exact functional
  dependency-threading shape the repository's hard struct-centered rule rejects.
- **Testable correction:** Put name, shutdown, current instance, builder,
  restart observer and backoff on one focused concrete restart owner with an
  eager fallible constructor and an async run method; keep the existing three
  call sites and restart behavior, remove the boxed dynamic-future return, and
  rerun the focused restart test plus the server/Forge journey that proves API
  availability during worker replacement.

## Verification notes

- Independently run against the candidate:
  - `git diff --check c1508b375..7fcb45fc1` — PASS.
  - `bash scripts/checks/mocks-scope.sh` from an archive of `7fcb45fc1` — PASS.
  - `uv run python scripts/check_unwrap_audit.py` from that archive — PASS.
  - `uv run python scripts/check_clippy_allow.py` from that archive — PASS.
  - `uv run python scripts/check_tenant_isolation.py` from that archive — PASS.
  - Static cumulative/remediation scans for changed files, added
    `#[allow]`/`#[ignore]`, local imports, qualified signatures, non-test
    unwrap/expect use, Python test nesting, task header state and generated
    outputs.
- Accepted as recorded evidence:
  - `mise run gate` at `30d31ebac` — exit 0 after the three verified test-only
    wiremock exceptions.
  - `mise run bench:bifrost:forge-capacity` at `ec9692bcf` — 16/16 checks.
  - TASK-PACKET-R1's changed-symbol documentation/source scans, format, lints,
    codegen, docs, integration and boundary results.
- Not rerun: the 1,638-second aggregate gate. Candidate code after its recorded
  pass is unchanged; the later in-range file is an unrelated Markdown spec and
  is independently blocking under `STD-RR-001`.

## Overall result

**FAIL**

The four prior packet standards findings are closed, but the immutable
candidate contains one unrelated change, contradictory/incomplete task
authority metadata, and a newly introduced stateful restart lifecycle that
violates the repository's mandatory struct-centered shape.
