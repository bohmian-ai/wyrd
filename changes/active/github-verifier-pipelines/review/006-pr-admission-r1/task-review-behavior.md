# Task 006 behavior review

**Result: FAIL.** Four source-proven behavior gaps remain. These are discovery proposals for independent validation, not final finding IDs.

## Subject and authority

- Base: `2165fe606efe498db4e0455866d29236e85d889c`.
- Candidate: `efda8cb2c59c8206b52dbe1d92fcae1ba34be430`; HEAD checked and identical.
- First formal review, full base-to-candidate diff; no carried-forward acceptance.
- Approved spec: revision 5 from `git show 0f08e6c9f:changes/active/github-verifier-pipelines/spec.md`. Draft revision 6 does not govern this task.
- Original task: `tasks/006-pr-admission-and-supersession.md`, including its A1–A18 obligation seed. Implementation completion claims were treated as claims and checked against source/assertions.
- Read task-review skill, root instructions, SQL/Python/TS/docs directory rules, spec-driven-development, agent_docs index, Rust/errors/agent-surfaces/maintainer guides, architecture authority map, Trigger/queued activation and operations recovery sections, and applicable security execution-authority text. Design supplements supplied context only.
- CodeGraph was used before source navigation; its map was expanded through actual source, sibling queue paths and tests. No delegation or source mutation occurred.

## Expanded path and reuse coverage

Verified signed `routes::webhook` → `GithubConnections::webhook` signature verification and installation routing → `pull_request` eligibility/provider read → selected Repository/declaring-principal resolution → `BackgroundAuthority`/`TenantTokenIssuer::accountable` → audit authorization per binding → queue lock/reservation/refusal/supersession → stable `ResultPayloadBuilder::inputs` → Scribe execution-keyed frame → existing `SubmissionSink` → activation after Scribe acknowledgement/commit → existing fair claim/runner/input resolver and fenced terminal settlement.

Expanded beyond the starting map into `installation_token`'s before/after-mint revision check, `lock_association`, declaration selection queries, the no-target branch, replay branch, zero-row deactivation, shared `frozen_input`, claims and delivery projection. Compared submission writer batching, scheduled/group admissions, input origins and existing cancellation with the PR extension. The extension uses the existing engine, queue, Scribe and submission outbox; no duplicated engine/broker or unearned generic layer was found. The durable reservation is needed to retain the complete fanout before independent input ACKs.

Inspected recovery's App JWT delivery pagination, origin filter, grouping by GUID/2xx outcomes/redelivery count and server worker lifecycle. Inspected queue status → contract → Rust/Python/TS/CLI/MCP paths and actual changed journey assertions, authored Python stubs, generated TS projection, MCP guidance and docs. No extraction/parking/provider-result delivery or UI implementation entered this task.

## Acceptance matrix

Paths below are relative to the repository. `PR tests` means `crates/wyrd/wyrd-server/tests/integration/pg_github_pull_requests.rs`; `queue PR` means `crates/wyrd/wyrd-sql/src/queries/verifier_executions/pull_requests.rs`. PASS means source plus available proof support that row, subject to listed failed scenarios, not an independently rerun all-surface gate.

| Obligation | Implementation evidence | Verification evidence actually inspected | Result |
|---|---|---|---|
| A1 signed, connected, selected repository resolves declared bindings, no PR-head policy | `github/service.rs:631–671` verifies signature before routing; `pull_requests.rs:213–238` gets installation token then reads only provider PR; `targets:314–385` selects stored policy | PR tests `suspension_disconnect_and_ineligible_pull_requests_owe_no_work`: signature failures are 401, foreign/unselected installation/repository create no rows; lifecycle test asserts two frozen bindings. Race after token check remains B4 | **FAIL B4** |
| A2 exact Repository/Verifier/Trigger/authority/source frozen; selection affects later generations | `RESERVE_SQL`/`REFUSE_SQL`, `reserve_pull_request`; frozen policy and source stored before handoff | Lifecycle test compares entire served PR object, inline/referenced Trigger and owner-policy versions; SQL attribution join asserts selecting principal; `policy_reselection_affects_only_later_admissions` asserts old rows/IDs unchanged and next source uses 1.0.1 | PASS |
| A3 admitting lifecycle actions, draft/title exclusions | `PullRequestEvent::observable`, `Observed::of` | Unit tests assert each admitting/deactivating action and metadata exclusions, state and draft classification; lifecycle/base-retarget tests assert no metadata work, ready/reopen and new base-ref generation | PASS |
| A4 declaring principal/current authority; durable refusal and 2xx | `targets`, `BackgroundAuthority::accountable`, issuer current resolve, `REFUSE_SQL` | `revoked_accountable_principal_settles_a_durable_refusal` checks both rows errored, exact credential-revoked code, 202, empty claim; SQL refusal test asserts stored error/pins | PASS |
| A5 ACK after committed admissions/obsolete/refusal; read/storage/staging errors retryable | `hand_off` awaits all AdmissionReply channels; SubmissionSink replies after commit | Lifecycle checks rows due at 202; interruption test denies accepted answer; failed-read test checks non-2xx/no rows then successful redelivery, but injects 502 only. 403 is incorrectly swallowed | **FAIL B3** |
| A6 duplicates/restarts/partial fanout retain first policy and IDs, newer source supersedes | reservations before Scribe; execution-keyed frames; idempotent ACTIVATE_SQL | `interrupted_handoff_completes_the_same_fanout_after_restart` compares all execution IDs and two due rows; `recovery_after_a_new_head_admits_the_current_source` checks old IDs cancelled/new a2 due; duplicate/reselection tests compare original rows. Partial queue commit is manually modeled, not an actual after-ACK crash | PASS |
| A7 bounded paginated failed-delivery recovery through App owner | `GithubApp::deliveries` page cap, next links/origin check; `recover_deliveries` GUID groups and attempts; server worker | Lifecycle test asserts asked 1, then 0 after 2xx, then three-attempt bound; App unit assertions cover next-page selection and foreign origin/scheme/path. B3 causes an incorrectly successful delivery to be excluded from recovery | **FAIL B3** |
| A8 newer source cancels all unfinished older work, fences leases, retains completed baseline | `SUPERSEDE_SQL`/`reserve_pull_request`; unfinished states match present schema | Lifecycle test checks queued/running supersession and StaleLease; `completed_baseline_survives_when_a_new_head_supersedes_running_work` asserts completed P unchanged, cancelled A, pending B and late A StaleLease. Zero-binding selected policy skips cancellation | **FAIL B2** |
| A9 delayed older source never becomes current after newer observation/deactivation | timestamp before provider read; `lock_pull_request`/LATEST_SQL and obsolete check | Lifecycle held-B-after-C and held-open-after-close assertions cover existing rows and changed generation; no same-source replay watermark or first-ever close marker is written | **FAIL B1** |
| A10 draft/close/removal/suspend/uninstall/disconnect cancel unfinished, retain history | DEACTIVATE_SQL, CANCEL_REPOSITORIES_SQL and connection access-loss calls | Lifecycle and access-loss tests assert codes/history; close under worker checks StaleLease. First-ever admission held across disconnect/removal bypasses zero-row cancellation | **FAIL B4** |
| A11 41 PRs under bounded ingress commit recoverable executions and progress fairly | existing bounded outboxes, fair CLAIM_SQL and runner | `burst_of_forty_one_pull_requests_admits_each_binding_once` launches 41, asserts 82 rows, two per head, all completed and each served head; existing shared staging-full proof inspected by task evidence | PASS |
| A12 queue/authority own pins and fences for future consumers | pins carried by ExecutionInput/claims/status; queue cancel clears running lease | SQL generation test asserts exact ExecutionInput pins, old status and StaleLease; server tests same. B2/B4 leave unsupported current/runnable work despite these primitives | **FAIL B2/B4** |
| A13 execution/binding inspection across HTTP/SDK/CLI/MCP and guide | shared execution contract; queue status; Python authored TypedDict; generated TS fields; existing handle/tool paths | Rust/Python/TS journey diffs assert number/repository/base/head ref+SHA/head repo/delivery, superseded code and retained pins; CLI checks disconnect code; MCP reads newest binding and old execution after new generation; guide directs inspection | PASS |
| A14 public docs explain eligibility/pins/cancellation/diagnosis | declare-a-repository §5, concept, MCP guide and generated llms projection | Content checked against implementation/assertions; docs check reported in original task. Documented guarantees are violated in B1–B4; no separate wording-only finding | **FAIL consequentially** |
| A15 amend design/operations for inline+referenced PR, ACK/recovery, bounded cancellation | architecture Trigger section and reliability/recovery | Actual amendment inspected: precise base-ref/head identity, binding source, account, queue handoff, three-redelivery limit and non-atomic remote cancellation preserved | PASS |
| A16 no new table/queue/engine/inbox/broker; Scribe, isolation, no credential served | migration adds only execution columns/index; server uses TenantConn and existing outboxes; no SDK durable owner | Migration source; lifecycle served JSON checked against every mock secret; dependency/tenant-isolation checks independently passed by orchestrator. Access-loss race B4 violates connected authority but does not introduce cross-tenant SQL | PASS structural; B4 covers access integrity |
| A17 ordinary schedule/group/submitted behavior preserved | default input branches retained, old admissions still executed in same writer; adds empty activation call and PR branch | Full changed matching/constructor paths inspected; implementation-reported 129 existing verification tests and SQL suite; no observed regression path | PASS |
| A18 installation routing admits only connected tenant | signature → installation/host operator lookup; all following queries tenant-bound | Two-tenant test checks other installation produces zero A rows, A creates two A rows and zero B rows | PASS (connection can change during B4) |
| Additional task constraint: do not execute DAG in webhook or read branch policy | webhook stages only; runner remains engine caller; no contents endpoint in admission | Source caller trace; 41-PR test enables ordinary verifier runtime; basic journey uses declared queued Verifier | PASS |
| Additional task constraint: no new extraction/baseline/parked/provider-write implementation | diff limited to admission, pins, present queue states, docs/testing | Actual diff and enum consumers inspected; completed P retained for later baseline consumer | PASS |
| Additional proof obligation: all shipped language surfaces use real connect/declaration | SDK/MCP/CLI tests build on existing connected declaration fixtures rather than fabricated bindings | Changed test assertions inspected; new Rust SDK test has lifecycle ignore consistent with its journey runner, so reported execution requires that runner | PASS available evidence |

## Proposed findings

### B1 — INCORRECT: latest observation is not retained for replay or zero-row deactivation

**Obligation:** REQ-005; A9; task Lifecycle paragraph 2; architecture Trigger ordering guarantee.

**Locations:** `github/pull_requests.rs:239–256,270–295`; queue PR `LATEST_SQL:57`, `DEACTIVATE_SQL:80`, `PullRequestHistory::obsolete:343`, `lock_pull_request:402`. The latest timestamp is the maximum of generation creation/deactivation timestamps on execution rows. The replay branch does no write. A deactivation updates only existing execution rows.

**Reachable failing paths:**

1. A is accepted at t1. Provider head becomes B; request R obtains timestamp t2 and its B response is delayed. Force-push restores A, and a newer synchronize R2 reads A at t3. R2 takes replay, commits without recording t3, and answers 202. Release R: history latest is still t1, so t2 is considered newer; it supersedes A and admits stale B even though a later current provider read confirmed A.
2. Hold the first-ever opened PR read with open source at t1, before any execution exists. Close PR; close read at t2 commits DEACTIVATE_SQL over zero rows and returns 202. Release the original read: LATEST_SQL is NULL; it reserves runnable work for the now-closed PR. Draft conversion has the same path.

The existing HeldAnswer mock fixes the provider response before waiting (`wyrd-testing/src/github.rs:1086–1099`), so both are the same supported delayed-response mechanism already tested for changed generations. Existing delayed-B-after-C and delayed-open-after-close tests first create generation rows; they cannot falsify these paths.

**Consequence/materiality:** stale/current-source integrity and explicit out-of-order guarantee fail, with runnable work and future current-result eligibility for obsolete or closed source. Force pushes, first opening and concurrent close are supported lifecycle operations, not arbitrary input hardening.

**Correction boundary/burden:** repair ordering once at existing PR observation/queue owner so replay and no-row deactivation cannot permit an older in-flight read to reserve work. Preserve frozen generation timestamps/pins and byte-identical Scribe replay. Existing per-PR serialization is the natural mechanism; serializing the provider observation and decision under that existing authority avoids a second journal/inbox. A downstream runner/provider guard alone cannot satisfy admission/2xx ordering. No new public contract or table is required.

**Focused proof:** the two deterministic held-read sequences above; assert 202 leaves no stale B/new closed executions, retains accepted pins/IDs and normal reopens still admit. These are source-proven scenarios, not newly executed repro tests.

### B2 — MISSING: removing future PR bindings skips supersession on the next source

**Obligation:** REQ-003 permits `verifier=[]`; REQ-005/A8/A12 require new source to cancel obsolete unfinished work while policy selection alone does not sweep.

**Locations:** `github/pull_requests.rs:274–278,326–331`; queue PR `reserve_pull_request:564–590` is the only new-source SUPERSEDE_SQL invocation.

**Producer-to-consumer:** accept A with queued/running executions. Declare a new exact Repository policy with an empty binding list (an approved normal declaration outcome); existing A must remain at declaration time. Push B. Provider read confirms B; `targets` resolves the new selected Repository then sees empty `pull_request_bindings` and returns None. Admission commits and returns success before calling `reserve_pull_request`, so A keeps its runnable status/lease and no supersession fence is established.

**Consequence/materiality:** old-source A continues and can settle as current despite B being observed. Explicitly removing future automatic review is supported; it must not remove source-obsolescence safety on an already accepted review. Current policy-reselection test only reduces two bindings to one, leaving this branch untested.

**Sibling coverage/correction:** source obsolescence belongs to `VerifierExecutionQueue`, independently of whether selected policy creates replacement work. Reuse its existing supersession update under the same PR decision; do not cancel A when the empty policy is merely selected, and do not invent a background sweep or fabricate an execution just to cancel it. This is a small local ownership correction, with B1 preserving no-work observation order.

**Focused proof:** accept and claim A, declare no PR bindings and assert A initially unchanged, push B and assert A cancelled with `pull_request_superseded`, late completion StaleLease, no B executions, history intact, successful webhook response.

### B3 — INCORRECT: GitHub 403 read failures are positively acknowledged as no work

**Obligation:** A5/INV-003 consequential ACK; task explicitly keeps failed GitHub reads retryable. A7 recovery cannot recover positively acknowledged loss.

**Locations:** `github/app.rs:454–458`; `github/pull_requests.rs:224–231`; `recover_deliveries:505–510`.

**Reachable failure:** selected connected repository, eligible current PR, installation token successfully minted. GitHub PR GET responds 403 due to REST rate limit. The App maps every 403 to `Ok(None)`; caller says GitHub no longer shows PR and returns success, route sends 202, no reservation or refusal exists. Delivery recovery sees 2xx and never redelivers. For B while A runs, the same path leaves A unfenced.

GitHub explicitly documents primary/secondary rate-limit failures as 403 or 429: [REST API rate limits](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api). A 403 is therefore not evidence of durable obsolete/refusal settlement. Existing failed-read test injects BAD_GATEWAY and does not exercise this status.

**Consequence/materiality:** a routine provider throttle silently loses required reviews/obsolescence and claims success to provider/operator. The task explicitly exercises burst traffic; this is a supported upstream failure, not speculative transport hardening.

**Correction/burden:** use the existing typed unavailable/decode error path for an unconfirmed PR read, including 403, so delivery remains failed/retryable. No retry subsystem is necessary. If an actual inaccessible response is separately settled, it must use the existing access-loss cancellation owner rather than reinterpret a failed read as completed admission. A minimal correction can simply fail 403 retryably.

**Focused proof:** protocol mock returns 403 with rate-limit headers/body once for PR GET; webhook is non-2xx and no work admitted; recovery/redelivery after recovery creates exactly one frozen fanout (and supersedes A when applicable).

### B4 — INCORRECT: first admission can commit after disconnect/removal finished

**Obligation:** REQ-001/REQ-004; A1/A10/A12; access loss must stop connection use and unfinished work before association release.

**Locations:** `github/pull_requests.rs:213–238,274–295`; `github/service.rs:876–931` installation-token revision check; `fence_and_release:368–386`, `access_lost:395`; queue PR `CANCEL_REPOSITORIES_SQL:128`.

**Producer-to-consumer:** for a first-ever PR with no queue rows, mint/recheck token, then hold the successful PR GET response. Disconnect through the normal connection owner (or remove the repository): slot change and cancellation commit, but cancellation finds no row. Release the held read. The admission transaction locks only the PR, reads selected Card/bindings, authorizes the declaration principal and inserts/activates new rows. It does not lock or revalidate the connection slot at that transaction boundary. The observation clock cannot help because no execution exists to carry deactivation time. Result is fresh runnable work after disconnect returned success.

The existing token revision check protects minting only; it completes before the held API read. Existing `lock_association` is used by declaration, and release/removal/suspension take the slot's authority, but PR reservation does not participate. Existing disconnect tests create rows first and therefore only prove cancellation of already-visible work.

**Consequence/materiality:** explicit connection revocation boundary is bypassed and work runs for a disconnected/inaccessible repository. This is independent of obsolete source observations: source can remain unchanged and open throughout. It affects suspension/uninstall/removal siblings of disconnect as well.

**Correction/burden:** join admission to the existing installation-slot lock/revision authority and revalidate original association, selected repository and suspension at reservation commit. Keep provider IO outside that short final slot check. Reuse the existing connection lock and revision machinery, rather than runner-side repeated checks or a new token/cache owner. Preserve lock order with connection/declaration writers. B1's ordering correction alone does not establish connected authority.

**Focused proof:** first-open read held; complete disconnect (then separately selected repository removal), release read; no runnable row is admitted, no successful stale association use occurs, no history is deleted. Repeat with previously accepted work to preserve existing cancellation/history behavior. No new harness is needed: existing HeldAnswer and connection APIs suffice.

## Verification and limits

Independently executed `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/components::github::/)'`: 4 passed, 598 skipped (nextest run `a30371f8-7e04-4fe2-9a65-b33b89e93fb1`). Inspected actual assertions in server, queue and every changed shipped-surface journey; no test name alone was used as proof. Orchestrator execution evidence is in `verification.md`; implementation-reported broad SQL/server/journey/docs/codegen checks remain explicitly reported evidence, not this reviewer's own executions.

No source edits, commits, generated-artifact regeneration, database mutation or independent reproduction tests were made. All four proposals are statically proven paths and require independent validation; passing current focused tests does not cover their missing branches. Other specialist conclusions were not read.
