# TASK-007 R5 Wave 2 Findings Validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `1e857c89f116fc7901e365bc456db6a20cbea684`
- Approved specification: `changes/active/verified-change-contract/spec.md`,
  revision 36, explicitly user-approved on 2026-09-24
- Original task:
  `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Remediation inputs: TASK-007-R1 through TASK-007-R4, their prior verdicts,
  and their validated ledgers

The candidate was the stated repository `HEAD` before and after validation.
The untracked R5 review directory is not part of the reviewed commit. This
checkout has no `.codegraph/` directory, so caller tracing used the immutable
candidate and `rg` directly.

## Independent source and caller validation

| Boundary | Complete reachable path inspected | Result |
|---|---|---|
| Operator key-source validation | `app::run` -> `WyrdServerConfig::load` -> `WyrdServerConfig::validate` -> `OperatorKeysConfig::validate`; accepted config -> boot state attachment -> `OperatorKeys::for_role` -> `OperatorKeys::key` from connection sealing, delivery opening, and rewrap | An API-bearing, production, explicitly single-tenant config passes with `OperatorKeySource::Env`; this contradicts revision-36's development-only environment-source constraint |
| Production readiness | `boot::verify_operator_keys` -> `OperatorKeys::verify_active` -> `OperatorKeys::key` | The early return for explicitly single-tenant production is intentional for deferred file/Vault availability, but it makes the accepted production env configuration live rather than repairing the configuration-policy defect |
| Provider retry classification | Slack `post_json`, PagerDuty `post_json`, and authored HTTP `http` -> `status_failure` -> `Attempt::Retry` -> `OperatorWorker::settle` | Every retryable provider path accepts a decimal `Retry-After` up to `u64::MAX` seconds and forwards it unchanged |
| Durable retry settlement | `OperatorWorker::settle` -> `OperatorDispatchQueue::retry` -> `RETRY_SQL`; all other `OperatorDispatchQueue::retry` callers are SQL integration tests | `millis` converts the maximum parsed delay to `i64::MAX`; `RETRY_SQL` constructs that interval before its outer `LEAST`, so PostgreSQL can fail with `interval out of range` instead of durably scheduling or terminating the dispatch |
| Prior remediation | R1-R4 tasks and ledgers -> cumulative candidate owners and focused proofs | Findings 1-17 remain closed as detailed below; neither new defect reopens their correction boundary |

`standards-review.md` proposed no finding. Its empty ledger was checked against
the same changed owners and the cumulative diff; no additional repository-rule
finding is added.

## Wave 1 proposal decisions

| Wave 1 proposal | Decision | Independent disposition |
|---|---|---|
| `TREV-R5-001` | **REVISED** | The reachable defect and proposed owner are correct. It is deduplicated with `SEC-TASK-007-1` as `FIND-TASK-007-18` and classified as an explicit constraint violation. The closure proof is expanded to preserve all approved env/file/Vault boundaries. |
| `SEC-TASK-007-1` | **REVISED** | Confirmed on the same live load/boot/key-read path and deduplicated into `FIND-TASK-007-18`. The correction belongs only in the existing configuration validator; single-tenant readiness must retain deferred provider availability. |
| `DELIVERY-1` | **REVISED** | The reachable arithmetic failure is confirmed as `FIND-TASK-007-19`. The smallest correction is selected from the proposal's alternatives: reuse PostgreSQL's existing deadline operand to bound `$5` with `$6` before interval multiplication in `RETRY_SQL`; no new Rust retry type, clock, or parser is needed. |

No proposal is rejected. Neither retained correction needs a new product,
public API, architecture, security policy, concurrency semantic, resource
owner, dependency, or persistent-data decision. `SPEC_REVISION_REQUIRED` is
not indicated.

## Final deduplicated finding ledger

### FIND-TASK-007-18 — REVISED — VIOLATION: production single-tenant deployments accept development-only environment KEKs

- **Wave 1 sources:** `TREV-R5-001`, `SEC-TASK-007-1`
- **Violated obligation:** revision-36 REQ-147 and TASK-007 Scenario 3 make
  environment-sourced Operator KEKs development-only. Production may use
  Vault, while an explicitly single-tenant production deployment may use the
  approved owner-only file source; production env is not an approved case.
- **Exact location:** contract at
  `changes/active/verified-change-contract/spec.md:1046-1065`, task at
  `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md:91-108`,
  validation at `crates/wyrd/wyrd-server/src/config.rs:1904-1925`, caller at
  `crates/wyrd/wyrd-server/src/config.rs:2484-2505,2871-2877`, and boot
  readiness boundary at
  `crates/wyrd/wyrd-server/src/boot/mod.rs:1442-1474`.
- **Evidence:** `WyrdServerConfig::validate` passes `multi_tenant = false`
  when `auth.tenant_slug` is configured. `OperatorKeysConfig::validate` then
  applies only the multi-tenant-production Vault rule and returns `Ok(())` for
  `OperatorKeySource::Env`. `app::run` consumes this validated config, and the
  single-tenant readiness path deliberately returns without reading a key.
  Later connection create/update and delivery paths read
  `WYRD_OPERATOR_KEK_V<version>`, so this is ordinary production behavior, not
  dormant or test-only code.
- **Observable consequence:** an explicitly single-tenant production server
  can create and open Operator credentials under a process-environment KEK,
  bypassing the approved production file/Vault boundary and exposing the root
  key to environment inheritance, capture, and inspection paths.
- **Decision-complete correction:** in the existing
  `OperatorKeysConfig::validate` owner, reject `OperatorKeySource::Env` whenever
  `production` is true. Preserve development env, production single-tenant
  owner-only file or Vault, production multi-tenant Vault-only/HTTPS, exact
  version validation, and the existing single-tenant deferred provider-read
  behavior. Update the validator's local rustdoc/error text as needed; add no
  source, resolver, flag, or abstraction.
- **Focused closure proof:** one focused configuration test proves development
  env is accepted, production single-tenant env is rejected before boot or key
  access, production single-tenant file remains accepted, and production
  multi-tenant non-Vault remains rejected. Retain the existing readiness tests
  proving multi-tenant active-key verification and single-tenant deferred file
  availability.

### FIND-TASK-007-19 — REVISED — INCORRECT: an oversized accepted `Retry-After` can prevent durable retry settlement

- **Wave 1 source:** `DELIVERY-1`
- **Violated obligation:** REQ-142 requires bounded provider-directed retry
  inside the five-minute dispatch deadline; REQ-146 fixes the retry budget and
  deadline; REQ-152 and INV-015 require PostgreSQL to own the coordination
  timestamp and durable scheduling decision.
- **Exact location:** decimal header parsing at
  `crates/wyrd/wyrd-server/src/verification/operators.rs:938-965`, production
  settlement at `crates/wyrd/wyrd-server/src/verification/operators.rs:395-420`,
  retry SQL at
  `crates/wyrd/wyrd-sql/src/queries/operator_dispatches.rs:99-117`, and binding
  at `crates/wyrd/wyrd-sql/src/queries/operator_dispatches.rs:274-299,342-345`.
- **Evidence:** `status_failure` accepts
  `Retry-After: 18446744073709551615` as `Duration::from_secs(u64::MAX)`.
  `millis` binds this as `i64::MAX`. PostgreSQL evaluates
  `statement_timestamp() + ($5 * INTERVAL '1 millisecond')` before the outer
  `LEAST` can clip the timestamp; the Wave 1 PostgreSQL 17 reproduction of that
  exact arithmetic returned `interval out of range`. Slack, PagerDuty, and
  authored HTTP all reach this classifier and settlement path. Existing proof
  covers only `Retry-After: 90`.
- **Observable consequence:** a retryable remote response can make settlement
  fail, leave the dispatch `running` until lease expiry, consume another
  attempt on reclaim, and repeat an externally ambiguous send instead of
  recording the required bounded retry or terminal transition.
- **Decision-complete correction:** keep PostgreSQL as the coordination owner
  and change the existing `RETRY_SQL` expression so the requested delay `$5`
  is bounded by the existing configured deadline `$6` with SQL `LEAST` before
  multiplication by `INTERVAL '1 millisecond'`. Retain the current outer
  absolute-deadline `LEAST`, attempt/deadline predicates, lease fence, error
  payload, and `millis` behavior for the other fixed queue ceilings. Do not
  add a retry type, clock, dependency, parser branch, or Rust-side deadline
  calculation.
- **Focused closure proof:** a focused Postgres-backed Operator retry test or
  existing real-server local-provider journey returns the maximum accepted
  decimal `Retry-After` and proves settlement succeeds, the row becomes
  `retrying` (or terminal when its existing budget/deadline requires),
  `next_attempt_at` never exceeds the database-owned five-minute deadline, and
  progress does not rely on lease expiry. Retain the ordinary 90-second
  provider-delay assertion and fenced/deadline SQL tests.

## Prior-finding closure

| Prior finding | R5 cumulative status and current evidence |
|---|---|
| `FIND-TASK-007-1` | **CLOSED** — `KeyError::log` and `From<KeyError> for WyrdError` expose stable classes and constant selector-free public text; sentinel coverage remains in `components/operators/keys.rs`. |
| `FIND-TASK-007-2` | **CLOSED** — multi-tenant production boot calls `verify_operator_keys` and `OperatorKeys::verify_active` over the active tenant directory before readiness. `FIND-TASK-007-18` concerns the separately approved source policy for single-tenant production. |
| `FIND-TASK-007-3` | **CLOSED** — initial and redirected HTTP URLs build a screened/pinned client before `Credential::attach`; fixed-provider bearer attachment likewise follows `client`. |
| `FIND-TASK-007-4` | **CLOSED** — the cumulative base-to-candidate diff contains no `.agents/skills` or `.claude/skills` change. |
| `FIND-TASK-007-5` | **CLOSED** — production Vault configuration requires HTTPS and Vault token files reuse the owner-only file reader. |
| `FIND-TASK-007-6` | **CLOSED** — Python exposes provider-discriminated `TypedDict` unions and TypeScript exposes the corresponding flattened redacted view/update unions. |
| `FIND-TASK-007-7` | **CLOSED** — mounted KEK and Vault token files use `spawn_blocking` through the shared owner-only reader. |
| `FIND-TASK-007-8` | **CLOSED** — the R1 function-scoped imports remain module-scoped. |
| `FIND-TASK-007-9` | **CLOSED** — rewrap runs beside claims; `rewrap_pass` applies one deadline to discovery and tenant work and reuses old versions within a pass. |
| `FIND-TASK-007-10` | **CLOSED** — API roles validate exact `i32` key versions, `OperatorKeys::new` returns `VersionOutOfRange`, and non-API roles retain the unused default owner without unwind. |
| `FIND-TASK-007-11` | **CLOSED** — key configuration uses redacted custom `Debug`, and `OperatorKeys` derives only through that redacted config and a token-free client. |
| `FIND-TASK-007-12` | **CLOSED** — Slack and PagerDuty request construction/interpretation remain in focused private provider modules while common screened transport stays single-owned. |
| `FIND-TASK-007-13` | **CLOSED** — PagerDuty rustdoc says stable dedup may group retry events and expressly disclaims exactly-once delivery and incident grouping. |
| `FIND-TASK-007-14` | **CLOSED** — the four previously cited trait/default methods retain their required invariant-focused rustdoc. |
| `FIND-TASK-007-15` | **CLOSED** — R4's cumulative declaration sweep remains present; the R5 standards review found no reopened import-manifest violation. |
| `FIND-TASK-007-16` | **CLOSED** — user-approved revision 36 now authoritatively selects the concrete server-owned env/file/HashiCorp Vault sources and records the resolver/cloud-source deferrals. |
| `FIND-TASK-007-17` | **CLOSED** — the MCP list descriptor promises only ID, provider, name, status, and nonsecret config, matching `OperatorConnectionView`; its catalog assertion remains present. |

## Ponytail disposition and recommendation

Both findings stop at existing shared owners. `FIND-TASK-007-18` is one
configuration-policy guard. `FIND-TASK-007-19` is one SQL-native bound using
the deadline already passed to the statement. No duplicated caller guard,
new dependency, abstraction, configuration knob, or contract is justified.

The validated ledger contains `FIND-TASK-007-18` and
`FIND-TASK-007-19`. Both are bounded implementation corrections within the
approved revision-36 behavior. The appropriate task-review outcome is
`FIX_REQUIRED`.

## Verification limits

- This validation was static and time-bounded. It inspected the complete
  cumulative diff inventory, every Wave 1 report, the original task, R1-R4
  tasks and ledgers, applicable authorities, and the full production owners
  and callers affected by both corrections. Generated schema snapshots were
  traced to source owners rather than reread line by line.
- `git diff --check f8811ac5..1e857c89` passed. No Cargo, Postgres, Python,
  TypeScript, codegen, or broad repository lane was rerun in Wave 2. Wave 1
  records current green format, boundary, codegen, and focused tests; the
  delivery reviewer independently reproduced the PostgreSQL interval failure.
- The credentialed Slack/PagerDuty live-provider smoke remains intentionally
  gated and was not run. Local-provider paths establish reachability of the
  retry defect without requiring vendor credentials.
- The candidate remained
  `1e857c89f116fc7901e365bc456db6a20cbea684` at completion.
