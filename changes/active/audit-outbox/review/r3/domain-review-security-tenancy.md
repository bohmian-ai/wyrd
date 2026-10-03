# Security, tenancy, and audit-integrity domain review

## Immutable subject and scope

- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `e54b1244f32950d1ab251dae6530c4e1694c78d5`
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 3
- Prior closure hypotheses: `FIND-AUDIT-OUTBOX-11` and
  `FIND-AUDIT-OUTBOX-3`
- Additional scope: security, tenancy, and audit-integrity regressions introduced
  by the immutable remediation range

`HEAD` matched the candidate before and after this review. The repository has no
`.codegraph/` directory, so source navigation used repository search and direct
caller inspection. No production source or test was modified.

## Reviewed boundary

The review traced the decision identity and tenant binding through:

1. `StagedAuditEvent::from`, the tenant-grouped audit sink, and
   `append_audit_events`;
2. the `(data_tenant_id, event_id)` staging uniqueness fence and every changed
   staging-row read used by publication;
3. `AuditPublisher::publish_tenant` and `publish_range`;
4. `project_audit_rows`, the canonical `AuditLogTable` schema, and Scribe's
   built-in registration/fingerprint checks;
5. retained-audit reads in `WyrdTestServer`; and
6. shipped query entry points through HTTP/gRPC service dispatch, MCP, CLI, and
   the shared Rust client projected into Python and TypeScript. The current UI
   has no retained authorization-audit reader; its unrelated change-workspace
   “Audit” page does not query `vala.system.audit_log`.

The audit is intentionally limited to whether the range loses, duplicates,
misattributes, or crosses tenant boundaries for authorization decisions. It
does not reopen earlier accepted implementation outside those outcomes.

## Authority coverage

| Boundary | Governing authority | Result |
|---|---|---|
| Event identity and read cardinality | Revision-3 `REQ-009` and `AC-009`; `AGENTS.md` audit-cardinality decision; `architecture/wyrd-security-posture.md` audit integrity | **FAIL** — identity reaches retained rows, but only test-harness readers collapse duplicates |
| Tenant isolation | `INV-003`; `AGENTS.md` tenant separation; security posture RLS and credential-derived tenancy | **PASS** — staging and publication remain tenant-bound, and projection rejects a foreign row |
| Retained-schema evolution | Revision-3 approved retained `event_id` schema change; `architecture/references/domain/iceberg.md` explicit compatibility requirement for nullability/required-field changes | **FAIL** — an already registered audit table is refused rather than evolved |
| Operational ownership wording | `FIND-AUDIT-OUTBOX-3`; one outbox/one publisher authority | **PASS** — the remaining Oracle commit-owner phrase now names the shared audit outbox |

## Source coverage and positive controls

- `crates/vala/vala-sql/src/queries/audit_staging.rs:113-195` checks existing
  event IDs under a `TenantConn`, retains the same `event_id` beside the same
  decision fields, and inserts it under the tenant-bound transaction. The
  existing database constraint is `(data_tenant_id, event_id)`, so two tenants
  may legitimately use the same UUID without colliding globally.
- `crates/vala/vala-sql/src/queries/audit_staging.rs:291-304,324-334,457-469`
  now selects `event_id` for resource, batch, and frozen-range reads. This
  prevents the publication projection from inventing or losing identity.
- `crates/vala/vala-bifrost-redux/src/tables/audit/projection.rs:76-88,108-135`
  binds the projection to the authenticated tenant and rejects any staged row
  whose `data_tenant_id` differs. Lines 228-266 project each row's own
  `event_id` into the matching decision row, without a parallel iterator or
  positional join that could misassociate identities.
- `crates/wyrd/wyrd-server/src/audit/publication.rs:272-348` publishes that
  projection using the same authenticated tenant and the sole
  `AuditPublisher`/Scribe path. No second writer or cross-tenant identity source
  entered the range.
- `crates/wyrd/wyrd-testing/src/server.rs:1737-1795` deduplicates a listing by
  `event_id`, and lines 1841-1848 count `DISTINCT event_id`. Both helpers first
  construct one tenant's authorized query context, so their in-memory set is
  effectively keyed by `(tenant, event_id)`, not globally across tenants.
- `architecture/wyrd-security-posture.md:423-428` now says “audit outbox write
  failure”; no Oracle-specific commit owner remains in the live authority.

These controls close the identity-carrying and tenant-association portions of
`FIND-AUDIT-OUTBOX-11` and close `FIND-AUDIT-OUTBOX-3`. They do not close the
two material gaps below.

## Security Audit

### Critical

None.

### High

None.

### Medium

#### SEC-TEN-R3-001 — shipped audit count/list reads do not collapse retained duplicates

- **Classification:** `INCORRECT`; `FIND-AUDIT-OUTBOX-11` remains open.
- **Violated obligation:** Revision-3 `REQ-009` says audit reads that count or
  list decisions collapse rows sharing `(tenant, event ID)`; `AC-009` requires
  that behavior through the production publisher. The security posture assigns
  cardinality to authorization decisions, not delivery attempts.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/server.rs:1737-1795,1841-1848` implements
  collapse only in test-support helpers. The shipped path forwards caller SQL
  unchanged at `crates/wyrd/wyrd-server/src/query/service.rs:200-220`;
  `crates/wyrd/wyrd-server/src/mcp/bifrost.rs:302-307,386-418`;
  `crates/wyrd/wyrd-cli/src/query/mod.rs:67-81,100-122`;
  `crates/shared/wyrd-client/src/bifrost/facade.rs:652-693`;
  `sdks/wyrd-sdk-python/src/bifrost/mod.rs:456-481`; and
  `sdks/wyrd-sdk-ts/native/src/lib.rs:731-743`.
- **Evidence:** `event_id` has no production use outside staging/publication;
  the repository-wide source trace finds deduplication only in
  `WyrdTestServer`. An authenticated caller can issue
  `SELECT count(*) FROM vala.system.audit_log` or list rows through HTTP/gRPC,
  Rust/Python/TypeScript, CLI, or `bifrost.query`; Oracle evaluates the raw
  table and returns both rows of the approved unknown-outcome retry. Tenant
  authorization prevents cross-tenant leakage, but it does not turn two
  delivery rows into one decision.
- **Observable consequence:** the shipped audit history can over-count or list
  one allow/deny decision twice during incident response, compliance export,
  or authorization-denial analysis. The new journey passes because it calls a
  harness-only wrapper that ordinary users and agents cannot call.
- **Required testable correction:** put decision-level deduplication on the
  shipped server-owned audit read boundary for every supported surface, keyed
  by tenant plus event ID, and prove the unknown-outcome production-publisher
  scenario through at least one real public query surface. A correction must
  not globally deduplicate equal UUIDs across tenant contexts and must not rely
  on callers knowing to add `DISTINCT`. If revision 3 intentionally leaves raw
  generic SQL as the only audit reader and expects each caller to rewrite its
  own SQL, that is contrary to the approved text and requires renewed spec
  approval rather than being accepted as implementation evidence.

#### SEC-TEN-R3-002 — the retained-schema change strands existing audit tables on the old fingerprint

- **Classification:** `REGRESSION`.
- **Violated obligation:** the user-directed no-regression boundary; revision-3
  retained audit correctness; and
  `architecture/references/domain/iceberg.md`, which requires explicit
  compatibility for schema nullability and required-field changes.
- **Exact location:** the range adds a required field at
  `crates/vala/vala-bifrost-redux/src/tables/audit/audit_log.rs:45-73`.
  Publication calls Scribe with the new projection at
  `crates/wyrd/wyrd-server/src/audit/publication.rs:332-348`. Scribe always
  calls `ensure_builtin` at
  `crates/vala/vala-bifrost-redux/src/scribe/ingress.rs:183-204`, while
  `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1016-1042,1110-1127`
  rejects an existing registration whose stored fingerprint differs. The
  remediation range adds no retained-table evolution or deployment migration.
- **Evidence:** adding non-null UTF-8 `event_id` changes the canonical user
  fingerprint. For a tenant that already has `vala.system.audit_log`, the next
  publisher attempt reaches `ensure_builtin`, sees the old catalog fingerprint,
  and returns `FingerprintMismatch` before Scribe accepts the batch. The claim
  that no compatibility path is needed because the schema is unshipped is not
  established by the approved spec or by a source guard that makes an old
  registration unreachable.
- **Observable consequence:** after an in-place upgrade, new authorization
  decisions remain in transient staging and never enter authoritative retained
  history. Public audit queries therefore omit post-upgrade decisions; a
  prolonged failure also grows the unbounded staging backlog. Existing rows are
  not cross-tenant, but the retained audit result is incomplete.
- **Required testable correction:** provide an explicit additive evolution or
  upgrade path for an existing tenant audit table and its control-plane
  fingerprint, preserving every old decision and assigning semantics that do
  not collapse unrelated legacy rows. Prove upgrade from the old canonical
  schema, successful publication of a new event-ID row, and a decision-level
  read spanning old and new files. If support is intentionally limited to
  deployments where no audit table can preexist, make that deployment boundary
  explicit in approved authority and prove it from release/upgrade state; an
  implementer note is not sufficient.

### Low / Defense In Depth

None in the user-directed scope.

### Positive Controls

- Event identity stays attached to the correct decision row from staging
  through retained projection.
- RLS-backed `TenantConn` and projection-time tenant verification keep
  publication tenant-scoped.
- The harness's dedup logic is tenant-local and does not globally collapse an
  event ID reused by another tenant.
- Publication still uses one canonical publisher and no alternate audit sink.
- The operational security authority now attributes write failures to the
  shared audit outbox rather than Oracle.

## Verification and limits

| Evidence | Result |
|---|---|
| `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^tables::audit::/)'` | **PASS, 7/7**, including cross-tenant rejection and event-ID projection |
| Static caller trace of HTTP/gRPC service, MCP, CLI, shared Rust client, Python, and TypeScript | **FAIL for decision collapse**; all expose the generic SQL result, and none adds audit semantics |
| Static upgrade trace from `AuditPublisher` to `ensure_builtin` and stored fingerprint comparison | **FAIL for compatibility**; old fingerprint is refused before ingest |
| Implementer-recorded Postgres and server journey evidence | Accepted as evidence that a fresh schema publishes and that harness readers collapse; it does not exercise a public shipped audit reader or an old registered table |

The sandbox rerun did not include the Postgres-backed journey. `FIND-5`,
`bench:capacity`, and `mise run gate` remain deferred to integration exactly as
directed and do not affect this domain verdict.

## Overall result

**FAIL**

`FIND-AUDIT-OUTBOX-3` is closed. `FIND-AUDIT-OUTBOX-11` is only partially
closed: retained rows carry the correct tenant-associated event ID, but shipped
audit reads do not collapse the permitted duplicate. The range also introduces
`SEC-TEN-R3-002`, a retained-audit availability/integrity regression for an
existing registered schema.
