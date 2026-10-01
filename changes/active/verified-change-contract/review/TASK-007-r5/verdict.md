# TASK-007 R5 Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `1e857c89f116fc7901e365bc456db6a20cbea684`
- Approved specification:
  `changes/active/verified-change-contract/spec.md`, revision 36, explicitly
  user-approved on 2026-09-24
- Original task:
  `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Remediation inputs: TASK-007-R1 through TASK-007-R4 and their prior review
  artifacts

The candidate remained the stated commit throughout both review waves. The
untracked R5 review artifacts were excluded from the reviewed subject.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| `REQ-097`-`REQ-099`, `INV-011`: failed-only transactional fan-out and independent durable delivery | Transactional settlement, unique dispatch identity, separate leased worker, provider-acceptance boundary, and real-server fan-out/status coverage | PASS |
| `REQ-138`-`REQ-141`: bounded context, per-attempt current credential, Slack and PagerDuty contracts | Closed failure context, fresh tenant-scoped connection lookup, private provider modules, provider journeys, and selector/redaction proofs | PASS |
| `REQ-142`, `REQ-146`, `REQ-152`, `INV-015`: bounded retry/deadline and PostgreSQL-owned coordination | Fixed budgets and database-owned scheduling are present, but a maximum accepted decimal `Retry-After` reaches interval multiplication before deadline clipping and raises `interval out of range` | **FAIL — `FIND-TASK-007-19`** |
| `REQ-143`: Workflow remains typed but unavailable for failure delivery | Registration rejects Workflow `on_failure`; the worker cannot report it delivered | PASS |
| `REQ-145`, `INV-007`, `AC-030`: RBAC, transactional audit, tenant isolation, capacity, shutdown, and recovery | Canonical permissions/audit, forced RLS, permit-before-claim, fairness, supervision, restart, and shutdown proofs | PASS |
| `REQ-147`, `AC-031`: encrypted tenant connections and approved env/file/Vault key-source policy | Envelope encryption, exact key versions, Vault readiness, and rotation/rewrap pass, but production single-tenant configuration accepts the development-only env source | **FAIL — `FIND-TASK-007-18`** |
| `REQ-148`, `REQ-150`: typed redacted HTTP/Rust/Python/TypeScript/CLI/MCP management surfaces | Shared typed client, thin projections, served OpenAPI, MCP catalog, generated contracts, and public journeys | PASS |
| `REQ-149`: exact connection authority and screen/pin before credential attachment | Registration and every attempt reuse compatibility checks; focused initial/redirect SSRF ordering tests pass | PASS |
| `AC-029`: real-server provider fan-out and bounded delivery | Local mock-provider journeys cover supported providers and failure paths; credentialed live smoke remains gated release evidence | PASS with stated verification limit |
| Scope and non-goals | No extra provider, cipher, dependency, cache, broker, Alert, executable Workflow, checked-in OpenAPI, CLI secret argv, or exactly-once claim | PASS |
| Prior remediation findings `FIND-TASK-007-1` through `-17` | Cumulative source retains R1-R4 corrections; revision 36 closes the former key-source ownership decision | PASS / CLOSED |

The complete obligation matrix and source evidence are preserved in
`task-review.md`; the domain-specific paths are preserved in the two domain
reports.

## Wave 1 results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | FAIL | `TREV-R5-001` |
| Repository standards | PASS | None |
| Security, tenancy, credentials, and SSRF | FAIL | `SEC-TASK-007-1` |
| Delivery durability, concurrency, and retry | FAIL | `DELIVERY-1` |

All required Wave 1 reports completed within the user-specified reviewer
cutoff. `TREV-R5-001` and `SEC-TASK-007-1` described the same configuration
path.

## Validated finding ledger

The independent Wave 2 review revised and deduplicated the proposals:

| Finding | Status | Classification | Required correction |
|---|---|---|---|
| `FIND-TASK-007-18` | REVISED | VIOLATION | Reject `OperatorKeySource::Env` in production in the existing configuration validator while preserving development env, production single-tenant file/Vault, production multi-tenant Vault/HTTPS, and deferred single-tenant provider reads. |
| `FIND-TASK-007-19` | REVISED | INCORRECT | In the existing retry SQL, bound the requested delay by the existing deadline operand before interval multiplication, retaining PostgreSQL ownership, the absolute deadline clip, attempt/deadline predicates, and fencing. |

Neither correction requires a new product, public API, architecture, security,
concurrency, resource-ownership, dependency, or persistent-data decision.

## Prior-finding closure

`FIND-TASK-007-1` through `FIND-TASK-007-17` are **CLOSED**. In particular,
revision 36 authoritatively closes `FIND-TASK-007-16` by approving the concrete
server-owned env/file/HashiCorp Vault key sources and deferring the absent
generic resolver. `FIND-TASK-007-18` is a narrower implementation mismatch
inside that approved policy, not a reopened ownership decision.

## Verification limits

- Current Wave 1 checks passed: `git diff --check`, `mise run fmt`,
  `codegen:check`, client-tier, PyO3-scope, tenant-isolation, unwrap-audit, and
  three focused SSRF/Slack tests. The delivery reviewer reproduced the exact
  PostgreSQL 17 `interval out of range` failure.
- The candidate records prior green SQL/shared/server, CLI/MCP,
  Rust/Python/TypeScript journey, typecheck, lint, codegen, and boundary lanes;
  the time-bounded review did not rerun every broad lane.
- The credentialed Slack/PagerDuty live smoke remains intentionally gated and
  was not run. Local provider paths establish the retained delivery finding
  without vendor credentials.
- CodeGraph was unavailable because the repository has no `.codegraph/`
  index; reviewers traced source and callers directly.

## Verdict

**FIX_REQUIRED**

The candidate substantially satisfies TASK-007 and closes every prior finding,
but the two independently validated, bounded implementation defects above
remain. Remediation is specified in
`TASK-007-R5-operator-key-source-and-retry-bounds.md`.
