---
id: SPEC-verification-closeout
revision: 2
status: approved
approved: 2026-10-08
---

# Verification closeout

## Objective

Close the remaining TASK-017 product gaps with one principal-centered access
model, one non-blocking path for every write to Scribe, and one canonical
support-desk journey that proves the complete workflow in Rust, Python, and
TypeScript.

This specification consolidates the previously proposed TASK-017 R3 and R4
outcomes and the approved R3A Scribe decision. It intentionally produces only
three implementation tasks: R3 and R3A may execute in parallel; R4 follows
both.

## Required behavior

### REQ-001 — Principal role assignments

Role assignments address a tenant principal by `principal_id`. Administrators
can discover assignable users, services, and agents; list their assignments;
and idempotently grant or revoke a direct assignment. IdP synchronization owns
only IdP-sourced user assignments and never removes direct assignments.

Assignment writes require tenant administration, reads retain the existing
principal-administration permission, every authorization decision is audited,
and unknown, deleted, platform, system, tenant-admin, or foreign-tenant
principals receive the same non-enumerating not-found response.

### REQ-002 — Four built-in roles

Wyrd ships exactly `admin`, `editor`, `workload`, and `viewer`, ordered as
`viewer` ⊂ `workload` ⊂ `editor` ⊂ `admin`. Card-bound Service and Agent
principals initially receive `workload`. Retired built-in role names receive no
alias. Existing custom roles remain tenant-owned.

### REQ-003 — Frictionless authenticated operation

An unbound user or tenant administrator may attribute observations and
verifications to any registered observation-target Card in the same tenant.
A Card-bound principal remains limited to its declared Card scope. The same
rule applies to Bifrost ingest, OTLP, and verification observations.

The public Rust, Python, and TypeScript clients expose principal discovery and
role assignment consistently. Stock gateway and OTLP clients can obtain a
fresh access token for each request through the stock client's supported auth
hook. Local development works with the setup admin key; the enterprise flow
works with a saved user login. User journeys do not call the test-only Bifrost
publication flush hook.

### REQ-004 — One non-blocking Scribe outbox

All records whose destination is Scribe use one server-owned producer/consumer
path:

```rust
pub type ScribeOutbox = Outbox<ScribeSink>;
```

Audit records, gateway captures, queued Eval and Drift results, and realtime
Verifier results enter `ScribeOutbox` as logical `ScribeWrite` values.
`ScribeOutbox` retains the existing `Outbox<S>` behavior: `stage()` only places
an item on its in-memory channel, the background worker continuously groups
accepted items by tenant, and orderly shutdown drains accepted work to its
deadline.

`ScribeSink` is the batching boundary. It groups the tenant slice by destination,
encodes Arrow batches, assigns deterministic batch identities, and pushes each
batch to the in-process Scribe when local or to a Scribe peer otherwise. The
existing Scribe ingress remains the only owner of item admission, byte limits,
memory accounting, WAL persistence, and acknowledgement. `ScribeOutbox` adds no
second Scribe admission or memory reservation.

Domain owners remain responsible for validated domain values and attribution;
they do not encode or deliver Arrow batches. A producer never waits for Scribe
acknowledgement, and a Scribe failure never changes the originating permission
decision, gateway call, run settlement, or verdict.

This path deliberately has no PostgreSQL intermediary. Work buffered only in
`ScribeOutbox` may be lost on abrupt process death or after the orderly-shutdown
deadline. Observable rejection and shutdown loss are logged and counted;
abrupt process loss is inherently not observable by that process. Once Scribe
accepts a batch, Scribe's existing WAL and storage lifecycle own durability. A
future durable outbox is a separate change if operational evidence requires one.

### REQ-005 — Observation runs remain separate

Observation acknowledgement continues to enqueue verification work through:

```rust
pub type ObservationRunOutbox = Outbox<ObservationRunSink>;
```

`ObservationRunSink` writes `wyrd.verifier_runs`. It never writes to Scribe and
is not merged with `ScribeOutbox`.

### REQ-006 — Verification activation and recorded results

A verification binding has one Verifier, optional activation, and optional
failure Operators. A binding without activation runs only when explicitly
invoked. An unbound writer's Eval observation activates matching
observation-ready bindings for the observation's subject.

Every completed queued or direct judgment produces its canonical result and
detail batches through `ScribeOutbox`. Direct execution still creates no
durable verification run, performs no sampling, and dispatches no Operator.
Its response is built independently of the non-blocking result write.

A queued run settles after its logical result has been staged in memory. It
does not retain encoded result bytes in PostgreSQL and does not wait for Scribe
acknowledgement. The accepted best-effort contract therefore permits a settled
run's analytical result to be absent after abrupt process loss.

Result rows use the application Run as their managed `run_id` when one exists:
the observed Run for continuous Eval and the caller's Run for direct
execution. Scheduled results without an application Run use null. The Verifier
Card remains the managed Card identity, and `result_id` identifies the verdict.

### REQ-007 — Declared tables and request correlation

A Service may declare supported Bifrost dataset tables. Registration validates
and ensures them before the Service becomes active, refuses incompatible
schemas, and never drops them when the Service is removed.

Gateway ingress accepts the paired Wyrd Run and Card correlation headers,
authorizes the Card attribution before the upstream call, and records both on
the captured call. Spans, observations, gateway calls, and verification results
for one request can therefore be joined by the application Run.

### REQ-008 — Gateway-owned agent and judge execution

LLM judge calls use the existing gateway under the initiating principal's
authority, admission, accounting, capture, and audit behavior. No separate
judge-provider configuration or fallback principal exists.

A Run over an Agent can invoke that Agent once through the gateway with string
variables and return its final text. Tool-calling and structured output are not
part of this change.

### REQ-009 — Telemetry and Run scope

Each SDK can install Wyrd telemetry once, export OTLP/HTTP using refreshed
credentials, and scope work to a Run so emitted spans carry the Wyrd Run and
Card correlation attributes. Shutdown flushes the provider. Wyrd refuses to
replace a foreign provider.

### REQ-010 — Canonical support-desk journey

One checked-in support-desk example and one matching journey per first-class
SDK prove the complete workflow against a real server: authenticate, deploy a
Service and Agent, ensure its declared table, invoke through the gateway,
record correlated evidence, run continuous and realtime verification, and
query the joined evidence through MCP. The journey uses only public surfaces
and no test-only publication hook.

## Locked decision ledger

These decisions are part of the approved behavior. Implementers must not
reopen or silently simplify them.

### R3 principal and local-flow decisions

1. Role assignments are subresources of `principal_id`; the Card-addressed
   `POST /v1/auth/grant-role` contract is removed without an alias.
2. Assignable principals are active or suspended tenant `user`, `service`, and
   `agent` principals. Tenant administrators, system/platform principals,
   deleted principals, and foreign-tenant principals are not enumerable.
3. The server resolves principal kind from the identifier. Callers never send
   it when assigning a Role.
4. Principal Role reads use `GET`; direct grant and revoke use idempotent `PUT`
   and `DELETE`. Both writes return the resulting assignments and `changed`.
5. Role writes require `*`; reads retain `service_accounts:write`. This change
   adds no “grant only what you hold” rule.
6. User assignments are sourced `idp` or `direct`. Login replaces only `idp`
   rows; revoke removes only `direct`; effective Roles are the union. Service
   and Agent assignments remain direct.
7. Assignment changes affect the next token. Already-issued tokens retain
   their current bounded lifetime.
8. `GET /v1/principals` provides exact-match discovery and UUIDv7 keyset
   paging. Key issuance returns the projected principal identifier.
9. Public CLI and Rust/Python/TypeScript `Principals` surfaces expose the same
   list, role-list, grant, and revoke behavior. No MCP tool is added.
10. The four built-in Roles and exact permission intent are:

    | Role | Permissions |
    |---|---|
    | `viewer` | all Card, artifact, audit, Operator, gateway, gateway-payload, Bifrost-table, and Bifrost-query reads |
    | `workload` | `viewer` plus Bifrost table/record writes, evaluation, Workflow execution, Trigger writes, Operator invocation, and gateway invocation |
    | `editor` | `workload` plus Card/artifact authoring, policy lock, and Service installation |
    | `admin` | `*` |

    Only `admin` holds credential, user, identity-connection, Operator-secret,
    and gateway-configuration administration. Platform-plane and engine-peer
    permissions belong to no tenant Role.
11. An identity connection with omitted `default_roles` defaults to `viewer`;
    an explicit empty list remains empty. Existing stored connections do not
    change.
12. A Service or Agent principal receives `workload` only on first projection.
    Reapplying the Card does not restore a revoked assignment.
13. An unbound principal's signed Card scope explicitly represents “any
    registered observation-target Card in this tenant”; it is not an empty
    bounded scope. Resolved writes stamp the Card UID.
14. Client token adapters use `WyrdClient.access_token()` on every request and
    never maintain another cache, retry policy, background refresher, or token
    log. OTLP uses `x-wyrd-access-token`; gateway clients use the standard
    bearer header.
15. Python supplies OTLP/HTTP exporter factories and `httpx.Auth`; TypeScript
    supplies OTLP exporter factories and a `fetch` adapter; Rust supplies
    OTLP/HTTP exporter factories behind the existing SDK feature boundary and
    no new gateway adapter.
16. Local development uses only the setup admin key. The enterprise journey
    uses a saved login and proves operation beyond the original access-token
    lifetime. Neither flow issues a Card key or calls `flush_bifrost`.

The principal HTTP contract is fixed:

| Method and path | Gate | Outcome |
|---|---|---|
| `GET /v1/principals?kind=&email=&name=&limit=&after=` | `service_accounts:write` | keyset-paged assignable principals |
| `GET /v1/principals/{principal_id}/roles` | `service_accounts:write` | assignments ordered by Role and source |
| `PUT /v1/principals/{principal_id}/roles/{role}` | `*` | idempotently add a direct assignment |
| `DELETE /v1/principals/{principal_id}/roles/{role}` | `*` | idempotently remove a direct assignment |

The wire contract includes `RoleSource`, `RoleAssignment`, `PrincipalRoles`,
`RoleAssignmentChange`, `PrincipalSummary`, and `PrincipalPage` as described by
those outcomes. Invalid identifiers, Roles, filters, and pagination use the
existing validation error; denied access uses the existing RBAC error; an
unavailable or hidden principal uses the existing non-enumerating not-found
error.

### R3A Scribe decisions

1. `ScribeOutbox` is the existing in-process `Outbox<S>` specialized with one
   `ScribeSink`; it is not a new persistence product.
2. `ScribeWrite` is the closed logical input set for audit events, gateway
   captures, and verification results. It is not an Arrow batch and is not a
   generic destination envelope.
3. The existing outbox worker continuously drains its in-memory channel and
   supplies each tenant's pending logical writes to `ScribeSink`. Each closed
   variant maps to a fixed destination set; `ScribeSink` groups the resulting
   rows by destination and encodes complete Arrow batches. No producer performs
   one Scribe call per analytical row.
4. `ScribeSink` owns deterministic batch identity and both in-process and peer
   routing. A retry of a failed sink write must use the identical logical slice
   so every destination batch retains the same identity; later arrivals wait
   for a subsequent sink write.
5. Existing Scribe ingress owns capacity admission and memory accounting. The
   outbox and sink acquire no Scribe memory lease and add no capacity setting.
6. Retryable Scribe failures use the existing outbox retry/backoff lifecycle.
   A terminal Scribe rejection is logged, counted, and consumed so one invalid
   write cannot block all later writes for that tenant.
7. `vala.audit_staging`, its chain head/publication progress, `AuditPublisher`,
   and the gapless audit hash-chain fields are retired. Retained audit history
   remains `vala.system.audit_log`, containing the authorization-decision
   content without `seq`, `entry_hash`, or `prev_hash`.
8. `wyrd.verifier_run_results` and runner-owned Scribe publication are retired.
   A queued run settles once its result is staged on `ScribeOutbox`.
9. Gateway capture no longer owns a deadline-bound delivery loop. Realtime
   result recording uses the same best-effort path and never changes the
   returned Judgment.
10. Observation-run enqueueing remains a second `Outbox<S>` instance because
   its sink schedules PostgreSQL work rather than writing Scribe evidence.
11. No PostgreSQL Scribe queue, consumer poller, claim size, lease,
    `SKIP LOCKED`, new publication concurrency, or second admission governor is
    introduced. Existing unconsumed staging rows may be discarded when the
    retired tables are removed; already retained Scribe history is untouched.
12. `ScribeOutbox` retains the existing unbounded in-memory queue. This change
    adds no queue-capacity setting; a prolonged retryable Scribe outage can grow
    producer-process memory until delivery resumes or the process stops.

### R4 verification and journey decisions

1. The checked-in example lives at `examples/support-desk` and exports the
   same `deploy`, `serve`, `wait_for_verdicts`, and `explain` workflow in each
   language. SDK journeys import that implementation instead of copying it.
2. `ServiceSpec.tables` contains dataset name plus JSON Schema in the existing
   supported subset. Tables are validated and ensured eagerly before Card
   activation; conflicts refuse before any write; removal never drops tables.
3. `VerificationBinding.runs_on` is optional. Absence means explicit-only
   execution, not an implicit default Trigger.
4. A direct Judgment uses `execution_id` as stored `result_id`, stores summary
   plus implementation-specific details, and creates no `verifier_runs` row or
   Operator dispatch. Direct Drift uses its execution interval as its window.
5. Managed result `run_id` is the application Run, not the Verifier run.
   Managed `card_uid` remains the Verifier; `subject_card_uid` remains the
   judged Card. Scheduled Drift without an application Run stores null. No new
   analytical column or table migration is introduced for this change.
6. `ExecuteVerificationRequest` gains optional `run_id`; SDK `observe.verify`
   and MCP `verification.execute` project that same field.
7. Gateway headers `wyrd-run-id` and `wyrd-card-ref` must appear together,
   validate before upstream IO, and are reserved transport headers for every
   supported gateway dialect.
8. Every LLM judge call uses `GatewayInvocation::decide` exactly once as the
   evidence-producing principal. Realtime uses the request caller; continuous
   evaluation uses the observation writer. There is no SYSTEM fallback or
   separate provider registry.
9. Queued judging may cache resolved principal authority by tenant/principal,
   but never beyond access-token TTL; cache hits perform no IO, missing or
   inactive principals are not cached, and normal API JWT verification does
   not use this cache.
10. `run.invoke` supports one tool-free Agent, string variables, gateway
    authorization, Run/Card correlation, and final text. Non-Agent Runs and
    Agents with tools refuse before upstream IO.
11. `state.start_telemetry` installs tracing once using the state's refreshing
    client; a second call on the same state is idempotent; a foreign provider
    receives `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS`; shutdown flushes only the
    provider Wyrd installed.
12. Run scope uses native context propagation: Python `with run`, TypeScript
    `run.scope`, and Rust `run.scope`. Only tracing is installed by this helper.
13. `explain` calls only `bifrost.query` over MCP with a fresh token. No SDK MCP
    helper is introduced.
14. `deploy` configures metadata capture, verifies the Prompt model is deployed,
    and names the missing model plus remediation when it is not.

The public R4 contract includes:

- `ServiceTable { name, schema }` and `ServiceSpec.tables`;
- optional `VerificationBinding.runs_on`;
- optional `ExecuteVerificationRequest.run_id`;
- paired gateway Run/Card headers and an optional gateway-call subject;
- Python/TypeScript/Rust `Run.invoke`, telemetry setup, and Run scope with
  idiomatic language signatures;
- the existing stable errors for schema conflict/parse, RBAC, missing Scribe
  role, malformed correlation, unresolved/out-of-scope Card, and invalid Agent
  invocation, plus the one telemetry-provider conflict error above.

The canonical workload is exactly 100 requests: ten contain “refund.” Expected
evidence is 100 continuous `answer-quality` passes, 90 realtime
`no-refund-promise` passes, and ten realtime failures. The journey explains one
passing and one failing request and proves the observation, service record,
span, gateway call, and both verdicts join by application Run.

## Invariants

- **INV-001 — Tenant and authorization isolation.** Every public and internal
  path preserves tenant isolation, existing permission checks, audit coverage,
  and non-enumerating identity failures.
- **INV-002 — Non-blocking Scribe writes.** Audit, capture, and result
  producers never wait for Scribe and never fail their originating operation
  because Scribe is unavailable.
- **INV-003 — Existing batching model.** `ScribeOutbox` uses the existing
  per-tenant `OutboxSink::write` contract, and `ScribeSink` converts each
  tenant slice into destination-specific Arrow batches. This change adds no
  claim protocol, polling publisher, durable staging table, or new concurrency
  mechanism.
- **INV-004 — One delivery owner.** Only `ScribeSink` chooses local versus peer
  Scribe and owns Scribe delivery retry behavior.
- **INV-005 — Stable write identity.** A retry reuses the same Scribe batch
  identity so Scribe's existing deduplication remains effective.
- **INV-006 — Observation work is not evidence delivery.** Observation-run
  enqueueing remains independently drainable and cannot be delayed by Scribe
  delivery.
- **INV-007 — One admission owner.** Only existing Scribe ingress admits batch
  items and bytes. The producer outbox performs no Scribe capacity reservation.
- **INV-008 — Explicit best effort.** Before Scribe acknowledgement, audit,
  capture, and verification-result writes are process-memory state and may be
  lost on abrupt process death without changing their originating operation.

## Scope and non-goals

Included:

- the R3 principal, role, authentication-adapter, Card-attribution, local-flow,
  and public-client closeout;
- the R3A shared Scribe outbox and removal of producer-specific Scribe delivery;
- retirement of audit and verification-result PostgreSQL staging and the audit
  hash-chain-only analytical fields;
- the R4 verification, correlation, gateway, telemetry, declared-table, and
  canonical support-desk outcomes.

Excluded:

- a new durable generic Scribe staging system, claims, leases, polling, or
  publication concurrency;
- a second memory governor, outbox capacity setting, Scribe admission policy,
  or producer-specific delivery guarantee;
- new MCP tools or an SDK MCP abstraction;
- Workflow-step correlation;
- Agent tool-calling or structured output;
- metrics or log-provider setup in the telemetry helper;
- unrelated table evolution, table dropping, layout, or compaction controls;
- compatibility aliases for retired routes, roles, or behavior;
- UI work.

## Acceptance criteria

- **AC-001.** Public principal APIs and all three SDKs prove discovery,
  idempotent direct grant/revoke, IdP/direct provenance, tenant isolation, and
  the four built-in roles.
- **AC-002.** Local admin-key and saved-login journeys can register, invoke,
  observe, verify, export telemetry, and query without a Card-scoped API key or
  test-only Bifrost flush.
- **AC-003.** Focused server tests prove audit, gateway capture, queued
  Eval/Drift results, and realtime results all enter `ScribeOutbox`; its sink
  batches logical writes and delivers them through local and peer routes;
  retryable failure reuses the identical batch identity; terminal rejection
  cannot poison later tenant writes; existing Scribe admission remains the
  only capacity owner; observation-run enqueueing remains separate.
- **AC-004.** Focused server tests prove declared tables, realtime-only
  bindings, unbound-writer Eval activation, application-Run result identity,
  stored realtime verdicts, gateway correlation, and gateway-owned judge
  calls.
- **AC-005.** Rust, Python, and TypeScript run the same support-desk journey and
  query correlated gateway, telemetry, observation, and result evidence
  through public MCP/SDK surfaces.
- **AC-006.** Generated contracts, dependency boundaries, tenant-isolation
  checks, docs, examples, formatting, and lints remain clean.

## Open decisions

None.

## Revision history

- **Revision 2 — 2026-10-08.** Approved the in-memory best-effort Scribe path:
  `ScribeSink` batches logical writes and pushes them through existing local or
  peer Scribe ingress, with no PostgreSQL intermediary or duplicate admission.
  Audit staging/hash-chain publication and queued-result staging are retired;
  observation-run scheduling remains PostgreSQL-backed.

- **Revision 1 — 2026-10-08.** Approved consolidation of TASK-017 R3, the
  shared R3A Scribe outbox, and TASK-017 R4 into a three-task closeout packet.
  The user required R3 and R3A to execute in parallel and R4 to follow both.

## Authority

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/wyrd-security-posture.md`
- `TESTING.md` — definitive Wyrd guide for test ergonomics, understandability,
  structure, ownership, and lane selection
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/implementation-execution.md`
- `architecture/references/languages/testing-workflows.md`
