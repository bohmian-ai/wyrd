# Consistent subject and actor attribution

- Change: `sub-act-attribution`
- Specification: `SPEC-sub-act-attribution`, approved revision 6
- Completed: 2026-10-15
- Reviewed target: `cc80ed06b`, approved by the human owner in place of a final change review
- Delivery reference: not supplied

Every Wyrd table that records work now carries the same two server-stamped
columns, `sub_principal_id` (who the work was for) and `act_principal_id` (who
did it), following RFC 8693's subject/current-actor split. The four earlier
attribution schemes (`principal_id`, `caller_principal_id`,
`requested_by_principal_id`, `audit_principal_id`, plus `created_by`/`actor`)
are gone. Every decision, cost and audit row traces back to the principal the
work served, and says plainly whether a principal or Wyrd itself did it.

## Shipped behavior

- Direct work records caller/caller; a delegated token (B for A) records A/B.
  Scheduled and observation-triggered Verifier runs record the binding owner's
  principal with the tenant SYSTEM principal as actor, never the observation
  writer. Judge calls keep the run's subject with SYSTEM as actor. Workflow
  step model calls record the captured caller pair, the Workflow `card_uid`,
  and a null `run_id`.
- Bifrost managed columns are `run_id, card_uid, sub_principal_id,
  act_principal_id, wyrd_request_id, wyrd_ingested_at`. Both pair names are
  reserved; a client frame declaring either is refused with the existing
  reserved-column error in every SDK. Gateway calls, verification results,
  Drift/Eval result rows and tenant server spans use the managed pair;
  result tables name the Verifier as `verifier_card_uid`, and `card_uid` is
  the Card verified.
- Audit records the verified pair and `sub_principal_kind`, with the full
  delegation chain in detail. Platform decisions keep platform identities in
  the reserved audit tenant. `PLATFORM_AUDIT_PRINCIPAL` is the single
  no-verified-caller marker on audited exchange refusals and on creation rows
  no verified caller caused. Requests refused before any principal resolves
  are not audited.
- Gateway permission, spend and admission follow the subject under the
  accepted authority: a delegated request keeps the subject/actor
  intersection at every hop, and SYSTEM as actor grants nothing.
- An accepted manual run stores its credential-free effective authority with
  the queue record; claim, retry and restart reload it, and later grant edits
  or token expiry neither widen nor replace it. Background runs freeze the
  binding owner's authority at acceptance; a missing, inactive or foreign
  owner, or a tenant without SYSTEM, accepts no run and logs the refusal.
- Registration and manual-verification idempotency is scoped by tenant,
  subject, actor and key.
- Tenant provisioning by an authenticated platform operator records that
  operator's pair on the tenant's SYSTEM principal and built-in tables;
  callerless creation (bootstrap, reconciliation, reserved audit-owner tables,
  SQL backfill, dev-fixture seeding) records the sentinel. SYSTEM is never its
  own creator and the first pair is immutable.
- A locally loaded registered Workflow sends its Card UID on gateway calls.
  Callers whose Card scope includes it, or who are bound to no Card, may run
  it; other Card-bound callers get `WYRD_VALA_403_BIFROST_CARD_SCOPE` before
  any provider call.

## Invariants and constraints

- The pair is server-derived from verified identity, the binding owner, or
  the explicit sentinel; never from client input, headers, trace context or
  row values. It is attribution, not a permission grant.
- SYSTEM is never a subject; it is an actor only for Wyrd-owned execution.
- Tenant isolation is unchanged; owner and SYSTEM resolve within the run's
  tenant.
- Creation attribution and accepted run authority are immutable.
- Verification work mints no token. Drift and Eval input reads use a
  credentialless SYSTEM authority limited to the run's input table UIDs.
- Audit stays on the one non-blocking Scribe outbox; audit content keeps its
  decision pair while the frame travels under the transport sentinel.

## Decisions and deviations

- No migrations: nothing had shipped, so defining migrations and Iceberg
  declarations were edited in place, `20261014000001` was deleted, and the
  verify-attribution enqueuing trace context folded into the defining queue
  table. Bifrost fingerprints changed with the managed-column set.
- Plain `principal_id` remains only as identity/ownership keys; operational
  lineage tables and `operator_dispatches` keep their existing identities.
- Managed per-principal sampling uses the subject.
- r5 settled callerless creation provenance (sentinel versus operator pair)
  and left pre-identity bearer refusals unaudited, superseding a finding that
  asked for them to be audited.
- r6 settled local Workflow Card authorization: the existing gateway Card
  check applies unchanged. Journeys run local Workflows as the unbound tenant
  administrator; a Card-bound key outside the Workflow's scope can no longer
  run it locally.

## Acceptance closure

AC-001 through AC-019 closed through Rust server and SDK journeys (scheduled,
observation-triggered, direct, delegated, multi-hop, realtime judge, local and
hosted Workflow, reloaded authority after grant change and token expiry),
Python and TypeScript gateway, verify and action-timeline journeys,
provisioning and catalog integration tests, durable queue and idempotency
tests, audit and reserved-column negative flows, and the format, lint, codegen,
SQL, dependency, tenant-isolation and docs lanes. Two task reviews returned
`FIX_REQUIRED`; both remediations were implemented, the second for
documentation only. The human owner approved completion without a final
change review.

## Current owners

- [Wyrd design](../../../architecture/wyrd-design.md),
  [Bifrost design](../../../architecture/bifrost-design.md) and
  [security posture](../../../architecture/wyrd-security-posture.md).
- [Attribution pair](../../../crates/wyrd-spec/src/auth/attribution.rs),
  [accepted authority](../../../crates/shared/wyrd-runtime/src/request_context.rs),
  [managed columns](../../../crates/vala/vala-bifrost-redux/src/tables/managed_columns.rs)
  and [audit table](../../../crates/vala/vala-bifrost-redux/src/tables/audit/audit_log.rs).
- [Scribe outbox](../../../crates/wyrd/wyrd-server/src/scribe_outbox.rs),
  [peer capture](../../../crates/wyrd/wyrd-server/src/grpc/capture_peer.rs),
  [SYSTEM read authority](../../../crates/wyrd/wyrd-server/src/verification/authority.rs),
  [Verifier run queue](../../../crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs)
  and [local Workflow runtime](../../../crates/shared/wyrd-client/src/workflow/local.rs).
- Journeys and integration proof:
  [verification runtime](../../../crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs),
  [platform provisioning](../../../crates/wyrd/wyrd-server/tests/integration/platform_admin_e2e.rs),
  [Rust gateway inference](../../../sdks/wyrd-sdk-rust/tests/integration/gateway_inference.rs),
  [Python gateway inference](../../../sdks/wyrd-sdk-python/tests/integration/test_gateway_inference.py)
  and [TypeScript gateway inference](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-inference.test.ts).
