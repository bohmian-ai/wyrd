# OTEL domain review

Result: **FAIL** — one proposed merged-main attribution regression, `OTEL-9`.

## Immutable subject and scope

Reviewed cumulative base `dac4c340f835676adc29f3dcc01f98e1d3d51465` to candidate
`d088519ea846aba8b2a77c24b2af2491ad2b3936`, including the authorized main merge.
HEAD was that candidate at entry and at the final source check. No current peer
reports were read. This report is the only file this reviewer writes.

The domain is framework callbacks → global OTEL provider → stock authenticated
OTLP export → accepted signal attribution → SDK readback, and managed versus
application-owned lifecycle. Imported main was inspected at that seam; this is
not an audit of its unrelated identity, workflow, catalog, or maintenance work.

## Authority and dependency grounding

Applied approved spec revision 2, original TASK-001, repository AGENTS.md supplied
in the review context, `architecture/agent-rules.md`, the task-review skill,
reference router, spec-driven-development and maintainer-style references,
`domain/telemetry-observations.md`, Wyrd design's authentication/managed-provider
and Observation identity sections, Bifrost design's system/table/row identity
sections, and docsite CONTRIBUTING/PRODUCT rules. These require optional exact
correlation, authenticated subject/actor attribution, native instrumentation,
explicit provider ownership, safe callback data, and ordinary OTLP interoperability.

Dependency-sensitive behavior was checked against installed sources matching
the pinned dependency records: Python OpenTelemetry 1.42.1, OpenAI Agents 0.23.1,
Pydantic AI 2.56 and ADK 2.12; TypeScript Agents 0.20 and the OTEL package versions
declared in package.json/pnpm-lock. In particular Python's installed
`opentelemetry/sdk/_shared_internal/BatchProcessor.force_flush` explicitly ignores
its timeout and exports synchronously, justifying the narrow managed wait bound.
Python `ResponseSpanData.usage` is populated independently of captured response
content; TS Responses places full response metadata in `_response` only when
its framework tracing setting permits it. Neither bridge should invent absent
provider identity or counters.

## Coverage and proof assessment

| Boundary / obligation | Source inspected | Proof inspected / result |
| --- | --- | --- |
| Explicit optional bridge, parent fallback, timing, late metadata, safe errors, duplicate callbacks and shutdown (REQ-010) | Python `otel.py::WyrdAgentsTraceProcessor`, `_nanos`, `_span_name`, `_gen_ai`; TS `index.ts::WyrdAgentsTraceProcessor`, `agentsTime`, `agentsSpanName`, `agentsGenAi`, structural callback types | Both callback lifecycle unit bodies cover late model/usage, unknown parent fallback, enclosing OTEL parent, timestamps, payload sentinels, duplicate starts/ends, interruption and late callbacks. Python thread test exercises shared callbacks. PASS source assessment. |
| Native framework emission with no authored internal spans (REQ-011/016) | Python framework app's Pydantic/ADK/OpenAI builders, streaming and failure paths; TS real Chat Completions agent/tool setup | Python real framework matrix asserts two model spans, one tool, exact usage and Card/Run, ancestry; TS asserts available model data and documents Chat Completions' absent span usage. PASS source assessment; execution is root-owned. |
| Managed setup and application provider ownership (REQ-012) | Python `_start_telemetry`, `install_run_correlation`, PyWyrdState setup lock/OnceLock; TS `startTelemetry`, WyrdState's post-import recheck, correlation factory | Setup tests include overlapping same state, other state, existing application provider, Python retry/lost race; owned-provider tests retain resources and sampling. Failed global install shuts down only the losing provider. PASS. |
| Per-call bounded managed flush, native drain on failure, provider continuation (REQ-013) | Python `_ManagedTelemetry.force_flush/_flush`, state flush/shutdown/error projection; TS `#flushTelemetry`, `#settle`, flush/shutdown | Python one monotonic deadline covers previous-attempt wait and a flush started after current entry; `_started` prevents old completion satisfying later calls. Held stock exporter test includes repeated timeouts, one worker, stale earlier flush for both lifecycle methods and continued active spans. Real live journeys read before app shutdown, write again, and export after state shutdown. PASS source assessment; closes the inspected stale-flush/timeout hypotheses. |
| Start-time Run/Card, concurrency, sampling and content (REQ-014) | Python correlation processor/context stack and TS Run.scope/correlation processor; bridges keep start contexts, read end metadata only | Native-framework tests cover nested/concurrent/uncorrelated cases, errors, streaming, sampling and content sentinels; callback tests directly separate start and end scopes. Application providers remain caller-owned. PASS. |
| Ambient credential selection and fresh per-export auth (REQ-017) | Python stock exporter factories/session auth and TS stock exporter config header callback, framework gateway construction | Both factories ask the selected shared client for its token per export, without a new cache/transport. Freshness tests use real local HTTP receivers. Gateway journeys exercise stock http_auth/httpAuth and verify upstream receives its own provider key with no Wyrd headers. PASS. |
| Executable documentation prerequisites (REQ-015) | Observe framework install table, complete builders, readback, owned-provider and lifecycle/error sections; Connect/Understand links | R2 TypeScript install command now supplies logs and metrics exporters as well as traces because `otel` imports all three. Recipes construct tools/models and identify ambient Wyrd credentials, separate provider credentials, versions, privacy residuals, sampling, add-vs-replace and both lifecycle codes. Draft retained. PASS source assessment. |
| Merged signal publisher and tenant seam (INV-001, Observation identity) | HTTP OTLP Caller→AuthContext, Gate's three signal entry points and `dispatch_canonical`, managed columns, Scribe ingress/decode/stamping and fence suppression; SDK timeline consumers | Trusted tenant and attribution reach Scribe; Card/Run remain optional record values. Subject/actor stamping is consistent, but the shared OTLP deterministic batch-id producer still binds only the subject. FAIL: OTEL-9. |

## Proposed material finding

### OTEL-9 — deterministic OTLP retry identity omits the newly distinct actor

Classification: **REGRESSION**. Proposed new stable ID: next available ID from 9
(or consolidate with an independently discovered identical cause).

Obligation: approved INV-001 and the active Wyrd/Bifrost Observation identity
contract require accepted generic and correlated signals to retain authenticated
subject and actor. Existing OTLP retry suppression explicitly promises that two
authorized publishers do not collapse onto one fence.

Location: `crates/vala/vala-bifrost-redux/src/gate/mod.rs:974` derives the batch ID
with `otlp_batch_id(auth.tenant, auth.principal.id, ...)`; the changed dispatch at
line 988 separately forwards `auth.attribution()`. The existing helper at line
129 hashes only one PrincipalId. Main's changed
`gate/auth.rs::AuthContext::attribution` and
`wyrd-runtime/src/request_context.rs::delegated_attribution` make that principal
the subject while the actor may now differ.

Reachable path and consequence:

1. Two authorized delegated publishers act for the same subject in one tenant,
   with different current actors, and submit identical accepted OTLP content.
   This includes forwarding/re-exporting the same signal batch under distinct
   publisher authority; the direct subject and a delegated actor also differ.
2. All three signal ingest methods project through the same dispatch helper.
   `tables/signal.rs::projected_signal_schema` adds only Card and Run columns,
   so neither logical nor correlation digest can bind the authenticated actor
   before Scribe stamping. Tenant, subject, table and both digests are equal.
3. Scribe's `execution_lanes.rs::managed_arrays` correctly stamps different
   `act_principal_id` values, but `preprocess.rs::logical_data_identity` excludes
   all managed columns. The fence's logical digest does not distinguish them.
4. `shards.rs::commit_batch_control_fences` and spent-batch settlement classify
   the second identity as already committed and suppress its insertion. The
   second export can succeed while its actor attribution disappears; only the
   first publisher's retained evidence remains. This is an ingest identity
   source defect, not an SDK reader or exporter defect.

Smallest correction: reuse the existing trusted `Attribution` value at
`dispatch_canonical` and have the existing deterministic OTLP batch-id owner
bind both its subject and actor IDs. Keep tenant/table/content/Card/Run inputs,
same-pair replay convergence, existing Scribe fences, and every native Arrow
batch contract unchanged. Do not add an identity registry, exporter queue,
client identity field, or downstream guard. The shared dispatch fixes traces,
logs and metrics together.

Remaining semantic uncertainty for independent validation: this diagnosis uses
the existing helper's explicit publisher-separation promise and the new actor
as the authenticated emitting party. If approved authority instead deliberately
defines re-export under a changed actor as the *same publisher attempt* whose
original actor must win, the proposed finding must be rejected rather than
inventing new replay semantics. I found no such exception in the inspected
Observation identity or Bifrost table/row identity authority. No test was run
to resolve this product-semantic question.

Focused closure proof: extend the existing OTLP derived-identity regression to
show same tenant/subject/content with different actors yields distinct IDs,
while repeating the exact subject/actor pair converges. Exercise accepted OTLP
through the existing server ingest fixture under two delegated actors for one
subject and read back the two correctly stamped publisher identities; replaying
either pair must not insert it twice. Root/validator should select the existing
smallest owning test fixture; no new test harness or dependency is required.

## Limits

No build, test, install, staging, source, index, or HEAD mutation was performed.
Committed execution evidence was treated as a claim; fresh runtime verification
belongs to the root's `verification.md`. The finding is source-traced, not a
claimed executed reproduction. Imported main's unrelated features and live
provider/cloud infrastructure were not audited. No optional improvements are
proposed.
