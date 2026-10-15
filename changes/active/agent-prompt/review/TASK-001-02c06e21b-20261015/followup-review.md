# Focused registration/version/lifecycle follow-up

Result: **RESOLVED**.

Immutable subject: `c1aac2d132b19848ca1940c3907e1fa68a8d08a5` → `02c06e21bd5fd613341935a843fabe5350ea0a9d`; observed HEAD remains the candidate. This report investigates only INV-REV-001, DATA-001 and DATA-002 and the conflicting conclusion that reuse of registry owners already establishes these guarantees. It is a discovery follow-up, not an acceptance verdict or independent final validation.

## Review Findings

### Critical

None established in this scope.

### Important

- **INV-REV-001 remains reachable:** `crates/wyrd/wyrd-server/src/components/cards/service.rs:1130–1140` normalizes an inline Agent against the current Prompt line before checking an existing exact Agent pin. Reapplying an unchanged pinned Agent after the Prompt line advances can falsely fail with spec drift. Preserve the exact existing Agent's Prompt relationship when validating an unchanged exact-pin submission; retain normal version selection for new Agent versions and genuine spec-drift rejection.
- **DATA-001 remains reachable:** the same changed block turns a reused Prompt outcome into an Agent dependency without the existing Active-reference row lock. A concurrent deletion can commit first, after which registration can commit an Agent pointing at a deleted Prompt. Apply the existing exact UID/Active `FOR SHARE` authority to reused generated dependencies inside this transaction; preserve transaction-owned fresh Pending dependencies.
- **DATA-002 remains reachable, with a corrected reproduction:** repeated generated Prompt selection in one transaction cannot see its own Pending rows through the Active-only auto resolver. Two exact versions of one Agent in a Service composite can attempt the same Prompt version and roll back with registry unavailable. The authored-Prompt form must use `0.1.0` on an empty line to demonstrate the claimed unique collision: the report's `1.0.0` seed assertion is incorrect. Account for a matching Prompt selection already made by the current registration transaction through the existing identity/version owner, without widening public Pending visibility or changing lifecycle policy.

### Suggestions

None; no optional refactors proposed.

## Authority and inspected paths

The governing obligations are approved spec revision 2 REQ-002 (existing composite transaction/lifecycle), REQ-003 (normal Prompt line, exact identity reuse, immutable Agent pins, authored-line collisions), REQ-005 (atomicity, conflict errors and idempotency), and INV-003 (reuse existing owners). Original task scenarios 1–3 additionally require unchanged registration reuse, concurrency, exact-pin conflict preservation, and normal locks/rollback for generated children. AGENTS.md, agent rules, protocol/doctrine changes, spec-driven development, maintainer style and architecture patterns supply the ownership and transaction constraints. Nothing here requires another version store, graph, lifecycle owner, deletion policy or public contract.

CodeGraph was consulted before locating/expanding code. Reviewed the cumulative diff, approved spec/task and relevant connected owners, including:

- `service.rs::register_card`, `RegistrationWriter::write`, `persist_node`, `resolve_existing`, `inline_prompt_submission`, `find_submission`, replay, upload initialization, completion and deletion entry points.
- `resolve.rs::EffectiveSpecs::resolve`, sibling identity and binding; spec graph build, canonical topo ordering and Service composition validation.
- SQL version resolution/line locking, ordinary exact lookup, insert, relationships/Active recheck, delete, TenantConn lifecycle, and identity/relationship migrations.
- Shared SDK registration saga and hydrated state/consumer diffs; added HTTP and Rust registration proofs and language navigation/fixture changes.

## 1. Exact-pin reapply after line advance

The ordinary SDK path genuinely enters a new server registration operation: `Cards::register_from_path` loads input and calls `register` (`crates/shared/wyrd-client/src/cards/handle.rs:256–282`); `cards/saga/mod.rs:58–64` mints an idempotency key per invocation. Same-key replay therefore does not make the unchanged-file reapply case unreachable.

For a concrete valid sequence, register Agent `desk@1.0.0` with inline native Prompt A; its unversioned generated Prompt seeds at `0.1.0`. Register `desk@2.0.0` with inline B; the child changes on its own line to `0.1.1`. Both registrations can finish normally and leave the old Agent immutable. Reapply the first exact-pin submission with a fresh key:

1. `service.rs:1127–1134` always builds/persists the unversioned child before the parent. `inline_prompt_submission:1574` sets no child version intent.
2. `version_resolve.rs:83,103–119` compares A only with latest stable B. It selects fresh `0.1.2`, not original A's `0.1.0`.
3. The Agent spec is serialized with this new UID-bearing Prompt ref (`service.rs:1134–1140`). `persist_node:1187–1190` hashes that normalized spec.
4. Exact parent lookup finds original `desk@1.0.0`, and `resolve_existing:1410–1426` rejects the different hash. The unchanged authored file thus receives spec drift.

The transaction drops before `write:1163` commits, so the attempted extra Prompt and new operation roll back. No committed orphan is claimed. Later reads of the old Agent still work; the defect is reapplying it. The tests immediately repeat A before B, then read A after B; they do not exercise this reapply order.

**Correction boundary:** the existing server registration owner must identify an existing exact Agent pin before choosing a replacement inline child identity for that parent. Validate the submitted inline body against the existing pinned Prompt and normalize an unchanged body to the existing exact ref; validate the remaining parent spec normally. Reuse existing exact lookup, transaction/lock and hash mechanisms. A new Agent version still uses ordinary Prompt auto selection. Do not search all historical Prompt hashes globally or change ordinary Prompt dedup rules. Any reused registered dependency retains the Active-reference protection discussed below.

**Focused proof:** exact A@1 → B@2 → unchanged A@1 under a fresh key succeeds with original Agent/Prompt UIDs and no additional Prompt version. Changed inline content under A@1 still conflicts and leaves no partial writes.

## 2. Generated reuse versus deletion

The ordinary external-ref guarantee is stronger than version-line serialization. `write:1100` calls `recheck_active_card_refs` only for `plan.external_refs`, which were collected before materialization. The inline generated Prompt was not an authored ref, so it is absent. `relationships.rs:17–26,36–68` explicitly requires exact identity plus UID plus Active and takes `FOR SHARE` until the referencing transaction commits.

A reachable interleaving uses an independently authored matching Active `desk-prompt`, with no existing inbound Cards:

1. Delete obtains that row's `FOR UPDATE` lock (`delete.rs:19–26,82`) and sees zero inbound references (`105–125`). It can update status to deleted while keeping its transaction uncommitted.
2. Inline `desk` registration selects the previously committed Active version through ordinary MVCC reads (`version_resolve.rs:48–63`, `get.rs:20–27,147–165`). Neither takes a lifecycle row lock. Its advisory version lock (`version_sql.rs:63–85`) is irrelevant to delete: the actual delete service/SQL path takes no such advisory lock.
3. `persist_node` returns the reused Prompt; the Agent stores its ref. Relationship insertion (`relationships.rs:87–103`) checks a tenant/UID FK, not status. If the FK check waits on delete's lock, delete has already observed no incoming edge and can commit; the retained deleted row still satisfies the key.
4. Registration can then commit the Agent and edge. SQL migration `20260601000018_card_relationships.sql:23–25` has no lifecycle predicate. No later relation status trigger was found. This does not depend on physical row deletion or cross-tenant access.

Post-commit handling does not repair this. The reused outcome was Active when selected; upload initialization does not recheck outbound targets, and completion only iterates Pending outcomes (`service.rs:819–854`). `complete_card_inner:1883–1949` validates the completing Card/manifests/blob, not its dependencies. The Agent can become Active while Prompt public lookup excludes the deleted UID (`get.rs:12–27`), leaving an unhydratable graph.

If registration took the existing `FOR SHARE` lock first, delete would wait and then see the committed inbound reference; if delete won first, the Active recheck would wait and refuse the target. The missing lock is consequently material, not a request to replace independent deletion with ownership/cascade behavior.

**Correction boundary:** protect a generated dependency reused from a previously committed row using `recheck_active_card_refs` on this same TenantConn before writing the parent. Fresh children owned by this transaction remain Pending and must not be passed through an Active-only check. Do not change the global deletion policy or move activation before the graph commit.

**Focused proof:** hold the authored matching Prompt's delete row lock and establish the zero-inbound delete decision before materialization; allow delete to commit while registration proceeds. Registration must refuse/roll back, or acquire protection early enough to make deletion refuse. It must never leave a live Agent pointing to the deleted UID. Use distinct keys and a real Postgres seam.

## 3. Multiple same-line selections before activation

Valid graph structure does not exclude this case. `graph/mod.rs:136–154` keys siblings by full CardRef identity; `reference.rs:45–54,228–234` includes version. `resolve.rs:994–1017` retains distinct exact versions; the existing `sibling_identity_includes_version` test at `:1224` confirms that contract. A Service may reference `desk@1.0.0` and `desk@2.0.0` under distinct aliases. Composition validation (`composition.rs:54–87,426–435`) rejects duplicate aliases and peer-only kinds, not multiple Agent versions. Both Agent specs can contain the same valid native inline body.

For this composite on an empty derived line:

1. Leaves are processed before Service (`topo.rs:74–130`), with canonical ready ordering (`133–139`). The two Agent identities are distinct. Each `find_submission` resolves its authored exact version (`service.rs:1605–1620`).
2. First Agent materialization inserts generated `desk-prompt@0.1.0` Pending (`service.rs:1209–1221`). It inserts the first Agent and edge too. No post-commit upload/completion has run yet: the only graph commit is at `1163`.
3. Second Agent materialization reacquires the same transaction-scoped line lock, which does not block its own transaction. `latest_stable_in_line` explicitly filters `status = 'active'` (`version_resolve.rs:48–50`); its own Pending child is visible to the transaction but excluded by that predicate.
4. Resolver again selects `seed_version()` = **`0.1.0`** (`wyrd-semver/src/lib.rs:30–31`). The unique live identity index includes Pending (`20260601000014_card_registration_operations.sql:60–62`). Second child insert fails; `register.rs:295–297,348–350,559–561` maps it to registry unavailable, and the whole composite rolls back.

An equally valid authored-Prompt form has Service sibling components for `desk-prompt@0.1.0` and inline Agent `desk@1.0.0`, with identical canonical native Prompt content. Prompt precedes Agent because CardKind derives ordinal ordering and declares Prompt before Agent (`envelope.rs:482–503`), and canonical topo selection uses that identity ordering. The Pending authored Prompt is filtered out and the generated allocation collides at `0.1.0`.

**Revision to DATA-002:** its specific initially empty authored `desk-prompt@1.0.0` example does not establish this unique collision: normal auto resolution seeds `0.1.0`, not `1.0.0`. Retain the shared Pending-selection diagnosis using the two-Agent example or authored `0.1.0` example. Do not require all differently pinned authored siblings to collapse to one identity; authored exact-version intent remains protected. With an older Active row, a collision likewise requires selection of the same next version; it is not unconditional for every same-name sibling.

**Correction boundary:** make current-transaction Prompt selections participate in the existing registration owner's identity/hash resolution so repeated identical materialization reuses the already selected exact child, including an authored child at the selected identity. Preserve exact-pin content conflicts, separate names/tenants, normal future version policy, and Pending → post-commit lifecycle. Reuse current outcomes and canonical hash/identity authority instead of a separate graph/version store or globally exposing arbitrary Pending rows. Correcting this one repeated producer should cover both composite shapes.

**Focused proof:** empty-line Service composite with two pinned same-name Agent versions and identical inline content succeeds with one Prompt UID; both Agents pin it and hydrate. A same-request authored Prompt at `0.1.0` plus inline Agent succeeds/reuses the selected identity. Preserve conflicting exact-pin refusal and atomic rollback.

## Open Questions

None needed to resolve these three reachability questions. Independent Ponytail validation still owns final disposition and deduplication of proposed findings.

## Verification Notes

Static source review only; no builds, tests, fault injection, source edits, baseline checkout or extra agents. Existing A/B registration proofs and recorded green evidence were inspected; none proves the reapply-after-advance, delete interleaving, or repeated same-line selection within one transaction. The positive owner-reuse conclusions remain accurate for tenant scope, atomic commit, durable response seed, normal activation/reconciliation, local hydration and old-row immutability; those properties do not supply the three missing state/lock conditions above.

No new source-local finding beyond the three supplied hypotheses was added. Only this report was written.

Skipped runtime reproduction and broader suites. Residual risk: PostgreSQL interleaving proof is source-based and needs the coordinated focused regression test during remediation.
