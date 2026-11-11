# Independent findings validation — Task 004

Recommendation: **BLOCKED** on missing approval of the affected specification
revision. Technical ledger: one retained mandatory repository-gate violation,
**FIND-004-1**. Without the authority limitation the technical disposition would
be **FIX_REQUIRED**, not PASS. No production changes are requested by this report.

## Immutable subject and evidence

Wyrd base `81aca5479`; candidate
`359a19f41597b8cfdc673373409b0d96e8298447`. External library
`/home/thorrester/Documents/GitHub/bohmian-code-review`, base
`d8c772de4248ce0130f79ec3b5fefda518b1431a`, candidate
`d380f57fa53429eaa4f3e7272ce651e1825a8b38`. Both checked-out HEADs still match
these candidates; Wyrd has only the new review directory untracked, and the
library is clean. First review; there is no prior acceptance or finding closure
to carry forward. This validator wrote only this report.

Read all selected reports: `task-review-behavior.md`,
`task-review-invariants.md`, `standards-review.md`, `maintainer-review.md`,
`system-review.md`, `domain-review-data.md`, `domain-review-security.md`, and
`followup-review.md`, plus `scope.md`, `claim-comparison.md` and
`verification.md`. Claims were checked against actual source and the recorded
Git ranges, not adopted by vote. CodeGraph preceded Wyrd code navigation; the
external library has no root index.

Applicable authorities inspected include root and SQL AGENTS, the spec-driven
and implementation-execution workflows, Rust and maintainer guides, TESTING,
the architecture authority map, Verifier code-review cache custody, Bifrost
durability and security Postgres tenant binding. The external repository's own
AGENTS, agent rules and Rust reference independently govern its rustdoc;
Wyrd's rules are not imported into that repository. Original Task 004 and its
storage amendment were read separately from its implementation claims. The
approved revision-5 source is available at `0f08e6c9f`; revision 6 was introduced
in `81aca5479` before implementation.

## Specialist applicability and coverage

| Affected obligation/boundary | Required coverage and validation |
|---|---|
| Frozen/full/active source comparisons and unchanged ReviewContext, L1–L15 / REQ-006/007 / AC-004 acquisition | Behavior reviewer covers the public facade, provider comparison/tree/blob consumers and actual journey assertions. Its acceptance matrix includes source failures, moved live head, baseline failure, rename/delete, binary/excluded/oversize source, truncation/reconstruction, empty active comparison and warm reuse. Coverage is present. |
| Shared snapshot/context lifecycle, W3–W5 | Invariant reviewer traces producer refresh/create/CAS and claim/delete/retry; data reviewer covers the actual SQL conflict and timestamp authority. Their combined scope includes concurrency; another concurrency reviewer is unnecessary. The disputed publication interpretation received the required focused follow-up. |
| Cross-tier rules, fields/helpers/tests and external dependency delivery | Repository reviewer covers server, SQL, storage, manifests, architecture, tests and library. Changed-item rustdoc was included in this same pass; its retained claim is checked below. Missing approval is reconciled independently below rather than accepting its direct-implementation assumption. |
| New workflow/adapter/query ownership and abstraction changes | Maintainer reviewer compares complete existing pipeline, provider, storage and SQL owners, not merely leaf reuse. Source confirms the new adapters do not duplicate extraction/ranking or create a second metadata service. |
| Partial publication, uncertain commit and interrupted cleanup | System reviewer follows per-call failure containment, object-before-record deletion, retry and existing provider cancellation. This task adds no server boot/liveness dependency or new worker. Its documented lifecycle preconditions are part of coverage. |
| Persistence, RLS, database time, CAS and expiry | Data specialist covers the migration, all query capabilities and transactional adapter calls. Actual source has tenant-leading keys, forced RLS, TenantConn operations and a read-only OperatorPool expiry enumerator. |
| Tenant/object scope, numeric identity, privileged sweep and evidence separation | Security specialist covers key producers and exact-key consumers, tenant binding, rename continuity, expiry identity checks and retained-evidence isolation. Actual source uses the separate code-review namespace; operator enumeration does not mutate across tenants. |

All required selected reports are present. No omitted required specialist was
identified. UI, Python, TypeScript and public Wyrd SDK operations are absent
from the actual range; no language/UI specialist is required. No model call,
judge or agent loop changes, so no AI-domain specialist is required. Public
library behavior has real public-library journeys; the Wyrd adapters have real
storage/Postgres integration proof. Task 005/006 own queued execution and SDK
journeys; demanding those here would enlarge this task. There is no prior
evidence being used to excuse a missing selected report.

Verification inspected: independent 11/11 SQL/server adapter tests, 10/10
library pipeline journeys, tenant-isolation and dependency checks, and diff
check, as recorded in `verification.md`. This validator did not rerun them.
Source assertions, rather than test names alone, establish the reported
protocol coverage. Real GCS create-only execution remains unavailable and is
not claimed passed; native Create implementation and other backend assertions
provide bounded evidence, not a fabricated cloud result.

## Proposal dispositions

### B-004-1 — REJECTED

The SQL pointer statements at
`crates/wyrd/wyrd-sql/src/queries/code_review_cache.rs:28–39` do not inspect a
claimed record or share a record lock. That fact is true. The proposed stronger
consequence—publication may never acknowledge an already-claimed target even
when called directly without its producer workflow—is not the library
protocol this task adopts.

The actual non-test publisher is library `snapshot.rs:887–925::ColdBuild::publish`:
refresh the record at 898, create/adopt and byte-check its deterministic object
at 899–909, then tree-CAS at 917–923. The update and pinned-preparation paths
reuse this owner; other pointer-call occurrences are traits, store
implementations/delegation or tests. The deleting consumer is
`snapshot.rs:403–438::Snapshots::collect`. Its explicit contract at 413–417
requires grace longer than the longest build and non-overlapping collection
for one identity; repository removal at 441–455 requires no running build.

Claim and refresh contend on the same record row. Claim first means refresh
is refused by `TOUCH_RECORD_SQL`'s `WHERE NOT claimed` and the adapter returns
Collecting before object creation or CAS (`review_cache.rs:300–319`). Refresh
first makes the row too recent for collection until publication completes
under the required grace. A collector observing the prior pointer when a
publication moves away can conservatively retain that old object; it cannot
delete the freshly refreshed new target under those preconditions.

Read together, library `persistence.rs:151–163,174–225` describes this workflow:
atomic selection/claim, claimed-refresh refusal and pointer tree-CAS. The
README PostgreSQL implementation at 289–321 and 350–374 prescribes the same
pointer INSERT/UPDATE and one-statement claim shape as Wyrd. The reference
memory store at `testing.rs:292–307,344–369` also permits a later direct CAS to
a claimed key. Thus sequential claim-then-direct-CAS does not prove violation
of the requested workflow, even though the phrase “against ... pointer swaps”
could be read more strongly in isolation. Task 004 expressly asks for the
library protocols equivalent to the README schema; it does not supersede
their caller preconditions. Architecture cache custody does not introduce a
stronger concurrency outcome.

The existing library `collection_claim_and_publication_never_race` assertion
path at `snapshot.rs:2996–3070` exercises claim-first refusal, readable retry,
and refresh-first exclusion. Real SQL assertions cover touch/claim exclusion;
actual-adapter assertions cover retry and static pointer protection. No report
or this validator claims an orchestrated arbitrary pointer/claim schedule was
tested in PostgreSQL. Such a schedule, detached from refresh/grace, would
prove a new guarantee rather than the supported task path.

The focused follow-up is source-resolved. No supported failure remains that
justifies new locks, claimed-target CAS outcomes or target-existence guards.
The proposed correction would strengthen concurrency semantics beyond the
demonstrated defect; it is deleted from remediation, not retained as advice.

### REPO-004-001 — REVISED and retained as FIND-004-1

The mandatory rustdoc gate is source-proven. The finding is narrowed below to
changed items with established omissions. Its instruction to document nearby
helpers “consistently” is not an independent reason to alter helpers whose
input/invariant and panic behavior is already adequately described. No broad
documentation cleanup, runtime behavior change or new check is justified.

### Approval-authority proposals — REVISED: separate BLOCKED limitation

The behavior report correctly refuses to treat draft revision 6 as approved.
The standards report's contrary direct-implementation exception is unsupported
by the supplied user instruction. The prior visible request directed the
ObjectStorage/Postgres storage amendment to design/tasks, and the current
request asks for review. Neither supplies explicit approval to implement the
affected material storage contract. The task's appended assertion that the
user directed implementation, and similar wording in design notes, cannot
create an approval absent from the underlying request.

`changes/spec-driven-development.md` puts user instructions first and requires
renewed approval for material spec changes. Its specification contract permits
only explicit human approval to promote a draft. The bounded direct exception
in `changes/implementation-execution.md:43–47` requires an explicit
implementation request and cannot bypass the approved spec. Approved revision
5 at `0f08e6c9f` allows no new Postgres tables except installations; revision 6
adds the cache-table exception, remains draft, and expressly says revision-5
approval does not approve it. Task 004's opening independently remains
proposed pending renewed approval. Candidate architecture additions cannot
bootstrap approval of the implementation that introduced them.

This is **BLOCKED**, because an approved affected specification cannot be
obtained from the provided authority. It is not SPEC_REVISION_REQUIRED: the
draft already records the exact directed storage outcome, and source review
has not established a new material behavior/design decision or a required
concurrency redesign. Resolve the existing approval status and contradictory
implementation-authorization claim faithfully; do not claim retroactive
approval, undo the implementation, or redesign it by inference. This authority
limitation has no FIND ID and does not erase the independently established
documentation defect.

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

## Recommendation and bounded handoff

Return **BLOCKED** for the whole task until the affected specification approval
is actually established. Retain FIND-004-1 as mandatory technical remediation;
do not relabel the blocked task FIX_REQUIRED to make remediation appear ready.
A single diagnostic proposed documentation-only task may package the exact
ledger above while explicitly recording pending authority. Local reversible
rustdoc correction is an implementation-owned class under the execution guide,
but that does not authorize material storage implementation, external delivery,
publication or acceptance. No new approval workflow or extra design decision is
introduced by this report.

B-004-1 is rejected and creates no remediation. No new blocker was discovered
or self-validated. The only retained technical finding is FIND-004-1. Runtime
tests were not repeated by this validator; real GCS remains unverified. Future
callers must still honor the existing grace and collection preconditions.
