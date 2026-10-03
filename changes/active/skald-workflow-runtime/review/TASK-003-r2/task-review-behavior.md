# TASK-003 behavior review

## Subject and review boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `2e6f13f505daf19050e588ea0d1d7fc936697e27`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior verdict and remediation: `changes/active/skald-workflow-runtime/review/TASK-003-r1/`, including `TASK-003-R1-preserve-client-context-and-native-call-safety.md`

The human-approved 2026-10-03 addition of `pub model: ModelRef` to
`WyrdGatewayCall` and the same-spec R1 correction of TASK-003's native-401
wording are treated as authoritative, not drift. I reviewed the complete
base-to-candidate range and expanded the supplied navigation map through the
shared Workflow facade, Skald route planning, authenticated transport, public
gateway ingress, fallback codec, Python ownership handoff, and their recorded
tests.

This was a strict source-only review. I did not build, compile, run tests, run
Cargo or Mise, or execute any other verification command. Verification entries
below are the implementer's recorded results, assessed against the source that
purports to exercise them.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-024/031/045/046 and AC-019: one shared Rust remote handle exposes exact create/get/cancel/wait behavior, stable create idempotency, one-second polling, direct snapshots, and drop-without-cancel semantics | `crates/shared/wyrd-client/src/workflow/remote.rs:23-109`; Rust SDK re-export in `sdks/wyrd-sdk-rust/src/lib.rs` | Recorded `shared_workflow_client_contract` and `test:shared` results cover first acceptance/replay, stable key on retry, routes, terminal statuses, canonical errors, and dropped polling | PASS |
| REQ-036A/038/039/043 and approved model amendment: every local WyrdGateway attempt carries the Prompt-derived model and immutable fallback/deadline/cancellation/correlation; supported public dialects dispatch and Vertex refuses before IO | `crates/skald/skald-workflow/src/plan.rs:264-294`; `route.rs:62-95,283-315,368-425,546-590`; `crates/shared/wyrd-client/src/workflow/gateway.rs:50-122,215-284` | Recorded Skald route proof and `public_gateway_call_context_and_errors` cover Prompt model carriage, four public projections, Vertex refusal, cancellation, timeout, and concurrent isolation | PASS |
| Native model POST is sent once; 401 renews through the existing auth owner without replay, failed renewal propagates its auth error, successful renewal preserves the original refusal | `crates/shared/wyrd-client/src/transport/http.rs:350-402` has one request send and only calls `force_refresh` after reading a 401 response | Recorded focused test covers uncoded and known-code 401, one model POST, refreshed bearer on the next call, original refusal, and failed renewal | PASS — prior FIND-TASK-003-2 closed |
| REQ-043 and INV-012: native refusal normalization retains only safe status/code/field/message/remediation and never trusts provider-controlled recognized-code message text | `crates/shared/wyrd-client/src/workflow/gateway.rs:155-211` discards envelope messages and uses derive-backed catalog title/remediation for recognized codes | Recorded focused test supplies canary text in OpenAI, Anthropic, and Google recognized-code envelopes and compares the normalized result to catalog metadata | PASS — prior FIND-TASK-003-3 closed |
| AC-011A: the fallback header is exactly unpadded base64url over the JCS UTF-8 serialization; after authentication every affected ingress parses and validates it, rejects malformed/oversized/empty/duplicate/self-referential values before dispatch, and never forwards it | Producer uses `serde_jcs` at `crates/wyrd-spec/src/gateway/policy.rs:126-140`; ingress decoding at `policy.rs:142-173` base64-decodes and deserializes but does not establish that the received JSON bytes are the JCS form; authenticated consumers are `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:198-224,261-303,343-430` and `routes.rs:863-920,1769-1805` | Recorded policy test asserts the producer's canonical bytes but passes only producer output to the decoder; malformed cases at `policy.rs:639-655` and the PG ingress proof do not include a semantically valid non-JCS encoding | **FAIL — B-001** |
| AC-011A/INV-020: authentication precedes fallback interpretation; absence preserves tenant policy; only typed fallback reaches `GatewayCallRequest`; header never reaches a provider | Protocol middleware establishes `Caller` before handler execution in `ingress.rs:101-169`; handlers decode only in the successful-caller branch; provider dispatch is built from typed request fields | Recorded `public_ingress_workflow_fallback` covers all four ingresses, authentication precedence, absence, assignment, and non-forwarding | PASS |
| AC-012: all affected served OpenAPI operations describe the fallback header, encoding, limits, and stable refusal | `FALLBACK_HEADER_DOC` at `ingress.rs:189-196`; operation parameters at `routes.rs:834-919` and `ingress.rs:226-245,305-329` | Recorded served OpenAPI contract result, rather than codegen alone | PASS |
| REQ-058 and INV-004/011/012: SDK/CLI local dependency assembly is shared, explicit native injection remains, only selected ExtGateway secrets resolve at run, loading/apply dispatch nothing, and provider credentials do not enter Skald or Cards | `crates/shared/wyrd-client/src/workflow/mod.rs:96-149`; `local.rs:29-143`; `global_config.rs:12-35`; redacted reader at `wyrd-utils/src/secret.rs:23-63` | Recorded selected-dependency proof covers Native, ExtGateway, public WyrdGateway, selected/unselected binding behavior, loading, and failure before dispatch; recorded shared/gateway/server evidence covers the moved reader | PASS |
| REQ-058/INV-007: a Python Workflow loaded through explicit Cards context or authored external refs retains that client through mutation and run; locally built values remain client-less | `sdks/wyrd-sdk-python/src/workflow.rs:203-214,304-435,480-580`; `state/mod.rs:2614-2638`; shared owner at `wyrd-client/src/workflow/mod.rs:33-40,74-94,123-170,244-270` | Recorded Python gateway journey exercises registered and authored-ref loads, ambient server B, absent ambient config, successful and refused mutation, and client-less local refusal | PASS — prior FIND-TASK-003-1 closed |
| Shared secret reading exposes only a redacted cross-module result and preserves existing file/env safety | `crates/shared/wyrd-utils/src/secret.rs:23-77`; plaintext file stage is private; gateway, server config, and local Workflow consumers use `read_secret_ref` | Recorded focused gateway credential checks and shared family evidence; static source confirms no public plaintext helper | PASS — prior FIND-TASK-003-4 closed |
| Mandatory module-level import placement remains satisfied | Imports are at the owning module/test-module tops in `wyrd-utils/src/secret.rs:11-18`, `wyrd-client/src/workflow/mod.rs:273-293`, and `wyrd-spec/src/gateway/policy.rs:570-580` | Static source proof plus recorded lint result | PASS — prior FIND-TASK-003-5 closed |
| Non-goals and drift boundary: no new ingress, Vertex endpoint, provider-body Workflow field, credential mutation, polling option, public arbitrary-header API, duplicate transport/parser/graph/executor, or language server-run lifecycle surface | Complete cumulative diff and current callers; native transport extension is crate-private and reuses the shared auth/pool; fallback uses the existing DTO/JCS/base64 facilities | Source review; recorded boundary checks for the exercised dependency surfaces | PASS |
| AC-013: focused proof directly exercises every TASK-003 behavior claimed complete | Recorded focused, family, language, codegen, boundary, gateway, PG, and served-OpenAPI results are present and consistent for the behaviors above except strict JCS receipt | No test supplies valid but noncanonical JSON bytes to the decoder or public ingress | **FAIL — B-001** |

## Proposed findings

### B-001 — fallback ingress accepts wire encodings outside the exact JCS contract

- **Classification:** INCORRECT
- **Violated obligation:** The approved Revision 12 gateway boundary and
  TASK-003 require `wyrd-gateway-fallback` to be unpadded base64url over the
  JCS UTF-8 serialization of `GatewayFallbackOverride`; AC-011A requires proof
  of the exact encoding and parsing contract.
- **Exact location:**
  `crates/wyrd-spec/src/gateway/policy.rs:153-172`, especially the direct
  `serde_json::from_slice` at lines 167-168. The public consumer is
  `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:207-224`.
- **Evidence and reachability:** `to_header_value` creates canonical JCS bytes,
  but `from_header_value` accepts any JSON byte representation that serde can
  deserialize. An authenticated caller can therefore base64url-encode, for
  example, a whitespace-bearing or differently ordered representation of the
  same override and reach `GatewayInvocation` successfully. The unit test at
  `policy.rs:624-655` proves the producer emits JCS and rejects several invalid
  documents, but it never gives the receiver a semantically valid non-JCS
  document. The PG ingress test builds its header through `to_header_value`, so
  it cannot falsify this path either.
- **Observable consequence:** The server accepts a public header value outside
  the one documented and approved canonical wire format. Clients and agents
  cannot rely on exact-byte canonicalization being enforced, and the task's
  claim that invalid header encodings dispatch nothing is broader than the
  implemented receiver behavior.
- **Required testable correction:** In the existing
  `GatewayFallbackOverride::from_header_value` owner, after deserialization and
  semantic validation, use the already-installed `serde_jcs` mechanism to
  require that the decoded bytes equal that value's canonical JCS bytes;
  otherwise return the existing `GatewayContractError` for field `fallback`
  before dispatch. Extend the existing policy/header proof with at least one
  valid-but-noncanonical JSON encoding and prove it is refused; include it in
  the existing public-ingress proof if needed to establish pre-dispatch
  behavior. Do not add a new codec, setting, option, checker, dependency, or
  test harness.

## Prior-finding closure

| Prior finding | Closure assessment |
|---|---|
| FIND-TASK-003-1 | Closed. Python now stores `wyrd_client::Workflow`, both loaders preserve it, mutation uses `as_skald_mut`, and `run` delegates to the retained owner. The recorded journey covers both registered and authored external-ref paths under changed/absent ambient configuration. |
| FIND-TASK-003-2 | Closed. `post_native` has one model send; a 401 renews without replay and propagates renewal failure. The corrected task wording matches source and the recorded focused cases. |
| FIND-TASK-003-3 | Closed. Recognized codes use catalog title/remediation; native envelope message text is discarded for all supported dialects and canary proof is recorded. |
| FIND-TASK-003-4 | Closed. The plaintext file stage is private and only `read_secret_ref -> SecretString` crosses the module boundary. |
| FIND-TASK-003-5 | Closed. The three cited imports are now in owning module blocks with their Unix cfg preserved. |

## Review findings

### Critical

None.

### Important

- **B-001 — `crates/wyrd-spec/src/gateway/policy.rs:153`:** The receiver does
  not enforce the approved canonical JCS bytes for the fallback header. This
  leaves an accepted public wire shape outside the exact task contract. Reuse
  `serde_jcs` in the existing decoder and extend the existing focused proof;
  no new machinery is warranted.

### Suggestions

None. This acceptance review does not prescribe optional improvements.

## Open questions

None. B-001 has a bounded correction within approved behavior and existing
owners.

## Verification notes

- No commands were executed by this reviewer under the human's strict
  read-only/source-only direction.
- The implementer recorded passing focused tests for remote transport, gateway
  context/error normalization and all five prior remediation findings, plus
  shared, Python, gateway credential, codegen, boundary, formatting, and lint
  lanes. Source supports those claims for the exercised cases.
- Exact receiver-side JCS enforcement has no recorded proof, and source shows
  the missing comparison. This is a finding for the implementer to correct and
  rerun, not a verification limit that can be waived.

## Overall result

**FAIL**

One bounded original-task behavior finding remains: **B-001**.
