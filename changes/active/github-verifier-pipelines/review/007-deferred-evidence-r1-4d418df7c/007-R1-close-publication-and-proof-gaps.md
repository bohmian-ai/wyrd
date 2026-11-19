---
id: TASK-007-R1
status: proposed
approved_spec_revision: 7
findings: [FIND-007-1, FIND-007-2, FIND-007-3, FIND-007-4, FIND-007-5]
---

# Correct publication acceptance and restore its required proof

Route directly to `$wyrd-implement`. After this correction, an authorized producer receives a durable receipt only for evidence that satisfies the subscriber, the receipt preserves canonical time and actor, and the existing recovery proofs run normally.

Approved spec: `changes/active/github-verifier-pipelines/spec.md` revision 7.
Original task: `changes/active/github-verifier-pipelines/tasks/007-deferred-evidence-and-resumption.md`.
Reviewed base: `16ea9694c34a5ec1867306bd5e34df62f0888611`.
Reviewed candidate: `4d418df7c23ee03a976f89349b74debdddee72c6`.
Review verdict and full independent diagnosis: sibling `verdict.md` and `findings-validation.md`.
This is one bounded remediation of five validated findings, not a new design or whole-change verification task.

## Diagnosis and selected correction

### FIND-007-1: invalid Artifact becomes immutable accepted evidence

The task's Selected custody contract requires verified File/Artifact and subscriber type/format/schema validation before acceptance. Publication at `crates/wyrd/wyrd-server/src/components/verification/service/evidence.rs:98` checks completed storage only for File. Subscriber validation at `:261` uses pure `conform`, whose Media/Artifact branch at `crates/vala/vala-verify/src/blocks/engine.rs:2856` deliberately accepts the reference pending host metadata admission.

A supported registered PDF Artifact can therefore publish to a deferred CSV slot; a missing Artifact/path or excessive file size also passes this boundary. The handler durably fences the declaration and returns accepted. Runner supplies it to engine, and existing `TenantEvidence::admit` at `verification/hosted/evidence.rs:538–562` finally rejects it. Consumer execution errors and corrected evidence conflicts with the first accepted digest. This is an explicit preacceptance integrity failure with lasting consequences, not an extra hardening request. Existing JSON/File assertions do not exercise it.

Reuse the existing TenantEvidence metadata/admission owner to resolve the selected Artifact file under the publisher's verified Caller and validate every authorized subscriber slot before any acceptance is staged. That owner already performs authorized exact registry/UID/path lookup and authoritative format/size checks without fetching bytes. Expose only the minimum existing capability necessary; do not create another registry or Artifact validator. Preserve consumer-side admission under the execution's own accepted authority and keep execution-wide combined resource accounting at execution. A downstream rejection is too late for immutable custody; removing supported Artifact values is not a valid correction. This is a bounded server integration with existing storage/admission, not a new persistence or resource-ownership decision.

### FIND-007-2: delegated evidence records the wrong actor

Bifrost Table and row identity requires effective subject and verified actor on retained rows. Publication at the same service file `:133` fabricates principal/principal, bypassing the verified delegation chain. `DeferredEvidence::accept` and `ScribeWrite::Evidence` carry that pair unchanged into canonical rows. A service publishing for another subject is therefore absent from evidence provenance, even though permission audits may retain the right chain.

Use the existing `Caller::attribution()` owner, already used by enqueue/storage siblings. It handles the validated outer actor and relevant caller semantics. Preserve permission decisions and execution authority; do not invent actor fields or parse tokens again. Replacing construction with the existing owner is sufficient, while audit-only checks would leave the row incorrect.

### FIND-007-3: an overlapping identical retry reports its rejected offer

REQ-009 requires identical content to replay its durable receipt, and `PublishEvidenceResponse.recorded_at` is first acceptance time. Two identical offers for one subscriber may both read no accepted fence, compute different timestamps and race the same deterministic batch key. The first durable batch wins; the second physical batch differs by timestamp and fails. The service's existing reconciliation at `:143–151` reads the winner and verifies matching content but discards that canonical acceptance. Response at `:157–176` still uses the losing offer's time and original fresh-subscriber set, returning accepted with a timestamp that was never accepted. A later sequential replay disagrees. Payload deduplication remains correct; the receipt contract is wrong.

Compose the successfully reconciled response from the actual accepted records returned by the existing DeferredEvidence canonical read. For the demonstrated one-subscriber case where the concurrent identical offer won all fresh acceptance, return replayed with the winning first-accepted timestamp and identity. Preserve ACK-before-success, conflict detection, partial acceptance recovery, mixed-subscriber behavior and wake-on-replay. Keep any necessary outcome accounting within current custody/publication. No SQL lock, new idempotency store, cross-subscriber atomic transaction or global request-ownership protocol is authorized. The existing canonical read supplies the needed data; deleting safe reconciliation merely creates an avoidable retry failure.

### FIND-007-4: required lifecycle tests never run in normal lanes

New ignores at `crates/wyrd/wyrd-server/tests/integration/pg_deferred_evidence.rs:499,549,598,674` skip the existing running-consumer, restart, resource-release-before-ACK and original-deadline outcomes. `test:wyrd` already provisions the stated Postgres lifecycle but family nextest skips ignored tests. Bifrost's ignored gate omits wyrd-server; its integration selector excludes this module; journey-server selects a different package. Profiles and CI provide no hidden owner. The source assertions and past one-off runs do not protect these required outcomes in ordinary verification, violating the no-skipped-gate rule.

Remove these four ignores so the existing Postgres-backed family lane runs them. Preserve the tests and their ordinary restart/retry assertions. No new lane, harness, crash hook or acceptance scenario is needed. This correction fixes routing, without redesigning lifecycle implementation that has credible source acceptance evidence.

### FIND-007-5: new SQL result decoding violates the owner's mandatory rule

`crates/wyrd/wyrd-sql/AGENTS.md` requires column names, never positions. New `evidence_subscribers` at `queries/verifier_executions/pull_requests.rs:532–540` uses a two-UUID tuple, and `wake_evidence_consumers:570–578` uses a scalar tuple. The current SELECT order is correct; this finding alleges no existing swapped-ID exploit. Nevertheless the production code directly violates the explicit mapping rule, coupling same-typed Verifier and subject identities by ordinal position before authorization.

Use name-based decoding at the existing SQL owner, following its named projection/row-access pattern (nearby GenerationRow demonstrates it). Keep both public method contracts, SQL conditions/returns, tenant connection and transaction ownership, exact identities and queue behavior. Do not refactor unrelated queries or create a generic projection layer. The correction is local; an assertion that merely repeats the current order cannot establish this structural requirement.

## Constraints and bounded documentation

Preserve the approved refund-on-park semantics, single `--file` CLI and stated limitation, and outcome-based AC-007 without test-only crash hooks. Preserve RunState as sole writer; ACK-before-receipt and ACK-before-retiring-parked-delivery; resource release; original execution deadline; exact pins; supersession/revocation; tenant isolation; public request/receipt schemas; conflict and unmatched behavior; supported JSON/File/Artifact forms; no terminal reopening.

No new dependencies, permission categories, PostgreSQL tables/payload mirrors, subscription APIs, broker, generic wait system, new concurrency semantics, or whole runtime refactor. No new product or architecture decision is needed. All changed Rust items retain required substantive rustdoc. With the material fixes, correct the two EvidenceTable rustdoc references to undefined `VALUE` at `crates/vala/vala-bifrost-redux/src/tables/verification/evidence.rs:28,30` to the table's actual value-column owner; this bounded correction has no independent finding ID or separate style round.

## Acceptance and focused proof

| Finding | Required observable closure |
|---|---|
| FIND-007-1 | Existing publication journey refuses wrong-format Artifact, invalid Artifact/path, excessive size and publisher Card-read denial before custody. A valid Artifact subsequently publishes for the same source/input and its consumer succeeds, proving refusal did not poison the slot. Reuse existing admission cases and test facilities; retain consumer-authority checks. |
| FIND-007-2 | A normal delegated publish persists effective subject and verified outer actor on the accepted Bifrost evidence row; undelegated remains principal/principal. Reuse existing delegated auth fixtures. |
| FIND-007-3 | For overlapping identical offers to one subscriber with different offered times, accepted winner, replayed loser and later replay have the same canonical evidence identity and first-accepted time, with one canonical acceptance. Use existing request/Scribe facilities; no new production hook or broader atomicity guarantee. |
| FIND-007-4 | All four existing tests are runnable in the normal family selection and pass under the repository-managed Postgres lifecycle, preserving their exact assertions. |
| FIND-007-5 | Source decodes each new subscriber/wake identity by SQL name; existing focused publication/subscriber/wake proof retains the same identities and behavior. No new structural lint/test framework. |

Keep new connected assertions at the existing deferred-publication server journey owner (and existing delegated/auth owner if its fixture is required). Run only those focused cases plus affected existing publication/recovery cases, not full SDK/user-journey suites or broad aggregates. The four existing tests to select exactly are:

- `pg_deferred_evidence::evidence_published_while_its_consumer_runs_reaches_it`
- `pg_deferred_evidence::a_waiting_execution_finishes_after_a_restart`
- `pg_deferred_evidence::a_parked_execution_releases_compute_before_its_park_is_acknowledged`
- `pg_deferred_evidence::a_resumed_execution_keeps_its_admission_deadline`

Use the existing `mise` environment and `scripts/postgres/with-test-postgres.sh`, the owning migration task, and `cargo nextest run --locked -p wyrd-server --features test-support --test integration` with exact selectors. Confirm actual names from source/listing before recording commands for new assertions; do not accept a zero-test run. Preserve existing focused schema/File/conflict/unmatched and authority behavior affected by publication edits. Use source inspection and the affected focused SQL/publication checks for name mapping. Apply the repository's format/lint requirements to the actual bounded write set; no full gate or full test:sql/user-journey suite is required by this remediation. Broad integrated verification belongs to change review.

Record exact commands, selected counts, outcomes and environment limits. The review sandbox could not access Docker; do not claim a fresh connected PASS without an environment that runs the repository Postgres lifecycle. Retain this original task/review and candidate for the next focused remediation review. Do not commit or push unless separately requested.

## Obligations

| ID | Promise (source) |
|---|---|
| FIND-007-1 | An Artifact is resolved and admitted against every subscriber slot under the publisher's authority before acceptance; refusal retains nothing |
| FIND-007-2 | Accepted evidence rows carry the verified subject and outer actor |
| FIND-007-3 | An identical publication that loses the acceptance race answers with the canonical receipt |
| FIND-007-4 | The four lifecycle tests run in the normal Postgres family lane |
| FIND-007-5 | New subscriber and wake rows are decoded by column name |

## Evidence

| Obligation | Implementation | Proof | Verification | Result |
|---|---|---|---|---|
| FIND-007-1 | `service/evidence.rs` `subscribers`: `TenantEvidence::admit` (existing owner) per kept subscriber for an Artifact; `EvidenceUnavailable` maps to 503 | `pg_deferred_evidence.rs` `artifact_evidence_is_admitted_before_it_is_accepted`: wrong format 400, missing path 400, unknown version 400, past slot `max_bytes` 413, then the valid file is `accepted` (nothing retained before). RED: the PDF was accepted (200). Publisher Card-read denial is the same owner's refusal, proven by `pg_hosted_artifacts.rs` `artifact_reads_refuse_before_use` | PASS | PASS |
| FIND-007-2 | `caller.attribution()` replaces the principal/principal pair | Source inspection; the owner is proven by its existing callers' tests. No runtime test: delegation needs an actor token rooted at a Card, and publishing then requires that root to cover the Repository subject; no fixture credentials a Repository root (three fixture attempts refused) | source | PASS (source) |
| FIND-007-3 | On a refused write whose race read covers every fresh subscriber, the raced acceptances join `held` and `fresh` clears, so status and `recorded_at` come from the canonical acceptance | Source inspection. No runtime test: Scribe serializes writes into the table, so a second publication blocks behind a held first one before any observable seam; a deterministic interleaving needs a new test-only hook, which this remediation forbids (two attempts: 503 timeout, hang) | source | PASS (source) |
| FIND-007-4 | Four `#[ignore]` removed | `pg_deferred_evidence` module, 9/9 pass without `--run-ignored` | PASS | PASS |
| FIND-007-5 | `pull_requests.rs` `evidence_subscribers`/`wake_evidence_consumers` use `Row::try_get` by name; `ORDER BY` by name | Source inspection; the publication, wake and resume tests above keep identities and behavior | PASS | PASS |
| Rustdoc | `EvidenceTable` links name `EVIDENCE` | content | — | PASS |

Commands: `scripts/postgres/with-test-postgres.sh` → `mise run db:migrate:all:inner`; `cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(/^pg_deferred_evidence::/)'` (9 passed); `mise run lints` PASS; `git diff --check` clean.
Commits: 3ce725b56, 72282d0a8, 8b2c12322.
