---
id: TASK-014
status: proposed
approved_spec_revision: 13
requirements: [REQ-015, REQ-017, REQ-018]
acceptance: [AC-010, AC-012, AC-013]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007]
depends_on: [TASK-012, TASK-013]
---

# Approach 20,000–30,000 direct verifications per second with Rust round trips under 2–3 ms

On this host (Ryzen 9 9950X, 16 cores / 32 threads, 91 GiB), the canonical
Verifier approaches 20,000–30,000 verifications per second with a client round
trip under 2–3 ms for Rust SDK clients. Flooding queued review work cannot raise
direct latency or reduce direct throughput. Keep TASK-012's dynamic pipeline
behavior, evidence, authority and recovery. The implementer independently
analyzes, researches and optimizes toward this outcome.

## Inputs, authority and measurable outcome

Execute after TASK-013. TASK-012 supplies the functionally verified composed B
tree, shared ledger, separated queued execution boundary, preserved pipeline
journeys, three-language benchmark/instrumentation, conflict dispositions, open
inherited obligation ledger and immutable full L1 baseline records. TASK-013
supplies the mixed-run record on that composed tree as part of this baseline. Do
not optimize A or an unintegrated B, and do not wait for A's old qualification
to pass. Pipeline revision 12 supplies integration authority over code-mode revision 9;
the performance outcome and 100-tenant amendment are settled maintainer directions
for this task. They supersede incompatible inherited mechanics, not the preserved
behavior/contracts. Reconcile affected contract text before implementation; do
not reactivate superseded tasks.

Historical artifacts under
`/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode/`:

- `target/bottleneck-rank.md`.
- `target/bottleneck-rank-drill.md`.
- `target/benchmarks/verification/direct-verify/20261115T132545Z-8482d4b2a.json`
  and its profiles: a VALID profiled diagnostic, not a qualification record or
  TASK-012's baseline.
- A's `benchmarks/README.md` Verification section.

Preserve A's ported `verification/workload.rs::CANONICAL` fixture as the
acceptance workload: one Assert, two independent Agent checks, four governed
model exchanges and two real code operations per verification. Reproduce on
TASK-013's exact candidate (TASK-012 plus the benchmark additions) before
production changes. Use TASK-012's records for the original composed baseline
and TASK-013's record for the mixed comparison. Old profiles are optional
historical leads, not established causes, acceptance inputs or selected fixes.
If a local diagnostic is unavailable, record that and diagnose the current tree.
All measurement uses the one Verification benchmark harness and its standard
records, profiles and `capacity report` summary tables. Add new scenarios as
declared steps there, never a separate harness, collector or record format. Keep
the canonical workload, model behavior, instrumentation mode and resource
settings fixed within comparisons. Do not cap or lower direct concurrency, queue
direct traffic, or shed direct work to stay below the degradation point.

Measure the goal on the canonical workload: approaching 20,000–30,000
verifications per second with Rust SDK client round trips under 2–3 ms on this
host. Report achieved Rust throughput and the existing full public-call
round-trip latency (p50/p99) together in each repetition across
c1/c4/c16/c32/c64 against TASK-012's baseline; c4/c16/c32 use the existing
selected diagnostic steps. The throughput/round-trip goal is Rust-only; Python
and TypeScript retain their existing canonical direct qualification, not new
mixed workloads or this Rust latency objective. Report distance to the stated
goal without inventing a percentage definition of “approaches” or a new
percentile threshold. Record misses honestly; reduced achieved concurrency is
not success.

| Measure | Required observation on the canonical workload |
|---|---|
| Worker and total CPU | Report live plus reaped worker CPU separately and parent+worker CPU per completion on the same windows. Keep parent and worker scopes visible rather than hiding moved work. These are measurements, not new CPU budgets or a prescribed attribution. Retain historical parent-only scope rather than rewriting old records. |
| Server overhead at c64 | REQ-018's existing target: **p99 strictly below 500 µs**, in each SDK/repetition, using the existing elapsed-server-time minus union-of-blocks definition, never subtracting percentiles or changing span/envelope boundaries. CPU time and elapsed overhead are separate measures. |
| Queued isolation | Use TASK-013's mixed run and its same-pace direct-only control. On the same single `combined` server configuration and with c64 Rust callers at the declared direct pace (record actual server concurrency), direct throughput and latency remain unchanged within measured repeat-to-repeat variability while queued work saturates the same server. Establish variability from paired repeated direct-only controls before evaluating interference; no invented fixed regression allowance. Direct progress must not depend on queued load subsiding. |

Apply the existing qualification integrity/sample rules to every repetition; do
not average away a failing SDK, window, target or wrong verdict. Retain
code-mode REQ-032's frozen calibration, total-CPU throughput ceiling derivation,
capacity-waiting test and required efficiency observations. For isolation,
record and freeze the variability estimator and paired comparison pass/fail rule
from direct-only controls before candidate/interference results are inspected.
Insufficient precision leaves isolation unproven; it does not authorize a wider
tolerance after observing a slowdown.

## Execution owners and target design

The current workflow uses B's `vala-verify` engine and `walk.rs::Tasks`, hosted
server blocks/sandbox/workspace, gateway/provider clients, shared runtime
resource ledger and background recording. SDKs remain thin public clients; no
durable behavior moves to them. These paths locate the current implementation;
they do not restrict redesign. Any execution design is allowed, including new or
replaced schedulers, runtimes, allocators, transports, storage, provider paths,
tracing and serialization, provided direct and queued verification keep the same
pipeline semantics, evidence and contracts (no separate fast path with different
behavior).

First rewrite the in-revision target design in `architecture/wyrd-design.md`
(Verifier direct execution and Agents, tools, and workspaces) and
`architecture/bifrost-design.md` (Admission and memory / Resource and failure
invariants), consistent with the supplied maintainer decisions and the
shared-ledger guarantees. Its implementation and ownership may be redesigned.
State ownership and the required execution/resource flow before matching code to
it; record numeric results as evidence, not architecture claims. Preserve B's
normative orchestration, queue/recovery/GitHub/Repository/security contracts and
A's `call_tool` contract and ledger guarantees.

Any scheduler change must preserve keyed cancellation, panic conversion,
settlement order, readiness/speculation, fanout/repeat identity, trace context,
deadline propagation and host-work settlement. Do not remove required tracing,
records, audits or diagnostic instrumentation to win a number.

Finish the capacity design for high-throughput, multi-tenant, many-replica
`wyrd-server` under revision 13's deployment targets: `combined` shares one
hosted capacity owner charged per use with no reservation, and direct work stays
unaffected by queued saturation on the same server. Never reintroduce
an idle queued reservation or a borrowing scheme. The one process
memory/scratch ledger charges direct and queued work, retained/suspended work,
queued snapshots, recording buffers and Bifrost without double promises.
Preserve tenant-fair progress, bounded nested-call progress and safe per-replica
host/filesystem assignments. Never introduce forbidden OS controls, credential
switching, `/proc` or platform-specific execution. If a portable enforcement
mechanism cannot meet the agreed boundary, report the concrete blocker rather
than describing a weaker guarantee as implemented.

No numeric CPU budget, bottleneck attribution, scheduler choice or sequence of
fixes is prescribed. The implementer chooses the redesign and experiment sizes.
Fixed constraints are behavior/contracts, tenancy, security, portable platform
rules, queued isolation and no capping, queueing or shedding of direct work.

## Measurement and implementation evidence

Record exact candidate identities and matched baseline/candidate observations
for throughput, client latency, server overhead, CPU, memory and achieved
concurrency. Explain the implemented change and its observed result; keep
functional and integrity proof connected to that candidate. The existing
profiles guide independent investigation without selecting a cause in advance.

Keep profiling separate from qualification. Use existing `bench:l1:verification`
selected-step/profile diagnostics; record actual supported arguments before
running, including c4/c16/c32 and reduced-workload diagnostics. No synthetic
host load, new concurrency cap or weakened benchmark criterion. Report all new
or moved hot-path waits against their sanctioned contract; warm direct execution
remains state-free and does not acquire SQL/persistence waits. Final
reproducible clean candidates and commits are supplied by the maintainer; dirty
diagnostics cannot qualify them.

## Obligation coverage, connected proof and handoff

| Obligation source | Exercise and expected observation |
|---|---|
| Maintainer direct-throughput outcome; REQ-018/AC-013; code-mode benchmark/resource obligations | Run the unchanged canonical Verifier through real public Rust/Python/TypeScript `observe().verify` against the composed server and repository Postgres. Rust results show progress toward the stated throughput/round-trip outcome together on the same workload and windows; report the full sweep and misses. All SDK/repetitions retain the inherited integrity/sample rules and meet REQ-018's c64 p99 server-overhead target with correct verdicts, no healthy-load refusal/lost write, full phase/envelope populations and unchanged exchange/code/workspace counts. Achieved server concurrency, paced arrival lateness/GC and sample counts demonstrate comparable offered work. |
| Engine invariants and code-mode settlement guarantees | If scheduling changes, unit/in-process tests at the chosen execution owners prove keyed cancellation, panic conversion, settlement/drain and ready-dependency behavior without timing sleeps; performance improvement is measured by the harness. Server and tier-1 pipeline journeys then prove the actual scheduler still drives fanout/repeat/speculation, nested calls and cancellation through the same evidence and settlement flow. |
| REQ-015/017, AC-010/012, INV-001–007; code-mode AC-028–034 | TASK-013's mixed run and paired same-pace direct-only controls prove isolation while exercising real queued canonical engine/model/Python work, not only held permits. The canonical fixture declares only `code.run`; prove nested-tool isolation separately through the combined code-mode journeys retained by TASK-012, without changing that fixture. On one `combined` server, direct is unaffected by queued saturation and queued holds no idle reservation. Held persistence, cancellation and park/takeover retain bounded charged custody, tool spend and original deadlines without recomputation of accepted predecessors. |
| Inherited TASK-003-R3 V3–V7, TASK-003/R1 and code-mode AC-025/028–034 | Carry TASK-012's row-level ledger to actual PASS/FAIL/UNVERIFIED evidence: full six-population c1/c64 qualification plus matched-rate steps, artifact-level CPU/native/Node/Python diagnosis, legacy report rendering, production feature/package exclusion and inherited cleanup/docs/ledger obligations. Preserve the managed-Python volume exception; it does not remove retention/peak evidence. No pending required row permits closing TASK-003/R3. |
| TASK-003-R3 V6 and maintainer HA/multi-tenant direction | Sustained saturation and recovery with **100 active authenticated tenants** (the maintainer's amendment replaces inherited REQ-033/AC-034 and R3 V6's 200-tenant wording), a continuously busy tenant, suspended code, direct/queued and Bifrost-only/changing mixed demand shows eligible progress without starvation, bounded memory/scratch/descriptors/queues, repeated worker reuse and complete drain. Repeat on **two real server processes** sharing repository Postgres/storage, with recorded routing and disjoint co-located memory/filesystem assignments; saturate one while the other has headroom. Each replica progresses and recovers. No cluster-wide quota, global fairness or linear-speedup promise is added. |

The capacity experiments record offered/accepted/completed/refused/cancelled
counts, per-tenant eligible progress, admission/grant/worker time series,
parent/child CPU/memory, scratch/descriptors and queued custody from baseline
through sustained saturation, load removal and recovery. Choose durations long
enough to distinguish plateau from accumulation and cover repeated reset/reuse;
record their rationale. Charges/queues/scratch/descriptors return to valid idle
baseline, memory plateaus with runtime retention explained, and no backlog
remains. Reconcile expected overload 429/503 separately from L1 zero errors.
Extend declared Verification steps in the same harness for missing 100-tenant,
changing-demand or two-replica coverage, reusing its server-process
provisioning, records/profiles/reporting. Record new step names and supported
invocations after declaring them; no tenant/replica flag is assumed to exist.
Existing L2 Verification/Bifrost experiments may supply part of this proof;
record actual commands for the chosen experiments. Their default scenarios do
not by themselves prove 100 tenants/two replicas. B's older TASK-010-R1 L2
exclusion does not cancel A's open obligation.

Use the existing engine library/integration targets and server `integration`
target, plus SDK/MCP/HTTP/CLI journey owners retained by TASK-012. Add
meaningful focused scheduler/isolation regressions there; record actual
names/commands before RED rather than inventing selectors here. The existing
exact direct-state regression is:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_durable_verifier_work::only_queued_verification_keeps_run_state)'
```

Use `mise run test:verification:sandbox`, affected `test:vala` / `test:wyrd`
owner slices and the TASK-012 connected journey runners with managed Postgres.
Run `mise run fmt`, `mise run lints`, `mise run check:deps`, `mise run
check:tenant-isolation`; use codegen, language format/lint/type and docs checks
only for affected projections/docs. Integrated change review owns full
shipped-surface coverage and any required cross-cutting aggregate.

Final performance evidence is full clean quiet-host, unprofiled `mise run
bench:l1:verification`, compared to TASK-012 and the newest compatible same-host
record; also full `mise run bench:l1:gateway` and `mise run bench:l1:bifrost`
for the shared runtime/resource effects, without regression or changing the
fixture/method to obtain a pass. Keep validity, measured scope and target
attainment separate. Full qualification requires every SDK/repetition's sample
minimum and zero errors; smoke/profile/subset or dirty runs remain diagnostics.
Preserve raw records, profiles and exact invocations, and use the existing
report renderer for current and legacy records without relabeling historical
parent-only CPU.

Outputs are the optimized composed execution path, reconciled target
architecture, isolated-capacity proof, matched candidate comparisons, final
qualification and an honestly closed or still-open inherited obligation ledger.
Do not declare either packet complete from a VALID record alone. No unrelated
product behavior, nested publication identity, payload-sizing packet, durable
direct state or direct-concurrency limiting belongs here.
