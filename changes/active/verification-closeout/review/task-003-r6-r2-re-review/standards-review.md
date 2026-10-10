# TASK-003 R2 repository standards review

**Result: FAIL**

## Immutable subject and authority

- Base `7f79fb3417db651adedac194ada8908f0a0372d7`; candidate `e2d324a916cfff25e2d362c7d7d2ab1c6aec74d9`. I inspected the cumulative changed surfaces and used `9a8f9f7ee..e2d324a91` to locate R2 edits. No `.codegraph/` directory exists. Source HEAD stayed at the candidate and `git diff --check` passed.
- Read `AGENTS.md`, `architecture/agent-rules.md`, the reference router, spec-driven development, implementation execution, maintainer style, Rust core, testing workflows, the applicable Bifrost/OLAP/DataFusion/reliability authorities, the approved spec and original task, and the prior verdict and R2 packet. The reported focused tests, `fmt`, `lints`, and tenant-isolation check passed; this read-only review did not rerun them.

## Authority coverage

| Changed surface | Applicable authority | Rule result |
|---|---|---|
| Oracle memory attribution, shutdown, resource snapshot, and unit tests | `AGENTS.md` §§4–6, 11, 16; `architecture/bifrost-design.md` admission and memory; `architecture/references/domain/{olap-serving,datafusion,analytical-operations-reliability}.md`; Rust and testing references | PASS for ownership, item docs, source-local unit proof, and wakeup ordering. |
| Forge quiescence, startup recovery, Prepared SQL claim and new integration test | `AGENTS.md` §§4–6, 11, 15–16; `architecture/agent-rules.md` SQL capability, import, test and rustdoc rules; Bifrost design lease/recovery; Rust, implementation-execution and testing references | **FAIL** for sleep-based negative proof (RS-R2-1); SQL capability and rustdoc pass. |
| Cumulative Gateway, Card, verification, outbox, Rust/Python/TypeScript SDKs, examples, schema/stubs, docs, roles and test infrastructure | `AGENTS.md` §§2–4, 7–12, 16; agent rules; Wyrd design/doctrine/security; relevant language, error, telemetry and evaluation references | PASS on the previously closed standards items and no R2 diff to those source surfaces. Original gate/codegen and R1 focused proof remain recorded, not rerun. |

## Applicable rule checks

| Rule | Source evidence | Result |
|---|---|---|
| New or materially changed Rust items, fields and fallible functions need substantive rustdoc, including `# Errors` | `resources.rs:733,834`, `forge/worker.rs:716–741`, `forge_tasks.rs:413–443`, and both new test functions have item docs; changed fallible operations retain `# Errors` and relevant cancellation sections. Prior missing Forge tuple field is now documented. | PASS; FIND-8 closed. |
| SQL work uses the right capability and keeps transaction ownership | `ForgeTasks` retains `OperatorPool` in its field; `claim_prepared_for_reconciliation` uses that pool and commits its own operator transaction. The R2 signature adds an `Option<Uuid>`, not a raw pool or caller-owned `TenantConn`; tests use fixture/admin pools at their test boundary. | PASS. |
| Tests needing Postgres live in the gated integration tier and exact commands select named tests | New `production_routes.rs::interrupted_startup_waits_for_its_prepared_lease` is an ignored Postgres/Iceberg/object-store integration test following the existing lane convention; R2 evidence records exact selectors and repository-managed Postgres wrapper. | PASS for tier/command placement; proof design fails below. |
| Do not add sleeps instead of deterministic synchronization to pass a check | New `production_routes.rs:1161–1176` sleeps two seconds and infers eight startup recovery passes without observing any pass. A restart delayed before or during its first recovery query can leave the lease unchanged and readiness false, satisfying both negative assertions without exercising the guard. | **FAIL, RS-R2-1**. |
| Import declarations belong at module top | New `oracle/admission.rs:2606` repeats a function-local `MemoryConsumer` import rather than placing it in the test module's import block; an earlier test in that module did the same. | FAIL, non-blocking placement note; no behavioral or public-contract effect. |
| Generated contracts, tenant boundaries and gate integrity remain intact | R2 modifies no generated contract, public SDK or auth path. The cumulative contract/codegen gate and user journeys were recorded green. No `#[allow]`, deleted test, or changed gate was introduced in R2. | PASS on available source and recorded evidence. |

## Material finding

**RS-R2-1 — The new Forge restart test does not prove its negative branch ran.** `architecture/references/languages/implementation-execution.md:238–242` forbids adding sleeps instead of deterministic synchronization. `crates/vala/vala-bifrost-redux/tests/integration/forge/production_routes.rs:1158–1176` starts a restarted worker, waits two seconds, then checks that a Prepared lease did not change and readiness is false. Those values are also true if the restarted task was not scheduled or its startup stalled before the claim check; elapsed time is not evidence of a recovery pass. The test can therefore pass without exercising the R2 `previous_owner = None` path it is meant to establish. Have the test observe at least one actual restart recovery pass through the existing worker/test observer boundary, then assert the lease remains unchanged and readiness stays down before expiring the claim. Keep a bounded timeout for test failure, not as proof that a pass occurred. This preserves the Postgres integration tier and needs no new runtime API.

## Non-blocking note

The function-local `MemoryConsumer` import in the new Oracle test violates `architecture/agent-rules.md` import placement, as the earlier neighboring test does. Move both to the test module's import block when making a relevant edit. It has no observable behavior or contract consequence and does not independently warrant remediation.

No other material repository-rule violation was established. The runtime correctness of Oracle accounting and Forge cancellation belongs to the implementation, system and data reviews.
