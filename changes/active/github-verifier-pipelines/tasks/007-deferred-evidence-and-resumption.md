---
id: TASK-007
status: proposed
approved_spec_revision: 5
requirements: [REQ-008, REQ-009, REQ-011, REQ-016]
acceptance: [AC-005, AC-006, AC-007, AC-011]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007]
depends_on: [TASK-001, TASK-003, TASK-006, TASK-010]
---

# Accept deferred evidence and resume the same execution

Implement **approved-deferred-inputs.md → Approved publication matching and
subscription lifecycle / No new PostgreSQL tables** and
**approved-runtime-and-github.md → Durable waits and resumption**. Use
**contracts.md → Deferred input publication / Errors, inspection and delivery**.
This is ordinary typed input dependency, not a Trigger, wait action or broker.
It requires task 010's memory execution runtime (spec revision 7 REQ-017), which
supersedes any engine ACK-before-progress wording in the design supplements.

## Input and subscription source

Add `deferred: true` only to typed JSON/media slots; omitted/false and samples/
grouped inputs retain existing paths. Existing action/reference validation finds
consumers from ordinary input refs; no extra subscription authoring/depends_on.
Task 003's active selected binding establishes subscription to each named
input even without a running execution. Task 006 supplies accepted PR repository/
head/policy/authority for consumers. Publication alone never starts a run.

### Revision — registration-derived `queue_only`

When introducing deferred inputs, extend the server-owned registration derivation
introduced by [010-R1](010-R1-restore-direct-verification-baseline.md) to set
`queue_only=true` when a Verifier declares `deferred: true`. Preserve other
supported queue-only conditions; users cannot set or override the flag. Omitted
or false `deferred` does not itself make a Verifier queue-only. This replaces the
former instruction to add pre-step slot validation for direct eligibility.
Reuse the cold registry-load flag check before cache insertion; do not scan
inputs on each direct request or repeat eligibility work on cache hits. Verify
registration classification for true/false/omitted, refusal before any direct
step, and unchanged ordinary-input eligibility. Task 010-R1 implements publish
detection only; this task owns the deferred-input extension and its proof.

Publishers use `wyrd evidence publish ci_results --file results.json` or
`client.evidence.publish(name, value, repository=None, commit=None)` from any
environment: a developer machine, a script, or any CI system. Wyrd is
CI-agnostic: no client or server code detects, names or reads the variables of
a particular CI provider, and nothing requires a Git checkout or a `git`
binary. Raw HTTP is `POST /v1/verification/evidence/publish`, matching tenant +
repository + commit + input. The publisher does not supply a Repository policy
version, Verifier ID, execution ID, group_id or retry key; tags are limited to
commit. Internal consumer matching still uses accepted execution context.
Client-supplied source identity grants no permission.

### Revision — repository slug and CI-agnostic source resolution

Decided with the maintainer during implementation, following the conventional
uploader shape (the client names the repository by slug, the server resolves
it). This replaces the request's `repository: {host, repository_id}`; the
receipt still returns the resolved `{host, repository_id}`.

- Wire: the request takes `repository: "owner/name"`. The server resolves the
  slug, ignoring case, among the tenant's selected repositories on the
  configured host through the existing `GithubConnections::verify_repository`
  remote path; an unselected or unknown slug is
  `WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE`. Matching and custody use only the
  resolved stable id, so a later rename does not affect accepted evidence.
  Publishers need no connection-read permission.
- CLI/SDK resolve `repository` and `commit` independently; the first source
  that supplies a value wins:
  1. explicit `--repository`/`--commit` (SDK `repository=`/`commit=`);
  2. `WYRD_REPOSITORY`/`WYRD_COMMIT`, which the caller sets however its
     environment provides the values;
  3. a Git checkout when one is present: `origin`'s `owner/name` and the
     `HEAD` commit, read from `.git` without running `git`.
  A value none of these supplies is a local input error naming the flag and
  the variable. The caller decides which commit the evidence is about; Wyrd
  never guesses a different one.

## Selected custody and dependency flow

Reuse `EvidencePlanRequest`, client `verify::File` and storage's plan/upload/
completion first. Only completed verified File/Artifact handling or validated
canonical JSON reaches publication. Extend existing evidence/subject permission
checks; do not grant unrelated execution/Card mutation rights or invent a new
permission category. Validate authorized subscribers' declared type/format/schema
before accepting. Identical matching identity/content replays its receipt;
conflicting replacement refuses.

Existing Bifrost `verification/execution_inputs` requires execution and Verifier
IDs, so it cannot hold pre-execution evidence by faking those IDs. Extend the
existing Bifrost verification/Scribe custody owner for standalone accepted
publication and subscriber acceptance, with existing storage File custody. Keep
SQL for binding/queue conditional coordination only; no payload mirror/new table.
Persist which existing subscribers accepted the publication internally so later
subscribers do not receive historical evidence. Do not expose subscription
generations, reactivation or rollback APIs. No eligible subscription returns
explicit unmatched without retained publication; temporary uploads keep normal
cleanup. Existing subscribers without execution retain accepted evidence under
normal retention; future matching executions may consume it.

For queued/running consumers, accepted evidence releases only actual dependent
steps; independent work continues. For waiting consumers, stage continuation of
the same ExecutionId through existing SubmissionSink/queue. Completed/cancelled/
superseded execution never reopens. Execution initialization and recovery read
accepted matching evidence as well as notifications; a lost notification cannot
lose an ACKed input. Existing executions keep their exact policy pins even when
source evidence matches another eligible policy subscriber.

Extend `vala-verify`'s existing logical execution state and `server/verification/
state.rs::RunState` with nonterminal blocked dependencies and accepted facts.
The sole execution state owner consumes them; do not introduce a second writer
racing running branches. When no branch is active/runnable, release execution,
workspace, Python and model resources immediately. Bounded background custody
keeps the claim permit and renewing lease, persists the parked state and waits
for its ACK **before** retiring current queue delivery; no compute permit is held
over that ACK. A crash before the ACK can repeat unpersisted work under REQ-017.
Use existing runner/SQL waiting/readiness coordination, not a sleeping block or
continuous polling execution. Every true continuation has stable internal delivery
identity so retry reacquires queued capacity for the same execution and does not
replenish attempts, spend, accepted work or original deadline. A pending queue
row's bounded canonical-state check repairs ACK→readiness gaps after restart.
Authority loss, deadline and task 006 supersession fence continuation while parked.

Publication positive receipt follows durable canonical evidence acceptance, not
upload/staging/notification or completed review. Persist enough coordination for
every accepted obligation to be rediscovered. Recovery returns exact accepted
state, authoritative absence or explicit unavailability; do not start fresh on
an unavailable read. Missing deferred evidence remains bounded by the original
execution deadline and cannot pass.

## Outputs, consumers and surfaces

Return approved evidence receipt/status/count/timestamp and waiting inspection
with deferred names. Task 008 reuses this checkpoint/park/continuation lifecycle
for decisions; task 009 projects waiting without recomputation. Ship HTTP/shared
client/all three SDKs/CLI/MCP publish and read projections, scoped writes,
schemas/errors, upload-based examples and diagnosis here. Amend
**wyrd-design.md → Verifier** and **bifrost-design.md → Table and row identity /
Durability and visibility / Ingest / Public surface** for standalone custody,
accepted facts, ACK and parked state; update relevant authorization guidance.

## Connected proof

**AC-005:** Through task 002 connection and task 003 declaration, publish real
JSON/completed media before consumer, waiting, running and with no execution.
Observe subscription-backed retained evidence then task 006 admission consuming
it without republish. Exercise no subscription, new subscriber after unmatched/
accepted publication, identical/conflicting retry, wrong tenant/repository/commit/
type and different exact policy pins for the same source. Assert typed receipts,
explicit unmatched/no retained replay, exact content and no terminal reopening.

**AC-006 (deferred portion):** With an independent branch, park only consumers;
when all block observe released task 001 compute resources while the parked ACK
is held, then the retired delivery/lease after ACK; publish and resume the
same ExecutionId with settled work/spend/retries/deadline unchanged. Supersede
through task 006 or revoke while parked; old acceptance cannot release work.

**AC-007 (evidence/park):** Restart while an execution waits for evidence. After
a positive receipt, the accepted evidence survives and the same execution
finishes without producer replay; a publication whose outcome is unknown is
retried; evidence published while its consumer runs still reaches it.
Unavailable canonical state never becomes empty/fresh state. Prove with ordinary
restart, retry and refusal journeys; no test-only crash hooks.

**AC-011 (evidence/wait):** Follow source-inference/override and publication docs,
inspect waiting state and durable receipt using shipped surfaces.

Tier 1: matching Rust/Python/TypeScript SDK stories and MCP journeys using README
runners, WyrdTestServer/Postgres and actual 003→006 outputs. Tier 2: Bifrost/Scribe
acceptance and queue recovery at existing verification/server and SQL integration
owners via `mise run test:bifrost:journey:server`, `mise run test:sql` and the
affected `test:vala` lane; tier 3: reference/slot/schema/matching branches. Add
new focused commands only after tests exist. Run README contract/type/boundary
checks. Task 008 owns the decision-specific half of AC-006/007.

## Obligations

| ID | Promise (source) | Open cases |
|---|---|---|
| REQ-008 | `deferred: true` only on json/media slots; registration derives `queue_only`, users cannot set it, direct verify refuses; only consumers of a missing input wait (§Input, §Revision — queue_only) | none |
| REQ-009 | Publish by tenant + repository slug + commit + input with `verifier:run`; validated against subscribers; matched, retained, replayed or refused; receipt only after the Bifrost ACK; publication alone starts nothing (§Input, §Selected custody, AC-005) | none |
| REQ-011 | A parked execution releases compute, keeps its claim until the park ACK, resumes the same ExecutionId with work, attempts, Agent budget and deadline kept; supersession, revocation and deadline fence it (§Selected custody, AC-006) | none; maintainer decision: a park refunds its attempt (waiting is not failure), and a resume spends one |
| REQ-016 | CI-agnostic source resolution; HTTP, client, three SDKs, CLI (`--file`), MCP, waiting inspection, docs, upload-based example, architecture (§Revision — slug, §Outputs, AC-011) | none; maintainer decision: one `--file` flag, a `.json` file or stdin sent inline and any other file uploaded (a media slot accepting `json` cannot be fed from the CLI) |
| INV-004 | Unavailable custody is never read as absent or fresh (spec INV-004) | none; maintainer decision: the evidence read path rests on source inspection plus the publish-path journey, no test-only hook |
| AC-007 | Restart while an execution waits for evidence: accepted evidence survives and the same execution finishes; unknown publication outcomes retry; evidence published while its consumer runs reaches it (spec AC-007, revised by the maintainer to drop crash-point enumeration) | none |

## Evidence

| Obligation | Implementation | Proof | Verification | Result |
|---|---|---|---|---|
| REQ-008 | `wyrd-spec` `InputSlot::is_deferred`; registry derivation; `vala-verify` engine wait | `card/verifier/mod.rs` `only_json_and_media_slots_can_be_deferred`; `pg_card_declaration_route.rs:1873`; `engine/tests.rs:4256` | PASS | PASS |
| REQ-009 | `wyrd-server/src/verification/evidence.rs`; `bifrost.verification.evidence` (no migration) | `pg_deferred_evidence.rs` `publication_is_matched_retained_and_resumes_the_waiting_execution` (other tenant 404, planned-but-incomplete upload 400, zero executions after publication, pin 1.0.1 consumes), `unacknowledged_publication_is_unavailable_and_its_retry_resumes`; Rust SDK `deferred_evidence.rs:224` | PASS | PASS |
| REQ-011 | `runner.rs` park branch; evidence wake; queue fences | `pg_deferred_evidence.rs` main test, `parked_executions_end_by_supersession_and_deadline`, `a_resumed_execution_keeps_its_admission_deadline`, `evidence_published_while_its_consumer_runs_reaches_it`, `a_parked_execution_releases_compute_before_its_park_is_acknowledged`, `lost_repository_access_fences_a_parked_execution`; `pg_verification_runtime.rs:4155`; `engine/tests.rs` `deferred_inputs_hold_only_their_consumers_until_supplied` (settled work not replayed), `a_resumed_execution_keeps_its_spent_agent_budget` (fails when the reseed is set to 0) | PASS | PASS |
| REQ-016 | `wyrd_client::verify::Evidence`; SDKs; `wyrd evidence publish`; `verification.publish_evidence`; docs guide; architecture | `wyrd-client/src/verify/evidence.rs:227` (explicit precedence; each missing value names flag and variable); Rust/Python/TS journeys; `github_journey.rs` `ci_publishes_deferred_evidence_through_the_cli` (`--file results.json`), `ci_uploads_a_report_file_through_the_guide_script` (`examples/publish-deferred-evidence/publish.sh`); `wyrd-cli/src/evidence.rs` `publish_takes_one_file`, `json_files_are_sent_inline`; `wyrd-mcp/tests/bifrost/mcp/github.rs:383` | PASS; `codegen:check`, `docs:check`, `check:examples` PASS | PASS |
| INV-004 | `evidence.rs:180` fenced-but-invisible → unavailable; `runner.rs:827,973,979,1714` an unreadable evidence read defers or retries as `evidence_unavailable`, never absent (source inspection) | `pg_deferred_evidence.rs` `unacknowledged_publication_is_unavailable_and_its_retry_resumes` | PASS | PASS |
| AC-007 | — | `pg_deferred_evidence.rs` `a_waiting_execution_finishes_after_a_restart` (runtime stopped while waiting, evidence accepted with no runtime, fresh runtime passes the same execution, one attempt spent), `unacknowledged_publication_is_unavailable_and_its_retry_resumes`, `evidence_published_while_its_consumer_runs_reaches_it` | PASS | PASS |

**Commands:** `mise run fmt`, `lints`, `py:lints`, `py:typecheck`, `ts:lints`,
`ts:typecheck`, `ts:napi:check`, `codegen:check`, `check:deps`, `docs:check`,
`git diff --check`; Postgres tests through
`scripts/postgres/with-test-postgres.sh` after `db:migrate:all:inner`, using
`cargo nextest run --locked` with exact selectors for every test named above. `test:sql` and `test:bifrost:journey:server` pass; `vala-verify` + `vala-bifrost-redux` lib under test Postgres: 1010/1010 pass.

**Process exceptions (written after code, no RED):** REQ-008a spec/registration,
REQ-008b engine test, publish route/client/CLI/MCP tests, the three SDK
journeys, wake on replay, REQ-009b refusal cases, the media journey and the
REQ-011a claim test. These tests pin existing behavior.

**Diagnoses:**
- `recorded_at` replay mismatch. Cause: the receipt carried nanoseconds while Bifrost stores microseconds. Fix site: truncate at receipt creation.
- Completed status was read before its `result` landed. Cause: the status and the Bifrost result become visible separately. Fix site: the test helper waits for both.
- REQ-011a, reverted. I read "no execution permit remains attached to that ACK" as requiring the claim slot to be freed, and freed it early (c71be4283). That conflicts with task 010: the claim permit bounds the whole claim lifetime and is not compute. This task's text also says custody "keeps the claim permit". Reverted in 798df3e1d and replaced by the claim-kept test (0ba344d30).
- The webhook-based claim test returned 504. Cause: the second PR's admission write queued behind the stalled events write in Scribe. Fix site: a direct submit in the test.
- A lease-check false negative. Cause: `lease_token` stays set after release by design. Fix site: assert `lease_expires_at`.

- Running-consumer journey answered 503 (first attempt). Symptom: holding the park's `execution_events` write to keep the consumer `running` made the publish answer `WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE`. Cause: Scribe queues the `bifrost.verification.evidence` write behind the held write, so a Scribe stall cannot hold a consumer running while evidence lands. Fix site: none in product; the test now holds the consumer running with the existing `EngineScript` hold instead of a Scribe stall, then lets the real engine resume.
- Wrong-tenant expectation. A tenant with no GitHub connection is refused with the existing `WYRD_GITHUB_404_NOT_CONNECTED`, not 403; the test expectation was corrected.

**Non-goals confirmed:** no Trigger, wait action, broker, new permission category, new PostgreSQL table, subscription API, or CI-provider detection; decision waits are left to task 008.
