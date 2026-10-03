# TASK-003 R2 Security Domain Review

## Subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `2e6f13f505daf19050e588ea0d1d7fc936697e27`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12, including the human-approved 2026-10-03 minimal `WyrdGatewayCall.model: ModelRef` amendment
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior review and remediation: `changes/active/skald-workflow-runtime/review/TASK-003-r1/`, including the human-approved same-spec native-401 wording correction

This review covers the security, credential, and trust boundaries changed by the cumulative candidate: authentication and reactive renewal in the native public-gateway client; provider-native error-envelope normalization; fallback-header authentication, validation, and consumption; selected secret resolution and redaction; retained Python client/credential context; and public gateway authorization/audit adjacency. The review used source, the complete diff, and the implementer's recorded evidence only. It did not build, compile, or run any test or verification command.

## Authority and source coverage

| Boundary | Authority and source inspected | Assessment |
|---|---|---|
| Client authentication and native `401` | `architecture/wyrd-security-posture.md`; `wyrd-client/src/auth.rs::AuthMiddleware::{bearer,force_refresh}`; `wyrd-client/src/transport/http.rs::post_native`; `workflow_transport.rs` recorded cases | The request is sent once. A `401` renews through the existing auth owner, renewal failure propagates as its catalog error, and success does not replay the non-idempotent model call. Fixed bearer behavior returns the original refusal without replay. Prior replay finding is closed in source. |
| Provider-native error trust | Revision 12 lines 1158-1188 and error-redaction requirements; `wyrd-gateway/src/adapter/{mod,http}.rs`; `wyrd-client/src/workflow/gateway.rs::Ingress::problem`; `skald-workflow/src/attempt.rs` | Known-code message text now comes from the derive-backed catalog, closing the prior message-canary path. OpenAI `error.param` remains untrusted and is copied verbatim into `RemoteProblem.field` and then `WorkflowRunError.details.field`; SEC-R2-001 remains. |
| Fallback header boundary | Revision 12 lines 1190-1208; `wyrd-spec/src/gateway/policy.rs`; server `gateway/ingress.rs`, `gateway/routes.rs`, and `gateway/invocation.rs`; PG test source and recorded evidence | Protocol-specific middleware verifies the Wyrd token before handlers run. Repeated, malformed, oversized, empty, duplicate, and self-referential overrides fail before invocation. Only the typed override enters `GatewayCallRequest`; caller headers are not forwarded to providers. |
| Model authority and request projection | Approved `WyrdGatewayCall.model` amendment; `skald-workflow/src/route.rs`; `wyrd-client/src/workflow/gateway.rs::NativeCall::project` | The typed call model, not a body-supplied model, determines the projected OpenAI/Anthropic/Gemini target. Vertex is refused before IO. No credential or workflow context is added to the provider body. |
| Secret references and external bindings | Security posture secret rules; `wyrd-utils/src/secret.rs`; `wyrd-client/src/workflow/local.rs`; gateway credential and server key consumers; external-route owners and recorded tests | Only selected bindings resolve at execution. Values are returned as `SecretString`; errors omit environment names, paths, and contents. File checks use the already-open handle, require a regular owner-only Unix file, and bound reads. The obsolete public plaintext helper is private. |
| Python retained authority context | Revision 12 client reuse contract; `wyrd-client/src/workflow/mod.rs`; Python `workflow.rs` and `state/mod.rs`; integration journey source and recorded result | Python now retains the complete shared `wyrd_client::Workflow`, including the loading client, and mutations change only its Skald graph. Runs use the retained server and credential without copying bearer material into Python-visible state. |
| Gateway authorization, tenancy, and audit | Security posture; server ingress authentication; `gateway/invocation.rs::{admit_call,route,decide}` | Verified credentials supply tenant/principal. Requested and fallback models receive typed permission decisions; the gateway loads the verified tenant's snapshot and uses the established tracked non-blocking canonical audit append required for gateway invocation decisions. The new fallback header cannot select tenant or bypass these decisions. |
| Dependencies and supply chain | Cumulative manifest and lockfile diff | Only existing workspace crates/dependencies changed tier placement or gained internal edges (`skald-providers`, `skald-runtime`, `secrecy`, `wyrd-utils`). No new external package or version was introduced. |

## Security Audit

### Critical

None.

### High

None.

### Medium

- **SEC-R2-001 — INCORRECT — [`crates/shared/wyrd-client/src/workflow/gateway.rs:164-169`, `:187-200`; `crates/skald/skald-workflow/src/attempt.rs:170-184`] Untrusted OpenAI `error.param` survives as portable diagnostic data.** `Ingress::problem` parses a provider-native refusal relayed by the gateway and copies `envelope.error.param` directly into `RemoteProblem.field`. The gateway intentionally preserves a native provider JSON refusal after removing only the resolved provider credential (`crates/wyrd/wyrd-gateway/src/adapter/mod.rs:258-299`), so a compromised or tenant-configured OpenAI-compatible upstream controls this string. A reachable exploit is a provider response carrying a recognized Wyrd code, a safe-looking catalog-independent message, and `param` containing prompt text, request fragments, or another canary. The catalog-title remediation removes the untrusted `message`, but `project_agent_error` then persists the attacker-controlled `param` at `WorkflowRunError.details.field`. This contradicts Revision 12's requirement that the generic problem carry no prompt, arbitrary upstream error text, or response-body data (`spec.md:1173-1188`) and the security authority's prohibition on secrets in errors. It can expose sensitive request material wherever workflow errors are returned, retained, logged, or observed. The existing recorded test demonstrates reachability by preserving `param: "fallback"`, but it does not test an unsafe or oversized `param` and therefore does not prove the specification's “safe param” condition. Correct the existing public-client normalization boundary so it retains only a bounded value that satisfies the repository's established request-field/path shape; otherwise omit the field. Do not add origin headers, provenance machinery, configuration, allowlists, or a new checker. Focused closure proof must send a known-code OpenAI refusal with prompt/canary text in `param` and show it is absent from both `RemoteProblem` and the final workflow error, while the ordinary safe `fallback` field remains projected.

### Low / Defense In Depth

None. No optional hardening is required for acceptance.

### Positive Controls

- Public gateway authentication runs in protocol-specific middleware before request bodies or fallback overrides are interpreted.
- Tenant and principal come only from the verified token; the fallback header and provider body cannot select tenant identity.
- The fallback header is request-scoped, duplicate-checked, size-bounded, JCS/base64url-decoded, typed, validated against the requested model, and never forwarded to a provider.
- `WyrdGatewayCall.model` is the sole target authority for native projection, preventing body/model confusion.
- Native model POSTs are not replayed after ambiguous or provider-originated `401` responses.
- Known-code error messages and remediation now come from the derive-backed catalog; uncoded responses use fixed category text.
- Gateway provider credentials stay server-owned and are scrubbed from provider refusals before relay.
- Secret references resolve only at execution for selected routes; plaintext is held in redacted secret types and reader errors disclose neither locator nor value.
- Python preserves the loading client without exposing or duplicating bearer material.
- Gateway model authorization and audit remain on the existing server owner for the requested model and every fallback candidate.
- No bespoke mechanism, setting, option, check, or dependency was added or required by this review.

## Prior-finding closure

| Prior finding | Source assessment | Recorded evidence assessment | Result |
|---|---|---|---|
| FIND-TASK-003-1 / Python loses loading authority context | `PyWorkflow.inner` is now `wyrd_client::Workflow`; registered and authored-external loads keep their client; mutation methods preserve it; `run` delegates to the retained owner. | The recorded Python journey covers explicit server A, hostile/incorrect ambient server B, absent ambient config, authored external refs, registered loads, successful and refused mutations, and client-less local refusal. | CLOSED |
| FIND-TASK-003-2 / native `401` replay | `post_native` performs one POST, reads the original refusal, calls `force_refresh`, and returns the original response; renewal errors propagate. | The recorded focused test covers uncoded and forged-known-code `401`, one model POST, renewed bearer on the next call, and renewal failure. | CLOSED |
| FIND-TASK-003-3 / known-code message trust | `Ingress::problem` no longer reads native envelope message text for recognized codes and uses catalog title/remediation. | Recorded OpenAI, Anthropic, and Google canary-message cases cover the changed behavior. The evidence does not cover the separate OpenAI `param` channel identified in SEC-R2-001. | MESSAGE PATH CLOSED; FIELD PATH OPEN |
| FIND-TASK-003-4 / public plaintext secret helper | `read_secret_file` is private; cross-module callers receive `SecretString` through `read_secret_ref`. | Recorded gateway credential and shared lanes are consistent with source. | CLOSED |
| FIND-TASK-003-5 / import placement | Relevant imports are in module/test-module import blocks with Unix cfg preserved. | Recorded lint and focused evidence is consistent with source. | CLOSED |

## Verification limits

No commands were executed by this reviewer. The implementation record reports green focused Rust tests, a Python integration journey, Python unit/type lanes, shared tests, formatting/lints, code generation, and client/PyO3 boundary checks. Source supports those claims for the named cases. No recorded case supplies a recognized Wyrd code with unsafe OpenAI `param` content or asserts that such content is absent from the final workflow error; that missing direct proof accompanies the source defect in SEC-R2-001 rather than serving as a generic verification limitation.

The candidate remained `2e6f13f505daf19050e588ea0d1d7fc936697e27` during this review.

## Overall result

**FAIL**

Authentication ordering, credential isolation, native-call replay safety, message redaction, selected-secret handling, tenant authority, and gateway authorization/audit adjacency satisfy the approved task in source. SEC-R2-001 leaves one reachable provider-controlled diagnostic channel that violates the task's explicit redaction contract and needs a bounded correction at the existing normalization owner.
