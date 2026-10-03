# Persistent-data and durability domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `e54b1244f32950d1ab251dae6530c4e1694c78d5`
- Reviewed range: `cf5ee4128..e54b1244f`
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 3, especially REQ-009 and AC-009
- Prior finding under closure: `FIND-AUDIT-OUTBOX-11`
- User-directed scope: persistent-data closure of FIND-11 and regressions introduced by the reviewed range; FIND-5, `bench:capacity`, and `mise run gate` remain deferred

`HEAD` was `e54b1244f32950d1ab251dae6530c4e1694c78d5` before and after this review. The repository has no `.codegraph/` index.

## Reviewed boundary

The reviewed durability boundary is:

```text
StagedAuditEvent.event_id
  -> TenantConn / AuditSink transaction
  -> vala.audit_staging uniqueness
  -> frozen AuditPublisher range
  -> audit RecordBatch projection
  -> Scribe retained vala.system.audit_log rows
  -> staging retirement
  -> retry after an unknown commit result
  -> retained audit count/list reads
```

I traced the persisted event identity, chain sequence, projection schema, built-in table registration, schema fingerprint, Iceberg physical schema, retirement transaction, retry ownership, and the production query surfaces that can count or list retained decisions. I did not reopen earlier accepted implementation except where it directly determines whether retained audit results are duplicated.

## Authority and source coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Revision-3 retained identity | Spec REQ-009 / AC-009; `vala-sql/src/audit_outbox.rs`; `vala-sql/src/queries/audit_staging.rs`; `vala-sql/src/row_types/audit_staging.rs` | The same tenant-scoped event ID survives an ordinary retry and is selected for publication. Staging remains unique per tenant and event ID. |
| Publication and retirement | `wyrd-server/src/audit/publication.rs`; `audit_staging.rs::freeze_publication_range`, `list_publication_range`, and `settle_publication`; Bifrost audit-publication authority | The frozen range remains range-idempotent. Settlement advances the tenant watermark and deletes staging through it atomically. After deletion, staging no longer fences the event ID. |
| Retained projection | `vala-bifrost-redux/src/tables/audit/{audit_log.rs,projection.rs,mod.rs}` | Every newly projected retained row carries non-null `event_id`; projection maps it by the canonical declared column order. No second audit store or publisher was introduced. |
| Retry lifecycle | `wyrd-runtime/src/outbox.rs::finish`; `vala-sql/src/queries/audit_staging.rs::append_audit_events` | A failed/unknown result retries indefinitely at the tenant front. Each attempt only checks current staging, so retirement between repeated unknown outcomes can make the same event fresh repeatedly. |
| Retained read semantics | `wyrd-testing/src/server.rs::retained_audit_records` and `retained_audit_rows`; HTTP `/v1/query`; gRPC query; MCP `bifrost.query`; CLI query; shared Rust client; Python and TypeScript SDK query methods | Collapse is implemented only in `wyrd-testing`. Every production surface passes caller SQL to the common Oracle query service and exposes the physical retained rows. |
| Retained schema/catalog compatibility | `AuditLogTable::arrow_fields`; `BifrostCatalog::ensure_builtin` / `create_table_locked`; `validate_physical_table`; schema fingerprint owners; Iceberg schema authority; deployment/release authority | The new required field changes both the catalog fingerprint and physical Iceberg schema. An already-registered old table would fail `FingerprintMismatch` before publication. Current release authority explicitly states that no Wyrd image has yet been published, so there is no supported predecessor deployment or rolling-version interval to migrate in this first release. This is not a finding under current authority. |
| Tenant isolation | `TenantConn`, per-tenant physical table binding, staging RLS, projection tenant validation, and tenant-scoped query planning | No lost, cross-tenant, or misattributed event-ID path was found in the reviewed range. The harness's set key omits tenant only because each query is already bound to one physical tenant table. |

## Schema-fingerprint compatibility judgment

The implementer's reported risk is technically accurate: adding required `event_id` changes the `vala.system.audit_log` user-schema fingerprint and physical Iceberg schema. `BifrostCatalog::create_table_locked` rejects an existing control row whose fingerprint differs (`bifrost_catalog.rs:1110-1113`), and even a rewritten control row would then encounter the exact physical-schema comparison (`bifrost_catalog.rs:1238-1261`). The candidate contains no catalog or Iceberg evolution path.

That is not a closure defect under the current approved and deployment authority. Revision 3 explicitly approves the retained-schema change and leaves no material decision open, while `architecture/operations/deployment-and-release.md:209-213` states that no Wyrd image has been published and defines the next artifact as the first release. Therefore no supported old image, retained production catalog, or rolling old/new replica pair exists in the current compatibility interval. The Iceberg requirement for explicit compatibility becomes controlling once a released schema must remain readable (`architecture/references/domain/iceberg.md:24-29`), and future released schema changes must use the expand-and-contract rules at `deployment-and-release.md:163-168`; it does not require an upgrade shim for this pre-release replacement.

If integration has an external retained catalog that is intended to survive into the first supported release despite that authority, this judgment must be revisited before merge: the current candidate will refuse publication for that tenant. No such supported deployment is evidenced in the reviewed repository.

## Material findings

### PDATA-R3-001 — production audit reads do not collapse retained copies by event ID

- **Classification:** `MISSING`
- **Violated obligation:** Revision-3 REQ-009 requires audit reads that count or list decisions to collapse rows sharing `(tenant, event_id)`; AC-009 requires the production-publisher proof to show that behavior.
- **Exact location:** `crates/wyrd/wyrd-testing/src/server.rs:1707-1709,1751-1795,1817-1818,1828-1848`; `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:909-921,1006-1049`; production pass-through surfaces at `crates/wyrd/wyrd-server/src/query/routes.rs:281-296`, `crates/wyrd/wyrd-server/src/grpc/query.rs:120-136`, `crates/wyrd/wyrd-server/src/mcp/bifrost.rs:376-420`, `crates/wyrd/wyrd-cli/src/query/mod.rs:67-81`, `crates/shared/wyrd-client/src/bifrost/facade.rs:652-693`, `sdks/wyrd-sdk-python/src/bifrost/mod.rs:448-481`, and `sdks/wyrd-sdk-ts/wyrd/src/index.ts:840-880`.
- **Evidence:** The only collapse implementation is the test harness: `retained_audit_records` builds an in-memory `HashSet` of event IDs and `retained_audit_rows` issues `SELECT DISTINCT event_id`. The new journey explicitly proves that ordinary retained SQL sees two rows (`await_retained_where(..., 2)`) and only then calls those harness helpers to obtain one. The production HTTP, gRPC, MCP, CLI, Rust, Python, and TypeScript query paths accept the caller's SELECT and return Oracle's physical result without audit-specific collapse. No production UI audit reader was found, but the absence of a UI surface does not close the first-class query surfaces.
- **Observable consequence:** An operator or agent issuing `SELECT COUNT(*)` or listing rows from `vala.system.audit_log` through any shipped query surface observes one authorization decision multiple times after the exact unknown-outcome/retirement path revision 3 accepts. Retained audit cardinality is therefore wrong on production surfaces even though the test harness reports the expected logical count.
- **Required correction:** Put event-ID collapse in the production owner of retained `vala.system.audit_log` reads so the common Oracle path supplies one logical decision to every HTTP, gRPC, MCP, CLI, Rust, Python, and TypeScript caller. Do not duplicate this rule in each client. Preserve the physical at-least-once rows and the one publisher; use a privileged/internal storage assertion, rather than a public audit read, when a test must prove that two physical copies exist. Add a real server-to-client journey showing both `COUNT` and a row listing return one decision after publication, retirement, and retry.

### PDATA-R3-002 — repeated unknown commit outcomes can retain more than one extra copy

- **Classification:** `INCORRECT`
- **Violated obligation:** AC-009 requires a retry after retirement to produce **at most one** extra retained row for the event ID.
- **Exact location:** `crates/vala/vala-sql/src/queries/audit_staging.rs:57-71,113-130,166-209,472-522`; `crates/shared/wyrd-runtime/src/outbox.rs:368-423`; insufficient proof at `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:821-870,909-932,988-1007`.
- **Evidence:** `append_audit_events` rejects an event ID only while it is present in `vala.audit_staging`. `settle_publication` deletes that evidence. The generic outbox retries the same owned batch after every reported failure with no attempt limit, as revision 3 otherwise requires. Therefore this reachable cycle can repeat: commit succeeds but acknowledgement is lost; publisher retains and retires the row; retry sees no staged ID and commits another row; that acknowledgement is also lost; publisher retires it; the next retry commits a third copy, and so on. Each copy receives a fresh sequence and a fresh range-derived Scribe batch identity. The journey's `AtomicBool` injects only one lost acknowledgement and makes the following write return success, so it proves exactly two copies only by excluding the repeated failure path.
- **Observable consequence:** One decision may occupy an unbounded number of retained chain entries rather than the maximum two approved by AC-009. A production read-collapse fix would hide the cardinality from logical audit results, but it would not satisfy the approved persistent-data bound or prevent unbounded duplicate retention under a recurring connection-loss pattern.
- **Required correction:** Establish a durable/reconcilable state transition that distinguishes the one permitted post-retirement restage from later ambiguous attempts without dropping an event whose latest attempt may not have committed. Then prove two consecutive commit-after-send acknowledgement losses with publication and retirement between them cannot create a third retained copy. If no correction can satisfy that bound while preserving the approved prohibition on a second identity ledger, retirement delay, or alternate publisher, this finding requires specification revision rather than a downstream guard or a test that injects only one unknown outcome.

## Regression assessment

Apart from the two REQ-009 closure failures above, the reviewed persistence range introduces no evidenced loss, cross-tenant decision, event-ID misattribution, chain gap, second audit authority, or publication-order regression. The event ID is projected from the tenant-bound staging row into the retained row, and later same-tenant events remain behind the retry in the outbox. The pre-release schema replacement is deliberately not treated as a regression for the authority reasons above.

## Verification limits

- I inspected the complete reviewed diff and the current owners/callers for staging, publication, retained projection, catalog registration, schema identity, retirement, retry, and public query surfaces.
- The implementer reports passing focused Redux audit tests (7/7), Bifrost SQL integration (119/119), the named unknown-outcome journey, and `test:bifrost:journey:server` (31/31). Those results were not independently rerun in this domain pass; shared-checkout Cargo lanes must run sequentially, and the report's defects are already demonstrated by source and by the journey's own raw-count assertion.
- The recorded journey proves one unknown outcome before a successful retry. It does not exercise repeated unknown outcomes, and its count/list closure checks call test-only helpers rather than HTTP, gRPC, MCP, CLI, or an SDK.
- FIND-5 / `bench:capacity` and `mise run gate` are deferred by user direction and are not domain-review gaps.

## Overall result

**FAIL** — `FIND-AUDIT-OUTBOX-11` is not closed under revision 3. The retained event ID is persisted correctly, but production audit reads still expose duplicate decisions, and the persistent path does not enforce AC-009's maximum of one extra retained row.
