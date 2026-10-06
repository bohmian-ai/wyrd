# TASK-001 r6 system-resilience review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `ef92074f057b0a240c54b4407d8f32cac1310921`
- Candidate tree: `e3c59991994125dac41187f17de12935e2b45424`
- Latest remediation range: `0e37748f3a27d3bcec4713e6210e97328e045886..ef92074f057b0a240c54b4407d8f32cac1310921`
- Authority: approved `SPEC-bifrost-variant` revision 13, original `TASK-001`, the r5 remediation packet and recorded evidence, and `architecture/bifrost-design.md`

The supplied candidate and tree matched the checkout before this review. This
review traced the cumulative deployed paths and isolated the latest remediation
for its changed crash, refusal, and recovery boundaries. It did not read any
other r6 report.

## Deployed-path evidence

| Runtime path | Deployed owner and propagation | Affected capabilities |
|---|---|---|
| JSON-row Variant admission | The shared queue's `EncodedVariant::from_json_text` parses one `RawValue`, records malformed/numeric/depth facts, and builds only within the 64-container boundary (`crates/shared/wyrd-queue/src/variant.rs:184-207`, `648-840`). Past that boundary, the explicit-stack numeric scan continues without building rejected descendants. Buffered Rust, Python, and TypeScript inserts all route through this shared owner before transport. | SDK row insertion and any server producer using the shared queue. |
| Raw Arrow Variant admission | Every supplied built-in user block reaches its table-owned canonical validator from Scribe `decode_rows` before fingerprint stamping (`crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:533-577`, `601-634`). Declared Variant cells call `EncodedVariant::from_bytes`; size is checked before the iterative encoded scan, shallow-accessor unwinds become `InvalidJson`, and recursive upstream validation runs only when neither malformed nor over-depth state was found (`crates/shared/wyrd-queue/src/variant.rs:210-247`, `896-958`). The latest node budget rejects aliased/shared object layouts before they can drive exponential traversal. | Raw IPC ingestion for canonical built-ins: signals, verification results, gateway captures, audit publication, and developer traces. |
| Scribe durability boundary | Decode/semantic validation executes in the fixed, application-bounded ingress Rayon pool; that pool also catches an unexpected worker unwind and returns a request error (`crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:141-339`). `prepare_and_dispatch` does not create a durable waiter, preprocess, enqueue a shard command, or await durable completion until decode has succeeded (`crates/vala/vala-bifrost-redux/src/scribe/ingress.rs:552-627`). The architecture defines ACK only after WAL fsync, batch fence, and memtable insertion (`architecture/bifrost-design.md:190-205`, `313-325`). | All Scribe writes, their stable batch retry identity, WAL replay, and live-query visibility. |
| Nullable built-in Struct admission | The r5 correction keeps nullable physical children needed for absent parents but refuses parent/child validity mismatches through the existing table validator seam. Results, metric buckets, and resolved gateway models therefore fail before stamping and durable handoff; required `requested_model` children retain their non-null schema. | Verification publication, OTLP metrics, gateway capture, subsequent WAL/staged/hot reads. |
| Oracle `parse_json` | Every Oracle session installs the one `OracleVariantSql` owner. Strict and lenient parsers call `EncodedVariant::from_json_text` row by row; strict failures become the existing `QueryCatalogError`, while only invalid JSON becomes null for `try_parse_json` (`crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:535-628`). The architecture requires the same owner on leader, admission, follower, analytical, and worker sessions and preserves catalog identity across peers (`architecture/bifrost-design.md:441-467`). | Interactive and analytical query over HTTP, gRPC, MCP, CLI, scheduled verification, and all SDKs. |
| Query failure terminal | A `parse_json` or stored-Variant execution failure remains a query-scoped failed terminal. Analytical ownership binds the complete stage graph to one cancellation tree; terminal error, transport loss, cancellation, or deadline cancels and joins descendants, and the client may use streamed rows only after a valid terminal (`architecture/bifrost-design.md:469-534`). | Local and distributed Oracle queries; unrelated Scribe, Forge, and other queries remain available. |

## Failure-path and recovery assessment

| Failure or interruption | Boundary, surviving state, and recovery | Assessment |
|---|---|---|
| A raw Variant nests far beyond 64 containers | `scan_encoded` is iterative and runs before recursive upstream validation. Once depth is present, `Variant::try_new` is skipped, so the hostile value cannot consume the worker stack. The request receives `VariantTooDeep`; no shard command, WAL record, fence, ACK, or retained row exists. The ingress reservation drops and a later write can use the same process. | PASS |
| Object fields alias the same encoded child and describe an exponential logical tree | The latest `visited > value.len()` fence is reached after work linear in the bounded encoded input, records malformed at the root, stops the walk, and causes `from_bytes` to skip upstream recursive validation (`variant.rs:907-913`, `953-957`; `from_bytes`:239-245). This confines the hostile input to one bounded ingress job instead of monopolizing a Scribe worker or threatening the shared server process. | PASS |
| A shallow upstream accessor panics on malformed bytes | `from_bytes` contains the scan with `catch_unwind` and maps the unwind to `VariantInvalidJson`; the surrounding ingress CPU lane independently contains an unexpected decode-worker unwind. The failed job releases its permit and accounting, increments failure/panic telemetry as appropriate, and does not poison the Rayon pool, shard, or server. | PASS |
| Malformed bytes coexist with depth or an unsupported Decimal16 | The complete iterative scan records violation classes and selects malformed, numeric, then depth. No rejected `EncodedVariant` can flow into preprocessing. The public error is deterministic across traversal order and still occurs before WAL/ACK. | PASS |
| Client/request cancellation during validation | Before shard dispatch, the Rayon job owns its admitted values and reservation until it finishes; cancellation drops the waiting request future but does not abandon unowned memory. The bounded scan completes and releases those owners without durable state. After dispatch, the unchanged shard owner and stable batch identity govern uncertain completion and deduplicated retry. | PASS |
| Server process or pod exits during a refused write | A semantic refusal happens before any durable authority transition, so restart has nothing from that request to replay or reconcile. Existing acknowledged writes retain their WAL and batch fence; startup replays WAL and validates staged evidence before admission reopens (`architecture/bifrost-design.md:386-410`). | PASS |
| Oracle `parse_json` sees invalid, over-depth, or out-of-domain input | The error is produced inside the query operator, crosses the local/distributed catalog envelope, and closes the attempt with a failed terminal. The attempt cancellation tree releases query memory, scratch, slots, tasks, and transports; no Scribe/Forge authority changes. A fresh query uses a new attempt and the process-wide Variant registry. | PASS |
| Oracle worker/pod loss after a Variant error or during cancellation | There is no fallback or hidden successor attempt. Peer loss and execution failure are terminal for the selected analytical attempt; head cancellation joins descendants. Replacement pods reconstruct queryable durable state from Postgres/object storage and local Scribes recover their own WAL/staging. Unrelated subsystem protocols continue. | PASS |

## Recovery and proof assessment

Recorded r5 remediation evidence reports these relevant green checks on the
final implementation line:

- all 12 `wyrd-queue` Variant unit tests, including hostile depth, compound
  malformed/depth inputs, exact Decimal16 admission, and
  `raw_shared_field_values_are_refused`;
- the real-server `builtin_variant_columns_are_refused_before_ack` journey,
  including exact refusal, no retained row, the aliased-object case, and a
  successful following write/read;
- the strict Oracle `parse_json` tests, including numeric-before-depth order;
- verification and metrics journeys proving semantic Struct refusal before a
  subsequent valid write and hot/published read;
- `fmt`, `lints`, and `git diff --check` green after the latest addendum.

Per review coordination, this reviewer started no Cargo, mise, codegen, or
environment-backed test process. The assessment relies on current source and
the tracked command evidence in
`changes/active/bifrost-variant/review/TASK-001-r5/TASK-001-R5-close-remaining-variant-contract-gaps.md:286-363`.

The proof directly covers request-scoped continued availability after the
hostile raw inputs. It does not kill a pod in the middle of the new scan or
inject cancellation at each scan instruction. That is not a blocking gap: the
scan precedes dispatch and durable authority, owns no side state, is bounded by
the existing Variant/request ceilings, and the unchanged lifecycle owners
already cover interruption after dispatch.

## Material proposed findings

No material system-resilience findings.

The candidate closes the reachable stack-exhaustion and exponential-scan paths
at the shared raw Variant trust boundary, contains remaining dependency
unwinds, refuses malformed semantic state before WAL/ACK, preserves structured
Oracle cancellation and failed terminals, and introduces no new durable owner,
retry identity, recovery state, or cross-pod coordination.

## Overall result

**PASS**
