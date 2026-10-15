# TASK-001 R1 review navigation map

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/verify-attribution`
- Original base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Prior reviewed candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`
- Prior candidate tree, also present at `e9f476e82`: `9df6ae0e34c646e6ef2c32a1c030e2a800683644`
- Current candidate: `4a6af5320ab8c236fbe640ff0dc5ddf544bdcde2`
- Current candidate tree: `036d647413d0acfd1afdebff2b7057b45ac00609`
- Approved authority: `changes/active/verify-attribution/spec.md`, revision 3
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`
- Prior verdict and ledger: `changes/active/verify-attribution/review/task-001-827ec9f15/{verdict.md,findings-validation.md}`
- Remediation task and evidence: `changes/active/verify-attribution/review/task-001-827ec9f15/TASK-001-R1-close-trace-continuity-gaps.md`

The current review is cumulative from the original base. For remediation
closure, inspect commits `981caeb88`, `6464d5c31`, `e0a43a97f`, `ca614a37a`,
and evidence commit `011b051a8`, plus both later main merges `cb84ddac8` and
`4a6af5320` wherever they touch the task's owners or public SDK contracts.
Unrelated upstream changes are not task findings unless they regress an
approved TASK-001 obligation in the resulting repository.

## Changed owners and consumers

| Boundary | Existing owner / changed symbols | Callers and proof to trace |
| --- | --- | --- |
| Queue byte ownership | `wyrd-queue` `Producer::enqueue_prepared`, `ClientByteBudget`, `Row`, `RecordQueue::seal`, `SealedBatch` | all traced `record`/`observe`/table inserts; ambiguous retry and settlement; focused queue test |
| Timeline identity | `wyrd_client::Bifrost::trace_timeline` | blocking Rust, PyO3, Python sync/async, N-API/TypeScript; Rust timeline fixture and three SDK action timelines |
| Scribe outage proof | `pg_verification_runtime::sustained_scribe_outage_caps_tenant_spans` using the role-separated Bifrost cluster | HTTP verification/result staging, gRPC health, MCP tools/list, peer outbox recovery, span permits/loss metrics |
| SQL helper documentation | `verifier_runs::{fixed_width,stored}` | all verifier-run decoders; format/lints |
| Post-merge Python contract | `wyrd.bifrost._client`, `wyrd.bifrost.SpanRecord`, native `trace_timeline` projection | Python action-timeline helpers and journey; authored typing after generator removal |
| Post-merge TypeScript contract | generated `SpanRecord` and `Bifrost.traceTimeline` surface | TypeScript action-timeline/gateway journeys and type checks |
| Original trace behavior | `wyrd-telemetry` sampler/routes/export, `serve_request`, Gate producer links, one Scribe outbox | HTTP/gRPC/MCP, verifier/gateway/Eval, tenant/operator copies, tenancy and sampling tests |

## Prior findings to close

- `FIND-TASK-001-1`: every retained producer context byte is owned by the
  existing client byte budget from admission through terminal settlement.
- `FIND-TASK-001-2`: timeline returns one row per `(trace_id, span_id)` after
  deterministic ordering, even when duplicate start times differ.
- `FIND-TASK-001-3`: one real outbox remains capped during Scribe outage,
  siblings remain available, and that same outbox drains and releases permits
  after recovery.
- `FIND-TASK-001-4`: `stored` and `fixed_width` each have the correct rustdoc.

Also re-audit all original task requirements, invariants, acceptance criteria,
prohibited changes, and non-goals. Preserve the prior rejection of forwarding
producer `tracestate`/reserved flags and the user-approved localized
`SpanData` clone unless current source supplies new contrary evidence.

## Available verification evidence and known gap

The remediation record reports all three focused tests green before the main
merges, plus `test:shared`, `test:bifrost:journey:sdk`,
`test:bifrost:integration:server`, format, lints, and diff check. After the two
merges, only format, lints, typecheck, codegen, and docs checks were reported.
The focused remediation tests and Python/TypeScript timeline journeys have not
been rerun on `4a6af5320`; treat source compatibility and current execution as
unproved until fresh evidence exists.

## Governing authority

- `AGENTS.md`, `architecture/agent-rules.md`
- `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/references/languages/{spec-driven-development,maintainer-style,testing-workflows,python-api-and-stubs,typescript-guide}.md`
- `architecture/references/domain/{telemetry-observations,olap-serving,arrow-analytical-interop}.md`

Review source, the actual cumulative diff, and callers independently; this map
is navigation, not evidence or a conclusion.
