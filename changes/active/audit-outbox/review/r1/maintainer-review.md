# Maintainer Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `f451d52be01acba66bae003b8509c95086fc8558`
- Approved specification: `changes/active/audit-outbox/spec.md`, revision 1
- Tasks: `01-publication-progress.md`, `02-one-outbox.md`, and
  `03-remove-audit-unavailable.md`
- Review scope: `base..candidate` only

The candidate remained at the stated commit throughout this review. The
repository has no `.codegraph/` index, so navigation used the cumulative Git
diff, direct source reads, and caller/test searches.

## Changed-surface coverage

| Surface | Owner and material symbols reviewed | Caller, consumer, and proof coverage | Result |
|---|---|---|---|
| Process audit outbox | `vala-sql::audit_outbox::{AuditOutbox, AuditOutboxWriter}`, batching, tenant dispatch, failure accounting, settle and shutdown | `AppState`, Bifrost composition, server shutdown, auth test helpers, `pg_audit_outbox` | PASS with naming finding `MAINT-03` |
| Audit staging and publication progress | migration `20261003000000_audit_publication_progress.sql`; `append_audit_events`, `AuditRows`, `freeze_publication_range`, `list_publication_range`, `settle_publication` | `AuditPublisher`, SQL integration tests, server publication journeys, test-server watermark readers | PASS |
| Server composition and lifecycle | `compose_bifrost`, `BifrostComposition`, `Bifrost::audit_outbox`, `AppState::audit_outbox`, `BoundServer` drain | Gate, Oracle, peer security, gateway, verification, HTTP/MCP/gRPC request owners | PASS |
| Shared authorization helpers and request surfaces | `audit::{authorize, authorize_recording_denial, authorize_service_accounts_write}` and changed Cards, operators, principals, admin, platform, storage, OTLP, query, gateway, verification, and revoke consumers | Route/service tests and the audit-failure journey named in the task evidence | FAIL: changed documentation still describes the removed transaction semantics (`MAINT-01`) |
| Auth package | `TenantTokenIssuer`, API-key exchange/issuance, callback, refresh, JWT bearer, card-scope mint, platform authz and sessions | Server auth composition and Postgres-backed auth tests | FAIL: materially changed auth docs retain obsolete transactional-audit claims (`MAINT-01`) |
| Gate, Oracle, catalog, and peer authority | `GateAudit`, `Gate<AuditOutbox>`, `OracleAudit`, `PeerSecurityAudit`, query admission audit owner, peer authority and peer audit adapter, catalog registration | Server specializations, local/remote query callers, Gate/Oracle/peer tests | FAIL: staging collaborators retain append/durable-writer names that obscure the sole real append (`MAINT-03`); stale Oracle audit docs are also covered by `MAINT-01` |
| Error and wire contract removal | `WyrdError`, `BifrostError`, query terminal enum/proto, Rust client mappings, generated TypeScript error constants, Python/TypeScript test-hook removal | Served OpenAPI tests, proto snapshot, schema snapshots, SDK tests named in task evidence | FAIL: the `AuditEvent` source description and both regenerated schema copies still publish the superseded same-transaction contract (`MAINT-01`) |
| Tests | New SQL outbox tests, publication concurrency tests, per-surface audit-failure tests, end-to-end Gate plus run-start journey | Names, assertions, fixture ownership, and teardown/drain helpers | FAIL: permanent test rustdoc embeds disposable change-packet IDs (`MAINT-02`) |
| Architecture and operations docs | `AGENTS.md`, agent rules, Wyrd/Bifrost/security authority, runbook, harness guidance, agent-facing error remediation | Compared with the new outbox owner, failure log fields, and removed Oracle/query audit implementation | FAIL: contradictory authority and an inaccurate runbook field claim remain (`MAINT-01`) |

## Material findings

### MAINT-01 — The changed documentation set still teaches superseded audit owners and guarantees

- **Changed locations:**
  - `crates/wyrd-spec/src/vala/api.rs:2422-2456` still calls the contract
    "transactional audit staging" and says every `AuditEvent` is appended in
    the same transaction before the operation proceeds.
  - `crates/wyrd-spec/schemas/bifrost_audit_event.json:4` and
    `crates/wyrd-spec/tests/schemas/bifrost_audit_event.json:4` faithfully
    regenerate that false public description.
  - `crates/wyrd/wyrd-auth/src/callback.rs:165-173` still says the canonical
    append and session commit succeed together, although the changed issuer
    stages on `AuditOutbox` and never waits for that commit.
  - `crates/wyrd/wyrd-auth/src/issue_api_key.rs:42` still describes fields as
    needed by "transactional audit" after issuance was moved outside the
    credential transaction.
  - `crates/wyrd/wyrd-server/src/auth/revoke.rs:25-33` still promises that an
    allowance and suspension commit or roll back together, while the changed
    handler stages the allowance before the independent mutation.
  - `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:963-969` still calls the
    test sink a durable writer retaining a "real fail-closed audit call" after
    the trait became a non-blocking staging collaborator.
  - `architecture/wyrd-security-posture.md:369-381` still specifies an Oracle
    fsynced acceptance WAL/relay and transactional gateway-administration
    audit, contradicting both the newly edited paragraph immediately above it
    and the one-outbox/no-other-WAL authority.
  - `architecture/operations/runbooks.md:120-124` tells operators the failure
    log names the tenant, but `record_commit_failure` at
    `crates/vala/vala-sql/src/audit_outbox.rs:311-329` logs only the error,
    operation, and request id, exactly as the approved specification requires.
- **Governing rule / guide principle:** `AGENTS.md` §16 makes documentation
  part of implementation correctness and requires materially modified Rust
  items to explain their real invariants and side effects. Maintainer Style's
  documentation rule requires the text to expose the actual durability,
  partial-progress, and retry contract. The approved specification requires
  one process outbox, no request-coupled audit commit, and no other WAL or
  relay.
- **Concrete maintenance cost:** a maintainer following the public Rust
  contract, generated schema, auth/route rustdoc, security authority, or
  operations guide will implement and diagnose mutually incompatible systems.
  The generated artifacts being green is misleading here: codegen currently
  proves parity with stale source prose, not parity with the new contract.
  During an incident, the runbook directs the operator to a tenant field the
  implementation does not emit.
- **Smallest testable correction:** update the source rustdoc and governing
  security/operations text to the single process-outbox semantics, remove the
  obsolete Oracle acceptance-WAL/relay and transactional-admin claims, and
  make the runbook list only fields actually logged. Regenerate the two schema
  copies from the corrected `AuditEvent` source. Close with focused searches
  for the obsolete durability phrases plus `mise run codegen:check` and
  `mise run docs:check`.

### MAINT-02 — New permanent test documentation embeds disposable task requirement IDs

- **Changed location:** `crates/vala/vala-sql/tests/pg_audit_outbox.rs:83-85`
  names `AC-003`, `INV-002`, and `AC-007`; lines `131-132` name `REQ-002`.
- **Governing rule / guide principle:** `architecture/agent-rules.md` prohibits
  plan/task IDs in permanent code, and Maintainer Style says not to leave task
  IDs or implementation history in permanent code.
- **Concrete maintenance cost:** completion deletes the active change packet,
  leaving future maintainers with opaque identifiers whose definitions are no
  longer adjacent to the tests. The prose already explains the asserted
  behavior, so the IDs add a stale second index without improving the test.
- **Smallest testable correction:** remove the parenthesized requirement IDs
  and retain the behavior-focused test descriptions. `git grep` over production
  and test source should then find no audit-outbox task/spec IDs.

### MAINT-03 — Staging-only collaborators are still named as durable append operations

- **Changed locations:**
  - `crates/vala/vala-bifrost-redux/src/gate/mod.rs:268-281`
    (`GateAudit::append_write_decision`)
  - `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:833-889`
    (`OracleAudit::append_read_decision` and `append_security_violation`)
  - `crates/vala/vala-bifrost-redux/src/oracle/peer.rs:729-742`
    (`PeerSecurityAudit::append_*`)
- **Governing rule / guide principle:** Maintainer Style requires function names
  to describe their actual responsibility and make failure/partial-progress
  behavior visible. The approved design makes "stage" and "append" different,
  load-bearing operations: these methods only enqueue and return, while
  `append_audit_events` is the sole canonical database append.
- **Concrete maintenance cost:** callers and repository searches now present
  several apparent audit appenders despite the one-write-path invariant. A
  maintainer tracing AC-001 or adding a new sink has to read every body to learn
  that these methods merely stage and can drop an event, while the real append
  lives elsewhere.
- **Smallest testable correction:** rename the staging collaborator methods and
  their implementations/callers from `append_*` to `stage_*`; leave the real
  SQL owner named `append_audit_events`. This is a mechanical private/internal
  rename with no contract or behavior change. Compile the affected Gate,
  Oracle, peer, and server targets, and verify that production `append_audit*`
  references resolve only to the canonical outbox writer path.

## Calibration notes

- The user-approved eventual-consistency model, receipt acknowledgement,
  graceful-shutdown drain, and counted hard-kill loss window were treated as
  intentional and are not findings.
- `AuditOutbox` is a cohesive, state-owning struct in the crate selected by the
  approved task; its queue, writer, tenant batching, settle, and shutdown
  operations are discoverable as inherent methods. No alternate owner shape is
  requested.
- The three public tuning constants in `audit_outbox.rs` have no external
  callers and could be private, but their visibility alone does not create a
  concrete acceptance risk, so it is not a blocking finding.
- No separate follow-up question is needed for maintainer scope; each finding
  has a bounded correction within the approved behavior.

## Verification notes

- Reviewed the complete `base..candidate` file set and the task-recorded
  verification evidence, including the SQL, server, Bifrost, auth, SDK,
  codegen, docs, format, and lint lanes.
- Independently ran `git diff --check base..candidate`; it passed.
- I did not rerun the task's long Postgres, journey, capacity, or aggregate
  lanes. The findings are source/documentation and naming defects not disproved
  by the reported green runs.
- Candidate identity was rechecked before writing this report and remained
  `f451d52be01acba66bae003b8509c95086fc8558`.

## Overall result

**FAIL**

The core owner and batching shape is maintainable, but the candidate is not
maintainer-complete while changed public/generated documentation describes the
removed audit architecture, permanent tests retain disposable task IDs, and
staging-only collaborator names obscure the single true append boundary.
