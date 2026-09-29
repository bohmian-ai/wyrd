# Repository Standards Review — TASK-006 R2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `3593bbc31273673f87159315fbf66a73562d3c99`
- Candidate was `HEAD` when inspected and did not change during this review.
- Scope: the complete cumulative base-to-candidate diff, including the original task, revision-36 authority update, prior remediation task, product source, migrations, manifests, and verification evidence. Prior Wave 1 conclusions were not used.

## Authority coverage

| Changed surface | Governing authority inspected | Coverage result |
|---|---|---|
| Active specification, task/remediation evidence, workflow-skill mirrors | `AGENTS.md` §§1, 11–16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `.agents/skills/wyrd-implement/SKILL.md`; `.agents/skills/wyrd-task-review/SKILL.md` | Covered |
| System-principal Eval read authority and denial audit | `architecture/wyrd-security-posture.md` (principal lifecycle, authorization, tenant isolation, audit); `architecture/wyrd-design.md` (Verifier/Eval/Bifrost); `AGENTS.md` §§2, 9; `architecture/agent-rules.md` audit and tenancy rules; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md` | Covered |
| Continuous Eval engine, sampling, gate/capture, judge/media path | `changes/active/verified-change-contract/architecture/verifier/eval.md`; `architecture/references/domain/evaluation.md`; `architecture/wyrd-design.md` §Verifier/Eval; `AGENTS.md` §§3–6, 9–10, 16; Rust and architecture-pattern references | Covered |
| Eval observation and result table projection | `changes/active/verified-change-contract/architecture/logic/table_schema.md`; `architecture/references/domain/telemetry-observations.md`; `architecture/references/domain/olap-serving.md`; `architecture/bifrost-design.md` table/ingest/query sections | Covered |
| Gate/Scribe post-ACK disposition and asynchronous enqueue | `architecture/bifrost-design.md` ingest/durability sections; `architecture/references/domain/olap-serving.md`; `architecture/references/domain/analytical-operations-reliability.md`; `AGENTS.md` §§6, 9–11 | Covered |
| Verifier-run migration, queue serialization, trace wait, and tenant SQL | `architecture/agent-rules.md` SQL/TenantConn rules; `AGENTS.md` §§4–6, 9, 16; Rust core Postgres rules; approved Eval authority | Covered |
| Storage-bounded media reads | `AGENTS.md` §§3–6, 10, 16; architecture patterns storage/provider boundaries; Rust core async/error rules; approved Eval authority | Covered |
| Audit publisher concurrency remediation | `AGENTS.md` audit decisions; `architecture/agent-rules.md` single audit path; `architecture/wyrd-security-posture.md` audit integrity; analytical reliability bounded-concurrency rules; Rust core async/documentation rules | Covered |
| Hydrated YAML serialization | `AGENTS.md` client ownership and contract rules; architecture patterns client boundary; Rust core rules | Covered |
| Production-shaped Rust journeys, SQL/storage tests, fixture APIs, and manifests | `AGENTS.md` §11 and §16; `architecture/agent-rules.md` test placement, SQL handle, and gate rules; `architecture/references/languages/testing-workflows.md`; Rust core testing/Postgres rules | Covered |

## Rule results

| Applicable rule | Evidence | Result |
|---|---|---|
| Durable Eval behavior remains in Vala/server owners; no client-side durable implementation | `vala-eval` owns execution/scoring; `wyrd-server::verification::eval` owns server orchestration; the client change is limited to YAML serialization | PASS |
| Stateful workflows use cohesive owners; pure helpers remain free functions | `EvalEngine`, `EvalReadAuthority`, `BifrostReader`, `ObservationEnqueue`, `VerifierRunQueue`, `StorageHandle`, `AuditPublisher`, and `TenantCycles` own their dependencies/state | PASS |
| Async is restricted to IO or orchestration that awaits IO | New async methods perform database, Bifrost, storage, provider, task, or stream work; sampling, capture, projection, decoding, and mapping helpers remain synchronous | PASS |
| Tenant SQL uses `TenantConn`; callees do not commit caller-owned transactions | `VerifierRunQueue` accepts `&mut TenantConn`; `ObservationEnqueue` is the transaction owner and commits after composing queue calls; Eval registry/authority reads acquire tenant handles from the owning Postgres structs | PASS |
| Raw SQL pool handles do not propagate through library APIs | Candidate adds `WyrdTestServer::superuser_pool() -> Result<sqlx::PgPool, _>` at `crates/wyrd/wyrd-testing/src/server.rs:2685`, forwarding the fixture-owned pool through a second library surface | **FAIL — REPO-1** |
| Cross-tier imports use owning re-exports | Vala SQL types enter through `vala_sql`; server-owned Wyrd SQL calls use `wyrd_sql`; no new Vala-to-`wyrd_sql` reach-through was found | PASS |
| System Eval reads use the approved stored tenant System principal, narrow table scopes, Oracle authorization, and canonical denial audit | `EvalReadAuthority::resolve`, `ScheduledQueryCaller::record_object_denial`, the revision-36 security text, and the fail-closed journey align | PASS |
| Sensitive errors and storage locators do not cross the public result boundary | `failed`, `ReadError`, and `TenantMedia` return fixed public messages and keep causes in structured diagnostics; journey evidence covers provider, SQL, and locator sentinels | PASS |
| Bifrost acknowledgement and replay identity remain explicit; post-ACK work cannot roll back ingest | `FrameAdmission.first_commit`, `DurableCompletion`, Gate filtering, and bounded `ObservationEnqueue` preserve the approved best-effort seam | PASS |
| Analytical work is bounded and deterministic | Trace reads use a closed time range, total order, `TRACE_SPAN_LIMIT + 1`, query deadline, and admission deferral; media uses a limit-plus-one bounded read; publisher concurrency is fixed and tenant-keyed | PASS |
| Persistence maps fields by name and keeps approved table shapes | Eval observation decoding and result/trace decoding use named columns; the ordinal migration adds a shaped positive per-binding value and unique index | PASS |
| New/materially changed Rust items carry intent/invariant/error documentation | Inspected changed production items and remediation-targeted fan-out documentation; no material missing rustdoc remained | PASS |
| External tests earn separate binaries and user-facing behavior has a real journey | `wyrd-testing/tests/bifrost/server/eval_verification.rs` drives SDK → real server → Scribe/Oracle/runtime/provider/Postgres/Bifrost and is registered in the existing capability target | PASS |
| No gate was weakened, ignored, allowed away, or replaced by a positional selector | Complete diff contains no new `#[ignore]`/production `#[allow]`; evidence uses exact nextest expressions and repository-managed wrappers | PASS |
| Cargo/dependency changes stay in owning crates and use workspace pins | Server additions support the existing Vala/Skald execution adapter; journey-only Skald dependencies are dev-dependencies; lockfile matches manifests | PASS |
| Shared skill mirror stays synchronized | `.agents` and `.claude` edits match; `mise run check:skills-sync` passed during this review | PASS |
| Diff hygiene | `git diff --check base..candidate` passed during this review | PASS |

## Material finding

### REPO-1 — Raw PostgreSQL pool escapes the fixture owner

- **Violated rule:** `architecture/agent-rules.md` permits only `TenantConn` and `OperatorPool` in library signatures and says the test-fixture exception is for pool construction, not propagation. `architecture/references/languages/rust-core.md` repeats that raw `PgPool` values do not cross library signatures.
- **Location:** `crates/wyrd/wyrd-testing/src/server.rs:2676-2691`, especially line 2685.
- **Evidence:** the new public `WyrdTestServer::superuser_pool` returns `sqlx::PgPool` and merely forwards `PgFixture::superuser_pool`. The same server already exposes the owning fixture through `WyrdTestServer::pg_fixture()` at line 3476, so this adds a second raw-pool propagation surface rather than keeping the exception at its fixture owner. The new journey is the only new consumer, at `tests/bifrost/server/eval_verification.rs:644,1072,2039`.
- **Consequence:** the test-server library now publishes an unsanctioned cross-tenant database capability and widens the raw-pool API that the repository rule is designed to contain. Future tests can bypass the fixture boundary without making that authority explicit.
- **Testable correction:** delete `WyrdTestServer::superuser_pool`; have the three journey call sites obtain the assertion pool from the already exposed `server.pg_fixture().superuser_pool()` (or move each required DDL/assertion behind an existing narrow fixture helper if one already owns it). Re-run the three affected Eval journeys, `mise run check:from-pools-allowlist`, `mise run lints`, and `git diff --check`.

## Verification limits

- Review was static apart from `git diff --check`, `mise run check:skills-sync`, and `mise run check:from-pools-allowlist`; all completed. The allowlist check emitted its existing `rg: python/: No such file or directory` warning but exited successfully and does not inspect raw-pool return signatures.
- Long Cargo, Postgres, storage-emulator, and journey lanes were not rerun within the review time budget. Their command/result evidence was inspected in the task and remediation packet, including focused Eval, SQL, principal, storage, Bifrost journey, format, lint, and boundary runs.
- No Python or TypeScript source changed in this candidate; no language-runtime lane independently applies to the changed behavior.

## Overall result

**FAIL** — repository-rule finding `REPO-1` must be closed before merge. No other material repository-standard violation was found.
