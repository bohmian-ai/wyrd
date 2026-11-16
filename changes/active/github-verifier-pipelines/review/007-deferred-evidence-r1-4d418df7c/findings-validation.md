# Independent findings validation — TASK-007

Recommendation: **FIX_REQUIRED**. Five deduplicated material findings remain. This is independent source validation, not a vote on reviewer outcomes. No new blocker was discovered by the validator.

## Immutable subject, scope and inputs

Repository `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-007`; base `16ea9694c34a5ec1867306bd5e34df62f0888611`; candidate `4d418df7c23ee03a976f89349b74debdddee72c6`. HEAD was checked and remains the candidate. This is first-round full-range review, with no prior acceptance/finding closure to carry forward. The common map, selection rationale and execution evidence are in `scope.md` and `verification.md`.

Read all seven discovery reports, claim comparison and focused follow-up; original task, approved revision-7 specification, publication contracts, relevant supplements, applicable root/SQL and SDK/docs rules, maintainer guide and architecture publication/custody/security provisions. CodeGraph preceded source navigation. Independently read complete publication orchestration, subscriber validation, custody acceptance and canonical read, Artifact metadata/admission, attribution, changed SQL projection methods and relevant consumer bodies/config routing. The actual Git range establishes changed ownership; reports alone do not establish a finding.

Approved attempt refund on park, the one CLI `--file` flag and its stated limitation, and ordinary restart/retry/refusal AC-007 with no new crash hooks remain settled. Neither review nor remediation reopens them.

## Specialist applicability and coverage validation

| Role | Why required / actual coverage | Validated assessment |
|---|---|---|
| behavior-rev | Full task, all surfaces, original obligations and non-goals; explicit implementation/assertion matrix | Complete. Its two proposals are independently traced below. |
| invariant-rev | Engine/RunState ownership, branches, accepted work/budget, park ACK/claim/deadline, exact pins and canonical reads | Required and complete. Scoped source PASS is credible; ignored-test routing qualifies ongoing proof, not these source traces. |
| repo-rev | 92-file cross-tier Rust/Python/TS/SQL/docs/tooling and generated declaration authority | Required and complete. Its SQL rule finding is validated. Its provisional statement about dedicated ignore conventions is superseded by actual routing evidence below. |
| maintainer-rev | New custody/client handles and changed loader/state/orchestration; entire existing-owner comparison | Required and complete. No material DRIFT established. Metadata admission omission is behavioral reuse failure, not evidence that standalone custody itself duplicates an owner. |
| system-rev | Publication durability, park/resource/lease lifecycle, lost wakes, restart and dependency failure | Required and complete. Correctly keeps component errors within request/claim boundaries and preserves separate direct capacity; test-routing qualification applies to its inspected assertion evidence. |
| domain security | Publisher/subject rights, selected repository, tenant isolation, completed evidence and delegated actor; accepted continuation authority | Required and complete. Both proposals independently traced below. |
| domain persistent data/concurrency | Batch identity/fence, acceptance/read visibility, immutable content, concurrent replay receipt and queue coordination | Required and complete. Its Artifact proposal consolidates with behavior/security; receipt proposal is narrowed below. |
| followup-rev | Specific conflicting claim about four ignored lifecycle tests and owning lanes | Appropriate bounded follow-up; RESOLVED against exact commands and registration. No need for another broad discovery pass. |

No required specialist is missing. Concurrency and persistent custody form one coherent selected data boundary, while lifecycle is independently covered by invariant/system. No separate UI/model-protocol reviewer is required: no UI or model-wire implementation changes; changed Agent budget restoration is covered by engine invariant proof, and existing GitHub access is covered by security. Reports include changed tests/helpers and all surface rules. Candidate authority amendments were explicitly required task outputs, rather than an unapproved attempt to excuse implementation drift.

## Claim decisions

| Discovery IDs | Decision | Final ID |
|---|---|---|
| BEH-001, SEC-007-1, DATA-007-1 | CONFIRMED; one invalid-state producer and correction boundary | FIND-007-1 |
| SEC-007-2 | CONFIRMED | FIND-007-2 |
| DATA-007-2 | REVISED: retain canonical receipt reconciliation, with an explicit one-subscriber losing-race proof; do not require a new global atomic-publication or exact per-request ownership protocol | FIND-007-3 |
| BEH-002, FOLLOW-007-01 | CONFIRMED; source routing resolves the standards report's provisional statement | FIND-007-4 |
| STD-007-01 | CONFIRMED as an explicit mandatory repository-rule violation, not a current swapped-identity exploit | FIND-007-5 |

No material proposal is rejected outright. The documentation link observation is not a separate retained finding or a new acceptance requirement; any small correction may be batched with otherwise-required work. No optional improvements are prescribed.

## Final deduplicated finding ledger

### FIND-007-1 — CONFIRMED — INCORRECT: Artifact admission occurs after immutable evidence acceptance

Discovery sources: BEH-001, SEC-007-1, DATA-007-1.

**Obligation and exact changed location.** Original task Selected custody requires completed verified File/Artifact handling and subscriber type/format/schema validation before accepting. Contracts Deferred input publication and REQ-009 preserve that boundary. `crates/wyrd/wyrd-server/src/components/verification/service/evidence.rs:98` checks completed storage only for File; `:261` validates subscribers only with pure `conform`.

**Independent source proof and consequence.** The public request explicitly accepts Artifact at `wyrd-spec/src/verification/execution.rs:184`; shared client `Evidence::declare` forwards ArtifactRef without resolving it. For a normal deferred CSV media slot, a registered Artifact holding a PDF, or a missing selected Artifact path, passes the Media/Artifact arm of `vala-verify/src/blocks/engine.rs:2856`. That arm intentionally delegates metadata checks to its host. Publication skips its File-only inventory check, stages source/input/Verifier custody and returns durable success. `DeferredEvidence::accept` writes the deterministic acceptance key, and `conflicts` subsequently refuses different declaration content. Runner `deferred:803–865` supplies that accepted declaration to the original execution; engine `run:740` finally invokes `TenantEvidence::admit`, whose Artifact branch at `hosted/evidence.rs:538–562` resolves metadata and rejects the format/path/size. `runner::engine_failure:1710` terminates invalid input as errored. Correcting the declaration now conflicts with the durable acceptance. The producer receives an irreversible success for evidence that was never eligible for that slot.

**Sibling authority and existing owner.** HTTP and MCP share VerificationControl; all SDKs use the shared client; ordinary direct/queued evidence admission shares TenantEvidence. Read full `artifact_file`, `inventory` and `admit` bodies: they already enforce Artifact kind, authorized exact registry read, UID/path, authoritative format/size and limits without reading bytes. `inventory` uses the caller's audited Card-read authority. Existing consumer admission runs under the execution's accepted authority and must remain there. Publication's own caller must also be checked before retaining its reference; publisher and consumer authority are not interchangeable.

**Materiality and correction burden.** This is an explicitly supported media form and first-write integrity boundary, not merely reachability of malformed input. The consequence is poisoned evidence and failed PR review. Deleting Artifact support violates the approved surface, and relying on downstream rejection cannot unfix the accepted slot. A bounded reuse of existing metadata/slot admission is sufficient; no new storage, validator, queue or public decision is necessary.

**Decision-complete correction.** At publication, before staging any subscriber acceptance, resolve and validate the Artifact file under the publishing Caller through the existing TenantEvidence metadata/admission owner, against every authorized subscriber's slot. Extend that owner narrowly only as necessary to expose its existing checks; do not fork registry/path/format validation or fetch bytes solely for metadata. Preserve existing JSON/File validation, per-execution combined input accounting, later execution-authority checks, original custody/conflict and queue behavior. The producer owns this guard because it is the trust/immutability boundary; adding more consumer guards is insufficient.

**Focused closure proof.** In existing deferred-evidence journey setup, a real wrong-format Artifact and invalid path must refuse before acceptance; a corrected valid Artifact at the same source/input must then accept and be consumed. Cover oversize and publisher Card-read refusal using existing authoritative admission cases, and ensure the ordinary valid Artifact path works. No crash hook or new harness.

### FIND-007-2 — CONFIRMED — VIOLATION: delegated publisher attribution loses the actor

Discovery source: SEC-007-2.

**Obligation and location.** Bifrost design Table and row identity (`architecture/bifrost-design.md:206–217`) requires accepted subject and actual actor: a delegated publisher records the party it acts for as subject and itself as actor. `components/verification/service/evidence.rs:133` instead constructs `Attribution::new(caller.principal.id, caller.principal.id)`.

**Independent proof.** Read full `Caller::from_authenticated`, `Caller::attribution` and `delegated_attribution`: authenticated Caller retains the verified initiator-first chain; `attribution` selects its last actor and applies an executed-by override when present. Publication's fabricated pair bypasses that owner. `DeferredEvidence::accept` forwards the pair unchanged to `ScribeWrite::Evidence`; `scribe_outbox.rs:539–546` carries it to Rows attribution, which Scribe stamps. A valid service acting for a different subject therefore persists subject/subject rather than subject/actor. Authorization remains bound to the subject, but canonical evidence provenance is false. Correct permission audits do not correct the evidence row.

**Materiality and burden.** Standard delegated tokens are supported input, and row provenance is an explicit architecture boundary. This is a one-call correction, not a new token or persistence protocol. Removing attribution or reconstructing actor selection locally is insufficient; existing enqueue/storage siblings already use Caller attribution.

**Correction and proof.** Use `caller.attribution()` for publication custody, preserving all permission checks and the execution's later accepted authority. Through existing delegated-auth fixtures, publish evidence and inspect its authorized canonical Bifrost row: subject is the effective principal, actor the verified outer actor; nondelegated remains principal/principal. No new actor field or audit behavior.

### FIND-007-3 — REVISED — INCORRECT: raced replay returns a noncanonical receipt

Discovery source: DATA-007-2.

**Obligation and location.** Task REQ-009 requires identical accepted content to replay its receipt. `PublishEvidenceResponse.recorded_at` at `wyrd-spec/src/verification/execution.rs:227` is when evidence was first accepted, and architecture Verifier says identical replay reports replayed. The error-reconciliation branch at `components/verification/service/evidence.rs:143–151` reads canonical raced acceptances but discards them; response at `:157–176` uses the original empty `held`, stale `fresh` and local `now`.

**Independent reachable proof.** Two authorized identical publishes can both finish `accepted` while the fence is absent. With one subscriber, both have `held={}` and one fresh subscriber, but compute different microsecond timestamps. Read full custody accept, row/key, Scribe frames/delivery/tally and existing memtable retained-identity comparison: the source/input/subscriber batch key is identical while row `recorded_at` differs; changed data digest rejects the losing physical batch. Only acknowledged frames notify acceptance. The loser reaches the explicit fallback, reads the winner's canonical `Accepted` row and verifies identical digest, yet discards its recorded time. It then returns status accepted with its own timestamp, even though that row did not win. A sequential retry returns the actual earlier timestamp and replayed status. Evidence bytes remain correctly deduplicated; no data-loss allegation is retained.

**Materiality and narrowing.** This violates the explicit durable-receipt/retry contract on a realistic overlapping unknown-outcome retry. It does not justify all-or-nothing publication across subscribers or a new globally ordered request-ownership protocol. The one-subscriber losing-race scenario alone proves a meaningful public-contract defect; this revision avoids expanding concurrency guarantees.

**Existing owner, smallest correction and burden.** Existing `DeferredEvidence::accepted` already returns the authoritative content and timestamp; it is the proper reconciliation source. Use the canonical raced acceptances when constructing the successful recovered receipt rather than its rejected local offer, and report replay for the demonstrated case where every fresh acceptance was won by the concurrent identical publication. Preserve ACK-before-success, conflict refusal, partial acceptance recovery and wake-on-replay. Keep any necessary outcome accounting inside the existing custody/publication boundary; do not introduce SQL locking, a second idempotency store, cross-subscriber transactions or new API semantics. A local response reconciliation is sufficient; removing the safe-race fallback merely converts a required retry into an avoidable error.

**Focused closure proof.** Overlap two identical offers for one existing subscriber using current request/Scribe test facilities, with different offer timestamps. Assert the canonical winner's accepted receipt, loser's replayed receipt and later sequential replay share evidence identity and first-accepted time, and only one canonical acceptance exists. No new production fault hook is required. Preserve existing mixed-subscriber behavior rather than broadening this test into an atomic-publication guarantee.

### FIND-007-4 — CONFIRMED — VIOLATION: four required lifecycle proofs have no executing normal lane

Discovery sources: BEH-002, FOLLOW-007-01.

**Obligation and location.** Root AGENTS Testing prohibits circumventing gates with `#[ignore]`; task AC-006/007 requires the existing running/restart/resource/deadline proof. New ignores at `crates/wyrd/wyrd-server/tests/integration/pg_deferred_evidence.rs:499,549,598,674` exclude exactly those four tests.

**Independent routing proof.** Module registration is only `wyrd-server` integration. `test:wyrd` already provisions repository Postgres and calls `scripts/run-family-tests.sh:44`, which invokes nextest without run-ignored. Bifrost's ignored-test aggregate at mise:323–340 omits wyrd-server. Its server integration route at mise:380–383 excludes this module and does not enable ignores. Journey-server selects `wyrd-testing --test server`, not this target. The remaining ignored server routes select unrelated live Operator/identity tests; nextest profiles configure resource/concurrency/timeouts rather than selecting ignored tests. The focused follow-up traced CI/aggregate composition and found no hidden executing owner. The same module's other Postgres tests are nonignored. One-off implementation-time explicit runs do not establish an ongoing lane.

**Materiality and correction burden.** Normal aggregates can pass while the four explicit recovery/resource proofs silently never run. This is a current gate-routing failure, independent of whether source assertions are convincing or past manual runs passed. Removal of four attributes lets the existing correctly provisioned family lane own the tests; no new tests, suite or harness is necessary.

**Correction and proof.** Remove these four new ignores and preserve their ordinary restart/retry/resource/deadline assertions and approved semantics. Show existing family selection includes them as runnable; execute the four exact existing tests under repository Postgres. No broad remediation suite or new crash hook. This resolves the provisional standards statement about dedicated journey conventions; it does not reopen lifecycle implementation already source-reviewed.

### FIND-007-5 — CONFIRMED — VIOLATION: new SQL decoding is positional despite mandatory name mapping

Discovery source: STD-007-01.

**Obligation and location.** `crates/wyrd/wyrd-sql/AGENTS.md`, Queries, says “Map columns by name, never by position”; `agent_docs/rust.md` repeats it. New `evidence_subscribers` at `verifier_executions/pull_requests.rs:532–540` decodes `Vec<(Uuid,Uuid)>`; `wake_evidence_consumers:570–578` decodes `Vec<(Uuid,)>`.

**Independent proof and scope.** Read both full methods, their query constants and service callers. SQL selects `verifier_card_uid, card_uid`, then tuple FromRow ordinal slots become verifier and subject for target resolution/authorization. The present projection order is correct; no present swapped-ID exploit is alleged. Tuple decoding demonstrably never uses names. The scalar wake result repeats the forbidden mapping. Existing `GenerationRow` in the same module uses a named FromRow projection, so the owner already supplies the conventional alternative.

**Materiality and correction burden.** Retained under the explicit mandatory-rule branch, not hypothetical runtime hardening: this source currently violates the narrow SQL owner's mapping rule. The two equal UUID types make ordinal coupling particularly unsafe to maintain, but a future edit is not claimed as an existing failing input. Name-based decoding at the current SQL owner is a small bounded correction and requires no new workflow, API, migration or generic type layer.

**Correction and proof.** Decode the subscriber and returned execution identities by their SQL column names using the existing named projection/row-access mechanism; preserve method contracts, transaction ownership, RLS, exact identities and wake SQL semantics. Do not refactor unrelated preexisting queries. Source inspection proves use of names; focused existing subscriber/wake SQL or publication integration proof confirms unchanged observable identities. A test that merely repeats the current SELECT order cannot prove this structural rule; no invented lint framework is needed.

## Reconciled acceptance evidence

| Obligation | Independent reconciliation | Result |
|---|---|---|
| REQ-008 typed deferred slots and derived queue-only/direct baseline | Behavior matrix references decoder/registration assertions; fresh typed-slot test passed. Existing dependency walker/engine owner retained | PASS |
| REQ-008 independent branch progress and no settled replay | Invariant and behavior both cite exact first/resumed call lists; fresh dependency engine test passed | PASS |
| REQ-009 pre-execution retained subscription, source/policy/tenant matching, unmatched/no historical replay, no publication-started run | Source and main journey assertions cover exact stable ID/pins, zero executions and later subscriber wait; metadata admission is the isolated failing boundary | PASS except FIND-007-1 |
| REQ-009 completed media and authorized Artifact acceptance | Complete Artifact trace establishes missing preaccept admission; File/JSON baseline remains covered | FAIL FIND-007-1 |
| REQ-009 durable ACK, conflict and retry | ACK/fence/unavailability traces credible; sequential replay asserts original timestamp; concurrent fallback receipt is wrong | FAIL FIND-007-3 |
| REQ-011 source lifecycle, state owner, ACK-before-delivery retirement, compute release, same ID/deadline/refund and accepted budget | Invariant/system source paths credible, exact accepted budget test passed, no semantics changes required | PASS implementation; ongoing proof FAIL FIND-007-4 |
| AC-007 accepted evidence survives ordinary restart, unknown outcomes retry, running input eventually consumed | Existing assertions match revised outcome; no demand to kill full server or add crash hooks. Routing currently skips four required tests | FAIL FIND-007-4 |
| INV-004 canonical unavailable never empty/fresh | Fenced-but-invisible/failed read branches defer; approved source inspection plus publish refusal journey remains credible | PASS |
| INV-005/006 exact policy/authority, original expiration, supersession/revocation/no terminal reopening | Source trace and inspected cancelled/timed-out assertions accepted; approved attempt semantics preserved | PASS |
| REQ-016/AC-011 surfaces, inference/override, waiting inspection, schemas/docs/example | Behavior/standards/maintainer coverage spans all three SDKs, CLI, HTTP/MCP, loader and generated roots; no missing required specialist | PASS baseline, publication defects qualify all shared callers |
| Provenance and SQL mandatory rules | Narrow architecture/SQL owner rules prove current violation independent of green aggregate checks | FAIL FIND-007-2, FIND-007-5 |
| Non-goals / ownership | Standalone accepted evidence cannot fit execution_inputs without fake identity; Scribe/storage/RunState/queue remain owners. No broker, payload SQL mirror/new table, Trigger, generic wait framework or new permission category | PASS |

The scoped PASS reports do not negate unique boundary findings. Reports are complete, and no missing review/source blocks the review. There is no new specification decision: all five corrections restore already-approved behavior/rules at existing owners. No prior remediation rounds or convergence assessment apply.

## Verification limits

Validator executed only source/config navigation and Git identity checks. Root observed whitespace, dependency and tenant-isolation checks and three exact pure tests passing; details and commands are in `verification.md`. These do not prove Artifact publication, delegated evidence attribution, overlapping acceptance or actual restart. Docker denial prevents fresh Postgres/SDK journey execution; implementation-recorded green runs are not fresh review results. Source-backed findings remain sufficient for FIX_REQUIRED. No source/generated artifacts/Git settings/refs were changed; only this validation report was written.
