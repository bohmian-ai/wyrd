# TASK-003 cumulative behavior review

Result: **FAIL**. The behavioral corrections close FIND-TASK-003-1 through
FIND-TASK-003-5. The original five diagnoses now have inspectable independent
reports, but the same missing-diagnosis-evidence cause recurs for the two new
failure-driven HTTP test adaptations. Retain FIND-TASK-003-6 with that narrowed
remaining scope. No production malfunction is inferred from this evidence gap.

## Immutable subject and scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-003`.
Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
Candidate: `ea317179820b24d8b08ed499454ce2cf4675ce4e`.
I inspected the cumulative `git diff ef3a57b07...ea3171798`; the
`9ffdf753a...ea3171798` range was only a remediation locator. HEAD remained
the candidate. This report is the only file I wrote.

Authority: original `TASK-003-bounded-outbox-admission.md`, its authorized
REQ-006/AC-005/AC-006/INV-001–005 specification scope, the prior verdict,
validated findings and evidence reassessment, and TASK-003-R1. Revision 14's
unapproved pre-release schema addition is excluded. Repository authority includes
AGENTS.md, agent rules, spec-driven development, maintainer style, testing/error
references, Wyrd's outbox/submission contract, security audit contract and
analytical operations/recovery boundaries. The implementation skill's diagnosis
rule governs the newly recorded failure adaptations. No current discovery
report was read.

CodeGraph exploration preceded code navigation, including the outbox,
verification service/runner and retained-reader/SQL acceptance owners. I read
the changed owner methods, relevant unchanged consumers, pressure stories and
the prior diagnostic reports rather than using the completion summary as proof.

## Caller-to-result and reuse assessment

* Direct HTTP/MCP and SDK verify reaches `VerificationControl::execute`
  (`service.rs:468`): authorization remains first, then a two-slot Scribe
  reservation precedes target loading/engine work. `stage_direct` spends that
  reservation for inputs and result. Pressure returns the derive-backed
  `VerificationStagingFull`; it cannot stage one source row before refusing
  the second slot. This retains in-memory acceptance, not a durable receipt.
* Submit reaches `enqueue` (`service.rs:619`): the existing Submission and
  request-key machinery still defines identity. It reserves the submission
  slot and Scribe slot before staging inputs. The reservation travels in
  `ScribeWrite::ExecutionInputs`. `ScribeSink::write` (`scribe_outbox.rs:913`)
  completes all frames before handing acknowledged non-lost submissions to
  their reserved downstream slots. A retryable frame refusal returns before
  handoff and retries the unchanged slice. `SubmissionSink::write` reuses
  tenant transactions and `VerifierExecutionQueue::accept_submissions`;
  `accept` (`verifier_executions.rs:1095`) checks existing execution identity
  and keyed replay under the existing transaction advisory lock. No second
  identity resolver, producer SQL write or durable-outbox workflow was added.
* The existing generic Outbox owns both admission shares and writer lifetime.
  `send` counts pending/reserved work before the channel send. Channel,
  waiting, failed and in-flight collections all retain their charge. Writer
  success/terminal loss releases it; failure keeps the exact slice and its
  charge. `Reservation::Drop` releases only unspent slots. Audit, span and
  capture siblings retain nonblocking `stage`; overflow counts/logs loss.
  The new `Admission`/`Reservation` state earns its place inside this existing
  owner; it does not duplicate an existing quota or queue service.
* Runner result admission (`runner.rs:361`) happens before execution; a full
  share defers without spending an attempt. Fenced settlement spends the slot
  only for an applied completion. Existing `ClaimLoop` now bounds spawned
  executions with `execution_slots`, and Operator delivery uses its existing
  global permit bound as its active limit. Existing leases, cancellation,
  tenant transactions and claim rounds remain the owners of durable progress.
* On concurrent shutdown, closing the sender precedes writer termination.
  Once both are fenced, only the caller taking nonzero `pending` takes
  `staged_reserved` and releases the pair (`outbox.rs:350–359`). A second
  caller returns before touching the paired counter. Late sends release only
  their own admission, and unused reservations retain their independent Drop
  ownership. This closes the prior split-owner interleaving without another
  lifecycle lock or service.

## Acceptance matrix

Verification listed as “recorded” is the candidate's inspectable command/result
claim, not an execution by this reviewer. “Coordinated” results were supplied
by the review orchestrator from the independent running reviewers.

| Requirement, criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Whole queued/waiting/retrying/in-flight work has finite process bounds | `outbox.rs:214–295,436–473,520–700`; both shares sum to capacity, releases occur on consumption/loss rather than dequeue | Bound/audit/retry/drain tests inspected; coordinated seven exact runtime selectors passed | PASS |
| Scribe ACK → submission handoff is bounded without losing accepted source work to overflow | `service.rs:691–706`; reservation stored in `ScribeWrite`; `scribe_outbox.rs:949–959`; `submissions.rs:48–60` | SDK pressure stories and real HTTP pressure test; recorded Green, coordinated HTTP pair 2/2 | PASS |
| Ordinary active execution work is bounded at claims | `claims.rs:157,238`, runner constructor and `RuntimeLimits::execution_slots` | Existing cap test and adjusted 70/200-slot regression tests inspected; Report B explains their separate intended coverage | PASS |
| Typed retryable refusal before source staging; SDK/HTTP/MCP parity | `execute` reserve(2), enqueue's two reservations; `error.rs:1986`; shared HTTP mapper adds Retry-After | All three SDK pressure stories, MCP `staging_pressure_is_retryable`, HTTP mapper and HTTP pressure seam inspected; recorded focused passes | PASS |
| Successful calls retain existing durability, retry identity and crash/deadline loss semantics | Existing Scribe frame identity/retry loop and Submission queue acceptance preserved; outbox writer retains failed slice | Exact retry/drain/deadline selectors; coordinated runtime 7/7 | PASS |
| Audit remains nonblocking/countable, with no invocation audit-write error | `Outbox::stage` bounded unreserved share and `count_lost`; audit/span/capture use the same owner | `bounded_audit_staging_remains_nonblocking`; no new authorization failure path | PASS |
| B completes while A continues controlled offers, then accepted work drains/reconciles | Rust story `verify_in_real_time.rs:1254–1295`, Python `:487–505`, TS `:459–479` offer normal A calls until B's wait completes; held slow A still active; final row count is `3 + offered` | Existing three real SDK stories extended, recorded 1/1 each, source inspected independently | PASS |
| Exact-key refusal → drain → retry/replay preserves identity and one durable execution | HTTP `pg_verification_routes.rs:496–541` uses one body/JWT/key before and after refusal; `(3,1)` inputs before accepted replay avoids inventing source-row replay deduplication | Coordinated managed-Postgres exact pressure/replay pair 2/2; existing SQL replay owner independently traced | PASS |
| Concurrent shutdown settles each loss/share once; unused reservations remain held | `outbox.rs:350–359`, new regression at `:1251` | Coordinated exact shutdown regression passed; exclusive-winner source argument above proves the interleaving | PASS |
| Touched Rust Errors/Panics documentation is complete for prior cited items | Execute/enqueue Errors, Scribe constructors, bound/audit tests, HTTP mapper, new helper/test and retained reader docs | Source inspection; recorded fmt/lints/target Clippy passes | PASS |
| Normative security/recovery prose agrees with finite staging and nonblocking overflow | `wyrd-security-posture.md:469–487`; `reliability-and-recovery.md:73–81`; Wyrd design submission paragraph | Static consistency against stage/reserve/write/shutdown | PASS |
| Original diagnoses have independent, honest cause reports; new failure-driven test changes obey same rule | Reports A/B preserved; C recovered original; D/E explicitly retrospective; R1 evidence row 151 introduces two further test-failure corrections with no corresponding report | Reports C/D/E checked against current/historical owners; no report covers the two new HTTP adaptations | FAIL |
| Keep one Scribe writer, Rust-owned durable logic, tenant isolation and portable facilities | Existing ScribeOutbox/SubmissionOutbox/ClaimLoop extended; language changes project test-only setup and real SDK journeys | Cumulative source/diff inspection; no new SQL, dependency, limiter, queue/transport or platform gate | PASS |
| Exclude distributed quota, strict fairness, stronger receipt, loss-free crash claims and unrelated grouping/schema work | Reserved/unreserved shares are local item accounting; queue remains asynchronously committed; no grouped public API/schema addition | Cumulative diff and original task scope audit | PASS |
| Standalone proof is independent; grouped J-005/capacity qualification remains assigned to integration/TASK-004 | Ordinary verify/submit tests do not require group_id; original task explicitly records grouped seam OPEN | No grouped/capacity claim made by this review | PASS |

## Prior-finding closure

| Stable ID | Current source-backed disposition |
|---|---|
| FIND-TASK-003-1 | CLOSED. The pending-swap winner alone owns final reserved-count settlement. New test verifies once-only loss metrics and retained unused reservation; normal retry/drain/deadline tests remain. |
| FIND-TASK-003-2 | CLOSED. Each SDK story continues A offers after accepting B and waits for B completion before stopping that loop, retains held A and pressure/refusal assertions, then reconciles all accepted A source rows. No strict-share guarantee was introduced. |
| FIND-TASK-003-3 | CLOSED. The added real HTTP test controls exact key/body/authority through pressure and retry; returned execution identity and existing SQL replay owner establish the standalone seam. It deliberately counts inputs before accepted replay. |
| FIND-TASK-003-4 | CLOSED. The cited Errors/Panics omissions and newly added Rust helper/test docs have substantive contracts. |
| FIND-TASK-003-5 | CLOSED. Security and recovery authority now describes finite admission and counted/logged nonblocking audit overflow without changing ACK/retry/crash boundaries. |
| FIND-TASK-003-6 | PARTIALLY CLOSED / RETAINED. Original diagnoses 1–5 now have independent reports with honest temporal limits. Two new unexpected test failures were corrected during R1 without inspectable independent diagnosis reports; this is the same evidence discipline defect, not two new IDs. |

## Proposed finding

### B-R2-001 — Retain FIND-TASK-003-6 for new failure-driven test adaptations

Classification: **VIOLATION**, acceptance-evidence gap. Location:
`TASK-003-R1-close-admission-review.md:151`; affected current test code:
`pg_verification_routes.rs:466–474,518–524`.

The R1 evidence explicitly records two failed runs caused by errors in the
test itself, followed by edits: moving the queue assertion until after ACK/drain,
and removing a VARCHAR cast whose Arrow view representation the retained reader
rejects. These are unexpected failures of the proof, not the intended Red that
demonstrates a missing production capability. The implementation skill
(`.agents/skills/wyrd-implement/SKILL.md:72–89`) requires a fresh read-only
diagnostician before a fix touching a test/assertion, separately from its
ordering/concurrency trigger. R1 repeats this requirement at line 108.

The supplied independent reports A–E cover only the five original diagnoses.
Neither the R1 row nor the supplemental reports include an independent report
for these two new adaptations. The stated reasons are plausible from source:
`enqueue` carries the Submission with inputs, `ScribeSink::write` hands off
only after ACK, and `SubmissionSink` commits later; `retained_records` uses
`audit_cell_text` (`server.rs:142–159`), which accepts StringArray or Variant
and rejects other physical arrays. This source reasoning does not establish
the missing independent failure diagnosis or the actual historical traces.

Observable consequence: R1's newly added acceptance proof was changed after
unexpected failures without the inspectable independent cause evidence required
by the same retained finding. This blocks declaring complete evidence closure;
it does not establish a wrong assertion, tenancy weakening, or product bug.

Smallest correction: preserve Reports A–E and all source-supported fixes. Recover
the independent report for these two failures if one exists. Otherwise obtain
one fresh retrospective, read-only diagnosis for this cohesive HTTP proof owner,
using the failing command/output and historical diff rather than an implementer
hypothesis. It must distinguish the ACK/queue-read ordering mistake from the
projection/decoder mismatch, inspect sibling retained readers and enqueue/replay
callers, identify their existing owners, and state what historical evidence is
unavailable. Add it beside the R1 diagnosis evidence with honest temporal limits.
No source change, broader test suite, new reader, or production mechanism is
required solely by this finding. The current independent HTTP 2/2 pass is Green
evidence and need not be rerun merely to attach the missing report.

## Verification and limits

I ran no Cargo, SDK, provider, build or lint lane. The orchestrator reported
independent current-candidate runs of the seven exact outbox selectors (7/7)
and managed-Postgres HTTP pressure/replay pair (2/2, tracing enabled). These are
coordination evidence, not this reviewer's runs. Native rebuilds, all three SDK
focused passes and static lanes remain candidate-recorded claims; their source
coverage is assessed above. The grouped seam and benchmarks remain unrun and
are not prerequisites for this standalone task review.

I ran `git diff --check ef3a57b07...ea3171798`: exit 2 for trailing whitespace
in verbatim recovered transcript excerpts at `supplemental-diagnoses.md:72,209`.
This is a nonblocking evidence-format note, not another behavioral finding or
reason to rewrite retained transcript evidence. It differs from checking only
an empty working-tree diff. HEAD was rechecked as the immutable candidate;
the only pre-existing untracked area was this new review directory.
