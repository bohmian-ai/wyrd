# Independent findings validation — TASK-009

Recommendation: **FIX_REQUIRED** for this immutable correctness/proof review. Four material findings are retained below. No runtime admission, persistence, tenancy, recovery, or ownership defect is inferred from the proof/documentation findings. TASK-009 also has downstream measurement and delivery obligations; those remain outstanding in their original V13–V16 sequence.

## Subject, inputs and method

- Base: `6f38db16c524417301ffc4a5dc0b478daf997ed1`.
- Candidate: `f0ea3b4023e544d0f95e9b5e38d398105dbe2e33`; HEAD independently checked unchanged.
- Approved specification: `changes/active/benchmark-experiments/spec.md`, revision 24, particularly REQ-035–039 and AC-015–019.
- Original task: `changes/active/benchmark-experiments/tasks/TASK-009-unified-queue-and-closeout.md`, revision 2. This is its first review; no previous FIND IDs or closure verdict are carried forward.
- Read this directory's `scope.md`, `verification.md`, `claim-comparison.md`, and all five selected reports: `task-review-behavior.md`, `domain-review-persistence.md`, `system-review.md`, `standards-review.md`, `maintainer-review.md`. No selected report is missing.
- Read `.agents/skills/wyrd-task-review/SKILL.md`, supplied root rules, applicable nested SQL/Python/TypeScript/docs rules, maintainer style, agent-doc index/Rust guidance, spec-driven workflow, architecture authority map, and relevant complete owning architecture paragraphs. CodeGraph preceded code navigation. Read actual candidate source and the final two executable patches rather than accepting reviewer agreement as proof.

This pass validates the proposed union and specific coverage gaps, rather than repeating discovery. Source tracing includes complete `bound_verifier`, Observe submission/projection/enqueue and target methods, `Target::enqueue/execute`, the complete new V18 journey, `InFlightSet`/`InFlight::drop`, background handoff and staging branches, complete Scribe successful-write handoff, complete shared admission writer, and SQL scheduled/group acceptance methods. Reviewed the shared outbox clock decision and its focused regression for the empty system/maintainer ledgers. No new blocker originated in this validation, so separate new-finding validation is unnecessary.

## Proposal disposition

| Discovery ID | Decision | Stable finding | Independent conclusion |
|---|---|---|---|
| B-01 | REVISED | FIND-TASK-009-1 | Final-source V13 gate evidence is missing. Keep this an original evidence obligation; do not make a new full aggregate a per-finding remediation check or imply a benchmark run is currently authorized. |
| B-02 | REVISED | FIND-TASK-009-2 | V18 omits its explicitly required public Run submission path and does not exercise Window observation authoring through Observe. Retain a bounded journey correction, without claiming production admission is broken or demanding new journeys on every language. |
| DP-001 | CONFIRMED | FIND-TASK-009-3 | Existing normative owner paragraphs still prescribe commit-before-selector-publication, contradicting the approved ACK-first flow and newly edited paragraphs. |
| REPO-001 | CONFIRMED | FIND-TASK-009-4 | The new `InFlight::drop` has no method rustdoc despite the explicit changed-item merge gate. |

No duplicates share a correction owner: these concern final proof provenance, required SDK journey coverage, normative architecture consistency, and a lifecycle item's documentation. Batch their bounded source/documentation work in one remediation; retain the original integrated closeout separately. No rejected proposal creates optional work. The two nonblocking standards observations (tuple decoding and test-local imports) establish no source-proven material failure in this candidate; they are not promoted into remediation findings.

## Validated ledger

### FIND-TASK-009-1 — Final executable candidate has no attributable V13 gate

**Source:** B-01. **Status:** REVISED. **Classification:** MISSING.

**Obligation:** REQ-038 requires the final integrated candidate to pass the complete gate; AC-019 requires reproducible gate evidence. Original V13 explicitly requires affected proof and renewed final gate evidence after any final source/test/config change, and says to record the SHA, invocation, overall exit, lane outcomes and log path.

**Locations:** task V13 at `changes/active/benchmark-experiments/tasks/TASK-009-unified-queue-and-closeout.md:544` and evidence at its `### V13 gate`; executable changes at `crates/shared/wyrd-client/src/state.rs:888`, `crates/shared/wyrd-client/src/observe/mod.rs:511`, `crates/wyrd/wyrd-testing/src/gateway_capture.rs:445` and `:532`.

**Source/provenance evidence:** Independently read external `gate2-sha.txt`: `e9e3d95cb57249a37238235259437dd8a64fab98`. `gate2.log` ends with `Finished in 1210.73s` and `gate=0`. Its directory is the scratchpad in `verification.md`. Git chronology shows evidence-only `19f4190a8`, then executable commits `da366eefe` and `f0ea3b402`. The first changes a typed hydrated Verifier lookup from serialization/reparse to borrowing, consumed by Observe verify and both submission stages. The second moves the paced upstream's buffered-answer construction from each request to startup and shares `Bytes` through Axum state; the complete buffered and stream branches retain timing/recording behavior. These are real executable differences after the proved gate. No supplied final-source full gate exists; the orchestrator's one passing outbox regression proves its named case only.

**Reachable consequence/materiality:** Supported Run/Window client authoring and the measured Gateway upstream execute later code. The older genuine aggregate cannot establish their final integrated acceptance. This violates an explicit proof requirement even though neither optimization has a demonstrated runtime defect.

**Smallest correction and burden:** Preserve the source changes if wanted, settle the bounded corrections from this review, then fulfill original V13 once for that final executable source using the existing gate and recording exact provenance. Reuse available unchanged proof; do not repeat already-covered component suites as a checklist, invent a harness, revert correct code merely to reuse an old log, or relabel the earlier gate. The only additional burden is the already-authorized original final proof, not a new product or architecture choice.

**Focused closure:** Remediation corrects evidence attribution and exercises its changed behavior through narrow exact-selector/static checks. Keep final-source V13 explicitly pending unless genuine same-source aggregate evidence exists. Reconcile final executable SHA, gate log and lane results at original integrated change-review/closeout; do not require a broad aggregate inside the review-produced remediation or repeat it per correction. No benchmark or publication operation belongs in this review.

### FIND-TASK-009-2 — Required public authoring path is absent from V18

**Source:** B-02. **Status:** REVISED. **Classification:** MISSING.

**Obligation:** Original V18 requires a real SDK → WyrdTestServer/Postgres/Bifrost → SDK journey using actual Run submit, scheduled Window observations and grouped source inputs, with ACK gating/fixed readback and direct verification while archival is paused. V11 delegates the new ACK assertion to that journey. REQ-035 preserves Window submission as observation recording; REQ-038 requires real client journeys. No stronger authority prohibits this bounded proof.

**Location:** `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:7632`, complete `queued_origins_share_ack_gated_admission`.

**Producer-to-consumer evidence:** V18 calls public HTTP through `WyrdClient::request_json` for `/v1/verification/enqueue` and `/execute`. It writes drift records using `Bifrost::write_batch`, activates/binds the schedule with `VerificationFixture`, and uses typed `Execution` for readback. It does prove the server ACK boundary, all-origin completion and group membership. It never creates a `WyrdState`/Run/Observe submission. Actual `Observe::submit` calls `record_or_stage`, resolves the bound spec, projects inputs, and either records Window features through `drift_value` or queues Run evidence through `enqueue_run` → `Target::enqueue`; the latter adds normal target correlation and its idempotency key before reaching that HTTP endpoint. These are concrete supported paths, including the final client's changed borrowed spec lookup. Raw endpoint equivalence therefore does not execute all the specifically planned authoring path.

**Sibling proof and materiality:** `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs::grouped_conversations_coexist_with_drift` already uses Run/Observe Window submission and grouped queued Run submission; other existing stories cover projection and refusal behavior. They are valuable retained evidence, but do not combine those APIs with V18's held-input assertion. The candidate could fail before reaching the correct server writer and V18 would still pass. This is a missing explicitly required regression proof, not evidence that the current SDK fails, and not a reason for a new cross-language ACK test matrix.

**Smallest correction and burden:** Extend the existing V18 journey using existing public hydrated-state/Run/Observe fixture patterns to submit its grouped Run source and scheduled Window observations, retaining its paused-input/direct-return/queue-absence/fixed-readback assertions. Use the ordinary returned execution handle. Keep harness controls only for deterministic publication and schedule timing. Reuse current card fixtures, state setup patterns and ingest barrier; no new SDK method, transport, raw-SQL story, scheduler, or test framework. Public direct verification in that same setup can reuse Observe verify. This replaces bypasses in one existing journey rather than adding parallel coverage machinery.

**Focused closure:** Run the confirmed exact V18 nextest selector in its existing `wyrd-testing --test server -P journey --run-ignored=all` lane. Inspect that Run submission, Window recording, paused-ACK assertions and fixed grouped readback occur in the same journey. Retain already-covered V11 language stories unless changed dependencies or new failure evidence justify a particular rerun. Original final V13 covers integrated surfaces once.

### FIND-TASK-009-3 — Normative owner sections retain the retired ordering

**Source:** DP-001. **Status:** CONFIRMED. **Classification:** INCORRECT.

**Obligation:** REQ-038 and the task's explicit architecture-update requirement retire superseded early admission. Root authority rules and `architecture/README.md` require one normative architecture and correction in the owning document.

**Locations:** `architecture/wyrd-design.md:1638`–1659 and `architecture/bifrost-design.md:1827`–1830. Revised ACK-first clauses already exist at Wyrd `:1751` and Bifrost `:387`.

**Full-context/source evidence:** The Wyrd group section says the selection transaction enqueues the ordinary execution and deletes scheduling state, with the selector staged immediately after commit; its concurrent-activity paragraph describes row-lock/recreation as the handoff mechanism. Bifrost's permission/capture paragraph explicitly says group/schedule selector publication follows the PostgreSQL commit and early runner claims retry missing evidence. These clauses describe runnable candidates, not solely the intentionally preserved terminal-refusal or unchanged-content outcomes. They contradict the new paragraphs and approved no-pending-row-before-ACK behavior.

Actual producer and consumer owners implement ACK-first: scheduled candidates drop their TenantConn before selector publication; `hand_off_groups` distinguishes settled existing/refused outcomes from new candidates, drops the candidate transaction, and stages the retained command. Scribe's complete `write` constructs private `Acknowledged` only for successfully submitted, non-lost input items. `SubmissionSink::write` accepts frozen commands on one tenant transaction, commits, then issues a notification. `accept_scheduled` conditionally moves the expected cursor; `accept_grouped` conditionally consumes the selected generation. No second runtime mechanism is needed.

**Consequence/materiality:** Maintainers following the normative group/storage contract are directed to recreate precisely the early runnable/consumed-state ordering this task removes. This is a central durability/admission contract contradiction and explicit acceptance failure, rather than optional prose polish.

**Smallest correction and burden:** Replace the stale ordering clauses in these existing paragraphs with preparation and transaction release → own-input publication/ACK → shared tenant admission plus conditional source transition. State selected-generation preservation of newer group activity. Preserve existing cutoff/order/null/source-retention rules, existing unchanged-content and terminal-refusal transactions, infrastructure-retry behavior, and defensive missing-evidence handling. Correct prose only; do not alter SQL, historic migrations, runner retries or public protocol to match the stale text. A bounded owner-text edit has no new architecture decision.

**Focused closure:** Re-read the complete affected paragraphs against the existing scheduler/Scribe/admission/SQL path and check that remaining normative group/schedule publication wording never prescribes queue acceptance before own-input ACK. Static text/diff inspection suffices; no runtime test or new documentation checker is warranted.

### FIND-TASK-009-4 — New lifecycle destructor omits mandatory rustdoc

**Source:** REPO-001. **Status:** CONFIRMED. **Classification:** VIOLATION.

**Obligation:** Root Code rules explicitly require rustdoc on every new/changed Rust item and state that missing rustdoc blocks merge. `agent_docs/rust.md` extends this to private helpers and requires workflow intent, invariants and side effects.

**Location:** `crates/wyrd/wyrd-server/src/verification/scheduler.rs:148`, `impl Drop for InFlight::drop`.

**Evidence/callers:** The full new method has no rustdoc and removes `self.source` from the shared mutex-protected set, recovering poison. `InFlightSet::claim` creates the Arc-held claim; `claimed` supplies scheduler exclusions. Both schedule and group candidates retain it in `Queued` across Scribe and admission. Final Arc drop releases the suppression after acceptance/loss/cancellation. Type-level `InFlight` documentation describes that lifecycle, but does not document the separately added method under the explicit item rule. Nearby established lifecycle methods, including `PacedUpstream::drop` and gateway `ActiveCall::drop`, carry method rustdoc.

**Consequence/materiality:** This item fails a mandatory merge gate at the operation that releases source preparation eligibility. No runtime-loss claim, extra panic claim, or speculative refactor is needed to establish that violation.

**Smallest correction and burden:** Add concise method rustdoc describing final-owner source-claim removal and poison recovery. Preserve the existing guard and behavior; deleting them would lose required bounded duplicate suppression. No helper, lifecycle redesign, test or lint exception is needed. Batch this small documentation fix with the substantive remediation.

**Focused closure:** Inspect the method's rustdoc for the workflow/side-effect invariant and the bounded changed-item documentation set. No runtime test is justified by this correction alone.

## Acceptance and standards coverage reconciliation

The selected reports cover all 35 changed paths, each of the 15 commits, all shipped changed tiers and their nested rules. Their coverage is retained as first-round discovery evidence, with the following independently checked seams and limits; an empty specialist ledger is not treated as automatic proof.

| Obligation / surface | Coverage retained and independently challenged | Reconciled result |
|---|---|---|
| REQ-035 / AC-015; own-input gate, concrete producers, direct path | Behavior + persistence + system; checked Scribe success filtering, typed ACK-only writer and full V18; no extra queue/public receipt | Implementation supported; V18 authoring proof gap is FIND-2 |
| REQ-036 / AC-016; atomic schedule/group acceptance, frozen state, tenant isolation | Persistence and behavior trace migration, RLS, conflict/replay, cursor/generation tests; independently read both complete accept methods and shared writer, background candidate/settled split | No additional finding; no source-backed defect in selected coverage |
| AC-017; bounded preparation, mixed-origin commit, retry and distributed polling | System + persistence + maintainer; one existing outbox, one tenant writer, local claim lifetime; source-backed retry/notification analysis and V2–V6/V17 tests | No additional finding; static proof plus earlier executed evidence, not fresh database execution |
| REQ-037 / AC-018; existing Bifrost runner, engine, stale fence, payload owners, direct IO/linger | Behavior/system/maintainer cover runner/results/readers and V7–V11; independently checked final spec borrowing consumers and upstream branch, shared outbox single-clock decision/regression | No further finding; exact outbox run is one pass supplied by root |
| REQ-038; architecture, retirement, unchanged exact work/query counts | Behavior complete diff/count protection; domain owner-text inspection; standards diagnostics/generated-artifact coverage | FIND-3; no change to exact 4-attempt/14-query obligations |
| V12/V13; final integrated gate | Actual earlier gate provenance independently read; later executable patches and consumers inspected | FIND-1; earlier green is real but does not identify final source |
| Standards across Rust/SQL/tests/Python/TS/generated docs | Standards table maps every changed path and authority; nested rules and Rustdoc gate read, candidate destructor independently inspected | FIND-4; no additional material rule finding retained |
| Ownership / complexity | Maintainer report covers all material owners and existing alternatives; source confirms extension of Scribe/submission/outbox and reused SDK typed spec | Validated empty material ledger; no new framework/owner needed |
| Failure propagation / recovery | System report covers failed publication, SQL retry, cancellation, restart, lost notification and stale lease; independently checked claims, ACK/write commits and outbox clock | Validated empty material ledger within this scope; accepted memory-only handoff and retention limits remain |
| REQ-039 / AC-019; measurements/history/PR | Existing five accepted records identify `19f4190a8`; candidate later changes executable client/upstream; caller reports PR unopened | Original downstream completion remains pending; do not fabricate later-SHA attribution or open PR |
| Non-goals / INV-001–003,006–008 | No new durable prequeue store, PG input loader, stronger delivery, count weakening, privacy exposure, benchmark gate, L2, merge or release in retained scope | No additional finding |

## Closeout recommendation and limits

The user's acceptance of all five older L1 results as valid overrides their older driver-budget validity labels. No driver saturation or validity finding is retained. That acceptance does not authorize replacing `19f4190a8` in those records with a later SHA. The final executable candidate includes the two later optimizations; final-source measurement coverage is therefore an outstanding original REQ-039/AC-019 obligation unless the user separately changes that obligation.

V13 explicitly puts correctness review before final V14 measurements and V16 PR delivery. An intermediate correctness review must not require an already opened PR or already rerun benchmarks as a prerequisite to conducting that review. Equally, it must not mark the entire TASK-009 complete while those original obligations remain. Resolve the bounded findings, obtain original final V13 evidence for the settled source, complete required immutable correctness reviews, then follow original V14–V16 for final-source measurements, separate history/report publication and the main-based code PR when authorized. Do not fold those operations into this static review or prescribe a new full-suite remediation checklist. No specification revision is required for the four retained corrections.

This validator ran no Cargo, database integration, benchmark, full gate, publication or Git mutation. Evidence consists of independently read source, authority, Git chronology, the available older gate log, and the root's exact one-test passing outbox run. Earlier logs establish only their recorded sources. No source was edited; this report is the sole write.

Skipped live execution and benchmarks as assigned. Risk: final-source integrated proof and final measurement/delivery closeout remain incomplete; approved memory-only prequeue loss and source-retention limitations remain unchanged.
