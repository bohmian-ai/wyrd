# TASK-007 R3 Wave 1 Task Implementation Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `57ee8dd0b3dddc3e1da34bf543ecc566bae2e831`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Prior review and remediation inputs:
  `changes/active/verified-change-contract/review/TASK-007-r1/`,
  `changes/active/verified-change-contract/review/TASK-007-r2/`,
  `TASK-007-R1-operator-delivery-corrections.md`, and
  `TASK-007-R2-operator-delivery-corrections.md`

The candidate was the stated commit at the beginning and end of this review.
The review inspected the complete cumulative base-to-candidate range and used
the prior artifacts only to identify obligations and prior finding IDs; the
judgments below come from candidate source, callers, tests, and independently
run focused checks.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-097–099: only completed failed binding-created runs create one independent durable dispatch per distinct Operator; no direct call, result rewrite, broker, or Alert | `wyrd-sql/src/queries/verifier_runs.rs` inserts dispatches in completion settlement; `operator_dispatches.rs`, `verification/claims.rs`, and `verification/operators.rs` keep claims and fenced settlement independent | `pg_operator_delivery::failed_verdict_fans_out_to_every_provider_independently`; SQL fencing coverage in `pg_verifier_runs` | PASS |
| REQ-138: one bounded immutable failure context and closed template field set | `wyrd-spec/src/card/operator.rs::OperatorFailureContext`, frozen dispatch JSON, and registration/render validation admit only the approved result summary and identities | Contract tests and the real-server provider journey | PASS |
| REQ-139: resolve the latest exact tenant/provider/name credential per attempt; fail closed with no secret/selector diagnostics | `OperatorWorker::credential` performs a fresh tenant read/open per attempt; `KeyError` retains only source kind/version/failure class; the public detail and remediation are constant and selector-free | Independently run `components::operators::keys::tests::key_failures_disclose_no_selector` passed | PASS; prior `FIND-TASK-007-1` closed |
| REQ-140: Slack channel/token request and JSON `ok` handling | `verification/operators/slack.rs` owns request construction and response classification; common screened transport remains in `OperatorDelivery` | Independently run `verification::operators::slack::tests::slack_reply_classification` passed; mock-provider journey covers payload and failure behavior | PASS; prior `FIND-TASK-007-12` implementation boundary closed for Slack |
| REQ-141: PagerDuty Events v2 route, severity, summary, source, stable dedup key, and HTTP acceptance boundary without stronger delivery claims | `verification/operators/pager_duty.rs::event` builds the provider body and `OperatorDelivery::pager_duty` keeps the HTTP acceptance boundary | Mock-provider journey covers payload and retry identity; the new provider rustdoc promises guaranteed incident collapse, contrary to the approved limit | FAIL (`TREV-R3-002`) |
| REQ-142 and REQ-146: bounded attempts, deadline, backoff, concurrency, shutdown/restart, response bounds, retry classification, and delivery availability beside rewrap | `OperatorDispatchQueue`, `ClaimLoop`, `OperatorWorker::process`, screened transport, and runtime limits preserve the fixed ceilings; rewrap runs as a concurrent future under pass/tenant budgets | Slow endpoint, permit fairness, shutdown/restart, and `slow_rewrap_never_holds_back_another_tenants_delivery` journeys | PASS |
| REQ-143: Workflow remains non-executable for verification failure delivery | registration rejects Workflow `on_failure`; the worker also terminates a Workflow action as unavailable | Registration and provider journeys | PASS |
| REQ-145 and INV-007: read/write RBAC separation, RLS, and transactional authorization audit | typed control owners use `TenantConn`, normal permissions, and the canonical audit path; shared `audit::record_unless_committed` preserves each caller's commit boundary | Route/audit/RLS, principals, MCP, and tenant-isolation coverage | PASS |
| REQ-147: UUIDv7 forced-RLS ciphertext storage, authenticated envelope encryption, external exact tenant/version KEKs, production readiness, and bounded rewrap | SQL/envelope owners, active-tenant boot verification, owner-only files, HTTPS Vault, and bounded independent rewrap exist. API-bearing roles reject an oversized version, but `ForgeWorker` skips that validation and boot still constructs `OperatorKeys`, reaching the new `expect` on external configuration | Exact-version unit proof passes only through direct `OperatorKeysConfig::validate`; no proof covers non-API boot with the same value | FAIL (`TREV-R3-001`); prior `FIND-TASK-007-9` is closed, while `FIND-TASK-007-10` is incomplete on the reachable boot path |
| REQ-148 and REQ-150: identical typed closed CRUD projections over HTTP, shared client, Rust/Python/TypeScript SDKs, CLI, and MCP | provider-tagged wire contracts and redacted views live in `wyrd-spec`; all language/CLI/MCP surfaces project the shared client operations | Generated schemas/stubs, static typing fixtures, and surface journeys | PASS |
| REQ-149: exact active connection authority and resolve-screen-pin before credential attachment on every effective HTTP hop | registration and attempt predicates precede decryption; `HttpRequest` is nonsecret; `OperatorDelivery::http` builds a pinned client before `Credential::attach` for each initial/redirect URL | credential-attachment ordering tests and redirect/network mock journey | PASS; prior `FIND-TASK-007-3` remains closed |
| REQ-152, INV-015, and AC-033: PostgreSQL owns coordination timestamps and predicates | dispatch claim/retry/deadline SQL uses `statement_timestamp()` and returns remaining duration rather than a wall-clock deadline | SQL coordination and dispatch tests manipulate database state | PASS |
| INV-006, INV-011, and INV-013: no Card secrets; failed-only independent reaction; inline/reference parity | connection names replace secret selectors, settlement is the sole durable handoff, and both frozen target forms resolve through the same worker | schema, settlement, and fan-out tests | PASS |
| AC-029: complete local provider journey and gated live smoke boundary | common worker plus provider modules preserve supported delivery, retries, SSRF, and independent status | Local real-server mock journey exists; credentialed smoke remains deliberately gated and was not run | FAIL only for the stronger PagerDuty guarantee in `TREV-R3-002` |
| AC-030: authorization, fairness, runtime limits, shutdown/restart, health, metrics | permission owners, permits, queue, supervisor, and metrics are present | authorization and runtime integration journeys | PASS |
| AC-031: tenant-admin encrypted CRUD, redaction, RLS, rotation, and every first-class surface | connection service/SQL/keys plus shared client and thin projections implement the complete path | route, Postgres, Rust/Python/TypeScript, CLI, and MCP journeys | PASS for the tenant-admin path; the separate non-API boot regression is covered by `TREV-R3-001` |
| R2 `FIND-TASK-007-1`: public remediation contains no generic or concrete selectors | `wyrd-spec/src/error.rs:1696-1708`; complete problem is built from constant safe metadata | Independently run selector-redaction test passed | PASS |
| R2 `FIND-TASK-007-9`: the complete rewrap pass bounds discovery and returns without mutation on expiry | `OperatorKeys::rewrap_pass` applies one `timeout_at(deadline, referenced_key_versions(...))`, then caps each tenant by the remaining pass budget | `stalled_rewrap_discovery_returns_within_the_pass_budget` locks discovery, asserts no mutation, releases, and proves the later pass succeeds | PASS |
| R2 `FIND-TASK-007-10`: representable version is exact and oversized input cannot panic or select another key | API-role validation rejects above `i32::MAX` and `OperatorKeys` stores exact `i32`; non-API validation/boot disagree | Existing test covers direct validation but not the reachable `ForgeWorker` boot path | FAIL (`TREV-R3-001`) |
| R2 `FIND-TASK-007-11`: key configuration/owner `Debug` is redacted by construction | custom `Debug` implementations emit only source kind, active version, and Vault presence/client state | Independently run both key-debug redaction tests passed | PASS |
| R2 `FIND-TASK-007-12`: provider wire details live behind focused private modules with common transport/policy single-owned | Slack request/reply details are in `slack.rs`; PagerDuty event fields are in `pager_duty.rs`; common screening, auth ordering, response limits, and status policy remain in `operators.rs` | Source inspection plus Slack unit and provider journey evidence | PASS structurally; `TREV-R3-002` concerns the new PagerDuty contract claim, not module placement |
| Original/R1 security corrections: production readiness, HTTPS/token-file policy, selector-safe errors, nonblocking file reads, and attachment order | boot/config/key and screened transport owners retain all corrections | Focused source/tests and prior real-server paths inspected | PASS; `FIND-TASK-007-2`, `-3`, `-5`, and `-7` remain closed |
| Scope and task non-goals: no new provider, public API, cipher/dependency, cache, broker, Alert, executable Workflow, checked-in OpenAPI, or exactly-once/dedup guarantee | Product/code scope remains bounded, except the extracted PagerDuty module now states an unconditional incident-collapse guarantee | Cumulative diff inspection | FAIL (`TREV-R3-002`) |

## Proposed findings

### `TREV-R3-001` — REGRESSION: an ignored Forge-worker Operator-key setting now panics boot

- **Violated obligation:** R2 `FIND-TASK-007-10` requires an oversized active
  version to be refused before boot/provider access and preserved exactly when
  accepted. `AGENTS.md` also forbids `expect` on environment/configuration
  input. Existing role validation deliberately ignores API-only settings for a
  `ForgeWorker`.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/config.rs:2869-2876`,
  `crates/wyrd/wyrd-server/src/boot/mod.rs:1489-1502`, and
  `crates/wyrd/wyrd-server/src/components/operators/keys.rs:248-265`.
- **Evidence:** `WyrdServerConfig::validate` calls
  `OperatorKeysConfig::validate` only when `role.serves_api()`. The existing
  `forge_worker_validation_ignores_api_only_settings` test establishes that a
  dedicated Forge worker intentionally skips such validation. Nevertheless,
  every role goes through `attach_config_fields`, which constructs
  `OperatorKeys`; the constructor converts `active_version` with `expect`.
  Consequently a Forge-worker TOML or environment value of `2147483648`
  passes `WyrdServerConfig::load` and then panics during `build_state`.
- **Reachable consequence:** a valid dedicated Forge deployment can be taken
  down by an irrelevant Operator-key setting instead of either ignoring it or
  returning a typed configuration/boot error. The new exact-version invariant
  is therefore not actually established at every constructor call.
- **Required testable correction:** preserve the existing role-ownership rule:
  do not construct configured `OperatorKeys` for a role that does not serve the
  API/Operator runtime, leaving its existing default unused owner in state.
  Keep the current fail-closed validation and exact conversion for every role
  that does own Operator keys; do not add a second version type or broaden
  Forge validation to unrelated API settings. Add one focused boot/config test
  showing an oversized TOML/environment Operator version cannot panic a
  `ForgeWorker`, while the same value remains a typed pre-boot refusal for an
  API-bearing role.

### `TREV-R3-002` — VIOLATION: PagerDuty rustdoc promises guaranteed incident deduplication

- **Violated obligation:** REQ-141 says PagerDuty deduplication *may* group
  repeated trigger events and expressly forbids treating the stable dedup key
  as an exactly-once guarantee. TASK-007 likewise excludes an exactly-once
  delivery claim.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/verification/operators/pager_duty.rs:12-15`.
- **Evidence:** the newly extracted provider function says the dispatch-ID
  default means “every retry of a dispatch collapses into one PagerDuty
  incident.” That unconditional outcome is stronger than the approved
  best-effort provider behavior. The implementation itself correctly sends a
  stable key and treats HTTP acceptance as success; only the permanent source
  contract overclaims the downstream result.
- **Observable consequence:** maintainers and generated internal Rust
  documentation are told that an accepted retry is guaranteed to collapse to
  one incident, although Wyrd neither observes nor controls PagerDuty's final
  grouping behavior.
- **Required testable correction:** keep the stable dispatch-ID fallback and
  all wire behavior unchanged; replace the guarantee with the approved bounded
  statement that PagerDuty may group repeated trigger events and that the key
  is not an exactly-once delivery or incident guarantee. Source review is the
  focused closure proof; no runtime test or abstraction is warranted.

## Prior-finding closure

| Prior finding | R3 status |
|---|---|
| `FIND-TASK-007-1` | CLOSED: public detail, remediation, problem JSON, and logs are selector-free. |
| `FIND-TASK-007-2` | CLOSED: multi-tenant production verifies every active tenant key before readiness. |
| `FIND-TASK-007-3` | CLOSED: each effective URL is screened and pinned before credential attachment. |
| `FIND-TASK-007-4` | CLOSED: unrelated workflow-skill changes are absent. |
| `FIND-TASK-007-5` | CLOSED: production Vault requires HTTPS and token files use owner-only reads. |
| `FIND-TASK-007-6` | CLOSED: Python and TypeScript expose the precise closed provider contracts. |
| `FIND-TASK-007-7` | CLOSED: mounted-secret reads use Tokio's blocking boundary. |
| `FIND-TASK-007-8` | CLOSED: changed production/library imports are module-scoped. |
| `FIND-TASK-007-9` | CLOSED: rewrap is delivery-independent, bounded across discovery and tenants, and reuses old versions per tenant pass. |
| `FIND-TASK-007-10` | OPEN: API-role validation is exact, but non-API boot can reach the constructor invariant without validation (`TREV-R3-001`). |
| `FIND-TASK-007-11` | CLOSED: all three key configuration/owner types have redacted custom `Debug`. |
| `FIND-TASK-007-12` | CLOSED: provider wire fields and Slack response interpretation are confined to private provider modules; common transport/policy stays single-owned. |

## Verification evidence and limits

- Independently run at candidate `57ee8dd0b3dddc3e1da34bf543ecc566bae2e831`:
  the exact six-test `wyrd-server` unit selector covering public key redaction,
  both redacted `Debug` boundaries, exact-version validation, Slack reply
  classification, and shared key decoding. All six passed.
- The stalled-discovery Postgres test source was inspected end to end: it holds
  an `ACCESS EXCLUSIVE` lock, measures the pass budget, asserts zero mutation,
  releases the lock, and proves the next pass rewraps all three rows. This
  reviewer did not rerun the Postgres wrapper.
- Candidate evidence records the original task's SQL/shared/server/CLI/MCP and
  Rust/Python/TypeScript journey, typecheck, codegen, boundary, format, and lint
  lanes as exit zero. Those broad lanes were treated as available evidence but
  were not independently rerun here.
- The existing exact-version test calls the private validation owner directly
  on the default API-bearing configuration; it cannot falsify
  `TREV-R3-001`'s dedicated-Forge path. The path is established directly by
  the role gate, unconditional boot constructor, and constructor `expect`.
- Credentialed Slack/PagerDuty smoke remains gated release evidence and was
  not run. Generated schemas and language declarations were traced through
  their owners and codegen/typecheck evidence rather than reviewed line by
  line.

## Overall result

**FAIL**

The cumulative candidate closes the R2 security, rewrap, diagnostic, exact
API-role version, and provider-isolation work, but the exact-version change
introduces one reachable dedicated-Forge boot panic and the new PagerDuty
module violates the explicit no-guaranteed-dedup claim. Both corrections are
bounded and require no specification revision, dependency, schema, or public
API change.
