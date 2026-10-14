# Integrated review verdict

## Immutable subject

- Repository: detached candidate worktree at this directory.
- Base: `ef03f624a1f3a6bcd51bbd17060271f0b5e8cabf`.
- Candidate: `c4ba2b0502f8968a78e5e960bcc9eba43e960643`.
- Approved authority: `changes/active/example-journey-fixes/spec.md`, revision 4.
- Binding authority: `changes/active/example-journey-fixes/owner-decisions.md`, OD-1 through OD-7.
- Original tasks: `TASK-001-jev-typed-provider.md` through
  `TASK-005-local-keyring.md`; TASK-005 is the OD-5 replacement.
- Reviewed range: the complete `ef03f624a..c4ba2b050` diff.

The candidate commit stayed unchanged throughout review. Only this review
directory was written.

## Independent review results

| Required pass | Report | Result |
|---|---|---|
| Behavior implementation | `task-review-behavior.md` | FAIL |
| Invariant implementation | `task-review-invariants.md` | FAIL |
| Repository standards | `standards-review.md` | PASS |
| Maintainer | `maintainer-review.md` | FAIL |
| System resilience | `system-review.md` | PASS |
| Security/RBAC/secrets | `domain-review-security.md` | FAIL |
| Tenant-scoped persistent data | `domain-review-data.md` | FAIL |
| Bifrost/DataFusion | `domain-review-bifrost.md` | FAIL |
| Attribution/trace | `domain-review-observability.md` | FAIL |
| Focused conflict resolution | `followup-review.md` | RESOLVED |
| Structured Ponytail validation | `findings-validation.md` | FIX_REQUIRED |

The focused follow-up was required because repository and maintainer reviews
disagreed on the rustdoc gate, and discovery raised ambiguous authority
boundaries for direct execution, historical attribution, credential-body
relay, and `tracestate`. The follow-up resolved those questions. The Ponytail
validator then independently checked every proposal. It rejected `SEC-001`:
arbitrary transformed-provider output is not a bounded regression and OD-4
requires the existing sanitized refusal-body relay. No specification revision
is required.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 typed Jev provider | Typed Skald wire/provider, gateway adapter, native and unified routes, SDK projections, judge path | Recorded unit and Rust/Python/TypeScript journeys | **FAIL**: unified preparation applies forbidden custom field bounds (`FIND-4`); changed fallible Jev methods also fail the hard documentation gate (`FIND-5`) |
| REQ-002 actionable Bifrost caller errors | Shared Oracle planning-error mapper after table authorization | Mapper unit passed; existing language/MCP journeys cover unknown column/table/function | **FAIL**: real type-mismatch and ambiguous-reference paths are not proved (`FIND-8`) |
| REQ-003 MCP compatibility | Object-root operator create args and full-catalog schema test; official SDK journey | Recorded Rust catalog and TypeScript framework journeys | PASS |
| REQ-004 execution attribution | Independent gateway Run/Card context, result requester, SYSTEM resolution, origin references and observation link | Recorded SQL/server/SDK journeys | **FAIL**: direct execution can succeed while dropping its result without SYSTEM (`FIND-1`); historical internal rows are not backfilled (`FIND-6`) |
| REQ-005 trace linking | Server inbound middleware, MCP re-scope, in-process inheritance and observation links | Recorded malformed-parent and cross-surface trace journeys | **FAIL**: OD-3-forbidden SDK injection remains (`FIND-3`), and malformed `tracestate` is retained (`FIND-7`) |
| REQ-006 live-table subqueries | Live-leaf scalar-subquery filter stays on the leader; shared DataFusion oracle covers four states | Mapper and support-workload focused tests passed; recorded state matrix | **FAIL**: no live query correlates on managed `run_id` (`FIND-9`) |
| REQ-007 deployment sealing keyring | Existing `SealingKeyring`, binding payload, shared rewrap inventory and local key provisioning | Recorded rotation, restart, tenant-isolation and CLI journeys | PASS |
| INV-001 tenant isolation | Tenant connections/RLS, bound secret envelopes, query authorization before planning | Recorded `check:tenant-isolation` and cross-tenant tests | PASS |
| INV-002 credentials and keys are never returned or logged | Secret wrappers, bound sealed payloads and shared refusal scrubbing | Recorded negative and leak checks; `SEC-001` rejected by independent validation | PASS |
| INV-003 audit remains non-blocking | Existing Scribe outbox path retained | Diff and system-resilience audit | PASS |
| INV-004 only approved typed Jev chat entry | Unified typed pass-through; no legacy shim or translation | Source and route journeys | PASS |
| AC-001 required user journeys | Extensive Rust/Python/TypeScript/MCP/CLI journeys exist | Task evidence | **FAIL**: missing Bifrost caller-fault and managed-`run_id` cases (`FIND-8`, `FIND-9`) |
| AC-002 required negative flows | Invalid Jev type/refusals, unauthorized Bifrost table and malformed `traceparent` | Recorded focused journeys | PASS |
| AC-003 format, lints, codegen and owning lanes | All task files record green scoped commands | Task records; local diff check and two Bifrost focused tests passed | PASS on recorded evidence, subject to limits below |
| AC-004 external `wyrd-example-ts` journey | No execution record exists | Candidate and task-evidence audit | **FAIL** (`FIND-2`) |
| Non-goals: no other provider, new Verifier semantics or public release | No such surface entered the diff | Complete diff audit | PASS |
| OD-1 actor and origin | New work uses caller/SYSTEM and origin references | SQL and runtime tests | **FAIL**: `FIND-1`, `FIND-6` |
| OD-2 optional independent gateway context | Optional Run/Card fields and unchanged direct authorization | Four-way attribution tests | PASS |
| OD-3 server-side trace context; no client injection | Server middleware exists | Source and journeys | **FAIL**: broad outbound SDK injection was added (`FIND-3`) |
| OD-4 approved Jev r2 | Both routes, typed pass-through, shared refusal and Agent refusal | Jev tests/journeys | **FAIL**: unified route adds prohibited per-field validation (`FIND-4`) |
| OD-5 deployment sealing keyring | Rejected keyring deleted; shared sealing path and no restart | Secret/CLI evidence | PASS |
| OD-6 analytical correctness bar | Shared seeded workload, DataFusion oracle, hand checks, published/live/mixed/remote states | Focused oracle tests and recorded state lanes | PASS |
| OD-7 root causes and minimum complexity | Most fixes extend existing owners | Complete diff and review | **FAIL** because `FIND-3` retains explicitly excluded machinery |

## Validated finding ledger

The decision-complete source traces and corrections are in
`findings-validation.md`. Only independently confirmed or revised findings are
retained here.

| ID | Status | Class | Exact evidence | Observable consequence |
|---|---|---|---|---|
| `FIND-example-journey-fixes-1` | REVISED | INCORRECT | `crates/wyrd/wyrd-server/src/components/verification/service.rs:417` | Direct execution can return a successful judgment after dropping the whole result and requester when SYSTEM is absent. |
| `FIND-example-journey-fixes-2` | CONFIRMED | MISSING | `changes/active/example-journey-fixes/spec.md:192` and absent task evidence | The motivating external example remains unproved without its forbidden workarounds. |
| `FIND-example-journey-fixes-3` | CONFIRMED | VIOLATION | `crates/shared/wyrd-client/src/transport/trace.rs:1`; `crates/shared/wyrd-client/src/transport/http.rs:876`; foreign-runtime bridges | Automatic client trace injection contradicts OD-3 and changes unrelated authenticated requests. |
| `FIND-example-journey-fixes-4` | CONFIRMED | VIOLATION | `crates/wyrd/wyrd-gateway/src/adapter/jev.rs:63` | Unified Jev requests can be rejected by custom field bounds OD-4 explicitly excluded. |
| `FIND-example-journey-fixes-5` | CONFIRMED | VIOLATION | `crates/skald/skald-providers/src/clients/jev.rs:134`; `crates/vala/vala-eval/tests/integration/orchestrator_judge_skald.rs:440` | Changed fallible/panicking methods violate the hard rustdoc gate. |
| `FIND-example-journey-fixes-6` | CONFIRMED | MISSING | `crates/wyrd/wyrd-sql/migrations/20261014000001_verifier_run_requester_every_origin.sql:1` | Resolvable historical internal actors remain null while new rows record SYSTEM. |
| `FIND-example-journey-fixes-7` | CONFIRMED | INCORRECT | `crates/wyrd/wyrd-server/src/http/middleware/trace_context.rs:52`; `crates/wyrd-spec/src/trace.rs:19` | Invalid W3C `tracestate` is persisted instead of being dropped independently of a valid parent. |
| `FIND-example-journey-fixes-8` | CONFIRMED | MISSING | `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3739` and current query journeys | Type-mismatch and ambiguous-reference mapping can regress while all current journeys stay green. |
| `FIND-example-journey-fixes-9` | CONFIRMED | MISSING | `changes/active/example-journey-fixes/spec.md:148` and current live-subquery journeys | Managed-`run_id` live correlation remains unproved. |

## Verification limits

- Task evidence records format, lint, codegen, dependency, tenancy, docs and
  owning journey lanes green.
- Independent focused reruns passed the Bifrost error-mapper test and the two
  support-workload oracle tests.
- `git diff --check ef03f624a..c4ba2b050` passed.
- A local `codegen:check` rerun could not complete because this sandbox could
  not write/download the Python tool environment. Its temporary generated-file
  formatting was restored exactly; the candidate remained clean.
- Postgres, peer-cluster, Python, TypeScript, external-provider and external
  example suites were not independently repeated. This is material for AC-004
  and is captured by `FIND-example-journey-fixes-2`.

## Prior-finding closure

This is the first integrated review of this immutable candidate. There are no
prior `FIND-*` items to close.

## Verdict

**FIX_REQUIRED**

The nine retained findings are bounded within the approved specification and
owner decisions. They are packaged in
`TASK-006-R1-integrated-acceptance-remediation.md` for a fresh
`$wyrd-implement` pass.
