# TASK-006 Repository Standards Review

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `55c5bff84fbfcc8f43967a8ad3aeac6057f10c3f`
- Reviewed range: the complete `base..candidate` diff (42 files; 3,779 additions and 358 deletions).
- Result: **FAIL**

The checkout advanced after the immutable candidate only for workflow-skill and review-artifact work. `git diff --name-status 55c5bff..HEAD` showed no reviewed production, test, manifest, or task source change; all source judgments below remain against the named candidate range.

## Authority Coverage

| Changed surface | Applicable authority read | Coverage result |
|---|---|---|
| `crates/vala/vala-eval/**`: execution error semantics, judge/media binding, result aggregation, sampling | `AGENTS.md` §§3-6, 10-12, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` §Verifier/Eval; `architecture/wyrd-doctrine.mdx`; `references/architecture/patterns.md`; `references/languages/rust-core.md`; `references/languages/errors.md`; `references/domain/evaluation.md` | Complete; **FAIL** on deterministic sampling and mandatory rustdoc. |
| `crates/wyrd/wyrd-server/src/verification/**`: continuous Eval orchestration, registry/media/input reads, settlement | `AGENTS.md` §§3-6, 9-12, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` observation identity and Verifier/Eval; `architecture/wyrd-security-posture.md`; `references/doctrine/architecture-constraints.md`; `references/architecture/patterns.md`; `references/languages/rust-core.md`; `references/languages/errors.md`; `references/domain/{vala-architecture,telemetry-observations,evaluation,olap-serving,analytical-operations-reliability}.md` | Complete; **FAIL** on query authority/audit identity, unbounded query collection, media bounds, and public error leakage. |
| `crates/vala/vala-bifrost-redux/**`: post-ack hook, receipt time, Eval observation table and Scribe lane visibility | `architecture/bifrost-design.md` system boundary, table/row identity, ingest, resource/failure invariants, public surface; `AGENTS.md` §§3, 6, 9-12, 16; `architecture/agent-rules.md`; `references/domain/{vala-architecture,telemetry-observations,olap-serving,analytical-operations-reliability}.md` | Complete; **PASS** for server ownership, post-durable-ack ordering, tenant-qualified table handling, bounded tracked callback admission, and typed Scribe errors. |
| `crates/wyrd/wyrd-sql/**`: observation enqueue, trace wait, ordinal selection, tenant transactions | `AGENTS.md` §§3-6, 9, 11, 15-16; `architecture/agent-rules.md` SQL/RLS/clock rules; `architecture/wyrd-security-posture.md` tenant isolation; `references/languages/rust-core.md`; `references/domain/evaluation.md` | Complete; **PASS** for `TenantConn`, caller-owned commit, RLS, and PostgreSQL coordination time; **FAIL** for mutable ordinal sampling. |
| `crates/shared/wyrd-client/src/cards/hydrate/bundle.rs` | `AGENTS.md` §§2-5, 9, 16; `architecture/wyrd-design.md` client model and spec authoring; `architecture/wyrd-doctrine.mdx`; `references/architecture/patterns.md`; `references/languages/rust-core.md` | Complete; **PASS**. The JSON-model conversion remains client-side serialization ergonomics and has a focused regression test. |
| `crates/wyrd/wyrd-storage/src/handle.rs` and Eval media consumer | `AGENTS.md` §§3-6, 9, 16; `architecture/wyrd-security-posture.md` tenant/data isolation; `references/architecture/patterns.md`; `references/languages/rust-core.md`; `references/domain/analytical-operations-reliability.md` | Complete; **FAIL** because metadata preflight does not bound the subsequent body read. |
| `crates/wyrd/wyrd-server/src/query/scheduled.rs` | `architecture/bifrost-design.md` query/resource/audit contracts; `AGENTS.md` §§6, 9-11, 16; `architecture/wyrd-security-posture.md`; `references/domain/{olap-serving,analytical-operations-reliability}.md` | Complete; **FAIL** because decoded query batches are accumulated without a caller-owned bound and the new Eval span query has no upper time bound. |
| `wyrd-testing`, `pg_verifier_runs`, `orchestrator_judge_skald`, and Bifrost server journey | `AGENTS.md` §11 and §16; `architecture/agent-rules.md` test placement; `references/languages/testing-workflows.md`; `architecture/bifrost-design.md` public verification surface | Complete; **PASS** for placement and real server/provider seams. Coverage does not exercise the concurrency and bound failures in RS-002 through RS-004. |
| `Cargo.toml`, crate manifests, `Cargo.lock` | `AGENTS.md` §§1, 2, 4; `architecture/agent-rules.md` feature-cost rule; crate ownership in `AGENTS.md` §3 | Complete; **PASS**. Added dependencies stay in server/testing owners and no new Cargo feature was introduced. |
| Task evidence Markdown | `AGENTS.md` §§11-12, 14; `references/languages/spec-driven-development.md`; `references/languages/testing-workflows.md` | Complete. Evidence names focused and family lanes, but this review did not treat the self-reported results as substitutes for source conformance. |

## Applicable Rule Results

| Repository rule | Exact evidence | Result |
|---|---|---|
| Durable Eval remains Vala-owned and `wyrd-server` only composes/serves it | `vala-eval::ScenarioScoring` remains the engine; `verification/eval.rs:221-245` invokes it; no Vala listener or SDK durable engine was added. | PASS |
| Stateful workflows use cohesive concrete owners | `EvalEngine` (`verification/eval.rs:78-248`) and `ObservationEnqueue` (`verification/observations.rs:26-119`) own their dependencies and workflows. | PASS |
| Tenant SQL uses `TenantConn`, relies on RLS, and callees do not commit | Queue methods accept `&mut TenantConn`; `ObservationEnqueue` opens and commits the caller transaction at `observations.rs:71-86`; queue methods do not commit. | PASS |
| PostgreSQL owns coordination timestamps | Trace polling/deadline SQL uses `statement_timestamp()` at `verifier_runs.rs:252-266`; retry/claim paths retain database time. | PASS |
| Continuous Eval uses the existing typed engine and distinguishes execution error from subject failure | `executor.rs:385-417` propagates task errors; `verification/eval.rs:237-246` maps only completed reports to verdicts. | PASS |
| Sampling must be repeatable from stable durable inputs | `OBSERVATION_ORDINAL_SQL` recomputes a rank from all currently visible peer rows (`verifier_runs.rs:269-279`), and `EvalEngine::sampled` rereads it on every attempt (`verification/eval.rs:177-198`). | **FAIL (RS-001)** |
| Internal/external requests use authenticated, authorized, attributable identities; audit records stored principals | `BifrostReader::new` fabricates a fresh `PrincipalKind::User` and self-grants `bifrost_query:read` (`verification/eval.rs:281-300`) before Oracle's audited query path. | **FAIL (RS-002)** |
| Every queue/buffer/query is bounded and analytical reads specify a bounded time range | `ScheduledQueryCaller` collects every decoded batch into a `Vec` (`query/scheduled.rs:152-223`); Eval spans use only a lower time predicate with no upper bound or row cap (`verification/eval.rs:389-423`). | **FAIL (RS-003)** |
| Size validation must govern the effective bytes acted upon | `TenantMedia::resolve` stats the object, then independently reads the complete object (`verification/eval.rs:732-746`); `StorageHandle::get_object` materializes the whole body (`wyrd-storage/src/handle.rs:272-287`). | **FAIL (RS-004)** |
| Public error/status surfaces do not expose raw database, provider, storage, or transport diagnostics | Eval converts raw query, SQL, registry, storage, and provider displays into `VerificationError.message` (`verification/eval.rs:122-156, 177-198, 633-664, 732-745`); that field is returned by public HTTP/MCP run status (`wyrd-spec/src/verification.rs:341-362`). | **FAIL (RS-005)** |
| Every new or materially modified Rust item, including private helpers/tests, has intent rustdoc and fallible sections | The materially changed `fan_out_bucket` has no rustdoc or `# Errors`/cancellation contract (`vala-eval/src/executor.rs:385-418`). Repository authority classifies missing rustdoc as `BLOCK_BEFORE_MERGE`. | **FAIL (RS-006)** |
| Post-ACK work cannot delay or roll back Scribe acknowledgement | Gate calls the synchronous hook only after `ingest_frame` returns durable admission (`gate/mod.rs:979-993`); the hook uses bounded `try_acquire_owned` and a tracked task (`verification/observations.rs:97-118`). | PASS |
| Tenant/card correlation is derived from authenticated scope, not client UID | `ObservationsTable::acknowledged` reuses `resolve_card_uids` with the authenticated `Principal` (`tables/eval/observations.rs:89-124`). | PASS |
| Public errors introduced across boundaries use typed/stable codes | No new public `WyrdError` variant was added. Engine-local codes are stable strings, but their raw messages violate the separate sanitization rule in RS-005. | PASS with RS-005 caveat |
| Test placement follows repository tiers | SQL tests use the Postgres integration target; provider integration remains external; the Bifrost journey uses `WyrdTestServer` and repository-managed Postgres. | PASS |
| New user-facing behavior has a real client-server-client journey | `wyrd-testing/tests/bifrost/server/eval_verification.rs` drives the real Rust client, server, Scribe/Oracle, Postgres, storage, and local provider. No new Python/TypeScript contract was changed in this candidate. | PASS |

## Material Findings

### RS-001 — `every_nth` sampling is not stable across retries

- Violated rule: `architecture/references/domain/evaluation.md` requires deterministic ordering for repeatable evidence; `sampling.rs:3-5` itself promises that retry/reclaim/restart reaches the same decision.
- Location: `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:269-279`; consumed at `crates/wyrd/wyrd-server/src/verification/eval.rs:177-198`.
- Evidence: the ordinal is a fresh `count(*)` of currently visible observation runs ordered by `(created_at, run_id)`. With two concurrent enqueue transactions, an earlier-stamped row can commit after a later row has already been sampled. The later run first observes ordinal 1; after the earlier row commits it observes ordinal 2. A retry can therefore switch between selected and sampled-out.
- Observable consequence: one immutable observation can execute tasks on one attempt and complete inconclusive as sampled-out on another, making results depend on transaction visibility rather than the frozen run input.
- Testable correction: freeze the sampling position or selection as immutable durable run input at enqueue (or derive it from an already immutable observation identity), and add a Postgres regression that delays an earlier enqueue commit until after a later run's first sampling read, then proves the later run's decision is unchanged on retry.

### RS-002 — Eval input queries fabricate a self-authorized, non-durable principal

- Violated rule: `architecture/wyrd-security-posture.md` requires every internal request to be authenticated and authorized at its receiving boundary and authorization audit to identify stored principals; `architecture/bifrost-design.md` makes Oracle reads auditable authorization decisions.
- Location: `crates/wyrd/wyrd-server/src/verification/eval.rs:277-301`.
- Evidence: each `BifrostReader::new` creates `PrincipalId::new(UUIDv7)`, labels it `PrincipalKind::User`, injects `bifrost_query:read` into its permission set, and constructs `AuthorizedQueryContext` without loading or verifying any persisted tenant identity. Oracle then records this invented principal in read audit.
- Observable consequence: internal Eval reads are authorized by authority the caller granted itself, and retained audit names a different nonexistent user on each run. The decision cannot be attributed to a stored principal or revoked/inspected through the principal lifecycle.
- Testable correction: route Eval reads through a repository-authorized, tenant-scoped server capability whose principal identity and permissions come from the canonical persisted authority; add an integration assertion that the Oracle read-audit principal resolves to that stored identity and that an unavailable/invalid authority fails closed. If no existing principal class may hold this read authority, the governing security architecture must decide that identity before implementation.

### RS-003 — The Eval trace read is open-ended and fully collected in memory

- Violated rule: `AGENTS.md` §10 requires explicit tenant, time range, projection, and pruning; `architecture/bifrost-design.md` and `references/domain/olap-serving.md` reject unbounded `collect()`; analytical reliability requires every buffer and query result to be bounded.
- Location: `crates/wyrd/wyrd-server/src/verification/eval.rs:389-423`; `crates/wyrd/wyrd-server/src/query/scheduled.rs:152-223`.
- Evidence: the span SQL has a lower event-time bound but no upper bound or row/byte cap, and `ScheduledQueryCaller` appends every decoded batch to a `Vec<RecordBatch>`. A caller can publish arbitrarily many rows under the same trace ID, and the query scans every partition after the lower day.
- Observable consequence: one Eval run can consume a growing amount of Oracle work and server heap, repeatedly hit admission/result ceilings, or starve useful work; future partitions remain in the scan scope.
- Testable correction: give the trace lookup a closed, architecture-valid time window and a hard result row/byte bound enforced before materialization (or consume a bounded stream); add a journey with more matching spans than the ceiling and assert typed refusal/retry without unbounded accumulation, plus plan evidence that both time bounds prune partitions.

### RS-004 — Media size enforcement has a metadata/body check-to-use race

- Violated rule: `AGENTS.md` §§6 and 15 and `references/domain/analytical-operations-reliability.md` require bounded external/storage work; security validation must govern the effective value acted upon.
- Location: `crates/wyrd/wyrd-server/src/verification/eval.rs:732-746`; supporting full-body read at `crates/wyrd/wyrd-storage/src/handle.rs:272-287`.
- Evidence: `object_len` validates one metadata snapshot, then `get_object` performs an independent whole-object read and allocates its full body. There is no version/ETag binding, ranged limit, streaming ceiling, or post-read length check. A replacement between calls (or inconsistent backend metadata) bypasses `MEDIA_LIMIT_BYTES`.
- Observable consequence: oversized content can reach base64 expansion/provider construction and consume substantially more memory than the declared 20 MiB ceiling.
- Testable correction: make the effective body read itself bounded (for example, read at most limit + 1 through the existing storage backend and reject overflow, optionally version-bound), and add a backend double whose metadata reports a small object while the body exceeds the limit; prove the resolver refuses without materializing/encoding the oversized body.

### RS-005 — Raw infrastructure/provider errors are persisted and returned publicly

- Violated rule: `architecture/references/languages/errors.md` says boundary conversion must not leak raw database, storage, provider, filesystem, or transport strings; `AGENTS.md` §4 requires stable public error contracts.
- Location: `crates/wyrd/wyrd-server/src/verification/eval.rs:122-156, 177-198, 633-664, 732-745`; public projection at `crates/wyrd-spec/src/verification.rs:341-362`.
- Evidence: `.to_string()` values from Bifrost query failures, `TenantConn`/registry SQL, storage operations, and judge/provider failures become `VerificationError.message`; `GET /v1/verification/runs/{run_id}` and the MCP run-status tool return that message.
- Observable consequence: callers with run-read access can receive database/storage/provider diagnostics and tenant object paths, while behavior and message text drift with dependencies rather than the stable Wyrd catalog.
- Testable correction: log raw causes in structured server diagnostics, persist only stable code plus sanitized operation/resource-safe text, and add HTTP/MCP status tests that inject representative SQL, storage, and provider errors and prove their raw marker strings are absent while stable codes remain.

### RS-006 — A materially changed fallible async helper lacks mandatory rustdoc

- Violated rule: `AGENTS.md` §16 and `architecture/agent-rules.md` require rustdoc for every new or materially modified Rust item, including private helpers, with `# Errors` and cancellation/partial-progress behavior where relevant; missing documentation is `BLOCK_BEFORE_MERGE`.
- Location: `crates/vala/vala-eval/src/executor.rs:385-418` (`fan_out_bucket`).
- Evidence: the candidate materially changes the helper from folding executor errors into failed assertions to aborting on the first error and dropping the `JoinSet`, but the function has no rustdoc, no `# Errors`, and no explanation of sibling-task cancellation on early return.
- Observable consequence: the new error/cancellation contract is undocumented at the exact concurrency boundary maintainers must preserve, and the candidate violates an explicit hard merge gate even if compilation/tests pass.
- Testable correction: document the helper's stage fan-out role, propagated error cases, and early-return cancellation semantics; run the repository documentation/lint gate that enforces touched-item rustdoc coverage.

## Verification Notes

- Inspected the complete named diff, surrounding SQL/schema/query/security consumers, crate manifests, and the new unit/integration/journey tests.
- Read all routed authority listed above, including `AGENTS.md`, `architecture/agent-rules.md`, the reference router, Wyrd design/doctrine, Bifrost design, security posture, Rust/errors/testing, Vala/telemetry/evaluation/OLAP/reliability references, and spec-driven workflow rules.
- CodeGraph was attempted first as required; the repository has no `.codegraph/` index, so ordinary source inspection was used.
- Candidate task evidence reports successful focused `vala-eval` tests, `test:vala`, `test:sql`, `test:wyrd`, `test:bifrost`, `test:wyrdstate:journey`, format, lints, and `git diff --check`. This reviewer did not rerun Cargo/mise lanes because the review was time-bounded and the shared checkout was active; those claims therefore remain supplied evidence rather than independently reproduced results.
- Existing tests cover the happy/terminal matrix, post-ACK loss behavior, simple sequential ordinal values, media type/scheme/tenant/size metadata refusal, and real provider delivery. They do not cover the concurrent ordinal reorder in RS-001, persisted query authority/audit identity in RS-002, bounded span overflow in RS-003, metadata/body mutation in RS-004, or public error redaction in RS-005.

## Overall Result

**FAIL.** Six material repository-rule violations remain. RS-006 is independently a hard `BLOCK_BEFORE_MERGE`; RS-001 through RS-005 affect reproducibility, authorization/audit attribution, resource bounds, and public information exposure.
