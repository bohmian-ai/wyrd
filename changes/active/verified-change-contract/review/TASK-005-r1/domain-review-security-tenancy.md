# Security And Tenancy Domain Review

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `9cfe9b69c5990603e07458aa4402f6750131bfbc`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Task: `changes/active/verified-change-contract/tasks/TASK-005-production-drift-verifier.md`

The candidate remained at the stated commit during this review.

## Reviewed Boundary

This review covered only reachable tenant-isolation, authorization, trust-boundary,
and resource-isolation behavior required by TASK-005:

- tenant-scoped resolution of the exact baseline Data Card and registered artifact;
- baseline work/status RLS, cross-tenant discovery, claims, leases, and settlement;
- authorized Card status exposure of baseline identity and structured errors;
- direct and binding-created run subject authorization;
- fixed typed Oracle plans, authenticated provider replacement, exact subject/window
  filters, table authorization, and tenant tripwires;
- fitter resource admission for tenant-supplied Parquet;
- cross-tenant SDK and Postgres journey coverage.

## Authority Coverage

| Boundary | Governing authority | Source and evidence inspected | Result |
|---|---|---|---|
| Verified tenant and Card scope for manual runs | `architecture/wyrd-security-posture.md` security principles and authorization; `spec.md` REQ-082, REQ-145, INV-007 | `components/verification/service.rs:245-380`; Rust SDK refusal journey `drift_verification.rs:656-740` | PASS |
| Exact baseline Card resolution and artifact binding | `spec.md` REQ-072, REQ-073, REQ-113; task scenarios 1-2 | `components/cards/resolve.rs:249-345`; `components/cards/service.rs:1142-1205`; `verification/fitter.rs:216-323` | PASS |
| Baseline control-state tenancy | `AGENTS.md` sections 2, 3, 9; `architecture/agent-rules.md` TenantConn/RLS rules; `architecture/wyrd-security-posture.md` tenant and data isolation; `spec.md` REQ-078 | migration `20260601000032_drift_baselines.sql:12-59`; `queries/drift_baselines.rs:34-137,172-394`; `pg_drift_baselines.rs:400-478` | PASS |
| Authorized status exposure | `spec.md` REQ-074, REQ-100, REQ-134, REQ-145 | `components/cards/service.rs:152-360`; `queries/drift_baselines.rs:112-126,325-370`; Rust SDK status assertions `drift_verification.rs:218-250,460-475` | PASS |
| Oracle table and row boundary | `architecture/wyrd-security-posture.md` authorization and tenant tripwire; `architecture/references/domain/olap-serving.md`; `spec.md` REQ-080, REQ-082, REQ-113, INV-010; drift architecture fixed-plan contract | `verification/drift.rs:93-267,395-669`; `oracle/planner.rs:174-240,300-334`; `oracle/mod.rs:4680-4753` | PASS |
| Shared verifier/baseline admission | `spec.md` REQ-115 and REQ-146; task scenario 2 refactor requirement | `verification/mod.rs:344-409`; `verification/fitter.rs:145-213`; `verification/runner.rs:218-272`; `verification/permits.rs` | FAIL (`SEC-TEN-001`) |
| Bounded tenant-supplied artifact processing | `architecture/wyrd-security-posture.md` client request bounds and tenant semi-trust; `spec.md` REQ-115; task outcome and scenario 2 bounded fitter | `verification/fitter.rs:47-51,191-253,297-316`; `verification/mod.rs:46-84,361-367` | FAIL (`SEC-TEN-002`) |

## Security Audit

### Critical

None.

### High

- **SEC-TEN-002 — VIOLATION** — [`crates/wyrd/wyrd-server/src/verification/fitter.rs:227`](../../../../../crates/wyrd/wyrd-server/src/verification/fitter.rs) reads a tenant-authored Parquet object and lines 235-250 decode every batch into a `Vec`, concatenate the complete decoded dataset, and run the fitter in a detached `spawn_blocking` operation. The only size check, at lines 297-315, bounds compressed object metadata to 256 MiB; it does not bound decoded Arrow memory, row count, CPU time, or the blocking task's lifetime. `RuntimeLimits::execution_timeout` is not supplied to `BaselineFitter` at `verification/mod.rs:361-367`, and dropping the awaited `spawn_blocking` handle on shutdown does not stop the blocking closure. **Exploit path:** an authenticated tenant with normal Card registration authority uploads a highly compressed Parquet baseline whose stored size is below 256 MiB but whose decoded columns are much larger, then registers a PSI or SPC Verifier. The background fitter can exhaust shared process memory/CPU; cancellation or lease release can detach the still-running work, and a retry can start another copy. **Impact:** one tenant can crash or starve the shared server and prevent other tenants' baseline and verification progress, contrary to the required bounded worker/engine behavior. **Required correction:** enforce a decoded-work budget while reading bounded batches instead of collecting and concatenating the entire file, and make timeout/cancellation stop further decode/fit work rather than detach it; retain the existing structured failed-baseline status. **Focused closure proof:** a small-on-disk/highly-compressible Parquet fixture whose decoded size exceeds the configured bound must settle `failed` without excessive allocation, and a cancellation/timeout test must prove no decode/fit work continues or is duplicated after the lease is released.

### Medium

- **SEC-TEN-001 — VIOLATION** — [`crates/wyrd/wyrd-server/src/verification/mod.rs:361`](../../../../../crates/wyrd/wyrd-server/src/verification/mod.rs) constructs `BaselineFitter` without the existing `VerifierPermits`, while a separate permit set is created only for `VerifierRunner` at lines 384-389. The fitter claims and executes work directly at `verification/fitter.rs:145-213`. This violates REQ-146's one shared ceiling of 16 global and 4 per tenant across Verifier and baseline executions, and the task explicitly requires reuse of the generic runtime permits. **Exploit path:** a tenant keeps four Verifier runs active and registers a due baseline; the same process may execute the baseline as a fifth tenant operation, while sixteen runs plus one fit exceed the global ceiling. **Impact:** the promised tenant fairness and process admission bounds do not hold, allowing one tenant to consume capacity reserved for other tenants. **Required correction:** create one existing `VerifierPermits` owner in runtime composition and share it with both runner and fitter; the fitter must acquire the tenant and global permit before claiming baseline work and hold it through settlement. **Focused closure proof:** saturate one tenant's four shared permits and show its due baseline remains unclaimed while another tenant can progress; separately show aggregate active runs plus fits never exceed sixteen.

### Low / Defense In Depth

None. Optional hardening outside the approved task was not reported.

### Positive Controls

- `drift_baselines` uses composite tenant/Card foreign keys, forced RLS, and
  tenant-only mutation grants; the privileged pool can only discover due tenant IDs.
- Baseline registration resolves references under `TenantConn`, pins exact Card UIDs,
  and writes the pending baseline in the Card registration transaction.
- The fitter re-enters through the discovered tenant's `TenantConn`, uses a fixed
  tenant-qualified artifact path, and fences settlement with the lease token.
- Card status is read in the caller's existing tenant transaction and does not expose
  fitted profile contents.
- Manual runs evaluate `evals:run`, enforce signed Card scope for Card-bound callers,
  and transactionally audit allow and deny decisions.
- Drift plans use typed DataFusion literals rather than interpolated SQL, filter exact
  subject/series/half-open window, and enter Oracle's catalog-derived table
  authorization before source materialization.
- Oracle provider replacement fails closed when an authenticated provider is absent,
  and the changed rebuild preserves scan projection, filters, fetch, and statistics
  requests.
- Postgres and SDK tests exercise cross-tenant baseline reads/claims/inserts, foreign
  run rejection, and cross-tenant result refusal.

## Verification Limits

- This was a static, review-only audit. I did not rerun the task's reported `mise`
  lanes or construct denial-of-service fixtures.
- The task evidence reports successful `test:vala`, `test:sql`, `test:wyrd`,
  `test:bifrost`, SDK journeys, `check:tenant-isolation`, format, and lints; the
  relevant source tests were inspected, but command output was not independently
  reproduced here.
- No existing test exercises combined runner-plus-fitter permit accounting or a
  compressed-to-large decoded Parquet baseline, so the two findings remain unclosed.

## Overall Result

**FAIL**

Tenant identity, RLS, Card scope, fixed-plan authorization, and status isolation are
implemented coherently, but `SEC-TEN-001` and `SEC-TEN-002` leave the tenant-supplied
baseline path outside the approved shared admission and bounded-execution contract.
