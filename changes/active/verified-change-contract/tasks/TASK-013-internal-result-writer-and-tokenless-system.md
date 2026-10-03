---
id: TASK-013
kind: implementation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 59
requirements: [REQ-086, REQ-114, INV-021, AC-023, AC-030, AC-043]
depends_on: [TASK-004, TASK-005, TASK-006]
---

# Write Verifier results through the capture writer; remove SYSTEM tokens

## Outcome and Value

Verifier results become a server-internal write, like gateway capture: the
process's one capture writer submits result batches to Scribe in-process or
over the mutually authenticated peer RPC, with no token, no
`wyrd_client::Bifrost`, no public listener, and no Gate (REQ-086). Gate
refuses every public write to the three result tables, from any principal
(INV-021, AC-030). The SYSTEM result-write token and the SYSTEM Drift read
token, and their issuance and verification paths, are deleted; Drift reads
through the same tokenless SYSTEM read authority Eval already uses, carried to
a local or peer-forwarded Oracle (REQ-086, AC-023). The peer RPC carries the
three result tables in addition to the two capture tables and refuses
everything else (AC-043). Architecture and public docs stop describing a
SYSTEM token or a result ingest endpoint (REQ-114).

## Owners, Scope, Consumers, and Prohibited Changes

- `wyrd-server` `components/gateway/capture.rs` owns the one capture writer;
  `grpc/capture_peer.rs` owns the peer RPC; `wyrd-tonic` owns its proto.
- `wyrd-server` `verification/` owns result submission (the token-backed
  `publisher.rs` is deleted) and the SYSTEM read authority shared by Drift and
  Eval.
- `vala-bifrost-redux` Gate owns the public write refusal; Scribe already
  stamps `card_uid` from the frame principal's Card scope and refuses a row
  whose `card_ref` names another Card.
- `wyrd-auth`, `wyrd-auth-issue`, `wyrd-auth-verify`, `wyrd-runtime` lose the
  SYSTEM token paths. `PrincipalKind::System` stays: it is the in-process
  attribution and read identity.
- Result rows carry the tenant's SYSTEM principal as `principal_id` and the
  frozen Verifier Card UID as `card_uid`; the values come from the run row and
  tenant state, never from a Verifier or the Arrow payload.
- Prohibited: a new token format, principal kind, permission, public route,
  Oracle endpoint, plan serialization, or identity store; dropping a result
  batch; any Drift completeness or freshness logic; the withdrawn revision-58
  half-pool bound.

## Approach

1. Widen the capture writer's destinations to the three result tables, with a
   Verifier attribution (run, Verifier ref with UID, SYSTEM principal) that
   builds the frame principal; result submissions retry retryable refusals
   until acknowledged, terminally refused, or cancelled.
2. Widen the peer RPC request with the attribution fields; refuse any other
   table, a reserved tenant, a result table without attribution, and a capture
   table with one.
3. Replace `ResultPublisher` and the `ingest_endpoint` configuration with the
   capture writer; compose the runner whenever this process reaches a Scribe.
4. Make Gate refuse every public result-table write and delete the SYSTEM
   write branch and its frame scope check.
5. Generalize the Eval read authority into one SYSTEM read authority scoped to
   a run's input tables; Drift uses it; delete the SYSTEM token issuance,
   verification, and `drift_table_read` permission.
6. Update architecture and docs.

## Ordered Implementation Scenarios

### Scenario 1 — Peer RPC admits exactly the five server-internal tables

**Behavior.** The peer RPC accepts the two capture tables without attribution
and the three result tables only with a run, Verifier ref, Verifier UID, and
SYSTEM principal; every other table, the reserved tenant, and a mismatched
attribution are refused (REQ-086, AC-043).

**RED.** Extend `requests_are_confined_to_tenant_capture_destinations` to
submit result tables; it fails because they are refused.
`mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=grpc::capture_peer::tests::requests_are_confined_to_tenant_capture_destinations)'`

**GREEN.** Add the proto fields and the result destinations with attribution.

**REFACTOR.** Keep one batch and frame builder for local and peer paths.

### Scenario 2 — Result frames carry the SYSTEM principal and Verifier scope

**Behavior.** A result batch's frame names the SYSTEM principal and a Card
scope of exactly the frozen Verifier, so Scribe stamps them and refuses a row
naming a different Card; result submission retries saturation until
acknowledged and stops on cancellation (REQ-086, INV-021).

**RED.** A capture-writer unit test submitting a result batch to the recording
Scribe and asserting the frame principal, and a saturation-then-cancel case.
`mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=components::gateway::capture::tests::result_batches_submit_under_the_system_principal)'`

**GREEN.** Implement the result submission path on the capture writer.

**REFACTOR.** Share the retry pause between deadline-bounded capture and
cancellation-bounded results.

### Scenario 3 — Runner writes results through the capture writer

**Behavior.** Queued runs complete with results acknowledged by Scribe through
the capture writer; the runner is composed without any ingest endpoint or
issuer (REQ-086, AC-023).

**RED.** The existing runtime journeys in `pg_verification_runtime` and
`wyrd-testing` `verification_runtime` fail to compile once the publisher and
endpoint are removed.

**GREEN.** Wire the runner to `AppState::gateway_capture`; delete
`publisher.rs`, the endpoint configuration, and the fault harness; port the
fault-dependent tests to a recording or refusing Scribe.

**REFACTOR.** Delete dead configuration and docs.

### Scenario 4 — Gate refuses every public result-table write

**Behavior.** Every principal, including a wildcard administrator, gets
`ReservedBuiltinWriteDenied` for each result table (AC-030, INV-021).

**RED.** Gate unit test with a wildcard principal writing each result table.
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::public_result_table_writes_are_refused_for_every_principal)'`

**GREEN.** Reserve the result tables like `vala.gateway.calls`; delete the
SYSTEM branch and its frame scope check.

**REFACTOR.** One reserved-table predicate.

### Scenario 5 — SYSTEM tokens no longer exist; Drift reads tokenless

**Behavior.** The issuer cannot mint any SYSTEM token, verification refuses any
`kind=system` claim set, and Drift runs complete through the SYSTEM read
authority scoped to `vala.drift.observations` via the ordinary query service
(REQ-086, AC-023).

**RED.** Existing issuer/verify tests that mint SYSTEM tokens are replaced by
refusal tests; the Drift runtime journey fails while Drift still needs an
issuer.

**GREEN.** Delete the issuance and verification paths; generalize the read
authority; drop the issuer from `DriftEngine`.

**REFACTOR.** Remove `Permission::drift_table_read` and unused errors.

## Acceptance Criteria

- No result write touches Gate, a token, or the public listener; Gate refuses
  every public result-table write; the peer RPC admits exactly five tables.
- Result rows carry the SYSTEM `principal_id`, the exact Verifier `card_uid`,
  and explicit run/binding/subject IDs; a mismatched `card_ref` is refused.
- The issuer refuses SYSTEM; verification refuses `kind=system` claims.
- Drift and Eval reads use one tokenless SYSTEM read authority.
- Architecture and docs no longer describe SYSTEM tokens or an ingest endpoint.

## Expected Write Set and Consumer Closure

`crates/wyrd/wyrd-server/src/{components/gateway/capture.rs,grpc/capture_peer.rs,verification/*,config.rs,app/server.rs}`,
`crates/wyrd/wyrd-tonic/proto/wyrd.v1.proto`,
`crates/vala/vala-bifrost-redux/src/gate/mod.rs`,
`crates/wyrd/wyrd-auth/src/issuance.rs`, `crates/shared/wyrd-auth-issue`,
`crates/shared/wyrd-auth-verify`, `crates/shared/wyrd-runtime/src/permission.rs`,
server and `wyrd-testing` tests, `architecture/wyrd-design.md`,
`architecture/bifrost-design.md`, `docs/src/content/docs/self-hosting/configuration.svx`.

## Verification and Evidence

- The exact focused commands above.
- `mise run fmt`, `mise run lints`, `mise run test:bifrost:integration:server`,
  `mise run test:bifrost:integration`, `mise run test:principals:integration`,
  `mise run test:vala`, `mise run docs:check`.

## Material Stop Conditions

- Scribe cannot attribute result rows to the SYSTEM principal without a Card
  scope in the frame principal.
- The peer plane cannot carry the SYSTEM read authority to a remote Oracle.

## Authority Links

- [Approved spec revision 59](../spec.md): REQ-086, REQ-114, REQ-178..180,
  INV-021, AC-023, AC-030, AC-043.
- `AGENTS.md`, `architecture/wyrd-design.md`, `architecture/bifrost-design.md`.
