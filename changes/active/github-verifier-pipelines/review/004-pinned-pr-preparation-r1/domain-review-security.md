# Security and tenancy domain review

Result: **PASS** within Task 004's source-preparation and cache-custody boundary. No material proposed findings.

## Immutable subject and method

Wyrd base `81aca5479`; candidate `359a19f41597b8cfdc673373409b0d96e8298447`.
Library base `d8c772de4248ce0130f79ec3b5fefda518b1431a`; candidate
`d380f57fa53429eaa4f3e7272ce651e1825a8b38` in
`/home/thorrester/Documents/GitHub/bohmian-code-review`.
Both checked-out HEADs match these candidates. Final read-only Git checks found
no source difference from the recorded candidates in the reviewed source trees.
Only this report was written. No runtime or mutation tests were run.

CodeGraph was consulted before Wyrd code navigation. The library has no root
`.codegraph/`; ordinary source navigation was used there. Read the original task,
specification, scope, task ledger as unverified claims, task-review skill,
spec-driven workflow, maintainer guide, testing map, root rules and SQL directory
rules. Governing security authority: `architecture/wyrd-security-posture.md`
Security principles, Trust boundaries, GitHub App association, Outbound
credentials, Tenant and data isolation / Postgres tenant binding. Also inspected
the changed Verifier cache-custody and Bifrost durability sections.

The scope records revision 6 as draft and explicit user direction authorizing
Task 004's storage amendment. This domain result assesses that directed storage
behavior; it does not promote draft specification text to formal approval. The
orchestrator retains the authority-reconciliation decision.

## Boundary and source coverage

| Contract / obligation | Producer → consumer evidence | Verification source inspected | Result |
|---|---|---|---|
| W6 / INV-001: tenant, installation and stable repository scope | Library `storage.rs:53` constructs `tenants/<tenant>/installations/<id>/repositories/<numeric-id>`. Wyrd `review_cache.rs:177-199` validates Wyrd tenant IDs and checked BIGINT conversion. `CodeReviewMetadata::tenant` opens the existing `ServerPostgres::tenant_conn` for each statement and commits it. SQL statements bind installation/repository and rely on RLS for tenant. | SQL `pg_code_review_cache.rs:337-381` asserts a different tenant sees no pointer/context, deletes nothing, claims nothing, and cannot affect the owner's pointer/record. `:430-520` tests sibling installation/repository scopes and pointer correlation. | PASS |
| W2/W7: forced RLS and narrow operator authority | Migration `20261111000000_code_review_cache.sql` makes all three tables tenant-owned with tenant-leading composite keys, tenant foreign keys, ENABLE/FORCE RLS, USING/WITH CHECK tenant predicates and operator policy restricted TO CURRENT_USER. Production query signatures use `TenantConn`; only `expired_code_review_contexts` takes `OperatorPool` and only enumerates. Existing `ServerPostgres`, `TenantConn`, `OperatorPool` remain connection owners. | Migration RLS metadata assertions and SQL isolation assertions inspected; adapter expiry path inspected end to end. | PASS |
| W1/W8: object paths retain scope and cannot reach Card/evidence namespaces | `CodeReviewObjects::path` strips the literal library `tenants/` envelope, parses its tenant as `DataTenantId`, retains the installation/repository suffix, and calls existing `tenant_path` ownership. `tenant_path::code_review:148` rejects empty/dot/traversal/invalid segments and places bytes exclusively under `<tenant>/code-review/`. `ObjectKey` is constructor-controlled and revalidated on deserialization. | Storage `tenant_path` test `code_review_paths_stay_under_the_tenant_prefix`; server `pg_code_review_cache.rs:306-318` asserts all actual pipeline objects retain tenant/installation/repository prefix; `:381-389` rejects a non-Wyrd tenant key. | PASS |
| Foreign pointer/record keys do not become object authority | Existing library `Snapshots::read` (`snapshot.rs:383`) reconstructs the expected scope/commit key before IO, then checks digest and decoded identity. `Snapshots::collect` (`:422`) rejects keys outside the caller's snapshot namespace before delete. `Contexts::read` (`context.rs:344`) reconstructs the exact identity/id key, checks record ID/key, digest and decoded identity. `Contexts::collect_expired` (`:396`) rejects a record whose key is not the returned identity/id's context key before delete. These complete workflow owners are reused by the new facade/adapters. | Library `context.rs:559-594` asserts foreign record read yields ForeignKey; `snapshot.rs:3393` asserts collection rejects foreign keys. Their checks remain in the pinned dependency; no Wyrd replacement bypasses them. | PASS |
| W7: privileged expiry cannot select a foreign tenant for mutation | Adapter `expired_contexts` requires the composed operator pool, deserializes the stored identity and compares its tenant UUID against the actual row tenant before returning any cleanup inputs. Library checks the returned context key; deletion then calls adapter `delete_context` through the identity's ordinary tenant connection. Operator SQL itself performs no mutation. | Server `pg_code_review_cache.rs:619-635` drives real `Contexts::collect_expired`, asserts exact identity/id deleted, no lag, absent row and absent object. SQL expiry and non-extension assertions also inspected. | PASS |
| W6: rename continuity without label-based authority | Library `RepositoryIdentity` equality/hash/order use numeric ID; provider reads address `/repositories/<id>/...`. Wyrd table keys and object keys exclude owner/name labels. | Server `pg_code_review_cache.rs:332-350` performs a review with changed owner/name, asserts no archive rebuild, equal full comparison, and reads the original context under renamed labels. | PASS |
| L8/L14: provider identities and pins remain caller inputs | New `ReviewPipeline::pinned` takes caller identity/pins/baseline, delegates to existing owners, and makes no installation, tenant, policy or delivery decision. `Comparisons::pull_request` checks a present live fork ID against the pinned ID. Existing `GitHubClient::pull_request` verifies PR number and base repository numeric ID before returning metadata; tree/commit/blob paths retain provider identity/digest checks. | Library journey fork assertion and mismatched-head-repository refusal; provider identity tests; new facade/comparison source inspected. | PASS |
| W8/N1: cache cleanup cannot erase accepted execution evidence | New keys remain in code-review namespace; migration stores library records/pointers only, no execution ID or payload bytes. Adapters use neither Bifrost nor Scribe. Existing library cleanup receives only these typed cache keys. Owner architecture states separate custody. | Actual pipeline object-prefix assertion plus source/schema call tracing. | PASS |
| Incidental storage journey change preserves credential policy | `storage_e2e.rs` now consumes the server-returned `download.plan`. Existing client download dispatch sends Wyrd credentials only for root-relative plans. Removing the test's manually built absolute local URL exercises that existing auth boundary without changing it. | Diff and existing client `storage/download::authenticated` plus dispatch inspected. | PASS |

## Trust interpretation and existing-owner assessment

The adapters are internal process capabilities over configured storage/Postgres,
not request handlers. They intentionally accept a library `Identity`; they do not
authenticate it. The task explicitly assigns verified identities and installation
tokens to Task 002 and the eventual queued invocation to Task 005. Current
adapter callers are the production-shaped integration tests; this task introduces
no public Wyrd action. Requiring the adapter to implement new principal checks or
revalidate association on every cache IO would invent Task 005 behavior. The
receiving queued boundary must eventually construct identity from verified
server authority; that future guarantee is not claimed here.

The object adapter similarly has no independent caller identity with which to
compare its typed key. A key with another valid tenant UUID is not inherently
unauthorized at this process-wide storage layer. The existing library workflows
bind keys to their caller identity before read/delete, and the existing SQL owner
binds metadata transactions to their identity. The invalid-tenant adapter test
proves UUID refusal, not authentication of arbitrary UUIDs. These distinctions
avoid treating path shape as verified principal authority.

Metadata JSON is not promoted directly to arbitrary object access: persisted
keys are revalidated structurally, then existing library read/collection owners
perform semantic identity checks. The operator sweep additionally compares the
stored identity tenant against the actual row tenant. The reviewed writers
produce matching repository/installation/identity/record values; no supported
external writer accepts arbitrary cache JSON. No material reachable foreign-key
read/delete path was established.

Reuse is sufficient at this boundary: Wyrd's existing storage handle owns bytes,
existing tenant-path owner owns path validation, SQL owns RLS/transactions, and
the library owns exact-key/integrity/collection checks. No alternative token,
identity resolver, authorization layer or cache service is introduced.

## Findings and limits

Material finding ledger: empty. No optional hardening or Task 005 demands are
proposed.

This is source review and inspection of committed test assertions and recorded
verification, not independent execution evidence. I did not rerun Postgres,
object-store matrices, cloud GCS, or mutation checks. No live GitHub credential
flow was exercised; App authentication is unchanged and outside this task's
invocation boundary. Concurrency/publication recovery is covered here only where
it affects scope; the dedicated data/lifecycle reviewers own its full audit.

The orchestrator subsequently supplied `verification.md` in this review directory:
its independent focused SQL/server adapter run passed 11/11, library public
pipeline journeys passed 10/10, and tenant-isolation/dependency checks passed.
That evidence supports the inspected integration assertions above; I read the
record but did not personally execute those checks. Real GCS remains unverified.
