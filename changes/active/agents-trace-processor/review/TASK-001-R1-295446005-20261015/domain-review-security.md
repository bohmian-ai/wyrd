# Security domain review

Result: **PASS**. No material security findings identified.

Intended artifact: `changes/active/agents-trace-processor/review/TASK-001-R1-295446005-20261015/domain-review-security.md`.

## Subject and scope

- Base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`.
- Candidate: `295446005b18604030bf376de6209098a28886f7`.
- Prior candidate: `692d390dc40e43c6e1d7e25c7850d5c826d26a9e`.
- Authority: approved specification revision 2, original TASK-001, prior verdict/validation and R1 remediation.
- Boundary: authenticated telemetry publisher, tenant/Card attribution, selected shared-client credentials and token freshness, Python flush thread handoff, bridge payload mapping, lifecycle diagnostics, provider credential separation, and real negative journeys.

This review inspected the cumulative security boundary and R1 changes. It adds no authorization, redaction, credential, or retry subsystem.

## Authority and source coverage

| Obligation and authority | Source inspected | Assessment |
|---|---|---|
| Verified credentials select tenant/publisher; Card assertions remain server-authorized. Spec INV-001; AGENTS §§2/9/10; security-posture trust/authorization/tenant rules; telemetry-observations identity rules | Server `http/middleware/authenticate.rs:64`, `http/otlp.rs:241,531`; Gate `gate/mod.rs:823`, `gate/attribution.rs:86`; `tables/signal.rs:580`; `scribe/ingress.rs:65` | PASS. Caller-derived auth precedes ingest. Gate checks table write permission and resolves unbound Card assertions through the authenticated tenant’s registry. Projection checks each asserted Card against authorized members; Scribe rejects tenant/principal disagreement. |
| Reuse the selected client and shared refresh owner. REQ-017 and INV-001; wyrd-design API-key/exporter contract | Python `src/state/mod.rs:181`, `src/client.rs:118`, `python/wyrd/otel.py:191`; shared `client.rs:438`, `auth.rs:841`; TS `src/index.ts:1524,2924`, `src/otel.ts:25` | PASS. State setup retains its selected client. Export requests obtain current tokens through the existing shared auth middleware. No setup-time bearer snapshot, extra token cache, or Card-derived identity selection appears. |
| R1 thread handoff preserves identity and credential ownership | Python `otel.py:216–286`; `src/state/mod.rs:506`; `src/trace.rs:91`; existing exporter session/auth closure | PASS. The new worker invokes the retained provider’s flush. Its stock exporter still holds the same client and requests tokens per export. Moving lifecycle waiting to a daemon thread does not re-resolve ambient credentials or propagate Run/Card attributes as authority. At most one attempt remains in flight per managed handle. |
| Bridge mapping excludes sensitive content and raw error payloads. REQ-010, INV-003; telemetry-observations payload rules | Python `otel.py:340–497`; TS `index.ts:2490–2709`; Python callback test around `test_observe_surface.py:995–1100`; TS callback tests | PASS. Mapping reads names, model and usage fields, with fixed error type/status. R1’s Python Responses change reads direct safe usage with per-count fallback; it does not require or export response content. TS structural callback types retain explicit field selection and primitive attribute filtering. |
| Lifecycle error projection stays scrubbed. REQ-013, INV-003; secret-handling rules | Python `src/state/mod.rs:506`; shared `state.rs:2088`; TS `index.ts:2638`; native `cards.rs` lifecycle projection; error catalog | PASS. Provider exception messages and error payloads do not enter newly projected lifecycle errors. Causes contain exception type/name information; secondary native failure detail is its stable code. |
| Wyrd credentials stay separate from external model credentials. REQ-017; security-posture credentials/privacy rules | Python framework builders and `gateway()` in `frameworks/app.py`; TS direct/gateway builders; existing Python `_http_auth.py`; TS `WyrdClient.httpAuth`; Observe recipe additions | PASS. Direct-provider builders use independent local provider keys. Wyrd auth hooks are attached to Wyrd gateway URLs. Documentation explicitly distinguishes provider keys from `WYRD_API_KEY` and retains ambient Wyrd setup. |
| Query/readback inherits authenticated tenant authority and uses bound inputs | Shared `bifrost/query.rs:205`, `bifrost/facade.rs:834`; server `query/service.rs:179`; Oracle `AuthorizedQueryContext::try_new` | PASS. Trace/run values are parameters. Query context checks principal/tenant equality and enters the existing authorized query path. No read-authorization change is introduced. |
| Dependency boundary | Python manifest/lockfile, Cargo ABI floor, TS optional peer manifest/lockfile changes | No concrete supply-chain issue identified from the inspected changes. Framework support remains opt-in for production consumers; lockfiles retain registry/integrity metadata. This is not an advisory scan. |

Applicable authority read included agent rules, reference router, spec-driven development, maintainer style, telemetry-observations, security posture, and Wyrd design’s runtime identity/exporter and observation-identity sections. CodeGraph was consulted before source searches. No other current discovery reports were read.

## Negative proof assessment

- Python `test_framework_sampling_privacy_and_authorization:233` exercises all three actual frameworks. Viewer credentials persist no spans; another tenant’s credential cannot attribute spans to the original Card; readback checks both tenants. Prompt and tool-result sentinels must be absent.
- TS `framework sampling privacy and authorization` checks equivalent denial/privacy cases. Its foreign-tenant credential may store its own uncorrelated request span, while original-tenant Card assertions are rejected. Original-tenant readback stays empty.
- Python freshness test at `test_observe_surface.py:933` and TS `exporter token freshness` send two HTTP exports and require successive tokens, proving per-export retrieval without duplicating renewal logic.
- Both gateway journeys assert the upstream receives the configured provider key and no `x-wyrd` headers. Missing ambient credentials fail using isolated configuration.
- R1’s real `ResponseSpanData(response=None)` callback case asserts safe direct usage, including zero, without content.
- Committed task/R1 execution records were treated as claims and checked against source assertions. No tests were rerun by this reviewer.

## Security Audit

### Critical

None identified.

### High

None identified.

### Medium

None identified.

### Low / Defense In Depth

No in-scope change recommended.

### Positive Controls

- Tenant and publisher authority remain independent of mutable telemetry correlation.
- Signed Card scopes and tenant-registry resolution remain server-owned.
- Exporters reuse stock transports and the shared credential refresh owner.
- R1’s worker retains the existing provider/client rather than selecting another identity.
- Bridge mapping and lifecycle diagnostics exclude raw content/error messages.
- Real server journeys cover denied export, tenant separation and credential separation.
- Recipes disclose native instrumentation’s residual content and OpenAI exporter registration semantics.

## Limits and final result

**PASS** for the assigned security boundary, with an empty material finding ledger. No unresolved security question requires remediation. Final HEAD remained the candidate; tracked source was unchanged.

Skipped source/report edits, builds, tests, live services and advisory scans. Native instrumentation can still export content it emits, as the approved contract and recipes disclose.
