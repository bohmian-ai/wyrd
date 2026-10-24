# Independent findings validation

Recommendation: **SPEC_REVISION_REQUIRED** for the application-owned exporter public entry point. Six other findings are bounded implementation corrections under the current approval. Neither PASS nor BLOCKED is supported: the proposed failure paths are traceable, but required behavior is missing. Do not package the exporter API choice as an implementation-only remediation.

Subject: cumulative `036fc48fe26fb87648a277b10099bc475d293b92...b1ff72dc34c344b596c85cf408e6a9217685be35`, TASK-002 and TASK-009, approved working-tree revision 15. HEAD remained `b1ff72dc34c344b596c85cf408e6a9217685be35`. This is a fresh independent structured Ponytail validation, with no prior FIND IDs to preserve. Only this report was written.

## Inputs and method

Read subject.md, all eight discovery reports, followup-review.md, claim-comparison.md, verification.md and authority-observation.md. Read the task-review skill, repository and applicable nested rules, workflow, maintainer/Rust guidance, the approved task/spec/supplement clauses and owning architecture. CodeGraph was used before source searches. The source audit used the complete cumulative range as its subject, expanded the discovery navigation through actual producers, production mounts, shared owners, callers, sibling writers and consumers, and compared each proposed correction with the smallest existing mechanism.

TASK-003's canonical tables, schema/projection, gateway capture/readback and physical layouts remain deferred. TASK-009 explicitly owns the invocation/accepted-execution and evaluator context seams; its scenario 3 cannot be satisfied by a status echo. This report does not demand a TASK-003 warehouse migration or final row-readback suite here.

## Every discovery claim reconciled

| Source claims | Disposition | Result and source proof |
| --- | --- | --- |
| B-001; INV-DIRECT-001; FUP-AUTH-001 | CONFIRMED | FIND-002-1. Direct execution passes no subject into the shared Card-access decision; enqueue does. The native/gateway security-domain PASS did not inspect or close this direct consumer. |
| B-002; FUP-AUTH-002 | REVISED | FIND-002-2. Required User-state native admission fails. Preserve the synchronous verifier and repair server composition, including contradictory prose added in this range; do not simply remove the Gate refusal. Security-domain fail-closed assessment is correct but does not prove the positive flow. |
| B-003; INV-BINDING-002; DATA-01; MAINT-01; SYS-001 | REVISED | FIND-009-1. One lost-occurrence producer causes all these symptoms. Existing projected BindingId is usable only when activation was projected; explicit-only bindings intentionally have no such row. Reject any correction requiring invented Trigger activation or a BindingId for every explicit check. |
| B-004; INV-ENV-003; DATA-02; MAINT-02; SYS-002 | REVISED | FIND-009-2. One frozen-context handoff is missing across direct/claim/engine and scheduled/group writers. Require accepted/evaluator context and selector behavior; defer canonical warehouse projection/readback to TASK-003. |
| STD-002009-01 | REVISED | FIND-009-3. The materially extended target read violates the mandatory tenant-capability boundary. It is not evidence of a demonstrated cross-tenant HTTP exploit; the existing SQL matching check is real. Correction stays in this extended workflow. |
| STD-002009-02 | CONFIRMED | FIND-002-3. Each cited item was checked against its source body/docs and base-to-candidate change. Detailed inventory below. Root AGENTS explicitly makes missing changed-item rustdoc blocking, including private/test items. |
| CONC-001 | REVISED | FIND-009-4. Configured Python/TS exporter producer is inaccessible to supported public callers. The proposed thin projection needs a public API decision not actually specified by the packet; route that decision to specification approval. |
| Maintainer/security remarks that native signed-only refusal is locally appropriate | REVISED | Correct as a property of the store-free Gate verifier; insufficient as the production composition for the approved existing User bearer flow. See FIND-002-2. No separate finding. |
| Standards/maintainer MCP Service\|Agent wording note | REJECTED as a blocking finding | `mcp/principals.rs:175` has stale wording, while the Service-only schema and enforcement are correct. No demonstrated behavior or independent blocking consequence. It may be corrected alongside changed-owner docs without a separate FIND ID or gate. |
| Provider-retry/long-lived-object proof limits in concurrency report | REJECTED as an additional implementation finding | Snapshot-before-await paths are present in `_http_auth.py:108–136` and TS `index.ts:3227–3264`; absent journey assertions do not establish a dispatch defect. Retain the disclosed evidence limit, not speculative new transport machinery. |
| Security-domain no additional crypto/federation/account findings; data-domain no additional mutation/activity/replay findings | Validated empty proposals | No opposing source claim or independent material defect established at those boundaries. Their PASS statements do not override the specific consumers above. |

## Final deduplicated ledger

### FIND-002-1 — CONFIRMED / INCORRECT: direct verification omits subject admission

Discovery: B-001, INV-DIRECT-001, FUP-AUTH-001. Obligation: REQ-010/012/033, TASK-002 scenarios 2/3/7, auth supplement Authorization and Direct verification (`:383–405`, `:782–787`).

Exact location: `crates/wyrd/wyrd-server/src/components/verification/service.rs:490`, with `authorize_run:932–973`.

Source trace: issuance `ResolvedContext::resolve` (`wyrd-auth/src/issuance.rs:355–380`) produces RootRequired without a root or Scoped with a resolved root. Runtime `RbacCheck::check` (`wyrd-runtime/src/permission_check.rs:65`) checks only operation permissions. HTTP `components/verification/routes.rs:348–365` and MCP `mcp/verification.rs:197–224` both call `VerificationControl::execute`, whose entire engine path is `run_direct:472–636`. Its `None` subject skips both Card-access refusal arms. `resolve:849–895`, `Target::resolved` and `VerificationTarget::refusal` check target existence/readiness, not caller membership. Hosted blocks can run a supplied scalar/code check without a gateway call or trace read; there is no universal downstream repair. Direct result staging uses the requested subject. Sibling enqueue (`:683–689`) already passes the subject to the same decision/audit owner.

Reachable consequence: a family account holding a verifier operation grant can execute Card-sensitive work rootless; a rooted caller can execute a same-tenant subject outside its signed graph. This was not made acceptable by the old `None` call: the cumulative task introduces the required explicit RootRequired/Scoped contract at this consumer.

Decision-complete correction: pass the direct request's subject through existing `authorize_run`, before reservation/engine work, exactly as enqueue does. No second authorization service, live-role re-resolution or gateway-only guard. Preserve the single audit decision, System execution attribution and authorized AnyRegistered rootless verification.

Closure proof: focused direct HTTP/MCP cases for RootRequired refusal, scoped outside/inside graph and authorized AnyRegistered rootless success. Refusals must produce no engine/provider dispatch or staged result/input. Reuse current real-server verification test owners.

### FIND-002-2 — REVISED / MISSING: native state IO refuses an authorized User's selected root

Discovery: B-002, FUP-AUTH-002. Obligation: REQ-013; TASK-002 scenarios 3/4; auth supplement User access token and WyrdState (`:408–419`) and rootless selector semantics (`:396–405`).

Exact locations: `crates/vala/vala-bifrost-redux/src/gate/auth.rs:157–184`; production composition `crates/wyrd/wyrd-server/src/grpc/mod.rs:360–365`; contradictory added owner prose `architecture/wyrd-security-posture.md:495–499`.

Source trace: state `client/pinned` (`wyrd-client/src/state.rs:762–787`) binds the state root. `AuthMiddleware::bearer:966–980` returns an existing User bearer unchanged. `bifrost/grpc.rs:272–314` sends it beside that root. `build_app_grpc` mounts Gate directly; Gate `write_batch:1145–1157` calls its synchronous authenticate. `needs_root_resolution` (`wyrd-runtime/src/request_context.rs:91–100`) correctly answers true for a rootless User bearer. Gate then refuses rather than resolving it. The existing OTLP sibling (`server/src/grpc/otlp.rs:275–305`) already verifies, calls `AuthenticatedPrincipal::select_root:86–105`, and supplies the verified context to Gate. `wyrd-auth::select_request_root:397–426` reuses bounded root resolution, narrows the signed Card snapshot and preserves account/grants/chain/expiry, without recording activity.

Consequence: the approved existing User bearer state can use rooted HTTP operations but cannot acknowledge its native observations. A User is not required to possess an exchangeable machine credential. The refusal is safe locally but incomplete behavior.

Authority reconciliation: the signed-only native exception was introduced by this cumulative implementation, as shown by its architecture diff. The approved packet explicitly replaces owning prose and includes User-bearer native state IO. It also allows registry work when a rootless request explicitly selects a root, while normal signed-token verification stays synchronous/store-free. Keeping the exception would require narrowing approved behavior; implementing the already approved flow does not require a new security policy.

Decision-complete correction: compose the existing server-owned optional-root selection before native Gate admission, reusing the HTTP/OTLP shared auth owner. Update the introduced exception accordingly. Keep the store-free verifier, native payload/permission/row attribution, limits, readiness, lifecycle and acknowledgement ordering. A consumer-side selector guard remains necessary at this real trust boundary; do not remove it without a validated replacement context, ignore the selector, require Service credentials for Users, add per-record SQL, or crash the shared server on resolution failure.

Closure proof: existing User Bifrost bearer + loaded state native observation reaches acknowledgement with unchanged User identity and validated owner/subject. Unknown/foreign/wrong-kind roots, signed-root mismatch, missing operation grant and out-of-graph subject refuse. Already-rooted machine and bare rootless generic admission remain valid. Exercise the actual native production mount, not only selector/parser or OTLP tests.

### FIND-009-1 — REVISED / MISSING: selected binding occurrence is discarded before acceptance

Discovery: B-003, INV-BINDING-002, DATA-01, MAINT-01, SYS-001. Obligation: REQ-015/052, INV-004; TASK-009 approach 4, scenario 3 and implementation reference explicitly require retaining the resolved occurrence through request and accepted work.

Exact locations: `crates/shared/wyrd-client/src/state.rs:887–939`; `src/observe/mod.rs:363,468,504–519`; `crates/wyrd-spec/src/verification/execution.rs:40–65`; server `verification/cache.rs:168–203`; `components/verification/service.rs:739,800`; SQL `queries/verifier_executions.rs:105–133`.

Source trace: a Run retains its component alias, but all three bound-verifier callers pass only subject/name. `bound_verifier` collects every matching component and Agent binding, selects a first match, and returns only subject/verifier/spec. Target/request cannot encode the selected site. The server's new `binding_environments` repeats UID matching and turns every occurrence into a set of selectors. `effective_environment` refuses the whole set when distinct selectors exist; equal selectors erase site identity. Direct/input staging sets binding_id None, manual queue insertion omits binding/owner, so retry cannot restore it. Sibling scheduled/group queue and result paths retain existing binding identity.

Existing authority to reuse: `BindingProjector::freeze` (`components/cards/service.rs:1341–1420`) traverses owner `OWNER_OCCURRENCE_KEY`, component alias and standalone Agent sites; `NewBinding::subject_occurrence_key` (`queries/verification.rs:296–310`) defines the total natural key with owner/subject/verifier. Crucially `freeze` skips bindings lacking `runs_on`. `resolve_binding:2915–2930` resolves projected activated rows only. Therefore a proposal to use BindingId alone for all bound explicit calls is incomplete and would invent activation/persistent semantics.

Consequence: two legal aliases of one subject using one Verifier either spuriously conflict across environments or collapse to indistinguishable accepted work. A component's binding and that Agent's own binding also cannot be distinguished. This is invocation loss, before TASK-003 row projection.

Decision-complete correction: retain the site already selected by the existing Run/bound-verifier path, using the established containing Card and occurrence-key semantics plus exact subject/verifier. Extend the existing typed invocation/accepted-work contracts to carry and validate that selected site, as expressly directed by TASK-009; reconcile only its selector. Reuse projected BindingId where it exists, and existing pinned Card binding resolution for explicit-only sites. Do not require an activated SQL binding for an explicit-only binding, infer a site from shared UID membership, create a second registry/traversal, or guess first-match identity. Preserve permitted unbound explicit verification and existing scheduling/group lifecycle.

This is already approved public invocation behavior: the task explicitly directs carrying resolved occurrence through its existing request contracts, and existing owner/alias semantics fix the identity. Private DTO composition/field spelling does not require a new product decision. If implementation discovers it needs a different identity model or activation semantics, that would return to specification; this correction does not authorize either.

Closure proof: current real SDK/server fixtures with two component aliases of the same Card and same Verifier, both equal and differing selectors. Each selected direct/submitted occurrence must stay distinct through claim/retry/restart; forged owner/site/subject/verifier combinations refuse before staging. Include an explicit-only binding and preserve authorized unbound User execution. No new warehouse schema/readback demanded here.

### FIND-009-2 — REVISED / MISSING: effective environment never reaches the executing work

Discovery: B-004, INV-ENV-003, DATA-02, MAINT-02, SYS-002. Obligation: REQ-052/053, TASK-009 scenario 3; auth supplement `:437–454`, `:789–795`.

Exact locations: server `components/verification/service.rs:513–549`; SQL `queries/verifier_executions.rs:73–85,105–133,410–428,1324–1372,2934–3072`; server `verification/runner.rs:418–472`, `verification/hosted/blocks.rs:62–79,123–159`, `components/gateway/invocation.rs:133–140`; population consumer `verification/drift.rs:120–165,778`.

Source trace: direct admission computes the effective environment but only returns it in VerificationResult. Enqueue canonicalizes the request with the effective environment; manual insert persists it and status reads it. CLAIM_SQL, ClaimedRow/into_claimed, ClaimedExecution and ExecutionInput omit it. Runner reconstructs Caller and Execution with no environment. Hosted engine binds evidence, trace reader and server gateway caller with no such value. Scheduled `resolve_binding`/INSERT_SCHEDULED and grouped INSERT_GROUP/enqueue_group also omit the binding selector; these are sibling writers of runnable work, not a reason for another queue. ObservationWindow builds subject/series/time predicates only and receives no selector. AcceptedAuthority contains authentication facts, not an alternative hidden environment path.

Consequence: queued status may truthfully echo prod while the actual restarted engine/gateway-origin context cannot know prod. Environment-scoped population evaluation likewise cannot receive the accepted selector. The scripted restart proof returns before real dispatch (`runner.rs:418–423`), so its status assertion cannot close this seam.

Decision-complete correction: freeze the one already reconciled optional selector in the existing execution/input snapshot, retain it through claim/decode/retry into direct and queued engine/gateway-origin/evaluator context; scheduled/group acceptance must obtain its binding selector through the existing exact binding owner. The population owner must receive and honor that scope before judging according to approved mixed/unknown/whole-trace semantics. Preserve account authority, leases, deadlines, unscoped/null behavior, complete authorized trace inputs and no schedule fanout. Do not use worker defaults, token claims, a new ambient source, a new queue or a second evidence reader. Final canonical columns/capture/projection remain TASK-003; do not require those deferred changes as this correction.

Closure proof: focused real-engine/local-provider case across claim/restart observes the accepted selector at the gateway invocation seam; direct and scheduled/group contexts retain their appropriate selectors too. At the existing population query/evaluator seam, prove selector application precedes judging and that unscoped/whole-trace rules are unchanged. Use existing query preparation tests for predicates where final canonical-table fixtures belong to TASK-003; final cross-table readback runs there. A status-only or EngineScript-only assertion is insufficient.

### FIND-009-3 — REVISED / VIOLATION: extended tenant read bypasses TenantConn

Discovery: STD-002009-01. Obligation: SQL AGENTS Connections/Transactions; security Postgres tenant binding: tenant functions/fields accept TenantConn, never bare PgConnection; only the boundary binds tenant authority.

Exact locations: `crates/wyrd/wyrd-sql/src/queries/verification_target.rs:78–145`; server `verification/cache.rs:217–224,319–330`; new `20261026000000_verification_deployment_environment.sql:46–52`.

Source trace: the new owner-card read materially changes VerificationTargets::read and its SQL function. It retains raw connection + independent tenant ID, adds a fourth function argument and reads the owner through manually bound SQL. The cache miss takes `postgres.conn()`, while submission (`submissions.rs:175–182`) and runner (`runner.rs:321–329`) erase their TenantConn into its transaction. SQL refuses a differently bound connection and binds the supplied tenant locally. These protections matter: no request-level tenant crossover is proven. Nevertheless the expanded tenant capability still accepts an unbound raw connection and independently supplied authority, violating the explicit security/type boundary on the changed workflow. It is not a request to clean all old SQL debt.

Decision-complete correction: retain the existing reader/cache and use TenantConn throughout its tenant read, taking the existing tenant connection constructor on pool misses and preserving callers' current tenant transactions for acceptance/claim reads. Remove redundant tenant rebinding from the replacement read path; keep target readiness, cache invalidation, one logical target read, same-tenant visibility and accepted snapshots. Do not widen to OperatorPool or invent a connection wrapper. Respect immutable applied migration rules when altering the function.

Closure proof: production reader/cache signatures and retained fields expose only approved capabilities; focused existing target/tenant/environment tests verify foreign rows remain invisible, cross-tenant rebinding cannot be requested, and acceptance/claim stay in their original transaction. Directory-limited tenant-isolation checks do not replace signature review. No exploit or higher severity is inferred solely from the raw type.

### FIND-002-3 — CONFIRMED / VIOLATION: changed Rust items miss the mandated rustdoc contract

Discovery: STD-002009-02. Obligation: root AGENTS Code and agent_docs/rust.md Rustdoc explicitly cover every changed/private/test item, require Errors for fallible contracts and Panics for possible panic paths, and make missing rustdoc merge-blocking. This one correction spans baseline TASK-002 and supplemental TASK-009; it must not be split into repeated style rounds.

Exact locations and individual confirmation are in the inventory below. The fallible-owner list has eleven function items (the source report groups the two exchange functions in one row). Each is changed in the cumulative range and lacks the stated section; `run_direct` returns a tuple containing Result and is still fallible. Trait implementation inheritance does not repair the missing changed-item contract: the changed trait method also lacks its Errors section.

Consequence: required private/test failure and invariant contracts are absent despite reported format/clippy success. This is the explicit rustdoc exception to the review's usual non-blocking style threshold.

Decision-complete correction: add accurate local rustdoc to the confirmed inventory, batching it with bounded implementation corrections and any additional Rust items they change. Document actual workflow/errors/panic invariants; preserve executable behavior, assertions and error handling. No boilerplate promises, suppressions, checker, fixture rewrite or runtime test solely for docs. Existing format/lint plus source review closes this finding. The two OTLP indexing helpers and their callers require panic docs even though they have no explicit panic macro; Signal::rejected calls the documented panic-capable HttpEncoding::decode.

### FIND-009-4 — REVISED / MISSING / SPEC_REVISION_REQUIRED: configured application-owned exporters have no public producer

Discovery: CONC-001. Obligation: spec automatic telemetry ownership paragraph following REQ-053; auth supplement `:456–460`; TASK-009 scenario 2 and implementation reference require state-bound exporter integration when a provider already exists.

Exact locations: Python `python/wyrd/otel.py:226–240,346–406`, native `src/state/mod.rs:231–237`, public client `src/client.rs:244–274,364–381`; TS `wyrd/src/otel.ts:25–59`, public/native-bound state `wyrd/src/index.ts:3147–3154` and client `:1589–1599,1620–1641`.

Source trace: configure creates an immutable private state-bound Rust client and deliberately leaves the supplied client unchanged. Managed telemetry privately obtains it; state HTTP adapters privately obtain it too. Python and TS public exporter factories accept only WyrdClient plus optional root UID. Their shared `_session`/`config` reads the client environment, but supported public client constructors, UID rebinding and delegation cannot acquire the state's configured environment. The property is read-only. The public state supplies no exporter capability or client accessor. Thus after automatic startup correctly refuses an application-owned provider, following its prescribed current exporter path exports without the configured environment for all three signals. Rust has `state.client()` and does not share this public producer gap. Reading private Python fields or bypassing TypeScript private members is not supported integration.

The defect cannot be dismissed as TASK-003 table work: the request headers lack the configured value before transport. It also cannot be solved by mutating the caller's client or adding an exporter-local environment option; those contradict immutable state ownership and the task's no exporter-specific copy rule.

**Exact unresolved public decision:** choose and approve how a Python/TS application passes its configured state into stock exporter creation while preserving plain-client factories, all three signals, freeze timing, renewal and provider lifecycle. Existing factories accepting a state, versus an exported state-bound factory/method, are different user-visible contracts. The packet calls this an “existing state-bound exporter integration” but the inspected public source has no such integration for configured environment. Its instruction to “extend existing exporter/correlation owners” approves the capability/outcome, not one of these new public entry-point contracts. Unlike occurrence, there is no explicit approved exporter input contract or existing public state binding value to carry. TASK-009 expressly forbids adding a public state.client accessor. REQ-054's approved state.http_auth/httpAuth is a provider-request adapter, not an OTLP exporter input, and cannot silently be repurposed.

Return this public API choice to the specification owner under the task-review stop rule. Do not prescribe a signature or package this as decision-complete implementation remediation. Retain the current bounded shared auth/exporter owners after approval; no new provider pipeline, token cache or durable behavior is needed. The approval must make the application-provider remediation real or explicitly revise that promise; deleting the promise is not currently authorized.

Post-approval closure proof: each language attaches span/log/metric exporters for two independently configured states over one unmodified client to application providers (or drives stock exporters against a local receiver), observes each root/environment before and after renewal, preserves plain-client behavior and typed automatic-provider refusal, and confirms no replacement or flush of the application's provider. Transport-level proof suffices here; canonical row projection remains TASK-003.

## Verification, execution evidence and reconciled acceptance

The five exact review-time tests in verification.md passed, alongside diff-check. They substantiate immutable state/cache isolation, Python/TS adapter capture and fail-closed claim decoding. No test claims to prove the missing direct/native/exporter/actual worker paths. Existing task evidence was assessed, including principal/account/delegation/federation and SDK journeys; no broad rerun is required to diagnose these source omissions.

The late TASK-009 evidence suffix asserting a waived independent diagnostician is excluded from frozen authority and is not accepted as a waiver. The journey repair diffs retain refusal/lifecycle assertions, use an out-of-graph stale bundle instead of the now-authorized ambient admin premise, and replace Agent roots with Service account roots as approved. Those changes are not source evidence of an authorization weakening. The original record identifies stale premises, but this review has no independent diagnostician report for those two repairs; do not claim that execution-discipline proof was supplied or that the late waiver was authorized. No separate unproven runtime defect is invented from that evidence gap.

| Acceptance area | Reconciled result |
| --- | --- |
| Stable accounts, atomic restrictions/credentials, explicit signed authority, federation/delegation, activity and existing accepted principal snapshot | No retained functional finding beyond specific consumers above. Discovery source and existing evidence support bounded acceptance. |
| Scoped/root-required direct verification | FAIL, FIND-002-1. |
| Existing User bearer + state native IO | FAIL, FIND-002-2. |
| Immutable configure, Service-only roots, state provider-request adapters and plain adapter preservation | Focused evidence/source support PASS within task scope; final cross-table projection remains deferred. |
| Exact selected bound occurrence and accepted execution environment | FAIL, FIND-009-1 and FIND-009-2. |
| Application-provider exporter remediation | FAIL requiring public decision, FIND-009-4. |
| Changed tenant capability and rustdoc obligations | FAIL, FIND-009-3 and FIND-002-3. |
| TASK-003 schemas/projection/storage/readback and unrelated final-docsite work | Not required by this review; absence does not create a finding. |

Recommended bounded implementation scope after resolving the exporter decision: shared direct admission; native server optional-root composition and introduced owner prose; existing binding selection/request/accepted-work seam; environment snapshot/claim/engine/gateway-origin/evaluator seam and sibling scheduled/group writers; the materially extended tenant target read; and the exact rustdoc inventory. Focused proof follows each seam above, using existing test owners. No new dependency, queue, registry, transport, global context or broad cleanup is justified.

Skipped live Postgres/IdP/Vault/UI/provider reruns, generators and broad aggregates; existing evidence limits remain. Risk: green local tests/status echoes do not establish the required production consumer behavior, and exporter remediation cannot be finalized without the public contract decision.

## Rustdoc item-by-item source validation

The following inventory is independently checked against the actual source and the cumulative base. Every row is changed/new in this range; none is an unrelated untouched-item cleanup. D = no rustdoc; E = no Errors section on a fallible contract; P = no Panics section despite an explicit or reachable panic path. Every row is CONFIRMED. Locations are candidate line numbers. The table is an inventory of one FIND-002-3 correction, not separate findings.

| Item | Cumulative changed status / validation | Missing contract and evidence |
| --- | --- | --- |
| `crates/shared/wyrd-auth-oidc/src/registry.rs:206` `WorkloadBindingResolver::binding` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/shared/wyrd-client/src/auth.rs:1169` `exchange` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/shared/wyrd-client/src/auth.rs:1187` `exchange_workload` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/wyrd/wyrd-auth/src/jwt_bearer.rs:153` `resolve_workload_binding` | CHANGED / CONFIRMED | D/E: no item rustdoc; fallible result, no Errors section |
| `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:230` `PgWorkloadBindingResolver::binding` | CHANGED / CONFIRMED | D/E: no item rustdoc; fallible result, no Errors section |
| `crates/wyrd/wyrd-server/src/boot/mod.rs:2024` `seed_federation` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/wyrd/wyrd-server/src/boot/mod.rs:2122` `build_workload_binding` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/wyrd/wyrd-server/src/components/verification/service.rs:472` `run_direct` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/wyrd/wyrd-server/src/verification/cache.rs:88` `Target::resolved` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/wyrd/wyrd-testing/src/server.rs:3662` `bootstrap_machine` | CHANGED / CONFIRMED | D/E: no item rustdoc; fallible result, no Errors section |
| `crates/wyrd/wyrd-testing/src/verification.rs:176` `release_with` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/shared/wyrd-client/src/auth.rs:2063` `spawn_workload_mock` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/shared/wyrd-runtime/src/request_context.rs:268` `selector_never_rebinds_a_signed_root` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/gate/mod.rs:2609` `rootless_family_account_needs_a_root_to_attribute_cards` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:3857` `context_for_leasing` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:3233` `native_run_id_is_replaced_by_one_authoritative_nullable_field` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:3518` `scoped_service_principal` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:3697` `member_uids` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/scribe/ingress.rs:1298` `oversized_decoded_request_is_rejected_before_wal_append` | CHANGED / CONFIRMED | P: panic expression `assert!` |
| `crates/vala/vala-bifrost-redux/src/scribe/replay.rs:2100` `nested_accepted_subset_replays_one_fence_and_digest` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd-spec/src/auth/workload.rs:352` `workload_uid_selector_accepts_one_card_uid` | CHANGED / CONFIRMED | P: panic expression `assert_eq!` |
| `crates/wyrd-spec/src/error.rs:4515` `api_key_invalid_remediation_names_credential_issue` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert_eq!` |
| `crates/wyrd-spec/src/vala/audit_detail.rs:1331` `audit_detail_golden_vectors_cover_all_variants_and_nested_values` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/card_scope.rs:416` `issue_scope_error_passes_through_other_errors` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-auth/src/card_scope.rs:514` `uid_of` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/card_scope.rs:527` `resolve_single_service_card_produces_own_scope` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/card_scope.rs:548` `resolve_refuses_foreign_and_unknown_roots_identically` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/card_scope.rs:586` `resolve_scope_populates_every_member_uid` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:719` `unregistered_uid` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1068` `api_key_exchange_issues_no_refresh_token_or_row` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1185` `seed_suspended_account_key` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1349` `cross_tenant_key_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1373` `revoked_key_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1408` `hash_mismatch_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/issuance.rs:1366` `issue_signs_current_grants_and_follows_grant_changes` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/issuance.rs:1477` `unregistered_uid` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:863` `stored_human_connection_requires_the_sub_subject_claim` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:992` `binding_resolver_prefers_audience_then_falls_back` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/refresh.rs:1365` `a_machine_refresh_row_cannot_rotate` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/revoke.rs:251` `a_service_account_is_revoked_under_the_service_kind` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/revoke.rs:310` `a_mismatched_kind_is_refused_as_not_found` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/revoke.rs:338` `a_failed_revocation_write_keeps_its_cause_server_side` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/service_accounts.rs:36` `principal_with_permissions` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-cli/src/auth/workload_binding.rs:226` `add_parses_required_args` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-cli/src/auth/workload_binding.rs:243` `add_requires_principal` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-cli/src/auth/workload_binding.rs:258` `add_requires_subject` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-cli/src/auth/workload_binding.rs:273` `add_accepts_optional_audience` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-cli/src/principal/assignment.rs:236` `list_accepts_only_assignable_kinds` | CHANGED / CONFIRMED | P: panic expression `assert!` |
| `crates/wyrd/wyrd-cli/src/principal/revoke.rs:106` `revoke_parses_all_kinds` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query/investigation.rs:370` `refuse_unauthorized_or_oversized_reads` | CHANGED / CONFIRMED | P: panic expression `assert_eq!` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:171` `workload_jwt_bearer_mints_service_token` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:237` `workload_jwt_bearer_falls_back_to_tenant_slug_with_service_root` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:291` `workload_jwt_bearer_refuses_an_agent_default_root` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:341` `wrong_audience_is_rejected_and_audited` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:398` `expired_assertion_is_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:440` `untrusted_issuer_is_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:475` `server_owned_binding_denial_returns_principal_not_found` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:512` `cloud_workload_subject_shape_uses_server_owned_binding` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:560` `same_issuer_subject_is_tenant_scoped` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:612` `stable_account_mapping_selects_only_allowed_context` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:762` `api_key_grant_still_mints` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:810` `test_state_with_external` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:892` `bootstrap_principal` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:1201` `binding` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:1274` `card_ref` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/boot/mod.rs:2461` `sample_binding_entry` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/boot/mod.rs:2486` `workload_binding_rejects_a_non_https_issuer` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect_err(` |
| `crates/wyrd/wyrd-server/src/components/admin/routes.rs:1474` `delete_issuer_with_live_binding_conflicts_then_cascades` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/components/admin/routes.rs:1525` `workload_binding_crud_round_trip` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/components/admin/routes.rs:1614` `create_binding_for_unknown_issuer_is_not_found` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/components/auth/principal_extractor.rs:206` `extension_hit_returns_stored_principal_without_invoking_verifier` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/components/auth/routes.rs:570` `mounts_token_routes` | CHANGED / CONFIRMED | D: no item rustdoc |
| `crates/wyrd/wyrd-server/src/components/auth/routes.rs:575` `extracts_tenant_from_unverified_access_token` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/config.rs:5385` `workload_binding_toml_round_trip` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/config.rs:5435` `workload_binding_entry_unknown_field_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect_err(` |
| `crates/wyrd/wyrd-server/src/config.rs:5578` `binding_entry_empty_issuer_invalid` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/config.rs:5596` `binding_entry_empty_subject_invalid` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/config.rs:5614` `binding_entry_malformed_principal_id_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect_err(` |
| `crates/wyrd/wyrd-server/src/config.rs:5670` `bindings_without_tenant_slug_invalid` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/verification/cache.rs:524` `targets_serve_only_while_listening_and_until_invalidated` | CHANGED / CONFIRMED | P: panic expression `assert!` |
| `crates/wyrd/wyrd-server/src/verification/cache.rs:595` `effective_environment_follows_the_selector_rules` | CHANGED / CONFIRMED | P: panic expression `assert_eq!` |
| `crates/wyrd/wyrd-server/tests/integration/identity_e2e.rs:94` `binding_card_ref` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/tests/integration/identity_e2e.rs:505` `workload_jwt_bearer_unbound_subject_returns_404_keycloak` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/src/queries/auth/principal_directory.rs:151` `directory_excludes_unassignable_principals` | CHANGED / CONFIRMED | P: panic expression `assert_eq!` |
| `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:538` `restriction_statements_stay_tenant_implicit` | CHANGED / CONFIRMED | P: panic expression `assert!` |
| `crates/wyrd/wyrd-sql/src/queries/auth/workload_bindings.rs:299` `upsert_targets_composite_key_and_overwrites_columns` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_admin_principals.rs:575` `tenant_admin_principal_persists_without_a_card` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_admin_principals.rs:611` `tenant_admin_principal_cannot_carry_a_workload_restriction` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:711` `human_connection_upgrade_preflight` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:1681` `cloud_issuer_fk_rejects_delete_with_live_binding` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:1745` `cloud_issuer_binding_maps_a_stable_account` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:1889` `cloud_issuer_read_path_roundtrips_and_rls` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:2039` `cloud_issuer_crud_write_path_conflict_and_cascade` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_executions.rs:499` `request` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs:214` `card_attributed_resource_spans` | CHANGED / CONFIRMED | P: indexes fixture resource/scope vectors at `[0]` |
| `crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs:227` `card_attributed_resource_logs` | CHANGED / CONFIRMED | P: indexes fixture resource/scope vectors at `[0]` |
| `crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs:961` `Signal::body` | CHANGED / CONFIRMED | P: calls the indexing/panic-capable signal fixture helpers |
| `crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs:979` `Signal::rejected` | CHANGED / CONFIRMED | P: calls `HttpEncoding::decode`, whose response decode uses expect |
