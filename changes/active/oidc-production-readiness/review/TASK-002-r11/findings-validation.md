# TASK-002 R11 Structured Ponytail Findings Validation

## Immutable subject

- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `bae424cc647dad4be80e7debec976d0b7b3e4cf8`
- Latest fix: `454bdb90ef8eedca4bbd4754e083d41f15d0c4ae..bae424cc647dad4be80e7debec976d0b7b3e4cf8`
- Authority: approved specification revision 5, original TASK-002, both R10 remediation packets, `AGENTS.md`, `architecture/agent-rules.md`, and applicable Wyrd design and security documents.

The candidate remained at the stated commit during validation. No `.codegraph/` index exists. I read all four complete Wave 1 reports, the cumulative diff, the latest fix diff, the cited full functions and their callers, and the R10 verdict and finding ledger. Fresh `git diff --check` passed. This was a static review; recorded test results were not rerun.

## Wave 1 disposition

| Proposal | Disposition | Stable ID | Independent validation |
|---|---|---|---|
| `TASKREV-R11-1`, `SEC-TASK-002-R11-1` | **REVISED** | `FIND-TASK-002-25` | One shared defect: the keyless activation guard returns after the handler evaluates permission but before the allowed decision is appended. The existing activation transaction and committed-refusal path suffice. A second audit sink or separate transaction is unnecessary. The existing journey already injects activation-audit failure; the new keyless test needs the missing-row assertion. |
| `STD-R11-1` | **CONFIRMED** | `FIND-TASK-002-26` | The materially changed resolver module still claims `wyrd-server`, a stored `PgPool`, and an ephemeral commit reference. Its actual owner is `wyrd-auth`, and both resolver fields are `WyrdPostgres`. |
| `STD-R11-2` | **REVISED** | `FIND-TASK-002-27` | The touched, live server audit module still passes raw `PgPool` through standalone audit signatures. The active human instruction allows no raw-pool exceptions, so the older origin of those signatures does not excuse this candidate. Reuse the existing `ValaPostgres::tenant_conn` owner and canonical append; extend the existing boundary check, with no new audit path. |
| Tenancy/data domain | Validated empty proposed ledger | — | No distinct tenancy, persistence, or concurrency finding arose from its report or the retained corrections. |

## Independent path and simplification checks

### Activation audit

`identity_router` mounts `activate_candidate`; the handler calls `decide` and passes its returned `AuditEvent` to the sole production call of `HumanConnections::activate` (`identity.rs:37-46,303-314`). `decide` uses `authorize_recording_denial`: denial commits immediately, while allowance is returned undurable (`audit/mod.rs:214-253`). The new `require_keyring()?` at `connections.rs:469` runs before `begin_locked` at `:471`; `begin_locked` is the existing slot-lock and canonical audit append owner (`:659-670`). Thus an authorized keyless request reaches a real permission evaluation and a `sealing_key_missing` refusal with no audit row. The keyless test at `:1322-1352` proves only the refusal reason.

The sibling connection writers `put_candidate`, `stamp_candidate`, `deactivate`, and `remove` all enter through `begin_locked` and use `commit_refusal` after an audited refusal (`connections.rs:304-349,409-434,527-563`). The same `activate` body uses it for stale candidate, missing test, and invalid recovery key (`:479-496`); `commit_refusal` commits the already appended decision and returns the refusal (`:1009-1017`). `recovery_key_authorizes` is a later, separate permission evaluation and must remain untouched (`:966-1007`). The handler's `refuse_after_decision` records an allowance standalone for request parsing before a tenant transaction exists (`identity.rs:81-99,180-195`); using it here would split activation from its established transaction owner.

Ponytail choice: move the keyring check inside the existing `begin_locked` transaction and pass its error to `commit_refusal` before reading candidate or recovery key. This keeps keyless activation fail closed, preserves the one transaction and lock order, and adds no helper or audit path. The existing real-server journey injects failure for `identity.oidc.candidate.activate` into `vala.audit_staging` and proves no promotion (`identity_e2e.rs:2291-2334`); it already proves that `begin_locked` cannot proceed on audit failure. Extend the focused keyless Postgres test to assert one redacted allowed decision and no promoted connection. No second failure-injection harness is needed unless the changed path does not use `begin_locked` as specified.

### Resolver module contract

The module is compiled in `wyrd-auth` and has live consumers: `PgIssuerResolver::new` is used at server boot and in auth paths; `PgWorkloadBindingResolver::new` is used at server boot and by the real-server fixture (`boot/mod.rs:1571-1575`, `wyrd-testing/src/server.rs:4308-4312`). Their full resolver bodies acquire `TenantConn` through `self.postgres.tenant_conn` (`pg_resolvers.rs:96-189,226-253`). The cumulative diff replaces the workload resolver's `Arc<PgPool>` with `WyrdPostgres`, removes its `PgPool` import, and changes its acquisition path. Yet module rustdoc at `:3-9` still says the implementations live in `wyrd-server`, hold `PgPool`, and call “commit 03's” queries. The `PgPool` intra-doc link no longer has its imported target. This is a materially modified module with an inaccurate ownership and isolation contract under `AGENTS.md` §16 and the source permanence rule in `architecture/agent-rules.md`.

Ponytail choice: replace only the stale module-rustdoc sentences with the actual `wyrd-auth` owner, `WyrdPostgres` handle, optional issuer sealing key, and tenant-scoped RLS reads. Remove the ephemeral commit and `F02` markers while editing the same block. The existing item docs already describe each resolver accurately; no new abstraction, code change, or test is needed.

### Raw-pool scope audit

The repository rule and active human direction are absolute: production domain code must not receive a raw `PgPool`. `wyrd-server/src/audit/mod.rs:145-186` still takes `&PgPool` in `record_audit` and owned `PgPool` in `record_audit_owned`, then calls `TenantConn::acquire` itself. This module is in the cumulative diff, albeit for `audit_event_unauthenticated` rustdoc only. Its signatures are older than the base, and `components/admin/identity.rs` is unchanged, but the live identity connection refusal and shared denial paths still use `record_audit` (`identity.rs:89-98`, `audit/mod.rs:203-254`). Other live callers include cards, Bifrost, gateway, and query admission; the owned form has two query-service calls (`query/service.rs:167-219`). The specific rule violation is capability propagation, not a claim that current RLS acquisition is missing.

The R10 human-directed packet §1 scoped its raw-pool implementation work to touched auth, boot, and SQL paths, but it cannot override the active human “no exceptions” constraint for a touched, production, live module. The existing `ValaPostgres` owner is cloneable and already exposes `tenant_conn` (`vala-sql/src/postgres.rs:62-69,130-146`), so both standalone audit forms can acquire a scoped transaction without taking a raw pool parameter. Keep the existing canonical `append_audit` and transaction commit; preserve the owned transport future's `Send` behavior and gateway's tracked non-blocking task. The existing `check:tenant-isolation` can cover this module, so no new checker or pool wrapper is needed.

`components/eval/resolver.rs:92-104` is unchanged in both diffs, and `resolve_card_for_tenant` has no production caller in the current tree. It is a separate existing violation, not an exception to the rule, but it is outside this finding's reachable TASK-002 correction boundary. Do not fold an unrelated eval migration into the audit correction.

## Final deduplicated finding ledger

### FIND-TASK-002-25 — Keyless activation drops an evaluated allowed decision

- **Wave 1 IDs:** `TASKREV-R11-1`, `SEC-TASK-002-R11-1`
- **Status:** REVISED
- **Classification:** INCORRECT / VIOLATION
- **Violated obligation:** Spec REQ-017 and `AGENTS.md`/`architecture/agent-rules.md` require each evaluated authorization decision, allowed or denied, to commit through the canonical audit path before the authorized operation proceeds or refuses.
- **Exact location:** `crates/wyrd/wyrd-auth/src/connections.rs:469-471`; caller `crates/wyrd/wyrd-server/src/components/admin/identity.rs:303-314`.
- **Evidence:** `decide` returns an undurable allowed event; `require_keyring()?` returns before `begin_locked` appends it. The live keyless test does not inspect `vala.audit_staging`.
- **Observable consequence:** an authorized administrator's activation attempt on a keyless deployment is refused safely but has no canonical permission-decision record.
- **Decision-complete correction:** in `HumanConnections::activate`, enter `begin_locked(tenant, decision)` before the keyring check. On `require_keyring` failure, use existing `commit_refusal(conn, refusal)` before any candidate or recovery-key read. Keep the same `sealing_key_missing` response, do not promote a connection, leave the handler denial path and recovery-key authorization untouched, and add no audit sink or transaction.
- **Focused closure proof:** extend `activation_without_a_sealing_key_is_refused_for_a_secretless_provider` to inspect the tenant's canonical staged audit row for one redacted allowed `identity.oidc.candidate.activate` decision and verify no connection was promoted. Reuse the existing activation audit-failure journey as proof that an append failure refuses before promotion. Run the exact focused Postgres test through the repository-managed wrapper, then the relevant identity gate plus required format and lint checks.

### FIND-TASK-002-26 — Resolver module rustdoc names the wrong owner and dependency

- **Wave 1 ID:** `STD-R11-1`
- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Violated obligation:** `AGENTS.md` §16 requires materially modified Rust module documentation to accurately explain ownership and invariants; `architecture/agent-rules.md` forbids ephemeral task or commit references in source.
- **Exact location:** `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:3-9`.
- **Evidence:** the module claims `wyrd-server`, `PgPool`, and “commit 03”; the changed production resolver stores `WyrdPostgres` and acquires tenant transactions through it (`:195-253`). The removed `PgPool` import also leaves the old intra-doc link without its target.
- **Observable consequence:** maintainers are directed to a nonexistent module owner and obsolete pool boundary at an RLS-sensitive resolver.
- **Decision-complete correction:** replace the module header's stale sentences with the actual `wyrd-auth` ownership, `WyrdPostgres`-backed tenant reads, and optional issuer sealing key; remove the ephemeral commit and `F02` labels. Preserve all executable code and the accurate resolver item docs.
- **Focused closure proof:** inspect the corrected header against both complete resolver implementations and confirm a documentation-only subdiff. Run `git diff --check`, `mise run fmt`, and `mise run lints`; no behavior test is justified for a comment correction.

### FIND-TASK-002-27 — Live standalone audit functions propagate raw SQL pools

- **Wave 1 ID:** `STD-R11-2`
- **Status:** REVISED
- **Classification:** VIOLATION
- **Violated obligation:** The active human instruction permits only scoped SQL capabilities and no raw SQL pool exceptions; `architecture/agent-rules.md` also bans `PgPool` in library function signatures and fields. The touched, live audit module must comply even though these two signatures predate the base commit.
- **Exact location:** `crates/wyrd/wyrd-server/src/audit/mod.rs:145-186`, with the live identity refusal caller at `crates/wyrd/wyrd-server/src/components/admin/identity.rs:89-98` and shared denial callers at `audit/mod.rs:203-254`.
- **Evidence:** `record_audit(&PgPool, ...)` and `record_audit_owned(PgPool, ...)` carry raw pool capabilities across the audit boundary. The identity refusal and permission-denial calls pass `state.postgres.vala_pool()`. Other callers pass the same raw handle; the existing boundary check does not scan this module.
- **Observable consequence:** production callers can select and pass an arbitrary SQL pool into the canonical audit operation, bypassing the intended typed acquisition boundary. A passing current audit test does not establish the required capability shape.
- **Decision-complete correction:** preserve one canonical audit append and the present standalone transaction semantics. Make `record_audit` and its owned transport form acquire their `TenantConn` through the existing `ValaPostgres::tenant_conn` owner, pass that existing owner rather than `PgPool` at every caller, and keep the owned form's `Send`/tracked-task behavior. Do not alter gateway invocation's non-blocking audit policy or audit publication. Extend `check:tenant-isolation` to reject raw-pool signatures in the server audit module; do not add a second check or wrapper type. Leave the unrelated dormant eval resolver out of this correction.
- **Focused closure proof:** inspect the final audit module and all `record_audit`/`record_audit_owned` callers for absence of raw-pool parameters and `vala_pool()` forwarding. Run `mise run check:tenant-isolation` and confirm its audit-module rule rejects an injected raw-pool signature; run the identity connection refusal/audit journey and the focused gateway/query audit checks covering the owned and tracked paths, plus required format and lint checks.

## Prior finding closure and limits

`FIND-TASK-002-1` through `FIND-TASK-002-22` remain closed at the owners recorded in the R10 validated ledger; the latest fix retains exact subject/claim checks, tenant RLS and scoped SQL resolution, callback/issuance/refresh/revocation serialization, and redacted audit paths. `FIND-TASK-002-23` and `FIND-TASK-002-24` are visibly closed by the corrected token-contract test rustdoc (`token.rs:225-229`) and four-surface auth-routes module rustdoc (`routes.rs:1-2`). The R10 human-directed additions are reviewed as approved work, not assigned retrospective `FIND-*` IDs. The three retained findings use existing owners and do not require a new product, public API, architecture, security, concurrency, resource-ownership, or persistent-data decision; no specification revision is needed.

**Result: VALIDATED WITH THREE FINDINGS — FIX_REQUIRED.**
