# Independent remediation findings validation

Recommendation: **PASS**. Both prior findings are closed. The independently validated open ledger is empty. No new production finding or finding ID is proposed.

## Immutable subject and coverage

Repository: `/Users/stevenforrester/Documents/GitHub/schema-change-task-7`.
Original base: `76e095738d2cf89e10523a92bb9b4f585fa1dd3d`.
Prior candidate: `dabe0e7dde58c6e26d584120ae0a48cfede79436`.
Candidate: `bfe479b9248f4f71c43a719b61db1ce35bad69d9`.
Reviewed remediation range: `git diff dabe0e7dd bfe479b92`, with the original cumulative range available for baseline correspondence. HEAD remained the candidate; this reviewer changed only this report and ran no Cargo command.

Inputs inspected: the new scope map, root AGENTS, Rust/index/maintainer guides, spec-driven workflow, TESTING, approved revision-16 specification and storage supplement (especially lookup-performance, measurement and Bloom-cost sections), applicable Bifrost source-authority/measurement sections; original TASK-007; prior verdict, validated ledger, behavior/query-domain reports and verification; R1 remediation and appended evidence. No CodeGraph directory or applicable nested directory instructions exist, as established by the scope map.

Expanded source coverage: full changed `ScanEvidence::of`, `Attributed::of`, `StoredBlooms::{read,since}`, `exploration_filters_keep_exact_populations`, and telemetry `resources`/test; existing checkpoint, delta, process-window producers and their render/consumer path; cluster/server audit-retention synchronization; existing gateway setup/capture context. Checked the complete changed test assertions and their writers, filters, name/UID equivalences and declaration transitions. No materially changed production query, authorization, persistence, distributed execution or lifecycle owner requires another specialist. No new blocker requires separate focused validation.

## Closure matrix

| Obligation | Independent source/evidence assessment | Result |
|---|---|---|
| FIND-TASK-007-1: final-contract matched baseline | Original-base to prior-candidate production changes are only `oracle/live.rs` and `oracle/analytical.rs`; Cargo change is a comment, and other changed files are tests/task evidence. Swapping precisely these production files on the common R1 harness therefore represents the intended baseline. No dependency/schema backport or production fix contaminated the stated base. Conditions and three repetitions are stated in R1:112–124. | PASS |
| FIND-TASK-007-1: expose existing elapsed/query/resource observations without a new profiler | `ScanEvidence::of` retains `delta.interval_seconds`, adds the duration histogram family, and prints existing stage metrics plus `delta.resources()`. `BifrostTelemetryDelta::resources` is read-only rendering of the existing process window. Its unit test asserts actual output. | PASS |
| FIND-TASK-007-1: truthful answer/attribution limits | R1:125–176 labels process co-tenancy, endpoint RSS, summed operators, unavailable page/decode detail, single-node scope, orchestration versus query duration, and invalid baseline ordered answers. Equal writer rounds and separate audit totals avoid the prior unsupported causal comparison. | PASS |
| FIND-TASK-007-1: retained per-path source/scan/query timing, repeated spread and writer memory observations | R1:138–168 summarizes elapsed/planning/work and writer CPU/filter bytes. Existing six raw journey outputs independently supply source/scan/query timings, spread and writer RSS; the numerical appendix below retains them durably. | PASS |
| FIND-TASK-007-2: populated gateway and exact shared filters | Gateway model/mock setup, workload-rooted release token, public HTTP call with accepted subject/environment, awaited call count; all eight writer tables must return nonempty answers to all ten positive filters, and exact UID/name-version answers must agree (`verification_runtime.rs:6490–6735,6855–6917`). | PASS |
| FIND-TASK-007-2: truthful audit population and lookup keys | Ninth table participates in nonempty ground truth and every shared filter. A settled cutoff freezes its self-growing population; absent descriptor columns decode as null. Each of its four existing keys requires an actual nonnull value and checks the exact population count (`:6855–6895,6920–6949`). No observation subject is fabricated from an audit target. | PASS |
| FIND-TASK-007-2: proper-subset declaration and equal writes with physical evidence | One full, leading-half and empty stage through existing alter-table API; each stage writes the same checkout round and compares exact answers live/published. Every default has multiple columns, so the asserted nonempty `n/2` set is strict. Existing Parquet inspection separates audit by its distinctive schema and computes each stage's added filter bytes (`:6965–7109`). | PASS |
| Changed assertions and standards | Named JSON columns plus explicit cast aliases correctly replace positional attribution decoding, preserving duplicate populations by sorting full rows. Changed result-count/UID/version assertions follow actual rounds. New/changed Rust items have substantive rustdoc, fallible helpers explain errors, and the new accessor has no IO or public product contract. No dependency, generated artifact, default or runtime behavior changed. | PASS |

## Source-backed closure details

The resource renderer's scope is accurate. `checkpoint` captures the process and start instant; `delta_without_sampler` uses elapsed monotonic time and endpoint process samples; `process_window` computes existing CPU/Tokio deltas and max endpoint RSS. The new accessor does not claim a continuous peak or writer-specific allocation. Its only external caller is the shared `ScanEvidence::of`; result/trace/window/group/custom/exploration statements and equal write windows reuse that owner. A separate profiler or sampler would add no necessary closure behavior.

The exploration fixes the prior vacuous gateway case at both producer and consumer: calls go through the authenticated gateway, capture is awaited, every table's ground truth must be nonempty, and every writer table must have positive shared-filter answers. Release/version and subject kind/name/version checks are equivalent to their exact UIDs rather than merely syntactically successful. Existing same-name Agent/Model, concurrent billing releases/environments, shared subjects and skew remain. The audit exception to positive Service/subject requirements is correct: it is still checked against truthful values and its accountability keys. `await_audit_retained` drains acknowledged staged decisions, and the fixed timestamp excludes the subsequent reads' decisions. Named lookup in `Attributed::of` matches omitted JSON nulls and the explicit projection aliases, avoiding a shifted all-null row. Existing `texts` remains confined here to scalar/count or fully populated projections; the changed attribution/audit-key decoding no longer relies on its positional behavior.

The half-default and empty stages retain the same release/subjects/frame counts and existing public table alteration. Stored writer filters decrease and disappear as required; audit bytes are separately reported and honestly attributed to prior reads/policies. No positive-pruning quota, exhaustive policy matrix, extra ingestion API, private capture seam or writer allocator is warranted.

## Validated closure ledger

| Stable ID | Validation | Closure and smallest correction boundary |
|---|---|---|
| FIND-TASK-007-1 | CLOSED | Existing matched runs, common harness and telemetry owner supply the requested elapsed/query/stage/work/spread and representative writer CPU/RSS observations. The compact R1 table initially omitted several dimensions and raw-output references; the orchestrator located the existing experiment logs. This reviewer independently read and summarized those observations below. No new experiment or source change is needed. |
| FIND-TASK-007-2 | CLOSED | Existing gateway/public query/table-alter owners now demonstrate representative nonempty gateway, truthful audit and full/proper-subset/empty exactness with physical evidence. |

The initially investigated record-completeness gap was part of FIND-TASK-007-1, not a new production defect. Finding existing observations resolves it. Reject raw-log retention as a general requirement, exhaustive per-filter numerical tables, new metrics/samplers, writer allocation attribution, distributed/capacity runs, and any new numeric performance target. The numerical summary, existing task evidence and explicit limits satisfy the bounded remedy.

## Accepted carry-forward and verification limits

Carry forward the prior source-backed acceptance of native sorted sibling merge plus appended native sanity checker, one distributed planner, query-local failure propagation, exact result/window/group/trace semantics, tenant/table/column authority, live/hot/published and retained source visibility, deadlines/ceilings, storage validation, unchanged defaults and non-goals. R1 changes only test infrastructure, fixtures/assertions and evidence; it does not invalidate those production rows. Do not reopen their discovery or rerun the entire suite to address a record-retention issue.

`git diff --check dabe0e7dd bfe479b92` passed. This reviewer deliberately ran no Cargo work to avoid overlap with the orchestrator. The orchestrator owns the exact telemetry-unit and five focused journey reruns and their verification report. The exact telemetry unit passed in the orchestrator rerun (1/1, run `c10d99aa-18da-4d60-ab9a-7d8970f3b558`). The existing three-repetition comparison/storage/trace execution outputs were independently inspected here; format/lints and tenant-isolation remain implementation-reported. `build:server` also passed (1m56s); the five focused journeys are running serially in isolated Postgres at this report handoff. See the orchestrator verification report for their final result. No cross-platform/capacity benchmark or source swap was performed in review.

Open ledger: **empty, independently validated**. Closed: **FIND-TASK-007-1 and FIND-TASK-007-2**. No rejected optional expansion creates work; no new material blocker was found. This is the first remediation review, so the two-unsuccessful-round convergence rule does not apply.

## Independently retained numerical evidence

Exact input directory: `/private/tmp/claude-501/-Users-stevenforrester-Documents-GitHub-schema-change-task-7/e8060899-5ac4-426b-8ba3-09a54d77cba8/scratchpad/measure/`.
Inputs inspected: `base-journeys-1.log`, `base-journeys-2.log`, `base-journeys-3.log`, `candidate-journeys-1.log`, `candidate-journeys-2.log`, `candidate-journeys-3.log`, and the correspondingly named six `storage` and six `trace` logs. The neighboring `measure.sh` confirms serialized exact selectors, three repetitions, common harness, two production-file baseline substitution and restoration; `summarize.py` documents the metric aggregation. Neither script was executed by this reviewer; a read-only parser independently extracted the existing raw observations. Current source correspondence confirms the stated production boundary.

All three candidate journey runs passed 5/5; base repetitions 1–2 passed 4/5 with the known custom ordering failure, and repetition 3 passed 5/5. All six storage runs passed 2/2, all six trace runs passed 1/1. These are original experiment results, distinct from review reruns. Base custom timing remains incorrect-answer work and is not a successful equivalent-answer speed comparison.

Each cell below is median (min–max) of three observations. Times are milliseconds. For each exploration stage, each repetition sums its 112 measured filter/key statements; other rows use the existing named measurement window. `cut` adds cut-read/materialization; scan is summed `DataSourceExec.time_elapsed_processing` (zero means no emitted file-scan time, not zero total live execution). Query time uses the duration histogram sum and does not replace the caller boundary. Independent statement work/counts agree with the task's existing tables.

| Path/window | Side | Caller | Query | Planning | Listing | Cut | Scan |
|---|---|---|---|---|---|---|---|
| passed status | base | 29.4 (25.4–32.8) | 3.3 (2.9–4.7) | 4.6 (4.5–5.9) | 2.3 (2.3–2.3) | 9.3 (5.2–9.6) | 2.2 (1.9–3.6) |
| passed status | candidate | 27.4 (27.2–30.1) | 4.6 (3.0–4.7) | 4.8 (4.7–5.4) | 2.2 (2.2–2.3) | 6.9 (5.1–7.5) | 3.3 (1.9–3.5) |
| failed status | base | 19.4 (17.9–22.2) | 2.8 (2.4–2.8) | 4.3 (4.1–4.3) | 2.3 (2.3–2.3) | 0.3 (0.3–5.0) | 1.7 (1.3–1.8) |
| failed status | candidate | 18.0 (17.5–22.7) | 2.7 (2.6–2.9) | 4.3 (4.2–4.3) | 2.2 (2.2–2.3) | 0.3 (0.3–4.7) | 1.6 (1.4–1.8) |
| two trace executions | base | 115.4 (59.2–190.8) | 11.9 (9.8–22.8) | 28.7 (17.7–83.5) | 7.9 (6.3–18.8) | 31.0 (22.6–81.7) | 6.6 (5.6–9.9) |
| two trace executions | candidate | 168.4 (116.9–172.5) | 15.3 (9.5–17.4) | 55.2 (17.8–76.9) | 15.4 (6.3–15.7) | 58.0 (22.5–76.0) | 7.3 (5.4–7.3) |
| three drift windows | base | 31276.9 (31275.1–31299.1) | 13.2 (12.1–14.7) | 33.7 (32.2–34.1) | 12.4 (11.2–12.4) | 86.0 (85.2–101.2) | 10.7 (10.5–11.1) |
| three drift windows | candidate | 31235.2 (31215.6–31319.0) | 11.5 (8.9–16.0) | 30.9 (30.5–38.3) | 11.3 (10.6–22.3) | 80.8 (72.9–131.9) | 7.6 (7.0–10.8) |
| four turns and two groups | base | 5079.1 (5063.2–5121.3) | 35.9 (34.7–37.8) | 46.8 (45.3–53.2) | 14.7 (13.3–15.3) | 7.8 (6.0–8.7) | 0.0 (0.0–0.0) |
| four turns and two groups | candidate | 5091.2 (5079.9–5107.5) | 38.0 (37.9–38.8) | 50.7 (47.5–53.2) | 15.0 (14.1–17.7) | 7.6 (6.7–8.4) | 0.0 (0.0–0.0) |
| custom eu lookup | base | 53.1 (47.9–77.0) | 7.0 (6.0–10.0) | 6.9 (6.2–8.4) | 4.7 (4.3–6.4) | 10.7 (3.7–12.2) | 2.2 (1.4–2.3) |
| custom eu lookup | candidate | 58.0 (51.2–62.6) | 7.2 (5.2–8.0) | 6.9 (6.8–8.4) | 4.5 (4.4–4.8) | 10.5 (8.3–10.6) | 1.9 (1.7–2.9) |
| results bloom column | base | 147.4 (145.5–149.3) | 22.3 (21.0–22.5) | 36.1 (34.8–36.7) | 21.9 (21.7–22.4) | 4.0 (3.9–4.1) | 22.4 (22.2–22.7) |
| results bloom column | candidate | 146.6 (145.4–148.9) | 22.2 (21.0–22.8) | 36.4 (35.4–36.6) | 21.5 (21.3–22.4) | 4.1 (4.0–4.2) | 21.7 (20.8–23.6) |
| live with default blooms | base | 2258.3 (2244.0–2261.8) | 370.8 (359.1–372.9) | 759.1 (758.1–766.6) | 286.9 (285.5–287.6) | 44.7 (43.8–45.1) | 409.2 (390.0–415.7) |
| live with default blooms | candidate | 2225.8 (2210.9–2789.6) | 362.7 (352.8–392.1) | 749.5 (748.6–809.4) | 282.4 (280.9–314.7) | 43.9 (43.3–53.6) | 397.8 (381.3–427.2) |
| published with default blooms | base | 2164.0 (2162.6–2174.7) | 348.4 (343.7–350.1) | 676.0 (675.3–682.4) | 258.5 (257.5–259.9) | 47.1 (45.4–51.2) | 468.0 (451.0–482.6) |
| published with default blooms | candidate | 2160.5 (2152.5–2357.7) | 349.7 (336.4–351.0) | 689.3 (685.4–721.6) | 260.0 (259.8–270.3) | 46.0 (45.9–46.9) | 457.4 (437.4–458.1) |
| live with half the default blooms | base | 2320.8 (2289.2–2330.8) | 395.9 (387.7–396.3) | 764.5 (740.6–765.0) | 293.8 (287.2–295.6) | 46.0 (44.9–46.0) | 463.4 (451.3–476.7) |
| live with half the default blooms | candidate | 2289.6 (2258.4–2372.6) | 397.9 (379.8–404.6) | 733.0 (728.4–780.6) | 281.9 (280.5–297.0) | 44.3 (44.2–46.1) | 476.4 (442.4–478.6) |
| published with half the default blooms | base | 2259.7 (2248.1–2283.4) | 361.2 (350.6–362.0) | 706.0 (702.7–710.9) | 265.3 (262.6–267.3) | 49.4 (48.5–54.9) | 573.8 (541.8–573.8) |
| published with half the default blooms | candidate | 2226.9 (2208.1–2269.0) | 345.4 (336.2–348.5) | 693.8 (684.5–717.2) | 259.5 (256.8–264.6) | 53.2 (52.7–55.1) | 518.9 (514.7–541.3) |
| live without blooms | base | 2384.7 (2363.0–2386.8) | 408.4 (401.7–419.3) | 750.8 (748.6–774.1) | 289.9 (289.8–292.9) | 48.0 (47.9–48.7) | 587.3 (578.6–626.2) |
| live without blooms | candidate | 2372.9 (2345.3–2375.8) | 394.8 (389.5–405.3) | 763.0 (761.7–777.0) | 290.6 (288.1–292.3) | 47.8 (47.1–48.0) | 551.3 (547.2–571.6) |
| published without blooms | base | 2291.3 (2275.2–2310.8) | 366.6 (357.4–368.0) | 691.8 (687.7–713.8) | 259.5 (259.0–267.0) | 54.3 (50.3–55.1) | 625.8 (619.3–646.8) |
| published without blooms | candidate | 2283.8 (2281.9–2313.5) | 352.5 (351.8–361.4) | 712.2 (709.1–713.1) | 262.6 (261.3–266.3) | 55.4 (50.4–55.6) | 600.1 (594.0–610.0) |

Writer cells are whole-process CPU seconds and endpoint RSS MiB, including all roles and the journey; they are not writer allocations or continuous peak memory. Each stage writes equal checkout inputs; increasing total RSS includes accumulated process state. Audit/background writes prevent causal filter-cost attribution. Filter-byte/accounting observations and unavailable page/decode detail are already retained in R1 and remain accepted.

| Declaration | Side | Ingest CPU s | Ingest RSS MiB | Publish CPU s | Publish RSS MiB |
|---|---|---|---|---|---|
| with default blooms | base | 0.29 (0.28–0.29) | 456.0 (440.7–467.9) | 1.46 (1.45–1.46) | 556.3 (539.8–569.1) |
| with default blooms | candidate | 0.29 (0.28–0.29) | 473.8 (456.6–480.0) | 1.45 (1.43–1.49) | 555.8 (477.4–579.8) |
| with half the default blooms | base | 0.28 (0.28–0.29) | 630.5 (613.3–641.2) | 1.11 (1.08–1.18) | 718.8 (711.3–729.4) |
| with half the default blooms | candidate | 0.29 (0.29–0.29) | 629.3 (552.0–652.6) | 1.06 (1.06–1.08) | 728.7 (654.7–743.7) |
| without blooms | base | 0.30 (0.29–0.33) | 777.8 (770.0–790.0) | 0.56 (0.56–0.59) | 865.1 (859.7–877.8) |
| without blooms | candidate | 0.29 (0.28–0.33) | 787.5 (714.6–803.0) | 0.57 (0.57–0.58) | 881.8 (808.4–890.2) |

The values establish bounded observations, not statistical significance or a speed guarantee. In particular the full published exploration planning median is 676.0 → 689.3 ms (about 2% higher), and those three-sample ranges do not overlap; this report does not carry forward the blanket sentence that the added traversal is below spread on every path. There is no measured causal isolation of that traversal, and the corresponding caller/query observations do not establish a material supported-use performance regression or violate a new budget. Keep the observed variation and original limits as the conclusion; no tuning or further experiment is required by this bounded remedy.
