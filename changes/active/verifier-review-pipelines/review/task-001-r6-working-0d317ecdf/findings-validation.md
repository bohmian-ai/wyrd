# Independent R6 findings validation

**Recommended verdict: PASS.** The independently validated final material
finding ledger is empty. FIND-TASK-001-19, FIND-TASK-001-23 and
FIND-TASK-001-27 close; the original unnumbered hosted-dependent observation
and approved USER-RESULT-IDEMPOTENCY-001 are satisfied within their recorded
boundaries. No new specification decision is required.

## Subject and method

The subject is the explicitly submitted working source on HEAD
`0d317ecdf2d28d2e77b6f394ebe0f443d256d580`, pinned by this directory's
subject-sha256.txt and packet-sha256.txt. Both manifests passed at entry and
again after validation. R3 `d89356985566ab3492699682942438c25c8322fd` and original
base `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb` remain cumulative context. This
uses the expressly authorized working-tree exception, not a fabricated R5 or
R6 commit. No staging, source, test, task, authority or manifest change was made.
This report is my sole authored output.

Read the task-review skill, shared subject/navigation/applicability record,
current data and system reports and verification.md, R5 verdict and independent
validation, and relevant R5 invariant, maintainer, data and system evidence.
Original Task001 revision1/R4 and the R6 task retain the acceptance context;
the latter separately records the explicitly approved result-idempotency
addition. Root and SQL rules, maintainer/Rust documentation guidance,
spec-driven-development authority, implementation-execution, TESTING, approved
terminal supplement revision3, Bifrost identity/retention and security tenant
binding govern the reviewed boundary. Unrelated draft TASK007 is excluded.

Used CodeGraph before source navigation. Independently inspected the exact
HEAD-to-working outbox diff (that owner was unchanged at R5), complete changed
frame construction and supporting shares/chunks/encoding/identity/submission/
tally/ACK bodies, result builder and both queued/direct producer composition,
the complete new restart/conflict, capacity and hosted-dependent tests, retained
row helper, changed documentation and SQL plan assertion. Traced the affected
result readers/group completeness and existing strict fingerprint/durable
fence suppression paths. R5 accepted production terminal orchestration is
carried from its source-backed reports; its affected result reconstruction and
receipt are rechecked here. A green count or matching file hash alone is not
acceptance evidence.

## Independently assessed specialist coverage

| Contract / invariant | Coverage and omission assessment | Result |
| --- | --- | --- |
| Stable tenant/table result identities, strict content, durable acceptance and replay | Fresh data specialist covers actual producers, keyed framing, generic retained fence, partial/conflicting writes and result consumers. Source confirms that map; the report's empty proposal ledger is supported. | Required domain review present |
| Failed settlement, interruption/partial ACK, exhausted capacity, hosted consumer and shared capability availability | Fresh system specialist traces the deployed outbox/runner/queue route and complete proof assertions. Current source confirms operation-local refusal, unchanged recovery ownership and no service termination. | Required system review present |
| Shared RunState decision, memory locks, ticket coverage and claim lifecycle | R5 invariant report and independent validation provide source-backed accepted rows. R6 changes no production RunState/runner/claim state machine. New physical frame/receipt invariants are covered by current data/system reviews and this validation. | Separate invariant specialist reasonably omitted |
| Ownership, abstraction and Rust rules | R5 maintainer report accepts the existing terminal owners, identifies the exact residual documentation gap, and covers changed test/support composition. R6 reuses existing Frame.key, ResultPayload and batch fence, adding no owner, configuration or dependency. Current local documentation and tests are inspected here. | Separate maintainer specialist reasonably omitted |
| Broader security, cross-tier/SDK/platform/native lifetime rules | Prior R3/R5 source-backed security/standards acceptance covers exact identity, File custody, grants and projections; those owners and governing authority remain unchanged. The new hash includes tenant/table and changes no permission or transport principal. Fresh data review covers this affected tenant boundary. No wire/SDK/platform surface is added. | Additional specialists reasonably omitted |
| Independent validation and proposal reconciliation | This fresh validator inspects source, acceptance, all selected current reports, prior residuals and omission justifications. No reviewer-only new blocker or conflicting proposal remains. | Satisfied |

Carry-forward is limited to the prior reports' accepted rows and proof limits:
common terminal operation/no second acknowledged-failure pass, actual
cancellation/revocation/admission producers, exact frozen identity, short locks,
accepted-prefix recovery, publication/File custody, checkpoint charge and
protobuf retirement, original bounds and direct successful fast return. The
R5/R6 manifests establish unchanged relevant owners/authority; prior evidence
establishes their acceptance. Changed framing is not carried without inspection.
No required report is missing. No follow-up discovery is warranted: there is no
material conflict or unresolved path, and I discovered no new blocker requiring
separate focused validation.

## Source validation and requirement/closure matrix

| Obligation | Implementation and producer-to-consumer evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| USER-RESULT-IDEMPOTENCY-001: independent staging retains selected result identity | `scribe_outbox.rs::frames:1268` isolates each Result row under `payload.result_id()`; `Frame::shares:1233` excludes keyed frames. `chunks:1363`, `submit:1648` and `batch_id:1593` make its one-row identity tenant + destination table + `(result_id,0,0)` + start0. Slice/order/neighbors cannot change it. Output and summary tables differ. Other write kinds retain absorption and existing bounds. | `result_redelivery_keeps_batch_identity` writes five results across three differently composed slices, compares exact bytes/IDs, requires ten one-row frames and six distinct IDs for three results/two tables. Root independent unit 1/1 PASS. | PASS |
| Reconstructed rows preserve strict content and correlation | `runner.rs::conclude:1480` resolves canonical selection; `result:1581` builds with selected ID/interval, frozen run/verifier and subject/SYSTEM attribution. `results.rs::build_as:318` authors exactly output and summary. `preprocess.rs::logical_data_identity:505` retains nonvolatile fields and complete Arrow representation (including offsets/validity/buffers); no digest relaxation is introduced. Direct service staging uses the same Result projection with its existing attribution and no new wait. | Restart test compares complete sent batch identity and bytes per table across claims, including drifting output; retained reads count physical rows by result_id. Root integration PASS. | PASS |
| Matching accepted write does not append; changed content remains conflict | `scribe_batch_commits.rs::classify:588` distinguishes AlreadyCommitted from Conflicted. `shards.rs:3911` marks accepted repeats spent and suppresses insertion; `refuse_conflict:4052` sends BatchConflict. Existing retained replay resolves canonical WAL coordinates. This is reuse of the existing fence, not a second acceptance registry or reader dedup correction. | `result_redelivery_is_idempotent_after_restart:2795` covers accepted result/failed settlement/fresh claim and output-only acceptance/restart, requires `(1,[failed])`, zero recovery engine entries, matching batches, then conflicting rewrite with closed ACK and unchanged retained data. Root PASS. | PASS |
| Partial/rejected result cannot retire delivery | `ScribeSink::write:1885` awaits detail phase then summary phase and existing group activity; Result ACK requires its owner index absent from lost. `tally:1750` marks rejected owners lost and retains retryable refusal. `runner.rs::deliver:1527` maps absent receipt to Defer; existing computation and delivery settlement fences remain owners. | Restart partial case first requires `(1,[])`; P5 holds output, cancels, fences stale attempt, refuses summary, then retires delivery under the original ID only after required receipt, with one retained row/table. Lost sealed-ACK test separately verifies transport retry. All root PASS. | PASS |
| Result/group/completion consumers preserve logical selected result | `inputs.rs::result:409` joins summary/output by exact result_id and input by execution_id; group read/decode/Visible::result retain completeness and result-keyed logical selection. These consume row identities, not frame position or aggregation. R5 validation traces queue/group/Operator idempotent completion effects; new physical dedup does not relocate those effects. | Current retained-row and zero-engine observations plus R5 source-backed completion evidence. No broader external-effect guarantee inferred. | PASS |
| FIND19 separate source and real reserved-share outage on exhausted recovery | `pg_verification_runtime.rs:2283` preserves source grant outage, stops both earlier runtimes, obtains the actual shared outbox free reservable share and asserts reserve(1) refusal. Recovery builder omits publication_fault, so `verification/mod.rs:553` selects that same outbox. Refusal occurs before state load; exhausted Defer maps to LeaseLost (`runner.rs:1370`). No synthetic payload or new capacity control exists. | Test baselines refusal metric only after earlier runtimes stop; blocked row stays running/attempts1 and commits contain only running claims. Release permits original selected completion with attempts1 and zero engine entries. Root exact PASS; implementation-attributed mutation dropping the hold fails the refusal observation. | CLOSED |
| Original hosted A/B→C observation | `pg_durable_verifier_work.rs:2996` runs real HostedBlocks/with_checkpoints/RunState with left/right and declared combine inputs. Revisions are released singly; when combine's pending start appears, accepted state must already hold both complete predecessors and no combine. Dropping the run there and reclaiming exercises actual consumer restoration. | Real requirement requires `[1,two]`; resumed outcome Passed, A/B settlements unchanged, and left/right/combine each attempt1. Root exact PASS. Existing accepted-prefix/takeover proofs retain earlier evidence; this test closes the specifically missing hosted seam. | CLOSED, unnumbered |
| FIND27 bounded SQL plan gate | `verifier_executions.rs:4372` documents/allows the retained delivery_due index alongside the two computation indexes; still permits only Index/Index Only Scan. No production query or migration modification is selected. | Root exact SQL library selector 1/1 PASS and both migration apply/idempotence checks 1/1 PASS. | CLOSED |
| FIND23 mandatory changed-item rustdoc | `settle_delivery:3397` states matching claimed_until, caller transaction/commit, stale no-op and database error. `state.rs:410` Counted write/flush document saturation/no retained buffer/always-Ok behavior. Changed Result/frame/chunk and new test/helper docs describe their invariants and panic conditions. | Static source inspection; implementation-recorded fmt/lints supporting evidence. No invented error path or new checker. | CLOSED |
| Previously accepted terminal/security/bounds/charge/protobuf/direct behavior | R5 independent matrix and invariant/maintainer/data/system reports, rooted in R3 accepted rows; unchanged owner hashes and unchanged authorities, with result producer/receipt/consumer dependencies rechecked above. | Prior source-backed evidence retains its stated execution limits; current focused owner checks corroborate affected paths. | PASS carried forward |

## Final deduplicated ledger and convergence

**Validated retained material finding ledger: empty.** Both current discovery
proposal ledgers are empty, and independent source validation supports them.
No finding is rejected merely because another reviewer omitted it. Prior
FIND19/23/27 have concrete closure evidence above; the hosted observation stays
unnumbered. USER-RESULT-IDEMPOTENCY-001 remains a human-approved addition and
receives no FIND number. No new numbered finding is assigned.

The minimum complete implementation is already present: choose the existing
result UUID at the existing frame owner, reuse keyed-frame isolation and generic
batch acceptance, finish existing-seam tests, restore the valid plan alternative
and document the three local methods. Deletion of the key would reintroduce the
demonstrated independent-slice duplicate; another durable registry, generic
framework, reader DISTINCT or strengthened distributed fence is unearned.

R5's remaining capacity/hosted gaps were incomplete carried proof; FIND27 was a
test regression and FIND23 a mandatory local gate. R6 supplies those proofs and
repairs without another terminal redesign. The approved addition introduces a
bounded, explicitly accepted two-frame-per-result cost and is independently
tested. The evidence now converges; further rediscovery is not required to PASS.

## Verification attribution and limits

I ran no Cargo or database command. Current verification.md records root's
serialized independent pinned-candidate runs: unit
`d52b5c3c-ef87-4c49-aae2-51f951f6f761` 1/1; server
`78c35472-5941-4261-8e92-7123fd774ade` 5/5; SQL
`fe1c6de4-5bad-4adc-b223-1397c4c2e15e` 1/1; Wyrd/Vala migration checks each
1/1. All exit zero. Broader 28/28 and 84/84 owner runs, fmt/lints and mutation
evidence remain explicitly implementation-attributed. `git diff --check` passes.
Both manifest checks pass after this report is written; review reports are
excluded from the candidate pin.

Application-runtime restart/reclaim uses the same real Scribe/Postgres owners;
it is not a whole-host/Scribe-cluster crash experiment. Existing durable fence
and replay source/proof carries its prior limits. Idempotency is within retained
batch fences and the current deterministic producer content, not indefinite
retention, arbitrary cross-version producer equivalence, historic duplicate
cleanup, global result_id uniqueness over arbitrary raw producers or exactly-once
external effects. Those are not approved obligations for this round.

Skipped broad journeys, aggregate gates, benchmarks and unrelated discovery.
Risk: deduplication retains the existing batch-fence retention boundary; full
integrated change verification remains separate.
