# TASK-003 R1 findings validation

Subject: prior candidate `c93287b54bf944c8c8cc9323f81c12ba5174369c` to remediation candidate `d3db737e5fccacbe02421ea135fd5327b6335d79`; original cumulative base `a5b1d54174227b26e2f3eb09e833a9172d32e2ab`. Evidence commit `fa990be1989959de963d062ad8a47336b40c62da` adds only the seven prior review reports and implementation evidence. The working production source, tests, public documentation, spec/task and build configuration match the pinned candidate.

The fresh remediation validator read the original approved revision 1, TASK-003, prior verdict and validated findings, implemented R1 task and actual remediation diff. Review checks the changed owners and their affected consumers, retaining prior specialized evidence for unchanged five-owner encryption, lifecycle and refresh behavior. Root owns serial executable verification; this validator runs no Cargo command and writes only assigned review artifacts.

## Prior finding closure

### FIND-TASK-003-1 — CLOSED: trusted-issuer replacement preserves references

`admin/routes.rs:645` mounts the typed `PUT /v1/admin/trusted-issuers/client-secret` operation and applies the existing `service_accounts:write` permission, staging its decision before fallible work. It rejects an empty replacement, reads through the authenticated tenant's `TenantConn`, commits that read, then calls `seal_trusted_issuer_replacement` (`wyrd-auth/src/pg_resolvers.rs:900`) with the supplied replacement. That helper uses the existing ClientSecretOwner context and WyrdKms seal; it never decrypts the old envelope. The second transaction calls `swap_trusted_issuer_secret` (`wyrd-sql/src/queries/auth/trusted_issuers.rs:222`): exact observed-envelope CAS updates only ciphertext and modification time, leaving issuer identity, client metadata and workload bindings untouched. Deletion or another secret write prevents the update; the handler returns the existing 409 conflict without reinsertion.

The request lives in `wyrd-spec/src/auth/admin.rs:109` and is re-exported by the owner. `wyrd-cli/src/auth/trusted_issuer.rs:299` exposes the new verb, reuses existing file/environment secret resolution and authenticated transport, and prints only the issuer. No SDK or MCP method was added.

`admin/routes.rs:1794` asserts the unchanged view, changed ciphertext, exact new secret from PgIssuerResolver, retained binding, successful replacement over corrupted old bytes, empty-input and reader refusals, KMS outage without mutation, unsupported secretless issuer, deletion race, missing-resource result and seven allowed/one denied decisions. `identity_e2e.rs:5636` composes the real CLI request with a real server and unchanged Keycloak workload binding. The supplied evidence and review reruns are bound in `verification.md`.

### FIND-TASK-003-2 — CLOSED: composed renewal and consumer proof

`identity_e2e.rs:5636` creates an active confidential Keycloak connection and an actual human session, renews via the served HTTP route, asserts identity/revision/state and redaction, uses the existing access token for a delegated `/v1` write, refreshes the old session and completes a fresh confidential sign-in. Its `human_login` helper drives provider authentication and the common callback/token redemption, so this reaches the actual reopened secret rather than merely inspecting a view.

The same journey authors the trusted issuer and binding through the real CLI, exchanges a real workload JWT, then runs CLI renewal and replacement against the real server, checking changed envelopes and successful exchange through the same binding. A missing replacement source fails and leaves ciphertext unchanged; success output excludes the submitted secret.

`compatible.rs:937` now renews over HTTP, waits on the existing snapshot invalidation fixture, requires another Transit decrypt during preparation, invokes the provider after renewal and finally asserts the entire captured upstream key sequence, including the unchanged rotated key after renewal. Warm calls still cause no further Transit request.

Proof is interpreted at the actual protocol boundary: Keycloak workload JWT exchange does not authenticate using the trusted issuer's stored client secret. That journey proves routed CLI behavior and binding continuity, not provider acceptance of the replacement secret; the owner test separately asserts the exact new secret returned by PgIssuerResolver. No unsupported authentication protocol is required to close the finding. The controlled Transit fixture is HTTP wiremock, not a live Vault process; inherited real-provider facade proof remains applicable.

### FIND-TASK-003-3 — CLOSED: exact served Operator contract includes renewal

`pg_openapi_contract.rs:2111` retains exact operation-set equality and now expects six operations including the renewal POST. It also asserts no request body, the typed redacted OperatorConnectionView 200 response and UUID path parameter. The test is renamed to `operator_connection_contract_publishes_six_typed_operations`. This corrects the reproduced five-versus-six mismatch without weakening the check.

### FIND-TASK-003-4 — CLOSED: owner-specific response and effect documentation

`reference/stored-secret-encryption.md:117` preserves semantic identity, settings, state, revision and session effects while explicitly allowing `updated_at` and Operator `updated_by` changes. Its per-owner table distinguishes disabled Operator successful renewal from inactive human 404, and records each owner's 404/409 cases. The trusted-issuer replacement row identifies the new HTTP/CLI operation, binding retention and supported secret sources. The generated llms projection carries the same changes. No production lifecycle is changed to fit the documentation.

### FIND-TASK-003-5 — CLOSED: allowed decision survives persistence acquisition failure

`HumanConnections::begin_locked` (`wyrd-auth/src/connections.rs:851`) now stages the existing decision once before acquiring the tenant transaction or advisory slot lock. All five existing callers retain that shared mechanism, with no later duplicate stage. The renewal route's preparation-refusal branch remains responsible for staging its earlier failures, so KMS refusal still records one decision and never opens a write transaction.

The augmented `admin/identity.rs:864` test uses separate runtime pools, lets Transit begin resealing, then closes only the owner's write pool. It observes the exact `WYRD_AUTH_503_VERIFY_UNAVAILABLE` error, unchanged committed candidate ciphertext and six allowed/one denied renewal decisions, including the persistence failure. This directly covers the former missing audit branch. Staging remains process-memory admission; no request waits for an audit write.

## New finding — independently validated

### FIND-TASK-003-6 — INCORRECT: new issuer replacement contract omits its store-error code

**Authority:** REQ-008's safe public response and retained HTTP boundary; `agent_docs/agent-surfaces.md` requires typed, inspectable runtime OpenAPI contracts with stable catalog errors. This applies to the new replacement operation, not an expanded review of unchanged handlers.

**Location:** `crates/wyrd/wyrd-server/src/components/admin/routes.rs:635`, the new replacement operation's 503 response annotation; backing mapper at `:1206`.

**Starting conditions and trigger:** An authorized administrator replaces an existing trusted issuer's client secret, and the store cannot acquire the read/write transaction or the read/CAS/commit fails. The operation is explicitly documented to support a 503 store refusal without exposing secret or SQL data.

**Expected:** Its served OpenAPI 503 declaration identifies the stable catalog code that this supported store-failure path returns, alongside the KMS code.

**Actual:** The annotation says the store or key service is unavailable but lists only `WYRD_SERVER_503_SERVICE_UNAVAILABLE`. The new operation sends transaction acquisition, row read, CAS and commit failures through `sql_unavailable`, which returns `WYRD_AUTH_503_VERIFY_UNAVAILABLE`. Only the KMS branch returns the advertised server code. An agent using the served declaration therefore cannot identify the documented store failure by its actual stable code. The existing generic OpenAPI code/status check proves listed codes exist and have the declared status; it does not check omission of a reachable code.

**Validation:** Fresh `r1_contract_check` independently traced this route, `acquire_conn` and `sql_unavailable` and confirmed the narrow mismatch in `focused-contract-validation.md`. This is a new fix-introduced contract defect, not incomplete closure of FIND-TASK-003-1 through -5. No other new finding was retained.

**Smallest correction:** Include `WYRD_AUTH_503_VERIFY_UNAVAILABLE` in the new operation's 503 declaration while retaining `WYRD_SERVER_503_SERVICE_UNAVAILABLE` for KMS refusal. Extend a focused served-document assertion for this PUT operation to require both actual codes. Preserve runtime errors, response status, encryption/CAS/audit behavior, and existing shared error mapping; do not change production behavior merely to fit the annotation or broaden the review to other handlers.

## Retained boundaries and limits

The new replacement performs only administrative read/prepare/CAS work and reuses the common encryption owner. The shared audit movement adds no IO. The diff changes no verification, Gateway invocation, Oracle or Scribe ingest hot path, migration, dependency, SDK/MCP/UI, session policy or refresh mechanism. Original Operator, Gateway, platform and other auth lifecycle acceptance evidence is carried forward because those owners are unchanged except for the documented shared audit stage and issuer CAS reuse.

Fresh focused execution is **5/5 PASS**: both server library owner tests (`8f0f2834-bffa-494a-a9b4-6e25d31f197e`), the exact six-operation served contract (`4a13ee3b-b65f-4633-be49-43adb0aded0c`), Gateway post-renewal dispatch (`49166cba-0a92-42fb-bf23-a4af8cae7234`) and the real Keycloak/Dex/CLI identity journey (`9b26c16d-79fe-4565-85ab-77d4336bd4b6`). Exact setup-aware commands and successful migration/build/lifecycle results are recorded separately in `verification.md`. Full lint/codegen/docs/dependency checks in the implemented R1 evidence remain implementation-reported; no additional full aggregate, live-cloud run or benchmark is claimed. Historical process records remain in the prior review and are not a new acceptance blocker.
