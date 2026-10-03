# TASK-003 r2 focused follow-up review

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `2e6f13f505daf19050e588ea0d1d7fc936697e27`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12, including the human-approved 2026-10-03 addition of `pub model: ModelRef` to `WyrdGatewayCall`
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior review/remediation: `changes/active/skald-workflow-runtime/review/TASK-003-r1/`, including the human-approved same-spec native-401 wording correction

This follow-up investigated only the four conflicts assigned by the orchestrator. It used the complete cumulative diff, candidate source, callers, tests, applicable authority, and the implementer's recorded evidence. No build, compilation, test, Cargo, mise, pnpm, pytest, package-manager, `git diff --check`, or other verification command was run. No source was modified.

## Resolution summary

| Uncertainty | Resolution | Finding disposition |
|---|---|---|
| A `401` whose body cannot be collected returns before `force_refresh` | **RESOLVED** | `INV-R2-001` is confirmed as a reachable implementation defect. |
| Recorded cancellation proof never cancels after dispatch | **RESOLVED** | `CONC-R2-001` is confirmed as a required proof gap, not a source defect claim. |
| OpenAI `error.param` is provider-controlled although the approved contract permits its safe projection | **RESOLVED** | `SEC-R2-001` is rejected as a TASK-003 implementation finding; changing or constraining this approved field requires specification authority. |
| Receiver accepts semantically valid noncanonical JSON despite the JCS sender contract | **RESOLVED** | `B-001` is rejected; strict receiver byte-canonicality is not an approved requirement and requiring it would be DRIFT. |

## 1. Native `401` renewal after an unreadable body

**Resolution: RESOLVED — confirm `INV-R2-001`.**

### Source path and reachability

`HttpTransport::post_native` sends the model request once and obtains a `reqwest::Response` at `crates/shared/wyrd-client/src/transport/http.rs:389-393`. At that point the HTTP status is known. The method then awaits `response.bytes()` at line 394 with `?`, and only afterward tests `status == StatusCode::UNAUTHORIZED` and calls `AuthMiddleware::force_refresh` at lines 395-399.

A server can send complete `401` headers and then close or truncate the declared body. That is a normal reachable HTTP failure: the client has observed `401`, but `response.bytes()` fails. The early return bypasses `force_refresh`; `PublicWyrdGatewayCaller::call` maps the resulting internal transport error to `ProviderError::Connect` at `workflow/gateway.rs:101-107`, and the auth cache can retain the refused token for the next call. The existing raw-HTTP `TcpListener` fixtures under `crates/shared/wyrd-client/tests/transport/http.rs` and the truncated-response fixtures under `crates/shared/wyrd-client/src/bifrost/query.rs` establish that this failure shape is supported by the repository's ordinary test practice.

### Authority reconciliation

The corrected TASK-003 wording at lines 82-87 says a native model call is sent once and, "on a `401`," the adapter renews through `AuthMiddleware::force_refresh`, propagates renewal failure, and otherwise returns the original native refusal. The R1 remediation repeats the same selected outcome at lines 26-30. Neither condition limits renewal to a `401` with a fully readable body. The system report's PASS accurately describes complete-body `401` cases, but it does not cover the earlier fallible body collection and therefore cannot close this path.

### Proposed finding `FU-R2-001`

- **Classification:** INCORRECT
- **Violated obligation:** The approved same-spec 401 correction requires reactive renewal once a native response is known to be `401`, without replaying the model call.
- **Exact location:** `crates/shared/wyrd-client/src/transport/http.rs:393-399`.
- **Observable consequence:** A renewable client can keep and reuse a refused access token after a truncated/reset `401` response body.
- **Smallest correction:** Keep the single POST and existing auth owner. Preserve the body-read result, attempt the existing `force_refresh` whenever the known status is `401`, let a renewal failure remain authoritative, and after successful renewal return either the collected original refusal or the existing body-read transport error. Do not add retry, provenance, configuration, or another transport.
- **Focused closure proof:** Using the repository's existing raw-HTTP fixture pattern, return `401` headers with a deliberately incomplete body and prove one model POST, one renewal, no resend, and the existing body-read/connection failure after successful renewal. Retain the existing complete-body renewal-success and renewal-failure cases.

## 2. Cancellation after model dispatch

**Resolution: RESOLVED — confirm `CONC-R2-001` as a proof finding.**

### Source and recorded evidence

The implementation shape is direct: `PublicWyrdGatewayCaller::call` creates the full authenticated `post_native` future and races it against `cancellation.cancelled()` and `tokio::time::timeout` in one `tokio::select!` at `crates/shared/wyrd-client/src/workflow/gateway.rs:94-114`. Dropping the losing future is the intended cancellation mechanism; no separate source defect is established.

The required named proof does not exercise that active-cancellation path. In `crates/shared/wyrd-client/tests/workflow_transport.rs:578-588`, the token is cancelled before `call` is created and polled, so no model POST is dispatched. The only already-dispatched slow call at lines 589-612 exits through the call timeout, not through cancellation. The implementer's recorded GREEN result for `public_gateway_call_context_and_errors` therefore cannot prove the task's distinct claim that cancellation drops caller IO after dispatch.

### Authority reconciliation

TASK-003 Scenario 2 explicitly says "cancellation drops caller IO" and assigns that behavior to `public_gateway_call_context_and_errors` (`TASK-003:180-195`). The review direction says missing, unclear, or source-contradicted required evidence is an implementer finding. AC-011A also requires caller cancellation proof, while AC-020 names caller connection/token cancellation for WyrdGateway execution. The system report correctly assesses the source composition, but source plausibility does not replace the task's required behavioral evidence.

### Proposed finding `FU-R2-002`

- **Classification:** MISSING
- **Violated obligation:** TASK-003 Scenario 2 and the mapped acceptance evidence require direct proof that cancellation stops a public-gateway call whose IO is already pending.
- **Exact location:** `crates/shared/wyrd-client/tests/workflow_transport.rs:564-613`.
- **Observable consequence:** The recorded evidence establishes pre-cancel refusal and in-flight deadline expiry, but not the separately claimed post-dispatch cancellation behavior.
- **Smallest correction:** Extend the existing `public_gateway_call_context_and_errors` test with the existing mock server and `CancellationToken`. Hold a response pending, establish that its model POST reached the mock boundary, cancel that call's token, and assert the existing cancellation/timeout category returns promptly with no resend. Do not add a harness, setting, option, transport, or production cancellation mechanism.
- **Focused closure proof:** Record the exact existing focused selector after this case is added; the proof must distinguish post-dispatch cancellation from timeout.

## 3. OpenAI `error.param` projection

**Resolution: RESOLVED — reject `SEC-R2-001` as a TASK-003 finding.**

### Source and authority

`Ingress::problem` parses the OpenAI envelope and carries `error.param` into `RemoteProblem.field` at `crates/shared/wyrd-client/src/workflow/gateway.rs:164-169,187-209`; the Workflow projection then carries that optional field into `WorkflowRunError.details`. A native provider refusal can control the relayed envelope because `wyrd-gateway/src/adapter/mod.rs:258-299` preserves a credential-scrubbed native JSON refusal. The security review is therefore correct about provenance: source does not prove that every relayed string was generated by Wyrd.

That provenance fact does not establish noncompliance with the approved TASK-003 contract. Revision 12 explicitly selects "OpenAI's safe `param` as `field` when present" at `spec.md:1173-1176`, defines `WorkflowRunError.details` from that field at lines 1182-1188, and TASK-003 repeats that `OpenAI`'s safe `param` becomes optional `field` at lines 99-119. The R1 remediation and independently validated prior ledger deliberately preserve the "permitted safe field" while replacing provider-controlled message text with catalog metadata. The current source implements that selected shape exactly.

Neither the approved spec nor task defines a parameter grammar, maximum length, allowlist, catalog-to-field mapping, or provenance signal by which this client may distinguish a "safe" provider `param` from another `param`. Requiring a new validator/allowlist or dropping the field would choose new public error-normalization semantics and could break the approved smallest-common parity with in-process `details.field`. Under the standing direction, such unapproved machinery cannot be prescribed as remediation merely because it could harden the boundary.

### Disposition

`SEC-R2-001` is **REJECTED** from the implementation ledger. If the human intends "safe `param`" to mean a new syntactic bound, fixed set, or gateway-origin-only field rather than the selected OpenAI envelope member, that is a material security/public-contract clarification and must return through `$wyrd-spec`. This review does not recommend such a revision; it records only that TASK-003 cannot be failed for implementing the field the approved authority expressly requires.

## 4. Receiver-side JCS byte enforcement

**Resolution: RESOLVED — reject `B-001` as DRIFT.**

### Source and authority

`GatewayFallbackOverride::to_header_value` uses `serde_jcs::to_vec` and unpadded URL-safe base64 at `crates/wyrd-spec/src/gateway/policy.rs:126-140`; its focused test asserts the exact canonical bytes at lines 624-637. The receiver enforces encoded and decoded size limits, unpadded base64url, typed JSON deserialization with denied unknown fields, and semantic validation against the requested model at lines 142-172. Public ingress consumes only that typed value after authentication.

Revision 12 describes the sender wire value as base64url over JCS, then explicitly describes receiver work as "decodes, deserializes, and validates" and enumerates malformed, oversized, empty, duplicate, and self-referential refusals (`spec.md:1190-1201`). TASK-003 repeats the exact sender encoding and the same rejection set at lines 89-97; Scenario 3 names exact encoding/limits/model validation and malformed/oversized/empty/duplicate/self-reference at lines 200-217. It does not say the receiver must reserialize a valid value and reject byte-unequal JSON.

The repository shows the distinction when canonical bytes are a receiver invariant. `crates/wyrd-spec/src/gateway/record.rs:344-369` explicitly documents rejection when input "differs from its canonical serialization" and performs the equality comparison. No equivalent authority or behavior exists for the fallback header. JCS is ordinarily the producer's deterministic serialization mechanism; ordinary JSON consumers accept insignificant whitespace and member order unless a signed, hashed, or explicitly canonical-input boundary says otherwise. Adding a second serialize-and-compare rejection here would be a bespoke strictness not required by this task or common JSON parsing practice.

### Disposition

`B-001` is **REJECTED**. The existing proof covers the approved contract: exact canonical producer output plus receiver decoding, parsing, bounds, and semantic refusal. A valid alternative JSON representation is not one of the approved invalid forms. Requiring a canonical-byte equality guard and a noncanonical-rejection test would add an unapproved mechanism/check and is DRIFT under the standing direction.

## Final follow-up result

**RESOLVED**

The conflicts are source-resolved. Carry forward two proposed findings for independent Ponytail validation:

1. `FU-R2-001` / discovery `INV-R2-001`: known `401` status can bypass required renewal when body collection fails.
2. `FU-R2-002` / discovery `CONC-R2-001`: required post-dispatch cancellation proof is missing.

Reject `SEC-R2-001` and `B-001`. No unresolved uncertainty remains, and no new mechanism, setting, option, checker, file, dependency, or harness is required by the retained corrections.
