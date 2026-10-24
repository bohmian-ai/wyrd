# TASK-002 and TASK-009 cumulative acceptance verdict

**SPEC_REVISION_REQUIRED**

The cumulative candidate does not pass either task's completion gate. Independent validation retains six bounded implementation findings and one missing public contract decision: Python/TypeScript applications cannot construct exporters carrying their configured state's environment through the supported public API. The approved packet requires that integration but does not define its public entry point. This review does not choose one or revise the specification.

## Immutable subject and governing authority

| Item | Reviewed subject |
| --- | --- |
| Repository | `/home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry-task-002` |
| Base | `036fc48fe26fb87648a277b10099bc475d293b92` |
| Candidate | `b1ff72dc34c344b596c85cf408e6a9217685be35` |
| Branch | `task/service-identity-and-telemetry/TASK-002` |
| Original task | [TASK-002](../../tasks/TASK-002-stable-accounts-and-auth.md), including its historical revision-13 baseline and original acceptance obligations |
| Supplemental task | [TASK-009](../../tasks/TASK-009-configured-service-context.md), revision 15, superseding the original Agent-workload allowance and adding configured context/provider adapters |
| Approved specification | [Revision 15](../../spec.md), its auth, table/correlation and telemetry supplements, root/nested repository rules and applicable architecture owners |
| Scope | Complete cumulative base-to-candidate implementation, including TASK-002 and TASK-009; not just the final fixes or implementation summaries |

[subject.md](subject.md) records the common navigation map and initial authority hashes. Source HEAD and the branch ref still matched the candidate at final inspection; runtime source, tests, SDKs, scripts, fixtures, docs and TESTING.md had no working-tree diff from it. Approved uncommitted plan/architecture revisions were explicit review authority, not additional source implementation. [authority-observation.md](authority-observation.md) records an external four-line TASK-009 evidence append during review: removing only that suffix exactly restores the initial hash. It changes no obligation and is excluded as acceptance authority or proof of a diagnostician waiver. The original reviewed source and normative approval remained fixed.

No prior task-review verdict exists for TASK-002/TASK-009. There are no prior FIND IDs or remediation closures to preserve. TASK-001/TASK-004 historical reviews are dependency context, not prior rounds of this subject.

## Independent review results

| Required role | Report | Discovery result |
| --- | --- | --- |
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| Repository standards | [standards-review.md](standards-review.md) | FAIL |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL |
| System resilience | [system-review.md](system-review.md) | FAIL |
| Security/RBAC domain | [domain-review-security.md](domain-review-security.md) | PASS within its inspected boundary |
| Persistent authority/tenancy domain | [domain-review-data.md](domain-review-data.md) | FAIL |
| Client context/concurrency domain | [domain-review-concurrency.md](domain-review-concurrency.md) | FAIL |
| Focused auth follow-up | [followup-review.md](followup-review.md) | RESOLVED |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | SPEC_REVISION_REQUIRED recommendation; seven deduplicated retained findings |

All required reports are present. The orchestrator grouped claims but did not validate its own findings. [claim-comparison.md](claim-comparison.md) records why follow-up was required: the security review's local fail-closed assessment did not resolve direct-verification admission or the required positive native User-state flow. The focused reviewer traced both production consumers. Independent validation then assessed those claims, every unique proposal and all repeated binding/environment proposals against source. Agreement was not treated as proof.

The native signed-root-only sentence was introduced by this implementation and conflicts with the approved User-bearer state/native flow. Validation selects the already existing server optional-root-selection owner while keeping signature verification store-free; it does not authorize a new grant or wider Card policy. The exporter entry point has no equivalent approved public contract to reuse and is the separate specification decision below.

## Reconciled acceptance matrix

PASS here means source plus available evidence supports this bounded obligation; it does not claim independent reruns of every implementation suite. The complete obligation matrices and producer-to-consumer evidence remain in the behavior/invariant reports, reconciled by findings-validation.md.

| Obligation | Implementation and verification evidence | Result |
| --- | --- | --- |
| TASK-002 scenario 1; REQ-001–006: stable explicit accounts, owned credentials, no registration principal/grant; atomic unique family restrictions | Principal routes, shared account/credential owners, service-account SQL and migration; recorded account/SDK/CLI/MCP proofs | PASS |
| TASK-002 scenario 2; REQ-007–009/033: signed explicit Card states and bounded authority; TASK-009 scenario 1: Service-only roots/families | Shared issuer/root resolver, WorkloadKind/CardAccess and claim verifier; recorded issuer/root tests; review-time exact claim test passed | PASS at producers; direct consumer fails below |
| TASK-002 scenario 3; REQ-010/012/033: Card-sensitive direct verification enforces RootRequired/Scoped | `run_direct` passes no subject to the existing authorization decision; enqueue supplies one; focused positive proofs do not cover this omission | FAIL, FIND-002-1 |
| REQ-013; TASK-002 scenarios 3/4: existing authorized User bearer with automatic state root can emit native observations | State sends bearer plus selector; direct native Gate mount refuses unresolved selection; HTTP/OTLP already have a server-owned selection mechanism | FAIL, FIND-002-2 |
| TASK-002 scenario 4; REQ-013–014: local state/root isolation, root-keyed renewal/cache | Immutable derived client context, root-sensitive auth and producer caches; exact review-time cache/configure tests passed | PASS |
| TASK-002 scenarios 5/6: delegation intersections/provenance and federation mapping; preserved human/platform/System boundaries | Shared delegation/issuer/federation owners and recorded SDK/identity tests; no additional validated finding | PASS within reviewed scope and disclosed external-test limits |
| TASK-002 scenarios 7/8; REQ-014–017/040: accepted principal/grant snapshot, current scheduled authority and exact account/root activity | AcceptedAuthority, acknowledged Scribe handoff, stored authority, scheduler and activity SQL; recorded authority/restart/activity tests | PASS for principal authority/activity; binding/environment fail below |
| TASK-009 scenario 2; REQ-050: immutable configure defaults, validation/freeze, state isolation and automatic provider conflict | State OnceLock and thin projections/provider ownership; exact Rust/Python/TS context tests passed; recorded configured SDK journeys | PASS for configure/managed provider behavior |
| TASK-009 scenario 3; REQ-015/052; INV-004: selected binding occurrence survives direct/submitted/recovered work | Run alias is discarded by bound-verifier/request; cache merges occurrences; acceptance cannot recover the missing identity | FAIL, FIND-009-1 |
| TASK-009 scenario 3; REQ-052–053: effective environment reaches real execution/gateway origin and server-read populations | Admission/status carry a value; claim/engine/scheduled/group/evaluator handoffs omit it; scripted restart returns before actual engine dispatch | FAIL, FIND-009-2 |
| TASK-009 scenario 4; REQ-054–056: state-bound stock-provider adapters, per-request snapshot, conflict refusal, plain-client preservation | Existing HTTPX/HTTPX2/fetch owners extended, context captured before await; focused adapter tests passed and provider journeys recorded | PASS for implemented adapter paths; retry coverage limits retained |
| Application-owned provider integration required by REQ-053 follow-on ownership rule and TASK-009 scenario 2 | Public Python/TS factories cannot consume the private configured state client/environment; no approved public exporter entry point | FAIL requiring specification decision, FIND-009-4 |
| Required tenant SQL capability and changed-item rustdoc gate | Extended target read takes raw connection/rebinds tenant; independently checked changed-item rustdoc inventory remains incomplete | FAIL, FIND-009-3 and FIND-002-3 |
| INV-001–008 and regression boundaries | No new credential store, permission model, token descriptor expansion, environment grants, schedule fanout, global mutable Service context or provider-specific wrapper; direct authority and exact-context gaps remain above | FAIL where mapped to retained findings; other inspected non-goals preserved |
| TASK-003 warehouse schemas/default layouts/projection/readback, TASK-006 later trace work, TASK-008 docsite completion | Explicitly deferred task ownership | N/A; not findings in this review |

## Validated finding ledger

Detailed source traces, sibling writers/callers, decision-complete bounded recommendations and focused closure proofs are preserved in [findings-validation.md](findings-validation.md). The following is the reconciled ledger, not discovery proposals.

| Stable ID / validation / classification | Violated obligation and exact location | Consequence and correction boundary | Focused closure proof |
| --- | --- | --- | --- |
| **FIND-002-1**, CONFIRMED, INCORRECT | REQ-010/012/033; server `components/verification/service.rs:490` | Direct checks skip Card subject admission. Pass the request subject through existing `authorize_run` before reservation; preserve AnyRegistered privilege and one audit decision. Sources B-001, INV-DIRECT-001, FUP-AUTH-001. | Real direct HTTP/MCP RootRequired and scoped-negative cases refuse without engine/result/provider work; authorized scoped/rootless cases succeed. |
| **FIND-002-2**, REVISED, MISSING | REQ-013; Bifrost `gate/auth.rs:157–184`, server `grpc/mod.rs:360–365`, introduced security prose `:495–499` | Existing User-token state native IO is refused. Compose existing server optional-root resolution before Gate, preserve store-free verifier and all native admission/lifecycle behavior; fix conflicting introduced prose. Sources B-002, FUP-AUTH-002. | Actual native mount acknowledges authorized User-state IO; wrong/foreign/unknown roots, signed mismatch, missing grant and out-of-graph subject refuse; generic/rooted cases remain valid. |
| **FIND-002-3**, CONFIRMED, VIOLATION | Root AGENTS rustdoc gate; exact item-by-item locations in validation inventory | Required Errors/Panics/item docs absent on changed Rust owners/helpers/tests. Batch accurate local docs with bounded corrections, preserving assertions and behavior; no new checker or style-only round. Source STD-002009-02. | Source review of the confirmed inventory and newly changed remediation items, plus relevant existing formatting/lints. |
| **FIND-009-1**, REVISED, MISSING | REQ-015/052, INV-004; client `state.rs:887–939`, `observe/mod.rs:504–519`; request `execution.rs:40–65`; server `cache.rs:168–203`, acceptance `service.rs:739,800`; SQL `verifier_executions.rs:105–133` | Exact occurrence collapses to subject/verifier, losing identity or falsely conflicting across selectors. Carry existing owner/containing Card/occurrence-key semantics through selected request and accepted work; use BindingId where projected, pinned binding resolution for explicit-only sites. No invented activation/mandatory SQL binding. Sources B-003, INV-BINDING-002, DATA-01, MAINT-01, SYS-001. | Two aliases of one Card/Verifier, equal and differing selectors, explicit-only binding, direct/submit/restart; preserve selected site and reject forged combinations; unbound authorized checks remain. |
| **FIND-009-2**, REVISED, MISSING | REQ-052–053; direct `service.rs:513–549`; SQL claim `:410–428`, DTO `:1324–1372`, scheduled/group `:2934–3072`; runner `:418–472`, hosted blocks `:62–79,123–159`; population `drift.rs:120–165,778` | Status echoes a selector unavailable to actual execution. Freeze and carry the effective selector through existing direct/accepted/claim/engine/gateway-origin/evaluator context and sibling scheduled/group writers; preserve authority, leases, trace/unscoped rules and deferred TASK-003 projection. Sources B-004, INV-ENV-003, DATA-02, MAINT-02, SYS-002. | Real engine/local provider observes original selector across restart; binding-only scheduled/group context retained; population selection precedes judgment; status/script-only proof insufficient. |
| **FIND-009-3**, REVISED, VIOLATION | SQL capability rules; `verification_target.rs:78–145`, server cache `:217–224,319–330`, new environment migration `:46–52` | Extended tenant read erases TenantConn and manually binds independent tenant authority. Keep existing reader/cache, retain TenantConn and existing caller transactions, acquire it at pool misses, remove redundant rebinding with immutable-migration discipline. No demonstrated cross-tenant exploit is claimed. Source STD-002009-01. | Capability signature/source check plus focused target/tenant/environment regressions; retain transaction and visibility boundaries. |
| **FIND-009-4**, REVISED, MISSING / SPEC_REVISION_REQUIRED | Approved application-provider remediation; Python `otel.py:226–240,346–406`, state `mod.rs:231–237`; TS `otel.ts:25–59`, state `index.ts:3147–3154` | Configured state environment has no supported public producer for external exporters. Approval must define how Python/TS applications supply configured state to existing stock-exporter creation, preserving plain-client factories, all signals, freeze/renewal and provider ownership. This review does not choose factory state input versus a state-bound exporter surface or add forbidden `state.client`. Source CONC-001. | After contract approval: two configured states over one unchanged client export distinct roots/environments for spans/logs/metrics before/after renewal; automatic conflict/plain-client/application-provider ownership behavior stays intact. |

The stale MCP description and provider-retry evidence limitations do not create additional findings. Rejected proposals are not remediation requirements. The late diagnostician-waiver statement is not accepted as authorization; independent source inspection found no authorization weakening in the repaired stale-premise journeys, but their missing independent diagnostic report remains an evidence limit.

## Verification limits and routing

[verification.md](verification.md) records five exact independently rerun passing tests: two Rust client configure/cache tests, the claim-verifier test, Python adapter snapshot and TS state context, plus diff-check. No empty test selection was accepted. Existing task evidence supports other account, delegation, federation, SDK and runtime coverage; source findings expose paths those assertions do not cover.

No live Postgres/IdP/Vault/UI/provider journey reruns, generators or broad aggregate suites were performed during review. Existing UI-build, IdP, Vault and Keycloak exclusions remain. Several implementation proofs have no recorded RED; provider journeys disable retries and do not independently establish every renewal/retry promise. A passing status read or scripted restart does not establish execution-context preservation.

**Remediation task: none created under this verdict.** The task-review skill requires a public API decision to return to specification approval rather than being prescribed as implementation remediation. Resolve FIND-009-4 through `$wyrd-spec` and update the task packet through `$wyrd-plan`; then package the six bounded corrections with the approved exporter contract for `$wyrd-implement`. This does not authorize an API choice, require broad cleanup, or reopen settled identity/layout decisions. TASK-002 would independently require bounded fixes; TASK-009 additionally requires this public contract decision. Neither is complete.

Reviewed source was not implemented, merged, committed, pushed or deployed. Only review reports were written.
