# Security domain review — TASK-004 second remediation

**PASS. Material proposed finding ledger: empty.**

## Subject and authority

Prior candidate: `c9d72729b85187f2399725e0ed9496c4c4a39298`. Candidate/checked HEAD: `0bf0cb8662a221233a963ed147d57ef0cbb6720f`. Reviewed actual remediation diff, merge `9a540b7b8` against both parents, and postmerge classification change `2f312af5c`.

Working-tree authority: `changes/active/benchmark-experiments/spec.md`, revision 9, SHA-256 `007d27e3a0e22a4aa912108da6a7478849a3f2f56c953fdea9e1491c65e96dce`. Scope supplies approved revision-8 behavior plus maintainer-directed revision-9 smoke without a stale-header blockade. Current REQ-007 lines 245–254 explicitly describes arrival-time classification, no tenant/storage/sampling authority, ordinary deployment ignoring, and counted invalidity for unknown labels. The orchestrator confirmed there is no later explicit direction retaining authenticated-only classification besides the older R2 preservation instruction.

Read original TASK-004, R1/R2 remediation and historical verdict/validated ledgers, supplied root instructions, task-review skill, spec-driven authority order, maintainer guide and applicable security-posture principles/trust boundaries. No nested AGENTS.md applies to these production paths. Imported PR215 context: merge conflict decisions and existing TASK-003 R2/TASK-007 R1 verdict/verification evidence under `changes/active/service-identity-and-telemetry/review/`. Imported acceptance is carried forward, not a new full schema audit. No reports from other reviewers in this round were read.

Assigned scope: closed benchmark classification at HTTP/gRPC receiving boundaries and its interaction with authentication, tenant attribution, refusal, timing and audit owners. TASK-005 qualification and unrelated imported schema behavior remain excluded.

## Authority and source coverage

| Obligation / boundary | Implementation evidence | Verification evidence / result |
|---|---|---|
| REQ-007: finite classification ignored ordinarily; no authority from labels | `crates/shared/wyrd-telemetry/src/timing.rs:65–105,139–218`: finite case/phase lists, static labels, disabled early return, nearest ancestor lookup; raw input never becomes a registry key. `Registry::record`/`scan` bound series and count overflow. | Existing closed-label, bounded-series and `benchmark_histograms_preserve_bounded_intervals` tests inspected. PASS. |
| Classification at arrival context | HTTP `crates/wyrd/wyrd-server/src/http/router.rs:185–207`; gRPC `crates/wyrd/wyrd-tonic/src/server/mod.rs:228–250`; `timing.rs:173–196`. Unknowns increment an aggregate and become static `invalid`; timing inherits request labels. | Source trace and telemetry test cover accounting/ancestor classification. PASS under current REQ-007. |
| Authentication/refusal preserved | Full bearer/browser/OTLP middleware in `wyrd-server/src/http/middleware/authenticate.rs:113–228`, principal extractor `components/auth/principal_extractor.rs:129–152`, Gateway ingress `components/gateway/ingress.rs:130–174`. Removed only classification activation and its docs/tests. Same-origin enforcement, token/key verification, workload selection, principal insertion and refusal returns remain. | Actual pre/postmerge diffs and full owner bodies inspected. PASS. |
| Gate verified tenant/context preserved through merge | `crates/vala/vala-bifrost-redux/src/gate/auth.rs:74–101,198–235`; Gate `mod.rs:702–758`. Fallible environment parsing precedes tenant route binding; tenant/subject/actor derive from VerifiedToken. Readiness and metadata authentication remain. | Against PR215 parent, merge added classification activation; later removal leaves imported context handling intact. PASS. |
| gRPC refusal timing does not grant admission | `wyrd-server/src/grpc/otlp.rs:188–239`: authentication precedes codec/unary construction, refused exports still produce failed timing. | Producer-to-consumer source trace; failure labels do not manufacture success or insert a principal. PASS. |
| Snapshot remains opt-in, local, read-only and aggregate | `wyrd-server/src/config.rs:1699–1711,3417–3426`, `app/metrics.rs:364–384`, `app/server.rs:727`: default false, exposed/disabled metrics bind refused, GET-only aggregate snapshot without tenant/query/reset selector. | Existing loopback configuration and `benchmark_snapshots_are_local_read_only_and_opt_in` tests inspected. PASS. |
| Raw OTLP constructor preserves credential | `wyrd-testing/src/bin/capacity/query/run.rs:686–700` and callers: existing bearer preserved, scoped `benchmark_headers()` added, conversion errors returned. No replacement transport/auth owner. | Existing constructor regression inspected; parent independently ran scoped testing regressions. PASS. |
| Invalid evidence cannot silently become favorable | `wyrd-testing/src/capacity.rs:710–725`: invalid-label count or dropped series becomes explicit invalid evidence; timing outcome remains independently server-generated. | Source trace to validity consumer. PASS. |

## Prior FIND-TASK-004-4 reassessment

The old scenario remains reachable in enabled benchmark mode: a public request or subsequently refused protected request with an unknown benchmark header increments `invalid_labels` at arrival. A closing measurement interval becomes invalid. This follows directly from outer HTTP/gRPC span construction, `label_request` and `Population::from_interval`; this review does not claim the old authenticated-only closure test still passes.

The prior finding's specific authenticated-activation obligation and selected Pending/activate_request mechanism are superseded by the supplied current arrival-time contract. Security posture still requires authentication for application operations and verified credentials as tenant authority. Those controls remain at the same owners. Classification selects a finite measurement series; it cannot authorize execution, choose another tenant, change audit decisions, select sampling, write durable data or expose sensitive identity. Unknown input invalidates evidence rather than creating favorable measurements.

A party reaching the application listener of an explicitly enabled benchmark process can therefore disrupt its measurement. The loopback restriction protects the readout, not ingress; it does not establish authenticated provenance for labels. Ordinary deployments return before accounting. Under the current finite, opt-in measurement contract, this is not a demonstrated material production security regression or an unmet authenticated-only measurement obligation. Requiring successful-auth activation again would reinstate the superseded mechanic. No remediation is proposed.

F4 is **not retained as a current finding**, rather than marked as still satisfying its old closure proof. F6 is unaffected. Removal of Pending also removes F5's tuple-field documentation target.

## Security Audit

### Critical

None.

### High

None.

### Medium

None within assigned scope.

### Low / Defense In Depth

No material recommendations retained. Existing HTTP/gRPC request-span rustdoc and the timing-evidence description in `capacity.rs:573` still mention authenticated activation. These are stale descriptions, not changed authorization behavior or newly missing item documentation. Nonblocking documentation observations do not warrant another security remediation round.

### Positive Controls

- Closed static labels and hard series limits prevent arbitrary cardinality and raw secret strings in snapshots.
- Disabled collection ignores metadata; production authentication is independent.
- Bearer, API-key, browser-session, Gateway and native-ingest owners still verify before executing protected operations.
- Verified principals retain tenant authority; benchmark labels never enter identity/permission resolution.
- Imported malformed environment handling remains refused before tenant-span binding.
- Unknown metadata, cohort mismatch and dropped observations remain invalid evidence.
- Readout default-off/loopback validation and existing recorder/listener lifecycle survive.

## Proof limits and result

Used CodeGraph first, actual Git diffs/source, both merge-parent comparisons and existing test/evidence inspection. `git diff --check c9d72729b 0bf0cb866` passed. Candidate HEAD was checked unchanged. No source edits; only this report was written. Parent reported 46 focused testing tests and scoped fmt/clippy green; those are not independent security-review reruns.

No runtime suites, network attack exercise, full imported schema audit, cross-language security journeys or full-size qualification were rerun. Residual limit: unwanted traffic reaching an explicitly enabled benchmark application's listener can invalidate its measurement; ordinary deployments ignore the headers.

**Overall: PASS. Smallest correction: none.**
