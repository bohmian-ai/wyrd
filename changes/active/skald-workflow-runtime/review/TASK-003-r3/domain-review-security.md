# TASK-003 R3 Security Domain Review

## Subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `0732ed92fc2907cb806ac918b09abbf6d44ff04f`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12, including the human-approved 2026-10-03 minimal `WyrdGatewayCall.model: ModelRef` amendment
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior remediation: `review/TASK-003-r1/TASK-003-R1-preserve-client-context-and-native-call-safety.md` and `changes/active/skald-workflow-runtime/review/TASK-003-r2/TASK-003-R2-close-renewal-proof-and-source-contracts.md`, including the approved same-spec native-`401` wording correction

This review covers the security, RBAC, credential, and trust boundaries changed
by the cumulative candidate: selected secret references and redacted values;
public-client bearer selection and reactive renewal; retained Rust/Python client
context; fallback-header encoding, authentication ordering, bounds, decoding,
and consumption; model/provider authority; gateway tenant/principal
preservation, per-model authorization, and canonical non-blocking audit; and
portable error redaction. It used source, the complete base-to-candidate diff,
and recorded implementation evidence only. It did not build, compile, run tests,
or execute any verification command.

## Authority and source coverage

| Boundary | Authority and source inspected | Assessment |
|---|---|---|
| Credential selection and retained authority context | Revision 12 `REQ-038`, `REQ-058`, `INV-004`, `INV-012`, `INV-020`; `wyrd-client/src/{config.rs,auth.rs,workflow/mod.rs,workflow/local.rs}`; Python `workflow.rs` and its recorded journey | Registered and authored-external workflows retain the loading `WyrdClient`; local authored values lazily use ambient configuration only when their selected routes require it. No bearer is copied into a Workflow or Python-visible field. |
| Selected secret references | Security posture secret rules; `wyrd-utils/src/secret.rs`; `wyrd-client/src/workflow/local.rs`; `skald-workflow/src/route.rs` | Only selected external bindings resolve during run preparation. The shared reader returns `SecretString`, checks metadata on the already-open file, bounds file size, requires owner-only Unix permissions, and returns locator-free errors. Bound header values are marked sensitive before transport. |
| Native bearer and reactive renewal | Approved R1 wording; R2 `FIND-TASK-003-2`; `wyrd-client/src/auth.rs::{bearer,force_refresh}`; `transport/http.rs::post_native`; `workflow/gateway.rs::call`; recorded transport proof | The model POST is sent once and is never replayed. Complete and abruptly truncated `401` bodies lead to renewal, but a body that remains pending until the outer call timeout prevents the already-observed `401` from reaching `force_refresh`; `SEC-R3-001` remains. |
| Fallback-header trust boundary | Revision 12 `REQ-036A`, `AC-011A`; `wyrd-spec/src/gateway/policy.rs`; server `gateway/{ingress,routes,invocation}.rs`; recorded PG/OpenAPI evidence | Authentication middleware verifies the Wyrd token before the handler reads the fallback header. Duplicate, malformed, oversized, empty, duplicate-candidate, and self-referential values fail before gateway invocation. Only the typed override enters `GatewayCallRequest`; provider dispatch receives no caller header map. Receiver-side JCS byte recanonicalization is neither required nor ordinary JSON practice and is correctly absent. |
| Model/provider authorization | Approved `WyrdGatewayCall.model` amendment; `skald-workflow/src/route.rs`; `wyrd-client/src/workflow/gateway.rs`; server `gateway/invocation.rs` | The Prompt-derived typed `ModelRef`, not an invocation-body override, determines the projected route target. The verified caller authorizes the requested model and every fallback candidate. Unsupported Vertex public ingress refuses before IO. |
| Tenant, principal, and audit preservation | Security posture; ingress authentication; `gateway/invocation.rs::{admit_call,route,decide}` | Tenant and principal come only from the verified `Caller`. The gateway loads that tenant's snapshot, retains the principal on the admitted call, and records allowed and denied requested/fallback-model decisions through the established tracked non-blocking canonical audit path. The fallback header cannot select identity or bypass authorization. |
| Error and telemetry redaction | Revision 12 portable-error contract; `workflow/gateway.rs::{Ingress::problem,wyrd_problem}`; gateway invocation tracing fields | Known codes use catalog title/remediation; uncoded refusals use fixed category text; transport failures use fixed text. Prompt/request bodies and bearer values are skipped from spans. The approved OpenAI `param` projection remains the sole optional field; constraining or deleting it without new authority would be contract drift, so it is not a finding. |
| Dependency and supply-chain surface | Manifest and lockfile diff | Changes add only existing workspace dependencies and move existing Skald dependencies from dev-only to production where the new shared runtime path uses them. No external package or version was added. |

## Security Audit

### Critical

None.

### High

None.

### Medium

- **SEC-R3-001 — INCORRECT — [`crates/shared/wyrd-client/src/transport/http.rs:402-410`; `crates/shared/wyrd-client/src/workflow/gateway.rs:100-105`; proof at `crates/shared/wyrd-client/tests/workflow_transport.rs:1039-1062`] An observed native `401` can still time out before reactive renewal.** The approved task wording and R2 remediation require every observed native `401` to renew through `AuthMiddleware::force_refresh` without replay, including when its body cannot be collected. `post_native` records the status, then awaits the entire response body before checking the status and refreshing. If the server sends `401` headers with a positive `Content-Length` and keeps the body connection open, the outer `PublicWyrdGatewayCaller` deadline cancels `post_native` while it is still at `response.bytes().await`; `force_refresh` is never polled. A failed or compromised endpoint can therefore keep a refused cached access token active for the next call, causing repeated presentation of known-refused authority and repeated authentication/audit failures until another path refreshes it. The recorded R2 proof closes the connection after a short partial body, so `bytes()` returns an error and renewal runs; it does not prove the pending-body path and is contradicted by the current ordering. Keep the single POST and existing auth owner, but perform the `401` refresh immediately after the status is known and before waiting for body completion. Renewal failure remains authoritative; after successful renewal, return the original refusal when its body completes or the existing body-read/timeout outcome otherwise. Extend the existing raw-HTTP focused proof so a `401` body remains pending, observe the `/auth/token` renewal before releasing or closing that body, and prove one model POST with no resend.

### Low / Defense In Depth

None. No optional hardening or new mechanism is required for acceptance.

### Positive Controls

- Protocol-specific middleware authenticates before request bodies or fallback overrides are interpreted.
- Tenant and principal are derived only from the verified token; neither the model body nor fallback header can select identity.
- The fallback override is request-scoped, duplicate-checked, size-bounded, typed, model-validated, and never forwarded to providers.
- Prompt-derived `WyrdGatewayCall.model` is the target authority, preventing native-body model confusion.
- Model calls are never replayed after a `401`, preserving the non-idempotent gateway/provider boundary.
- Requested and fallback models each receive a typed permission decision and the established gateway audit append.
- Gateway provider credentials remain server-owned and do not enter Skald, Workflow state, client configuration, errors, or telemetry.
- External-binding secrets resolve only for selected routes, remain in redacted secret types, and are marked sensitive as HTTP header values.
- Python keeps the complete shared Workflow/client owner rather than duplicating bearer or connection state.
- Recognized error messages and remediation come from the Wyrd catalog; provider-controlled message text does not survive normalization.
- No novel check, option, feature, allowlist, dependency, or bespoke security mechanism was added or required by this review.

## Prior-finding closure

| Prior finding | Source and evidence assessment | Result |
|---|---|---|
| `FIND-TASK-003-1` — Python loses loading authority context | `PyWorkflow` owns `ClientWorkflow`; registered and authored-ref loaders preserve it; mutations touch only the Skald graph; `run` delegates to the retained owner. The recorded public Python journey exercises explicit server/credential context against hostile or absent ambient configuration. | **CLOSED** |
| `FIND-TASK-003-2` — native `401` replay / renewal | Replay is removed, renewal failure is authoritative, and a completed or abruptly truncated body renews. The still-pending body path bypasses renewal until the outer deadline, so the same renewal invariant remains open as `SEC-R3-001`. | **REOPENED / REVISED** |
| `FIND-TASK-003-3` — known-code provider message crosses redaction | `Ingress::problem` ignores provider message text and uses catalog metadata for recognized codes. Recorded canary cases cover OpenAI, Anthropic, and Google. | **CLOSED** |
| `FIND-TASK-003-4` — public plaintext file-secret helper | `read_secret_file` is private and all cross-module consumers receive `SecretString` through `read_secret_ref`. | **CLOSED** |
| `FIND-TASK-003-5` / `-6` — import-manifest violations | Security-sensitive changed declarations now use module imports and role-specific aliases; no runtime security effect remains. | **CLOSED** |
| `FIND-TASK-003-7` / `-8` / `-9` — documentation and cancellation proof | Source contains the required panic/cancellation contracts, and the recorded public-caller proof cancels only after the model POST reaches the boundary with no resend. These do not close the distinct pending-`401` renewal ordering above. | **CLOSED** |

## Verification limits

No command was executed by this reviewer. The implementation record reports
green focused shared-client tests, server fallback ingress proof, the Python
retained-context journey, code generation, boundary checks, formatting, and
lints. Source supports those claims for the named cases. No recorded case holds
a native `401` body pending while checking whether refresh begins before body
completion; the missing proof accompanies `SEC-R3-001` and is not treated as a
generic limitation.

The candidate remained `0732ed92fc2907cb806ac918b09abbf6d44ff04f` during
this review.

## Overall result

**FAIL**

The candidate preserves the intended tenant/principal boundary, server-owned
model authorization and audit, selected-secret handling, fallback-header
validation, retained Python client context, send-once model semantics, and
portable error redaction. `SEC-R3-001` leaves one bounded reactive-renewal gap
inside the existing HTTP/auth owner and requires direct source correction and
recorded focused proof.
