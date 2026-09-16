# Persistent Data, Resource Ownership, Recovery, and Audit Review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `bbfdf35e26212b2a831bda5e31e1ef4433e41900`
- Candidate tree: `58f5c2acfe8da01f3e4b58363f38ce761626df35`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 8
- Reviewed tasks: original `TASK-002`, its prior verdict and R3 remediation,
  `TASK-004`, and `TASK-003`

The candidate commit and tree matched the supplied immutable identities at the
start and end of inspection. The shared working checkout contains only review
outputs under the assigned review directory and an unrelated untracked
`changes/active/verified-change-contract/architecture/verifier/` directory;
neither is part of the candidate tree or this review's source evidence. The
repository has no `.codegraph/` index, so source and caller tracing used Git,
`rg`, and direct inspection.

## Findings

### `PERSIST-DATA-01` — INCORRECT: root preparation can activate Scribe with unusable managed children

- **Violated obligation:** specification REQ-055 and INV-022 require creation
  and validation of the root-derived Scribe/Oracle paths before role activation
  and forbid readiness with an unusable Bifrost root. TASK-004 lines 50-53 and
  62-64 specifically require an unwritable or contradictory root to fail with
  no partial role activation; AC-018 requires proof of failure before readiness.
  `architecture/bifrost-design.md` makes the exclusively locked root the owner
  of every local path and requires startup recovery to fail closed on ambiguous
  authority.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/boot/data_root.rs:71-108,140-188`;
  `crates/vala/vala-bifrost-redux/src/resources.rs:518-606,753-776`;
  `crates/wyrd/wyrd-server/src/boot/mod.rs:439-491,606-652,790-817`;
  first staged-record mutation at
  `crates/vala/vala-bifrost-redux/src/scribe/hot_stage.rs:594-620`.
- **Evidence:** `BifrostDataRoot::prepare` calls `create_dir_all` for the root,
  `scribe-stage`, `scribe-output-scratch`, and `oracle-spill`, then proves only
  that the root's `.lock` file is writable and lockable. `create_dir_all`
  succeeds for an already-existing directory even when that directory cannot
  create children. `BifrostVolumeGovernor::register` then lists the output
  scratch namespace, reads metadata and existing durable occupancy, and probes
  free capacity, but performs no write probe for the stage or output directory.
  An empty readable-but-unwritable `scribe-stage` or
  `scribe-output-scratch` therefore passes every pre-activation check. Scribe
  replays existing WAL and activates at `boot/mod.rs:806` before its first
  staged member creates a directory or writes the temporary record. The current
  tests prove only a file-backed uncreatable root and lock contention; they do
  not exercise an existing unwritable managed child. The same construction also
  follows a pre-existing child symlink, so the lexical `starts_with` assertion
  does not prove that the actual managed directory remains inside the locked
  root.
- **Reachable consequence:** a pod can report its Scribe role ready with an
  empty read-only stage/output mount, then acknowledge WAL-backed writes until
  rotation reaches staging and fails. That defers an explicitly boot-time
  configuration failure into live durable operation. A child symlink can place
  staged or scratch files outside the exclusively locked root, allowing a
  second configured root to alias the same mutable namespace and defeating the
  claimed single-owner boundary.
- **Required testable correction:** make the existing `BifrostDataRoot`
  preparation boundary prove that every managed directory is an owned,
  non-escaping directory on which the role can perform its required create and
  remove operations, and return the existing structured `Unusable` failure
  before external dependency composition or role reservation when any proof
  fails. Preserve existing contents, the one root lock, restart identity, and
  Oracle's own prefixed cleanup rules. Add focused boot/root tests for an
  existing readable-but-unwritable Scribe managed child and for an escaping
  child link, asserting refusal before role activation; retain the existing
  default, override, restart, and lock-contention proofs.

### `PERSIST-DATA-02` — VIOLATION: stalled Oracle audit commits accumulate an unbounded Wyrd-owned task and event backlog

- **Violated obligation:** `architecture/bifrost-design.md` Resource and
  failure invariants require every Wyrd-owned task set and queue to be bounded.
  Spec REQ-026/REQ-026A requires Oracle audit commits to remain tracked and
  non-blocking without starving reader-epoch renewal or query execution, with
  failures logged and counted. AGENTS.md section 6 requires bounded concurrency
  at external-call boundaries. The TASK-003 correction addressed pooled
  connection starvation but must preserve those surrounding ownership
  invariants.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:34-46,54-105` and
  `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:314-371`.
- **Evidence:** `OracleQueryAudit::new` creates a semaphore for one quarter of
  the Vala pool, but `stage` first spawns an owned `TaskTracker` task containing
  the complete `AuditEvent`; the spawned task then waits asynchronously for a
  connection permit. Neither the task set nor the permit waiters have a count
  or byte ceiling. The new journey deliberately locks one tenant's
  `audit_chain_head`, issues twice the pool size in completed reads, and asserts
  that at least a pool's worth of audit tasks remain pending. It proves spare
  database capacity, but it does not prove a finite task/event backlog. Because
  query rows are not held for audit completion, each completed request releases
  query admission while its audit task survives, so the query-slot limit does
  not bound accumulation across successive reads.
- **Reachable consequence:** while a tenant chain-head transaction remains
  stalled, sustained reads continue succeeding and allocate one Tokio task,
  semaphore waiter, and owned audit event per request without limit. Memory can
  grow until the server is killed; an abrupt kill then loses every not-yet-
  committed decision and interrupts unrelated tenant query, recovery, and
  retained-publication work. Shutdown also inherits an unbounded amount of
  tracked work to report or await.
- **Required testable correction:** bound total pending Oracle audit ownership,
  not only the number of tasks holding database connections, using the existing
  `OracleQueryAudit` owner and a capacity derived from existing runtime/pool
  limits rather than a second durable path or audit WAL. Saturation must keep
  reads non-blocking and follow the already-approved audit-commit failure
  behavior by logging and incrementing `oracle_audit_commit_failures_total`;
  preserve the connection share reserved for reader renewal and queries. Extend
  the locked-chain-head journey (or a focused owner test plus that journey) to
  drive more requests than the bound and prove pending ownership stays finite,
  spare pool access remains available, saturation is counted, release drains
  accepted commits, and shutdown has bounded outstanding work.

## Reviewed boundary and source coverage

| Boundary | Authority | Source and lifecycle traced | Result |
|---|---|---|---|
| Unified local root, configuration, readiness, and replica ownership | REQ-055/055A, INV-022, AC-018; TASK-004; Bifrost durability and recovery authority | `config.rs`; `boot/data_root.rs`; production `build_state`/`compose_bifrost`; `app::run`; `state.rs`; resource-volume registration; Kubernetes mixed and role-separated manifests; deployment validation; in-process and child-process test builders | **FAIL**: root and replica lock are retained correctly, but Scribe child usability/containment is not established before activation (`PERSIST-DATA-01`) |
| Scribe WAL, node identity, staging, publication, restart, and shutdown/drop order | Bifrost durability/visibility and live-tail recovery; TASK-004 restart and identity acceptance | node-identity load/create; `WalWriter` construction/replay; stage and output owners; Scribe activation cleanup; abrupt and graceful restart harnesses; `BootedServer`/`ComposedBifrost` ownership; runtime and lock retention in `app::run` | PASS apart from the pre-activation root validation gap above; no second configured WAL root or weakened replay/fence identity found |
| Oracle scratch/spill and follower cleanup | Bifrost distributed resource invariants; REQ-055; INV-021/022 | `OracleSpillRuntime` stale-prefix cleanup, process `TempDir`, query runtime; Oracle role reserve/build/activate ordering; follower worker-context detachment; ownership inspection and shutdown paths | PASS: Oracle creates its active child before activation, bounds query scratch through admitted ownership, and no remaining follower cycle retaining spill was found |
| Forge local resources, durable tasks, recovery, and cleanup | Bifrost Forge scheduling/fencing/reconciliation; INV-021/022; TASK-003 closeout | worker-local FIFO admission; table-policy extraction; recovered-claim settlement; orphan-cleanup uniqueness/coalescing migration and SQL; worker observation/drop order; deployment targets | PASS: no Forge spill/scratch path exists, durable claims retain their fencing/settlement authority, and the reviewed cleanup changes do not expose a second local-data owner |
| Audit staging, retained publication, tenancy, and restart replay | REQ-014, REQ-026 through REQ-029, REQ-026B, INV-008 family; AGENTS audit rules; Bifrost read-audit/publication authority | canonical append; tenant chain; frozen range/list/settle SQL; local-Scribe publication; batch-id replay; system-owner tenant seed/verification; publisher sweep and multi-tenant journeys | **FAIL** only for unbounded Oracle task ownership (`PERSIST-DATA-02`); the one staging table, one publisher, per-tenant frozen upper bound/watermark settlement, system-owner sentinel, and Scribe dedup replay remain coherent |

No additional material defect was found in the Scribe accounting settlement,
Forge recovered-cleanup settlement/deduplication, follower spill-cycle fix,
system-owner UUIDv7 persistence, deployment per-replica volumes, or retained
audit freeze/publish/settle protocol.

## Verification limits

- This was a read-only source, history, configuration, migration, caller, and
  test-evidence review. No implementation source was edited and no test suite
  was rerun.
- TASK-004 records passing focused root/configuration tests, Bifrost unit and
  journey lanes, deployment/resource checks, formatting, lints, unwrap audit,
  and `git diff --check`. TASK-003 records broad local closeout evidence. Those
  results do not exercise an existing unwritable or escaping managed child and
  explicitly allow the audit pending count to grow with issued reads, so they
  cannot close the two findings.
- Credentialed live-cloud storage, pushed CI performance, and nightly workflows
  remain externally unavailable according to TASK-003. They are residual
  verification gaps, not the basis of either finding.
- `git diff --check` for the immutable base-to-candidate range completed without
  output during this review.

## Overall result

**FAIL** — the candidate preserves the intended one configured root, exclusive
replica lock, stable Scribe identity/replay, Oracle spill lifecycle, Forge
no-spill boundary, and audit publication durability, but it does not prove all
managed Scribe children usable before readiness and replaces audit connection
starvation with an unbounded in-memory task/event backlog. Proposed finding
ledger: `PERSIST-DATA-01`, `PERSIST-DATA-02`.
