# Fresh independent validation — Task 004, approved revision 6

Recommendation: **FIX_REQUIRED**, with one retained finding: **FIND-004-1**.
No runtime or concurrency finding is retained.

## Authority and immutable scope

The user explicitly confirmed: “I already approved the spec revision. run the
full review like you were instructed”. Revision 6 is approved authority.
`changes/spec-driven-development.md` puts current user instructions and approved
outcomes first. Stale draft/proposed markers and the former approval objection
are superseded; no approval question, approval prerequisite or BLOCKED limitation
remains. No new behavior decision or specification revision is required.

Wyrd root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
Base: `81aca5479145b2b94d5db4e0d15816d82cf40bc7`.
Candidate: `359a19f41597b8cfdc673373409b0d96e8298447`.
Library root: `/home/thorrester/Documents/GitHub/bohmian-code-review`.
Library base: `d8c772de4248ce0130f79ec3b5fefda518b1431a`.
Library candidate: `d380f57fa53429eaa4f3e7272ce651e1825a8b38`.
Task: `changes/active/github-verifier-pipelines/tasks/004-pinned-pr-preparation.md`.
Approved spec: `changes/active/github-verifier-pipelines/spec.md`, revision 6.

Both HEADs freshly match those identities; tracked worktrees and indexes have no
source changes. Wyrd has review reports untracked and the library is clean.
There is no source remediation delta or prior-finding closure. This continuation
carries the complete first audit, rather than replacing it with an approval-only
check or restarting unchanged discovery.

I read the new scope, original task and its complete obligations/proof ledger,
revision-6 contract, and all seven original discovery reports in sibling
`004-pinned-pr-preparation-r1/`: behavior, invariants, standards, maintainer,
system, data and security. I also read its scope, claim comparison, focused
follow-up, findings validation and independent verification record. Their
source/assertion evidence carries; their superseded approval reasoning does not.
Agreement among reports was not treated as proof.

CodeGraph preceded Wyrd source navigation; the library is not indexed. Fresh
inspection covered actual adapter bodies, SQL/migration, conditional object
create, relevant SQL/server assertions, library facade/provider diff,
publication/collection owners, reference stores, changed rustdoc items and full
new journey bodies. Authorities freshly checked include supplied root AGENTS,
SQL AGENTS, spec-driven authority order, agent_docs index/Rust/maintainer rules,
TESTING, Wyrd/Bifrost cache-custody sections, and the library's own AGENTS,
agent-rules, reference router and Rust-core reference. Library documentation is
required by its own rules, not by importing Wyrd rules.

## Specialist applicability and carried acceptance coverage

All seven selected independent discovery reports and the focused follow-up are
present and complete for their assigned boundaries. Source and governing
behavior are unchanged. No required specialist or concrete acceptance gap was
identified; targeted additional discovery is unnecessary.

| Boundary / obligations | Validated coverage and result |
|---|---|
| REQ-006/007 acquisition, AC-004, L1–L15 | Behavior matrix covers every obligation. Fresh facade and full six journey bodies confirm supplied pins/baseline, full merge-base and separate active comparisons, moved live head, fork/metadata, exact source, rename/delete, binary/excluded/oversize, unusable history, truncation/rebuild/paging, omissions, empty active, existing context publication/readback and warm reuse. Carry PASS. |
| Snapshot/context shared lifecycle, W3–W5, INV-004/005 | Invariant and data reviewers trace all publication, read, claim, delete and retry owners. Fresh source confirms refresh→create/adopt identical bytes→tree CAS and object-before-record cleanup. Original W4 conflict is reconciled below. Carry PASS. |
| Cross-tier rules, changed Rust/test/helpers, manifests/config/architecture | Standards report covers every changed layer and diagnoses failure-clearing test changes. Fresh source confirms mandatory rustdoc omissions. Other unchanged rules carry PASS; documentation FAIL remains FIND-004-1. |
| Ownership and abstraction reuse | Maintainer report compares complete pipeline/Update/PrContext, provider transport/validation, StorageHandle/ValidatedPath and ServerPostgres/TenantConn/SQL owners. Fresh facade/adapters confirm complete workflow reuse and no second extractor/index/cache service. Carry PASS. |
| Failure propagation, interruption and recovery | System report covers per-call containment, uncertain commit, orphan discovery, context lag, provider cancellation and cleanup retry. Fresh ordering agrees. No boot/liveness worker or process-crash policy is added. Carry PASS. |
| Persistent data, RLS, concurrency, database time and expiry | Data specialist covers this coherent domain including concurrency, so another concurrency role is unnecessary. Fresh migration/SQL/assertions confirm tenant-leading keys, forced RLS, duration-derived statement time, pointer XOR/tree CAS, guarded touch/claim arbitration and original context stamps. Carry PASS. |
| Tenant/object identity, numeric repository continuity and privileged expiry | Security report traces scoped key producers, exact-key consumers and internal trust boundaries. Fresh adapter bodies validate DataTenantId/path/BIGINT handling, TenantConn per operation, read-only OperatorPool enumeration and stored-identity tenant comparison before tenant-scoped deletion. Carry PASS. |
| W1/W2/W6–W9, N1, D1/D2 and non-goals | Combined matrices cover object distinctions, actual adapters, isolation/rename, independent dependency delivery and cache/evidence separation. Fresh server assertions show dedicated namespace, cold/warm archive reuse, rename readback, interrupted cleanup and unchanged expiry. No Bifrost/Scribe metadata, execution IDs, policy/baseline decisions, delivery or new cache service. Carry PASS. |

Original behavioral W4 FAIL becomes PASS through the source resolution below;
its approval FAIL is superseded by the explicit confirmation. No other matrix
row loses its authority or evidence. No UI, Python, TypeScript, generated binding,
public Wyrd operation or model/agent-loop change appears in the actual range;
those specialists remain inapplicable. Library public journeys and Wyrd real
Postgres/storage integration assertions exist. Task 005/006 explicitly own queued
execution and SDK journeys; requiring them here would expand this adapter task.
Unchanged lifecycle/provider dependencies were reviewed, not omitted merely
because their files did not change.

## Independently checked proposal dispositions

### B-004-1 — REJECTED

The SQL pointer INSERT/tree UPDATE at `queries/code_review_cache.rs:28–39`
do not check a claimed flag or lock the record. That limitation is real; no
arbitrary direct pointer/claim serializability guarantee is claimed.

Actual library publication converges on `snapshot.rs:887–925::ColdBuild::publish`:
refresh at 898, create/adopt and byte-compare at 899–909, then CAS at 917–923.
A search of all pointer publishers confirms no independent production writer;
update and pinned preparation reuse this owner. Claim-first makes SQL
`WHERE NOT claimed` refuse refresh, which adapter `upsert_record` maps to
Collecting before object IO or publication. Refresh-first makes the row too
recent to claim under supported grace.

`Snapshots::collect` at 403–438 explicitly requires grace longer than the longest
build and non-overlapping collections per identity; removal at 441–455 requires
no active build. It claims, deletes bytes, then deletes the record. Thus the
supported producer cannot publish a target being collected. A collector seeing
the old pointer can conservatively retain that old key; the new target remains
protected by refresh/grace.

I independently checked `persistence.rs:151–225`, root library README pointer
publisher 289–321 and collector 350–374, memory publisher `testing.rs:292–307`
and claim 344–369. The task expressly adopts those protocols/README schema.
The README prescribes the same unguarded pointer statements and single UPDATE
claim; even the reference memory store allows a later direct CAS to a claimed
key. Its workflow wording does not add a claimed-target refusal outcome absent
from the publication protocol. Approved revision 6 and cache architecture do
not replace its caller preconditions with a stronger concurrency promise.

Fresh assertions at `snapshot.rs:2996–3070` prove claim-first refusal/readable
retry and refresh-first exclusion. SQL 395–430 races touch/claim, static pointer
protection/retry is asserted at 151–195, and real-adapter cleanup retry at
480–504. None is claimed to force an arbitrary PostgreSQL pointer/claim
interleaving. Direct post-claim CAS, a delay past grace or zero-grace removal
during a build does not prove supported failure. New locks, existence checks
or CAS outcomes would strengthen semantics beyond the demonstrated defect.
The focused follow-up is source-resolved; this rejected proposal creates no
remediation or optional advice. No new follow-up is needed.

### REPO-004-001 — REVISED and retained as FIND-004-1

Fresh inspection confirms the established changed private fields and nested
helper lack docs; the named helper/test bodies have expect/assert/index paths
without panic contracts; the create/commit owners lack uncertain-completion
and retry contracts; and `BIG` is not the actual `big` fixture helper. I read the
full affected adapter/helper and new journey bodies and their relevant consumers.
These are explicit mandatory documentation-gate failures, not an allegation
that runtime recovery is broken. Existing module transaction-order prose does
not explain a lost response after durable success. The correction may reuse
shared module/trait contracts with precise references.

The discovery request to document nearby helpers “consistently” remains narrowed
to the established omissions below. Adequate existing invariant docs do not
create extra work. One bounded bundled correction preserves stable FIND-004-1;
no new blocker was discovered or self-validated.

## Final deduplicated finding ledger


### FIND-004-1

- **Discovery:** REPO-004-001.
- **Validation:** REVISED.
- **Classification:** VIOLATION — mandatory changed-item rustdoc gate.
- **Obligation:** Wyrd root AGENTS requires rustdoc for every new/changed Rust
  item and Errors/Panics where applicable; missing rustdoc blocks merge.
  `agent_docs/rust.md#Rustdoc` explicitly covers fields, private helpers,
  helpers/tests and cancellation/partial-progress/idempotency/retry for async
  or durable operations. The library independently imposes the same private
  item/test and applicable panic/cancellation documentation through its own
  AGENTS, agent-rules and Rust reference.

**Exact established affected items and evidence:**

| Repository / item | Omission and smallest correction |
|---|---|
| Library `src/comparison.rs:304–307::Comparisons` fields `client`, `identity`, `max_file_bytes`, `sources` | All four are newly introduced and have no field rustdoc. State the provider/identity, invocation ceiling and per-stage verified-source cache role. |
| Library `src/comparison.rs:624::needs_patch::text` | Newly introduced nested function has no rustdoc. Explain absent side as empty text and non-text as unavailable for textual patch comparison. No helper extraction needed. |
| Library `src/github.rs:829–832::PullRequestWire` fields `html_url`, `title`, `body` | These three added fields lack rustdoc. Document their provider meaning and absent/default description semantics; do not sweep pre-existing unchanged wire fields. |
| Library `src/github.rs:972–976::FileCompareWire` fields `status`, `base_commit`, `merge_base_commit`, `commits`, `files` | All five new wire fields lack rustdoc. Document the compare-response meaning including the commit-page/file projection. |
| Library `src/comparison.rs:685::tests::sha` | Arbitrary caller character is repeated and passed to `ObjectSha::new(...).expect`. Document non-hex input panic. |
| Library `src/comparison.rs:691::hunks_split_at_headers`, `:721::blobs_classify_as_text_or_binary`, `:741::only_text_changes_need_patches` | New tests have descriptive summaries but no Panics section despite assertion/expect failure paths. Document failed expectations, without changing their assertions. |
| Library `tests/pipeline_journey.rs:919::text`, `:927::assert_pinned_spans` | Document UTF-8 refusal in text, and missing/invalid span path, UTF-8/range and assertion failure in the span checker. Their full bodies show the expect/index/assert paths. |
| Library `tests/pipeline_journey.rs:948::pinned_first_review_obeys_the_pins_cold_and_warm`, `:1085::pinned_incremental_review_compares_from_the_baseline`, `:1154::pinned_reviews_keep_limits_and_failures_explicit`, `:1253::pinned_full_review_compares_from_the_merge_base`, `:1293::pinned_review_rebuilds_a_truncated_head_listing`, `:1346::pinned_reviews_verify_a_paged_comparison` | Six new assertion/expect-driven journeys lack Panics documentation. Add the applicable failed-setup/result/assertion contract; retain all source, failure and coverage assertions. |
| Library `tests/pipeline_journey.rs:763::PINNED_FILE_BYTES` | Rustdoc links nonexistent `BIG`; reference the actual `big` fixture helper. |
| Wyrd server `tests/integration/pg_code_review_cache.rs:48::sha`, `:61::tarball`, `:187::identity`, `:236::Custody::stored` | Missing Panics contracts despite respectively arbitrary non-hex input, archive/path/gzip failures, invalid caller repository labels, and directory/entry/path IO expectations. Document the actual conditions. These are new helpers, not unrelated existing test debt. |
| Wyrd storage `src/handle.rs:402::StorageHandle::create_object` | Documents create-only outcome/errors but omits uncertain completion on cancellation/backend response loss and retry meaning. State that bytes may have been accepted before the caller receives success; retry is create-only, and false/AlreadyExists establishes existence, not byte equality. Existing library adoption reads and compares bytes; preserve it. |
| Wyrd server `src/components/github/review_cache.rs:214::CodeReviewMetadata::tenant` and new object/metadata durable trait writes at 93, 123, 276, 306, 329, 348, 365, 378, 465, 481 | Module/method docs describe transaction order and ordinary outcomes but not cancellation/uncertain-commit/retry. Document the shared commit boundary once, with precise references and operation-specific outcomes where needed: CAS may already have moved before a lost reply; a repeated CAS can return false; refresh/claim can persist; interrupted cleanup remains retryable; context retry retains its original record/stamps; delete may already have completed. For object create/delete reuse the storage contract and idempotent-delete semantics. No blanket claim that every retry is successful is valid. |

Library paths in the table are relative to
`crates/bohmian-code-review-pipeline/`. Wyrd paths name the crate-relative
locations under their listed server/storage owners. The table is one bundled
gate failure, not separate findings per helper or missing section. Documentation
at shared owners with precise links is sufficient; duplicate paragraphs on
every method are unnecessary. The changed SQL module's caller-owned
transaction documentation can be referenced when describing the adapter
boundary; do not change SQL behavior or extend a transaction across IO.

**Observable consequence and materiality:** the candidate does not satisfy an
explicit mandatory merge gate, independently of successful runtime checks.
For durable writes, callers additionally cannot learn from these new docs
that an absent success response may coexist with committed state, or what a
retry can establish. This is a documented-contract omission, not a claim that
the inspected runtime recovery ordering is defective.

**Decision-complete correction and burden:** perform one bounded documentation
pass over the established changed-item scope above, completing meaningful
field/helper/panic and durable-operation contracts and fixing the broken link.
Reuse existing module/trait/storage ownership with precise references. Preserve
all signatures, object bytes, migrations, SQL/CAS/claim behavior, grace
preconditions, errors, tests, auth and tenancy. Do not add an abstraction,
dependency, config, check script, blanket allow or test harness. Do not modify
untouched items for consistency. No runtime test is needed for a docs-only
correction. Library edits require a newly reviewed revision and an ordinary
dependency/lock update through the existing authorized delivery path; this
review does not authorize publication or pin changes.

**Focused closure proof:** inspect changed-item docs and links against the
owning rules; confirm the correction diff changes only rustdoc plus any
necessary reviewed dependency pin/lock references. Use the existing owning
format/lint/documentation commands rather than adding a checker. Library
`mise run fmt:check` and its existing `mise run lints` cover the external write
set; Wyrd `mise run fmt` and the narrow owning lint/doc tasks discovered in
`mise.toml` cover its write set. Check rustdoc links with the existing owning
doc lane or `mise exec -- cargo doc --locked -p <owning-crate> --no-deps` where
no managed owner task exists. No full runtime suites, new cloud harness or
whole-change aggregate is required for this documentation correction. If a
corrected library pin is delivered, verify its exact reviewed identity and
normal manifest/lock resolution without treating that delivery as spec
approval.

## Recommendation and limits

Return **FIX_REQUIRED** for FIND-004-1. Approval is established. Package one
bounded documentation remediation across the established Wyrd/library items,
with normal reviewed dependency delivery if the library revision changes.
No SQL locking change, behavior redesign, new service or product decision is
required. No source remediation has occurred, so no unsuccessful-remediation
convergence escalation applies.

The original verification record independently reports SQL/server adapters
11/11, library public pipeline journeys 10/10, dependency/tenant checks and diff
checks passing. Broader owning lanes are committed task evidence. This validator
inspected assertions and carried those precise execution results; it did not
rerun tests, mutation checks, cloud checks or broad aggregates on unchanged
source. Real GCS create-only execution remains unrun and is not claimed passed.
Future collection callers must honor existing grace/non-overlap/no-active-build
preconditions. Only this report was written; source remains immutable.
