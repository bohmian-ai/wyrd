# Focused source-authority follow-up

**RESOLVED** — the referred paths are reachable, with a bounded correction at the existing source-selection boundary. This is discovery evidence, not a vote, finding ledger or task verdict.

Subject: cumulative `ef3a57b0776dd7bc451273b9176498fcca700550` → `9f9f7743311aec0f2191f775e7bf054aca00ff91`; HEAD matched. Authority: approved revision 15, original TASK-001, AGENTS/agent rules, task-review skill and applicable verification/doctrine and Bifrost acknowledgement/query-cut contracts. CodeGraph preceded source navigation. Compared current discovery reports and relevant prior validation/closure claims. Scope is only their referred uncertainty, not a replacement cumulative audit. Owner decisions concerning uncapped identities, payload ceiling, one membership/value statement, PostgreSQL time and newest64/no cursor remain binding. TASK-002/003 and REQ-015 remain excluded.

## Producer-to-consumer trace

- `components/verification/service.rs:554–647` and `verification/results.rs:293–354`: enqueue stages input with `queue_execution=true`, before PostgreSQL acceptance. Direct staging (`service.rs:653–690`) uses inputs(..., None), hence marker false. Identical keyed replay repeats staging with a fresh recorded time.
- `scribe_outbox.rs:811–845,881–929` and `verification/submissions.rs:84–95`: encoded row content determines batch identity; Scribe submission/ACK precedes staging SubmissionOutbox. Its independent transaction later accepts/commits. A retained input can precede, or outlive loss of, its queue handoff.
- `wyrd-sql/src/queries/verifier_executions.rs:1238–1380`: acceptance decides replay/conflict under the existing key lock. Refused/conflicting attempts get an errored, never-claimed row. Accepted pending work does not require terminal readiness.
- `verification/runner.rs:474–565,806–845`: runner requires committed acceptance/claim, then stages its new result identity before completion commit. Failure/lease recovery can leave abandoned visible results. Submitted results have null binding IDs, like direct results.
- `tables/verification/{execution_inputs,results}.rs` and `verification/results.rs:248–289`: inputs have an origin discriminator; results do not. Direct result-only visibility is expressly allowed by REQ-002.
- `verification/groups.rs:141–155,430–480,512–547,600–650`: SQL classification precedes the independent Bifrost read; unknown input/result executions are treated as direct. Decode retains physical times, while payload/list/member selection deduplicates. Ended consumes all physical times.
- `verifier_executions.rs:250–257,989–998,1693–1727`: GroupSubmissions classifies committed rows/selected results. Coverage compares newest time and unique member count; immutable snapshots cannot be retracted by later refusal.

SDK/HTTP/MCP enqueue callers converge on these owners. Direct verify publishes its own result; submitted/scheduled/grouped work uses the same runner/queue settlement. Drift, SDKs, evaluators and Operator dispatch cannot repair source authority downstream.

## Resolved uncertainties and narrowed proposals

**INV-R4-01, replay activity:** this is explicit behavior, not clock/performance speculation. REQ-001 excludes duplicate physical rows as extra executions, REQ-004 requires identity deduplication, and J-001 (`spec.md:730–731`) expressly requires “no extra list element or idle refresh.” A keyed replay reuses execution ID but freshly encodes recorded_at, so the batch fence does not collapse the new producer invocation. Its managed ingestion time is later. Ended advances through although members/values are unchanged: idle refreshes before review; after settlement covers can fail and cause another review within finality. Replay-before-first-review and identical-timestamp duplicate tests do not disprove this.

Normalize eligible logical activity in Sources/Ended before first/newest/idle/finality/coverage, reusing earliest visible input-copy and authoritative-result-copy selection. After authority selects at most one result per execution, existing surface/execution identity can unify its copies. A newly selected result remains activity distinct from its input. Preserve managed timestamps and accepted visibility/retention limits; add no historical first-seen inventory.

**BEH-R4-001 / INV-R4-02 / DATA-R4-01, pre-acceptance input:** ACK→SubmissionOutbox is a real production interval, including key-lock waits/retries. Sources discards the existing marker. A max_age-due group can pin conflicting evidence before acceptance refuses it; later removal cannot retract that judgment and decreasing through/members need not cause repair. It can also falsely overflow N valid sources at ceiling N. Current refusal seams wait for queue status.

In the existing single statement, direct inputs remain immediately eligible; marked queued inputs require a known non-refused accepted identity from GroupSubmissions. Apply before count/payload gate and logical activity. Accepted pending/running/retrying sources remain eligible with null outcomes. No terminal wait or pending-turn tracking is needed.

**SYS-R4-001, newly queued result:** absence in the earlier map is not direct provenance. Poll reads no X; independent submission accepts X, runner claims it and stages R; Bifrost returns R before, or despite failure of, completion commit. Old NOT queued admits R as direct. A max_age-due group can pin it. This needs no unusually fast source execution: the analytical read can be delayed while ordinary queue/runner work progresses. Existing visible-before-selection proof creates X before polling and covers only known-X.

Input marker alone does not close this result case. Requiring accompanying direct input deletes approved result-only evidence. Null binding inference is invalid. A result-origin field would add retained schema/producer contract; that persistent-contract choice is unnecessary and is not prescribed as pre-approved.

## Smallest common existing-owner boundary

Input/result proposals share one cause: **absence in an earlier queue observation is treated as positive direct origin**. Reuse GroupPoller/Sources and VerifierExecutionQueue. Besides the input predicate above, obtain a subsequent group_submissions observation after the complete Bifrost read and before Ended/coverage/enqueue. If a returned result's execution was unknown initially but is queued/refused subsequently, invalidate that whole group read and defer through the existing next-poll path. Do not postfilter while retaining the previously computed count/overflow/payload gate. Check relevant returned identities; unrelated queue arrivals need not invalidate safe evidence.

Every queued result producer requires committed acceptance/claim before staging. Thus any queued result returned by the completed Bifrost query has a queue row available to the subsequent fresh SQL observation. Acceptance after that observation cannot have produced a result in the already-completed query. Known queued/unselected results were excluded initially; later selection remains discoverable next tick. Direct result-only rows remain eligible. The next successful Bifrost statement still computes membership/count/value together with updated authority.

Use the same caller-owned TenantConn. `tenant_conn.rs:40–53,103–106` opens plain BEGIN; no repository isolation override was found. Normal PostgreSQL READ COMMITTED gives a fresh statement snapshot for the later group_submissions read. No second connection or global setting is warranted. An exploratory extra-connection alternative is explicitly rejected as unnecessary resource expansion. Preserve scheduler transaction ownership, source acceptance/settlement and Scribe staging order/durability; add no locks, producer IO, persisted origin, inventory or readiness state.

This guard belongs at the actual cross-store source-authority boundary, not the evaluator. It does not restore the deleted separate-payload completeness guard: whole stale authority reads defer, while every accepted read retains one-cut membership/values. Replay is a distinct adjacent normalization defect: eligible authority first, canonical identity activity second. Both fit one source-selection correction without an expensive-to-reverse decision. Independent validation decides consolidation and final IDs.

## Focused proof and limits

Use existing PG group/query/submission seams: review then replay the same keyed source later, proving unchanged activity/no idle refresh/no second review while a new execution remains activity; expose marked input before acceptance beside N valid inputs at ceiling N and prove no contribution/overflow, then commit refusal, separately proving pending acceptance and immediate direct input; classify first, then accept/claim X and expose unselected R before the analytical read completes, proving whole-read deferral, later known-X exclusion until selection, then one correct snapshot from retained selected R. Preserve direct result-only evidence. No new production fault endpoint, dependency, outage, synthetic load or broad language rerun is needed.

Root reports five units, four PG seams, seven Rust, five Python, five TypeScript grouped stories, eleven MCP and two query-lifecycle cases plus applicable checks PASS. These are root results, not this reviewer's executions, and do not exercise the disputed interleavings. All counterexamples here are source-only.

Skipped execution and unrelated re-auditing as assigned. Residual risk for validation is immutable judgment from misclassified or replay-refreshed evidence; no unresolved product decision is needed by the narrowed proposal.
