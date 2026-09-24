# TASK-007 Wave 1 Task Implementation Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `b50ce7bc45c67f62dc7dabe34a2a1ef1ed421c34`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Remediation task: `changes/active/verified-change-contract/review/TASK-007-r1/TASK-007-R1-operator-delivery-corrections.md`

The candidate was the stated commit at the start and end of this review. This
review inspected the complete base-to-candidate range and used the prior review
only to establish remediation obligations; conclusions below come from the
candidate source, callers, tests, and recorded verification evidence.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-097–099: only completed failed binding runs fan out one independent durable dispatch per distinct Operator; no direct call, Alert, result rewrite, or sibling coupling | `verification/runner.rs`, `verification/operators.rs`, `wyrd-sql::operator_dispatches`, and the settlement transaction preserve the durable handoff and independent fenced settlement | `pg_operator_delivery::failed_verdict_fans_out_to_every_provider_independently`; recorded SQL/server/journey lanes | PASS |
| REQ-138: one bounded immutable failure context and closed template field set | `OperatorFailureContext`, frozen dispatch JSON, registration validation, and render paths admit only the approved IDs, identities, verdict, completion time, and bounded summary | Contract/schema tests and the real-server provider journey recorded green | PASS |
| REQ-139: resolve the latest exact tenant/provider/name credential per attempt; fail closed and leak no secret or selector | `OperatorWorker::credential` re-reads through `TenantConn`, checks active authority, and opens only for the attempt; `KeyError` retains only source kind/version/failure class and maps publicly to constant detail | Independently rerun `key_failures_disclose_no_selector` PASS; rotation and revoked/key-outage journeys recorded green | PASS |
| REQ-140: Slack channel/token request and JSON `ok` handling | `OperatorDelivery::slack` renders the authored channel/text, attaches the bearer after endpoint screening, bounds the response, and classifies Slack `ok`/error | Mock-provider fan-out journey recorded green; gated live smoke remains release-only | PASS |
| REQ-141: PagerDuty Events v2 route, severity, summary, stable dedup key, and acceptance boundary | `OperatorDelivery::send` freezes the dispatch ID fallback and emits the route in `payload.custom_details.wyrd_route`; HTTP acceptance is the success boundary | Mock-provider fan-out journey recorded green; gated live smoke remains release-only | PASS |
| REQ-142/146: bounded attempts, deadline, backoff, permits, retry classification, shutdown/restart, metrics, and no Verifier rerun | `OperatorDispatchQueue`, `ClaimLoop`, `OperatorWorker::process`, runtime limits, health supervision, and metrics retain the 3-attempt/30s/5m and 16/4 ceilings | Slow endpoint, permit fairness, crash/restart, shutdown, and retry journeys recorded green | PASS |
| REQ-143: Workflow remains non-executable | Registration rejects Workflow `on_failure`; `OperatorDelivery::send` also terminates a Workflow action as unavailable | Registration and provider journeys recorded green | PASS |
| REQ-145 / INV-007: read/write RBAC separation, forced RLS, and transactional authorization audit only at permission decisions | Typed routes use the existing permissions and canonical audit path; tenant storage uses `TenantConn`; worker mechanics do not authorize or audit | Connection route/audit/RLS, MCP, principals, and tenant-isolation lanes recorded green | PASS |
| REQ-147: UUIDv7 forced-RLS ciphertext storage, authenticated envelope encryption, external exact tenant/version KEKs, fail-start production readiness, and bounded rewrap | Encryption/AAD, SQL storage, active-tenant boot verification, HTTPS Vault, owner-only files, independent budgeted rewrap, and per-pass old-version reuse are present; however `OperatorKeys::active_version` silently substitutes `i32::MAX` for any configured version above that range | Crypto/SQL/route/boot/rewrap evidence is otherwise credible; no test rejects or preserves an out-of-range configured active version | FAIL (`TREV-R2-001`) |
| REQ-148/150: identical typed CRUD projections and closed provider-tagged create/update/redacted-view contracts | HTTP, shared Rust client, Rust SDK, CLI, MCP, Python `TypedDict` unions, and TypeScript discriminated unions project the flattened wire contract | Python/TypeScript static fixtures and integration assertions recorded green; codegen/typecheck lanes recorded green | PASS |
| REQ-149: exact connection authority and resolve-screen-pin before credential attachment on every effective HTTP hop | Registration and attempt-time authority checks precede decryption; `HttpRequest` is nonsecret; `OperatorDelivery::http` screens/pins each initial or redirect URL before `Credential::attach`; cross-origin redirects stop before another attachment | Independently rerun `blocked_and_unresolved_destinations_never_get_a_credential` and `each_screened_hop_attaches_its_own_credential` PASS | PASS |
| REQ-152 / INV-015 / AC-033: PostgreSQL owns coordination timestamps and due/expiry decisions | Dispatch creation, availability, lease, retry, and deadline predicates remain in the SQL queue using database time | SQL integration evidence recorded green; no new Rust wall-clock coordination predicate entered the remediation | PASS |
| INV-006/011/013: no Card secrets, failed-only independent reaction, inline/reference parity, and no Alert resource | Card contracts name connections only; frozen UID/digest resolution and generic dispatch path cover referenced and inline Operators without hidden Cards or Alerts | Contract, registration, settlement, and fan-out evidence recorded green | PASS |
| AC-029/030/031: real-server provider, runtime/authorization, and tenant-admin journeys across all required surfaces | Server provider adapters, CRUD, runtime, shared client, three SDKs, CLI, and MCP are all wired through the server-owned contracts | Recorded focused and broad lanes are green; six remediation unit tests were independently rerun and passed; live Slack/PagerDuty smoke is correctly gated | PASS except the unproved/out-of-range key-version behavior in `TREV-R2-001` |
| Remediation FIND-TASK-007-1: selector-safe failures | `KeyError`, `KeyFailure`, `KeyError::log`, and constant public detail contain no selectors or raw causes | Selector sentinel test independently passed | PASS |
| Remediation FIND-TASK-007-2: production key readiness | `boot::verify_operator_keys` calls `OperatorKeys::verify_active` for every active tenant before `build_state` returns | Production boot helper journey covers unavailable, missing, malformed, wrong-length, and valid keys | PASS |
| Remediation FIND-TASK-007-3: screen before credential attachment | Per-hop client screening occurs before `Credential::attach` or Slack bearer attachment | Both focused credential-attachment tests independently passed | PASS |
| Remediation FIND-TASK-007-4: remove unrelated workflow policy | Base-to-candidate diff under `.agents/skills` and `.claude/skills` is empty | Direct diff inspection | PASS |
| Remediation FIND-TASK-007-5: protect Vault transport and token file | Production validation requires HTTPS; Vault token files use `read_owner_only` before network use | TLS and owner-only token tests independently passed | PASS |
| Remediation FIND-TASK-007-6: exact Python/TypeScript contracts | Python and TypeScript now expose closed create/update/view unions matching the flattened Rust wire representation | Static rejection/acceptance fixtures and runtime flattened-view assertions recorded green | PASS |
| Remediation FIND-TASK-007-7: nonblocking mounted-secret reads | `read_owner_only` uses `spawn_blocking` for metadata and read, shared by KEK and Vault-token files | Stalled FIFO executor test independently passed | PASS |
| Remediation FIND-TASK-007-8: production/library imports at module scope | Changed production/library `base64::Engine` and `PermissionsExt` imports are at module scope | Source inspection; recorded format/lint lanes | PASS |
| Remediation FIND-TASK-007-9: rewrap cannot starve delivery | Rewrap runs concurrently beside `ClaimLoop`; pass/tenant time budgets cancel transactions; old KEKs are cached only inside one tenant pass | Slow-rewrap multi-tenant journey and rotation/outage regressions recorded green | PASS |
| Task non-goals and scope | No new provider, cipher, dependency, broker, credential cache, Alert, executable Workflow, read-secret API, checked-in OpenAPI, or compatibility path; unrelated skill changes are absent | Cumulative diff and manifests inspected | PASS |

## Proposed findings

### `TREV-R2-001` — INCORRECT: an accepted active key version is silently changed

- **Violated obligation:** REQ-147 requires the server to read, use, and persist
  the exact configured positive tenant key version. The task also requires
  exact tenant/version resolution and fail-closed key handling.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/config.rs:1843-1847,1873-1894` and
  `crates/wyrd/wyrd-server/src/components/operators/keys.rs:268-272`.
- **Evidence:** configuration accepts `NonZeroU32`, including values from
  `2147483648` through `4294967295`, and validation has no storage-range check.
  `OperatorKeys::active_version` then converts to the PostgreSQL `i32` key
  version with `i32::try_from(...).unwrap_or(i32::MAX)`. Every oversized value
  therefore becomes `2147483647` rather than being rejected. Boot readiness,
  new sealing, SQL metadata, and rewrap all use the substituted value.
- **Reachable consequence:** a deployment configured for version `2147483648`
  queries the external selector for version `2147483647`. It may fail with a
  misleading provider-unavailable error, or, if that different key exists,
  boot successfully and persist new credentials under a key version the
  operator did not activate. Multiple distinct configured versions collapse
  onto the same stored version.
- **Required testable correction:** reject an active version greater than
  `i32::MAX` in the existing `OperatorKeysConfig` validation boundary, then
  preserve the validated value exactly in `OperatorKeys` instead of
  saturating. Do not widen the SQL schema or add a new version abstraction.
  Add a focused configuration test proving `i32::MAX` is accepted and
  `i32::MAX + 1` is refused before boot or any key-provider request.

## Verification evidence and limits

- Independently run, all exit zero: `git diff --check` and the exact six-test
  `wyrd-server` unit selector covering selector redaction, Vault token-file
  permissions, nonblocking file reads, production HTTPS, and credential
  attachment ordering.
- The candidate records exit-zero results for the original task's SQL/shared/
  server/client/CLI/MCP/Python/TypeScript, codegen, boundary, format, and lint
  lanes, plus the focused production-boot and slow-rewrap Postgres selectors.
  This time-bounded reviewer did not rerun those database and broad lanes.
- The credentialed Slack/PagerDuty live smoke remains gated release evidence
  and was not run. Generated schemas/declarations were traced to their owners
  and recorded codegen/typecheck proof rather than reviewed line by line.
- These limits do not cause the finding: the out-of-range conversion is a
  direct reachable source path and has no rejecting validation or closure
  test.

## Overall result

**FAIL**

The cumulative candidate closes all nine prior findings and otherwise meets
the original task, but `TREV-R2-001` leaves exact key-version selection
incorrect at the configuration boundary.
