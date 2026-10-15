---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-verify-attribution
spec_revision: 3
requirements: [REQ-006, REQ-007, REQ-008, INV-005, AC-001, AC-005]
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4]
---

# Close trace-continuity implementation gaps

Execution skill: `$wyrd-implement`. This is the single remediation task for
TASK-001. It implements already-approved behavior; there is no migration and no
new planning or specification decision.

## Authority and immutable review inputs

- Approved spec: `changes/active/verify-attribution/spec.md`, revision 3.
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`.
- Reviewed base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`.
- Reviewed candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`.
- Verdict: sibling `verdict.md`.
- Validated evidence and rejected proposals: sibling `findings-validation.md`.

Apply AGENTS.md and the referenced architecture authority. Make the smallest
changes at the existing owners. Preserve the approved sampled-parent-or-link
rule and the user's accepted localized span-data clone.

## Required corrections

### FIND-TASK-001-1: producer contexts bypass the existing queue byte budget

`Producer::enqueue_prepared` charges the Arrow rows but not the owned
`TraceContext` retained on the unbounded handoff. Sealing then clones up to 32
contexts into `SealedBatch`, where they remain uncharged through ambiguous
retries. `used_bytes()` can therefore understate live queue memory and the
configured client byte ceiling is not a ceiling.

Keep `ClientByteBudget` as the only byte owner. Charge each admitted context's
owned string bytes before it enters the handoff, share that charged owner
across the logical record's rows, and transfer the charge only for contexts
selected by the existing first-32-distinct seal rule. Charges for unselected
contexts leave with their rows; selected charges remain with the sealed batch
through retry and leave on ACK, terminal loss, or shutdown. Do not add a
configuration option, another limiter/counter, another context representation,
or change batch identity and retry bytes.

Add one focused queue test named
`producer::tests::producer_context_bytes_stay_bounded_through_ambiguous_retry`.
With a small existing budget, distinct non-empty trace states, and the existing
ambiguous mock sink, prove admission refuses before the ceiling is exceeded,
the retained retry remains charged, and terminal settlement returns
`used_bytes()` to zero.

### FIND-TASK-001-2: timeline deduplication depends on adjacency

`Bifrost::trace_timeline` sorts by `(start_time, trace_id, span_id)` and then
uses identity-only `Vec::dedup_by`. Duplicate identities with different start
times need not be adjacent and can both reach every SDK.

Keep the existing query, decoder, return type, and sort. After sorting, retain
the first row for each `(TraceId, SpanId)` with one local standard-library set.
Extend the existing `deduplicates_and_orders_ties_by_trace_and_span` fixture so
the duplicate identity has a different start time and another span sorts
between its two copies. Assert one identity remains in the existing total
order. Do not move deduplication into SQL or duplicate it in language SDKs.

### FIND-TASK-001-3: Scenario 7's required recovery proof is absent

The owner unit test proves the cap and shutdown release, but it abandons the
failed outbox and uses another healthy outbox. It does not provide the named
cross-boundary proof that the same outbox recovers while request and
verifier-result siblings stay available.

Leave production behavior unchanged unless the required test falsifies it.
Add the already-specified
`pg_verification_runtime::sustained_scribe_outage_caps_tenant_spans` using the
existing test-server and Scribe/outbox seams. Hold one Scribe route unavailable,
drive more than 2,048 tenant copies, and prove the cap and loss count. During
that outage exercise the existing HTTP, gRPC, and MCP probes and stage a
verifier result. Restore that same route, prove the retained items drain and
all permits return, then accept one subsequent tenant copy. Existing owner
unit coverage remains sufficient for direct audit and gateway-capture staging.
Do not add a production fault switch, second outbox, general fault framework,
or another integration task.

### FIND-TASK-001-4: `stored` lost its rustdoc

In `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs`, move the existing
“Decode a stored value” documentation and its `# Errors` section immediately
above `stored`. Leave only the fixed-width documentation and its `# Errors`
above `fixed_width`. Change no runtime code and add no documentation test.

## Explicitly excluded

- No migration, compatibility layer, new public API, configuration, or task.
- Do not preserve producer `trace_state` or reserved trace-flag bits in Gate
  links. Approved INV-004 excludes those client fields from operator content;
  the sampled bit is the approved routing input.
- Do not replace the accepted `export.rs` span-data clone with a speculative
  sampler-to-processor channel.
- Do not change tenant routing, sampling, catalog projection, timeline depth,
  SDK shapes, Scribe write ownership, or the 32-context wire bound.

## Focused verification

Run the exact focused proofs first:

```bash
mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=producer::tests::producer_context_bytes_stay_bounded_through_ambiguous_retry)'
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-client --test integration -E 'test(=trace_timeline::deduplicates_and_orders_ties_by_trace_and_span)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(=pg_verification_runtime::sustained_scribe_outage_caps_tenant_spans)'"
```

Then run `mise run fmt`, `mise run lints`, the narrow existing owner lanes
covering the changed shared/server/SQL surfaces, and `git diff --check`. Do not
run `mise run gate`, all SDK journeys, code generation, or schema/migration
checks unless the implementation expands beyond this task's prescribed write
set.

Record the exact commands and selected counts, and map evidence to all four
finding IDs. A later `$wyrd-task-review` reviews the cumulative corrected
candidate against the original task and this remediation.

## Implementation evidence

Commits on `split/verify-attribution`, each one finding:
`981caeb88` (FIND-4), `6464d5c31` (FIND-2), `e0a43a97f` (FIND-1),
`ca614a37a` (FIND-3).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-1: producer context bytes are charged by `ClientByteBudget` from admission through retry to settlement | `wyrd-queue/src/producer.rs` `enqueue_prepared` reserves the record's arrays plus its context's string bytes in the record's one reservation; `queue.rs` `seal` reserves and keeps, on the sealed frame's guard, the bytes of the contexts the first-32-distinct rule selected; `context_bytes` sizes one context. No new type, limiter, option, or representation; batch id and frame bytes unchanged. | `producer::tests::producer_context_bytes_stay_bounded_through_ambiguous_retry` PASS (1 selected, 5 repeated runs PASS); fails when either charge site is removed | PASS |
| FIND-TASK-001-2: timeline keeps one row per `(TraceId, SpanId)` regardless of adjacency | `wyrd-client/src/bifrost/facade.rs` `trace_timeline` keeps the sort and retains the first copy per identity with a local `HashSet` | `pg_bifrost_e2e::trace_timeline::deduplicates_and_orders_ties_by_trace_and_span` PASS (1 selected); the strengthened fixture (copies at offsets 0 and 20, another span at 10 between them) fails on the previous `dedup_by` with the later copy read twice | PASS |
| FIND-TASK-001-3: sustained Scribe outage caps tenant copies, siblings stay available, the same outbox recovers | Test only, production unchanged: `pg_verification_runtime::sustained_scribe_outage_caps_tenant_spans` boots the existing role-separated cluster with its Scribe pod held back (`start_spec_delayed_last`), so the Oracle pod's process outbox peer route reaches no Scribe. Traced `/v1/verification/execute` calls (each also staging a Verifier result) run until `outbox_events_lost_total{outbox="scribe"}` rises; gRPC health and MCP `tools/list` answer during the outage; `restart_node` boots the Scribe pod; the same outbox settles to zero; exactly 2,048 outage copies and every staged result publish; a later traced copy publishes with no new loss. | Named test PASS (1 selected, 3 runs, ~10 s; 342 requests reached the cap); fails with `left == right` when the pool is set to 1,024 | PASS |
| FIND-TASK-001-4: `stored` has its rustdoc and `fixed_width` only its own | `wyrd-sql/src/queries/verifier_runs.rs`: documentation moved; no runtime change | `mise run lints` | PASS |

Non-goals held: no migration, configuration, public API, task, or second
outbox/limiter/fault switch; no change to routing, sampling, catalog
projection, timeline depth, SDK shapes, Scribe write ownership, the 32-context
bound, or the accepted `export.rs` span-data clone; producer `trace_state` and
reserved flag bits are still not forwarded to Gate links.

Deviation: the FIND-3 test reaches the Scribe outage through the existing
multi-pod cluster harness (`wyrd_testing::bifrost::cluster`) rather than the
single-pod `WyrdTestServer`, because a single pod's route is its in-process
Scribe, which has no reversible unavailable state without a production fault
switch. The test's `SPAN_PERMITS` mirrors the server's private cap. The MCP
probe is a raw stateless JSON-RPC `tools/list` (no MCP client is a
`wyrd-server` dependency).

### Verification commands

```bash
mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=producer::tests::producer_context_bytes_stay_bounded_through_ambiguous_retry)'
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-client --test integration --run-ignored all -E 'test(=pg_bifrost_e2e::trace_timeline::deduplicates_and_orders_ties_by_trace_and_span)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(=pg_verification_runtime::sustained_scribe_outage_caps_tenant_spans)'"
mise exec -- cargo nextest run --locked -p wyrd-queue    # 59/59
mise run fmt
mise run lints
mise run test:shared
mise run test:bifrost:journey:sdk        # wyrd-client pg_bifrost_e2e owner binary
mise run test:bifrost:integration:server # includes pg_verification_runtime
git diff --check
```

The timeline test is `#[ignore]`-gated as a controlled-Postgres journey, so its
corrected selector is `pg_bifrost_e2e::trace_timeline::…` with
`--run-ignored all`; the task text's `trace_timeline::…` selects nothing.

### Lane results

| Command | Result |
|---|---|
| `mise run fmt` | PASS |
| `mise run lints` | PASS (first run refused an `f64` counter cast in the new test; the counter is now parsed as an integer) |
| `mise run test:shared` | PASS, 796 run |
| `mise run test:bifrost:journey:sdk` | PASS, 39 run |
| `mise run test:bifrost:integration:server` | PASS, 97 run |
| `git diff --check` | PASS |

Status: `IMPLEMENTED`, routed to `$wyrd-task-review`.
