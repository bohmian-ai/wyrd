---
id: TASK-008
kind: implementation
status: proposed
spec: SPEC-verified-change-contract
spec_revision: 35
requirements: [REQ-089, REQ-101, REQ-114, REQ-151, REQ-152, INV-015, AC-017, AC-020, AC-021, AC-022, AC-023, AC-024, AC-030, AC-032, AC-033]
depends_on: [TASK-005, TASK-006, TASK-007, TASK-009, TASK-010]
---

## Outcome and Value

The complete Verifier change is proven as one production-shaped product across
Rust, Python, TypeScript, HTTP, MCP, multi-server Bifrost, Postgres restart,
authorization, tenant isolation, resource ceilings, and current architecture.
This task closes cross-task seams and missing journey registration; it adds no
new durable behavior or alternate implementation.

## Owners, Scope, Consumers, and Prohibited Changes

`wyrd-testing` owns production-shaped servers, clusters, telemetry capture, and
capability journey registration. Each language runtime owns its lifetime-
dependent tests. Existing `mise` capability lanes own environment setup.
Architecture and generated contracts are static closure.

Do not repair a failing journey by weakening assertions, mocking away a real
server/dependency, adding sleeps, changing approved behavior, or building a
second harness. Do not add product code solely to satisfy a test unless the
test reveals an in-scope missing seam owned by TASK-001 through TASK-007 or
TASK-009.

## Approach

1. Audit every REQ/INV/AC against landed focused proof and identify only
   missing cross-owner seams.
2. Add/complete the minimum real SDK/server journeys in existing harnesses and
   register them in current `mise` lanes.
3. Exercise multi-server, restart, concurrency, authorization/audit, partial
   result, provider, and analytical pruning scenarios without credentials in
   fast lanes.
4. Regenerate all public artifacts and remove stale architecture/build
   references to retired surfaces.
5. Run the complete affected capability gates sequentially and record exact
   evidence for final change review.

## Proof Strategy

TDD is not applicable because this task is verification-only: executable
behavior is introduced by TASK-001 through TASK-007 and TASK-009. Each missing
journey is a regression/conformance proof against already-required behavior;
it must fail first for the expected missing seam or absent test registration,
then pass without inventing a new product contract.

The integrated proof matrix must include:

- Rust/Python/TypeScript registered Service journeys containing PSI, SPC,
  Custom, deterministic Eval, and local LLM-judge Eval bindings; each crosses
  exact-principal authentication, locked run API, queue/IPC, Gate/Scribe,
  runtime, Bifrost result query, status, and one failed-result Operator.
- Python framework-created spans inside `with state.run(card="...")` carrying
  the exact run ID and CardRef through OTLP to server-resolved Bifrost Card UID,
  including persisted joins to caller-owned custom rows by `run_id` and Eval
  rows by trace/span identity, nested Card scopes, asyncio isolation,
  private-provider install, and fail-open absence or failure of optional
  OpenTelemetry integration.
- HTTP/MCP manual binding and direct Drift runs with requester identity,
  nullable direct owner/binding, idempotency, unauthorized and cross-tenant
  refusals, and existing Bifrost query for result/detail rows.
- Multi-server result writing where the worker owns no Scribe, including the
  complete internal tenant SYSTEM identity/table matrix, public principal and
  credential-path refusals, single existing-issuer/JWT reuse, one Gate audit
  decision, and one raw observation feeding two independently filterable
  bindings.
- Daily partitions and physical Bloom evidence across two time partitions,
  Oracle partition/row-group pruning, exact Eval record-day lookup, and one
  result event time across ACKs straddling UTC midnight.
- Duplicate record/batch/cron handling, expired leases, retry exhaustion,
  fail-open Eval enqueue, partial result visibility without settlement,
  zero-detail summary-only publication, restart recovery, and no stale old
  table/route/crate path.
- PostgreSQL-owned activity, schedule, claim, lease, retry, and dispatch
  deadlines, with no process wall-clock predicate, skew workaround, injectable
  coordination clock, or permanent source checker.
- Permission allow/deny audit at every public/Gate boundary, no audit for
  internal mechanics, per-tenant/global saturation fairness, Operator timing
  ceilings, shutdown drain/reclaim, supervisor health, and required telemetry.
- Operator connection CRUD/rotation/redaction and local Slack/PagerDuty/HTTP
  protocol fixtures, including typed path IDs, secret-free CLI argv/debug,
  served OpenAPI, and runtime MCP catalogs. Credentialed live smokes remain
  release-gated evidence.
- Exact Card-bound API-key and workload-`jwt-bearer` exchanges activate owners;
  delegation, human refresh, Card-free automation, SYSTEM issuance, cached
  bearer requests, and idle expiry do not. Principal suspension blocks new
  work immediately while ordinary authorization retains the current
  five-minute permission-snapshot semantics.

## Acceptance Criteria

- Every spec obligation maps to a passing strongest applicable proof or an
  explicitly inherited static contract; no acceptance criterion relies only on
  a lower tier when a journey is required.
- All user-facing surfaces use the same server-owned durable behavior and
  stable errors.
- The current tree contains no stale authoritative alternative, generated
  drift, retired route/table/crate/check reference, or unregistered journey.
- No material decision is made in this task; discoveries requiring one return
  to `$wyrd-spec`.

## Expected Write Set and Consumer Closure

Expected changes are limited to existing journey/integration test homes,
capability registration in `mise.toml` when missing, generated artifacts from
their sources, and permanent architecture/public documentation required by
`REQ-114`. Production changes are allowed only as bounded corrections to a
missing in-scope seam in its established owner and must be recorded.

## Verification and Evidence

Run Cargo-backed work sequentially. Record every specifically named test with
its exact focused command. The integrated lanes are:

```bash
mise run test:cards:integration
mise run test:principals:integration
mise run test:sql
mise run test:shared
mise run test:vala
mise run test:wyrd
mise run test:wyrd-sdk
mise run verify:bifrost
mise run test:e2e
mise run test:bifrost:journey:sdk
mise run test:bifrost:journey:server
mise run test:bifrost:journey:mcp
mise run test:bifrost:journey:python
mise run test:bifrost:journey:typescript
mise run py:test:unit
mise run py:test:integration
mise run py:typecheck
mise run ts:test:unit
mise run ts:test:integration
mise run ts:typecheck
mise run ts:napi:check
mise run codegen:check
mise run check:client-tier
mise run check:pyo3-scope
mise run check:tenant-isolation
mise run check:registry-tx-coupling
mise run check:from-pools-allowlist
mise run check:unwrap-audit
mise run fmt
mise run py:format
mise run lints
mise run py:lints
git diff --check
```

This change is intentionally broad; after all capability lanes pass, run
`mise run gate` as whole-plan closeout evidence.

## Material Stop Conditions

Stop for any need to change a public/persisted contract, security or tenancy
semantics, reliability ceiling, accepted non-goal, dependency direction, or
test tier requirement. A proven unrelated baseline failure is recorded and
does not justify weakening proof.

## Authority Links

- `changes/active/verified-change-contract/spec.md`
- all documents under `changes/active/verified-change-contract/architecture/`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/wyrd-security-posture.md`
- `architecture/references/languages/testing-workflows.md`
- `AGENTS.md`
