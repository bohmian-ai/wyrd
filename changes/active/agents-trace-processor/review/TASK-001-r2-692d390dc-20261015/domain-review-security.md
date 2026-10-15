# Security domain review

Result: **PASS**. No material security finding in this domain.

## Immutable subject and boundary

- Base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`.
- Candidate: `692d390dc40e43c6e1d7e25c7850d5c826d26a9e`.
- Reviewed the cumulative change, with emphasis on Python/TypeScript framework telemetry producers, selected shared-client credentials, OTLP export, authenticated tenant/publisher and Card authority at ingest, query readback, metadata/content separation, and new lifecycle diagnostics.
- This is a static, review-only domain audit. It does not propose a new authorization or redaction subsystem. Source files were not edited and no builds/tests were run.

## Authority and source coverage

| Boundary / obligation | Governing authority | Source and evidence inspected | Assessment |
|---|---|---|---|
| Tenant and publisher come from verified credentials; optional Card is authorized | AGENTS.md §§2, 9–10; agent-rules; wyrd-design §§Runtime identity / Observation identity; wyrd-security-posture §§Security principles, Authorization and policy, Tenant and data isolation; spec INV-001 | `http/router.rs:69`, `http/middleware/authenticate.rs:64`, `http/otlp.rs:241`; `vala-bifrost-redux/src/gate/mod.rs:823`, `gate/attribution.rs:86`, `tables/signal.rs:580`, `scribe/ingress.rs:65`, `scribe/execution_lanes.rs:1083` | PASS: transport builds auth from Caller, not span metadata; table write permission precedes projection. Card-bound principals use signed scope; unbound principals resolve registered observation-target Cards using TenantConn for the authenticated tenant. Per-record extraction rejects unauthorized Card values. Scribe validates physical tenant binding and reconstructs managed identity columns. |
| Reuse selected client and fresh-token owner | spec REQ-017, INV-001; wyrd-design API key/JWT and exporter contract | Python `src/state/mod.rs:181`, `python/wyrd/otel.py:190`; TS `src/index.ts:2880`, `src/otel.ts:25`; shared `wyrd-client/src/client.rs:438`, `auth.rs:841`; Python `src/client.rs:118` | PASS: managed setup wraps/clones the state's selected client; exporters obtain its current access token at export time. No new resolution chain, token cache, identity selection from Card/Run, or renewal implementation was introduced. |
| Gateway credentials stay separate from external provider calls | spec REQ-017, INV-003; security-posture secret handling | Python framework app `gateway()` and direct framework builders; TS journey `direct()` and gateway case; existing Python `client._http_auth` boundary and TS `WyrdClient.httpAuth` projection | PASS: direct-provider recipes/tests use independent local provider credentials; Wyrd hook is attached only to Wyrd base URLs. Gateway journeys assert upstream Authorization is the configured provider key and no `x-wyrd` headers reach upstream. |
| Bridge does not copy payloads or raw errors into metadata | spec REQ-010, INV-003; telemetry-observations §§Sampling and payload safety, Instrumentation contract | Python `otel.py:293–445`; TS `index.ts:2494–2663`; callback tests in Python `test_observe_surface.py:927` onward and TS `telemetry.test.ts:216` onward | PASS: bridge accesses identity/model/token-count fields, and basic names/hierarchy/timing. Error mapping records fixed `_OTHER` and ERROR without the framework error message/data. Shutdown uses a fixed interruption description. Custom span data, prompts, completions and tool arguments/results are not copied. Names/models remain ordinary framework metadata, not identity authority. |
| Lifecycle failures do not project exporter messages | spec REQ-013, INV-003; AGENTS secret handling | `wyrd-client/src/state.rs:2088`; Python `src/state/mod.rs:506`; TS `index.ts:2610`; TS native `cards.rs:301`; shared error catalog | PASS: provider exceptions project type/name information and stage, with only the native drain's stable code as secondary detail. TS inspects a timeout message for classification but does not include it in the projected error. Unit cases contain secret sentinels in exception messages and assert absence from projected details/messages. |
| Native content policy remains explicit; no universal redaction claim | spec REQ-014/015; telemetry-observations payload rules | Observe page additions at `docs/src/content/docs/build-and-verify/observe/record-a-run-and-observations.md:157`; Python framework builders | PASS: recipes use Pydantic `include_content=False`, ADK's content switch, disclose Pydantic tool definition/schema residual content, and explain replacing versus retaining OpenAI's exporter. The bridge's allowlisted mapping does not alter the existing native OTLP payload contract. |
| Readback uses authenticated query authority | security-posture §§Authorization and policy, Tenant and data isolation; spec INV-001, AC-011 | shared client `bifrost/facade.rs:834`, `bifrost/query.rs:205`; server `query/service.rs:179`; Oracle `mod.rs:196`, `mod.rs:3426`, `mod.rs:3018` | PASS: trace id is bound as a parameter to existing SQL, not interpolated as SQL. Query context verifies principal/tenant equality, resolved tables require effective permission, and existing payload-column handling remains in the query owner. No query authorization/tenant behavior changed. |

Also read the review skill, reference router, maintainer/spec-driven references, approved r2 spec, original task and execution evidence, applicable doctrine and telemetry/privacy/security authority slices. CodeGraph was used before source searches to locate bridge/exporter and correlation owners; subsequent source reads expanded server/auth/query consumers. Production changes and relevant test/docs diffs were inspected independently of implementation conclusions.

## Negative and privacy proof assessment

- Python `tests/integration/otel/frameworks/test_framework_telemetry.py:176` parameterizes all three actual frameworks. It checks always-off sampling yields no rows, viewer export yields no rows, another tenant's admin credential cannot persist spans asserted against the original Card (readback under both tenants), and prompt/tool-result sentinels are absent from stored timelines. Python Run entry also annotates the enclosing request span, explaining why this foreign-tenant trace is entirely rejected.
- TypeScript `tests/integration/agents-trace-processor.test.ts:284` checks the equivalent sampling/viewer/privacy paths. Its foreign-tenant case deliberately permits that tenant's uncorrelated `request` span while rejecting original-tenant Card assertions; original-tenant readback is empty. This is correct generic-telemetry behavior, not a cross-tenant bypass.
- Freshness tests at Python `test_observe_surface.py:872` and TS `telemetry.test.ts:184` send two actual HTTP exports through stock exporters and assert `token-1`, then `token-2`; they prove per-export retrieval without manufacturing a second credential-refresh engine.
- Python gateway journey at `test_framework_telemetry.py:217` and the TS gateway journey verify provider credential separation and absence of Wyrd headers upstream. Both also exercise missing ambient credentials using isolated configuration rather than a developer login.
- Task evidence reports successful focused U5/T5, J4/K4, J5/K5 and existing HTTP-auth regression runs. Those are reported execution claims, not runs performed by this reviewer; the assertions and production paths supporting those claims were inspected.
- Python OTEL's flush can log a refused export and return success. This does not admit unauthorized data: the server still refuses it, the real denied-export test checks no persistence, and the recipe explicitly discloses that Python limitation. No new authorization-success inference is made from flush alone.

## Security Audit

### Critical

None identified.

### High

None identified.

### Medium

None identified.

### Low / Defense In Depth

No in-scope recommendations requiring a change.

### Positive Controls

- Authenticated tenant/publisher authority is separate from mutable correlation attributes.
- Both signed-scope and tenant-registry Card paths fail closed before durable correlation is stamped.
- Existing stock exporters and the shared credential owner are reused; no setup-time bearer snapshot is added.
- Payload-free error status and lifecycle projections avoid raw exception messages.
- Denied writes and foreign-tenant attribution have real server/readback tests in both language surfaces.
- Framework privacy recipes distinguish native residual metadata from the bridge's content exclusion.

## Verification limits and final result

No source-local material finding, unresolved security-boundary question, or required mitigation was identified. This PASS covers the assigned authenticated telemetry/privacy boundary, not dependency advisory enumeration, deployment TLS configuration, unrelated pre-existing authorization behavior, or other reviewers' domains. No live services, builds, tests, external research, or vulnerability scans were run.

At final inspection HEAD remained `692d390dc40e43c6e1d7e25c7850d5c826d26a9e`; tracked source matched the immutable candidate. Only review artifacts were untracked.
