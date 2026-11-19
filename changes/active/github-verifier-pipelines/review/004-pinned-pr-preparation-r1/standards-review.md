# Repository standards review

Result: **FAIL** — one bundled changed-item rustdoc gate violation, `REPO-004-001`.

## Immutable subject and authority

Wyrd range: `81aca5479...359a19f41597b8cfdc673373409b0d96e8298447`.
Library range in `/home/thorrester/Documents/GitHub/bohmian-code-review`:
`d8c772de4248ce0130f79ec3b5fefda518b1431a..d380f57fa53429eaa4f3e7272ce651e1825a8b38`.
Both working-tree HEADs matched their recorded candidates during this review.
Only this report was written. No implementation, tests, dependency files, or
architecture were changed; no mutation checks were run.

This is a repository-standards review, not a task-acceptance or maintainability
verdict. The complete recorded ranges, including test helpers, configuration,
architecture, manifests, and lockfile, were inspected. CodeGraph was attempted
before source navigation in Wyrd; it did not resolve the new cache modules, so
their actual source was inspected directly. The library has no `.codegraph/`.
The only applicable nested Wyrd rules found under changed crate paths are
`crates/wyrd/wyrd-sql/AGENTS.md`; the external library's own root rules apply
there, rather than importing Wyrd-specific rules into that repository.

Revision 6 of `spec.md` remains **draft**. Its final paragraph expressly says
revision 5 approval does not approve the storage amendment. It must not be
described as an approved revision. Nevertheless, the recorded direct user
instruction to implement the storage correction is separate authority:
`design/notes.md:234`, `design/approved-code-review-support.md:199`, and the
task's Obligations preamble record that direction. The authority ordering in
`changes/spec-driven-development.md` places current user instructions first;
its bounded-direct-instruction exception and
`changes/implementation-execution.md#change-contract` apply. Accordingly, the
three adapters/tables are not rejected merely because revision 6 lacks formal
approval. This assessment does not approve other draft content or change the
spec's status.

## Authority coverage

Paths below are relative to the appropriate repository root. Wyrd root
`AGENTS.md`, `agent_docs/index.md`, `agent_docs/rust.md`,
`agent_docs/maintainer-style.md`, and the change workflow apply across Wyrd rows.
`architecture/README.md` routes the owning architecture sections.

| Changed surface | Applicable authority and inspected boundary |
|---|---|
| Wyrd `Cargo.toml`, `Cargo.lock`, server manifest | Root dependency tiers and feature rules; Rust build/features; server-only library dependency, dev-only testing feature and SHA-1 fixture dependency; immutable normal Git pin |
| Server `components/github/mod.rs`, `review_cache.rs` | Wyrd Verifier/code-review-source-cache custody; Bifrost durability/visibility/ingest; security tenant/data isolation and Postgres binding; errors guide; library object/persistence protocols |
| SQL query module and export | SQL nested AGENTS connections, caller-owned transactions, timestamps, operator capabilities, name-based mapping; security Postgres binding; Rust/errors rules |
| New SQL migration | SQL nested AGENTS immutable migrations, forced RLS; security tenant-leading keys and operator policy; Verifier cache custody and Bifrost evidence separation |
| SQL integration module, six new tests, migration assertion/doc update | SQL nested test rules; TESTING lane/named-test policy; root test tier and all changed-item rustdoc rules |
| Server integration module and five cache tests with fixtures/helpers | Root test tiers and pg naming; TESTING; security/SQL seam; changed-item rustdoc; actual public library through real storage/Postgres adapters |
| Server `storage_e2e.rs` correction | Existing storage journey and SDK download-plan authentication boundary; failure-routing and test-integrity rules; existing helper rustdoc |
| Storage `create_object`, `tenant_path::code_review` and inline test | Root portable-platform, trust validation, errors, storage ownership and rustdoc rules; Verifier cache namespace and Bifrost custody |
| Storage CRUD helpers/tests, `mise.toml` GCS cloud selector | Root/TESTING test integrity, narrow lanes and diagnosed failure rules; existing emulator/cloud split and actual object_store implementation |
| Wyrd architecture additions and task evidence | Architecture authority map; spec-driven authority and direct user correction; task evidence is a claim to inspect, not normative source |
| External library `comparison.rs`, `eligibility.rs`, `github.rs`, `pull_request.rs`, `review.rs`, `lib.rs` | Library AGENTS and `architecture/agent-rules.md`; `bohmian-code-review-design.md` product/owner/source/pinned-review boundaries; Rust-core and pipeline-consumer references |
| External library `tests/pipeline_journey.rs`, README and architecture addition | Same library authorities; testing-workflows reference and library mise/manifest target and feature ownership; public journey and changed-item rustdoc |

No new HTTP/MCP/SDK operation, generated declaration, wire-schema generator,
Python/Node binding, or UI is present in this range. Those surface-specific
authorities do not add checks to this standards scope. The library error
types are injected internal protocol errors, not new Wyrd public error
payloads; a Wyrd error-catalog change is not required merely to adapt them.

## Per-rule results

| Rule | Evidence and result |
|---|---|
| Approved authority/direct instruction; no silent promotion of draft | **PASS** for this bounded storage correction, for the authority reasons above. Revision 6 remains draft. |
| Durable ownership and dependency tiers | **PASS**: library runtime dependency is on `wyrd-server`; SQL and storage do not import it. SQL owns metadata statements/migration; server owns adapters; storage retains the shared conditional-put capability. Library pipeline alone performs Git/storage IO and consumes the public symbolic facade. No client-tier/server-tier or Skald/Vala reversal was added. |
| Existing deployment and accepted-evidence custody | **PASS**: migration contains keys, JSON metadata and timestamps, no object bytes or execution IDs. Objects map to `<tenant>/code-review/`; architecture additions distinguish cache custody from accepted Bifrost evidence. No Bifrost/Scribe metadata adapter or new database service. |
| Concrete state owners, sync versus IO, no new runtime | **PASS**: `CodeReviewObjects`, `CodeReviewMetadata`, `Comparisons` and existing `ReviewPipeline`/`PrContext` retain dependencies and workflows. Pure path/JSON/hunk transformations stay synchronous. No ad hoc runtime, target-OS branch, prohibited host isolation, new capability trait, or new feature introduced. |
| SQL connection capabilities, not just grep-check coverage | **PASS**: every tenant production query in `queries/code_review_cache.rs` takes `&mut TenantConn<'_>`; `expired_code_review_contexts` takes `&OperatorPool` and only enumerates. No raw pool/connection/transaction argument or field was added. `CodeReviewMetadata::tenant` acquires and commits the caller-owned connection after a query returns. |
| Caller-owned transactions and no transaction over external IO | **PASS**: SQL queries never commit/rollback; adapter protocol calls commit before returning to the library. Object-storage operations occur on a separate owner, after protocol-call transaction completion. |
| Cross-tenant authority | **PASS** for capability shape: the named expiry query is read-only and bounded; deletion returns to an identity-specific tenant connection. No cross-tenant mutation is admitted, so mutation lease/fence checks do not apply to this enumerator. Security-domain review separately owns end-to-end identity validation. |
| RLS, tenant SQL, migration policy | **PASS**: all three new tables lead with a tenant FK, enable and force RLS, and have both tenant predicates and migration-login operator policy. Tenant query predicates do not manually duplicate tenant filtering. New migration is additive; no applied migration changed. `pg_migration` asserts the exact three RLS relations. |
| PostgreSQL clock and result mapping | **PASS**: touch/publication/expiry/grace SQL uses `statement_timestamp()` plus bound durations; no host deadlines are passed to SQL. `query_as`/named columns replace positional reconstruction. `SystemTime::from` converts database-returned stamps only. |
| Typed errors, validation and secret handling | **PASS** in this standards scope: malformed tenant/path/JSON or BIGINT overflow maps to backend failure, missing object remains distinct, and SQL errors use existing `SqlError`. No credentials in tracing arguments or new secret-bearing public structure. Production has no unchecked IO/input unwrap. |
| Tests at appropriate tiers and credential-free fast lanes | **PASS**: new SQL tests use real PgFixture and fail on missing setup, server tests wire public pipeline plus real Postgres/storage, new path logic has an inline pure test. PG modules use `pg_` names. Storage integration retains its emulator feature/explicit environment lanes. No SDK user operation is claimed shipped by this adapter-only work. |
| Test failure diagnoses and preservation of assertions | **PASS**, independently diagnosed below. The new GCS assertion is retained in the real-cloud lane; no existing emulator CRUD assertion was removed. SQL retry retains interrupted-claim proof; download journey now redeems the server-issued plan through the actual SDK. |
| Format/lint/dependency/test evidence | **PASS within recorded limits**: committed task evidence records format/lints, metadata/storage lanes, tenant/dependency checks. `verification.md` records independently repeated 11/11 cache tests, 10/10 library journeys, diff check and tenant/dependency gates. These do not prove the manual rustdoc gate. This reviewer did not rerun broad lanes. |
| Generated artifacts, pins, no unrelated dependency expansion | **PASS**: lockfile source is the recorded immutable library revision; no source schema or generator output was hand-edited. Existing dependency versions were not widened; new dependency cone belongs to server. |
| Every changed Rust item documented, with Errors/Panics and durable-operation semantics | **FAIL**, `REPO-004-001`: new private fields/helpers and panicking tests/helpers are incompletely documented; some newly introduced durable operations omit cancellation/uncertain-commit/retry documentation. |

## Independent diagnosis of failure-clearing changes

The task records causes and fresh diagnostician agreement for GCS and the
download-plan failure but supplies no preserved standalone diagnostician
report. This section supplies fresh, read-only diagnostic source evidence for
the actual corrections under review; it does not infer that an unavailable
historical report existed. The task-review skill requires diagnosis and a
diagnostician report, not a particular retrospective filename. These source
checks are part of this reviewer report, and preserve the relevant assertions.

1. **Interrupted-claim retry.** Library
   `testing.rs:341-362::MemorySnapshotStore::unreferenced` applies
   `touched_at + grace <= now` even to already-claimed records. Wyrd's SQL
   `CLAIM_RECORDS_SQL` likewise retains the grace predicate. Retrying a freshly
   claimed zero-grace record with a new one-hour grace legitimately excludes
   it. The candidate SQL test retries with the original zero grace and still
   asserts the same claimed key returns; the server test additionally drives
   `Snapshots::collect` and checks object deletion and later record reuse.
   This correction does not weaken the durable claim protocol.
2. **Local download 401.** SDK
   `storage/download/mod.rs:47::authenticated` checks `get_url.starts_with('/')`;
   dispatch then selects the authenticated local transfer. The removed test
   branch constructed an absolute URL, selecting unauthenticated single-get.
   `storage_e2e.rs:357` now uses `download.plan`, preserving the actual
   server-issued capability and the subsequent transfer/byte assertions.
   The diagnosis explains the wrong credential path at its source.
3. **GCS create precondition.** Installed locked object_store 0.14.2
   `src/gcp/client.rs:384-420::put` sends `VERSION_MATCH: 0` for `PutMode::Create`
   and translates precondition failure to `AlreadyExists`. Thus the adapter
   uses the production backend's native conditional create, rather than
   emulating it with a racy head/write pair. The task's recorded curl result
   attributes emulator overwrites to ignored preconditions. The candidate
   leaves existing `gcs_handle_crud` intact; moves no old assertion out of the
   emulator lane; and runs the new create-only, unchanged-bytes and idempotent
   deletion assertions as `gcs_create_only` in the explicit real-cloud lane
   (`mise.toml:908`). Local/S3/Azure still exercise the new assertions in their
   matrix lanes. Real GCS and the historical curl probe were not independently
   rerun here, so their runtime result is not claimed.

## Material finding

### REPO-004-001 — VIOLATION: complete the mandatory changed-item rustdoc gate

**Rules.** Wyrd root AGENTS: every new/changed Rust item has rustdoc, including
`# Errors` on fallible items and `# Panics` when it can panic; missing rustdoc
blocks merge. `agent_docs/rust.md#Rustdoc` explicitly includes fields, private
helpers, test helpers/tests, and cancellation/partial-progress/idempotency/retry
semantics for async or durable operations. The external library independently
requires the same private-item/test documentation in its AGENTS, agent-rules
and Rust-core reference. This is an explicit mandatory gate, not a preference
about prose or a speculative runtime failure.

**Source-proven instances, all introduced in the recorded range:**

- Library `comparison.rs:304-307`: all four retained `Comparisons` fields lack
  rustdoc. `comparison.rs:624` introduces nested `needs_patch::text` with no
  rustdoc. `github.rs:972-976` introduces five undocumented `FileCompareWire`
  fields; new `PullRequestWire` fields at 829-832 are undocumented as well.
- Library `comparison.rs:685` accepts an arbitrary character then `expect`s a
  valid SHA, without a panic contract. New tests at 691, 721 and 741 assert or
  `expect` without `# Panics`. New `pipeline_journey.rs` helpers `text` (919)
  and `assert_pinned_spans` (927) can panic on UTF-8/path/range/assertion
  failures but lack `# Panics`; the six new pinned journey tests likewise
  have no panic sections. `PINNED_FILE_BYTES` rustdoc at 761 references
  nonexistent `BIG` rather than the new `big` helper.
- Wyrd server `pg_code_review_cache.rs:48::sha` can panic for a non-hex
  character, `:61::tarball` expects archive/gzip IO success, `:187::identity`
  expects valid caller-provided repository labels, and `:236::Custody::stored`
  expects directory IO success. They lack `# Panics`; the same changed helper
  block includes `blob_sha`, `FakeGitHub::{pinned,commit,client}`, `pins` and
  `Custody::identity`, whose invariant/panic documentation should be completed
  consistently, without annotating untouched helpers elsewhere.
- Wyrd `StorageHandle::create_object` (`handle.rs:402`) documents the normal
  boolean/error outcome but not cancellation after the backend accepted an
  object, nor what a retry observing `AlreadyExists` establishes. The new
  metadata adapter's committing helper (`review_cache.rs:214`) and durable
  trait writes similarly omit the uncertain-commit/retry contract even though
  they commit before returning and callers can lose the result. Module-level
  transaction-order documentation does not describe that caller-visible
  partial-progress condition.

**Consequence/materiality.** These items do not satisfy an expressly mandatory
merge gate. The missing retry documentation also obscures why a caller may
receive no success while an immutable object or metadata row already exists;
green compilation/tests do not establish documentation completeness.

**Smallest correction.** One bounded documentation pass across changed Wyrd
and library items: document the new fields/helper roles, real panic conditions,
and actual cancellation/commit/idempotency behavior; fix the unresolved link.
Reuse existing module/trait contracts by precise references where they truly
cover the operation. No new abstraction, API, feature, blanket allow, check
script, or test harness is needed. Do not modify untouched items merely to
make a repository-wide documentation sweep. If library source changes, publish
and pin that reviewed corrected revision through the existing delivery path.

**Testable closure.** Inspect every changed Rust item in the cumulative library
and Wyrd diff against the explicit documentation rules; check the new docs and
links using the existing owning documentation/format/lint commands. For a
documentation-only correction, no new runtime test is required. Preserve the
existing negative assertions and immutable-candidate evidence.

## Verification limits

Independent runtime results are those recorded in this round's
`verification.md`; I did not rerun broad aggregates or real GCS, and I did not
reproduce historical mutation/curl checks. Runtime/data/security acceptance
belongs to the other selected reviewers. This report's only proposed material
repository-rule finding is `REPO-004-001`.
