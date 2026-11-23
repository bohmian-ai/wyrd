---
id: TASK-012-R1-allocation-R1
kind: remediation
status: complete
spec: SPEC-github-verifier-pipelines
spec_revision: 16
parent_task: TASK-012-R1
remediates: [FIND-012-9, FIND-012-10, FIND-012-11, FIND-012-12, FIND-012-13, FIND-012-14, FIND-012-15]
requirements: [REQ-019, REQ-020, REQ-021, REQ-022, REQ-023, REQ-024, AC-015, AC-016, AC-017, AC-018]
---

# Complete allocation integration and qualify the successor

The user must be able to build the server, run gateway/verification/Bifrost under one effective resource policy, and retain substantial buffers until their real owners dispose or transfer them. Correct the validated integration gaps without replacing the accepted shared manager or restoring speculative precharges. Qualification remains incomplete until the required clean evidence exists.

## Inputs, identities and authority

[Approved spec revision16](../../spec.md), its [amendment](../../spec-revisions/16-shared-workload-management.md) and [finalized design](../../design/shared-workload-management.md); [original allocation task](../../tasks/012-R1-allocation.md); this review's [verdict](verdict.md) and [validated findings](findings-validation.md). Reviewed base `0ae00f4293f3d2dcf636bcbd88a419fe6245c6dd`, candidate `5c980841140721cfaa1945fba1057675805ca77a`. New implementation starts from that candidate plus only caller-authorized changes. Existing dirty Oracle fix and skill/workflow edits are not accepted candidate content; coordinate their ownership rather than overwriting/stashing them. No commit/push is authorized by this task.

Each finding's diagnosis, authority, exact source locations and source evidence is recorded once in findings-validation.md. The boundaries below define the correction and observable closure, not private helper/signature/fixture choices. Historical FIND-012-1–8 and precharge policy must not be reopened.

## Correction boundaries

- **FIND-012-9:** Oracle's query-start resource capability and initialization must exist in the production feature set. Keep inspection controls gated. Update the field's test-only rustdoc. Do not enable test-support in the shipped server.
- **FIND-012-10:** Connect the existing Bifrost governor's retained root custody to shared Bifrost workload usage/demand once, and supply its already-resolved smaller local cap. At fresh external Scribe's pre-ACK boundary, apply shared pressure with existing capacity/deadline outcomes. Existing governor reservations, pools, class/tenant policy, infallible accepted growth and bounded persistence/cleanup remain authoritative. Do not pressure-gate every governor resize or persistence CPU job.
- **FIND-012-11:** Reuse CpuLanes/context and the existing DataFusion spawn tracer at substantial gateway buffered/translated-stream CPU phases, authenticated remote Scribe fragment/analytical stage local entry and polling/spawned work, and checkpoint Writer preparation. Preserve existing owner pools/queues, tenant authority, deadlines and cancellation. Checkpoint preparation is mandatory drain and remains eligible under memory pressure. Acquire only for CPU work, never whole provider/query lifetimes or IO/admission/outbox/ACK/SQL waits. No new scheduler, queue, or unrelated result/sandbox conversion work is requested.
- **FIND-012-12:** Preserve the existing same-ledger charge across successful StreamCapture/MediaCapture finish, StreamEnd, facts, retained media objects/storage and existing outbox projection/takeover. Disposal/refusal/failure releases only when the real content owner is gone. Preserve redaction, tenant paths, capture shedding and answered-call independence; no future-copy estimates.
- **FIND-012-13:** Attach buffered response custody to the allocation handed to the consumer, using the existing owner-backed Bytes pattern where suitable. Last consumer reference, including clones/slices, controls release; account available owning JSON capacity. Preserve zero-copy media where possible and current Content-Length, payload/refusal and wire behavior. Fix the existing test expectation that treats retained collected Bytes as freed.
- **FIND-012-14:** Reconcile only affected resource policy paragraphs in architecture/wyrd-design.md2351–2381 and architecture/bifrost-design.md1371–1373 with the already-approved amendment. Correct conclude's obsolete charge/panic contract and bounded new sandbox Waiting::drop / worker READS / DelayedSink field/write/flush rustdoc omissions. Document materially changed items. Do not change architecture to legitimize any implementation defect or expand the approved policy.
- **FIND-012-15:** Add final immutable-successor functional results and required full clean L1 records/comparisons to original scenario7. Missing evidence alone demands no production change. The current user prohibition on running benchmarks persists; record qualification as pending until scheduling is authorized. Do not mark scenario7 or the task complete while qualification is missing.

## Observable closure scenarios

Run new nontrivial behavior through genuine test-first cycles; confirm exact new selectors before expected missing-behavior RED. Compilation/setup failure is not behavioral RED. Preserve existing TDD violations honestly; no retroactive repair.

| Finding / existing contract | Starting state, action and required observation | Narrow credible owner and runner |
|---|---|---|
| 9 / REQ-024; scenario7 | Build ordinary server without test-support: Oracle compiles, and existing query admission remains wired | Existing shipped-server clippy invocation below; no new runtime harness |
| 10 / REQ-019/021/023/024; scenarios3/6 | Allocate real governed Bifrost memory, contend with another workload: Bifrost usage counts once and remains demand until release; above-target work pauses and a smaller local cap redistributes its unspendable share | Existing redux resource/Scribe integration owners; managed Postgres only when actual dependency needed |
| 10 / AC-016/018 | With critical allocator pressure and root/local byte headroom, submit a fresh public append: it pauses/refuses under normal capacity/deadline semantics; previously accepted bounded persistence still drains | Existing Scribe integration plus focused public SDK/WyrdTestServer append journey; use existing journey runner with exact new selector |
| 11 / REQ-019/022/023; scenarios4/5 | Hold all shared lanes, trigger real substantial gateway buffered translation/decode and independently spawned stream translation: CPU phase waits; release permits progress with initiating tenant/workload. A held provider/downstream wait leaves a lane available | Existing gateway adapter/server integration owners; focused native journey for public wire behavior |
| 11 / AC-017 | Hold lanes on the actual remote follower process, issue authenticated fragment and analytical-stage work: local entry/polls and spawned CPU tasks wait; release yields correct typed output/identity; cancellation and peer IO do not leak/hold leases | Existing remote Oracle/Scribe follower/distributed integration and journey fixtures, not leader-only or raw tracer mocks |
| 11 / REQ-022/023 | Hold all lanes, dirty a substantial accepted checkpoint: preparation waits, then produces the correct single prepared row after release. Coalescing/charge and critical-pressure delivery remain correct; persistence waits hold no lane | Existing verification/state inline or server integration owner, depending required dependency; retain existing checkpoint/drain assertions |
| 12 / REQ-019/023; scenarios1/6 | Successfully finish JSON/media capture and retain its output: existing debit persists. Hold real storage while media capture persists, then permit success/failure: no free-live-content gap or double outbox debit, final disposal releases | Existing adapter owner tests and connected gateway capture integration/storage fixture |
| 13 / REQ-019/023; scenarios1/6 | Consume a buffered JSON/media body, drop body but retain outgoing Bytes, then clone/slice/drop: one allocation debit persists through last reference. JSON spare capacity is included | Existing response owner/body and pg_invocation_tests; update zero-after-collection assertion; no new test binary |
| 14 / explicit reconciliation and rustdoc rules | Read bounded owner paragraphs and changed-item docs against amendment/source: actual consumption, shared CPU and handoff/panic rules agree | Content/source inspection and existing format checks; no invented runtime test |
| 15 / REQ-024; AC-018; scenario7 | Bind affected functional outcomes and each named regression to successor; later supply VALID clean default L1 results versus compatible history, including correctness, throughput/tails/CPU/memory/waits/churn | Existing focused runners and prescribed L1 commands; no substitutes or invented tolerance |

New test names and exact commands belong to implementation evidence, not guesses in this task. Focused journey tests close changed behavior; unaffected full journey/language suites remain integrated change-review work. Existing original-task broader lanes/results may be reused only with valid identity/feature/dependency provenance.

## Checks and evidence

Read mise tasks and TESTING.md. Coordinate Cargo sequentially; do not overlap another session's build target. The structural build closure is:

```bash
mise exec -- cargo clippy --locked -p wyrd-server --bin wyrd-server -- -D warnings
```

Run each new named test exactly through its owning nextest target/feature set, with the repository Postgres wrapper where needed. Existing owner lanes are `test:verification:sandbox`, `test:bifrost:integration:redux`, `test:bifrost:integration:server`, `test:gateway:native`, and focused `test:bifrost:journey:{server,oracle,scribe,forge}` owners. Prefer selectors for this remediation's changed behavior; do not demand unrelated full suites solely because they were listed historically.

Retain original scenario7's three specifically prescribed regressions, recording per-test selection/results:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_verification_runtime::deadline_terminal_waits_for_started_host_work) | test(=pg_verification_runtime::held_persistence_bounds_queued_custody_by_claim_permits) | test(=pg_verification_runtime::two_replicas_claim_each_queued_run_exactly_once)'"
mise run fmt:check
mise run lints
git diff --check
```

Record the successor identity, actual assertions/results, exact selectors, feature sets and unresolved proof in the original task scenarios. Existing dirty-worktree successes require reconciliation, not blind carry-forward. Audit every added/moved PostgreSQL/Scribe/object-store/Vault wait against the sanctioned CPU-free wait boundaries. No gate/full aggregate unless actual integrated scope requires it.

Only when benchmark scheduling is authorized, the existing qualification commands remain:

```bash
mise run bench:l1:verification
mise run bench:l1:gateway
mise run bench:l1:bifrost
```

Require complete clean default coverage, VALID records for verification/gateway/all Bifrost experiments, compatible newest same-method host/history comparisons, correctness/reconciliation and no regression. Smoke/steps/profile/allow-dirty runs cannot qualify. Do not commit to manufacture a clean tree. TASK013's mixed L2 and 100-tenant fairness remain excluded.

## Preserved behavior and handoff

Keep accepted manager-core fairness/hysteresis/attribution/RAII and approved spill-poll exception; live worker independent/coalesced telemetry, stale retention and confirmed-exit release; local payload/heap/query/concurrency/provider/commercial limits; nested initiating identity and permission checks; Busy deferral, deadlines, tool spend, host settlement, claim fencing, evidence acceptance, ACK order, audit nonblocking and scratch confirmed-deletion semantics.

No new public API/schema/migration/dependency/runtime design, OS isolation, exact RSS accounting, speculative envelope/multiplier, durable resource queue, parallel scheduling system or unrelated refactor is requested. Return implementation/evidence to the caller without launching review. The next maintainer-initiated review checks FIND-012-9–15 closure and directly affected seams against this candidate. PASS remains unavailable while required qualification is missing; historical TDD remains separately recorded.
