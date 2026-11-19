# Fresh independent remediation validation — Task 004

Recommendation: **FIX_REQUIRED**. Prior **FIND-004-1 is CLOSED**. Retained
ledger: one new mandatory-gate violation, **FIND-004-2** (REPO-R2-001).
No runtime, concurrency, cleanup-redesign or specification-approval finding
is retained.

## Immutable subject and authority

Wyrd root `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`;
original base `81aca5479145b2b94d5db4e0d15816d82cf40bc7`, prior candidate
`359a19f41597b8cfdc673373409b0d96e8298447`, current candidate
`b5c0db2e01268d6ad033fbe7450069247edff6ee`.
Library root `/home/thorrester/Documents/GitHub/bohmian-code-review`;
original base `d8c772de4248ce0130f79ec3b5fefda518b1431a`, prior candidate
`d380f57fa53429eaa4f3e7272ce651e1825a8b38`, current candidate
`9046d1035538df8dfb661f826b102316ee6d0b4e`.

Both HEADs were independently checked and match. Wyrd tracked source/index
is clean; only this round's report directory is untracked. Library is clean.
Inspected the actual incremental ranges in both repositories: library changes
only documentation in comparison.rs, github.rs and pipeline_journey.rs;
Wyrd runtime-source changes only rustdoc in review_cache.rs, its integration
fixtures and storage handle.rs. Manifest/lock changes replace the reviewed
library revision in the existing dependency and seven lock source entries.
Other Wyrd additions are historical review/remediation records. No signatures,
SQL, assertions, errors, source-selection behavior or collection preconditions
changed. Source remains immutable; only this report was written.

Original task is `tasks/004-pinned-pr-preparation.md`, approved spec is `spec.md`
revision 6 in this packet. Explicit user approval supersedes stale draft/proposed
markers. No approval prerequisite remains. Read this round's scope, standards,
system and verification reports, original task/spec and previous approved
verdict, FIND-004-1 ledger and remediation/evidence. Carried reports are in
`../004-pinned-pr-preparation-r1/`; the approval reconciliation and independent
full validation are in `../004-pinned-pr-preparation-approved/`.

CodeGraph preceded Wyrd source navigation. Governing documentation rules are
root AGENTS (changed-item rustdoc and mandatory red gates), agent_docs Rust and
maintainer guidance and spec-driven remediation scope. The library independently
requires private-item/test/panic documentation in its AGENTS, agent-rules and
Rust-core reference. Wyrd/Bifrost cache-custody sections at
`architecture/wyrd-design.md:2098–2112` and
`architecture/bifrost-design.md:653–657` preserve the reviewed cache/evidence
boundary. No runtime authority changed in the remediation.

## Validated coverage and carry-forward

The two newly selected discovery reports are complete and independent. Scoped
repo-rev is warranted by a reported mandatory Rust-doc gate failure. Scoped
system-rev is warranted by the precise new documentation evidence about lost
repository-removal keys. The mandatory fresh ponytail role independently
validated their source, scope, authority and correction burden here.

| Applicable contract / obligation | Required coverage and disposition |
|---|---|
| L1–L12, REQ-006/007 acquisition, AC-004 | Carry r1 behavior matrix and approved validator's source/assertion evidence: frozen pins/live-head exclusion, merge base versus active baseline, explicit unusable history, file/status/source identities, fork metadata, rebuilt/paged comparisons, omission coverage, empty active. No changed executable code invalidates it. Fresh full six journey bodies and comparison helper assertions agree with the added docs. PASS. |
| L13–L15 shared lifecycle and complete workflow reuse | Carry r1 behavior/invariant/maintainer evidence for ReviewPipeline → existing snapshot/Update/PrContext owners and provider transport; docs/pin add no duplicate owner or index. Fresh affected helper bodies preserve the documented source cache and oversize classification. PASS. |
| W1–W5 snapshot/context durability, CAS, collection, clocks | Carry r1 invariant/data/system evidence and approved validation's resolution of B-004-1. Actual SQL, trait, facade and adapter bodies remain unchanged. Fresh commit/write-doc inspection below verifies uncertain completion without upgrading the underlying protocol. PASS. |
| W2/W6/W7 identity, security/RLS, tenant/operator capability and persistence | Carry r1 data and security source/assertion coverage: forced RLS/tenant-leading keys, numeric repository identity, guarded touch/claim, original stamps, statement clock and tenant-owned deletion after operator enumeration. Docs-only changes do not affect these boundaries. No new domain specialist required. PASS. |
| W8/W9/N1/D1 real-adapter proof, cache/evidence separation and non-goals | Carry r1 behavior/invariant/security/system and approved matrix: actual pipeline with repository Postgres/local storage, isolated namespace, no Bifrost cache metadata, execution IDs or new service. Task 005/006 continue to own connected execution/SDK journeys. PASS. |
| D2 and remediation R1-H delivery | Fresh manifest/lock diff names exact reviewed 9046d103; independently checked that revision is an ancestor of local origin/main. r2 verification and remediation evidence cover normal resolution/build and authorized PR #7 delivery. PASS. |
| Changed Rust/private/test documentation and explicit gates | Fresh complete FIND-004-1 closure below; original cross-tier standards audit otherwise carries. New scoped standards report establishes the separate required gate failure, independently reproduced here. FAIL only FIND-004-2. |
| Failure/recovery gap: destructive context repository removal | Fresh system report plus independent primitive/caller/expiry trace below narrows the carried cleanup claim to its proven collection/expiry paths. No supported regression or additional explicit obligation established. PASS. |
| Maintainer/abstraction/deployment coverage | Carry r1 maintainer and system reports. No changed owner, new abstraction, task loop, deployment dependency or retry policy requires a new full specialist pass. |
| UI/Python/TS/SDK/LLM runtime | Inapplicable: neither incremental range changes such a surface or ships a public Wyrd action; no missing specialist. |

The full first audit contains behavior, invariant, standards, maintainer,
system, data and security reports and the focused concurrency follow-up.
These provide source-backed coverage rather than an empty finding ledger or
unchanged-file inference. The approved validation already reconciles the
original W4 proposal and stale approval issue. There is no reason to rediscover
those unchanged areas. The only identified gap in prior recovery evidence was
specifically reviewed this round. No additional selected report is missing;
no further follow-up is needed and no new blocker was discovered by this
validator requiring separate source validation.

## FIND-004-1 closure table

Library locations below are relative to `crates/bohmian-code-review-pipeline/`.
Wyrd locations are relative to the named server/storage crates. Each row was
checked against the actual affected body and consumers, not the evidence's PASS
label. Source line shifts are due solely to documentation.

| Established omission | Actual correction and body/consumer evidence | Closure |
|---|---|---|
| comparison.rs Comparisons four fields | Lines 304–313 describe client, identity, invocation ceiling and stage-only blob cache. Source::Unavailable/Oversize source path and new journey oversize assertion agree; no claim of exclusion remains. | CLOSED |
| needs_patch::text | Lines 629–633 describe absent side as empty, non-text as unavailable; map_or(Some(""), Source::text) and differing-text match implement exactly that. | CLOSED |
| PullRequestWire html_url/title/body | github.rs:828–838 documents omitted defaults and absent/null description; serde default fields, Option body and PR projection preserve those outcomes. No unchanged wire-field sweep. | CLOSED |
| FileCompareWire five fields | github.rs:975–993 documents provider status/base/merge base, commit page and optionally patched capped files. Existing compare_files validation consumes these facts; docs no longer promise every file has a patch. | CLOSED |
| comparison test sha and three tests | comparison.rs:691–783 supplies Panics for invalid lowercase-hex fixture input, hunk parser assertions, binary/text classification and patch-need assertions. Read complete bodies; assertions unchanged. | CLOSED |
| Journey text/assert_pinned_spans | pipeline_journey.rs:918–951 documents UTF-8, missing/foreign path, line ranges, empty spans and text/blob assertion failures. Full bodies use precisely those expect/index/assert operations. | CLOSED |
| Six pinned journeys | pipeline_journey.rs:954–1425 supplies relevant Panics for failed reviews and their pin/source/readback/warm/no-live-head, baseline/context, refusal/omission, merge-base, reconstruction and paging assertions. Full six bodies retain all positive/negative assertions. | CLOSED |
| PINNED_FILE_BYTES BIG link | Line 763 links existing big helper rather than nonexistent BIG. Additional removal of public-to-private comparison/cap links preserves meaning and avoids invalid public-doc navigation. Independent library private docs -D warnings passed per r2 verification. | CLOSED |
| Server fixture sha/tarball/identity/stored | pg_code_review_cache.rs:47–53,63–84,192–204,245–268 supplies actual nonhex/archive/gzip/repository-label/directory-entry/root-path panic conditions. Full bodies inspected, including strip_prefix expect. Typed constant/UUID invariants do not demand speculative new failure descriptions. | CLOSED |
| StorageHandle::create_object | handle.rs:393–436 Cancellation explains unseen successful write, unchanged create-only retry, existence versus equality. Body uses native PutMode::Create and maps only AlreadyExists to false. Adapter create delegates; library adoption still reads and byte-compares before CAS. | CLOSED |
| CodeReviewMetadata::tenant commit boundary | review_cache.rs:214–242 documents atomic statement and uncertain committed response once at the shared owner. Body executes through TenantConn, commits before returning and owns no object/provider IO. Durable callers inherit that boundary. | CLOSED |
| Object create/delete retry semantics | review_cache.rs:85–135 documents AlreadyExists does not establish equality and absent/idempotent delete permits retry. Full adapter bodies agree with storage methods and retain Backend distinctions. | CLOSED |
| Snapshot publish/touch/claim/delete/remove pointers | review_cache.rs:284–395 documents moved-pointer repeat false and reread, persisted touch or Collecting, repeated claims, idempotent deletion. Full bodies and SQL INSERT/tree CAS, guarded touch, same-grace claim and deletes agree. No claim of arbitrary concurrency or altered grace. | CLOSED |
| Context upsert/delete/repository removal | review_cache.rs:400–529 documents original record/stamps retained, repeated delete succeeds, destructive removal repeats only remaining keys. Full bodies and SQL conflict no-op/DELETE RETURNING agree. The result is explicitly not replayable after committed removal; no blanket successful-cleanup claim. | CLOSED |
| R1-P docs-only / R1-H pin | Actual two incremental diffs preserve executable code. Wyrd manifest/lock resolves exact delivered library documentation revision; ancestry independently succeeds. | CLOSED |

Prior format/lints/build and focused owning-doc results are recorded in the
remediation Evidence and r2 verification; the library strict private-doc build
was independently rerun by the orchestrator. Integration-test rustdoc content
requires manual review and is covered above. The unrelated mandatory check:docs
failure is not folded back into FIND-004-1 or used to erase its actual closure.

## Recovery evidence assessment

System-review.md proposes no finding. Independent source/caller tracing agrees.
ContextStore::remove_repository (persistence.rs:313–324) is a destructive metadata
primitive returning keys for caller deletion; there is no Contexts removal facade.
Wyrd's implementation invokes delete_code_review_contexts inside tenant(), whose
commit precedes key return. SQL DELETE_CONTEXTS_SQL (code_review_cache.rs:96–99)
and function :372–387 delete rows and return keys. A lost committed reply can
therefore lose those keys and orphan bytes; repeat cannot rediscover removed
rows. Baseline library README at d8c772de:503 onward prescribes the same DELETE
RETURNING protocol; memory-store implementation :523–539 also removes records
while returning keys. This remediation documents rather than introduces it.

Library production callers implement/delegate the primitive but never invoke it
from pipeline context/expiry workflows; Wyrd search finds only the adapter and
SQL implementation. The task ships preparation/adapters, no offboarding service.
Normal Contexts::collect_expired (:396–422) validates identity/key and deletes
object before record, preserving lagging records for retry. Snapshot removal
instead drops pointers then collects record-backed objects with zero grace and
no-active-build precondition. Those owners cannot stand in as proof of replayable
context-offboarding receipts, and the present review does not claim they do.

The explicit approved storage amendment preserves library workflows and bounded
expiry/returning-key removal; it does not require restart-safe offboarding,
physical-erasure deadlines or persisted removed-key replay. Thus the technically
credible orphan scenario is not a supported-use regression or missing explicit
Task 004 outcome. A queue/tombstone/new facade would add persisted lifecycle
ownership; enumerate-without-delete would violate the trait; object IO inside
SQL transaction would violate the task. No such correction is justified. Carry
prior recovery acceptance only for its actually traced publication/collection/
expiry paths, with this primitive limitation explicit. No new finding or spec
revision follows.

## Proposed finding disposition and final deduplicated ledger

### FIND-004-2 — CONFIRMED

- Discovery source: **REPO-R2-001** (standards-review.md).
- Classification: **VIOLATION**, mandatory repository gate.
- Obligation: root AGENTS Testing prohibits circumventing gates and explicitly
  says a red gate is yours to fix whoever turned it red. The task-review skill
  preserves explicit mandatory gates independently of runtime probability.
- Exact location: `crates/wyrd-spec/src/github_connection.rs:4,6,7` (four bare
  module-doc destinations); runner `mise.toml:816–824`; crate enforcement
  `crates/wyrd-spec/src/lib.rs:8–9` and public module declaration :19–20.
- Source evidence: the introduction emits unqualified GithubConnectResponse,
  GithubSetupQuery, GithubCallbackQuery and GithubConnectionView links. Each
  target exists in public github_connection at :31, :118, :132 and :94,
  respectively; none is a root reexport. Independent managed check:docs run
  reproduced exit **101**, exactly four fatal unresolved-link diagnostics,
  followed by `could not document wyrd-spec`. This is not an inferred CI failure.
- Producer → consumer: module prose produces link destinations; the existing
  all-features public rustdoc command consumes them under broken-link denial.
  No runtime caller or external input is needed to reach this required gate.
  Sibling module links/target declarations and lib declaration were inspected;
  no target type or visibility change is necessary.
- Consequence/materiality: the required managed Rust documentation build fails
  on this immutable candidate. Attribution to another task cannot waive the
  explicit red-gate rule. This does not allege that Task 004 introduced a runtime
  regression and does not reopen the approved spec.
- Smallest correction: retain labels/prose and qualify the four destinations
  as `crate::github_connection::GithubConnectResponse`,
  `crate::github_connection::GithubSetupQuery`,
  `crate::github_connection::GithubCallbackQuery` and
  `crate::github_connection::GithubConnectionView`. Reuse the existing public
  module/types. Correct the invalid links at their producer in three doc lines
  (wrapping permitted), with no API, import, reexport, alias, dependency,
  configuration, warning policy or runtime change.
- Correction burden: trivial bounded documentation update. Deleting link markup
  would lose useful navigation; deleting the module/gate or excluding it would
  weaken required coverage. A helper/checker or warning sweep serves no task
  requirement and is rejected. Nonfatal private-link warnings are not retained.
- Focused closure proof: inspect the documentation-only diff and rerun
  `mise run check:docs`, requiring exit 0. No new tests, full journeys, aggregate
  gate, library revision or runtime migration is needed for this correction.

No duplicate finding or rejected optional proposal is retained. FIND-004-1 stays
closed under its stable ID; FIND-004-2 is the next unused number. This finding
originated with the independent standards specialist and was source/execution
validated here, satisfying the separate discovery/validation requirement.

## Recommendation and limits

Return FIX_REQUIRED and package only FIND-004-2 as one bounded link correction.
No change to product behavior or expensive decision is required. This is the
first source-remediation review; the two-unsuccessful-remediation convergence
threshold is not reached. The original omission is fixed; this remaining issue
is an observed mandatory gate, not an escalating runtime-hardening expectation.

Fresh execution here: managed check:docs fails as above; immutable identity,
status and library ancestry checks pass. Runtime/PG/storage/mutation/cloud suites
were not repeated for documentation-only changes. Prior approved 11/11 adapter
and 10/10 public pipeline executions carry with their inspected assertions;
real GCS remains unrun. The primitive lost-key limitation and existing
collection grace/non-overlap/no-active-build obligations remain explicit.
