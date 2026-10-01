# TASK-007 R4 Wave 1 Task Implementation Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `e2da27694e8a3d057f2ff863c5d17429adc0a495`
- Approved specification:
  `changes/active/verified-change-contract/spec.md`, revision 35
- Original task:
  `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Remediation inputs: `TASK-007-R1-operator-delivery-corrections.md`,
  `TASK-007-R2-operator-delivery-corrections.md`, and
  `TASK-007-R3-operator-delivery-corrections.md`, with their prior verdicts and
  validated ledgers

The candidate was the stated commit at the beginning and end of this review.
The checkout has no `.codegraph/` directory, so source and caller inspection
used the immutable Git range and repository files directly. This review covers
the complete cumulative base-to-candidate implementation, not only R3.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `REQ-097`: failed binding-created completion settles the run and inserts one idempotent dispatch per distinct effective Operator in the same transaction; all other outcomes and direct runs create none | `wyrd-sql/src/queries/verifier_runs.rs` owns transactional settlement/fanout; unique dispatch identity is enforced by the migration and SQL owner | `pg_operator_delivery::failed_verdict_fans_out_to_every_provider_independently`; verifier-run settlement/fencing tests | PASS |
| `REQ-098`: a separate generic leased worker owns independent delivery status/retry without a broker, Alert table, or direct Verifier call | `verification/operators.rs::OperatorWorker`, shared `verification/claims.rs::ClaimLoop`, and `wyrd-sql/src/queries/operator_dispatches.rs` separate committed dispatch work from the Verifier runner | Provider fanout, restart/fencing, shutdown, and retry integration tests | PASS |
| `REQ-099`: only failed completed binding runs invoke supported Notify/HTTP actions; later passes never rewrite earlier results; provider acceptance is the boundary | Settlement predicates and the independent worker preserve result immutability and provider-acceptance settlement | Mock-provider journey exercises accepted, retrying, and terminal siblings | PASS |
| `REQ-138`: each dispatch freezes one bounded closed failure context and rejects unknown template fields | `OperatorFailureContext` in `wyrd-spec/src/card/operator.rs` contains the approved identifiers, failed verdict, completion time, and bounded summary; registration/rendering share the closed field set | Contract tests and the real-server delivery journey | PASS |
| `REQ-139`: every attempt resolves the latest exact tenant/provider/name credential; failures and diagnostics disclose neither secret nor selector | `OperatorWorker::credential` performs a fresh tenant-scoped read/open per attempt; `KeyError` retains bounded source/version/failure data and maps to constant public detail | Selector-redaction, rotation-next-attempt, revoked-connection, and tenant-isolation tests | PASS; prior `FIND-TASK-007-1`, `-6`, and `-11` remain closed |
| `REQ-140`: Slack uses the named bot-token connection and authored channel and inspects JSON `ok` | Private `verification/operators/slack.rs` owns Slack request/reply wire behavior; common delivery owns transport policy | Slack reply unit test and provider fanout journey | PASS |
| `REQ-141`: PagerDuty Events API v2 carries route, severity, summary, exact source, and stable dedup key without an exactly-once/grouping promise | Private `verification/operators/pager_duty.rs::event` builds the approved payload; corrected rustdoc says grouping is only possible and disclaims exactly-once/incident guarantees | Provider journey asserts the default dispatch-ID dedup key; source inspection confirms R3's prose-only correction | PASS; prior `FIND-TASK-007-13` closed |
| `REQ-142`: attempts, retries, response bodies, redirects, and total delivery are bounded; every effective URL is screened/pinned before credential attachment | `OperatorDelivery` screens and pins before `Credential::attach`; `OperatorDispatchQueue` owns retry/deadline ceilings and stable identity | Slow endpoint, retry, redirect/network, credential-attachment ordering, and ambiguous-send coverage | PASS; prior `FIND-TASK-007-3` closed |
| `REQ-143`: Workflow remains typed but non-executable for verification failure delivery | Registration rejects Workflow in `on_failure`; worker has no successful Workflow execution path | Registration/provider negative coverage | PASS |
| `REQ-145`: existing permissions, exact Card scope, transactional audit, and unaudited engine mechanics are preserved | Operator connection service uses `operators:read`/`operators:write`, registration uses `operators:invoke`, `TenantConn`, and canonical audit composition | Route permission/audit tests, principals integration, MCP discovery/refusal, and tenant-isolation checks | PASS |
| `REQ-146`: 16 global/4 tenant permits, three attempts, 30-second attempt, five-minute deadline, 30-second/two-minute retry, bounded shutdown, restart, health, and metrics | `RuntimeLimits`, `OperatorDispatchQueue`, `ClaimLoop`, supervisor, health, and metrics owners preserve the fixed ceilings; permits precede claims | Per-tenant fairness, slow endpoint, shutdown/release, restart, and metrics coverage | PASS |
| `REQ-147`: UUIDv7 forced-RLS encrypted storage, canonical AAD, fresh DEK/nonce, exact key versions, production startup validation, and bounded rewrap | SQL/envelope/rewrap implementation supplies these behaviors. However, `config.rs` introduces a HashiCorp-specific `VaultKeysConfig` and `keys.rs` directly implements Vault KV v2 over `reqwest`, instead of using the approved existing external-secret resolver and `SecretRef::Vault` boundary | Storage/AAD/tamper/rotation/startup/rewrap tests prove the concrete Vault path, but no test can prove the required shared resolver/backend boundary because it is absent | **FAIL (`TREV-R4-001`)** |
| `REQ-148`: typed create/list/get/update/disable exists over HTTP, Rust/Python/TypeScript SDKs, CLI, and MCP; reads are redacted and writes audited | `wyrd-spec` wire owners, mounted routes, one `wyrd-client::OperatorConnections`, and thin projections implement the shared operations | HTTP, Rust, Python, TypeScript, CLI, MCP, OpenAPI, permission, and redaction journeys | PASS |
| `REQ-149`: registration and every attempt require active exact authority; HTTP origin/auth/header match; forbidden headers and origin-changing redirects fail before secret attachment | Card registration and `OperatorWorker::credential` use the compatibility predicate; screened per-hop clients precede attachment | Registration-authority integration test and delivery redirect/network cases | PASS |
| `REQ-150`: create/update/read use closed provider-tagged shapes and typed UUIDv7 IDs; Cards contain connection names, not credentials | `wyrd-spec/src/operator_connection.rs` and `card/operator.rs` own the closed unions and normalized `HttpsOrigin`; generated schemas and language declarations project them | Contract, schema, Python/TypeScript static typing, malformed-ID, and provider-view tests | PASS |
| `REQ-152`, `INV-015`, `AC-033`: PostgreSQL owns coordination timestamps and time predicates | Connection/run/dispatch SQL assigns and compares coordination time with `statement_timestamp()` and returns database verdicts/remaining intervals | SQL lease, deadline, retry, schedule, and dispatch tests control database state rather than a process wall clock | PASS |
| `INV-006`: Verifier Cards contain no secret or mutable runtime health | Operator Cards carry only named connections and nonsecret delivery configuration; secret-bearing values remain in write-only connection requests | Schema/fixture secret rejection and redaction tests | PASS |
| `INV-007`: tenant isolation and transactional authorization audit cover registration, run/result, and Operator surfaces | Forced RLS `TenantConn` paths and canonical audit append/commit boundaries remain in the owning services | Cross-tenant CRUD/registration/runtime tests and audit allow/deny coverage | PASS |
| `INV-011`: Trigger owns activation, Verifier owns verdict, generic runner creates failed-only dispatches, and Operators own independent reaction state | Scheduling, result settlement, and dispatch claims remain separate owners; no Alert resource exists | Settlement/fanout/provider journeys | PASS |
| `INV-013`: inline and referenced Operators have the same semantics without hidden Cards | Registration freezes referenced UID or inline digest and dispatch execution resolves both forms through the same worker | Registration and dispatch identity tests | PASS |
| `AC-029`: a real-server local-provider journey covers supported fanout, Slack/PagerDuty/HTTP wire behavior, failures, retries, SSRF, and independent statuses; live smoke remains gated | `pg_operator_delivery.rs` drives the generic runner and all providers; the ignored test uses the same worker for release credentials | Local journey exists and recorded server lane is green; credentialed live smoke was not run in this review, as the task explicitly gates it outside fast lanes | PASS with stated release-evidence limit |
| `AC-030`: authorization, pool fairness, timeout/retry/deadline, shutdown/recovery, supervision, health, and metrics are exercised | Permission owners, shared permits, durable queue, supervisor, and telemetry are present | Principals/tenant integration plus operator fairness, slow endpoint, shutdown, restart, and metrics tests | PASS |
| `AC-031`: tenant-admin encrypted CRUD, rotation, disable/re-enable, RLS/redaction, authority refusal, and all first-class management projections work | Connection control, SQL, envelope keys, shared client, SDK, CLI, and MCP owners cover the full surface | Route/Postgres and Rust/Python/TypeScript/CLI/MCP journeys, including flattened provider views and next-attempt rotation | PASS except the external-resolver boundary already failed under `REQ-147` |
| Task R1/R2 remediation obligations (`FIND-TASK-007-1`–`-12`) | Constant selector-free errors, production key readiness, screen-before-attach, removal of unrelated skill edits, HTTPS/owner-only secrets, exact SDK unions, nonblocking file reads, module imports, bounded independent rewrap, exact version validation, redacted debug, and private provider wire modules remain in the cumulative source | Prior focused proofs remain present; source/callers were rechecked where later remediation touched them | PASS |
| Task R3 remediation obligations (`FIND-TASK-007-10`, `-13`, `-14`, `-15`) | `OperatorKeys::for_role` leaves non-API Forge on the unused default owner; `OperatorKeys::new` returns `VersionOutOfRange`; PagerDuty claim is corrected; four trait methods have invariant rustdoc; cited standard types use module imports | Independently ran the exact four-test server selector: 4 passed, 406 skipped; source inspection confirms the prose/import corrections | PASS |
| Scope/non-goals: no new provider contract, cipher/dependency, credential cache, broker, Alert, executable Workflow, checked-in OpenAPI, secret CLI argv, weaker security, or exactly-once claim | Most non-goals hold, but the candidate silently substitutes a concrete HashiCorp Vault config/client for the approved backend-agnostic external resolver. The task names a different key-provider contract as a material stop condition | Complete cumulative diff and base-source search | **FAIL (`TREV-R4-001`)** |

## Proposed findings

### `TREV-R4-001` — VIOLATION: the candidate replaces the approved external-secret resolver with a bespoke HashiCorp Vault client

- **Violated obligation:** `REQ-147` fixes the deployment key provider as the
  existing external-secret resolver through `SecretRef::Vault`, whose configured
  backend may be Vault, AWS Secrets Manager, or Google Secret Manager. The
  original task repeats that owner at lines 21–24, requires reuse of the
  `SecretRef::Vault` resolver at lines 98–105, and makes a different key-provider
  contract a material stop condition at lines 233–239.
- **Exact location:** approved boundary at
  `changes/active/verified-change-contract/spec.md:1046-1051` and
  `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md:21-24,98-105,233-239`;
  replacement implementation at
  `crates/wyrd/wyrd-server/src/config.rs:1776-1863,1895-1955` and
  `crates/wyrd/wyrd-server/src/components/operators/keys.rs:270-301,320-400`;
  changed architecture claim at `architecture/wyrd-design.md:1383-1394`.
- **Evidence:** the candidate adds `OperatorKeySource::{Env, File, Vault}` and a
  HashiCorp-specific `VaultKeysConfig` containing address, mount, prefix, and
  token settings. `OperatorKeys::new` constructs its own `reqwest::Client`, and
  `vault_key` assembles and calls a Vault KV v2 URL with `X-Vault-Token`. Neither
  configuration nor execution constructs `SecretRef::Vault` or calls a shared
  external-secret resolver. A base-commit and candidate source search finds the
  documented `SecretRef` type but no implemented `wyrd_auth::resolve` or shared
  external-secret resolver to reuse, so the implementation did not merely miss
  an available call site: it silently chose a new concrete provider contract
  after the task's stop condition had been reached.
- **Observable consequence:** multi-tenant production is locked to HashiCorp
  Vault KV v2 and cannot use an already configured AWS Secrets Manager or Google
  Secret Manager backend through the approved backend-agnostic boundary.
  Operator KEKs also acquire a second provider configuration and resolution path
  instead of sharing the declared Wyrd secret owner, and current architecture
  text now contradicts the approved specification rather than recording the
  required resolver boundary.
- **Required testable correction:** do not add another local adapter as task
  remediation. Resolve the missing material owner decision first: either supply
  and approve the shared external-secret resolver contract that `SecretRef`
  already documents, then have `OperatorKeys` pass
  `<prefix>/<tenant>/<version>` through `SecretRef::Vault`, or explicitly revise
  the specification and task to approve the concrete HashiCorp Vault-only
  boundary. After that decision, the focused proof must configure the selected
  external backend through the shared resolver, read the exact tenant/version
  32-byte key, fail startup for missing/malformed keys, and show no
  HashiCorp-specific selector or token surface in the Operator key owner unless
  that concrete contract was explicitly approved. This is not a bounded
  implementation-only choice under the current approved task.

## Prior-finding closure

| Prior finding | R4 status |
|---|---|
| `FIND-TASK-007-1`–`FIND-TASK-007-9` | CLOSED; the cumulative source retains the validated R1/R2 corrections. |
| `FIND-TASK-007-10` | CLOSED; oversized direct/API construction is typed, exact `i32::MAX` is preserved, and non-API Forge ignores the unused setting without constructing it. |
| `FIND-TASK-007-11`–`FIND-TASK-007-12` | CLOSED; debug remains selector-free and provider wire behavior remains privately owned. |
| `FIND-TASK-007-13` | CLOSED; PagerDuty documentation now describes only possible grouping and expressly disclaims guarantees. |
| `FIND-TASK-007-14` | CLOSED; all four named trait methods carry the required invariant/error documentation. |
| `FIND-TASK-007-15` | CLOSED; the cited standard-library types are imported at module top and used by bare name. |

## Verification evidence and limits

- Independently run at candidate
  `e2da27694e8a3d057f2ff863c5d17429adc0a495`:
  `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E
  'test(=components::operators::keys::tests::oversized_active_version_is_typed_or_ignored_by_role)
  | test(=config::tests::oversized_operator_key_version_follows_role_ownership)
  | test(=config::tests::operator_key_version_must_fit_i32)
  | test(=config::tests::operator_keys_debug_redacts_selectors)'`; all four
  selected tests passed and 406 were skipped.
- `git diff --check base..candidate` passed. Candidate `HEAD` remained unchanged.
- The cumulative candidate contains and prior remediation records green results
  for the original SQL/shared/server/CLI/MCP, Rust/Python/TypeScript journey,
  typecheck, codegen, tenant/client/PyO3/unwrap, format, and lint lanes. This
  time-bounded reviewer did not rerun all broad lanes.
- The credentialed Slack/PagerDuty live smoke remains gated release evidence and
  was not run. That is an explicit task boundary, not the basis of the finding.
- No executable test can close `TREV-R4-001` on this candidate: the required
  shared resolver is absent, while the tests exercise only the substituted
  concrete Vault client.

## Overall result

**FAIL**

R3's four findings are closed, and the concrete connection/delivery behavior is
well covered. The cumulative candidate nevertheless does not satisfy the
approved key-provider ownership contract. Because the named shared resolver is
absent and the task explicitly required stopping rather than choosing a
different provider contract, closure needs an approved specification/ownership
decision rather than another local implementation patch.
