# TASK-001 r7 system-resilience review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `6147cc617d81f2c03464043be698ab2565e9d745`
- Candidate tree: `7b7bb069ecbe8600e09cc8b6ea4e938709614718`
- Approved contract: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation under review: `changes/active/bifrost-variant/review/TASK-001-r6/TASK-001-R6-close-variant-canonicality-and-proof-gaps.md`

The checkout matched the supplied candidate and tree before and after this
review. The review directory is untracked review output and is outside the
immutable candidate.

## Deployed-path evidence

| Runtime path | Deployed owner and propagation | Affected capabilities |
|---|---|---|
| Raw Arrow Variant admission and rendering | `EncodedVariant::validate` checks size, performs the explicit-stack encoded scan, and invokes upstream recursive validation only after Wyrd has excluded malformed and over-depth bytes (`crates/shared/wyrd-queue/src/variant.rs:266-284`, `959-1024`). `object_field_slots` gives each object field a disjoint byte region before traversal (`variant.rs:1027-1080`), and `variant_bytes_to_json` applies the same borrowed validation before rendering (`variant.rs:347-354`). Built-in Scribe validation reaches this owner before preprocessing, shard dispatch, WAL, or acknowledgement. | Canonical Arrow writes, built-in signal ingestion, live and published reads, HTTP/MCP/CLI and SDK rendering. |
| JSON-to-Variant admission | `EncodedVariant::from_json_text` first performs one complete `serde_json::RawValue` syntax validation, then `append_raw` builds only through Wyrd's 64-container boundary (`variant.rs:218-227`, `731-808`). Past that boundary, `scan_numbers` repeatedly deserializes each remaining container's complete raw subtree to find an out-of-range number that must outrank depth (`variant.rs:810-865`). | Oracle `parse_json`/`try_parse_json`, buffered JSON preparation, and programmatic JSON conversion. |
| Oracle execution and terminal | `ParseJson::invoke_with_args` synchronously invokes `EncodedVariant::from_json_text` for every non-null row and returns the catalogued failure through `QueryCatalogError` (`crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:588-620`). Interactive and distributed attempts retain the existing one-attempt cancellation and terminal framing; SDK collection still withholds partial rows after a failure terminal. | HTTP, gRPC, MCP, CLI, scheduled verification, Rust, Python, and TypeScript query callers. |
| OTLP non-finite values | OTLP projection reaches the canonical raw Variant numeric gate. The table projection rejects only the offending record, preserving accepted siblings; the new log test asserts one accepted and one rejected record (`crates/vala/vala-bifrost-redux/src/tables/logs/mod.rs:170-197`). | OTLP spans, logs, and metrics with NaN or infinite attribute/body values. |
| Gateway nullable Struct | The unchanged table validator refuses a present `resolved_model` with absent children before Scribe acknowledgement. The new peer journey drives the actual peer Scribe boundary, and the server journey verifies unresolved calls read null children from hot and published storage. | Gateway capture writes and subsequent Oracle reads. |
| Durable storage and recovery | The r6 code changes add no WAL version, fence, staged-run state, publication identity, Iceberg transition, Forge lease, or recovery owner. Semantic refusals still occur before durable dispatch; accepted batches retain the existing WAL/fence/replay and publication lifecycle. | Scribe restart, hot publication, Iceberg reads, Forge reconciliation and GC. |

## Failure-path and recovery assessment

| Failure or interruption | Boundary, surviving state, and recovery | Result |
|---|---|---|
| Malformed, overlapping, duplicate-name, non-finite, or noncanonical raw Variant bytes | The shared borrowed validator refuses the cell before WAL/ACK and before any renderer enters upstream recursive work. The failed request leaves no durable state; permits and request-owned values drop, and a following write can proceed. | PASS |
| Compact raw Variant nested far beyond 64 containers | The encoded scan is iterative and records depth before upstream recursive validation. Every renderer now enters the same validator, so a hostile stored cell fails the query rather than exhausting the process stack. | PASS |
| NaN or infinite OTLP body/attribute | The projection returns one record-level rejection and retains valid siblings. No rejected record reaches Scribe durability; later batches use the unchanged path. | PASS |
| Partial gateway `resolved_model` sent directly to a peer Scribe | Table-owned whole-or-absent validation refuses before ACK and retains no row. The journey proves the peer remains able to accept a subsequent valid gateway capture. | PASS |
| Server exit during a semantic refusal | Refusal precedes shard dispatch and WAL, so restart has nothing from that request to replay or reconcile. Previously acknowledged batches retain their ordinary WAL, fence, staging, and object evidence. | PASS |
| Oracle Variant failure after rows were produced | The catalogued error remains a query-scoped failure terminal; clients discard partial rows, and the attempt's existing cancellation tree owns descendant cleanup. No Scribe, Forge, or catalog authority changes. | PASS |
| Deep JSON text with no out-of-range number | After the depth limit, `scan_numbers` reparses the entire remaining subtree at every nesting level. Work grows quadratically with nesting and the synchronous UDF has no cancellation/deadline check inside that loop, so the query task cannot settle or release its slot until the scan finishes. | **FAIL — SYS-R7-001** |

## Review finding

### SYS-R7-001 — Deep JSON can monopolize an Oracle execution task with quadratic synchronous work

- **Classification:** resource-safety regression / incomplete remediation.
- **Violated boundary:** Bifrost computation and Oracle query execution must be bounded, and cancellation/deadline must terminate the complete attempt (`architecture/bifrost-design.md` system boundary and Oracle lifecycle; `architecture/operations/reliability-and-recovery.md` capacity and Oracle failure boundaries).
- **Location:** `crates/shared/wyrd-queue/src/variant.rs:819-862`; reachable from `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:601-620`.
- **Evidence:** for each container below depth 64, `scan_numbers` calls `serde_json::from_str` on that container's full remaining text, enqueues its child, then repeats on the nearly identical suffix; a single-element array nested `d` levels therefore scans roughly `d + (d-1) + ... + 1` bytes. The implementation evidence expressly records this as “Quadratic numeric scan,” while the focused proof stops at 10,000 levels and does not establish a production work bound. Oracle's 64 KiB SQL-text ceiling still permits about 32,000 literal array levels, and `parse_json` over an existing text column is not bounded by SQL-text length. `ParseJson::invoke_with_args` is synchronous and carries no cancellation token, so an expired deadline cannot interrupt the scan.
- **Observable system consequence:** an authorized query over crafted valid JSON can occupy a DataFusion execution worker and its admitted query resources for disproportionate time; concurrent hostile queries can consume Oracle capacity even after their deadlines expire, degrading unrelated analytical reads until each scan returns. No durable data is corrupted, and the boundary is the affected query task/Oracle capacity rather than the whole storage service.
- **Required correction:** keep `EncodedVariant` as the single owner and keep the initial `serde_json` syntax validation, but replace repeated subtree deserialization below the depth boundary with one iterative, single-pass traversal of the raw text that records the first out-of-range number and its JSON pointer while preserving invalid → numeric → depth selection; do not add a second Variant model or downstream timeout guard. Prove linear/bounded work with a near-input-limit deep value through the real Oracle UDF, assert the deadline/cancellation path can settle promptly, preserve the exact numeric pointer/error, and show a following query succeeds.

## Proof assessment

Recorded r6 evidence reports green results for 68 `wyrd-queue` tests, 847
`vala-bifrost-redux` tests under Postgres, the focused Oracle parser and OTLP
numeric tests, the gateway capture unit, two server journeys, the gateway peer
journey, and format/lint/codegen/skills/docs/diff checks. Those results directly
cover the raw canonicality predicates, pre-ACK refusal, gateway continuation,
record-local OTLP rejection, and ordinary terminal behavior.

The 10,000-level unit test proves stack survival and error identity, but it does
not prove a usable upper bound for the deliberately quadratic scan, query
deadline responsiveness during the synchronous UDF, or continued Oracle
capacity under near-limit hostile input. No additional Cargo, database, cluster,
or language-runtime lane was run by this reviewer.

## Overall result

**FAIL**

The durable and crash-recovery paths remain sound, but the r6 JSON remediation
leaves one reachable Oracle resource-amplification path that can outlive query
deadlines and consume shared execution capacity.
