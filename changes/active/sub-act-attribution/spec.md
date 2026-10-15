---
id: SPEC-sub-act-attribution
revision: 4
status: approved
---

# Consistent subject and actor attribution for Wyrd work

## Objective and user value

Today Wyrd records attribution four different ways. Bifrost rows carry one
managed `principal_id` (the row writer). Gateway calls add
`caller_principal_id`. Verification results and runs add
`requested_by_principal_id`. Audit uses `audit_principal_id`, and
registration tables use `created_by` or `actor`. Gateway calls and verification
results each store a principal twice with different meanings. Internal work
(scheduled and observation-triggered verification, and the judge calls inside
it) either records SYSTEM and loses who it was for, or records a real principal
as if it had acted.

After this change every table that records work carries the same two columns,
using RFC 8693's subject/current-actor distinction. Wyrd's delegated token already
has that shape: top-level `principal` plus an `act` chain. Every decision, cost
and audit row traces back to the service the work was for. The row also says
plainly whether a principal or Wyrd itself did the work.

RFC 8693 defines delegated identity and actor history. Billing, permission
intersection, SYSTEM execution, non-null storage, and the column names here
are Wyrd policy, not requirements imposed by that RFC.

## Approval

Revision 4, including realtime propagation and further delegation, was approved
by the human owner on 2026-10-14: "$wyrd-plan approved. create a task for this
work". Implementation must reuse existing owners and patterns, remove
superseded logic, and avoid unnecessary complexity.

Revision 1 was approved by the human owner on 2026-10-14: "I approve the design. there is no
need for migrations. nothing has shipped".

Revision 2 follows owner direction on 2026-10-14 that this change lands after
SPEC-verify-attribution r2 ("sub-act lands after this, so you will need to
update the sub-act spec"). It changes only what that ordering requires; see
"Relationship to verify-attribution".

Revision 3 settles the open Workflow-step decision and states that work a
request causes inside Wyrd keeps that request's originating `sub` and
authority. Approved by the human owner on 2026-10-14: "delegation should always
retain the originating sub … i agree and settle the spec".

## Definitions

- **`sub_principal_id`**: the principal the work was for. Permissions are
  checked against its accepted authority for caller-authorized work, and cost
  is billed to it. Existing narrow server-internal capabilities stay distinct.
- **`act_principal_id`**: the principal that did the work.
- **Accepted authority**: the server-verified effective permission snapshot,
  resource restrictions, tenant, subject, and originating delegation identity
  accepted for one operation. The attribution pair alone is not authority.

When a principal does its own work, the two are equal. When Wyrd does work in
the background, `act` is the tenant SYSTEM principal and `sub` is the principal
the work was for.

## Requirements

### REQ-001 One attribution pair

Every work-record table in the coverage inventory below carries `sub_principal_id` and
`act_principal_id`. Both are non-null on every row. The server stamps them, and
a client can never supply either one.

### REQ-002 Who is sub and who is act

| Work | `sub_principal_id` | `act_principal_id` |
|---|---|---|
| A principal calls the gateway | caller | caller |
| A principal calls verify directly | caller | caller |
| A delegated token (B acting for A) | A (top-level `principal`) | B (outermost `act`) |
| Manual binding run | caller | caller |
| Scheduled binding run | binding owner's principal | SYSTEM |
| Observation-triggered binding run | binding owner's principal | SYSTEM |
| Judge model call inside any Verifier run | the run's `sub` | SYSTEM |
| Model call made by a Workflow step | the run's captured caller (A under delegation) | the run's captured caller (B under delegation) |
| A client writes observations, spans, logs, metrics, or table rows | writer, or A under delegation | writer, or B under delegation |
| Server span copied to the tenant's `vala.traces.spans` (verify-attribution REQ-007) | the emitting operation's `sub` | the emitting operation's `act` (SYSTEM for judge execution) |
| Card, service account, API key, operator connection, or Bifrost table registration | caller, or A under delegation | caller, or B under delegation |
| Platform authorization decision | authenticated platform subject | authenticated platform actor, or subject for direct work |
| Authentication refusal with no verified identity | existing PLATFORM_AUDIT_PRINCIPAL sentinel | same sentinel |

- The binding owner's principal is the principal projected for the binding's
  `owner_card_uid` Service or Agent Card.
- The observation writer is never the `sub` of an observation-triggered run.
  It stays reachable through `source_record_id`.
- `sub` is carried, never re-derived. Work a request causes inside Wyrd, such
  as a Verifier's judge call or a Workflow step's model call, keeps the
  originating request's `sub` at every hop. A principal that calls verify is
  the `sub` of the judge's gateway call.
- A Workflow step's model call is the caller's own requested work, so its
  `act` is the captured caller, not SYSTEM.
- Outermost token `act` is the current actor; older nested actors remain
  history in audit detail. Preserve the existing verified-token chain order
  when projecting this value; do not reinterpret the oldest actor as current.
- Server-span attribution follows the operation that emits the span. A
  direct-request span carries A/B; a judge-operation span carries A/SYSTEM.
  Trace parentage never supplies or overrides either identity.

### REQ-003 Authorization and cost follow sub

- Every gateway call and every judge call checks the `gateway:invoke` permission
  of the call's `sub`. This holds whether the call came in through the gateway
  or was started inside Wyrd.
- A delegated external call keeps today's rule: its permissions are the
  intersection of the subject's and the actor's.
- Work a request causes inside Wyrd carries that request's authority. Each
  downstream call is checked against the originating `sub`'s permissions,
  intersected with the originating actor's when the request was delegated. A
  delegated request can never widen to `sub`'s full permissions on a later hop.
- SYSTEM as `act` adds no restriction and grants nothing. Work with no
  originating request (scheduled and observation-triggered runs) is authorized
  exactly as its `sub` would be.
- Gateway spend and admission accounting are charged to `sub`.
- SYSTEM is never a `sub`.
- Existing tokenless Verifier input reads retain their narrow server-owned
  capability and resource limits. Their attribution is run subject/SYSTEM;
  their authority is not inferred from that pair. This change neither grants
  the caller general access to Verifier inputs nor expands internal reads.

### REQ-008 Authority survives deferred execution

- Direct and delegated operations use the verified effective authority already
  produced by Wyrd authentication. Do not reconstruct subject-only authority
  from an ID or from the actor history. The existing delegated intersection
  remains authoritative; this change does not alter token issuance or token
  verification semantics.
- An accepted manual verification run retains that authority with its durable
  queue record, including originating actor identity and audit delegation
  history. Claim, retry, and process restart preserve it. The snapshot is
  server-only, bound to the tenant and run; it contains no bearer token, API
  key, refresh token, or provider credential and is never exposed in run views.
- Scheduled and observation-triggered runs resolve the binding owner's active
  principal and effective authority in the binding's tenant when the run is
  enqueued. The triggering observation's writer grants no authority. Missing
  or inactive owner/SYSTEM identities prevent acceptance with an observable
  binding execution failure; there is no fallback to SYSTEM as subject or to
  the observation writer. An accepted run freezes this resolved authority.
- Accepted work follows the existing Workflow snapshot lifetime rule: later
  grant edits or expiry of the initiating bearer token do not change that
  run's captured authority. They affect newly accepted operations. This is
  not a new global revocation mechanism or permission to reuse the snapshot
  outside its accepted operation. Existing cancellation remains available.
- Each downstream protected call still evaluates its required permission and
  resource against the accepted authority and audits the decision. Capturing
  authority does not pre-authorize every possible judge model or resource.
  Changing the execution actor to SYSTEM neither broadens nor erases the
  originating delegated restrictions.
- Workflow execution retains its current captured-authority lifetime and
  in-memory lifecycle; this change does not add durable Workflow execution.

### REQ-011 Realtime propagation and further delegation

For a realtime request made by B on behalf of A, the complete path is:

| Hop | Subject | Execution actor | Authority |
|---|---|---|---|
| Authenticated request → Verifier acceptance | A | B | Verified effective authority of the delegated request |
| Verifier → judge execution | A | Tenant SYSTEM | Same accepted authority |
| Judge → gateway | A | Tenant SYSTEM | Same authority, checked for `gateway:invoke` on the selected model |

For a direct request, replace B with A at acceptance. The subject remains A
throughout either path. The Verifier Card, model identity, provider credential,
gateway transport identity, and SYSTEM never replace A as subject. Gateway
cost/admission and permission-decision audit remain attributable to A; judge
gateway records carry A/SYSTEM and retain the original A/B delegation history
in audit detail.

Identity and effective authority travel together through existing trusted
server contexts, including asynchronous boundaries and authenticated internal
forwarding when used. Passing only A's ID and resolving A's full permissions
later is incorrect. An identity-bearing token or request header from an
untrusted caller cannot create this context. Missing trusted subject or
authority fails closed before protected work; never fall back to SYSTEM,
provider credentials, or ambient server permissions.

Permission composition uses the existing Wyrd evaluator. Delegated effective
authority is the subject/actor intersection, including existing resource and
token restrictions, not a union that permits either party to supply a missing
permission. Every later delegation may narrow the inherited authority and
must never widen it. Preserve already-applied restrictions; do not reconstruct
authority by evaluating historical `act` entries. Moving execution to SYSTEM
is an internal handoff, not a new delegation grant. No new delegation token,
permission algebra, or propagation service is required.

### REQ-009 Idempotency preserves both identities

- Caller-supplied idempotency keys for registration operations and manual
  verification are scoped by tenant, subject, current
  actor, and key, alongside any existing resource/operation discriminator.
  Same actor/different subject and same subject/different actor are independent
  operations, never replays of each other's work.
- Within that scope, an identical request reuses the accepted operation and
  its captured authority; a different request retains the existing conflict
  behavior. Authentication and admission to the replay/read occur before
  returning existing work. A replay never replaces or widens captured authority.
- Server-generated scheduled/observation occurrence deduplication retains its
  binding/source/occurrence identity. Sharing SYSTEM as actor does not merge
  independent runs or replace existing origin keys.
- Workflow run ownership and idempotency retain their existing subject-based
  semantics. Do not extend this schema cleanup into a new Workflow contract.
- Existing client registration-key derivation remains deterministic and
  compatible with its server scope. Sharing a generated key across actor
  contexts is safe because the server separates those contexts; no new client
  actor discovery or token parsing is required.

### REQ-004 Card columns

- `card_uid` is the Card a row is about. On verification tables that is the
  Card being verified.
- A row that references a second Card names it by its role:
  - `owner_card_uid`;
  - `verifier_card_uid`, which replaces `verifier_uid` and also replaces
    today's use of the managed `card_uid` for the Verifier on result tables.
- `subject_card_uid` no longer exists.
- A Workflow step's model call records `card_uid` = the Workflow Card and a
  null `run_id`. A Workflow run id is not an application Run; the step call
  correlates to its Workflow run through the trace and the
  `wyrd.workflow.run_id` span attribute.

### REQ-005 Schemas

A plain `principal_id` remains where it is the row's own identity or owner
key: `auth_refresh_tokens`, `platform.credentials`,
`platform.principal_grants`, `auth_login_state`, `auth_device_authorizations`
and `oracle_admission_blocks`, as well as the principal identity relation
`platform.principal_identities`. These are semantic categories, not a ban on
unrelated identity/foreign-key fields elsewhere. A legacy writer-attribution
`principal_id` never remains alongside the pair.
`auth_connection_test_state.tester_principal_id` is unchanged: it
names the principal holding the test, not the principal the work was for.

Bifrost managed columns, appended to every Observation-policy table:

```
run_id, card_uid, sub_principal_id, act_principal_id, wyrd_request_id, wyrd_ingested_at
```

Bifrost system tables:

| Table | Change |
|---|---|
| gateway calls | drop `caller_principal_id`; attribution is the managed pair |
| verification results | drop `requested_by_principal_id` and `subject_card_uid`; managed `card_uid` = Card verified; add `verifier_card_uid` |
| `vala.traces.spans` server-span rows | the managed pair replaces verify-attribution D6's single requesting-principal stamp |
| eval result items, drift result features | drop `subject_card_uid`; managed `card_uid` = Card verified; add `verifier_card_uid` |
| audit log | `audit_principal_id` → `sub_principal_id`; `principal_kind` → `sub_principal_kind`; add `act_principal_id`; `delegation_chain` stays in detail |

PostgreSQL tables:

| Table | Change |
|---|---|
| `wyrd.verifier_runs` | `requested_by_principal_id` → pair, both `NOT NULL`; `subject_card_uid` → `card_uid`; `verifier_uid` → `verifier_card_uid`; idempotency unique on `(data_tenant_id, sub_principal_id, act_principal_id, idempotency_key)`; retain accepted authority and the enqueuing trace context from verify-attribution D8 |
| `wyrd.verification_bindings` | `subject_card_uid` → `card_uid`; `verifier_uid` → `verifier_card_uid` |
| `wyrd.card_registration_operations` | `principal_id` → pair; unique on `(data_tenant_id, sub_principal_id, act_principal_id, idempotency_key)` |
| `wyrd.cards`, `wyrd.auth_service_accounts`, `wyrd.auth_api_keys`, `wyrd.operator_connections` | `created_by` → pair |
| `vala.bifrost_tables` | `actor` → pair |

Apply REQ-009 without changing Workflow run ownership.

### Coverage inventory and exclusions

The tables above, every Observation-policy table, and all audit decisions are
the required attribution inventory. Their create/update/read/query/export
paths, shared SQL helpers, HTTP and gRPC ingest, MCP, SDKs, examples, fixtures,
and documentation must agree. Creation attribution remains the original
creator's pair on later updates; it does not become last-modifier attribution.

An identity/ownership relation describes who an entity is or belongs to; it
does not acquire a work-attribution pair just because it contains a principal
foreign key. Likewise, Scribe/Forge maintenance, lease, publication, and
operational-lineage tables keep their existing operational identities. They
do not invent permission decisions or audit events.
`operator_dispatches` remains run-linked operational delivery state: its
attribution resolves through the run rather than duplicating the pair on each
lease/retry row. Operator permission checks and rechecks remain unchanged;
their audit decisions use the verified subject/actor convention.

Variant-policy payloads and schema-policy boundaries remain unchanged; the
managed pair is appended only where Observation policy already appends managed
columns. Where managed writer identity currently controls per-principal
sampling, use `sub` as the sampling identity. This aligns delegated traffic
with the subject's admission and cost accounting. Card authorization,
tenant filtering, sampling probability, and resource restrictions remain
independent checks.

Before implementation closure, record a source-backed disposition for every
remaining old attribution-name occurrence: replaced attribution, retained
identity/ownership key, operational lineage, or historical change record.
Unexpected work-record owners must be brought back to this specification;
an unbounded repository-wide rename is not an implementation decision.

### REQ-010 Audit identity and storage identity remain distinct

- Authenticated tenant and platform authorization decisions record the verified
  subject/actor pair, including denials. `sub_principal_kind` describes the
  subject, not the transport writer. Retain full delegation history in detail.
- Platform decisions remain in the reserved platform audit tenant. Their
  payload identities are platform principals; they are not rewritten into
  tenant principals to satisfy storage stamping.
- Audit records without a verified identity use the existing
  `PLATFORM_AUDIT_PRINCIPAL` Service sentinel for both fields, with existing
  failure detail indicating unresolved identity. Replace the nil-principal
  convention; never trust identity claims from a rejected token. The sentinel
  is an explicit unavailable-identity marker, not SYSTEM and not a principal
  whose permissions may authorize work.
- Existing internal transport sentinels for audit and gateway capture remain
  transport identities only. They never overwrite verified payload attribution.
  Tenant payload IDs resolve within their tenant; platform and unresolved
  sentinel audit records are explicit exceptions to tenant-principal lookup.
- Preserve the single non-blocking Scribe outbox path, allowed and denied
  decision coverage, and logged/counted audit loss. No new audit transport,
  synchronous persistence requirement, or permission for sentinels is added.

### REQ-006 Wire contracts and SDKs

- Public request, response and row types use the new names everywhere they
  expose these columns. This covers the gateway record, verification result
  and run views, audit detail and Bifrost managed-column constants.
- Rust, Python and TypeScript SDK typed rows, read paths, stubs and generated
  schemas match.
- `sub_principal_id` and `act_principal_id` are reserved Bifrost column names.
  A client frame that declares either one is rejected, as reserved managed
  columns are today.
- No old name survives as an alias.

### REQ-007 SYSTEM always exists

- Every tenant has an active SYSTEM principal, so `act_principal_id` is never
  NULL for internal work.
- Resolve it through server-owned tenant identity; never mint a SYSTEM token.
- `wyrd-dev-fixtures` tenant seeding creates it, like production provisioning
  does.

## Invariants

- **INV-001** `sub_principal_id` and `act_principal_id` are server-derived from
  verified identity, the binding owner, or the explicit unresolved-audit
  sentinel in REQ-010; never from unverified client input, headers, or row values.
- **INV-002** SYSTEM never appears as `sub`. SYSTEM appears as `act` only for
  Wyrd-owned execution, including judge work inside a caller-requested run.
- **INV-003** Tenant isolation is unchanged. The binding owner's principal and
  SYSTEM resolve within the run's tenant.
- **INV-004** Origin references (`run_id`, `card_uid`, `owner_card_uid`,
  `binding_id`, `source_record_id`, trigger) keep their meaning and are never
  copied into principal columns.
- **INV-005** Trace context stays observability only. It never sets `sub` or
  `act`.
- **INV-006** Attribution IDs are not permission grants. A trusted pair cannot
  substitute for the accepted effective authority or Card/resource checks.
- **INV-007** Creation attribution and accepted run authority are immutable
  for that entity/operation; reads and retries cannot rewrite either.

## Expensive-to-reverse decisions

- **No migrations.** Nothing has shipped, so each change edits the migration or
  Iceberg schema declaration that defines the column, in place:
  - `20261014000001_verifier_run_requester_every_origin.sql` is deleted. By
    the time this change lands it also carries verify-attribution's D8
    enqueuing trace-context column; that column moves into the defining
    `wyrd.verifier_runs` `CREATE TABLE` with its meaning unchanged;
  - any `ALTER` migration that exists only to adjust a renamed column is
    folded into the defining `CREATE TABLE`;
  - no backfill, compatibility view, or fingerprint migration path is written.
- **Managed-column set.** The managed-column set and every system-table schema
  change together. Bifrost table fingerprints change accordingly.

## Scope and non-goals

- **In scope:**
  - every table named above, and their writers, readers and wire types;
  - verification runtime authority, where judge calls run as the run's `sub`;
  - gateway attribution and spend;
  - Scribe stamping;
  - SDK projections;
  - generated contracts;
  - dev-fixture SYSTEM seeding;
  - `architecture/wyrd-design.md` and `architecture/bifrost-design.md`
    wherever they name the old columns or the old attribution rule;
  - `architecture/wyrd-security-posture.md`, replacing obsolete internal
    result-token prose with the accepted server-owned authority boundary.
- **Not in scope:**
  - changing token issuance or delegated exchange;
  - new permissions or roles;
  - trace propagation;
  - identity tables listed in REQ-005.
  - general revocation, durable Workflow execution, or changes to retention,
    sampling algorithms, telemetry propagation, or engine-internal lineage.

## Relationship to verify-attribution

SPEC-verify-attribution r3 lands first. This change rebases onto it and keeps
all of its behavior except the attribution columns it renames:

- verify-attribution REQ-004 stamps `run_id` and `card_uid` on judge gateway
  calls and joins Eval and observation results to their judge calls on
  `subject_card_uid` = `card_uid`. After this change that join is
  `card_uid` = `card_uid`, and judge calls carry the REQ-002 judge pair.
- verify-attribution D6 stamps tenant server-span rows with the requesting
  principal. This change replaces that stamp with the managed pair (REQ-002,
  REQ-005); the span frame's attribution comes from the emitting operation's
  trusted context, preserving the original subject and its execution actor.
- verify-attribution D8's enqueuing trace context on `wyrd.verifier_runs` is
  folded into the defining migration (expensive-to-reverse decisions).
- Trace continuity, span links, dual export, SDK propagation, and the trace
  timeline read are unchanged. INV-005 still holds: trace context never sets
  `sub` or `act`.
- Revision 3's tenant export eligibility for queued-ingest spans linked to
  sampled producer spans is retained. Attribution never substitutes for that
  eligibility decision. The prerequisite is still active; approval is not
  evidence that its implementation has landed.

## Relationship to example-journey-fixes

This change supersedes example-journey-fixes' attribution rules:
- OD-1's "SYSTEM is the requester" clauses;
- the open judge-authority item;
- REQ-004's requester for every origin.

OD-1's span link is kept. OD-3's verify-only trace propagation was already
superseded by verify-attribution D3.

## Acceptance criteria

| ID | Criterion | Evidence |
|---|---|---|
| AC-001 | A scheduled binding run, its judge calls (where Eval), and its result rows record `sub` = owner principal, `act` = SYSTEM. | Rust journey against `WyrdTestServer` and Postgres |
| AC-002 | An observation-triggered run records `sub` = binding owner principal, `act` = SYSTEM, even when another principal wrote the observation. Its judge gateway call is refused when the owner lacks `gateway:invoke` on the judge model, and its spend lands on the owner. | Rust journey |
| AC-003 | A direct gateway call and a direct verify record `sub` = `act` = caller in Bifrost, and the audit row matches. | journeys in Rust, Python and TypeScript |
| AC-004 | A delegated call (B on behalf of A) records `sub` = A and `act` = B on the gateway row and the audit row. | Rust journey |
| AC-005 | A client frame declaring `sub_principal_id` or `act_principal_id` is rejected with the existing reserved-column error. | journey in each SDK |
| AC-006 | No live schema, wire type, SDK, generated artifact, example, or architecture doc retains `caller_principal_id`, `requested_by_principal_id`, `subject_card_uid`, `audit_principal_id`, `verifier_uid`, `created_by`, or managed `principal_id` as work attribution. Identity keys and historical change records are classified rather than mechanically renamed. | source-backed disposition inventory plus `mise run codegen:check` |
| AC-007 | No new migration exists. `20261014000001` is gone. Postgres applies from empty. | `mise run test:sql` |
| AC-008 | A tenant seeded by `wyrd-dev-fixtures` has an active SYSTEM principal. | integration test |
| AC-009 | The smallest task set covering the actual write set passes, including real gateway SDK journeys (`test:gateway:gate` alone excludes them), all affected first-class language journeys, schema/codegen, SQL tenancy, and dependency boundaries. Avoid rerunning component lanes already covered by a selected aggregate. | scoped lane output and coverage mapping |
| AC-010 | A sampled, client-traced direct verify writes request-operation tenant spans with caller/caller, or A/B under delegation; judge-operation spans carry original subject/SYSTEM. The verify-attribution action-timeline journeys still pass, and Eval and observation results join their judge calls on `card_uid`. | Rust journey; the existing action-timeline journeys in Rust, Python and TypeScript |
| AC-011 | A Workflow step's model call records caller/caller for direct work and A/B under delegation, `card_uid` = the Workflow Card, and null `run_id`. | Rust journey |
| AC-012 | A delegated direct verify (B for A) whose judge model A may invoke but B may not is refused at the judge's gateway call; the judge call's `sub` is A. | Rust journey |
| AC-013 | A delegated queued manual verify survives worker reclaim/restart with A as subject, SYSTEM as judge actor, and the originally narrowed authority; a later subject-only permission lookup cannot authorize its denied model. Grant edits/token expiry do not widen or replace accepted authority. | durable queue integration and Rust journey |
| AC-014 | Registration and manual verification idempotency distinguish same actor/different subject and same subject/different actor. Exact replay reuses the operation; changed input conflicts; replay never rewrites accepted attribution or authority. Independent background bindings sharing SYSTEM never collapse; Workflow ownership semantics remain unchanged. | owning integration tests and Rust journey |
| AC-015 | Platform audit preserves the platform caller in the payload under the reserved audit tenant. Invalid/absent credentials use the existing unavailable-identity sentinel pair, never nil or unverified token claims. Allowed/denied auditing and non-blocking audit failure behavior remain intact. | HTTP/MCP authorization journeys and focused audit tests |
| AC-016 | Every inventory writer and read projection uses the pair, including HTTP/gRPC Scribe ingest, registration/create paths, denied decisions, and tenant server spans. Managed sampling uses subject identity; Variant payload policy remains unchanged. | schema/owner inventory, integration coverage and SDK journeys |
| AC-017 | Missing/inactive binding owner or SYSTEM fails acceptance observably without foreign-tenant fallback; accepted authority cannot be swapped during claim or retry. Tokenless input reads retain their existing narrow capability with run-subject/SYSTEM attribution; Operator authority rechecks remain intact. | binding/queue integration and cross-tenant Rust negative journey |
| AC-018 | Realtime request → Verifier → judge → gateway retains A as subject at every hop. Direct and delegated success record A/SYSTEM for judge gateway work and bill A. If either the subject or originating actor lacks model permission, gateway refuses even when the other permits it; further delegation cannot regain excluded permissions. Missing trusted authority fails closed rather than using ambient credentials. | Rust SDK → real server → gateway journey, including delegated negative flows; focused trusted-context boundary tests |

## Open decisions

None. Revision 4's authority lifetime, idempotency scope, unresolved audit
identity, and propagation/delegation decisions are approved.

## Implementation task

[TASK-001 — Consistent subject and actor attribution](tasks/TASK-001-sub-act-attribution.md)
owns the complete change. It depends on integrated verify-attribution r3 and
extends existing owners rather than introducing another attribution pipeline.

## Revision history

- r1 (2026-10-14): initial, approved.
- r2 (2026-10-14): lands after SPEC-verify-attribution r2. Adds the
  server-span row to REQ-002 and REQ-005, folds the D8 trace-context column
  into the defining migration, renames the judge-call join, and adds AC-010.
- r3 (2026-10-14): settles Workflow step calls (REQ-002, REQ-004); originating
  `sub` and delegated authority carry through every internal hop (REQ-002,
  REQ-003); adds AC-011 and AC-012; no open decisions.
- r4 (2026-10-14, approved): closes authority lifetime/restart semantics,
  subject-and-actor idempotency, table/policy coverage, platform and unresolved
  audit attribution, creation immutability, and sampling identity. Corrects
  delegated wording and RFC scope; preserves Workflow idempotency and narrow
  internal input-read authority; requires verify-attribution r3. Adds
  AC-013–AC-018 and explicit realtime propagation/further-delegation rules,
  plus one implementation task. Explicitly approved by the human owner.

## Authority

- `architecture/wyrd-design.md`:
  - SYSTEM principal (around line 173);
  - delegated token shape (lines 197-203 and 712-756).
- `architecture/bifrost-design.md`: managed columns and system tables.
- [RFC 8693 §4.1](https://www.rfc-editor.org/rfc/rfc8693.html#section-4.1)
  (`act` identity and nested historical actors; Wyrd policy is distinguished above).
- `architecture/wyrd-security-posture.md`: token and accepted Workflow
  authority snapshots; obsolete result-token prose must be reconciled.
- `architecture/references/languages/spec-driven-development.md`: approval
  and task derivation authority.
- `changes/active/example-journey-fixes/owner-decisions.md` OD-1 to OD-3.
