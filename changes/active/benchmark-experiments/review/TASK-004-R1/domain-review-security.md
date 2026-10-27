# TASK-004 security domain review

Result: **FAIL** — one proposed finding, `SECURITY-001`.

## Subject and boundary

Reviewed immutable range `0b500d677...a020500e8ea0c39311d508d6b5bfa85ebe90368c` in `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`. HEAD equals the candidate and there is no working-tree diff under `crates/` against it. This is first-review discovery, with no carried-forward TASK-004 acceptance. No other review reports or intended verdict were read.

Boundary: benchmark client classification through HTTP/gRPC request context, timing registry, local snapshot, and benchmark invalidity/report consumers; snapshot startup exposure on API and Forge targets. This is not a general deployment-hardening audit.

Authority: supplied root AGENTS instructions; `.agents/skills/wyrd-task-review/SKILL.md`; `changes/spec-driven-development.md`; original TASK-004 and revision-8 specification (approval accepted as instructed despite stale metadata); `architecture/README.md`; `architecture/wyrd-security-posture.md` authorization/request-principal and audit/privacy sections; relevant maintainer/Rust guidance. Scope map establishes no nested AGENTS applies to changed paths. REQ-007 explicitly requires authenticated receiving-boundary propagation, ordinary deployments ignoring metadata, closed classifications, bounded aggregate readout, and loopback-only enabled readout. INV-001 preserves authentication/tenant/permission behavior; AC-012 requires snapshot privacy and exposure refusal.

## Coverage and acceptance

| Obligation | Implementation and consumer evidence | Verification evidence examined | Result |
|---|---|---|---|
| Snapshot absent normally; labels ignored normally | `MetricsConfig::default` sets `benchmark_snapshot=false`; `metrics_router` returns before adding the route/enabling timings; `timing::label_request`, `record`, and `record_scan` return when disabled | Router opt-in test; telemetry disabled-before-enable assertions; implementation-reported V3/V7 | PASS |
| Enabled snapshot rejects non-loopback metrics bind | `WyrdServerConfig::load` applies overrides then validates; `validate` rejects snapshot plus disabled metrics/non-loopback resolved bind. The check is outside `serves_api`, covering Forge as well. `app::run` loads before either target branch; `WyrdServer::bind` and `run_forge_worker_process` both use `MetricsConfig::resolved_bind`. Default resolution always chooses `127.0.0.1`; an HTTP ephemeral port changes only the derived port | `config::tests::benchmark_snapshot_requires_a_loopback_metrics_listener` source checks wildcard refusal, disabled refusal, loopback acceptance | PASS for actual process startup paths |
| Readout is read-only and reveals bounded aggregates, process identity and monotonic time only | `app/metrics.rs:373-382` mounts a GET JSON snapshot without selectors or mutation. Registry snapshot constructs fields solely from static timing keys, closed labels, histogram buckets, counters, generated process identity and pid/time. No tenant/caller/SQL/payload/credential field is copied. Query strings are unused, not reset/selection operations | `benchmark_snapshots_are_local_read_only_and_opt_in` checks absence, mutation refusal, no secret span attribute in output, cumulative repeated reads and recorder preservation | PASS |
| Bounded labels, storage and output; raw unknown values never enter diagnostics | `Labels::label` selects static `CASES`/`PHASES` values or `invalid`; `Registry::record/scan` cap series at 128/32. HDR ranges/precision bound bucket cardinality; `Buckets::of` serializes nonzero buckets. `Population::from_interval` formats only static labels/counts. Runtime timing callers use closed boundary/outcome spellings; `record_scan` receives static terminal outcome from Oracle | `labels_are_closed`, `series_beyond_the_bound_are_dropped`, snapshot/privacy test and telemetry concurrent interval test source | PASS |
| Benchmark classification takes effect through authenticated receiving context | Client `transport/trace.rs` scopes its case header without changing credentials. However HTTP `request_span` and tonic `request_span` activate classification before authentication, and unknown labels immediately mutate the global invalid counter even for public health routes | Existing tests call `label_request` on synthetic spans and do not test a real unauthenticated receiving path | FAIL — SECURITY-001 |
| Metadata cannot select tenant, storage, permissions or sampling | New labels reside only in private span extensions and are read only by timing/scan recording. No changed authentication/authorization predicate consumes them; HTTP protected-router composition and OTLP/native authentication remain owners. The snapshot itself has no application route/tenant selector | Source trace of router auth layers, OTLP authentication and recorder consumers; existing implementation smoke evidence is reported, not independently rerun here | PASS for authority/data access |

Additional surrounding source traced: `LocalServer::serve/timings/operator`, `Population::collect/from_interval`, shared client `headers`, HTTP `build_router/request_span`, middleware `authenticate_on`, browser and OTLP successful-auth branches, tonic `build_grpc_router/request_span`, gRPC OTLP `TraceOtlpGrpcService::call/timed`, query `QueryTiming`, Gate `record_gate_request`, Gateway `Telemetry::point`, verification `ExecutionTelemetry::emit`, and Oracle `QueryTelemetry::finish`. Their new timing keys do not contain request or tenant identity. The existing Prometheus recorder/listener lifecycle remains the owner; no new authentication endpoint or recorder was added.

## Proposed finding

### SECURITY-001 — Unauthenticated metadata invalidates authenticated benchmark evidence

Classification: **VIOLATION** of REQ-007's authenticated receiving-boundary classification and TASK-004's authenticated request-context requirement.

Locations:

- `crates/shared/wyrd-telemetry/src/timing.rs:173-194`, especially unconditional invalid-label accounting at lines 185-186.
- `crates/wyrd/wyrd-server/src/http/router.rs:203-206` and the outer `TraceLayer` in `build_router` at lines 165-172.
- Sibling producer: `crates/wyrd/wyrd-tonic/src/server/mod.rs:247-250`.
- Observable consumer: `crates/wyrd/wyrd-testing/src/capacity.rs:608-613`.

Source-proven scenario: enable the benchmark readout, take the opening snapshot, then send `GET /healthz` to the application listener with `x-wyrd-benchmark-case: unknown`. `build_router` deliberately mounts health without authentication, yet its outer TraceLayer calls `request_span` for the request. `label_request` increments `invalid_labels` immediately; it neither waits for authentication nor requires an operation to reach any timed boundary. The same happens for a protected-route request that authentication subsequently refuses. The closing snapshot therefore has an invalid-label delta. `Population::from_interval` turns that delta into an invalid-measurement reason for otherwise valid offered work.

This is an evidence-integrity boundary failure, not an application authentication bypass or a tenant-data leak. No credential, legitimate workload request, privileged endpoint, or speculative deployment arrangement is needed to alter the experiment's validity. Enabled benchmarks explicitly require authenticated context to propagate their classification; unauthenticated/public incidental traffic currently changes the authoritative measurement state before that boundary. Normal deployments remain unaffected because recording is disabled.

Sibling tracing: tonic's server trace function also activates labels before service authentication. The OTLP timed wrapper surrounds authentication and can additionally record refused exports under the pre-auth classification. HTTP bearer, browser-session and OTLP API-key successful-auth branches already establish verified context; Gate and Gateway retain their own existing authenticated owners. A correction must cover these receiving paths together rather than special-case `/healthz` or suppress the downstream invalidity check.

Smallest required outcome: activate benchmark classification and its invalid-label accounting only after the existing receiving authentication owner has established the verified request context. Pre-auth span setup may remain, but an unauthenticated or public request must not activate benchmark labels or increment this measurement-invalidity counter. Reuse the existing HTTP/gRPC/Gateway successful-auth context owners; preserve their refusal responses, audits, tenant selection and existing operational timing. Keep authenticated unknown classifications counted and invalid, with raw values discarded. No second registry, new permission, endpoint, or deployment control is needed. Moving the one counter alone into generic recording is insufficient unless that recording is also guaranteed to follow authentication, because OTLP `timed` includes authentication failures.

Focused closure proof: with real router/context composition and timing enabled, compare before/after snapshots around an unknown-class unauthenticated health request and a protected request lacking credentials; assert unchanged invalid-label counter and no activated benchmark series. Then authenticate the same metadata on a timed supported route and prove the existing counted-invalid behavior. Cover the analogous gRPC authentication-refusal path. These are receiving-boundary checks, not a new benchmark suite.

## Verification limits and excluded proposals

Independent verification here was static source/diff/caller analysis using CodeGraph before code search, with shell commands through mise. No tests, builds, benchmarks or synthetic load were run, as assigned. Existing task V1/V3/V7/V8 results are implementation-provided evidence; test bodies were inspected where relevant, not independently executed. No claims are made about unrelated credential, RLS, durable-storage or production-export behavior.

No additional finding is proposed for public in-process router/server composition accepting an externally supplied unvalidated config: actual binary startup loads and validates before API/Forge binding, and an arbitrary embedding bypass was not established as a supported deployment regression. No TLS, metrics-auth, SSRF, anti-abuse or larger output-hardening requirement was invented for this explicitly local benchmark readout.

Skipped runtime checks by assignment; remaining risk is the source-proven unauthenticated influence on benchmark validity described above.
