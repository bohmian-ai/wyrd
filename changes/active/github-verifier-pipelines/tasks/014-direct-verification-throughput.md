---
id: TASK-014
status: proposed
approved_spec_revision: 16
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

TASK-013 now follows [TASK-012-R1-allocation](012-R1-allocation.md). Its candidate
and L1/L2 evidence therefore include that task's reviewed workload manager and
consumption accounting. Preserve those selected semantics during optimization;
do not restore speculative envelopes or use pre-allocation records as the new
implementation's qualification evidence. Older compatible baselines remain
comparison inputs. This changes the inherited runtime, not this task's targets.

Execute after TASK-013. TASK-012 supplies the functionally verified composed B
tree, shared ledger, separated queued execution boundary, preserved pipeline
journeys, three-language benchmark/instrumentation, conflict dispositions, open
inherited obligation ledger and the existing benchmark record owners. TASK-013
supplies the full L2 `competing`/direct-only-control record on that composed
tree, identifies the existing L1 `direct-verify` baseline, and supplies the
100-tenant fairness/drain/ledger regression. Do not optimize A or an
unintegrated B, and do not wait for A's old qualification to pass. Pipeline revision 15 supplies current authority over code-mode revision 9;
the performance outcome and the revised proof split in
[code-mode port](../design/code-mode-port.md#isolation-proof-and-throughput-qualification)
are settled maintainer directions for this task. They supersede incompatible
inherited mechanics, not the preserved behavior/contracts. Reconcile affected contract text before implementation; do
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
production changes. Use TASK-013's identified existing L1 and full L2 records
for matched direct-only and competing comparisons. Old profiles are optional
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
c1/c4/c16/c32/c64 against the identified L1 baseline; c4/c16/c32 use the existing
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
| Queued isolation | Use TASK-013's L2 `competing` run and its same-pace direct-only control. On the same single `combined` server configuration and at the declared open-loop direct rate (record achieved server concurrency), direct throughput and latency remain unchanged within measured repeat-to-repeat variability while queued work saturates the same server. Establish variability from the control's repetitions within TASK-013's 15–20-minute run, adding repeated full runs if needed, before evaluating interference; no invented fixed regression allowance. Direct progress must not depend on queued load subsiding. |

Apply the existing qualification integrity/sample rules to every repetition; do
not average away a failing SDK, window, target or wrong verdict. Retain
code-mode REQ-032's frozen calibration, total-CPU throughput ceiling derivation,
capacity-waiting test and required efficiency observations. For isolation,
record and freeze the variability estimator and paired comparison pass/fail rule
from direct-only controls before candidate/interference results are inspected.
Use the control's existing repetitions within each 15–20-minute full run and
repeat full runs if more precision is needed; do not lengthen a run or add
within-run repetitions. Insufficient precision leaves isolation unproven; it
does not authorize a wider tolerance after observing a slowdown.

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
`wyrd-server` under revision 15's deployment targets: `combined` shares one
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
| REQ-015/017, AC-010/012, INV-001–007; code-mode AC-028–034 | TASK-013's L2 `competing` run and paired same-pace direct-only controls prove isolation while exercising real queued canonical engine/model/Python work, not only held permits. The canonical fixture declares only `code.run`; prove nested-tool isolation separately through the combined code-mode journeys retained by TASK-012, without changing that fixture. On one `combined` server, direct is unaffected by queued saturation and queued holds no idle reservation. Held persistence, cancellation and park/takeover retain bounded charged custody, tool spend and original deadlines without recomputation of accepted predecessors. |
| Inherited TASK-003-R3 V3–V7, TASK-003/R1 and code-mode AC-025/028–034 | Carry TASK-012's row-level ledger to actual PASS/FAIL/UNVERIFIED evidence: full six-population c1/c64 qualification plus matched-rate steps, artifact-level CPU/native/Node/Python diagnosis, legacy report rendering, production feature/package exclusion and inherited cleanup/docs/ledger obligations. Preserve the managed-Python volume exception; it does not remove retention/peak evidence. No pending required row permits closing TASK-003/R3. |
| Inherited R3-SUSTAIN / REQ-033 / AC-034 and R3-REPLICA proof, as corrected in the port record | Consume TASK-013's deterministic 100-tenant fairness, completion, drain and return-to-starting-ledger evidence. Existing `two_replicas_claim_each_queued_run_exactly_once` proves claim coordination across runtimes sharing one Postgres. Admission budgets are per process; no sustained 100-tenant benchmark or two-process repeat is required here. Preserve inherited row history and cite actual evidence, without converting other open rows to PASS. |

Re-run the full L2 `competing` and its direct-only control after optimization,
using TASK-013's frozen canonical rates, windows, schedule and resource
settings, including the 15–20-minute full-run limit. Report direct
throughput/p50/p99 beside queued offered/accepted/completed/refused counts and
backlog; evaluate interference using the paired-control rule above. If the frozen
schedule no longer saturates the optimized candidate, report that limitation;
do not retune this comparison or claim it proves saturated isolation. Likewise,
report achieved concurrency rather than labelling an open-loop result c64 unless
the observed concurrency supports it. Use the existing L2 resource windows to observe whether memory levels off during long
runs and explain runtime retention. Memory plateau remains a benchmark
observation, while deterministic drain and charged-memory return are asserted
by TASK-013's integration test. Keep expected L2 overload refusals separate from
L1's zero-error integrity rule. TASK-010-R1's historical L2 exclusion does not
exclude this task's before/after L2 comparison.

Preserve the existing replica-claim proof through its exact command:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_verification_runtime::two_replicas_claim_each_queued_run_exactly_once)'
```

Use the existing engine library/integration targets and server `integration`
target, plus SDK/MCP/HTTP/CLI journey owners retained by TASK-012. Add
meaningful focused scheduler/isolation regressions there; record actual
names/commands before RED rather than inventing selectors here. The existing
exact direct-state regression is:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(=pg_durable_verifier_work::only_queued_verification_keeps_run_state)'
```

Use `mise run test:verification:sandbox`, affected `test:vala` / `test:wyrd`
owner slices and the TASK-012 connected journey runners with managed Postgres.
Run `mise run fmt`, `mise run lints`, `mise run check:deps`, `mise run
check:tenant-isolation`; use codegen, language format/lint/type and docs checks
only for affected projections/docs. Integrated change review owns full
shipped-surface coverage and any required cross-cutting aggregate.

Final performance evidence is full clean quiet-host, unprofiled `mise run
bench:l1:verification`, compared to TASK-013's identified L1 baseline and the
newest compatible same-host record; also full `mise run bench:l2:verification`
including competing and its control, and full `mise run bench:l1:gateway` and `mise run bench:l1:bifrost`
for the shared runtime/resource effects, without regression or changing the
fixture/method to obtain a pass. Keep validity, measured scope and target
attainment separate. Full L1 qualification requires every SDK/repetition's
sample minimum and zero errors; smoke/profile/subset or dirty runs remain diagnostics.
Preserve raw records, profiles and exact invocations, and use the existing
report renderer for current and legacy records without relabeling historical
parent-only CPU.

Outputs are the optimized composed execution path, reconciled target
architecture, isolated-capacity proof, matched candidate comparisons, final
qualification and an honestly closed or still-open inherited obligation ledger.
Do not declare either packet complete from a VALID record alone. No new benchmark
machinery, Python/TypeScript mixed traffic, sustained 100-tenant qualification,
multi-process proof, unrelated product behavior, nested publication identity,
payload-sizing packet, durable direct state or direct-concurrency limiting
belongs here.
