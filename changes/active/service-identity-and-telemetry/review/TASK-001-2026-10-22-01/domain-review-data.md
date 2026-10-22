# TASK-001 persisted registry data review

Result: **PASS**. No material proposed findings in this domain.

## Immutable subject and boundary

- Base: `f38dff803`; candidate: `0a29bae4f4ff20e98d2c7937f3943877ac31bc62`.
- `git rev-parse HEAD` still returned that candidate during the review.
- Approved specification: `changes/active/service-identity-and-telemetry/spec.md`, revision 12; original task: `tasks/TASK-001-flat-card-identity.md`.
- Reviewed boundary: registry identity, persistence, tenant isolation, declaration transactions, version-line coordination, dependency lifecycle, immutable Card storage and retained verification projections. Account authorization and the future stable-account behavior are outside this domain review.
- Inspected the cumulative range rather than a last-fix diff, including the changed-file inventory and persistence-related production and test hunks. Followed affected callers into contracts, graph canonicalization, client/state/pull, server routes/resolution, verification cache/registry, fixtures and SQL migrations. No sibling reports were read and no source was changed.

## Authority and source coverage

| Boundary | Authority | Source inspected and conclusion |
|---|---|---|
| Flat durable key | Spec REQ-018–020, REQ-029; TASK-001 Scenario 2; `architecture/wyrd-design.md` Card identity/reference and registry lifecycle sections | `migrations/20261024000000_flat_card_identity.sql:9` drops the space-dependent indexes and column and reconstructs identity, latest and spec-hash indexes on tenant/kind/name[/version]. The existing lifecycle predicate on the identity index is preserved. `row_types/cards.rs`, `queries/cards/{get,list,field_resolver,version_query,version_resolve}.rs` and their callers agree on the flattened columns and binds. No replacement namespace is introduced. |
| Migration baseline | Spec REQ-027, REQ-029, REQ-031; SQL AGENTS migration immutability; operations deployment/migration authority | The range adds one forward migration and does not modify an applied SQL migration. It contains no old-identity collision mapping, Card blob conversion, hash rewriting, dual read or alias. Dropping/recreating `verification_target` is required by its changed result type. The change is assessed for the expressly resettable baseline; it provides no release-upgrade compatibility guarantee for populated pre-change environments. |
| Tenant separation | INV-001; `architecture/wyrd-security-posture.md:657` Postgres tenant binding; SQL AGENTS | The migration leaves forced RLS policies and tenant-qualified source/target relationship FKs from `20260601000018_card_relationships.sql` in place. Tenant paths retain `&mut TenantConn`. Exact reads and line scans use RLS, while the unique key and lock namespace both retain the tenant UUID. `TenantConn` continues to own begin/binding and rollback-on-drop; query helpers do not commit. |
| Version coordination | TASK-001 preserves version allocation and deduplication | `queries/cards/version_sql.rs:63` takes the existing transaction advisory lock over tenant/kind/name; `service.rs:1200` still acquires it before exact lookup or allocation. `version_resolve.rs` retains stable-only selection, the submitting operation's own Pending rows, scoped bounds, hash/artifact comparisons and overflow handling. Latest/version queries retain kind and name predicates. The removal is applied to the existing workflow rather than a second allocator. |
| Declaration atomicity and replay | TASK-001 preserves transaction, artifact and idempotency behavior; design registry lifecycle | `service.rs:1066` retains one writer transaction for the operation, Card rows, manifests, edges, principal projection and verification bindings. `declare.rs:124` hashes the canonical submitted payload and artifact manifest hashes; `service.rs:1594` uses `graph::canonical_order`, now sorted by kind/name/version. Reservation remains keyed by tenant, subject, actor and caller key. A losing reservation rolls its transaction back before replay polling. No old stored hashes are rewritten. |
| Exact dependencies and lifecycle | TASK-001 Scenario 2 and preservation constraints | `resolve.rs:210` resolves external references before writing; `relationships.rs:35` rechecks exact kind/name/version **and expected UID**, requires Active and holds `FOR SHARE` until the declaration commits. `service.rs:1087` performs that check before bookkeeping writes. `relationships.rs:65` persists UID-bearing flat edges within the same transaction. Inbound projections retain the tenant-qualified join. `delete.rs` retains row locking, inbound-reference refusal, lifecycle restrictions, tombstone retention and idempotent cleanup state. |
| Immutable blobs, artifact integrity and consumers | TASK-001 artifact integrity and UID preservation | `service.rs:2620` builds the immutable envelope without space; `hydrate_card` and hash verification retain UID/name/version/kind, canonical spec hash, manifest hash and tenant/Card storage-path checks. Public exact reads retain the optional UID assertion in `routes.rs:157`. Client reads, graph pull and BundleReader drop only the removed dimension while retaining pinned version/UID and integrity checks. |
| Adjacent persisted projections | TASK-001 identity consumer closure, TASK-002 behavior exclusion | `auth_projection.rs` and `auth/service_accounts.rs` remove only the space projection/bind; the replacement binding check still makes all remaining Card-derived columns travel together. `drift_baselines.rs:121` emits flat structured references. `verifier_executions.rs:497` freezes `Kind/name@version` in dispatch context. `verification_target` retains its invoker/RLS behavior and tenant-binding check. No new durable owner or alternate identity resolver was added. |

## Proof assessment and limits

The original task records the focused flat registry test as 1/1 and the full SQL all-targets lane as 205/205. It also records public server declaration/read suites and SDK journeys. These are **recorded implementation evidence, not independently reproduced results**. No Cargo or database command was run in this review; the orchestrator explicitly requested completion from source and recorded evidence without another data-lane run.

The inspected tests establish the following properties rather than merely asserting that calls return:

- `pg_cards_declare::flat_registry_identity_is_tenant_and_kind_scoped` inspects the schema for removed columns, refuses a duplicate in one tenant, permits a same-name different kind and the same identity in another tenant, and verifies exact read UIDs. Its duplicate result is the low-level insert primitive's existing unavailable mapping; public registration semantics are owned by `resolve_existing`, which still maps incompatible exact content to spec drift before insertion.
- `relationship_rows_preserve_exact_target_identity`, `relationship_recheck_blocks_target_lifecycle_race` and `exact_delete_enforces_inbound_references_and_is_idempotent` retain real Postgres checks of edge values, row-lock exclusion, inbound-reference refusal and tenant parity after flattening. The lifecycle-race test proves write-time locking; its direct SQL update after release is not evidence that public deletion bypasses inbound checks.
- `pending_card_and_manifest_share_registration_transaction` and the subject/actor replay test inspect durable operation/Card/manifest state and replay scoping.
- `pg_card_declaration_route::concurrent_same_key_resolves_to_one_declaration` races real authenticated routes and inspects one operation and one Card. The public replay/content-conflict, absent/inactive/foreign dependency, wire-order permutation and flat HTTP tests exercise the production owners described above.

No load, crash-injection or populated-old-database upgrade rehearsal was performed. Such an upgrade is outside the approved fresh-baseline requirement. Static inspection cannot independently establish the recorded runtime counts, but no source-backed data/concurrency regression was found in the changed boundary.

## Proposed findings

Empty. No correction is requested from this domain review.

**Overall: PASS**, scoped to the persisted registry identity/concurrency/durability boundary above.
