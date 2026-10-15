# System resilience review

Result: **PASS**. Material proposed findings: **none**.

## Subject and review boundary

Original base `dac4c340f835676adc29f3dcc01f98e1d3d51465`; candidate
`1cf675c58d01cba0e714102c75cb7d89e7a678b5`. HEAD matched before and after
inspection. The cumulative original task, R1/R2 corrections, R3 correction,
and authorized imported-main seams were reviewed. The latest patch was a
navigation aid, not the acceptance range. No current peer reports were read.

Authority used: repository AGENTS.md and agent rules; task-review skill;
spec-driven development, maintainer style and reference router; approved
specification r2 and original TASK-001; R3 remediation; relevant Wyrd design
provider/authentication/observation contracts; Bifrost topology, acknowledgement,
admission, replay, query and shutdown contracts; telemetry-observations and
analytical-operations-reliability references. CodeGraph preceded source searches.
Historical review evidence was treated as hypotheses, not current execution.

This review assesses the changed deployed paths. It does not certify unrelated
imported-main registry/scheduler functionality or add a migration contract.

## Deployed paths

| Path and owner | Source evidence | System effect |
|---|---|---|
| Framework callbacks → application process OTEL provider | Python `python/wyrd/otel.py::WyrdAgentsTraceProcessor` (`_start`, `_end`, `shutdown`); TS `wyrd/src/index.ts:2552–2630` | Ordinary sampled spans use the existing global provider. Python protects its live map with a lock; both capture context at start, remove completed entries, finish unfinished entries at bridge shutdown and ignore later callbacks. Bridge flush/shutdown does not close the provider or execute framework work on the server. |
| Managed setup → one process-global provider | Python native `src/state/mod.rs:191–219`, Python `_start_telemetry:271–303`; TS `index.ts:2924–2940`, `otel.ts::startTelemetry` | Python serializes same-state setup using the GIL-aware lock and publishes the OnceLock only on success. TS rechecks after import's await. A losing installation releases its candidate and leaves the winning provider alone. Application-owned providers keep their lifecycle/configuration. |
| Stock exporter → current selected-client identity → public OTLP | Python `otel.py::_session/span_exporter/log_exporter/metric_exporter`; TS `otel.ts::config` and three exporter factories; server `http/otlp.rs` and `grpc/otlp.rs` | Export-time token retrieval reuses shared refresh; no copied setup bearer, telemetry auth cache or custom retry transport. HTTP/gRPC adapters authenticate and bound decode before Gate. External model clients remain independent; gateway journeys use existing stock HTTP auth. |
| Trace/log/metric → common Gate producer → Scribe | `gate/mod.rs:829–931`, `dispatch_canonical:959–1003`, `otlp_batch_id:133–155`; `gate/auth.rs:83–85` | All three signals share the corrected identity producer. Tenant, verified subject/current actor, table, logical content and accepted correlation determine identity. The exact verified pair also crosses in the Scribe frame; no wire-supplied publisher chooses authority. |
| Scribe → WAL + tenant SQL fence → active/staged/published rows | `scribe/ingress.rs:515–595`; `scribe/shards.rs::process_group`, `append_and_sync_batch_commits`, `commit_batch_control_fences`; `vala-sql/src/queries/scribe_batch_commits.rs`; `scribe/replay.rs` | Existing bounded admission, WAL fsync, durable fence and insertion own acknowledgement. Request cancellation after dispatch does not revoke shard ownership. R3 changes neither the durable fence nor native explicit batch identities. |
| State lifecycle → managed OTEL flush → existing native drain | Python `state/mod.rs:295–320,525–544`; TS `index.ts:2727–2746,2971–2994`; shared `state.rs::TelemetryFlushFailure`; `bifrost/facade.rs:700–737`, `handle.rs:315–383` | OTEL failure is captured before native drain, which is still attempted. Native drain runs off async workers and attempts every producer. Shared catalog settlement preserves the native stable code when both fail; native-only error passes through. |
| Readback → verified query context → Oracle streaming | server `query/service.rs:172–225`, `query/routes.rs:285–339` | Query failure/cancellation releases stream-owned work. It does not shut down exporters or unrelated server services. The existing query contract has a pinned published cut and best-effort live sources; a successful query is not a new universal write-visibility guarantee. |
| Server audit/capture/results/tenant-span siblings → one outbox → local or peer Scribe | `scribe_outbox.rs::ScribeRoute`, `ScribePeers::channel/submit`, `ScribeSink::encode/write`; `grpc/capture_peer.rs` | Existing outbox retains process-memory slices, hashes both attribution members, submits locally or over authenticated peer transport, rotates ready peers after retryable failure, and counts terminal loss. These owners are not replaced by the OTLP hash correction. |

## Failure and recovery matrix

| Credible failure | Affected capability and failure boundary | Recovery and proof assessment |
|---|---|---|
| Python stock batch export blocks past the configured SDK flush bound | Explicit state lifecycle returns shared 504 after the managed wait; native drain still runs. At most one daemon flush worker remains; unrelated framework execution and span callbacks continue. | `otel.py::_ManagedTelemetry:217–268` holds one in-flight Future, waits on one monotonic deadline and permits recovery once it settles. No thread cancellation or persistent client queue is claimed. `test_managed_telemetry_flush_deadline` holds actual stock export, asserts timeout, native secondary code, one worker, continued spans and recovery. |
| A later Python flush/shutdown begins after an old flush drained, before that old Future settles | The newer call cannot report the old barrier as its own success. | Entry snapshots `_started`; only a subsequently started operation covers the call. Older waiting and the fresh flush share the deadline. `_stale_flush` ends the late span before public invocation, waits until that invocation is waiting, then releases the old operation and requires the new span to be exported. Prior FIND-7 remains closed. |
| Same-state setup overlaps or optional import/installation fails | Setup may wait or return an explicit setup error; it must not reject its own successful provider or take down application execution. | GIL-aware setup lock plus recheck, failure leaves cell empty, losing global candidate is shut down. TS's post-await check closes its corresponding window. Existing setup/retry/conflict tests target these branches. Prior FIND-6/FIND-8 remain closed. |
| OTEL false result, timeout exception or rejected flush, including simultaneous native failure | Only explicit lifecycle fails. Native drain remains attempted; application-owned providers are untouched. | Python false/TimeoutError → 504; other exception → 503. TS maps rejection without copying exception payloads. `TelemetryFlushFailure::settle` preserves native secondary code and native-only results. Both runtime unit suites exercise settlement; actual denied export remains journey coverage. |
| Collector outage, admission pressure, dependency outage or export authorization denial | Affected export is refused/retried by stock policy; model success remains model success. Server refusal is request/component-local, not deliberate shared-process termination. | Existing OTLP 429/resource-exhausted and typed errors retain the admission boundary. No new retry loop or durable SDK spool was added. Python's upstream stock flush can return true after logging exporter refusal; documented behavior is not treated as acknowledged persistence. Denied-export journeys query absence of forbidden rows. |
| Lost response after Scribe accepts an identical OTLP export; retry through the other transport | Same publisher pair, table, logical content and accepted correlation derive the same ID despite request ID/token changes. A distinct actor must retain its own row set. | R3 hashes both verified attribution members before stamping. Existing memory/durable fence remains the sole duplicate owner. Unit regression varies actor only; real `negative.rs::identical_exports_from_two_principals_each_keep_their_own_attribution` repeats direct and two delegated pairs through gRPC/HTTP and groups current subject/actor/Run columns, requiring complete row sets without replay duplication. Prior FIND-9/FIND-12 are source-closed. |
| Cancellation after Scribe dispatch; WAL/SQL ambiguity; restart | Request may fail without knowing whether durable work completed. Shard retains dispatched or ambiguous work; an unready Scribe refuses new ingest. | `ingress.rs:512–595`, `shards.rs::commit_batch_control_fences/resolve_replay_fence` and `replay.rs` retain exact batch/digest/WAL identities and reconcile SQL. Existing ACK boundary remains fsync + fence + authoritative rows. R3 does not rederive IDs during WAL replay or change disk ownership. No new crash-loss guarantee is inferred. |
| WAL fault in a shared server process | Scribe readiness closes; its durable fence is withdrawn. Unrelated selected roles are not intentionally crashed. | `state.rs::ScribeWalFaultMonitor::run:784–801`, `Scribe::is_ready/activate:1009–1076` make the boundary explicit: local ingest closes first; failure to withdraw logs; faulted Scribe remains unready until restart, while other peer roles may activate. R3's request-local identity change introduces no process fail-stop. |
| Peer Scribe disappears or stalls; rolling replacement of a peer | Internal outbox slice waits only for peer timeout, then retries the identical identity via a ready peer. Queries retain their existing frozen-roster/degradation behavior. | `scribe_outbox.rs:435–552,832–891` selects Scribe capability, refreshes endpoints and bounds each peer call. Durable Scribe fences suppress identical accepted IDs. `state.rs` drains role owners before executor release. Candidate hash is deterministic across candidate replicas/restarts. Cross-version hashing equivalence or uninterrupted rollout is not an approved added contract: this hash differs from earlier intermediate candidates, and this review does not claim those versions converge on one implicit ID. No dual-write, migration or compatibility mechanism is proposed. |
| Abrupt application exit / process kill / expired shutdown deadline | Unexported stock batches, client queues and unacknowledged server outbox memory can be lost. | Managed shutdown keeps the global provider usable rather than shutting it down. Native ambiguous shutdown retains its writer for same-state retry. Existing documented durability boundaries remain; no disk spool or crash-proof client delivery is required. |

## Cumulative obligation assessment

REQ-010/011/014 and AC-007/010 retain framework-owned execution, optional
correlation, sampling and application results across the failure paths above.
REQ-012/013 and AC-008/009 retain ownership, current-call flush barriers,
bounded provider waits, drain ordering and continuation. REQ-016/017 and
AC-001/011/013 have real framework/public-authenticated-server readback source,
including live-process handshake before continuation, owned-provider control,
denied export, tenant rejection and exact missing-credential code. The Python
`app.py` boundary now emits the caught public `WyrdError.code` and rethrows;
its parent requires `WYRD_CLIENT_401_NO_CREDENTIALS`, preserving positive
gateway/key isolation proof. INV-001–004 remain intact at inspected runtime
seams. REQ-015/AC-012 and AC-003 verification are assessed only insofar as they
describe these lifecycle/recovery contracts; repository-wide documentation and
execution certification belong to their owning review lanes.

Managed real journeys explicitly read back after flush while the producer is
still alive (Python `test_framework_telemetry.py:148–179`, TS integration
`:235–257`), continue native/framework work, end the held span later and export
again after state shutdown through the same global provider. Healthy-path
readback alone is not used to claim timeout or interruption proof.

## Proposed findings and limits

Proposed finding set for independent validation: **empty**. No additional
failure owner, retry policy, health coupling, migration, queue or durable
guarantee is needed by the inspected approved paths.

No tests, builds, installs, live outages, restart drills or broad gates were
run by this reviewer; the root owns current execution evidence. Historical
GREEN records are not independently rerun attestations. Source-level recovery
assessment preserves the existing query visibility, idempotency-retention and
unacknowledged-memory-loss limits described above.
