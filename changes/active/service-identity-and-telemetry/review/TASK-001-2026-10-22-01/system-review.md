# TASK-001 system resilience review

Result: **FAIL** — one reachable request-boundary defect can turn a removed selector into a committed deletion. No process-crash or new-baseline recovery regression was established.

## Immutable subject and authority

- Base: `f38dff80327bb771975e2cdd8fbed283ab145159`.
- Candidate: `0a29bae4f4ff20e98d2c7937f3943877ac31bc62`; HEAD was checked before writing this report and still matched.
- Inputs: approved revision 12 of `changes/active/service-identity-and-telemetry/spec.md`, TASK-001, its recorded implementation evidence, actual cumulative base-to-candidate diff, repository/nested contributor rules, architecture authority map, relevant Card identity/declaration/reference sections, security tenant-binding rules, and operations deployment/recovery authority.
- No sibling reports or completion summary were read. Source remained immutable. This report is the only file written by this reviewer.

The approved fresh/reset v0.3.0 baseline expressly excludes historical conversion, old-token compatibility, and old/new release-upgrade machinery (REQ-027, REQ-029, REQ-031). Consequently, the destructive schema contraction is assessed as a fresh-baseline change, not against an invented requirement to keep old replicas serving it. Operations still governs migration ownership, interruption and same-baseline replacement.

## Deployed paths and ownership

| Changed path | Deployed owner and dependencies | Evidence and effect |
|---|---|---|
| Declaration and version allocation | Shared logical `wyrd-server`, Postgres, artifact storage | `components/cards/service.rs:1172–1259` retains the existing `persist_node` workflow: canonicalize spec, acquire tenant/kind/name transaction lock, resolve/reuse version, write Card/relationships/artifacts/projected principal/bindings under the caller-owned transaction. The change removes space from that authority rather than creating a second resolver. `wyrd-sql/src/queries/cards/version_sql.rs:63` includes tenant/kind/name in the advisory lock key. |
| Registry exact/latest/list reads and deletion | HTTP and MCP through the same server service and `TenantConn` | `components/cards/routes.rs`, `service.rs`, SQL `get.rs`, `list.rs`, `relationships.rs`, `delete.rs`; predicates and projections are flat, with tenant isolation remaining forced RLS. Exact UID assertions remain checked by `get_card_by_ref_for`. There is a query-decoding gap described below. |
| Durable completion and cleanup | Existing server Card reconciler; Postgres claims and storage | `components/cards/reconciler.rs:20–129`, `service.rs:1834–1908`, SQL lifecycle/reconciliation queries continue to use exact Card UIDs and persisted lease/state. Removing space from `CardRow` and its SQL projections reaches reconciliation reads as well as normal reads. |
| Credential and hosted verification consumers | Same server auth/verification owners; registry/Postgres | `wyrd-auth/src/card_scope.rs:50–110` resolves flat refs through the existing bounded tenant walk. `verification/hosted/registry.rs:47–58` reuses authorized Card reads and classifies server failures as retryable, denials/bad references as terminal. The replacement SQL `verification_target` function and `CardRow` agree on their space-free shape. No additional worker, transport or dependency is introduced. |
| Pull and offline runtime | Existing `wyrd-client` graph pull/bundle and WyrdState owners, projected to SDKs/CLI | `cards/pull/graph.rs` removes space only from fallback aliases; `state.rs:1778–1798` still requires pinned version and exact UID when hydrating. SDK and CLI changes remove arguments and update call sites; they add no durable client behavior or new recovery owner. |
| Boot and schema installation | One-off migrator session, serving Postgres readiness checks | New migration `20261024000000_flat_card_identity.sql` contracts keys/columns and replaces the affected function. Existing migrations are unchanged. `wyrd-sql/src/lib.rs:90–116` runs SQLx migrations on the lease session; `wyrd-server/src/main.rs:294–313` applies Wyrd then Vala and verifies both. `wyrd-server/src/postgres.rs:32`, `wyrd-sql/src/postgres.rs:120–131`, and `schema_check.rs:56–97` prevent a candidate missing its required migration/checksum from serving. Workload-binding config and boot construction lose space together. |

The cumulative diff also reaches contract/schema declarations, authoring/config/loader validation, fixtures, example calls, test harnesses and the three SDK projections. These are consumers of the same registry contract; the diff does not introduce another deployment surface or change Scribe/Oracle/Forge ownership. Vala runtime changes in this range are reference-fixture adjustments; physical telemetry storage is outside TASK-001.

## Failure and recovery assessment

| Credible scenario | What fails / remains available | Surviving state and recovery | Assessment |
|---|---|---|---|
| Fresh migrator fails or loses its connection during contraction | Migration gate fails; the candidate cannot pass readiness without its migration | Existing dedicated migration lease is session-owned; SQLx transaction execution and ledger retain the committed schema boundary. Rerun the existing migrator after dependency recovery. No schema DDL was added to serving boot. | No new interruption owner or partial-ACK path found. Fresh fixture/schema evidence is recorded, but this review did not kill a migrator. |
| Postgres unavailable during flat declaration | Affected request returns mapped registry failure; no new code terminates the shared server | `persist_node` never commits; the declaration owner commits only after the graph/seed is persisted. Dropped `TenantConn` rolls back. Retry uses existing operation/idempotency authority and the new flat lock key. | Transaction ownership and failure propagation preserved. |
| Crash after Pending declaration commits, before upload/completion | Pending Card remains unavailable as Active; unrelated server capabilities continue | UID, artifact inventory, operation response and reconciliation eligibility survive in Postgres. Existing reconciler resumes by UID, with SQL-owned lease/deadline and bounded attempts. No space-bearing field remains necessary to decode the row. | Source-backed recovery path retained. Recorded server-module evidence covers storage retry and bounded dead-letter cases; it is not a new process-kill experiment. |
| Artifact store fails during deletion cleanup | Delete may already be committed and returns artifact-cleanup failure | `service.rs:2126–2130` commits the tombstone before cleanup; `finish_card_delete` retains cleanup state on failure, and the reconciler retries cleanup. | Existing integrity/cleanup behavior preserved. This also makes SYSTEM-001 consequential: malformed-selector deletion crosses this commit boundary. |
| Registry unavailable during hosted Agent/Prompt verification | The affected block returns retryable failure rather than terminating the process | Existing accepted caller remains attached to the resolver; after registry recovery the same exact identity and optional UID assertion is read through the same authorization owner. | No new unbounded retry loop or sibling-service outage found. |
| Replica replacement within the new baseline | Disposable process state disappears; durable registry and claims remain | Replacement checks migration/checksum/RLS before serving and uses the same flat identity/UID-backed durable rows. Bundles created by this baseline remain self-contained with exact UIDs. | No new process-local identity cache or restart-dependent mapping introduced. Old space-bearing bundles/tokens are deliberately excluded, not silently converted. |

## Material proposed finding

### SYSTEM-001 — INCORRECT: removed HTTP selector is discarded before a destructive operation

**Violated obligation:** TASK-001's explicit “Removed forms fail with actionable existing typed validation,” Scenario 3 negative HTTP flow, and spec AC-006/REQ-019 contract closure. This is a newly removed identity field, not a request for a general new strict-query product policy.

**Exact changed locations:** `crates/wyrd/wyrd-server/src/components/cards/routes.rs:629–641` (`DeleteCardRefQuery` and `CardRefQuery` lose `space` but do not reject unknown fields). The destructive route consumes the former at lines 611–624.

**Reachable producer-to-consumer path:**

1. An authenticated caller with the ordinary Card-write grant sends `DELETE /v1/cards/by-ref?kind=Prompt&name=flat-http&version=1.0.0&space=wrong` while that unreferenced Active Prompt exists in the caller's tenant.
2. Axum's `Query<DeleteCardRefQuery>` deserializes the known fields; Serde's default struct behavior discards `space` because this DTO lacks `deny_unknown_fields`.
3. `into_card_ref` constructs a valid flat CardRef; `delete_card_by_ref_http` authorizes that new selector and calls the service.
4. `service.rs:2120–2130` invokes `soft_delete_card_by_ref`, whose SQL in `wyrd-sql/src/queries/cards/delete.rs:174–192` selects by kind/name/version and enters the existing soft-delete lifecycle. The service commits before post-commit artifact cleanup.

**Observable system consequence:** A form required to fail instead returns a successful deletion and can remove artifact objects. A retry/restart cannot restore those objects through the normal reconciler: it completes cleanup of the committed deletion. For a Service/Agent, existing deletion also changes its projected account status, but a standalone Prompt already proves the defect without invoking TASK-002 behavior. No cross-tenant bypass is alleged.

**Sibling consumers and existing owner:** GET `/cards/by-ref` has the same root decoding defect in `CardRefQuery`; version-list query decoding also accepts the removed key. The list endpoint's shared `ListCardsRequest` already uses `#[serde(deny_unknown_fields)]`, as do CardRef and metadata contracts. MCP uses typed CardRef rather than these private HTTP DTOs; SDKs/CLI already remove/refuse their old arguments. Fix the HTTP query boundary once; do not add guards to SQL, the reconciler or artifact cleanup.

**Smallest testable correction:** Reject removed query fields in the existing route query DTOs using the repository's strict deserialization mechanism, retaining the existing authorized flat request behavior, error projection, and deletion lifecycle. Apply the same refusal to the by-ref read/delete and version-list query shapes so no sibling HTTP query silently accepts `space`. Reuse the existing Card HTTP test/server fixture. Prove that removed `space` produces an actionable validation response, then read the original UID and verify it remains Active; for an artifact-bearing case verify no cleanup was requested. Also assert valid flat read/delete behavior still works. Do not introduce compatibility aliases or service-wide failure.

**Proof gap:** `pg_card_declaration_route.rs:4680–4795` tests a removed path segment and rejected metadata on declaration; it never sends a removed query selector to the still-existing by-ref route. The task's recorded “GET query structs ignore” limitation does not authorize this omission and misses the destructive DELETE sibling.

Focused closure command, using the existing test extended with these negatives:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_card_declaration_route::flat_card_http_contract)'"
```

## Proof limits and verdict

The recorded SQL, server, SDK and CLI results were assessed against actual test bodies and callers; they were not rerun here. This reviewer ran no Cargo command, dependency outage experiment, crash/restart test, or destructive migration/reset. Existing fault-oriented source evidence is stronger than a healthy-path-only argument for preserved reconciliation, but it does not establish production release qualification.

No additional recovery redesign, old/new rollout mechanism, backfill or speculative hardening is required by this report. **FAIL** is driven solely by SYSTEM-001's reachable malformed-selector mutation.
