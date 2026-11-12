# Independent findings validation — Task 006, round 1

Recommendation: **FIX_REQUIRED**. Seven deduplicated material findings remain. The SQL tuple proposal is rejected under the review threshold; it creates no remediation obligation. No specification revision or missing specialist blocks this review.

## Subject, authority and independence

- Base `2165fe606efe498db4e0455866d29236e85d889c`; candidate `efda8cb2c59c8206b52dbe1d92fcae1ba34be430`.
- First complete base-to-candidate review; no prior acceptance is carried forward.
- Approved specification is revision 5, read from `git show 0f08e6c9f:changes/active/github-verifier-pipelines/spec.md`. Revision 6 is draft and does not govern Task 006; tasks/README retains the revision 5 basis.
- Original task: `changes/active/github-verifier-pipelines/tasks/006-pr-admission-and-supersession.md`, including A1–A18 and its recorded evidence.
- Read the complete task-review skill, scope, claim comparison, verification record and all seven discovery reports. Read the workflow, maintainer/Rust guides, authority map and applicable SQL/Python/TypeScript/docs instructions. Applicable architecture includes queued activation, exact execution inspection, current permission, installation association and shutdown/drain authority. CodeGraph was consulted before source navigation, then expanded through actual source and callers.
- Independently inspected the complete admission, target selection, handoff, recovery, queue lock/history/reservation/activation/deactivation, installation mint/fence/access-loss and retained-inspection bodies implicated by corrections. Checked SQL constants, migration, sibling claim/settlement and access writers, shared SubmissionSink replies/commit, supervisor/drain consumers, changed wire/stub/journey diffs and the named rustdoc omissions. Discovery agreement was not treated as proof.
- Source and tests remain unchanged. Only this report is authored by this validator. Fresh managed-Postgres and static checks are credited to the orchestrator in verification.md; race failures below are independently source-proven, not newly executed reproductions.

## Specialist selection and coverage

| Boundary | Required coverage and validation |
|---|---|
| Complete task and user paths | behavior-rev covers A1–A18, eligibility, source and policy pins, real connect/declaration, all shipped inspection surfaces and exclusions. Its source/assertion mapping covers the complete task, rather than only changed leaf functions. |
| Shared lifecycle and concurrency | invariant-rev covers PR identity/generation, canonical fanout, replay, cancellation and sibling queue consumers; persistent-data domain review independently covers transactions, RLS, slot/execution writers and durability. Both are applicable and present. |
| Cross-tier repository rules | repo-rev covers all 42 changed paths: Rust contract/server/SQL/testing/CLI/MCP, Python authored stubs/PyO3/tests, TypeScript generated types/N-API/tests, human and generated docs, architecture and evidence. Required panic rustdoc is included; green language lints do not waive it. |
| Ownership and reuse | maintainer-rev compares complete submitted/scheduled/group workflows, Scribe and queue owners with PR admission. Its PASS is supported: existing owners lack provider observation and durable whole-fanout reservation, while candidate reuses their handoff, runner and settlement. No supported DRIFT proposal remains. |
| Failure/recovery/process lifecycle | system-rev covers consequential ACK, restart and partial fanout, provider recovery, process supervision and shared drain. Its unique shutdown proposal is source-validated below. |
| Security/RBAC/tenancy | security domain review covers signature/routing, current declaring authority, credential confinement, tenant SQL, access-loss fence and shared read authorization. Its unique inspection proposal is source-validated below. |
| Omitted roles | No material UI, model-quality or OIDC/JWT profile change is present; no additional specialist is required. GitHub protocol is covered by behavior/system/security and official rate-limit documentation. No language or lifecycle owner is substituted by another surface's tests. |

All selected reports are present and complete for their assigned boundaries. No missing specialist or carried-forward proof gap was found. No follow-up discovery is required: proposed findings are traceable and authority resolves the differences in suggested private mechanics. This validator found no additional blocker requiring separate validation.

## Retained ledger

### FIND-006-1 — REVISED · INCORRECT — observations can be accepted without fencing older reads

Discovery IDs: **B1, I1, DATA-2**.

**Obligation:** REQ-004/005, INV-003/006, A9/A10 and `architecture/wyrd-design.md:2233–2240`: an older observation cannot defeat the newest accepted observation or deactivation.

**Locations:** server `components/github/pull_requests.rs:207–295`; SQL `queries/verifier_executions/pull_requests.rs:55–97,329–344,383–455,493–515`.

**Independent source proof:** PostgreSQL time is read and committed before provider GET. Only afterward does admission acquire the per-PR transaction advisory lock. LATEST_SQL derives all ordering state from execution `pr_observed_at` and first `pr_deactivated_at`. Replay writes nothing; DEACTIVATE_SQL updates no row for first-ever close/draft and excludes already deactivated terminal rows, while COALESCE retains their first deactivation. Therefore:

1. Hold the first open response at t1; a close/draft response at t2 commits against zero rows and returns 202; release t1. History is empty, so t1 reserves runnable open work after the newer close/draft succeeded.
2. Admit A; hold intermediate B; restore A and accept a later replay; release B. The replay left no newer timestamp, so B wrongly supersedes A.
3. Admit/deactivate A; hold a ready/open B response; accept a newer draft decision while all old rows are terminal/deactivated; release B. The second draft leaves no newer timestamp, so B wrongly admits.

The same cause also permits an old held close to cancel a newer same-source open replay. The existing held-answer mock captures the response before waiting (`wyrd-testing/src/github.rs:1086–1099`), so these are supported deterministic response interleavings. Current tests only exercise ordering when the newer decision leaves a generation or first deactivation row.

**Consumers/siblings:** stale reservations enter existing handoff → Scribe → SubmissionSink activation → ordinary claim/settlement. Those consumers correctly trust queue state; consumer-side provider guards would duplicate source authority and leave admission/receipt incorrect. Access cancellation is a separate writer addressed by FIND-006-4. Frozen generation time is also the canonical Scribe input timestamp in `hand_off:442–443`; rewriting it to act as a watermark would change replay bytes and is rejected.

**Materiality:** ordinary first-open/close, draft/ready and force-push/replay operations can revive obsolete work or cancel current work despite an acknowledged newer decision. This directly violates explicit source/lifecycle correctness.

**Selected smallest correction:** extend the lifetime of the **existing per-PR transaction advisory lock** to cover provider PR observation and the decision. Mint the narrowed installation token before acquiring that transaction. Acquire the existing PR lock, then take PostgreSQL observation time and perform the bounded PR GET while holding that lock; keep it until the corresponding decision commits. Do not lock the installation slot or mutate/lock execution rows during the provider request. After the GET, acquire/validate the installation slot as FIND-006-4 requires and refresh history under the same PR lock before deciding. Commit before Scribe staging or AdmissionReply waits.

This serializes provider observations for the same tenant/host/repository/PR, including decisions with zero execution history. An older response cannot remain outstanding after a newer observation commits: the newer provider read waits for the older transaction to finish. Existing frozen generation timestamps, IDs, pins and canonical bytes remain immutable. Existing transaction drop rolls back/releases the lock on provider failure or cancellation. No watermark map, sentinel execution, journal, table, session-lock capability or new lifecycle owner is needed. A timestamp-only patch on execution rows is insufficient for zero-history decisions and is rejected.

**Burden/authority:** one existing lock is held across one bounded provider read. This occupies a tenant connection and serializes same-PR reads; independent PRs retain different lock keys. Minting and Scribe handoff remain outside the transaction. The existing HTTP client has a ten-second whole-request timeout; preserve the existing screening/request bounds and propagate unavailable reads. Applicable authority requires a per-PR ordering decision, and neither the approved task nor SQL rules prohibit this private bounded transaction lifetime. The discovery preference to keep every PR GET outside SQL would require additional durable zero-row coordination and is not selected. This changes no public ordering or resource-capacity contract.

**Focused closure:** prove same-PR observations serialize across the provider-read/decision boundary and that a later close/draft, replay or repeated deactivation cannot be undone after its receipt. Use existing held-read controls and explicit progress synchronization, not sleeps. Existing tests at server `pg_github_pull_requests.rs:568` and `:1237` currently await the second webhook before releasing the held first response; launch the second observation concurrently and release/join in the order appropriate to serialization so the tests cannot hang. Preserve their final generation/cancellation/lease assertions; do not retain an impossible test requirement that the second receipt precedes release of the serialized first read. Verify original frozen IDs/pins and canonical replay/partial handoff remain unchanged. No broad journey repetition is needed.

### FIND-006-2 — REVISED · MISSING — empty future policy bypasses source supersession

Discovery IDs: **B2, I2**.

**Obligation:** REQ-003 explicitly supports `verifier=[]`; REQ-005, INV-006 and A8/A12 require new source to cancel obsolete unfinished work, while selection itself performs no sweep.

**Locations:** server `components/github/pull_requests.rs:274–278,319–331`; SQL `queries/verifier_executions/pull_requests.rs:102–113,564–643`.

**Independent trace:** accept queued/running A under V1. Select V2 with no PR bindings; declaration selection correctly leaves A unchanged. Observe source B. `targets` returns None for the selected empty binding set, and admission commits/returns before the only SUPERSEDE_SQL call in `reserve_pull_request`. A remains due/running with a valid lease. Claim and settlement have no provider-source/selected-policy predicate, so A can run or settle as current. Existing reselection proof changes two bindings to one, always reaching reservation.

**Consumers/siblings:** selection settlement must retain no-sweep behavior; access-loss cancellation does not apply because connection access remains valid. Ordinary activation/claim/settlement and later consumers must inherit the queue owner's cancellation, rather than independently consult selected policy.

**Materiality:** removing future automatic review is an explicitly supported workflow; the next source must still invalidate the already accepted old review.

**Selected correction:** make the queue-owned source transition independent of replacement target count. Under the same PR decision, cancel unfinished older executions with `pull_request_superseded` and clear their leases even when no replacement bindings exist. Admit zero replacement executions. Mark the old current generation inactive for replay when no replacement generation is stored, using existing deactivation metadata, without changing its frozen pins/timestamp, settled outcomes or the supersession error code. Otherwise a return to A could incorrectly replay cancelled A as active. Reuse existing queue cancellation/supersession SQL ownership; do not sweep on declaration, fabricate an execution, introduce a watermark store, or add worker guards. FIND-006-1's observation serialization handles zero-row/no-target ordering without separate persistent source state.

**Burden/proof:** a local source-transition correction is necessary; deletion of the empty-policy feature would violate approval. Accept/claim A with completed history P, select an empty policy and assert rows initially unchanged, observe B and assert A cancelled, late completion StaleLease, P/pins unchanged and no B rows. A later legitimate re-enable/reopen must not reuse an inactive cancelled generation. Existing SQL/server focused lanes suffice.

### FIND-006-3 — CONFIRMED · INCORRECT — forbidden provider reads become successful no-work receipts

Discovery IDs: **B3, SYS-006-1**.

**Obligation:** A5/INV-003 require failed GitHub reads to remain retryable; A7 recovery must not exclude required work through false success.

**Locations:** `github/app.rs:439–463`; `github/pull_requests.rs:224–230,493–522`; `github/service.rs:657–668`.

**Independent trace:** a valid installation token reaches PR GET. App maps every 403 and 404 to None. Admission returns no work; webhook answers 202 without a reservation, refusal or source/cancellation decision. Delivery recovery skips every GUID with any 2xx attempt. Thus rate-limited reads can permanently lose a required review or deactivation. GitHub's official [rate-limit documentation](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api), independently checked, explicitly identifies 403/429 for both primary and secondary limits. Existing failed-read fixture emits 502 only.

**Selected correction/burden:** remove 403 from the absence classification and pass it through existing decode/unavailable handling. Preserve the existing separate 404 behavior. Do not add a retry engine, rate-limit parser or new refusal policy; a failed receipt lets the existing provider-log recovery/redelivery mechanism retry. The positive receipt guard belongs at the provider-read/admission boundary, before any queue consumer exists.

**Focused closure:** mock one rate-limited 403 PR GET; assert non-2xx/no new fanout, then successful redelivery after recovery creates exactly one expected frozen fanout. The same reader serves close/draft and interrupted recovery; no separate downstream fix is needed.

### FIND-006-4 — CONFIRMED · INCORRECT — in-flight reads admit after connection access loss

Discovery IDs: **B4, SEC-006-1, DATA-1**.

**Obligation:** REQ-001/004, INV-001/006, A1/A10/A12 require current connection access and fencing before release/removal/suspension completes.

**Locations:** server `github/pull_requests.rs:213–295`; `github/service.rs:368–407,623–705,721–760,876–919`; SQL `github_installations.rs` slot/revision writers and queue `CANCEL_REPOSITORIES_SQL`.

**Independent trace:** first-ever open PR obtains a token and passes the mint-time slot revision recheck. Hold its successful PR response. Disconnect locks/releases the slot and cancels existing rows in one transaction, but there is no execution for this PR. Selection/bindings remain registered. Release response: admission locks only PR history, resolves active declaration principal and creates reservations. Scribe activation makes them due. No reservation/activation/claim step rechecks connection access. Removal and suspension advance the same slot revision and call the same cancellation owner; uninstall shares fence_and_release. Declaring principal revocation fencing is independent and cannot detect connection revocation.

**Materiality:** a supported access revocation returns success and then stale provider IO creates runnable work; a valid old token is being mistaken for current admission authority. Ordering timestamps alone cannot prove installation authority, especially after replacement/reconnect.

**Selected correction:** reuse the installation owner's exact `GithubSlotBinding` identity/revision and selected-repository/unsuspended checks at final admission. Carry the binding associated with the narrowed token/provider read, and retain the routed signed installation identity; refuse a changed/replaced association rather than adopting another installation's current slot for an old routed delivery. Lock the current slot through reservation/replay/deactivation decision and commit, checking that host/installation, binding/revision, repository selection and suspension still authorize it. Reuse the existing connection-changed retryable error for changed authority, or existing authoritative no-access outcome after confirming the revoked state; do not insert new work under uncertainty. Keep token acquisition's existing fence and its public SecretString-returning API for connection tests/callers; expose the captured binding internally without duplicating token minting or authority resolution.

With FIND-006-1 the order is **PR advisory lock → provider GET with no slot or execution-row locks → slot lock/check → fresh history and execution writes → commit → Scribe handoff**. Existing access writers take **slot → execution rows**, never the PR advisory lock; declaration takes the slot before selection writes. Admission performs no execution-row writes/locks before its slot check, so it creates no slot/execution lock cycle. Access loss during GET commits promptly and the subsequent check refuses the stale admission; access loss after reservation waits for commit then cancels rows. ACTIVATE_SQL's pending/infinity predicate cannot revive cancelled rows. No slot lock is held over provider IO, and no runner/result guard substitutes for admission authority.

**Burden/proof:** one reuse of existing slot authority at the creating transaction, with internal propagation of already available binding identity, is sufficient. No new table, permission or connection owner is warranted. Hold first open GET, commit disconnect/removal/suspension, release, assert no new claimable work; cover replacement/reconnect so delayed old work cannot transfer to a new association. Also order reservation first then access loss and assert cancellation survives later activation. Focus tests at existing shared access owner rather than duplicate every transport.

### FIND-006-5 — REVISED · MISSING — PR execution reads omit repository authorization

Discovery ID: **SEC-006-2** (unique proposal, independently validated).

**Obligation:** `architecture/wyrd-security-posture.md:254–258` explicitly requires current exact verifier:read plus repository read where applicable; design `:1957–1960` requires recorded immutable identity without source Card retrieval. A13 ships PR inspection under INV-001.

**Locations:** `components/verification/service/inspection.rs:329–376`; `components/verification/service.rs:259–305`; queue status projection supplies the newly added PR field.

**Independent trace:** a caller with exact verifier:read but no cards:read passes require_verifier and retained's exact Verifier audit check. get_execution returns the queue state, including repository ID, PR number and base/head refs/commits. The repository feature's existing standalone gate is `Permission::card_read()` in get_binding `service.rs:212–223`; verifier permission does not imply it. IDs are not authorization. All HTTP/SDK/CLI/MCP consumers share this owner.

**Sibling coverage:** retained → accepted also serves invocation/publication listing and exact publication selection/download/content paths. Fixing only get_execution would leave the same repository-backed execution inspectable through sibling consumers. The cancel path uses authorized with Action::Run and has a separate existing contract; do not broaden cancellation permissions as part of this read correction.

**Selected correction:** apply and audit the existing cards:read gate once in **retained** for PR executions, in addition to exact verifier:read, before returning any inspection state. Determine applicability from immutable queue PR origin/pins while its row exists and from the existing accepted input record's `InputOrigin::PullRequest` when the queue row is absent; use its recorded owner/subject identity for audit context. The fixed SYSTEM reader/InputResolver already owns those input records. Do not fetch source Cards, invent a repository-specific permission, modify SDKs independently, or require new durable/public metadata. If required retained applicability evidence is unavailable, propagate existing unavailability instead of treating that as an ordinary non-PR execution. Ordinary non-PR reads retain their existing permission behavior.

**Burden/proof:** shared authorization plus retained origin lookup when needed is smaller and safer than transport-specific guards, and is mandated security rather than speculative hardening. Exact verifier-only caller must be denied for PR status and a representative sibling read; adding cards:read permits typed inspection. Cover accepted PR state with no queue row using the existing retained-read fixture, and preserve ordinary non-PR verifier-only reads. Check permission decisions through existing audit behavior; no new audit subsystem.

### FIND-006-6 — CONFIRMED · REGRESSION — recovery scan can exhaust shared shutdown drain

Discovery ID: **SYS-006-2** (unique proposal, independently validated).

**Obligation:** operations deployment `:272–275` stops new admission/cancels bounded work before durable settlement/release; reliability `:177–180` requires the shared Scribe drain boundary. Task 006's auxiliary worker must preserve this lifecycle.

**Locations:** `github/pull_requests.rs:527–543`, recovery `:493–522`; registration `app/server.rs:650–663`; supervisor `app/supervise.rs:298–316`; post-supervision drains `app/server.rs:800–828,878–895`.

**Independent trace:** shutdown is polled while waiting for a tick only. Once the tick arm awaits recover_deliveries, cancellation no longer polls. Listing is up to 50 pages/5,000 entries, followed by sequential redelivery calls; failed POSTs log then continue. ScreenedHttp bounds each request to ten seconds, but four slow failures can exceed the default 35-second shared process drain. Supervisor waits its JoinSet to the common deadline. Only afterward does server drain producers and Scribe; an exhausted deadline aborts Bifrost instead of draining roles. Thus provider backlog/outage during routine replacement can prevent unrelated accepted writes reaching their existing shutdown boundary. The claim is lifecycle propagation, not a running-server availability/SLA demand.

**Selected correction:** make the in-progress recovery future selectable against shutdown inside the existing worker, following the nearby Scribe publication worker at `server.rs:586–596`. Drop the maintenance future when shutdown wins; stop initiating later provider requests. Provider delivery log remains recovery authority and already-sent POST may complete; approved duplicate scan/admission idempotency handles that uncertainty. Keep errors local. Do not elect a scanner, extend drain budget, add a queue, or crash the service on a provider failure.

**Focused closure/burden:** one local cancellation boundary and one controlled held-provider test. Hold listing or redelivery, cancel worker, assert it joins without releasing the response and starts no later redelivery; existing process drain/source checks establish the shared consumer. If using the existing server shutdown fixture, an accepted ordinary Scribe write must retain its normal flush path. No load benchmark, full journey suite or new latency objective is required.

### FIND-006-7 — CONFIRMED · VIOLATION — mandatory panic rustdoc is incomplete

Discovery ID: **REPO-006-1**.

**Obligation:** root Code explicitly says missing changed-item rustdoc blocks merge; agent_docs/rust.md includes private helpers, test helpers and tests and requires Panics for possible panics. This is an expressly mandatory gate and cannot be waived by low runtime risk or passing compiler/lints.

**Verified bounded inventory:**

- `github/app.rs:670,694`: new tests assert and expect without Panics.
- `github/pull_requests.rs:552`: event helper expects serde decode without Panics.
- server `pg_github_pull_requests.rs:171`: Deployment::base expects bound URL without Panics.
- SQL `pg_verifier_executions.rs:4457,4486`: pins expects validated IDs; pr_targets calls background, whose documented SYSTEM-provisioning expect can panic; neither caller documents it.
- Python `src/testing.rs:1052` and TS `native-testing/src/github.rs:100`: new wrappers call mock pull_request_event, whose complete body expects a scripted PR and explicitly documents that panic; wrappers omit it.
- MCP `tests/bifrost/mcp/github.rs:215`: materially changed journey has assertions and only Errors docs.

**Selected correction/burden:** document actual panic conditions on this bounded changed-item inventory and equivalent omissions in that same touched set. Preserve runtime error/validation behavior; do not add wrapper validation, suppressions or unrelated doc rewrites. Batch this with substantive remediation. Regenerate N-API declarations through their owner if comments affect generated output; authored Python stubs remain authored. Source inspection and relevant format/generated-parity checks suffice; no new runtime test for documentation.

## Rejections and proposal narrowing

| Proposal / variant | Disposition and source/threshold reason |
|---|---|
| REPO-006-2: positional production SQL tuple decoding | **REJECTED as blocking finding.** SQL AGENTS says “Map columns by name, never by position”; the two reads at queue PR :469/:501 do use ordinal tuples. Independently checked BINDINGS_SQL returns binding_id then verifier_card_uid and the tuple consumes those same positions; DEACTIVATE_SQL returns execution_id then cancelled and consumes those same positions. No current misdecode, authorization error or supported-use failure is present. The stated scenario requires a future code change to reorder a projection. The skill requires a source-proven failing scenario and meaningful present consequence, and excludes structure-only proposals without a material consequence or express mandatory merge gate. Unlike rustdoc, named SQL mapping is not declared a nonwaivable merge gate. Explicit prose alone does not replace those other threshold elements. No remediation work or FIND ID is assigned. |
| REPO-006-2 extension to Deployment::frozen | **REJECTED.** Current name/version tuple order matches the projection; future reorder risk adds no present failing assertion or mandatory gate. It is also outside SQL directory ownership, so extending the production proposal into that test helper does not strengthen materiality. |
| B1/I1/DATA-2 timestamp-only repair on execution rows | **REJECTED correction variant.** Cannot represent first close/draft with zero executions; rewriting canonical observation time also changes Scribe replay bytes. The existing lock lifetime is the selected bounded correction. |
| Keep all provider IO outside SQL at any cost | **REJECTED as added constraint.** This is a discovery recommendation, not approved PR authority. It would force extra durable coordination for zero-history state. Only one bounded PR GET is placed under the existing PR lock; token mint and slot locking remain outside/after that request. Suspension's distinct revision-based observation workflow stays unchanged. |
| New watermark table, sentinel execution, generic inbox/broker, durable per-PR map | **REJECTED correction variants.** Existing serialization fully protects the required path without extra persisted authority; no specification revision is necessary. |
| Consumer-only current-source/access guards; declaration-time sweep | **REJECTED correction variants.** They leave false admissions/receipts or violate no-sweep selection, and duplicate the existing queue/authority owner. |
| Inspect PR status only in get_execution | **REJECTED incomplete correction.** Sibling accepted/invocation/publication reads share retained and require the same current repository gate. No cancellation-permission redesign follows from this read finding. |
| Broader future parked-state/result-delivery machinery | **REJECTED scope expansion.** Present schema has pending/retrying/running as its nonterminal set and cancellation covers all three. Future continuation/provider-write consumers belong to their tasks; this remediation preserves central queue fences only. |

## Reconciled acceptance and additional obligations

PASS rows use independently checked source/assertions plus the clearly attributed execution evidence. FAIL means a retained finding defeats the obligation; documentation consequences are not extra findings.

| Obligation | Implementation and assertion evidence | Result |
|---|---|---|
| A1 signed connected selected admission/no head policy | signature before routing `service:631–667`; selected registered targets; unsigned/mis-signed/unselected tests in server :1089. Final connection fence missing. | FAIL FIND-006-4 |
| A2 immutable full source/policy/authority pins; later selection only | reserve/REFUSE_SQL freezes before handoff; lifecycle :374–450 checks full PR JSON, inline/referenced Trigger, policy and principal; reselection :1012–1085 retains old IDs and new 1.0.1 pins. | PASS |
| A3 lifecycle eligibility and metadata exclusions | observable match and Observed::of; unit eligibility assertions, base retarget and draft/ready/reopen server assertions. Ordering failures separately mapped below. | PASS |
| A4 current declaring authority/durable refusal | accountable issuer/background owner plus per-binding audit; revoked-principal server case asserts both errored with credential-revoked code, 202 and empty claim; SQL refusal case retains pins/code. | PASS |
| A5 consequential ACK/failure retry | handoff awaits replies; SubmissionSink activates/commits before replies; stalled ACK/restart and 502 tests meaningful. 403 falsely succeeds and no-row close does not fence. | FAIL FIND-006-1/3 |
| A6 canonical duplicate/partial-fanout/restart recovery | execution-keyed Scribe frame; frozen timestamp/pins; ACTIVATE predicate; server :818–838 compares every original ID and due state, SQL partial activation/replay assertions. Partial commit modeled manually rather than injected actual ACK. | PASS with stated proof limit |
| A7 bounded paginated failed-delivery recovery | App page cap/origin screening; recovery GUID grouping/2xx/three-redelivery cap; server :445–475 asserts 1/0 and bound; next-page/origin unit assertions. False 403 success prevents recovery. | FAIL FIND-006-3 |
| A8 all unfinished supersession and completed P retention | SUPERSEDE clears leases; P/A/B test retains P and late A StaleLease. Empty policy skips it. | FAIL FIND-006-2 |
| A9 latest accepted observation/deactivation wins | current timestamp/history design plus held-B/close tests cover some rows, not zero-history/replay/repeated deactivation. | FAIL FIND-006-1 |
| A10 draft/close/removal/suspend/uninstall/disconnect | shared SQL fences, existing running-close StaleLease and lifecycle codes/history assertions. First delayed reads bypass no-row deactivation/access cancellation. | FAIL FIND-006-1/4 |
| A11 41 PR bounded ingress/fair progress | existing bounded outboxes/fair claim; server :694–721 checks 82 rows, two per head, all completed and matching served heads; shared staging-full proof. | PASS |
| A12 existing queue/authority owns pins and fences | existing shared queue/settlement reused; missing source and access decisions leave runnable stale work for consumers. | FAIL FIND-006-2/4 |
| A13 first-class typed inspection/binding/guide | Rust/Python/TS journey diffs assert full refs/SHA/repository/delivery and cancellation; HTTP lifecycle, CLI journey and MCP binding→execution assertions; typed spec/TS and authored Python parity inspected. Shared repository permission is absent. | FAIL FIND-006-5 |
| A14 docs eligibility/pins/cancellation/diagnosis | guide step 5, concept/MCP guide and generated machine docs match intended contract; reported docs check. Implementation violates documented lifecycle guarantees. | FAIL consequential to FIND-006-1/2/3/4 |
| A15 design/operations amendments | Trigger inline/referenced activation, source policy, ACK/recovery and already-sent-IO limit amended; operations bounded scan documented. | PASS |
| A16 existing Scribe/queue, no new table/engine/broker, tenancy/secrets | migration adds queue columns/index only; same Scribe/SubmissionSink/runner; TenantConn/RLS and served-secret-negative assertion; fresh deps/isolation checks PASS. | PASS structural; access integrity separately FIND-006-4 |
| A17 schedule/group/submitted behavior | existing admission branches and fair runner/settlement preserved; source comparison and task-recorded 129 server verification tests/SQL suite. | PASS |
| A18 connected installation tenant routing | signature → operator lookup → TenantConn; two-tenant case :1301 checks separate row sets; final association change race remains. | FAIL FIND-006-4; ordinary RLS routing passes |
| Additional: no webhook DAG or PR-head policy; no extraction/parking/provider-write/UI work | only current PR endpoint read; queued handoff; unchanged engine; actual diff excludes later capability implementations. | PASS |
| Additional: current repository permission/audit on every PR inspection sibling | retained currently checks exact Verifier only; security authority plus shared consumer trace above. | FAIL FIND-006-5 |
| Additional: auxiliary recovery preserves shared shutdown/drain | supervised worker tick branch ignores cancellation in progress; shared deadline precedes Scribe/Bifrost drain. | FAIL FIND-006-6 |
| Additional: changed-item rustdoc including binding/test panic contracts | bounded verified inventory above; explicit nonwaivable repository gate. | FAIL FIND-006-7 |
| Additional: all language/contract/tooling obligations | spec purity, Python placement/authored stub parity, TS N-API/generated field names, thin SDKs, no new dependency/features/platform forks; named reports plus directly inspected diffs. | PASS except mandatory docs above |
| Additional: failure-fix evidence and existing owner reuse | disconnect diagnosis matches RELEASE_SQL post-update host NULL; fix uses held pre-release row in existing fence owner. No weakening/ignored test or failed check suppression found. Existing full workflow reuse independently corroborated. | PASS |

## Verification limits and final recommendation

Fresh orchestrator proof: whitespace, 4 GitHub unit tests, Python format/lints/typecheck, TS format/lints, dependency/isolation checks, both migration lanes, and all 15 focused server PR/connection tests PASS. Implementation-reported SQL, all-surface journeys, codegen/docs/examples and Rust lint evidence remains attributable to the task. This validator reran none of those suites or generators. Existing green tests do not exercise retained findings' missing interleavings/permissions/403/shutdown paths. No source/test edits, commits or broad gate were performed.

Package **FIND-006-1 through FIND-006-7** into one bounded remediation, batching panic docs with substantive fixes. Use the selected lock/authority/transition boundaries above; no new product/public API/security/persistence decision is needed, so **SPEC_REVISION_REQUIRED is not indicated**. Keep the source immutable for this review. The orchestrator writes the final verdict/remediation task.

Skipped: repeated DB/journey/generator runs and unrelated hardening. Risk: current green lanes do not establish the missing concurrency, authorization, provider-failure or shutdown guarantees.
