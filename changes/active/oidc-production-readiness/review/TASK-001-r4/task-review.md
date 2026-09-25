# TASK-001 r4 task implementation review

## Immutable subject

- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `c2787b37d456cd2eeeee01e04a9f8bbfdf8866e5`
- Candidate tree: `e389f56318a664920b5ebead0aee45d20fda6b23`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Prior remediation tasks: `TASK-001-R1-production-readiness-gaps.md`,
  `TASK-001-R2-remaining-production-readiness-gaps.md`, and
  `TASK-001-R3-production-readiness-gaps.md`

The candidate commit and tree matched the supplied immutable subject before
and after inspection. This review covered the complete base-to-candidate diff,
the original task, all three prior verdicts and validated finding ledgers, all
three remediation tasks, current source and tests, and the task's recorded
verification evidence. The repository has no `.codegraph/` directory, so
caller tracing used `rg`, the cumulative diff, and complete function bodies.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001 task slice: OIDC is optional; connectionless startup, tenant provisioning, existing credentials, and machine authentication require no IdP or OIDC-specific secret | `HumanConnections` represents absence as no Active row; `ServerAuth` installs the durable owner without requiring a public origin or sealing key; keyless boot refuses only when ciphertext actually exists (`wyrd-server/src/boot/mod.rs:1508-1551`) | Committed keyless-boot tests and the task record name green identity and platform journeys; long lanes were not rerun in this review | PASS |
| REQ-002: one Active and at most one Candidate per tenant; same issuer remains tenant-isolated | Partial unique indexes and forced RLS in `20260925000000_auth_human_connections.sql:130-141`; every lifecycle mutation takes the tenant slot lock through `HumanConnections::begin_locked` (`connections.rs:632-648`) | `tenant_connection_admin_journey` covers two tenants and same issuer; `tenant_connection_rotation_journey` covers competing activation; `check:tenant-isolation` is recorded green | PASS |
| REQ-003: authorized create/replace, redacted inspect, test, activate, deactivate, and remove through the headless API, with durable cross-replica visibility | Six typed routes are registered in `components/admin/identity.rs:37-46`; handlers derive the tenant only from `Caller`; `HumanConnections` performs durable per-request reads/writes; PUT is the candidate/secret/map rotation path | Admin and rotation journeys exercise all lifecycle operations, wrong-tenant refusal, and two-replica visibility; served OpenAPI assertions cover the six operations | PASS |
| REQ-003 authorization: all six operations require `identity_connections:write`; `service_accounts:write` alone does not confer it | `decide` uses only `Permission::identity_connections_write` (`identity.rs:60-79`); built-in-role source grants it only through the `admin` wildcard and keeps `runtime_admin` at `service_accounts:write` (`builtin_roles.rs:52-92,120-133`) | Admin journey explicitly refuses a runtime-admin token and asserts its denied audit decision | PASS |
| REQ-004: callback is exactly configured public origin plus `/auth/callback`, is read-only, and is used by candidate proof and real login independent of request headers | `HumanConnections::new` derives one callback (`connections.rs:181-200`); views expose only that value; probes use `require_callback`; `begin_login` uses the same callback in authorization URL and state (`login.rs:131-171`) | Admin journey asserts the displayed callback; focused login coverage changes request Host/scheme without changing redirect URI; OpenAPI exposes the callback contract | PASS |
| REQ-004: issuer/discovery/JWKS/callback/client authentication are qualified; audience derives from client ID; unsupported auth is refused | `probe_candidate` runs pinned discovery, usable-JWKS, exact state-matching callback, and exact `invalid_grant` client-auth checks (`connections.rs:352-396,687-814`); `HumanConnectionView` stores only `client_id`; `ConnectionInput` has no `PrivateKeyJwt` variant and gives that spelling a stable refusal (`human_connection.rs:172-258`) | Probe unit tests cover mixed/repeated callback arms and client-auth outcomes; journeys cover unsafe discovery and bad secret; schema test excludes `PrivateKeyJwt` | PASS |
| REQ-005: provider secrets enter only the authorized boundary, are sealed at rest, and never appear in views, errors, audit, logs, or generated artifacts | PUT authorizes before decoding/staging (`identity.rs:180-195`); `SecretBearer` is converted and sealed by `HumanConnections::stage`; `HumanConnectionView` has no secret or envelope metadata; handlers use `skip_all`; public errors are redacted | Admin journey inspects response, ciphertext, and audit; served OpenAPI asserts the view has no secret property; codegen is recorded green | PASS |
| REQ-005: `Public` carries no secret; secret methods require nonempty secret material; a sealing key is required only while ciphertext exists | Cross-field validation distinguishes presence from content (`human_connection.rs:226-257`); the table check enforces the live-row invariant (`20260925000000_auth_human_connections.sql:119-128`); keyless rewrap scans all three stores and fails boot on any ciphertext | Contract unit cases cover omitted/present/empty combinations; keyless boot test covers empty and secret-bearing stores | PASS |
| REQ-005: sealing-key rotation is versioned, retains old keys through rewrap, proves a post-writer zero-reference pass, and permits K2-only serving | `SealingKeyring` embeds stable key IDs and supports retained-key open/rewrap; `SealedSecretRewrap` walks tenant connection, workload issuer, and platform stores with compare-and-swap and reports remaining rows (`wyrd-auth/src/sealing.rs:48-154`) | Rotation journey exercises K1 writer, K2+K1 rewrap, late K1 write, final pass after K1 writer shutdown, and K2-only login; operator documentation states the same ordering | PASS |
| REQ-014 task slice: a tested replacement is activated atomically only while its exact 15-minute revision stamp remains current and a recovery route exists | Database-derived stamp and promotion predicates are in `human_connections.rs:76-115`; activation checks candidate, stamp, and recovery key under the slot lock before retiring/promoting in one transaction (`connections.rs:437-513`) | Rotation journey covers failed qualification, stale/current revisions, competing activation, underprivileged/malformed recovery keys, and successful replacement | PASS |
| REQ-016 task slice: replacement, deactivation, or removal immediately blocks new login and refresh renewal through the old connection on every replica | Login reads the durable Active row each time; login state and refresh rows carry exact connection id/revision; session issuance checks that binding under the same slot lock; lifecycle changes serialize on that lock; removal tombstones and wipes the secret (`connections.rs:515-604`; `refresh.rs:109-175`) | Admin/rotation journeys cover login cutoff, cross-replica cutoff, tombstoning, and old-session refresh refusal; focused refresh tests cover binding propagation and inactive/unbound refusal | PASS |
| REQ-017: each permission decision is canonically and transactionally audited; required audit failure cannot establish a mutation | Handlers evaluate through `decide`; mutations append the bearer decision on their tenant transaction; test performs a second post-network evaluation; activation appends the independently evaluated recovery principal decision before promotion (`connections.rs:445-504,925-988`) | Journeys assert allowed/denied attribution, no fabricated row for unresolved recovery material, and rollback when bearer or recovery audit append fails | PASS |
| Provider IO obeys INV-004 and repository SSRF authority: production TLS, bounded DNS, address rejection, pinning, no ambient proxy/redirect, bounded decoded bodies | `ScreenedHttp::client_for` screens schemes, wraps the single lookup in the fixed timeout, rejects any disallowed answer, pins the returned addresses, disables redirects/proxies, and sets a fetch timeout (`screening.rs:123-263`); shared body reader caps decoded bytes | Screening tests cover address classes, mapped metadata address, proxy bypass prevention, scheme policy, stalled lookup, chunked/declared/gzip body ceilings; journeys cover unsafe discovery | PASS |
| Legacy Human trust moves to the sole human-connection store without arbitrary selection, unsafe grants, or broken workload bindings | Migration preflight refuses multiple Human issuers, default roles, audience/client mismatch, unsupported auth, and missing required secret; valid Human configuration is copied to Active; referenced issuer rows become Workload and unreferenced Human rows are removed (`20260925000000_auth_human_connections.sql:30-175`) | `pg_tests::human_connection_upgrade_preflight` uses isolated pre-migration databases for valid/bound, unsafe-default-role, and multiple-Human cases and asserts rollback/preservation; SQL lane is recorded green | PASS |
| Legacy human refresh state does not fabricate connection provenance | Migration clears in-flight state and leaves pre-existing refresh families unbound (`20260925000000_auth_human_connections.sql:177-205`); refresh rejects unbound human rows (`refresh.rs:136-147`) | Migration case asserts user and machine rows remain live/unbound and the human row cannot renew; focused refresh tests prove new bound rows rotate | PASS |
| Old Human trusted-issuer HTTP/CLI/boot paths cannot acknowledge trust ignored by login; Workload behavior remains | Shared `refuse_human_trusted_issuer` returns `HUMAN_CONNECTION_REQUIRED`; admin and CLI call it; boot redirects/refuses Human seeding through the durable connection contract; the migration adds a workload-only check | Admin journey exercises old HTTP/CLI Human refusal and successful Workload authoring; existing workload identity paths remain in the cumulative tests | PASS |
| Public contracts, stable errors, served OpenAPI, generated projection, CLI refusal, and operator docs agree (AC-009 task slice) | Typed structs and `WyrdError` catalog back handlers; utoipa registrations mount with the routes; TS error codes and four authentication docs are updated in the cumulative diff | OpenAPI integration, codegen, docs, format, and lint lanes are recorded green; static inspection confirms their source assertions | PASS WITH VERIFICATION LIMIT |
| Exact focused selectors and the minimal-feature identity journey remain credible | `mise.toml` supports `WYRD_IDENTITY_TARGET=server` and exact `WYRD_IDENTITY_FILTER`, counts exactly one match, and does not use `--all-features` in list/run commands | Task evidence records both filtered journeys and the unfiltered 23-test identity lane green | PASS WITH VERIFICATION LIMIT |
| Non-goals and scope: no UI delivery, hosted signup, commercial stub, second human trust store, `PrivateKeyJwt`, per-replica tenant secret environment, or compatibility surface | Complete cumulative diff keeps UI work in TASK-003, workload trust separate, and human login reading only `auth_human_connections`; no prohibited product hook or auth method was added | Static complete-diff inspection | PASS |

## Prior-finding closure

| Prior finding(s) | Closure in the cumulative candidate | Result |
|---|---|---|
| `FIND-TASK-001-1`, `-2` | Exact one-arm/state callback qualification and exact OAuth `invalid_grant` client-auth qualification are implemented and unit-covered | CLOSED |
| `FIND-TASK-001-3`, `-18`, `-20` | One deployment callback is used by setup/probe/live login; production scheme policy applies to server fetches and the freshly discovered browser destination | CLOSED |
| `FIND-TASK-001-4`, `-19`, `-23` | Shared provider IO is proxy-free, redirect-free, address-pinned, decoded-body-bounded, and DNS-deadline-bounded | CLOSED |
| `FIND-TASK-001-5` | Login/refresh provenance binds the exact connection revision; legacy rows remain unbound and cannot renew | CLOSED |
| `FIND-TASK-001-6`, `-14` | Connection owners and the issuer resolver acquire `TenantConn` through `WyrdPostgres`; no task-owned runtime path propagates a raw pool | CLOSED |
| `FIND-TASK-001-7`, `-22` | Post-probe stamping re-evaluates the bearer permission and activation separately audits the recovery principal's real permission decision | CLOSED |
| `FIND-TASK-001-8`, `-9`, `-16`, `-21` | Required item rustdoc/import placement is present at the previously identified locations, including `bounded_get`'s `# Errors` | CLOSED |
| `FIND-TASK-001-10` | Identity journey list/run commands use minimal features and exact nonzero selection checks | CLOSED |
| `FIND-TASK-001-11`, `-12`, `-17` | Keyless boot scans stored ciphertext; rotation has a post-writer final pass; active/retained key files use the restrictive bounded secret loader | CLOSED |
| `FIND-TASK-001-13` | Public rejects every present secret and secret methods reject absent/empty values | CLOSED |
| `FIND-TASK-001-15` | Candidate PUT keeps authorization before semantic interpretation while decoding raw bounded bytes through the typed contract owner | CLOSED |

`FIND-TASK-001-1` through `FIND-TASK-001-23` are closed at their validated
correction boundaries. The previously rejected host-selected callback-tenant
proposal is not revived here: it is unchanged from the base, outside this
task's declared requirement set, and requires the separate callback-routing
decision already identified by the prior validated ledger.

## Proposed findings

None. Static inspection found no reachable task-scoped MISSING, INCORRECT,
DRIFT, VIOLATION, or REGRESSION finding.

## Verification limits

- Static review only, as assigned. No Cargo-backed or `mise` Cargo lane was run.
- The task artifact records green focused admin/rotation journeys, the unfiltered
  identity lane, migration selector, principals/OpenAPI integration, SQL and
  platform journeys, codegen, docs, formatting, lints, tenant/pool/client/PyO3/
  unwrap/Clippy boundaries, and `git diff --check`; this reviewer inspected the
  commands and committed assertions but did not reproduce those results.
- Controlled Okta and Entra production qualification belongs to AC-008 and is
  not delivered or claimed by TASK-001; Keycloak/Dex and mock-provider evidence
  proves this task's administration and failure contracts, not a commercial-IdP
  support claim.
- The two-replica proof uses independent in-process server instances sharing
  repository-managed Postgres, not a deployed multi-pod topology.

## Overall result

**PASS**

The cumulative candidate satisfies the original TASK-001 obligations and all
three remediation tasks exactly within the available static evidence. There
are no proposed findings for Wave 2 to retain.
