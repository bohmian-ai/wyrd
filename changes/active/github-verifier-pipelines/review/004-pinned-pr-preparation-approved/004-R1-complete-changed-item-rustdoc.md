---
id: TASK-004-R1
kind: remediation
status: ready
spec: SPEC-github-verifier-pipelines
spec_revision: 6
parent_task: TASK-004
remediates: [FIND-004-1]
---

# Complete Task 004 mandatory changed-item rustdoc

Revision 6 is approved, as explicitly confirmed by the user. No approval
prerequisite remains. This task supersedes the diagnostic proposed remediation
in `../004-pinned-pr-preparation-r1/`; that record is historical.

## Subject and authority

Approved spec: `changes/active/github-verifier-pipelines/spec.md`, revision 6.
Original task: `changes/active/github-verifier-pipelines/tasks/004-pinned-pr-preparation.md`.
Wyrd root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
Base: `81aca5479145b2b94d5db4e0d15816d82cf40bc7`.
Candidate: `359a19f41597b8cfdc673373409b0d96e8298447`.
Library root: `/home/thorrester/Documents/GitHub/bohmian-code-review`.
Library base: `d8c772de4248ce0130f79ec3b5fefda518b1431a`.
Library candidate: `d380f57fa53429eaa4f3e7272ce651e1825a8b38`.

## Diagnosis, smallest correction and focused proof


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


## Acceptance and handoff

FIND-004-1 closes when the established changed items have the required accurate
field/helper/panic and durable-write documentation, the fixture link resolves,
and focused owning documentation/format/lint checks pass. Preserve every runtime
signature, SQL statement/migration, error, assertion, tenant boundary and library
collection precondition. No new runtime tests are needed for a docs-only change.

Route directly to `$wyrd-implement`. Normal Git/publication authorization still
applies; this review does not perform implementation, commit, push or delivery.
If library docs change, use the existing reviewed dependency delivery path and
record the corrected revision. Subsequent review checks the incremental diff
and FIND-004-1 closure with full cumulative context and carries unaffected proof.
B-004-1 was rejected: no collection locking, new CAS outcome, service or Task 005
behavior belongs in this remediation.

## Obligations

Contract: FIND-004-1 and the affected-item table above; root `AGENTS.md`
(rustdoc on every new/changed item, `# Errors`/`# Panics`) and
`agent_docs/rust.md#Rustdoc`; the library's own AGENTS rustdoc rules. Proof is
content review of the changed docs plus the owning fmt/lint/doc lanes.

| ID | Promise | Planned proof |
|---|---|---|
| R1-L | Library changed items documented: `Comparisons` fields; `needs_patch::text`; `PullRequestWire` `html_url`/`title`/`body`; `FileCompareWire` five fields; `# Panics` on `tests::sha` and the three comparison owner tests; `text`/`assert_pinned_spans` and the six pinned journeys in `pipeline_journey.rs`; `PINNED_FILE_BYTES` links the real `big` helper | doc diff review; library `fmt:check`, `lints`, `cargo doc` |
| R1-W | Wyrd fixture helpers `sha`, `tarball`, `identity`, `Custody::stored` carry `# Panics` | doc diff review; `fmt`, `lints` |
| R1-D | Durable writes state uncertain completion and retry meaning: `StorageHandle::create_object` (bytes may be accepted before success is seen; retry stays create-only; `AlreadyExists`/false proves existence, not equality); review-cache adapter commit boundary documented once at the owner with operation outcomes (CAS may have moved, a repeat returns false; refresh/claim may persist; cleanup retryable; context retry keeps original record/stamps; delete may have completed) | doc diff review; `cargo doc` for `wyrd-storage`/`wyrd-server` |
| R1-P | Docs-only: no signature, SQL, migration, error, assertion, tenancy or precondition change | `git diff` audit in both repos |
| R1-H | Handoff: the corrected library revision is delivered through the reviewed path and pinned by an ordinary manifest/lock update | merged PR; ancestry check; lock diff; build |

## Evidence

Library commits `f470f8e` and `9046d103` on `wyrd-production-rustdoc`
(parent `d380f57f`), merged to library `main` through PR #7 as `8adb0b7b`.
Wyrd commits `5c2cababc`, `4d80bacc1` (docs) and the pin commit that follows. Library paths are relative to
`crates/bohmian-code-review-pipeline/`.

| Obligation and required cases | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| R1-L: library fields, helper, panics, fixture link | `Comparisons` four field docs; `needs_patch::text`; `PullRequestWire` three field docs (omitted/`null` read as empty); `FileCompareWire` five field docs (commit page, capped file list whose patches are optional); `max_file_bytes` says an oversize blob is unavailable; `# Panics` on `tests::sha` and the three owner tests, `text`, `assert_pinned_spans` and the six pinned journeys, each worded from its actual expect/assert paths; `PINNED_FILE_BYTES` links `big`. Also fixed two broken intra-doc links from the task 004 candidate that failed the doc check: the `comparison` module doc linked private `Comparisons`, and `PullRequestFiles::truncated` linked private `COMPARE_MAX_FILES` | `git diff d380f57f..9046d103` | library `mise run fmt:check`, `mise run lints`; `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p bohmian-code-review-pipeline --no-deps` with and without `--document-private-items`: clean | PASS |
| R1-W: Wyrd fixture panics | `# Panics` on `sha`, `tarball`, `identity`, `Custody::stored` | `pg_code_review_cache.rs` in `5c2cababc`, `4d80bacc1` | content review; `mise run fmt`, `mise run lints` | PASS |
| R1-D: durable write cancellation/retry | `StorageHandle::create_object` `# Cancellation` (bytes may land unseen; retry stays create-only; `false` proves existence, not equality). `CodeReviewMetadata::tenant` documents the single commit boundary and a lost reply once; per-operation repeat outcomes on publish (moved pointer, repeat `false`, re-read), touch (repeat touches or `Collecting`), claim (persisted claim matches again), deletes (repeat succeeds), context upsert (original record and stamps), `remove_repository` (repeat returns only keys still recorded; removed keys' objects are no longer named), object create/delete (references storage contract) | `handle.rs`, `review_cache.rs` in `5c2cababc`, `4d80bacc1` | `cargo doc --document-private-items -p wyrd-storage -p wyrd-server` with broken-link warnings: none in the changed files | PASS |
| R1-P: docs-only | both docs diffs change only `///`/`//!` lines; no signature, SQL, migration, error, assertion or tenancy change | `git diff d380f57f..9046d103`, `git diff 359a19f41..4d80bacc1` | `-U0` diff audit (fresh checker and own) | PASS |
| R1-H: reviewed library revision pinned | PR #7 merged by the user's explicit authorization; `Cargo.toml:257` rev `9046d103…`; `Cargo.lock` changes only the seven bohmian source lines | `git merge-base --is-ancestor 9046d103 origin/main` | `cargo check --locked -p wyrd-server --tests`; `mise run check:deps` | PASS |

Limits: `mise run check:docs` covers only `wyrd-spec` and two auth crates, and
fails on `wyrd-spec` GitHub links from task 002's `9f0a8e5cf`. Task 004
changes no `wyrd-spec` file, so this remediation leaves them alone. Integration
test docs are not built by rustdoc and were content-reviewed. The fresh checker
found one inaccurate statement (oversize recorded as excluded) and four
incomplete ones (`files` patches, first-journey panics, context record retention,
the `stored` path check). All five were corrected in `9046d103` and `4d80bacc1`.
`remove_repository` documents that a repeat after a lost reply omits keys the
interrupted call removed. That is the existing behaviour, described accurately
rather than changed.
