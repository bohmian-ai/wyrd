---
id: TASK-004-R1
kind: remediation
status: proposed
spec: SPEC-github-verifier-pipelines
spec_revision: 6
parent_task: TASK-004
remediates: [FIND-004-1]
---

# Complete Task 004 changed-item rustdoc

This is a diagnostic proposed remediation, subordinate to the BLOCKED task
verdict. Specification revision 6 is draft, not approved; do not treat this task
as approval of material storage implementation, external publication or delivery.
No new behavior/design decision is requested. Establish the existing revision's
approval before affected acceptance proceeds. The locally reversible rustdoc
correction is bounded below; this review does not execute it.

## Subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
Spec: `changes/active/github-verifier-pipelines/spec.md`, revision 6 (approval
unestablished); last approved revision 5 available at `0f08e6c9f`.
Original task: `changes/active/github-verifier-pipelines/tasks/004-pinned-pr-preparation.md`
at `81aca5479145b2b94d5db4e0d15816d82cf40bc7`.
Wyrd candidate: `359a19f41597b8cfdc673373409b0d96e8298447`.
Library: `/home/thorrester/Documents/GitHub/bohmian-code-review`, base
`d8c772de4248ce0130f79ec3b5fefda518b1431a`, candidate
`d380f57fa53429eaa4f3e7272ce651e1825a8b38`.

Root/nested AGENTS and owning Rust guidance retain their mandatory rustdoc
requirements. Full diagnosis and independently validated correction follow.

## FIND-004-1 diagnosis and correction

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


## Acceptance and routing

- FIND-004-1: all established changed-item field/helper/panic and durable-write
  documentation omissions above are corrected, including the broken library
  fixture link. The docs accurately state existing outcomes and partial progress.
- The correction preserves signatures, runtime behavior, SQL/migrations, tenant
  authority, library lifecycle preconditions and every existing test assertion.
- Any delivered library revision is reviewed and pinned through the existing
  authorized path. No publication, commit, push or new authority is implied.
- Focused documentation/format/lint evidence is recorded; no broad runtime or
  full user-journey suite is demanded for this documentation-only remediation.

Route this bounded correction to `$wyrd-implement`, with the blocked approval
status supplied explicitly. A later task review checks FIND-004-1 closure and
the incremental diff while carrying forward unaffected technical evidence.
B-004-1 was rejected and is not remediation scope. No new collection locking,
CAS outcome, cache service, deployment or Task 005 behavior may be added here.
