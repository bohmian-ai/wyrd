# Independent findings validation — TASK-003

## Subject and evidence boundary

Repository: `/home/thorrester/Documents/GitHub/wyrd`. Base: `58d07d7260df1f022a721e720a28ea48e5096e35`. Candidate: `a1792c45323489157e818eb29722913114014927`. Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`. Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12, including the human-approved 2026-10-03 `WyrdGatewayCall.model: ModelRef` amendment. HEAD and the candidate identity remained equal at final source inspection.

This fresh validator read the cumulative diff, task, applicable repository ownership/error/security/authentication rules, and all eight independent reports: behavior, invariants, standards, maintainer, system, security, concurrency, and focused follow-up. Validation used source, Git navigation, and the implementer's recorded task evidence only. No builds, tests, lanes, package-manager commands, or verification commands were executed. Historical reviewer test runs in discovery reports are not acceptance evidence for this resumed source-only review.

The applicable acceptance authority includes Revision 12's shared-client projection, selected execution dependencies, supported public ingress, safe common error shape, and immutable per-call context; TASK-003's retained Cards connection and native-error contract; AGENTS.md §§2–9; agent rules; maintainer style; errors guidance; Rust secret/visibility rules; and the architecture's API-key re-exchange requirement. No prior `FIND-TASK-003-*` ledger existed in this attempt.

## Disposition of every proposed finding

| Discovery source IDs | Disposition | Final ID | Source-backed decision |
|---|---|---|---|
| B-001 | CONFIRMED | FIND-TASK-003-1 | Python discards the dependency-owning shared Workflow before run. |
| INV-REV-001 | CONFIRMED | FIND-TASK-003-1 | Same load-to-run state loss, including authored external refs. |
| MAINT-TASK-003-1 | CONFIRMED | FIND-TASK-003-1 | Same owner split; preserve the existing shared owner. |
| B-002 | REVISED | FIND-TASK-003-2 | Duplicate native POST is reachable; code-gated replay is not a safe correction. |
| SYS-001 | REVISED | FIND-TASK-003-2 | Provider refusal amplifies into a second gateway invocation; renew without replay. |
| CONC-001 | REVISED | FIND-TASK-003-2 | Request duplication and ignored refresh failure are confirmed; existing codes prove no origin. |
| SEC-002 | REVISED | FIND-TASK-003-2 | Delete resend, but retain architecture-required reactive credential renewal. |
| FU-001 | CONFIRMED | FIND-TASK-003-2 | Existing authentication owner can renew while returning the original refusal safely. |
| INV-REV-002 | CONFIRMED | FIND-TASK-003-3 | Catalog-derived portable text closes the client's safety invariant. |
| SEC-001 | REVISED | FIND-TASK-003-3 | Leak is confirmed; rewriting shared provider relay would alter sibling native clients unnecessarily. |
| RR-001 | REVISED | FIND-TASK-003-3 | Same leak; correction belongs to public-client normalization rather than native relay. |
| FU-002 | CONFIRMED | FIND-TASK-003-3 | Recognized codes authenticate metadata spelling, not envelope message provenance. |
| RR-002 | REVISED | FIND-TASK-003-4 | Unused public plaintext helper is unnecessary surface; privatize it rather than requiring another abstraction or claiming an observed leak. |
| RR-003 | CONFIRMED | FIND-TASK-003-5 | The exact mandatory agent-rule import boundary applies to these changed nongeneric functions. This is a repository conformance failure rather than an invented style preference; correction is import relocation, with no checker or runtime change. |

Agreement was not treated as proof. The following ledger traces each retained failure and resolves its correction independently.

## Validated ledger

### FIND-TASK-003-1 — preserve the shared Workflow through the Python boundary

Status: **CONFIRMED**. Classification: **INCORRECT**. Sources: B-001, INV-REV-001, MAINT-TASK-003-1. Correction route: bounded implementation.

**Obligation.** TASK-003's shared local execution contract requires a Cards-loaded Workflow to keep its Cards connection for public WyrdGateway execution. REQ-058 and Revision 12 require language SDKs to project the shared-client owner. AGENTS.md §§2–3 and §5 keep dependencies with that owner.

**Location and producer-to-sink proof.** `crates/shared/wyrd-client/src/workflow/mod.rs:82-94` preserves the client that resolves authored external refs; `:257-263` returns the registered Workflow with `client: Some(self.cards.engine.client.clone())`. `sdks/wyrd-sdk-python/src/state/mod.rs:2635-2638` and `sdks/wyrd-sdk-python/src/workflow.rs:503-507` consume that shared value using `into_skald()`. `PyWorkflow` at `workflow.rs:208-211` stores only Skald state. Its changed `run` at `:548-551` rebuilds the shared facade via `From<SkaldWorkflow>`, whose full body at shared `workflow/mod.rs:190-197` sets `client: None`. Shared `run_with` at `:123-149` then builds an ambient Wyrd client for a WyrdGateway route. These are reachable public Python load/run methods, not dormant code.

**Sibling and affected behavior.** TypeScript `native/src/workflow.rs:21-26,134-159` stores the complete shared facade and calls its run method; Rust receives the facade directly. Python's builder constructors, `from_yaml`, `add`, `add_after`, input/output binding methods, metadata setters, getters, and validation act on Skald state. Their existing successful/failed mutation behavior must survive correction; converting a loaded owner to Skald for mutation and reconstructing it would reproduce the same loss.

**Consequence.** A graph loaded using explicit Cards server/credential overrides can execute a public gateway step against a different ambient server or principal, or fail because ambient credentials are absent. The task's claim that Python uses the same connection composition is contradicted by source. Its recorded unit/type checks and the shared Rust dependency test do not exercise the Python load-to-run handoff.

**Decision-complete minimum correction.** Keep the complete `wyrd_client::Workflow` supplied by both loaders through Python execution; reuse that owner's run composition and shared runtime bridge. Preserve builder-created and inline-native workflows using the existing Skald-to-shared conversion where no retained client exists. Preserve any retained connection when Python authoring methods modify the Skald value. Do not copy the client/bearer into a Python-specific transport or configuration owner. Existing state is sufficient; no new public API, credential option, loader, registry, or executor is required.

**Focused closure proof for the implementer.** Using existing Python runtime fixtures, load a registered WyrdGateway Workflow with explicit Cards endpoint/credential A while ambient configuration is absent or points to B, run it, and prove only A receives the model call with A's credential. Cover the authored external-ref retained-client path and preserve existing pure-native authoring behavior. Record exact focused commands and results in implementation evidence. This review does not run them.

### FIND-TASK-003-2 — stop replaying a non-idempotent native model POST on ambiguous 401

Status: **REVISED**. Classification: **INCORRECT**. Sources: B-002, SYS-001, CONC-001, SEC-002, FU-001. Correction route: bounded implementation, with correction of private task retry wording through `wyrd-plan` if required.

**Obligation.** The public caller must preserve existing governed invocation ownership and native provider refusal semantics. Its native model operation is not replay-safe. The architecture at `architecture/wyrd-design.md:553-556` requires renewable credentials to be re-exchanged on refusal; it does not require resending this non-idempotent call. Revision 12 does not introduce response provenance or model-call idempotency.

**Location and producer-to-sink proof.** `crates/shared/wyrd-client/src/transport/http.rs:365-401` sends the model body, then at `:394-398` treats any 401 as an edge refusal, ignores `force_refresh()` failure, and loops to send the entire body again. Its sole production caller is `PublicWyrdGatewayCaller::call` at `workflow/gateway.rs:94-97`. Provider `ProviderError::Status` is turned into an already-dispatched refusal by `wyrd-gateway/src/adapter/http.rs:400-422`; `adapter/mod.rs:258-308` preserves its native body after credential scrubbing. `wyrd-server/src/components/gateway/invocation.rs:553-559` retains the provider status, and `routes.rs:1339-1360` relays it. Existing provider-401 assertions at `pg_invocation_tests.rs:4152-4159,4350-4358` match that source contract. Therefore a completed provider 401 reaches the new retry branch.

**Consequence.** One Workflow call can become two gateway invocations/provider dispatches, with two sets of admission/accounting/capture/audit effects. Reusing a request ID does not make a model POST idempotent. A supplied bearer can be resent unchanged because `AuthMiddleware::force_refresh` returns that supplied token directly. A renewal error is currently hidden before the resend.

**Correction comparison and authority resolution.** Recognized auth codes do not establish that the first operation stopped before dispatch: the native provider controls those envelope members and can return a real catalog auth code. No existing trustworthy native origin discriminator was found in the actual relay/auth/error path. Changing the provider relay affects ordinary native gateway callers. Adding provenance, a retry setting, or model idempotency would create an unapproved public/security mechanism. None is needed to satisfy the stronger existing authority.

**Decision-complete minimum correction.** Remove the resend from `HttpTransport::post_native`. Keep `AuthMiddleware::bearer` before the sole POST; on 401 preserve reactive renewal through the existing `force_refresh` owner, propagate a renewal failure through its existing Wyrd authentication conversion, and otherwise return the original status/body for native normalization. The existing caller's timeout/cancellation bounds continue to cover the exchange and renewal. Do not change sibling JSON/framed/gRPC retry paths or gateway-owned fallback/settlement. TASK-003:77-83's private “existing auth refresh/retry” instruction cannot justify an unsafe native POST replay; correct that wording through `wyrd-plan` under the approved spec if interpreted to require same-call replay. There is no approved public requirement forcing new provenance, so this is not SPEC_REVISION_REQUIRED.

**Focused closure proof for the implementer.** Extend the existing transport proof with an uncoded provider 401 and a provider 401 bearing a real auth code; each must produce exactly one model POST. The uncoded refusal must normalize to `SKALD_PROVIDERS_401_AUTH`. With renewable credentials, prove one reactive renewal updates the existing credential cache without resending the call, and failed renewal returns the existing auth failure with no second model dispatch. Preserve the original response after successful renewal. The task's recorded test table at `workflow_transport.rs:664-691` omits 401 and does not close this state.

### FIND-TASK-003-3 — derive portable recognized-code messages from trusted metadata

Status: **REVISED**. Classification: **INCORRECT / VIOLATION**. Sources: INV-REV-002, SEC-001, RR-001, FU-002. Correction route: bounded implementation.

**Obligation.** Revision 12's RemoteProblem contract, REQ-043, INV-012, TASK-003:95-115, and repository error/security rules prohibit arbitrary upstream text and prompt/credential diagnostics in normalized Workflow errors. Status, recognized stable code, safe optional field, and derive-backed remediation must remain.

**Location and producer-to-sink proof.** `crates/shared/wyrd-client/src/workflow/gateway.rs:155-209`, especially `:190-198`, parses native envelope messages and retains them whenever the accompanying code is in `WyrdError::codes()`. The provider-refusal producer in `wyrd-gateway/src/adapter/mod.rs:258-308` deliberately keeps native JSON after removing the resolved provider secret. The OpenAI, Anthropic, and Google envelope members therefore remain provider-controlled; choosing a real Wyrd code admits arbitrary message text. The per-attempt gateway adapter returns that RemoteProblem unchanged, and `skald-workflow/src/attempt.rs:170-184` copies its message into WorkflowRunError before bounded snapshot projection. Bounds shorten text; they do not redact it.

**Sibling and consequence assessment.** Ordinary gateway callers are entitled to the established native provider relay; stripping codes there bundles a sibling behavior change into this task. The public Workflow caller, however, explicitly promises a safe portable error. Catalog membership verifies metadata spelling only. A provider can inject prompt or diagnostic text into the terminal error while presenting a recognized Wyrd code. Current recognized-code assertions at `workflow_transport.rs:572-662` preserve the envelope message; the canary cases at `:664-691` cover unknown codes only.

**Decision-complete minimum correction.** At the existing public-client normalization boundary, retain required HTTP status, recognized code, safe field projection, and catalog remediation, but use existing trusted catalog message metadata, such as the reconstructed Wyrd error's title, for the portable message. Continue the existing fixed-message/category path for unknown/uncoded refusals. This consumer guard belongs here because this owner defines RemoteProblem safety; changing the provider producer would alter unrelated native clients. No new catalog, origin marker, setting, response header, parser, or dependency is required.

**Focused closure proof for the implementer.** Extend `public_gateway_call_context_and_errors` with each supported native envelope carrying a real Wyrd code and a canary message. Prove no canary appears in RemoteProblem or its WorkflowRunError projection while status/code/remediation and the permitted field remain. Update genuine gateway-coded cases to assert the selected catalog-derived safe message. Reuse existing fixtures and error projection tests; do not invent a new harness.

### FIND-TASK-003-4 — remove unused public plaintext secret-file surface

Status: **REVISED**. Classification: **DRIFT / VIOLATION**. Source: RR-002. Correction route: bounded implementation.

**Obligation.** AGENTS.md §4 and Rust/security guidance require secret-bearing boundary values to use redacted wrappers. Public visibility must be earned by a real consumer; the task calls for one shared SecretRef reader, not a second plaintext API.

**Location, callers, and source history.** `crates/shared/wyrd-utils/src/secret.rs:45-59` publishes `read_secret_file -> Result<String, _>`. Its full implementation preserves the established open-handle metadata check, regular/owner-only mode refusal, UTF-8 read, 64-KiB bound, and static errors. Its sole caller at `:34` is the adjacent `read_secret_ref`, which immediately wraps the result in SecretString. The old gateway public helper was used by server configuration; the candidate replaces that consumer with `read_secret_ref`. Gateway credential loading, server managed-key configuration, and Workflow binding resolution now all call the redacted SecretRef API. There is no remaining cross-module caller of `read_secret_file`.

**Consequence and calibrated claim.** The candidate relocates an obsolete public escape hatch into a broadly shared utility despite eliminating its external consumer. This imposes unnecessary public surface and permits downstream plaintext use. No current production leak is claimed, and the transient String inside the IO implementation is not by itself the finding. This is a changed, unused public abstraction rather than unrelated old debt.

**Decision-complete minimum correction.** Make the file-reading stage private within `wyrd_utils::secret`; expose only the already-used `read_secret_ref -> SecretString` boundary. Preserve all existing file checks, errors, and async caller blocking-pool behavior. Do not add a wrapper, trait, API alias, new policy, or checker. Deletion of public visibility is sufficient; changing the internal algorithm is unnecessary.

**Focused closure proof for the implementer.** Static source/API inspection must show no public plaintext file-reader surface and unchanged three consumer paths. Reuse the existing gateway/server/shared secret-file behavior coverage and record its implementation results; no new runtime test is needed solely for visibility.

### FIND-TASK-003-5 — restore the declared module import boundary

Status: **CONFIRMED**. Classification: **VIOLATION**. Source: RR-003. Correction route: bounded source-layout correction.

**Obligation and authority reconciliation.** `architecture/agent-rules.md:10` explicitly places every import at the top of its module and permits only test-module imports and a narrow single-generic-function trait exception. The invoked task-review skill explicitly audits repository standards, and AGENTS.md requires following this authority. General review restraint against personal style preferences does not make this specific mandatory conformance obligation inapplicable. No stronger approved or human direction exempts these imports. The standing standard-only direction does not require a novel mechanism here: ordinary module imports already exist throughout the owning modules. The finding requires source correction only, never a new checker.

**Exact source and owning bodies.** `crates/shared/wyrd-utils/src/secret.rs:66` imports PermissionsExt inside nongeneric `restrictive`; `crates/shared/wyrd-client/src/workflow/mod.rs:472` imports it inside nongeneric `secret_file`; `crates/wyrd-spec/src/gateway/policy.rs:622-623` imports base64 names inside nongeneric `fallback_header_round_trips_and_refuses`. The full function bodies and module import blocks were inspected. None meets the documented exception. The candidate itself puts PermissionsExt at the credential test-module top, demonstrating the existing applicable pattern.

**Consequence and minimum correction.** The file/test-module import blocks omit dependencies used by their bodies, contrary to the repository's required dependency manifest, so the changed code does not meet mandatory source acceptance. Move these imports to their owning module/test-module import blocks, preserving Unix cfg restrictions and behavior. Do not add a lint, feature, allowlist, checker, helper, or new test.

**Focused closure proof for the implementer.** Static review of the corrected source establishes import placement and unchanged function bodies. Record ordinary applicable formatting/lint evidence in implementation; runtime proof solely for moving imports is unnecessary.

## Complexity assessment

An initial preference-level reading of RR-003 was reconsidered against the exact invoked standards obligation. The final disposition is the source-backed conformance finding above. No other proposal is rejected wholesale; speculative or sibling-changing correction alternatives are rejected within the revised findings.

The changed concrete client handles, selected-route planning value, required two-implementation gateway trait, fallback DTO/codec, and shared SecretRef reader have actual owners/callers and satisfy approved outcomes using existing native/dependency facilities. No extra mechanism, configuration setting, option, feature, compatibility path, or bespoke verification file is required by the retained corrections. The only retained DRIFT is the unused public helper surface, removable by narrowing visibility. Under the human standing direction, none of the remediations may add an unestablished mechanism or check.

## Validation result and proof limits

The independently validated ledger contains five bounded findings. No unresolved source/caller uncertainty or unavailable required validation report remains. Replay's private task instruction has a bounded task-correction route; stronger authority does not require same-call replay or new provenance. The model amendment is approved and has no finding. There is no prior finding-closure claim in this first attempt.

The implementer records green focused/family/language/codegen/boundary/OpenAPI evidence. That evidence is accepted only for the states its source tests exercise. It omits Python connection retention, ambiguous/provider-origin 401, and recognized-code message canaries; those omissions are closure obligations on the implementation findings above, not review permission to execute verification. No source changes, verification reruns, merge, push, or implementation were performed by this validator.
