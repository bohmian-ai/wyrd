# TASK-002/TASK-009 authority-resolution verdict

**FIX_REQUIRED**

This supersedes the prior SPEC_REVISION_REQUIRED routing decision under the
maintainer-approved revision-16 telemetry contract. It does not supersede or
rewrite the prior source evidence, claim dispositions unrelated to the revised
contract, or implementation history. Five bounded implementation findings remain.

Superseding maintainer disposition: FIND-009-1 is limited to committed spec
REQ-012/015, INV-004 and AC-005 binding-occurrence preservation. FIND-009-2 as
written is rejected; its replacement requires unchanged accepted correlation
context on direct/queued inputs/results across restart, never execution input.
No implementation fix or new runtime proof is claimed.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry-task-002`.
- Complete source range: `036fc48fe26fb87648a277b10099bc475d293b92` to
  `b1ff72dc34c344b596c85cf408e6a9217685be35`.
- Branch: `task/service-identity-and-telemetry/TASK-002`; HEAD remains the candidate.
- Original tasks: [TASK-002](../../tasks/TASK-002-stable-accounts-and-auth.md)
  and supplemental [TASK-009](../../tasks/TASK-009-configured-service-context.md).
- Current approved authority: [spec revision 16](../../spec.md), the three
  normative supplements and applicable repository/architecture rules.
- Original review: [revision-15 verdict](../TASK-002-and-009-b1ff72dc3/verdict.md),
  [subject](../TASK-002-and-009-b1ff72dc3/subject.md),
  [validation](../TASK-002-and-009-b1ff72dc3/findings-validation.md).

This is a continuation resolving the former specification stop on the same
source, not a new implementation candidate or a post-remediation acceptance
review. The maintainer explicitly directed the proposed two telemetry paths
into the spec and requested remediation work. That instruction approves the
revision-16 change; no additional approval round is imposed.

## Revision-16 decision and prior-finding disposition

Wyrd-managed telemetry uses frozen state configuration and managed startup.
Application-managed telemetry supplies standard resource Service/version/
environment and uses supported authenticated OTLP without requiring WyrdState
or a new exporter API. Accepted request environment retains precedence; without
it, typed resource environment promotion applies to owned and unowned telemetry.
UIDs and graph/permission checks, not resource labels, establish attribution.
Existing application providers retain lifecycle ownership and are not replaced
or flushed by Wyrd. No schema, partition, sort, Bloom or authorization expansion.

**FIND-009-4 is REJECTED under superseding human-approved authority.** A public
state-bound external-exporter producer is no longer required. This is not a
claim it was implemented or that its former missing behavior was proved fixed.
Do not implement a state-taking exporter, public state.client accessor or
separate exporter identity configuration as remediation. The revised two-path
proof is AC-026: TASK-009 owns configure/provider refusal, TASK-003 owns canonical
three-signal OTLP projection/readback, and TASK-008 publishes the tested usage.

Five stable IDs remain open; FIND-009-3 is rejected by the later focused validation below. None has been closed by a source fix.

## Later tenant-exception clarification

The maintainer confirmed the TenantConn default and existing single-statement
hot-path exceptions preserving tenant isolation. A fresh independent focused
[source validation](statement-tenant-exception-validation.md) found
VerificationTargets explicitly covered by STATEMENT_TENANT_OWNERS at both base
and candidate. The candidate preserves SECURITY INVOKER, verified tenant context,
transaction-local RLS, differently bound-tenant refusal and caller transaction
lifetime. No tenant escape or out-of-exception behavior was established.

**FIND-009-3 is REJECTED**, not fixed. Its prior blanket conversion recommendation
missed this existing exact exception. Current remediation removes that work and
preserves the one-statement path without added database round trips. Governing
documentation now explicitly records the same existing three owners; this grants
no new exception and weakens no isolation. The original reports remain historical;
this later independently validated disposition supersedes that one finding.

## Independent review inputs and follow-up

The complete cumulative audit's required independent discovery reports remain
preserved and applicable to the unchanged source:

| Role | Preserved report | Original result |
| --- | --- | --- |
| Behavior | [behavior](../TASK-002-and-009-b1ff72dc3/task-review-behavior.md) | FAIL |
| Invariants | [invariants](../TASK-002-and-009-b1ff72dc3/task-review-invariants.md) | FAIL |
| Repository standards | [standards](../TASK-002-and-009-b1ff72dc3/standards-review.md) | FAIL |
| Maintainer | [maintainer](../TASK-002-and-009-b1ff72dc3/maintainer-review.md) | FAIL |
| System resilience | [system](../TASK-002-and-009-b1ff72dc3/system-review.md) | FAIL |
| Security/RBAC | [security](../TASK-002-and-009-b1ff72dc3/domain-review-security.md) | PASS within its boundary |
| Persistent authority/tenancy | [data](../TASK-002-and-009-b1ff72dc3/domain-review-data.md) | FAIL |
| Client concurrency/context | [concurrency](../TASK-002-and-009-b1ff72dc3/domain-review-concurrency.md) | FAIL |
| Focused auth follow-up | [follow-up](../TASK-002-and-009-b1ff72dc3/followup-review.md) | RESOLVED |
| Original independent validation | [original validation](../TASK-002-and-009-b1ff72dc3/findings-validation.md) | Specification stop plus six bounded findings (historical) |
| Revised-authority validation | [current validation](findings-validation.md) | Initially six retained; former exporter requirement rejected |
| Later tenant-exception validation | [statement validation](statement-tenant-exception-validation.md) | FIND-009-3 rejected; five retained findings remain |

The focused follow-up was needed because local fail-closed security assessment
did not establish direct subject admission or the required positive native User
flow. Its trace remains valid. No further discovery was needed for the unchanged
source: this continuation introduces no code or new independently alleged
failure path. A fresh independent validator rechecked the critical consumers,
reconciled all seven IDs under revision 16 and checked the combined remediation.
The later focused validation supersedes its FIND-009-3 disposition.
The orchestrator did not replace validation with its own conclusions. No required
original report is missing and no new full discovery audit is claimed.

A separate read-only comprehension reader reviewed the revision-16 clauses,
related task/supplement instructions and combined remediation. It found no
material contradiction, unresolved public decision or proof/dependency gap.
That is comprehension evidence, not task completion or runtime verification.

## Reconciled acceptance matrix

The original behavior/invariant matrices remain the full task-obligation map;
the current validator reconciles the changed contract and retained defects.

| Obligation / boundary | Evidence and revised result |
| --- | --- |
| Stable accounts, atomic restriction/credential changes, explicit signed authority, delegation/federation, exact account/root activity and accepted principal snapshot | Original source/evidence retains bounded acceptance; no new validated finding. |
| RootRequired/Scoped direct verification | FAIL: FIND-002-1; shared direct decision still omits subject. |
| Existing authorized User bearer plus loaded state native IO | FAIL: FIND-002-2; native composition still lacks optional-root resolution. |
| Immutable configure, Service-only roots and stock-provider request adapters | Original source/focused tests retain bounded acceptance; no new API required by this revision. |
| Exact selected binding through direct/submitted/recovered work | FAIL: FIND-009-1; selected occurrence preservation remains required by committed spec. |
| Accepted correlation context reaching direct/queued inputs/result rows | FAIL: FIND-009-2; superseding row-retention proof is required. |
| Changed-item Rust documentation | FAIL: FIND-002-3. |
| Existing single-statement tenant target reader | FIND-009-3 rejected; preserve the established isolated hot path. |
| Former state-bound external-exporter requirement | Removed by approved two-path contract; FIND-009-4 rejected, no implementation fix claimed. |
| AC-026 standard resource fallback and final three-signal readback | Assigned to existing TASK-003 proof; provider ownership/configure to TASK-009; docs to TASK-008. Not falsely marked complete here. |
| Excluded warehouse/schema/layout redesign, schedule fanout, new token descriptors or grant model, compatibility/backfill and global mutable Service context | Remain excluded. No rejected finding is prescribed as optional remediation. |

## Retained independently validated ledger

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

### FIND-009-1 — Binding occurrence preservation only

Superseding maintainer disposition: retain only committed spec REQ-012/015,
INV-004 and AC-005. They require exact owner, subject occurrence and Verifier
to survive direct/submitted work, retry and recovery. The Run's selected site
must not collapse to a first subject/Verifier match before acceptance.

Preserve that occurrence through the existing request, binding resolution and
accepted-work owners. Reuse existing containing Card and occurrence identity;
no new identity, resolver, registry or activation mechanism is required. Preserve
explicit-only bindings, authorized unbound work and existing schedule/group
semantics. Prove distinct selected occurrences through direct/submitted work
and restart, with false owner/subject/Verifier combinations refused.

### FIND-009-2 — Superseded by correlation context retention

FIND-009-2 as written is rejected by the maintainer: environment must not reach
execution. Replace its engine/gateway/evaluator/population handoff requirement
with correlation context retention:

Direct and queued verification capture one context
{service_name, service_version, deployment_environment} at acceptance. Queued
work stores it with the accepted execution; both paths write it unchanged to
the result and inputs rows. Nothing else reads it.

Prove direct and queued inputs/result row readback, including queued retry and
restart, with optional tags preserved unchanged. A status-only assertion does
not prove row retention. Preserve existing authority, leases, deadlines and
evidence semantics; use the existing accepted-work and row projection owners.
TASK-003 owns final canonical columns and full warehouse readback.

### FIND-002-3 — CONFIRMED / VIOLATION: changed Rust items miss the mandated rustdoc contract

Discovery: STD-002009-02. Obligation: root AGENTS Code and agent_docs/rust.md Rustdoc explicitly cover every changed/private/test item, require Errors for fallible contracts and Panics for possible panic paths, and make missing rustdoc merge-blocking. This one correction spans baseline TASK-002 and supplemental TASK-009; it must not be split into repeated style rounds.

Exact locations and individual confirmation are in the inventory below. The fallible-owner list has eleven function items (the source report groups the two exchange functions in one row). Each is changed in the cumulative range and lacks the stated section; `run_direct` returns a tuple containing Result and is still fallible. Trait implementation inheritance does not repair the missing changed-item contract: the changed trait method also lacks its Errors section.

Consequence: required private/test failure and invariant contracts are absent despite reported format/clippy success. This is the explicit rustdoc exception to the review's usual non-blocking style threshold.

Decision-complete correction: add accurate local rustdoc to the confirmed inventory, batching it with bounded implementation corrections and any additional Rust items they change. Document actual workflow/errors/panic invariants; preserve executable behavior, assertions and error handling. No boilerplate promises, suppressions, checker, fixture rewrite or runtime test solely for docs. Existing format/lint plus source review closes this finding. The two OTLP indexing helpers and their callers require panic docs even though they have no explicit panic macro; Signal::rejected calls the documented panic-capable HttpEncoding::decode.


## Remediation work and proof limits

Implement [TASK-002-R1-execution-context.md](TASK-002-R1-execution-context.md)
with `$wyrd-implement`. It is one self-contained combined task for the five
retained IDs, with diagnosis, existing-owner correction boundaries, explicit
preserved behavior/non-goals, focused scenario proofs and the confirmed rustdoc
inventory. Its public projection cases are exact affected tests, not full SDK
suites. No new exporter API or broad cleanup is authorized.

The task routes accepted execution context and actual population-query preparation
to their existing owners, while preserving TASK-003 ownership of canonical
warehouse columns, physical defaults, capture projection and final readback.
No scope widening or circular proof prerequisite is introduced. Preserve original
FIND IDs and audit the complete cumulative range after remediation; neither
TASK-002 nor TASK-009 has passed completion.

No runtime tests, services, generators or broad aggregates were rerun in this
authority-only continuation. After the tenant-exception correction,
`mise run check:tenant-isolation` and `git diff --check` passed. The five exact passing checks recorded in the
[original verification](../TASK-002-and-009-b1ff72dc3/verification.md) and original
implementation evidence retain their limits. Planned remediation selectors have
not run and must be confirmed before RED. The original missing-RED and independent
diagnostician evidence limits are not silently cured; the late claimed waiver
remains excluded as authorization.

Only specification, architecture/task instructions and review/remediation
Markdown were changed. No implementation, commit, merge, push or deployment.
