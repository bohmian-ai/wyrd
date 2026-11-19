# Scoped system resilience review

**PASS — no material finding in the assigned recovery/documentation scope.**

## Subject and scope

Wyrd base `81aca5479145b2b94d5db4e0d15816d82cf40bc7`, prior candidate
`359a19f41597b8cfdc673373409b0d96e8298447`, candidate
`b5c0db2e01268d6ad033fbe7450069247edff6ee`. Library base
`d8c772de4248ce0130f79ec3b5fefda518b1431a`, prior candidate
`d380f57fa53429eaa4f3e7272ce651e1825a8b38`, candidate
`9046d1035538df8dfb661f826b102316ee6d0b4e`. Both checked-out HEADs match.
Approved spec revision 6 and original Task 004 apply; approval is not reopened.

This review fills one precise gap in the prior system report: whether newly
explicit uncertain completion of context repository removal violates required
cleanup recovery. It does not rediscover accepted publication/CAS/lifecycle
behavior. Prior system evidence at `../004-pinned-pr-preparation-r1/system-review.md`
continues for those unchanged paths. I read the approved verdict/remediation,
not other new reviewer conclusions.

The runtime-relevant incremental Wyrd changes are rustdoc plus the library
revision pin; the external library diff changes only documentation. Report/task
evidence additions do not change runtime behavior. No new cleanup scheduler,
startup dependency, restart loop, process exit, shared-service health policy, or
production call to repository removal is introduced.

## Authority and source coverage

Read the review skill and scope, root rules, SQL directory rules, maintainer
style, spec-driven workflow, original task and its W4/W5/W7/W9 obligations,
spec REQ-006 and invariants, and approved support design's Task 004 storage
amendment. Owning architecture is `wyrd-design.md:2098–2112` (cache custody) and
`bifrost-design.md:653–657` (cache outside retained evidence). These preserve
library protocols, cross-tenant expiry enumeration, tenant-owned deletion and
evidence isolation; they do not define a crash-recoverable repository-offboarding
workflow or physical erasure deadline.

CodeGraph was used before navigating Wyrd source. The library has no CodeGraph
index. Inspected actual bodies and consumers in:

- Library `context.rs:282–422`, `persistence.rs:250–324`,
  `snapshot.rs:423–456`, `review.rs` read/context entry points, and
  `testing.rs:507–540,684–688`.
- Library README storage reference and operations table, including the same
  reference implementation at original base `d8c772de`.
- Wyrd `review_cache.rs:85–136,214–242,400–529`, SQL
  `code_review_cache.rs:80–99,310–387`, SQL integration assertions at
  `pg_code_review_cache.rs:282–306`, and server integration assertions at
  `pg_code_review_cache.rs:620–650`.
- Repository-wide Rust caller searches for `remove_repository` in the library,
  and server/SQL consumers of `delete_code_review_contexts`, `CodeReviewMetadata`,
  `Contexts` and `collect_expired`.

## Actual deployed and failure paths

There is **no `Contexts::remove_repository`**. The context facade supports publish,
read and expiry collection. The primitive is `ContextStore::remove_repository`;
it has no production caller in Wyrd or the library pipeline. Library testing
stores implement/delegate it. The README tells an offboarding caller to invoke
that primitive and delete returned keys, with no builds in flight. Wyrd's actual
SQL tests call its SQL query directly to assert repository scoping/removal; they
do not constitute an offboarding service.

The adapter primitive runs `DELETE … RETURNING record->>'key'` through
`CodeReviewMetadata::tenant`, obtains keys inside the transaction, awaits commit,
and only then returns them. SQL or commit failure maps to `PersistenceError::Backend`.
A process interruption/lost reply after commit can therefore remove metadata
without delivering the keys. A repeat returns only rows still present. If no
other actor retained the keys and objects remain, normal expiry enumeration
cannot discover them because it queries remaining context metadata. The storage
trait supplies no listing-based orphan recovery. Similarly, a caller that
receives keys and crashes before deleting objects must itself retain those keys
if it wants recovery. The new method docs accurately describe these limits; they
do not promise keys are replayed. The consequence is possible orphaned cache
objects, not cross-tenant reads, accepted-evidence loss, or shared-server failure.

This is also exactly the original library primitive/reference behavior: baseline
README `remove_repository` uses destructive `DELETE … RETURNING`, and the memory
store removes records while returning keys. The candidate preserves the required
protocol rather than introducing this ordering or changing a supported caller.

Normal supported expiry cleanup has different ordering. `Contexts::collect_expired`
first enumerates records, validates each key against its identity, deletes the
object, then deletes its record. Object deletion failure leaves the record and
adds lag while other records continue. If interrupted after object deletion but
before metadata deletion, the next sweep repeats idempotent deletion. If the
final SQL delete committed but its reply was lost, the object has already been
deleted. Failure to enumerate is returned explicitly, not converted to empty
success. Wyrd's adapter preserves this ordering and error distinction. No
repository-removal primitive participates in this path.

Snapshot repository removal is different again: `Snapshots::remove_repository`
drops pointers then calls record-backed `collect` with zero grace; collection
deletes each object before its record. Its documented retryability is not
evidence that the context metadata primitive has the same recovery semantics.

## Obligation and proof assessment

| Scoped obligation | Source and proof | Result |
|---|---|---|
| Preserve context protocol; bounded expiry and repository removal support object cleanup | Trait returns removed keys; adapter/SQL do so; SQL test asserts `k-live` returned. Baseline library uses the same primitive. No promised replay is removed. | PASS |
| Preserve retryable collection/expiry cleanup | Actual expiry facade deletes object before record; library `expired_cleanup_deletes_object_then_record_and_reports_lag` asserts failed object deletion keeps metadata and retry converges. Server real-adapter test asserts expiry collection deletes both metadata and object (`:629–650`). Prior snapshot interruption evidence carries. | PASS |
| Accurate uncertain-completion/retry rustdoc for the affected primitive | `review_cache.rs:505–527` states interrupted deletion can omit previously removed keys; transaction body and DELETE RETURNING prove it. | PASS |
| No runtime recovery regression or adjacent evidence/availability change | Incremental docs/pin diff, unchanged callers/SQL, isolated cache namespace and architecture custody. No new removal caller or service lifecycle. | PASS |

The prior report did not cover this exact removal primitive; its general cleanup
acceptance must be understood as the concrete record-backed collection/expiry
paths it traced. That proof does not establish durable offboarding key replay.
The present targeted trace closes the gap without asserting that stronger claim.

## Materiality and correction burden

The orphan scenario is technically credible for a caller of the primitive, but
no explicit Task 004 obligation requires restart-safe offboarding receipts,
replayable removed-key results, or garbage collection independent of metadata.
The task explicitly preserves existing library workflows/protocols, including
its removal primitive, and ships adapters/preparation rather than a Wyrd public
offboarding action. The approved design specifically attaches retryable
idempotent cleanup to the SnapshotStore row and gives ContextStore bounded expiry
and returning-key removal; actual expiry remains retryable. Broad cleanup wording
cannot silently require a different repository-removal lifecycle.

A docs-only correction cannot make destructive metadata removal replayable.
Changing the query to enumeration would violate the trait's removal contract;
deleting objects inside its transaction violates the explicit no-transaction-
across-object-IO rule. A durable removal queue/tombstone or new facade orchestration
would add persisted lifecycle/ownership behavior. No such correction is justified
by the supported Task 004 path, and no remediation or specification revision is
requested. A future crash-recoverable offboarding workflow would need its own
required outcome and owner; it is not a hidden acceptance criterion here.

## Limits

Static review only; no tests, process kills, service outages or cloud operations
were run. Existing assertion bodies were inspected and prior unchanged execution
evidence carried forward. No source was mutated; only this report was written.
The known primitive limitation remains: lost returned keys can leave context
cache objects orphaned if a future caller uses destructive repository removal
without separately retaining cleanup work. This scoped PASS makes no stronger
physical-deletion or offboarding-recovery guarantee.
