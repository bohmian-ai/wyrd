# TASK-007 R5 Wave 1 Task Implementation Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `1e857c89f116fc7901e365bc456db6a20cbea684`
- Approved specification:
  `changes/active/verified-change-contract/spec.md`, revision 36, explicitly
  user-approved on 2026-09-24
- Original task:
  `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Remediation inputs: `TASK-007-R1-operator-delivery-corrections.md`,
  `TASK-007-R2-operator-delivery-corrections.md`,
  `TASK-007-R3-operator-delivery-corrections.md`, and
  `TASK-007-R4-operator-key-source-revision.md`, with their prior verdicts and
  validated ledgers

The candidate was the stated commit at the beginning and end of this review.
The checkout has no `.codegraph/` directory, so source and caller inspection
used the immutable Git range and repository files directly. This review covers
the complete cumulative base-to-candidate implementation, not only R4.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `REQ-097`: failed binding-created completion atomically inserts one idempotent dispatch per distinct effective Operator; other outcomes and direct runs create none | `wyrd-sql/src/queries/verifier_runs.rs` owns transactional settlement/fanout and the dispatch uniqueness fence | `pg_verifier_runs::failed_binding_runs_dispatch_each_distinct_operator_once`; real-server provider fanout journey | PASS |
| `REQ-098`: a separate generic leased worker owns independent delivery status/retry without direct Verifier calls, a broker, or an Alert table | `verification/operators.rs::OperatorWorker`, `verification/claims.rs::ClaimLoop`, and `wyrd-sql/src/queries/operator_dispatches.rs` separate committed dispatch work from Verifier execution | Provider fanout, fenced settlement, restart, retry, and shutdown integration coverage | PASS |
| `REQ-099`: only failed completed binding runs invoke supported Notify/HTTP actions; later results remain independent; provider acceptance is the success boundary | Settlement predicates create failed-only dispatches; provider adapters classify provider acceptance without rewriting results | Mock-provider journey covers delivered, retrying, and terminal siblings while the Verifier run remains completed | PASS |
| `REQ-138`: each dispatch freezes one bounded, immutable, closed failure context and rejects unknown template fields | `wyrd-spec/src/card/operator.rs::OperatorFailureContext`, `MAX_SUMMARY_CHARS`, validation, and render helpers own the approved fields and bounds | Operator contract tests and the real-server delivery journey | PASS |
| `REQ-139`: every attempt resolves the latest exact tenant/provider/name credential and keeps secrets/selectors out of public and diagnostic surfaces | `OperatorWorker::credential` performs a fresh tenant-scoped lookup, compatibility check, and open on every attempt; key errors expose stable bounded classifications | Rotation-next-attempt, revoked connection, key outage, selector redaction, and tenant-isolation coverage | PASS; prior `FIND-TASK-007-1`, `-6`, and `-11` remain closed |
| `REQ-140`: Slack uses the named bot-token connection and authored channel and checks JSON `ok` | Private `verification/operators/slack.rs` owns the Slack wire contract; common delivery owns transport policy | Slack outcome unit coverage and provider fanout journey | PASS |
| `REQ-141`: PagerDuty carries the authored route/severity/summary, exact source, and stable dedup key without an exactly-once/grouping guarantee | Private `verification/operators/pager_duty.rs` owns payload construction and correctly qualified documentation | Provider journey asserts the default dispatch-ID dedup key | PASS; prior `FIND-TASK-007-13` remains closed |
| `REQ-142`: attempt timeout, retry budget/backoff, deadline, response/redirect bounds, stable identity, and per-hop screen/pin-before-attachment are enforced | `OperatorDispatchQueue`, `OperatorDelivery`, `HttpRequest`, and `Credential::attach` own the fixed limits and ordered send path | Slow endpoint, Retry-After, redirect/network, credential-ordering, timeout, and retry coverage | PASS |
| `REQ-143`: Workflow remains typed but non-executable for verification failure delivery | Registration rejects Workflow in `on_failure`; the worker cannot settle it delivered | Registration and worker negative coverage | PASS |
| `REQ-145`: existing permissions, exact Card scope, transactional audit, and unaudited engine mechanics are preserved | Connection control uses `operators:read`/`operators:write`; binding registration uses `operators:invoke`; tenant work uses `TenantConn` and canonical audit composition | Route permission/audit, principals, MCP refusal, and tenant-isolation coverage | PASS |
| `REQ-146`: 16 global/4 tenant permits, three attempts, 30-second attempt, five-minute deadline, 30-second/two-minute retry, bounded drain, restart, health, and metrics | `RuntimeLimits`, `OperatorDispatchQueue`, shared `ClaimLoop`, supervisor, and metrics owners implement the fixed ceilings; permits precede claims | Fairness, slow endpoint, shutdown/release, restart/health, and metrics coverage | PASS |
| `REQ-147`: UUIDv7 forced-RLS encrypted storage, canonical AAD, fresh DEK/nonce, exact key versions, approved deployment key sources, production restrictions, and bounded rewrap | SQL/envelope/rewrap behavior conforms, and revision 36 approves `OperatorKeys` with env/file/Vault. However, `OperatorKeysConfig::validate` returns `Ok(())` for `Env` in production whenever `auth.tenant_slug` makes the deployment explicitly single-tenant, although revision 36 says environment KEKs are development-only | Storage/AAD/tamper/rotation/readiness/rewrap tests cover the remaining behavior; no focused test rejects production single-tenant `Env` | **FAIL (`TREV-R5-001`)** |
| `REQ-148`: typed create/list/get/update/disable exists over HTTP, Rust/Python/TypeScript SDKs, CLI, and MCP; reads are redacted and writes audited | Closed wire types, five mounted routes, one shared `wyrd-client::OperatorConnections`, and thin first-class projections implement the shared operations | HTTP, Rust, Python, TypeScript, CLI, MCP, OpenAPI, permission, and redaction journeys | PASS |
| `REQ-149`: registration and every attempt require active exact authority; HTTP origin/auth/header match; forbidden headers and origin-changing redirects fail before secret attachment | Registration and delivery reuse the compatibility predicate; each effective URL is screened and pinned before its credential is attached | Registration-authority integration and redirect/network/attachment tests | PASS |
| `REQ-150`: create/update/read use closed provider-tagged shapes and typed UUIDv7 IDs; Cards contain connection names rather than credentials | `wyrd-spec/src/operator_connection.rs` and `card/operator.rs` own the closed unions, normalized origin, and redacted view; generated schemas and SDK declarations project them | Contract/schema, malformed-ID, provider-view, Python typing, and TypeScript typing coverage | PASS |
| `REQ-152`, `INV-015`, `AC-033`: PostgreSQL owns coordination timestamps and time predicates | Run/dispatch SQL assigns and compares coordination time with `statement_timestamp()` and returns database verdicts or remaining intervals | SQL lease, deadline, retry, schedule, and dispatch tests manipulate database coordination state | PASS |
| `INV-006`: Verifier and Operator Cards contain no secret material or mutable credential state | Cards carry only named connections and nonsecret action configuration; secret-bearing values remain write-only connection inputs | Schema/fixture secret rejection and redaction tests | PASS |
| `INV-007`: tenant isolation and transactional authorization audit cover registration, run/result, and Operator surfaces | Forced-RLS `TenantConn` paths and canonical audit append/commit boundaries remain in the owning services | Cross-tenant CRUD/registration/runtime and audit allow/deny coverage | PASS |
| `INV-011`: Trigger owns activation, Verifier owns verdict, generic settlement creates failed-only dispatches, and each Operator owns independent reaction state | Scheduling, result settlement, and dispatch claiming remain separate owners; no Alert resource exists | Settlement/fanout/provider journeys | PASS |
| `INV-013`: inline and referenced Operators retain the same semantics without hidden Cards | Registration freezes referenced UID or inline digest and one worker resolves both forms | Registration and dispatch identity coverage | PASS |
| `AC-029`: a real-server local-provider journey covers fanout, provider wire behavior, failures, retry/SSRF, and independent statuses; live smoke is gated | `pg_operator_delivery.rs` drives the real runner and all three providers; the ignored live smoke uses the same worker | Local provider journey is present and recorded green; credentialed Slack/PagerDuty smoke remains an explicit release-evidence limit | PASS with stated release-evidence limit |
| `AC-030`: authorization, fairness, timeout/retry/deadline, shutdown/recovery, supervision, health, and metrics are exercised | Permission owners, shared permits, durable queue, supervisor, and telemetry are present | Principals/tenant integration plus operator fairness, slow endpoint, shutdown, restart, and metrics tests | PASS |
| `AC-031`: tenant-admin encrypted CRUD, rotation, disable/re-enable, RLS/redaction, authority refusal, and all first-class projections work under the approved key-source policy | Connection control, SQL, envelope keys, shared client, SDK, CLI, and MCP owners cover the public surface, but production single-tenant env keys remain reachable | Route/Postgres and Rust/Python/TypeScript/CLI/MCP journeys cover the public behavior; no test closes `TREV-R5-001` | **FAIL (`TREV-R5-001`)** |
| R1-R3 remediation findings `FIND-TASK-007-1` through `FIND-TASK-007-15` | Selector-safe errors, startup readiness, screen-before-attach, HTTPS/owner-only Vault inputs, exact SDK unions, nonblocking file reads, bounded independent rewrap, exact version validation, redacted debug, private provider modules, role-safe construction, corrected PagerDuty claim, rustdoc, and module imports remain in cumulative source | Prior focused proofs remain present; later changes do not reopen their behavioral paths | PASS |
| R4 `FIND-TASK-007-16`: revision 36 records the user-approved concrete key-source decision | Spec front matter, `REQ-147`, task, architecture, revision history, and open-decision section consistently name env/file/HashiCorp Vault and the explicit deferrals | Source inspection of revision 36 and R4 diff | PASS |
| R4 `FIND-TASK-007-15` and `FIND-TASK-007-17`: remaining declaration imports conform and MCP list promises only returned fields | R4 imports declaration types at module scope; MCP list descriptor omits secret version | Cumulative source inspection and pinned MCP discovery assertion | PASS |
| Scope and non-goals: no extra provider, cipher/dependency, credential cache, broker, Alert, executable Workflow, checked-in OpenAPI, secret CLI argv, weaker SSRF/audit/tenancy, or exactly-once claim | Product non-goals remain excluded; the retained env-source production path is existing requested functionality with an overly broad availability boundary, not a new abstraction | Complete cumulative diff and source inspection | PASS except the explicit production key-source constraint failed under `TREV-R5-001` |

## Proposed findings

### `TREV-R5-001` — INCORRECT: production single-tenant deployments accept development-only environment KEKs

- **Violated obligation:** revision 36 `REQ-147` and TASK-007 Scenario 3 state
  that environment-sourced KEKs are development-only, while an explicitly
  single-tenant production deployment may use restrictive mounted files; the
  approved production alternatives are therefore file or Vault, not the
  process environment.
- **Exact location:** approved contract at
  `changes/active/verified-change-contract/spec.md:1046-1065` and
  `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md:90-109`;
  reachable validation path at
  `crates/wyrd/wyrd-server/src/config.rs:1904-1926,2871-2877`; boot bypass at
  `crates/wyrd/wyrd-server/src/boot/mod.rs:1442-1462`; environment read at
  `crates/wyrd/wyrd-server/src/components/operators/keys.rs:323-351`.
- **Evidence:** `WyrdServerConfig::validate` calls
  `OperatorKeysConfig::validate(production, tenant_slug.is_none())`. That
  validator rejects a non-Vault source only for
  `production && multi_tenant`, then unconditionally accepts
  `OperatorKeySource::Env`. With production plus `auth.tenant_slug = Some(_)`,
  `multi_tenant` is false, so the default/env source passes. Boot readiness
  also intentionally returns early for that single-tenant shape, and the live
  owner then reads `WYRD_OPERATOR_KEK_V<version>` on create/update/delivery.
  This is a normal configured server path, not dormant or test-only behavior.
- **Observable consequence:** an explicitly single-tenant production server
  can keep its Operator root key in a process environment variable even though
  the approved security boundary confines that source to development. It does
  not receive the owner-only mounted-file boundary revision 36 selected for
  production single-tenancy.
- **Required testable correction:** in the existing
  `OperatorKeysConfig::validate` owner, reject `OperatorKeySource::Env` whenever
  `production` is true. Preserve the current multi-tenant production Vault-only
  rule, HTTPS enforcement, and production single-tenant file/Vault choices;
  add no new source or abstraction. Add one focused configuration test proving
  development env remains accepted, production single-tenant env is refused,
  and the existing allowed production single-tenant file case remains valid.

## Prior-finding closure

| Prior finding | R5 status |
|---|---|
| `FIND-TASK-007-1` through `FIND-TASK-007-14` | CLOSED; the cumulative source retains the independently validated R1-R3 corrections. |
| `FIND-TASK-007-15` | CLOSED; R4's cumulative declaration sweep imports the remaining types at module scope. |
| `FIND-TASK-007-16` | CLOSED by the explicitly user-approved revision 36; the concrete `OperatorKeys` env/file/HashiCorp Vault ownership contract is now authoritative. |
| `FIND-TASK-007-17` | CLOSED; the MCP descriptor and its catalog assertion no longer promise a secret-version field. |

## Verification evidence and limits

- `git diff --check f8811ac5035c3aa165d34c38992f9889b3c9081f..1e857c89f116fc7901e365bc456db6a20cbea684`
  passed. Candidate `HEAD` remained unchanged.
- The cumulative candidate contains recorded green results for the original
  SQL/shared/server, CLI/MCP, Rust/Python/TypeScript journey, typecheck,
  codegen, tenant/client/PyO3/unwrap, format, and lint lanes. R4 additionally
  records green `fmt`, `lints`, `codegen:check`, focused MCP catalog,
  `test:bifrost:journey:mcp`, and `test:wyrd` results. This time-bounded
  reviewer inspected those tests and their source coverage but did not rerun
  all broad lanes.
- The credentialed Slack/PagerDuty live smoke remains gated release evidence
  and was not run. That is an explicit task boundary, not the basis of the
  finding.
- No current focused test covers the rejected configuration in
  `TREV-R5-001`; the production readiness test deliberately asserts only that
  explicitly single-tenant deployments defer provider availability, not that
  their configured source is one of the approved production sources.

## Overall result

**FAIL**

Revision 36 closes the prior key-provider ownership decision, and R4 closes
the remaining import and MCP descriptor findings. The cumulative candidate
still permits a development-only environment KEK in a reachable production
single-tenant configuration. That is a bounded implementation correction in
the existing configuration validator and does not require another
specification decision.
