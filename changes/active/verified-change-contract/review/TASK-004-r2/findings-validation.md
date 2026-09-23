# TASK-004 R2 Structured Ponytail Validation

## Immutable subject

- Base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Candidate: `2af4cc3ff95a609d1df682be4f633345f96934e1`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Prior verdict, finding ledger, and remediation task:
  `changes/active/verified-change-contract/review/TASK-004-r1/`
- Wave 1 inputs: `task-review.md`, `standards-review.md`,
  `domain-review-security-tenancy-audit.md`,
  `domain-review-persistence-concurrency.md`, and
  `domain-review-bifrost-publication.md`

The candidate remained
`2af4cc3ff95a609d1df682be4f633345f96934e1` throughout validation.
`.codegraph/` is absent, so caller and implementation tracing used repository
search and direct source inspection.

## Wave 1 disposition

| Wave 1 proposal | Disposition | Independent validation |
|---|---|---|
| Task review: empty proposed ledger | **REVISED** | Most of the acceptance matrix is supported, but the scheduler's cancellation-sensitive `turn` owns and can drop `TenantConn::commit`; the candidate itself concedes that this can leave a committed occurrence after cancellation. The task review's closure of prior `FIND-TASK-004-5` therefore relies on a test that exercises only the pre-commit rollback branch. |
| `STD-004-R2-001` | **CONFIRMED** | Every cited item is new or materially rewritten in the cumulative diff and uses a qualified type in a field, signature, return, bound, or impl header despite the explicit bare-type rule in `architecture/agent-rules.md`. The production composition method has one live caller, the health field has live readiness/runtime consumers, the ID macro supplies widely used durable types, the publisher `Debug` impl is the required redacted owner representation, and `AnyTableScribe` is used by the task-required Gate matrix tests. This is a mechanical source-shape correction, not a reason to add a lint or abstraction. |
| Security/tenancy/audit review: empty proposed ledger | **CONFIRMED** | Direct inspection supports closure of prior `FIND-TASK-004-4`; no retained Wave 2 finding arises from this report. |
| `PERSIST-R2-001` | **CONFIRMED** | `VerificationRuntime` is the sole production caller of `VerificationScheduler::run`. Its biased `select!` drops the whole pass whenever `stop` becomes ready. `schedule_tenant` includes `conn.commit().await` inside that dropped future. SQLx sends `COMMIT` and only marks the transaction closed after awaiting the server response; dropping after send queues rollback on a connection whose commit may already have landed. The scheduler test blocks the run write before commit and therefore cannot cover this admitted in-flight-commit branch. The remediation record expressly concedes the branch while its acceptance criterion requires known scheduler/runner admission ordering. Prior `FIND-TASK-004-5` remains open. |
| Bifrost publication review: empty proposed ledger | **CONFIRMED** | The reviewed result mapping, Gate/Scribe publication, role-separated identity journey, and detail-ACK crash/reclaim paths close prior `FIND-TASK-004-2`, `FIND-TASK-004-6`, and `FIND-TASK-004-7`; no retained Wave 2 finding arises from this report. |

The two retained issues are independent. `FIND-TASK-004-5` is a reachable
shutdown-ordering defect in the runtime. `FIND-TASK-004-8` is a bounded
repository-rule violation whose correction changes type spelling only.

## Final deduplicated finding ledger

### FIND-TASK-004-5 — Scheduler shutdown still has an unknown commit outcome

- **Wave 1 source IDs:** `PERSIST-R2-001`
- **Status:** **CONFIRMED** (prior stable finding remains open)
- **Classification:** INCORRECT
- **Violated obligation:** The R1 remediation of `REQ-146`, `AC-030`, and
  TASK-004 Scenario 7 requires scheduler cancellation and occurrence commit to
  have one known ordering: shutdown wins only with a known rollback, while a
  completed commit may be classified as admitted before closure. Its focused
  acceptance criterion requires a lock-controlled proof that cancellation
  admits no unknown new durable scheduler work.
- **Exact locations:**
  `crates/wyrd/wyrd-server/src/verification/scheduler.rs:99-117,134-166`;
  `crates/wyrd/wyrd-sql/src/tenant_conn.rs:67-73`;
  `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs:1181-1250`;
  `changes/active/verified-change-contract/review/TASK-004-r1/TASK-004-R1-close-validated-runtime-gaps.md:94-111,168,226,248`.
- **Evidence and reachability:** `Capability::spawn` starts the only production
  scheduler loop from `VerificationRuntime`. The loop places `pass`, every
  tenant iteration, and the consuming `TenantConn::commit` future inside the
  `turn` branch raced against `stop`. SQLx's Postgres commit path sends
  `COMMIT`, awaits readiness, and only then marks the transaction closed; its
  drop path queues rollback while the server may already have accepted the
  commit. Thus cancellation can discard the only future capable of reporting
  whether the run/cursor transaction committed. The added scheduler test holds
  the transaction before `COMMIT`, cancels it, and proves ordinary drop
  rollback. It never defers `COMMIT` itself. The implementation evidence's
  stated limit that an in-flight scheduler commit may still land confirms this
  remaining reachable branch.
- **Observable consequence:** shutdown can return from the scheduler after
  discarding an unknown commit, then expose a newly pending run and advanced
  binding cursor after the cancellation boundary. Restart observes different
  durable schedule state even though the claimed closure proof passed.
- **Decision-complete minimum correction:** keep the existing scheduler,
  tenant transaction, cancellation token, and atomic run/cursor transaction.
  Establish one linearization point before commit: if cancellation wins before
  that point, roll back and begin no later occurrence or tenant; once commit is
  selected, await it to a known result without cancellation and classify a
  successful transaction as admitted before scheduler closure, then exit if
  cancellation arrived. Do not add a shutdown table, admission registry,
  compensating delete/cursor rewrite, second scheduling protocol, or recovery
  coordinator.
- **Focused closure proof:** add one real-Postgres deferred-commit test, using
  the existing lock/fault machinery, that cancels while scheduler `COMMIT` is
  blocked. Release the commit and prove the selected ordering has a known
  result: either zero runs with the original cursor when cancellation won
  before commit selection, or exactly one atomic run/cursor advance classified
  before closure when commit selection won. Prove no next occurrence or tenant
  begins after cancellation and restart does not change that state. Retain the
  existing pre-commit rollback and runner late-commit release tests.

### FIND-TASK-004-8 — New Rust items still bypass the module import manifest

- **Wave 1 source IDs:** `STD-004-R2-001`
- **Status:** **CONFIRMED**
- **Classification:** VIOLATION
- **Violated obligation:** `architecture/agent-rules.md` requires types in
  struct fields, function parameters and returns, trait bounds, `where`
  clauses, and impl headers to be imported in the module's top-level `use`
  block and used by bare name. The rule applies to production and test code.
- **Exact locations:** `crates/wyrd/wyrd-server/src/app/server.rs:513-517`;
  `crates/wyrd/wyrd-server/src/state.rs:2088`;
  `crates/wyrd/wyrd-server/src/verification/publisher.rs:86-88`;
  `crates/wyrd-spec/src/ids.rs:264-267`;
  `crates/vala/vala-bifrost-redux/src/gate/mod.rs:2493-2508`.
- **Evidence and reachability:** `BoundServer::verification_runtime`, called by
  the live server composition at `server.rs:699`, spells its runtime types
  through `crate::verification`; `AppState::verification` is consumed by
  runtime composition and readiness; the required redacted publisher `Debug`
  return remains `fmt::Result`; the new `uuid7_id_type!` macro emits qualified
  `fmt` types for four durable IDs used throughout the task; and the two
  task-required Gate matrix tests instantiate `AnyTableScribe`, whose impl
  header, parameter, error, and return types use `crate::contracts` paths.
  These are compiled, required surfaces rather than dormant extensibility.
- **Observable consequence:** the cumulative candidate still violates the
  repository's mandatory dependency-manifest source shape at composition,
  state, durable identity, publisher, and Gate-test seams. Runtime behavior is
  unchanged, but repository acceptance is not met.
- **Decision-complete minimum correction:** add the exact cited types to each
  module's existing top-level import block and use their bare names in the
  cited fields, signatures, return types, and impl headers. Use a narrow alias
  only where `std::fmt::Result` would collide with the ordinary `Result` type.
  Preserve module structure and behavior; add no helper, abstraction,
  dependency, or permanent source check. Reinspect the full cumulative Rust
  diff for the same prohibited signature shape so this mechanical correction
  does not repeat the R1 under-scan.
- **Focused closure proof:** source inspection of the complete cumulative Rust
  diff shows no qualified types in newly added or materially modified fields,
  signatures, bounds, or impl headers. Run `mise run fmt`, `mise run lints`,
  and the affected `wyrd-server`, `wyrd-spec`, and
  `vala-bifrost-redux` compile/test lanes. No new behavior test is warranted.

## Prior-finding closure

| Prior finding | Result | Validation |
|---|---|---|
| `FIND-TASK-004-1` | CLOSED | Runtime and fixture tenant work now enters through `WyrdPostgres::tenant_conn`; the raw-pool wrapper is gone. |
| `FIND-TASK-004-2` | CLOSED | Result columns are assembled against table-owned schemas by name with reordered and malformed-column proof. |
| `FIND-TASK-004-3` | CLOSED AS SCOPED | Every R1-cited location was corrected. `FIND-TASK-004-8` covers distinct new cumulative-diff locations omitted from that remediation scope. |
| `FIND-TASK-004-4` | CLOSED | Exact Verifier scope participates in the canonical Gate decision before its single audit append. |
| `FIND-TASK-004-5` | OPEN | Runner cancellation ordering is closed, and scheduler pre-commit rollback is proved; scheduler in-flight commit remains unknown. |
| `FIND-TASK-004-6` | CLOSED | The existing role-separated journey now proves non-empty binding result/detail identity. |
| `FIND-TASK-004-7` | CLOSED | The focused test crosses durable detail ACK, unknown summary outcome, crash, reclaim, and final fenced dispatch. |

## Specification decision

Neither correction requires a new product, public API, architecture,
security-policy, compatibility, persistent-data model, or cross-service
decision. Both fit the approved runtime semantics and repository rules.
`SPEC_REVISION_REQUIRED` does not apply. The validated ledger is non-empty and
contains `FIND-TASK-004-5` and `FIND-TASK-004-8`.

## Verification limits

- This validation inspected the complete Wave 1 report set, the cited source,
  all callers of the scheduler and cited correction surfaces, the full bodies
  of the scheduler run/pass/tenant paths and `TenantConn::commit`, SQLx's
  transaction commit/drop behavior, the R1 remediation contract, and the
  cumulative diff for the standards citations.
- It did not rerun the repository's Postgres, SDK, MCP, codegen, lint, or broad
  test lanes. Their recorded green results do not exercise the deferred
  scheduler-commit branch and cannot waive the explicit source-shape rule.
- Candidate identity was rechecked before this artifact was written and
  remained `2af4cc3ff95a609d1df682be4f633345f96934e1`.
