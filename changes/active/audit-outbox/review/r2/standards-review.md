# Repository Standards Review

## Immutable Subject

- Repository root: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `6714ae35d732814240fdcc42fc226c079b14d3f0`
- Candidate: `5a5542cbb965af99e92a3983a2ddf586412cea73`
- Reviewed range: complete `base..candidate` range (67 changed files)
- Candidate check: `HEAD` was the candidate before this report was written. Existing untracked `review/r2` artifacts belong to the parallel review and are not candidate source.
- Scope: repository standards and architecture conformance only. Task-acceptance and Ponytail judgments are left to their assigned reviewers.

The repository has no `.codegraph/` index, so ordinary repository navigation was used as directed.

## Authority Coverage

| Changed surface | Applicable authority | Coverage result |
|---|---|---|
| Repository-wide Rust, SQL, tooling, tests, documentation, and verification | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/references/README.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/maintainer-style.md`; `architecture/references/languages/testing-workflows.md` | Complete |
| Generic Rust outbox, async lifecycle, dependency ownership, metrics, and client dependency cones | `AGENTS.md` §2-§6, §11-§12, §15-§16; `architecture/agent-rules.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `scripts/checks/client-tier.sh`; owning and consuming manifests plus `Cargo.lock` | Complete; failures `STD-R2-002`, `STD-R2-003`, and `STD-R2-005` |
| Audit retry, event identity, canonical append, staging retirement, and retained history | `architecture/wyrd-design.md`; `architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`; `architecture/operations/README.md`; `architecture/operations/reliability-and-recovery.md`; `architecture/operations/runbooks.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/domain/vala-architecture.md`; approved `changes/active/audit-outbox/spec.md` revision 2 | Complete; failures `STD-R2-001`, `STD-R2-002`, and `STD-R2-006` |
| Audit contract/schema and Rust/HTTP/client documentation | `architecture/wyrd-doctrine.mdx`; `AGENTS.md` §9 and §16; generated-artifact and public-surface rules above | Complete; generated schema source is aligned, but the modified security authority retains one obsolete surface-specific failure attribution (`STD-R2-006`) |
| SQL tenancy, event-ID migration, and Postgres tests | `architecture/agent-rules.md`; `AGENTS.md` §3, ©, ¡5-¡6; `architecture/references/languages/rust-core.md`; `architecture/references/architecture/patterns.md` | Complete; RLS correction passes, but mandatory bare-name style still fails (`STD-R2-005`) |
| Unwrap/expect audit checker and its exemptions | `AGENTS.md` §4, §12; `architecture/agent-rules.md` gate-circumvention rule; `architecture/references/languages/testing-workflows.md` | Complete; failure `STD-R2-004` |
| Gate, Oracle, auth, server, client, CLI, fixtures, journeys, and documentation adjustments | The repository, security, audit, server, Rust, and testing authorities above; surrounding callers and changed tests | Complete; no additional material standards finding beyond the shared-owner and authority findings below |

## Rule-by-Rule Results

| Applicable repository rule | Evidence | Result |
|---|---|---|
| A retried audit decision is never staged twice; loss is limited to abrupt process loss or an expired graceful-shutdown deadline | `append_audit_events` deduplicates only against rows currently in `vala.audit_staging` (`audit_staging.rs:111-130`), while `settle_publication` deletes those rows (`:470-520`). The outbox retries an unknown commit outcome after backoff (`outbox.rs:321-374`). | **FAIL — `STD-R2-001`** |
| A failed write is retained at the front of the tenant queue and is not dropped while the process remains alive | `OutboxWriter::finish` explicitly counts a panicked write task as lost, releases it from pending, and returns without retry (`outbox.rs:321-340`). | **FAIL — `STD-R2-002`** |
| Dependency cost stays with the narrowest behavior owner; client/foundation cones do not absorb server-only machinery merely through a broad shared shell | `wyrd-runtime` now unconditionally exports the outbox and depends on `metrics`, `tokio-util`, and `tracing`. Reverse dependency inspection shows the new `metrics` edge entering `wyrd-client`, `wyrd-auth-verify`, and all-feature Skald consumers solely through `wyrd-runtime`; none consumes the outbox. | **FAIL — `STD-R2-003`** |
| A gate must not be weakened or receive a broadened boundary glob to hide a violation | `is_ignored_path` now ignores every Rust file whose basename is `tests.rs` without proving it is included by `#[cfg(test)] mod tests;` (`check_unwrap_audit.py:31-38`). The current four files happen to be test-only, but the permanent check now silently exempts any future production module with that legal basename. | **FAIL — `STD-R2-004`** |
| Rust signatures and fields use top-level imports and bare type names | New code uses `std::sync::MutexGuard` in a signature (`outbox.rs:467-473`) and `uuid::Uuid` in local type declarations and `AuditRows` fields (`audit_staging.rs:111-124, 216-240`) despite an available imported `Uuid` type in the owning audit module. | **FAIL — `STD-R2-005`** |
| Governing security/operations prose must describe the actual one-outbox owner and failure attribution | The modified audit-integrity section correctly removes Oracle's special writer, but the same modified authority still names an `Oracle audit commit failure` as the security event (`wyrd-security-posture.md:418-427`). Oracle now only stages; the generic outbox performs commits. | **FAIL — `STD-R2-006`** |
| Tenant-scoped SQL relies on `TenantConn`/RLS and does not repeat manual tenant filters | The changed chain-head lock, already-staged lookup, and head update no longer add tenant predicates; insertion binds the row's tenant value as required (`audit_staging.rs:71-75, 83-207`). | PASS |
| One production audit append and one publisher | The canonical append remains crate-private and its production caller is `AuditSink`; direct append helpers are `test-support` only. Publication still flows through `AuditPublisher`. | PASS |
| Permission checks remain blocking while audit commits never fail or delay the request | Changed server/auth/Gate/Oracle callers stage synchronously on the shared handle; `Outbox::stage` performs only an unbounded channel send and returns `()` (`outbox.rs:136-148`). Failure-path tests and journeys were updated to recovery semantics. | PASS, subject to the loss/duplication paths above |
| Generic outbox has one cohesive owner and async is used for IO/timers/task coordination | `Outbox` owns staging and lifecycle; `OutboxWriter` owns queue state, retry scheduling, concurrency, and write tasks; `AuditSink` owns SQL IO. | PASS |
| Public audit contracts and generated schema describe non-blocking staging | `AuditEvent` source rustdoc now describes process-outbox staging outside the operation transaction (`wyrd-spec/src/vala/api.rs:2450-2458`), and both checked-in schemas project it. Historical `AUDIT_UNAVAILABLE` remains decode-only. | PASS |
| Prior r1 admin ordering, Oracle proof, stage naming, no-manual-predicate, and TypeScript obligations are closed | Candidate source and the remediation evidence show the six admin allowances staged before fallible work, Oracle failure/recovery journey coverage, `stage_*` collaborator names, corrected RLS usage, and the recorded TypeScript lanes. | PASS |
| Required verification for touched surfaces is recorded | The remediation record includes focused generic-outbox and SQL tests, server and Bifrost journeys, TypeScript build/declaration/type/integration lanes, codegen/docs/OpenAPI, format/lints, and boundary checks. `gate` and the capacity lane remain explicitly integration-owned. | PASS for this bounded remediation review; green checks cannot waive the checker or dependency-rule findings |

## Material Findings

### STD-R2-001 — The event-ID dedup fence disappears before an unknown commit can be retried

- **Violated rule:** `AGENTS.md` §2 and `architecture/agent-rules.md` require retries to preserve the stage-time event identity without duplicating audit; `architecture/wyrd-security-posture.md:358-370`, `architecture/references/architecture/patterns.md:260-265`, and approved REQ-009 state that an unknown commit outcome never stages a decision twice.
- **Locations:** `crates/shared/wyrd-runtime/src/outbox.rs:321-374`; `crates/vala/vala-sql/src/queries/audit_staging.rs:111-130,470-520`; `crates/vala/vala-sql/tests/pg_audit_outbox.rs:200-215`.
- **Evidence:** A commit can succeed server-side and return an error to `AuditSink`. The generic outbox then retains the same event IDs for retry. The append checks those IDs only in transient staging. A concurrent publisher can publish and delete the committed rows before retry. The retry then sees no matching staging IDs, allocates new chain sequence numbers, and stages the same decisions again. The existing retry test repeats while the first rows remain staged, so it cannot detect this path. The retained audit schema has no event ID that supplies a second fence.
- **Consequence:** One authorization decision can become two distinct retained audit records with different chain sequence numbers. This violates audit cardinality and makes an unknown commit outcome observable as duplicated accountability evidence.
- **Testable correction:** Establish one durable idempotency fence that survives staging retirement for every retryable event identity, while preserving the single append and publisher. Add a Postgres/publisher integration test that forces commit-success/error, publishes and retires the first row, then retries the original staged event and proves exactly one retained decision and no new chain sequence. If satisfying this requires another persistent authority or a retained-schema change, route that material choice through spec revision rather than weakening the no-second-table rule.

### STD-R2-002 — A sink-task panic drops accepted audit while the process remains alive

- **Violated rule:** Approved REQ-003/REQ-003a and the repository audit authorities permit loss only on abrupt process loss or an expired shutdown deadline; a failed write stays queued at the front and is retried. `architecture/references/domain/vala-architecture.md:74-79` also forbids discarding accepted authority during recoverable runtime failure.
- **Location:** `crates/shared/wyrd-runtime/src/outbox.rs:321-340`.
- **Evidence:** The write task owns the only `Vec<S::Item>`. If `OutboxSink::write` panics, `JoinSet` returns `JoinError`; the parent retained only tenant and count. `finish` calls `count_lost`, decrements pending, and returns. The process and writer continue, but the items cannot be requeued. The module-level promise that failed writes are never dropped is therefore false for an explicitly handled task outcome.
- **Consequence:** A sink bug or dependency panic creates an unapproved, counted audit gap without process loss or shutdown deadline, and later events for the tenant can proceed after the missing decision.
- **Testable correction:** Preserve recoverable ownership of each in-flight batch across task failure (or contain the sink panic while retaining the batch), then route the batch through the same front-of-queue retry/backoff path as an ordinary error. Add a generic sink test that panics once, recovers, and proves the original items commit once ahead of later tenant items with pending returning to zero.

### STD-R2-003 — Server-only outbox machinery was attached to the broad client/runtime foundation

- **Violated rule:** `AGENTS.md` §2 and §15 require dependency cost to remain in the narrowest crate that owns the behavior and forbid moving specialized behavior into a foundational or broadly consumed crate merely to centralize it. The client-tier rationale in `scripts/checks/client-tier.sh` requires lightweight client and auth cones.
- **Locations:** `crates/shared/wyrd-runtime/Cargo.toml:9-18`; `crates/shared/wyrd-runtime/src/lib.rs:13`; `crates/shared/wyrd-runtime/src/outbox.rs`; corresponding `Cargo.lock` entry.
- **Evidence:** The outbox is a server-side derived-work queue used by `vala-sql` today and a server Eval sink later. `wyrd-runtime` is also consumed by `wyrd-client`, `wyrd-auth-verify`, `wyrd-queue`, Python/TypeScript runtime bridges, and optional Skald bindings. The candidate adds unconditional `metrics`, `tokio-util`, and `tracing` edges to every such consumer. `cargo tree -p wyrd-client -i metrics`, `-p wyrd-auth-verify -i metrics`, and `-p skald-agent --all-features -i metrics` show `metrics` arriving solely through `wyrd-runtime`; the base lock entry had none of the three new dependencies. The scripted client-tier denylist passes only because it does not measure this dependency-cost boundary.
- **Consequence:** Lightweight clients and authentication/runtime consumers compile and carry server outbox instrumentation and cancellation dependencies despite having no reachable outbox use. Future server-only outbox dependencies now have a misleadingly broad sanctioned home.
- **Testable correction:** Keep the approved generic, SQL-free outbox in a narrow shared owner consumed only by its actual sinks (for example, a dedicated shared outbox crate), or use an equally narrow earned boundary that leaves default `wyrd-runtime` consumers unchanged. Prove with locked `cargo tree` checks that `wyrd-client`, `wyrd-auth-verify`, and Skald do not acquire outbox-only dependencies while `vala-sql` and the future Eval sink share the same implementation.

### STD-R2-004 — The unwrap audit now exempts every file named `tests.rs`

- **Violated rule:** `AGENTS.md` §12 and `architecture/agent-rules.md` prohibit broadening a boundary glob to hide a violation; only a check's sanctioned, legitimately test-only mechanism may exclude code.
- **Location:** `scripts/check_unwrap_audit.py:31-38`.
- **Evidence:** Basename alone does not make a Rust module test-only. Rust permits `mod tests;`, `#[path = "tests.rs"] mod runtime;`, or other production inclusion. The checker does not inspect the parent module declaration before returning `True`. The four current `tests.rs` files are all behind `#[cfg(test)]`, so the immediate false positive is real, but the chosen global exemption weakens the permanent production invariant. There is no checker regression test distinguishing cfg-gated and production `tests.rs` modules.
- **Consequence:** A production unwrap, dynamic/empty `expect`, or any future production module named `tests.rs` passes `check:unwrap-audit`, so a green gate no longer proves the policy it advertises.
- **Testable correction:** Exempt only files proven to be out-of-line bodies of a cfg-test module (or an exact sanctioned allowlist of such files), and add checker fixtures proving a cfg-gated `tests.rs` is ignored while a production-included `tests.rs` is scanned and rejected.

### STD-R2-005 — New Rust declarations still use fully qualified type paths

- **Violated rule:** `architecture/agent-rules.md` requires top-level imports and bare type names in fields, parameters, return types, bounds, and `where` clauses.
- **Locations:** `crates/shared/wyrd-runtime/src/outbox.rs:467-473`; `crates/vala/vala-sql/src/queries/audit_staging.rs:111-124,216-240`.
- **Evidence:** `MemorySink::lock` returns `std::sync::MutexGuard`, while the module imports only `Mutex`. `AuditRows.event_id` and local typed collections use `uuid::Uuid` rather than a top-level `Uuid` import; sibling audit code already uses an imported UUID type.
- **Consequence:** The candidate claims closure of r1's bare-name finding while introducing the same mandatory style violation in the remediation-owned code, hiding module dependencies inside declarations.
- **Testable correction:** Import `MutexGuard` and `Uuid` at the relevant module tops and use their bare names in the changed declarations; keep format and lint lanes green.

### STD-R2-006 — Security authority still attributes generic outbox failure to Oracle

- **Violated rule:** `AGENTS.md` §1-§2 and §16 require active authority and documentation to match the implementation; the one-outbox rule forbids surface-specific writer semantics.
- **Location:** `architecture/wyrd-security-posture.md:418-427`.
- **Evidence:** The candidate rewrites the same document's audit-integrity section to state that Oracle only stages and that the shared outbox commits with no surface-specific exception. The security-operations list still names `Oracle audit commit failure`. Oracle no longer commits audit; `Outbox<AuditSink>` does, and its failure metric is labelled only `outbox="audit"`.
- **Consequence:** Operators are directed to classify a shared persistence failure as an Oracle security event, misattributing the failing owner and contradicting the corrected observability contract.
- **Testable correction:** Replace the surface-specific phrase with the generic audit-outbox write/publication failure vocabulary and metric ownership used by the updated audit-integrity and runbook sections; a focused search must leave no live Oracle-specific audit-commit owner.

## Verification Assessment

Recorded implementation evidence includes the focused generic outbox tests, 119 SQL integration tests, server/OpenAPI integration, Bifrost server journey, Redux Gate/Oracle tests, TypeScript build/declaration/type/unit/integration lanes, codegen, docs, format, lints, client-tier, unwrap audit, Python lint, and diff check. The first broad `test:wyrd` run had two expected stale-semantics tests; both were rewritten and passed focused commands, but a final complete rerun is not recorded.

This review additionally ran read-only `scripts/checks/client-tier.sh`, the checker directly with `python3 scripts/check_unwrap_audit.py`, `cargo fmt --all -- --check`, and the immutable-range `git diff --check`; all completed successfully. Those green results confirm the current checker behavior and denylist, not the correctness of the broadened exemption or the broader dependency-cost rule.

The candidate's own remediation record correctly calls out both the retirement/dedup window and the unconditional `wyrd-runtime` dependency expansion as residual risks. They are reachable violations of governing repository authority, not optional hardening.

## Overall Result

**FAIL**

Six material repository-rule findings remain. Two can directly corrupt audit completeness/cardinality (`STD-R2-001`, `STD-R2-002`); two weaken repository ownership and enforcement boundaries (`STD-R2-003`, `STD-R2-004`); and two leave mandatory Rust/authority drift (`STD-R2-005`, `STD-R2-006`).
