# Security domain review — TASK-003-r4

## Subject and review boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `cfd60f6442e4dbdfcbc57cfec0bde494a225ae89`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediation authority: TASK-003 R1, R2, and R3 under `changes/active/skald-workflow-runtime/review/`
- Human approvals applied: the 2026-10-03 `WyrdGatewayCall.model` amendment, the R1 same-spec native-`401` task correction, and the approved use of `spawn_blocking` for run-start configuration, client, and secret reads

This review covers authentication before fallback interpretation, tenant and
principal preservation, send-once native `401` renewal, retained client and
credential context, selected secret resolution and file handling, public
fallback validation and consumption, native error redaction, and dependency
changes. The candidate remained at the stated commit during this review.

## Authority and source coverage

| Boundary | Governing authority | Source and caller coverage | Assessment |
|---|---|---|---|
| Public ingress authentication and fallback | Security posture trust boundaries; Revision 12 public-header contract; TASK-003 Scenario 3 | `wyrd-server/src/components/gateway/ingress.rs`, `routes.rs`, `pg_invocation_tests.rs`; `wyrd-spec/src/gateway/policy.rs`; gateway invocation callers | Authentication middleware verifies the Wyrd token and inserts the principal before a handler runs. Each affected handler branches on the authenticated `Caller` before interpreting the fallback, validates it against the effective requested model, and passes only the typed override into `GatewayCallRequest`. The caller header is not forwarded to providers. PASS. |
| Tenant, principal, authorization, and audit continuity | `architecture/wyrd-security-posture.md`; gateway authorization/audit doctrine | Public ingress, `GatewayInvocation` admission/routing/decision paths, existing gateway credential owner | Tenant and principal continue to come only from the verified credential. Requested and fallback candidates traverse the existing typed authorization, tenant snapshot, provider-credential, and tracked canonical audit owners. The new header cannot select identity or bypass a model decision. PASS. |
| Native bearer acquisition and reactive `401` renewal | Wyrd API-key re-exchange architecture; approved R1/R2/R3 send-once correction | `wyrd-client/src/auth.rs::{bearer,force_refresh}`; `transport/http.rs::post_native`; `workflow/gateway.rs::call`; focused recorded evidence | The model POST is sent exactly once. Once response headers establish `401`, the existing auth owner is refreshed before body collection; renewal failure is authoritative, and success does not replay the request. The outer deadline/cancellation still bounds the work. Fixed bearer behavior remains the previously approved existing-owner behavior. PASS. |
| Portable native-error redaction | Revision 12 `RemoteProblem` contract; TASK-003 native-envelope contract; prior validated findings | `workflow/gateway.rs::{Ingress::problem,wyrd_problem}`; Skald `attempt.rs` projection; recorded three-dialect canary evidence | Recognized codes use catalog title/remediation, uncoded responses use fixed category text, success decode failures quote no body, and only the approved optional OpenAI `param` becomes `field`. The prior proposal to add a new `param` grammar was authoritatively rejected as a specification change and is not re-raised. PASS. |
| Selected local bindings and secret containment | Security posture secret rules; Revision 12 shared local configuration; TASK-003 Scenario 4 | `workflow/{mod,local}.rs`; `global_config.rs`; `wyrd-utils/src/secret.rs`; gateway/server consumers; Python retained owner | Route selection precedes resolution; unselected bindings are not read. Secret values remain `SecretString`; errors omit the variable name, path, and value. The shared reader opens before metadata validation, accepts only a regular owner-only Unix file, bounds reads to 64 KiB, and returns static redacted failures. Python keeps the complete shared Workflow and its loading client. One mixed-route configuration-context defect remains as `SEC-R4-001`. |
| Dependency and supply-chain delta | AGENTS dependency-cost rule; task non-goals | Cumulative manifest and lockfile diff | The cumulative change adds existing workspace edges and already-resolved dependencies for the approved owners; no new registry package or version is introduced. PASS. |

## Security Audit

### Critical

None.

### High

None.

### Medium

- **SEC-R4-001 — INCORRECT — [`crates/shared/wyrd-client/src/workflow/mod.rs:190`](../../../../../crates/shared/wyrd-client/src/workflow/mod.rs)** A mixed ambient Workflow can assemble its external binding configuration and public Wyrd client from two different `GlobalConfig` snapshots. R3 requires the run-start owner to load one ambient `GlobalConfig` snapshot and reuse it for both selected external bindings and a client-less public-gateway client. `load_local_setup` first calls `GlobalConfig::load()` and immediately keeps only `.workflow` at lines 195–196. When the same Workflow also selects `wyrd_gateway` and has no retained Cards client, lines 200–208 call `WyrdClient::from_global()`, which reaches `ClientConfig::from_global()` and calls `GlobalConfig::load()` again (`client.rs:52-54`, `config.rs:96-98`). Per-step route overrides make this path reachable: one Workflow may select both `ExtGateway` and `WyrdGateway`. If `config.toml` is atomically rotated between reads, one run can resolve external credentials/endpoints from configuration A while sending its governed Wyrd call to the endpoint/tenant/cache context from configuration B. That contradicts R3's explicit single-snapshot correction and breaks the run's coherent credential/client context. The recorded selected-dependency proof tests ExtGateway and WyrdGateway separately and does not cover this mixed ambient path; R3's implementation evidence claims one snapshot but source contradicts it. The smallest correction is inside the existing synchronous `load_local_setup` owner: load at most one `GlobalConfig` when either selected external routes need it or a client-less WyrdGateway route needs it, return its `workflow` section, and build the gateway client from `ClientConfig::from_global_with_env(&that_same_snapshot)` through the existing `WyrdClient::with_config` constructor. Preserve a retained Cards client without ambient client assembly, selected-only secret reads, the approved blocking-pool boundary, and current error projection. Focused closure proof should exercise one client-less Workflow containing both route kinds and show both dependencies derive from one controlled configuration snapshot; no cache, watcher, new option, dependency, harness, or permanent check is required.

### Low / Defense In Depth

None. No optional hardening is required for acceptance.

### Positive Controls

- Authentication completes before fallback decoding and before gateway dispatch.
- The fallback header is bounded, duplicate-checked, typed with unknown-field denial, validated against the requested model, consumed at ingress, and excluded from provider headers.
- Tenant and principal identity remain token-derived; fallback models receive the ordinary gateway authorization and audit decisions.
- Native model calls are not replayed after ambiguous or provider-originated `401` responses.
- Provider credentials stay behind the server gateway boundary and do not enter Workflow state, client configuration, provider request bodies, errors, or tracing.
- Correlation contains run/step/attempt identifiers only and travels through scrubbed tracing fields, not provider payloads.
- Selected external secrets are resolved only at run start on the blocking pool; secret-file validation uses the already-open handle and redacted errors.
- Recognized native refusal text comes from the Wyrd catalog, and arbitrary upstream body/message text is discarded.
- No novel security mechanism, check, setting, option, allowlist, or dependency is proposed. The retained correction reuses the existing `GlobalConfig`, `ClientConfig`, and `WyrdClient` owners required by R3.

## Prior-finding closure

| Prior finding | Security assessment |
|---|---|
| `FIND-TASK-003-1` | CLOSED. Rust and Python retain the loading client through authoring and local execution. |
| `FIND-TASK-003-2` | CLOSED. A known `401` begins renewal before body collection, sends one model POST, and never replays it. |
| `FIND-TASK-003-3` | CLOSED. Recognized error codes use catalog text; upstream message text does not survive. |
| `FIND-TASK-003-4` | CLOSED. Cross-module secret consumers receive `SecretString`; the plaintext file reader is private. |
| `FIND-TASK-003-5` through `FIND-TASK-003-9` | No security regression found in the current source. |
| `FIND-TASK-003-10` | PARTIAL. Filesystem work is correctly moved to the blocking pool, but the explicit one-`GlobalConfig`-snapshot part of its R3 correction is not implemented (`SEC-R4-001`). |
| `FIND-TASK-003-11` | CLOSED. `force_refresh` documentation distinguishes replay-safe transports from the native send-once caller. |
| `FIND-TASK-003-12` | CLOSED. `Workflow::into_skald` documents loss of retained client and automatic dependency composition. |

## Verification evidence and limits

Per the human standing direction, this review was strictly source-only. It did
not build, compile, or run tests, Cargo, mise, pnpm, pytest, or any other
verification command. I inspected the cumulative base-to-candidate diff,
current source and callers, the original task and all three remediations, prior
validated ledgers, and implementer-recorded evidence.

The recorded evidence supports the send-once renewal, redaction, ingress,
secret-file, selected-only resolution, and retained-client paths. It does not
exercise one ambient Workflow that selects both ExtGateway and WyrdGateway,
and the recorded R3 claim of one `GlobalConfig` snapshot is contradicted by the
two source call chains identified above. That is a material finding rather
than a generic verification limitation.

## Overall result

**FAIL**

The cumulative candidate preserves the public authentication, tenancy,
authorization, audit, error-redaction, native renewal, secret containment, and
provider-credential boundaries. `SEC-R4-001` leaves one bounded but real
credential/client-context inconsistency against the approved R3 correction.
