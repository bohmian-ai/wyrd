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

**AC-007 (evidence/park):** Controlled crash before/after upload completion,
publication ACK, pending checkpoint ACK, retirement, continuation commit and
recovery read. After positive receipt observe exact accepted evidence and owed
continuation without producer replay; lost notifications are repaired. Unavailable
canonical state never becomes empty/fresh state. Assert through public journey
where observable, and real-dependency integration for the exact crash fences.

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

| ID | Promise (contract source) | Planned proof |
|---|---|---|
| REQ-008a | `deferred: true` only on json/media slots; omitted/false and samples/source slots keep existing paths; registration derives `queue_only=true` for a deferred slot, false/omitted alone does not; direct verify refuses before any step; cold registry flag only (task §Revision) | declaration route integration (true/false/omitted, media) + direct refusal; wyrd-spec decode unit |
| REQ-008b | References alone establish consumers: a step reading a missing deferred slot (and its dependents) waits while independent steps run; whole-Verifier requirements wait; no Trigger/wait action/depends_on (REQ-008, task §Input) | vala-verify engine unit (independent branch runs, consumers blocked, park outcome names slots) |
| REQ-009a | `POST /v1/verification/evidence/publish` matches tenant + connected repository (`owner/name` slug resolved by the server to host/repository_id, §Revision — repository slug) + commit + input; no Verifier/execution/group/retry key; tags commit only; Git metadata grants nothing; existing evidence/subject permission, no new category (contracts §Deferred input publication) | server route integration: wrong tenant/repository/commit, unauthorized caller, unconnected repository |
| REQ-009b | Only completed verified File or canonical JSON is published; type/format/schema validated against authorized subscribers before acceptance; incompatible → refused | route integration: wrong type/format/schema, unknown/incomplete File |
| REQ-009c | Subscribers = active selected bindings of the repository plus nonterminal executions of its commit whose Verifier declares the deferred input; accepted subscribers persisted internally; later subscribers get no historical evidence; no subscriber → explicit unmatched (409), nothing retained; identical identity+content replays receipt; conflicting replacement 409; different exact policy pins each receive (contracts table) | route + SQL integration: no subscription, new subscriber after unmatched/accepted publication, identical/conflicting retry, two policy pins |
| REQ-009d | Positive receipt (`evidence_id, repository, tags, input, status accepted|replayed, matched_subscriptions, recorded_at`) only after durable Bifrost acceptance; standalone custody in Bifrost verification owner via Scribe; no new PG table or payload mirror (INV-002/003/007) | Scribe/Bifrost integration: receipt after fence; held-Scribe publish not acknowledged |
| REQ-011a | No runnable branch with a waiting deferred input → compute (execution/workspace/Python/model) released at once; background custody keeps claim/lease, persists the parked state (waiting names + accepted facts) and retires the queue delivery only after ACK; crash before ACK may repeat unpersisted work (REQ-011, REQ-017) | runner integration: parked ACK held → compute free, row still leased; after ACK row pending not leased |
| REQ-011b | Publication to a waiting execution stages continuation of the same ExecutionId through the existing queue; running/queued consumers receive it through normal scheduling; initialization/recovery read accepted evidence, so a lost wake is repaired by the pending row's bounded canonical-state check; continuation preserves settled work, attempts, spend and original deadline (REQ-011, INV-005) | journey: publish while waiting/running/queued/before admission → same ExecutionId completes; settled step not replayed; attempts/deadline unchanged |
| REQ-011c | Completed/cancelled/superseded executions never reopen; supersession, revocation and deadline fence a parked execution; missing evidence expires under the original deadline and never passes (REQ-011, INV-006) | journey/integration: supersede and revoke while parked; deadline while parked → timed_out |
| INV-004 | Recovery and publish dedup return exact accepted state, authoritative absence, or explicit unavailability (503) — never fresh state on an unavailable read | integration: fenced-but-invisible evidence → unavailable, not absent |
| REQ-016a | Source resolution is CI-agnostic: explicit argument → `WYRD_REPOSITORY`/`WYRD_COMMIT` → Git checkout read from `.git`; no CI-provider detection or provider variables anywhere, no `git` binary, missing value is a local error naming flag and variable (§Revision — repository slug and CI-agnostic source resolution) | client unit tests for each source and precedence, no checkout, missing value; source inspection for provider names |
| REQ-016 | HTTP, shared client, Rust/Python/TypeScript SDKs, CLI (`wyrd evidence publish <input> --file f [--repository owner/name] [--commit sha]`; resolution per REQ-016a), MCP publish tool; waiting status with deferred names in execution inspection; stable errors/schemas; docs + upload-based example; wyrd-design Verifier and bifrost-design custody/ACK/parked sections; authorization guidance (AC-011) | SDK journeys ×3 + MCP journey; CLI test; codegen:check; docs content review |
