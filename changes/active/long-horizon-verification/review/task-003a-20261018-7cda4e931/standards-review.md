# Independent repository standards review

**Result: FAIL — mandatory Rust documentation is incomplete (STD-001).**

Subject: base `22335c2c40a5ad24c27bfa0d4686f4da49f922c5`, candidate
`7cda4e93159ab328de2a74068d5e1978a2590818`, complete three-dot diff
(54 files). HEAD still equals the candidate. This is a repository-compliance
review, not a task-acceptance or simplification verdict. No source was changed.
No older assessment packets or other reviewers' conclusions were consulted.

## Authority coverage

All rows also apply `AGENTS.md`, `architecture/agent-rules.md`, the reference
router (`architecture/references/README.md`), and the immutable subject. Reference
files named below were read as complete slices; the architecture sections named
below are the applicable governing anchors. No nested AGENTS.md or CLONES.md was
found under the changed crates, SDKs, docs, fixtures, or architecture directories.
CodeGraph was used before source navigation and for caller/consumer expansion.

| Changed surface (complete range) | Applicable authority |
|---|---|
| `wyrd-spec/src/card/trigger.rs`, `verification.rs`, `verification/execution.rs` | AGENTS §§2–5, 9, 16; design Doctrine, Client model, Verifier, Trigger; doctrine `positioning-and-vocabulary.md`, `architecture-constraints.md`; `architecture/patterns.md`; languages `rust-core.md`, `errors.md`, `agent-harness.md`; domain `evaluation.md` |
| Server `boot/mod.rs`, `components/cards/service.rs`, `config.rs`, `verification/{mod,hosted/mod}.rs` | AGENTS §§2–6, 9, 15–16; design Verifier/Trigger/Coordination clock; `architecture/patterns.md`, `rust-core.md`, `maintainer-style.md`, `evaluation.md`, `vala-architecture.md` |
| Server `scribe_outbox.rs`, gateway `capture.rs`, Oracle `peer_audit.rs`, `verification/fault.rs`; testing `src/server.rs`, `tests/bifrost/server/verification_runtime.rs` | AGENTS §§2, 4–6, 9–11, 16; design Verifier and Bifrost; Bifrost System boundary, Table and row identity, Durability and visibility, Read audit and terminal contract, Resource and failure invariants; security Tenant and data isolation/Audit integrity and privacy; patterns, Rust, testing, telemetry-observations, analytical-operations-reliability, vala-architecture |
| Server `verification/{claims,engines,groups,operators,runner,scheduler}.rs`, `query/scheduled.rs` | AGENTS §§3–6, 9–11, 15–16; design Verifier, Bifrost, Coordination clock, Trigger; Bifrost immutable planning/source authority and terminal/resource contracts; security Authorization and policy/Tenant and data isolation/Audit integrity and privacy; patterns, Rust, errors, testing, evaluation, telemetry-observations, olap-serving, arrow-analytical-interop, analytical-operations-reliability |
| SQL `queries/{verification,verifier_executions}.rs`, three changed migrations, SQL integration test | AGENTS §§4–6, 9, 11, 15–16; agent-rules SQL capability, RLS and transaction lifecycle rules; design Verifier and Coordination clock; patterns, Rust, testing, analytical-operations-reliability; approved pre-release in-place schema constraint |
| Server `tests/integration/{pg_openapi_contract,pg_verification_groups}.rs` | AGENTS §§11, 16; languages testing-workflows, Rust, maintainer-style, agent-harness; design Verifier and public surfaces; ordinary query authorization and terminal rules |
| MCP `tests/bifrost/mcp/verification.rs` | AGENTS §§2, 9, 11, 16; languages agent-harness, testing-workflows, Rust; design Verifier MCP projection; security authorization and tenant boundaries |
| Rust SDK integration journey | AGENTS §§2–6, 11, 16; design Client model and Verifier; languages Rust, testing-workflows, maintainer-style; evaluation and telemetry-observations |
| Python authored `verify/__init__.pyi` and Python integration journey | AGENTS §§2–3, 8, 11, 16; design Client model; languages python-api-and-stubs, testing-workflows, maintainer-style; shared wire contract. No PyO3 implementation or registration change occurred |
| TS generated `card-types.ts` and TS integration journey | AGENTS §§2–3, 11, 16; design Client model; languages typescript-guide, testing-workflows, agent-harness, maintainer-style; generated-source rule |
| All 14 changed YAML fixture files (coding-cycle/external/unbound/coding, forever, frames, late, refund-on-failure/review/refund, support-ambiguous/undeclared/unsourced/support) | AGENTS §§2, 10–11; design Verifier/Trigger and declarative Card envelope; positioning-and-vocabulary, evaluation, testing-workflows |
| Architecture change and all three docs pages | AGENTS §§1–2, 11–12; design wins over prose/generated output; doctrine, positioning-and-vocabulary, agent-harness, errors; after-only contract and accepted visibility/loss tradeoffs |
| Appended task evidence | AGENTS §§11–12, 14–16; languages spec-driven-development and implementation-execution; immutable task/spec revision 17; canonical commands in mise.toml |

The full `wyrd-doctrine.mdx` was read. Iceberg/DataFusion execution internals,
drift engine changes, PyO3/N-API implementations, frontend, external URL fetching,
and deployment mechanisms are not materially changed; their implementation-specific
reference routes do not independently apply. This does not waive the unchanged
query/admission/security contracts the changed callers consume.

## Per-rule results and source proof

| Rule | Result | Source evidence |
|---|---|---|
| Pure foundational contracts; durable server ownership (AGENTS §§2–3, 9) | PASS | Trigger and GroupSnapshot changes are declarative serde/schema Rust; SQL and snapshot orchestration remain in wyrd-sql/wyrd-server. No manifests, feature definitions, or lockfiles changed; inspected owner manifests retain existing serde_jcs/sha2/Arrow/sqlx dependencies |
| Exact shared wire vocabulary and typed public fields (AGENTS §§2, 9) | PASS | `trigger.rs:49` has required `after`; `verification.rs:242,271,279` define settings and minimal snapshot; `execution.rs:182` projects it. Trigger test rejects removed clock fields; served OpenAPI test asserts exact settings/snapshot fields |
| Domain identity types (AGENTS §4) | PASS | `GroupActivity` and `DueGroup` retain CardUid/GroupId/BindingId; `PinnedGroup` uses ExecutionId/VerificationResultId. Text/UUID arrays occur at SQL/Arrow boundaries, not as replacement public identity contracts |
| Concrete retained-state owners and synchronous pure work (AGENTS §§5–6) | PASS | GroupPoller retains state/deadline; Reader retains query/deadline; Sources/Visible retain selection state; scheduler Groups retains shared capacity. `Sources::{new,read,decode}`, `Visible::{executions,pin}`, `PinnedGroup::new` are synchronous; async methods await or compose actual IO |
| Tenant capability signatures and fields (agent-rules raw-pool rule) | PASS | `record_group_activity`, `claim_due_group`, `consume_group`, `enqueue_group`, `complete` accept `&mut TenantConn`; due-tenant enumeration accepts `&OperatorPool`. ScribeSink retains WyrdPostgres and acquires a tenant connection. No new raw PgPool/PgConnection/caller-transaction capability was introduced |
| Caller owns TenantConn lifecycle; RLS supplies tenant predicates | PASS | SQL queue methods never commit supplied connections. `ScribeSink::schedule_groups` owns/acquires its connection and commits it; scheduler acquires/commits its own transaction. Tenant claims/updates have no parallel tenant predicate; cross-tenant lists use OperatorPool |
| New tenant schema protection | PASS | `20261018000000_verification_group_pending.sql:10–35` declares tenant/binding/group PK, composite FK/cascade, due/claim indexes, ENABLE + FORCE RLS, tenant USING/WITH CHECK, and operator-session policy |
| Database owns relative coordination timestamps (AGENTS §15) | PASS | activity upsert uses `statement_timestamp()` and stored after duration; due queries use database time; completion uses database `settled_at`. No process Utc::now is bound into a new coordination predicate |
| One Scribe write path; non-blocking originating audit behavior (AGENTS §§2, 9) | PASS | `ScribeSink::write` invokes scheduling after frame ACK and before submission reservation handoff; production boot injects the same Postgres owner into the existing sink. No new audit queue, canonical writer, or permission-decision mechanism appears |
| Ordinary authorized query path and exact source isolation | PASS | `GroupPoller::poll:106–123` uses accepted owner authority and ScheduledQueryCaller; Sources read predicates include owner/subject/source-Verifier/group; string literals pass through ObservationWindow::text. SQL row tenancy remains RLS-bound |
| Terminal-safe analytical reads / no partial success | PASS | `Reader::run:158–182` collects through ScheduledQueryCaller and rejects non-success terminals. Scheduler failed reads return without consuming/committing pending work. Name-based column lookup is preserved in decode helpers |
| Portable implementation and existing resource ownership (AGENTS §§2, 6) | PASS | Shared Tokio Semaphore capacity and bounded scheduler passes replace the former active count; no platform cfg, OS sandbox, new runtime, worker-per-row, dependency, or concurrency setting added |
| Typed local errors / stable external catalog (AGENTS §4; errors reference) | PASS | New ScribeRefusal::Scheduling remains an internal thiserror variant; existing public Wyrd errors remain the HTTP/MCP/SDK boundary. No parallel public code/status/problem-json mapping added |
| No gate suppression or testing-runtime boundary bypass (AGENTS §11) | PASS | No new allow/ignore attribute or boundary-glob change. Renamed ignored journey functions preserve their existing gated tier; Postgres tests remain integration/pg surfaces. Changed Rust tests do not emulate Python or Node lifetimes |
| Journey-first proof on shipped surfaces (AGENTS §11) | PASS (source presence; execution reported) | Real Rust/Python/TS support/refund/coding/media/coexistence/pressure journeys and MCP discover/act/observe remain; SQL and server tests supplement them. Test-support scheduled stream stall remains cfg-gated |
| Python authored declaration parity (AGENTS §8) | PASS | `verify/__init__.pyi:340–362` exactly removes through/members/exceeded_ceiling, renames GroupSnapshot, and changes ExecutionState.group. This is a nested TypedDict return shape, not a newly runtime-exported native class; native wrapper/module/__all__ need no new registration. Public Python journey reads the projected shape |
| TS generated declaration parity / source-driven outputs | PASS (source comparison; generation reported) | generated `card-types.ts` TriggerSpecGroupEnd, GroupSnapshot, GroupSourceSnapshot, VerificationExecutionState match Rust declarations; no N-API declaration changed. Task reports codegen:regen then codegen:check. Generated errors.md change matches existing source metadata; generator ownership in mise.toml remains unchanged |
| Mandatory rustdoc on every new/materially changed Rust item (AGENTS §16; agent-rules documentation rule) | **FAIL** | Firsts alias has no rustdoc; new private selection methods have placeholder summaries; new test activity helper omits its panic contract. See STD-001 |
| Documentation and fixtures follow shipped shape | PASS, with non-blocking prose note | All changed Trigger YAMLs use after; runtime-limit page removes source ceiling; guide states debounce/visibility/post-ACK loss. Guide's unchanged sentence at line 166 calling schedule the only automatic Trigger contradicts its new group_end paragraph; this is prose cleanup, not an independently blocking standards finding |
| Canonical focused command and static verification evidence | PASS as reported, not independently rerun | Task C1–C13 specify mise + managed Postgres + exact selectors; appendix reports successful counts, format/lint/codegen/dependency/tenant/stub/type/docs checks. It expressly reports no separately executed RED for new C8/C9/C13, so RED execution is not independently established by that appendix |

## Material finding

### STD-001 — VIOLATION: mandatory private/test rustdoc is incomplete

**Rule:** AGENTS §16 requires substantive rustdoc for every new or materially
modified Rust item, explicitly including aliases and test helpers; every
possible panic needs `# Panics`. Agent rules and rust-core Documentation make
missing or placeholder rustdoc a hard blocker even when tests pass.

**Exact changed locations:**

- `crates/wyrd/wyrd-server/src/verification/groups.rs:407`: new local alias
  `Firsts` is preceded by an ordinary `//` comment, not rustdoc. The alias
  participates in the input-time/result-time ordering fallback at lines
  408–427, but has no item documentation.
- `crates/wyrd/wyrd-server/src/verification/groups.rs:385–391`: new
  `Visible::input` and `Visible::result` say only “Record one input row” /
  “Record one result row.” They omit the important distinction: deduplicate
  by execution versus result identity and retain earliest-ingest payload
  through Earliest::keep. These are signature-restating placeholders under
  the explicit documentation rule.
- `crates/wyrd/wyrd-server/tests/integration/pg_verification_groups.rs:252`:
  new `Support::activity(&str)` calls `GroupId::new(group).expect(...)` at
  line 257 without documenting its invalid-group panic. Its callers are the
  new concurrency/32-item group test at lines 631 and 670. The failure
  condition is caller-supplied test data, not an unrepresentable invariant.

**Consequence:** the changed selection and test seams do not meet the explicit
merge-blocking documentation contract. In particular a maintainer cannot infer
the two distinct logical-row keys or the fixture's accepted-input assumption
from these method docs. This is a documentation gate, not a claimed runtime bug.

**Smallest correction:** document Firsts with substantive `///` text explaining
its two optional timing anchors; document the two insertion methods' respective
identity key and earliest-ingest retention invariant; add `# Panics` to activity
for a group outside the GroupId grammar. Keep the implementation and test
behavior unchanged. Batch these corrections with any behavioral remediation;
do not create another abstraction or a new test solely for the documentation.

**Closure proof:** inspect those changed rustdoc blocks against their complete
function bodies and existing `Earliest::keep` behavior; run `mise run fmt` for
the touched Rust documentation. Existing selection/concurrency tests need no new
behavioral assertions for this correction.

## Verification limits

No tests, builds, codegen, or formatters were run by this reviewer, and no
performance claim was assessed. Static source inspection covers the complete
base-to-candidate diff and relevant owning callers. Executable outcomes above
are attributed to the candidate task evidence; raw output was not available in
the immutable subject. The orchestrator subsequently reported a successful C13
rerun using repository setup; that is additional attributed evidence, not a run
performed by this reviewer. REQ-015/AC-009 benchmark work is explicitly pending and
is not a standards defect in this task.

No optional refactor, terminology-only block, accepted ACK/visibility/crash-loss
tradeoff repair, or unrelated baseline defect is proposed. Source stayed
immutable; this report is the only file this reviewer wrote.
