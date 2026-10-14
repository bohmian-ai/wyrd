# Independent security and tenancy review

## Subject and result

**PASS.** No material security or tenant-isolation finding is proposed.

Reviewed the complete base `807c4fe68ddcf43f325e970ed15f8a7c9918e0d4` to candidate `100adbc04a39ca22022fb013b3c0e93bfe903c3a` (tree `3bb993d4c32769cda67650eb02b991f981ebc3c2`), plus the remediation delta from `6294bf6bbc0acb9abc37e4e4d617e2638f9dc3f7`. The approved spec is also the original task, by explicit owner instruction. Read the prior verdict, prior independently validated ledger, and frozen remediation task/evidence. This report changes no implementation, dependencies, generated files, or other review reports.

## Boundary and authority coverage

| Boundary | Applicable authority | Source coverage |
|---|---|---|
| Accepted provider-native request data | Spec REQ-001; AGENTS §§2, 9–10; active design Prompt wire envelope and gateway dispatch; doctrine public contract alignment | `skald-spec/src/wire/openai_responses.rs` easy input decoder, input-only schema customization, tagged item variants, parts/text tool output |
| Authenticated request and permission | Security posture principles, Trust boundaries, Authorization and policy; AGENTS §9 and agent-rules | `caller_extractor.rs:44–74`; `routes.rs:915–929,1341–1358,1723–1739,1779–1821`; `invocation.rs:265–272,424–486,503–550,652–687` |
| Tenant-bound configuration and persistence | Spec INV-001; security posture Tenant and data isolation; architecture-constraints Identity and Tenant Isolation; agent-rules TenantConn/RLS | `invocation.rs:459–486,746–760,774–790`; `service.rs:656–677`; `snapshot.rs:60–104` |
| Faithful upstream body and credential boundary | Spec REQ-001; security posture credentials and SSRF controls | `adapter/mod.rs:179–192,531–540`; existing tenant snapshot authority; no modified adapter, URL resolver, credential resolver or forwarding header implementation |
| Negative-flow proof | AGENTS §11 and agent-rules real SDK journey requirement; agent-harness server-owned authz | `gateway-inference.test.ts:135–158,203–213`; `tests/support/server.ts:61–112`; served contract checks in `pg_openapi_contract.rs:1109–1135` |

The applicable reference router, architecture-constraints and agent-harness references were read. Security-posture governs authorization, tenant identity, data isolation, audit, credentials and server egress. The diff introduces no new persistent-store, concurrency, peer, token issuance, authentication, authorization or deployment owner; those domains are unchanged.

## End-to-end security and tenancy trace

1. `/v1/responses` extracts a verified `Caller` independently of body validation. `Caller::from_authenticated` derives both `data_tenant_id` and effective principal from one authenticated principal; the body never establishes tenant identity or permission. `openai_call` refuses failed authentication before protected invocation.
2. `typed_body::<GatewayResponsesRequest>` decodes borrowed JSON for validation and returns the original JSON. The new `easy_items` normalization occurs in that temporary typed object only. It cannot overwrite the caller, tenant, model projection or request headers. Non-object/malformed message shapes still fail typed deserialization. The input schema permits required string role and string-or-existing-parts content, with omitted type or exactly `message`; it does not admit arbitrary JSON as a new message alternative.
3. `invoke_openai` parses the exact model projection and constructs the existing `GatewayCallRequest`, with no body-supplied tenant authority. `GatewayInvocation::invoke` passes `authorized=false`; `admit_call` therefore calls `decide` before loading the snapshot, reservation, or provider dispatch. `decide` checks the typed model invocation permission against `caller.principal`, stages Allowed/Denied on the shared Scribe outbox under `caller.data_tenant_id`, and returns denial synchronously. `route` independently checks fallback models rather than inheriting requested-model permission.
4. The admitted tenant is `caller.data_tenant_id`. `GatewayAdministration::snapshot` opens that tenant's `TenantConn`, and deployment and credential selection derive from that snapshot. Admission and accounting use the same admitted tenant with `TenantConn`. No changed raw-pool access, caller-chosen tenant, credential assignment, audit sink or bypass is introduced.
5. Native Responses preparation copies the original object and rewrites only `model`. It does not serialize the normalized Skald input. Content-part URLs remain provider request data; the changed parser does not fetch those URLs, create an egress client or bypass an existing endpoint screening owner. Expanded input data therefore traverses the same tenant, authorization, admission, provider credential and forwarding boundary as canonical input.
6. Sibling Skald tool-result writers in `request_builder.rs` and `prompt.rs` now wrap their existing strings through `From<String>` and remain text-producing. Python's output accessor is a documented text projection; it neither dispatches an operation nor chooses a tenant. Input-schema customization changes generated request alternatives while preserving canonical item and output variants. Neither adaptation widens a protected operation.

## Prior finding closure relevant to this domain

| Prior finding | Source-supported assessment |
|---|---|
| FIND-responses-input-forms-1 | The existing real-server test owner now drives actual `OpenAIResponsesModel` with a tool. The positive test checks two Responses upstream calls, original easy input, original parts-valued tool result, and final output. The refusal test uses `server.scopedApiKey("gateway_reader", ["gateway:read"])`, acquires that caller's access token through the shared SDK, points OpenAI at the Wyrd `/v1` address, asserts 403, and asserts no upstream request. The fixture clears ambient access/workload tokens, so the reader cannot silently inherit the writer's authority. This is the correct refusal boundary and credible closure proof, with independent R2 execution now passing (one selected test per exact Vitest selector). |
| FIND-responses-input-forms-3 | The source customization on `OpenAiResponsesInput::Items` names an easy input alternative beside canonical items; role/content remain required, type constrained, and content parts reuse the existing typed part schema. The served schema test accepts the five supported shapes and rejects absent-role, numeric-content and unknown-discriminator messages. No security escape hatch or broader output contract is introduced. |
| FIND-responses-input-forms-2 | Rustdoc and authored/public Python stub document text projection and wrong-kind failure. No authorization, tenant or raw-forwarding behavior changes. |

The tests do not weaken assertions, remove refusal coverage, introduce skips, or extend timeouts to claim success. The schema unit and served OpenAPI checks exercise different publication boundaries; any preference to remove one is not a security finding and does not justify another remediation round.

## Verification and limits

The frozen implementation evidence records successful exact schema/decoder, served OpenAPI, positive Agents SDK journey and permission-refusal selectors, plus final required format/lint/codegen/typecheck/Skald lanes. This reviewer inspected source and complete diffs and read `verification.md`: the orchestrator independently reran both exact Agents SDK selectors (one test passed each), the exact served OpenAPI selector (one passed), schema/decoder selectors (two passed), `fmt:check`, and `git diff --check`, all successfully. This reviewer did not duplicate those executions; broader final lanes remain supplied implementer evidence. The disclosed lack of a pre-fix rebuilt-server journey run does not weaken the current source-level refusal or forwarding assessment, and no older checkout was created.

No new cross-tenant test is demanded: the candidate changes input decoding and contract projection, not identity or data-access ownership, and the traced changed path retains the existing verified principal and RLS boundary. No live provider, cloud, crash, or penetration test was performed. There is no source-supported new security or tenancy risk requiring remediation.

## Proposed finding ledger

Empty. Overall result: **PASS** within the security/tenancy domain.
