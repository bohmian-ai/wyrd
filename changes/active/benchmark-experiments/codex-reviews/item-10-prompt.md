Independent read-only diagnosis and recommendation. Do NOT edit files, build, or run tests or benchmarks. Repository: this directory (Wyrd).

Goal: read only lines 253-300 of changes/active/benchmark-experiments/tasks/TASK-006-R2-revision-19-supplement.md (section "## 10. Fix 4: memtable Variant shredding"). Do NOT read the "### Item 10 design" section that follows it, or any *design*, *diagnosis* or *prompt* file in the scratchpad: they hold another agent's proposal, and this review must be independent of it. The architecture documents are input, not law: if a rule in them blocks the best design, say so and propose the amendment.

Problem: a span filter on a JSON (Variant) attribute, `SELECT COUNT(*) FROM bifrost.traces.spans WHERE attributes ->> 'region' = 'eu-west'`, takes 50.4 ms p99 when the rows are still in Scribe's in-memory memtable, against 17.7 ms when they are in published Parquet files whose Variant columns are shredded. Benchmark: crates/wyrd/wyrd-testing/src/bin/capacity/query/ (step `span-shredded`, data generator in telemetry.rs).

Explain, from the code, the full path of a row: request -> WAL -> memtable (active and frozen) -> local staged Parquet files -> the merged file Scribe uploads -> publication -> Forge compaction. For each step say whether Variant values are shredded, and, if so, exactly where the shredding layout (which keys get their own column) is chosen, from which rows, and how many. Quote file:line. In particular, explain the existing sampler precisely: where it runs, what it reads, how sample size is computed and how it scales with input size, what the thresholds are, and how big its input is in practice (use the metrics in /tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-worktrees-benchmark-rev19/3b4a802a-ab40-4e5b-b8d0-574ea2f1a6e6/scratchpad/flow-sustain-1.log, Prometheus text after "T <unix>" lines, and the records in target/benchmarks/bifrost/writes-parquet/20261028T101743Z-64ca466fb.json).

Then recommend how to make memtable reads of shredded Variant paths fast. Compare at least these alternatives with concrete pros and cons, including how good the key choice is (how representative the rows it is chosen from are) versus today's, memory cost, write-path cost, read-path cost, and what happens to staged files and uploaded files:
 (a) leave the memtable unshredded;
 (b) keep the unshredded batches and add a shredded copy;
 (c) shred memtable data with keys chosen from recent memtable data;
 (d) shred memtable data with the keys of the table's most recently uploaded file;
 (e) one fixed key list per table;
 (f) anything better you find (for example making the memtable filter itself cheaper without shredding).
Also say how query results must look to clients and to DataFusion (one schema per scan) and whether unshredding is needed anywhere, and why.

Write for a maintainer who wants plain language: short sentences, no jargon without a one-line definition, numbers with sources. Under 1500 words. End with one recommendation and the measurements that would confirm it.
