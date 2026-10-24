# TASK-002 + TASK-009 behavior acceptance audit

Result: **FAIL**.

Subject: base `036fc48fe26fb87648a277b10099bc475d293b92` to candidate
`b1ff72dc34c344b596c85cf408e6a9217685be35`, branch
`task/service-identity-and-telemetry/TASK-002`. HEAD was still the candidate at
the end of source inspection. This is the cumulative task audit, including the
stable-account baseline, against the frozen current revision-15 specification,
both original tasks, and normative supplements. No previous task findings were
provided. Historical TASK-002 wording permitting Agent roots is superseded.

## Scope and navigation

Read the root and applicable SQL, Python, TypeScript, and docs AGENTS rules,
the task-review skill, spec-driven workflow, maintainer guide and topic map,
Rust/error/agent-surface guides, spec and task authority, auth supplement, and
the pertinent configured-context/population clauses. CodeGraph was attempted
before source navigation; its broad symbol/path matches were expanded through
targeted source and cumulative diffs. The starting map was independently
expanded along these callers and consumers:

| Capability | Producer → consumer inspected |
|---|---|
| Stable account administration | principal HTTP create/restriction routes → tenant SQL account/restriction writes and partial unique index → summary; client/CLI/MCP and language projections in the cumulative diff |
| Issuance and selection | API-key exchange/direct-key admission and JWT-bearer mapping → TenantTokenIssuer/ResolvedContext → existing bounded graph resolver → signed Card access; bearer HTTP/OTLP selection → `AuthenticatedPrincipal::select_root` → `select_request_root` |
| Delegation | `DelegateToken::execute` → direct Service actor/root validation → current actor resolution and permission/Card intersections → signed actor chain → AcceptedAuthority and background execution |
| Client context | state load/configure/client/pinned/Run → middleware credential/cache → HTTP, native Bifrost and exporters; producer scope fingerprints include configured environment |
| Provider clients | Python HTTPX/HTTPX2 auth flows and per-state ContextVar Run lookup; TypeScript state `httpAuth` and per-state AsyncLocalStorage lookup → existing bearer adapter → gateway ingress; corresponding unit/provider journeys |
| Verification | Observe bound-verifier lookup → Target/VerificationRequest → direct and enqueue admission → target cache/environment reconciliation → Submission/Scribe acknowledgement → tenant queue → ClaimedExecution/runner → HostedBlocks, trace reader, gateway caller and result staging |
| Schedules | qualifying direct grants → exact-root/account activity → family account/activity SQL → BackgroundAuthority → existing schedule/group queue acceptance and frozen authority |
| Native ingress | state-pinned client → native transport auth metadata → Gate `write_batch` → synchronous `gate::auth::authenticate` → record admission |

Existing owners generally were extended rather than replaced: account and role
transactions, graph traversal, token verifier, shared credential cache, state
lifecycle, HTTPX/fetch adapters, queue/outbox/lease recovery, and schedule
activity remain recognizable. No new dependency or parallel graph/transport
workflow was required. The defects below concern missing behavior at those
owners, not opportunities to replace them.

## Acceptance matrix

PASS means the inspected implementation and available evidence support the
bounded obligation; it is not a claim that this reviewer reran all reported
journeys. Split rows distinguish implemented parts from failures hidden by
broader acceptance labels. TASK-003 warehouse schemas/projection/readback,
TASK-006 evidence-read redesign and TASK-008 final docsite are excluded.

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK-002 scenario 1; REQ-001–006: explicit stable account, first credential, no registration principal/grant | `principals/routes.rs` creates account/grants/key in its tenant transaction; registration auth projection deleted; account migration removes Card identity columns | Reported `stable_account_restrictions_and_credentials`, three principal SDK stories and CLI/MCP journeys | PASS |
| Restriction administration: unrestricted default, User null, wildcard-only family association, unique family conflict and atomic replacement, no role/key/ID mutation | `create_service_principal`, `set_workload_restriction_for`, `set_workload_restriction`; migration `auth_service_accounts_one_family`; summary projection | Reported HTTP/SQL/SDK/CLI/MCP principal proof | PASS |
| Credential lifecycle and role sources remain independent of releases | Existing credential mint/list/revoke and role owners retained; account rows no longer version-bound; issue-key route/CLI removed | Reported principal/identity lanes; some IdP/UI-dependent regressions explicitly not run | PASS |
| TASK-002 scenario 2; REQ-007–009,033: exact signed Card states, bounded graph/token, removed principal kinds and no machine refresh | `ResolvedContext`, auth workload contracts and verifier; existing signing/verification bounds retained | Reported `explicit_card_access_claims_are_fail_closed`, issuer and delegation cases | PASS |
| TASK-009 scenario 1; Service-only roots/families across issuance and bearer selection | `WorkloadKind` only Service; `resolve_workload_scope`; DTO validation and shared request-root selection | Reported `pg_principal_roles::workload_roots_and_families_are_services_only` | PASS |
| TASK-002 scenario 3; REQ-010,012; AC-001/003/011: root-required and scoped callers cannot directly verify arbitrary subjects | `run_direct` calls `authorize_run` with no subject; no subsequent subject admission occurs | Existing positive and enqueue negatives do not exercise this direct bypass; B-001 | FAIL |
| Ordinary rootless/generic operation and truthful HTTP/OTLP context selection | HTTP middleware and OTLP adapter reuse `select_root`, bounded resolver and signed-root mismatch check; unrelated registry/query calls retain normal permission checks | Reported OTLP optional-context negatives and principal journeys | PASS |
| REQ-013; AC-001/002/011: a state using an existing authorized User bearer emits native observations with its automatic root | State pins root and native transport sends it, but native Gate rejects rootless signed bearer plus selector | No positive User-bearer state/native journey establishes it; B-002 | FAIL |
| TASK-002 scenario 4; REQ-013–014: offline loading, concurrent independent roots and renewal/cache separation | State supplies immutable derived middleware; rooted caches isolated in memory, rootless disk entry not reused; gateway cache key includes root | Review-time 2/2 Rust configure/cache tests; reported SDK release-isolation stories | PASS |
| TASK-009 scenario 2; REQ-050: local configure defaults, precedence, explicit invalid values, freeze/late/shutdown refusal and independent states | `state.rs::configure_in`, OnceLock environment, `pinned`, Run/shutdown freezing; Python/TS thin projections | Review-time Rust configure test passes; reported three SDK configured journeys | PASS |
| Automatic telemetry provider ownership and existing provider integration | Rust `INSTALLED` associates middleware/provider and refuses another context; Python/TS retain managed provider ownership/refusal paths; exporters use bound client headers | Reported telemetry/observe journeys; no independent provider-global test rerun | PASS |
| TASK-002 scenario 5; delegation intersects snapshot grants and Card authority, preserves actor roots/provenance, refuses unresolved subject | `delegation_chain`, `into_access_grant`, existing permission intersection, AcceptedAuthority stores principal and full chain without bearer secret | Reported delegation intersection, User-through-Service gateway and queue snapshot tests | PASS |
| TASK-002 scenario 6; federation explicit/default/rootless mapping and human/platform/System boundaries | binding resolves stable account and optional default; request UID wins; issuer resolves active account and restrictions; human/browser grant owners retained | Reported federation/browser/identity tests; externally dependent tests not independently rerun | PASS |
| TASK-002 scenario 7; immutable accepted account/grants/Card/chain/provenance through acknowledgement, restart/retry | `AcceptedAuthority::new`, Submission and Scribe handoff, queue authority JSON, `Caller::accepted`, runner dispatch | Reported `accepted_authority_survives_account_and_context_changes` | PASS |
| TASK-002 scenario 7 and TASK-009 scenario 3; REQ-015,052; AC-005/024: preserve exact binding occurrence for bound direct/submitted verification | BoundVerifier/request carry no occurrence, target cache unions binding selectors, direct/input staging sets `binding_id: None`, manual queue insert omits binding | New configured journeys compare subjects/environments, not exact occurrence; B-003 | FAIL |
| TASK-002 scenario 8; REQ-016–017: schedule account is explicit; activity keyed by exact root/account, no latest-caller/System fallback | activity INSERT joins restricted family/account and root; binding activity reads that pair; BackgroundAuthority resolves current associated authority and exact root | Reported `only_associated_account_activity_activates_exact_root`, issuer activity and runtime coverage | PASS |
| TASK-009 scenario 3; equal/sole/absent/conflicting invocation and binding environment at admission | `effective_environment`, direct and enqueue call it before staging; malformed/conflicting values have typed validation | Unit selector rules and reported HTTP/MCP conflict/no-execution proof | PASS |
| REQ-053: environment frozen for status/readback across restart | request canonicalization and manual queue environment column; execution status read includes persisted value | Reported configured restart test asserts persisted column and get-execution readback | PASS |
| REQ-052–053: effective environment reaches actual execution, server-read deployment population and verification-origin gateway calls | ClaimedExecution does not contain environment; HostedBlocks Execution/Caller/gateway handoff receive none; scheduled/grouped acceptance omits selector | Scripted restart outcome bypasses the engine; B-004 | FAIL |
| TASK-009 scenario 4; REQ-054–055: state provider hook snapshots context before refresh, per-state nested Run, replaces placeholder credentials, trace precedence and local conflicts | Python `_context` before threaded access token; TS state hook before awaited underlying fetch; both use immutable state environment/root and state-specific Run lookup | Review-time Python snapshot 1/1 and TS context 1/1; reported actual OpenAI/Anthropic journeys | PASS |
| Plain client adapters and raw credential-slot mode preserved, no provider wrapper/new public state client accessor | Existing client adapter remains; state accessor composes it/native client privately; provider-specific wrappers not added | Reported plain and state provider integration cases | PASS |
| REQ-032 and TASK-002 fresh baseline/public contract closure | Typed restriction and federation projections through Rust/Python/TS, CLI/MCP; owning auth/design prose amended; removed route and Agent kind have no compatibility shim | Reported formatting/lints/types/generated checks; generated-output checks not rerun here | PASS |
| INV-001–002: credential-only tenant/principal authority and role-derived permissions | Tenant-bound issuer/account queries and signed-root narrowing retained; direct verifier Card admission has B-001 | Reported tenant/permission regressions do not close direct bypass | FAIL |
| INV-003–004: account/root/subject/occurrence stay independent and exact during work/recovery | Account/root/subject separation implemented; bound occurrence discarded and execution environment discarded | B-003, B-004 | FAIL |
| INV-005: generic telemetry and privileged rootless use remain valid | Rootless generic operations preserved; automatic rooted state native submission of User bearer fails | B-002 | FAIL |
| INV-006–008: durable server decisions, nonblocking audit, unchanged typed signal production | Authority snapshot stays server-owned; audit staging remains nonblocking; no new signal schema/warehouse producer in this task | Reported outbox/OTLP/source checks; complete stored schema proof correctly deferred | PASS |
| Non-goals: no environment grant/token descriptor growth, global mutable configured context, schedule fanout/group identity change, historical conversion, compatibility alias or new storage engine | New environment is client/request/status context; existing owners and baseline reset approach retained | Cumulative source/diff inspection, no claim of TASK-003 completion | PASS |

## Findings

### B-001 — INCORRECT: direct verification skips the subject Card-access gate

**Obligation:** REQ-010/012, TASK-002 scenarios 2/3/7, AC-003/011. A restricted
rootless principal must receive `workload_context_required` for Card-sensitive
verification, and a scoped caller cannot attribute an out-of-graph subject.

**Location:** `crates/wyrd/wyrd-server/src/components/verification/service.rs:485`
(`run_direct`), with the changed Card-access decision at `:947`.

**Path and evidence:** HTTP direct execute and MCP execute reach the same
`run_direct`. It calls
`authorize_run(caller, &request.verifier_card_uid, None, ...)`. The helper's
RootRequired and Scoped refusal branches both require `Some(subject)`.
`resolve` checks tenant existence/readiness, not caller subject scope. The
engine then runs and `stage_direct` attributes its selected subject under the
caller's root. Enqueue, the sibling consumer, correctly passes
`Some(&request.card_uid)`. HTTP root selection only narrows the principal; it
does not inspect this payload. No downstream subject check repairs the direct
path. A rootless family account with a valid verifier grant, or a caller scoped
to R1 with wildcard verifier permission, can directly run a simple scalar/code
Verifier on an arbitrary active same-tenant R2 subject. This is a required new
Card-state admission gap, even though the old direct call used `None` too.

**Consequence:** missing-context or out-of-graph direct work executes and may
produce falsely owned evidence while the equivalent submitted call refuses.

**Smallest testable correction:** pass the requested subject through the
existing direct authorization decision, reusing `authorize_run` rather than
adding another gate. Preserve the single decision/audit boundary and ordinary
AnyRegistered rootless User behavior. Add direct HTTP/MCP coverage for
RootRequired refusal, scoped out-of-graph refusal and an authorized rootless
User positive case, with no engine/result staging after refusal.

### B-002 — INCORRECT: native state writes reject normal User bearer context

**Obligation:** REQ-010/013, AC-001/002/011, TASK-002 scenarios 3/4. A User access
token and WyrdState must emit with sufficient grants; loading a Service supplies
root context without requiring a machine account.

**Location:** `crates/vala/vala-bifrost-redux/src/gate/auth.rs:170` (rootless
selector refusal), reached from `gate/mod.rs:1152`.

**Path and evidence:** `WyrdState::client/pinned` derives a root-bound client
even for `ResolvedCredential::BearerToken`. `AuthMiddleware::bearer` returns
that existing token unchanged. `BifrostGrpcTransport::add_auth_metadata:292`
sends the state root alongside it. Native Gate sees a valid rootless User token
and a root selector, obtains `true` from `needs_root_resolution`, and always
returns WorkloadUidRefused with remediation to exchange a credential. A User
bearer has no machine credential to exchange. The sibling HTTP middleware and
OTLP gRPC adapter already resolve exactly this selection with
`AuthenticatedPrincipal::select_root`/the shared `select_request_root`, without
changing identity/grants or the synchronous signature verifier.

**Consequence:** a normal authorized User can load and verify through state
HTTP paths but cannot flush its state native observations. The documented
"by design" task-evidence limit contradicts approved acceptance.

**Smallest testable correction:** apply the existing server-owned optional-root
resolution before native admission too, preserving the store-free signed-token
verifier and reusing the shared graph/restriction owner. Do not drop the header,
broaden credentials, or require a Service identity. Prove a real User-bearer
state → native Bifrost write → acknowledgement plus out-of-graph, false-root,
foreign-root and signed-root-mismatch refusal. Preserve machine already-signed
root and rootless generic admission.

### B-003 — MISSING: bound invocation loses the exact binding occurrence

**Obligation:** REQ-012/015/052, INV-004, AC-005/024, TASK-002 scenario 7 and
TASK-009 approach 4/scenario 3 explicitly require the bound occurrence to survive
direct/submitted/gateway-backed execution and recovery. This is accepted-context
work in these tasks; it is not TASK-003 table projection.

**Location:** `crates/shared/wyrd-client/src/state.rs:878`
(`bound_verifier`, returning BoundVerifier at `:934`),
`crates/wyrd/wyrd-server/src/verification/cache.rs:169`, and
`components/verification/service.rs:739`/`:800`.

**Path and evidence:** local binding lookup identifies one binding but returns
only verifier UID, subject UID and spec. Observe Target/VerificationRequest
contain no binding or occurrence. The server cache matches all component and
Agent bindings by owner/subject/verifier and collapses them into a set of
environment values. It never validates one exact occurrence. Both input and
direct result staging explicitly set `binding_id: None`; manual queue inserts
omit owner/binding columns. By contrast, `project_bindings`' natural key is
owner + **subject occurrence** + verifier, and scheduled/grouped queue paths
retain the resulting BindingId. Two component aliases can therefore reference
the same subject and verifier under different occurrences. Equal environments
silently collapse them; different environments reject the invocation as a union
rather than select its intended occurrence. A shared Agent's own binding also
is not equivalent to the Service component occurrence containing that Agent.

**Consequence:** a Run selecting a bound check becomes generic unbound work;
the accepted record cannot establish which authored occurrence was executed,
including after restart. Existing subject/environment assertions do not prove
the required distinction. The task's residual-risk disclosure acknowledges the
missing value, not an approved exception.

**Smallest testable correction:** retain the occurrence selected by the existing
local binding resolution and validate/resolve it through the existing server
binding owner, then freeze it with accepted work. Reuse existing BindingId and
subject-occurrence machinery; do not infer an occurrence from set membership
or add a parallel binding resolver. Preserve intentional generic explicit
verification without a binding. Add one real journey with two same-subject,
same-verifier component occurrences (including differing selectors), assert
the selected exact occurrence for direct and submitted work and after restart,
and reject a mismatched root/subject/occurrence before inputs/queue residue.

### B-004 — MISSING: environment is persisted for readback but absent from execution

**Obligation:** REQ-052–053 and TASK-009 scenario 3 require originating configured
environment through actual accepted execution/gateway origin and effective
selectors on server-read deployment populations. TASK-003 may add columns and
projection later; it cannot recover context that this task discards.

**Location:** `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:1324`
(`ClaimedExecution`) and `crates/wyrd/wyrd-server/src/verification/runner.rs:454`
(accepted caller/Execution construction); direct counterpart
`components/verification/service.rs:539`.

**Path and evidence:** enqueue computes effective environment and stores it in
the manual queue column; `ExecutionStatus`/get-execution later read it. However,
the claim SQL/ClaimedExecution and ExecutionInput do not carry it. Runner builds
`Caller::accepted` and HostedBlocks Execution without environment; Caller and
HostedBlocks Execution have no such context. Direct code similarly computes
`deployment_environment` only for the returned result, never passes it into
the engine. HostedBlocks creates the gateway caller with just Caller and
GatewayCallSubject, and creates CallerTraces without environment. Scheduled and
grouped insertion paths do not freeze binding environment either. Neither the
drift population reader nor hosted engine contains deployment-environment
selection. Thus the value cannot affect execution or reach a verification
originated gateway call. Whole-trace evidence must retain its specified whole
trace semantics; the correction must not blindly filter trace spans.

**Consequence:** accepted status can claim `prod` after restart while the actual
worker judges an unscoped deployment population or dispatches gateway work with
no originating environment. Binding-selected scheduled work also loses the
selector. The configured restart proof scripts a completed outcome and asserts
Postgres/status values, so it cannot detect this.

**Smallest testable correction:** carry the effective invocation/binding scope
through the existing frozen execution/claim context into existing engine,
gateway-origin and deployment-population owners for direct, submitted,
scheduled and grouped paths. Preserve unscoped/mixed-population semantics,
whole-trace behavior, authority, deadlines and no schedule fanout. Test actual
execution across a restart with distinct prod/staging observations and a
gateway-backed check; prove the effective population before judging and inspect
the originating gateway context at its accepted seam. Do not count status-only
readback as this proof, and leave canonical row schema/readback to TASK-003.

## Verification and limits

Inspected the cumulative diff and source paths above and assessed both tasks'
appended evidence, including diagnosed OAuth projection/actor-root/explicit
fixture grant changes. Review-time verification from `verification.md` reports
2 Rust context/cache tests, 1 Python snapshot test and 1 TypeScript context test
passed. Those small checks support the corresponding local behavior; they do
not exercise these four failures. No source edits, generated-output checks,
Postgres journeys, provider mocks, broad suites or benchmark runs were performed
by this reviewer. IdP, UI, Vault and other recorded environment limits remain.

The four findings are source-local, reachable task acceptance failures. No
optional improvement, unrelated storage/read redesign or new public product
decision is requested. Discovery result remains FAIL pending independent
validation of B-001 through B-004.
