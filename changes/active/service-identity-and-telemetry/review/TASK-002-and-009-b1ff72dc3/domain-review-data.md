# Persistent authority and tenancy review

Result: **FAIL**

Subject: cumulative `036fc48fe26fb87648a277b10099bc475d293b92...b1ff72dc34c344b596c85cf408e6a9217685be35`, TASK-002 plus TASK-009. HEAD remained the candidate at final inspection. Source was not changed.

## Boundary and authority coverage

| Boundary | Authority | Source and result |
| --- | --- | --- |
| Stable account, restriction, unique family association | Spec REQ-001–008,016–017; auth supplement; security posture; SQL AGENTS | Both new migrations, service_accounts.rs, principal_directory.rs, principal creation/restriction routes, issuance.rs. Accounts no longer contain Card/version identity. Partial unique family index includes suspended accounts and frees deleted ones. Conflicting restriction replacement is atomic. No material defect established. |
| Federation persistence | Auth supplement workload bindings; tenant binding authority | workload_bindings.rs, row type, pg_resolvers.rs, issuer boundary. Stable principal has tenant-composite FK; insert/upsert selects active Service accounts under RLS. Default root is revalidated by issuance. No material defect established. |
| Activity and scheduled authority | REQ-014–017; security posture accepted work | verification.rs, verifier_executions.rs due/schedule/group paths, scheduler.rs, BackgroundAuthority::resolve, TenantTokenIssuer::{scheduled,authorize}. Activity is tenant/root/account scoped and PostgreSQL stamped after qualifying direct machine-root resolution. Arming shares the transaction. Activity reads join the current family association. Schedule authority resolves fresh grants and exact owner root; no latest-caller/version-principal writer found. |
| RLS and SQL capability | Root and SQL AGENTS; security posture PostgreSQL tenant binding | Auth mutations take TenantConn; new activity table forces RLS and uses composite tenant FKs. VerificationTargets retains its pre-existing bare-connection/self-binding-function pattern; changed owner reads use the same tenant-bound function. No new reachable cross-tenant exposure established. This does not waive the standards review of that materially changed capability surface. |
| Accepted effective authority | REQ-014–015,032; security posture frozen authority; Bifrost acknowledged handoff | AcceptedAuthority serializes effective principal, grants/Card scope, delegation and provenance. Submission carries it through Scribe acknowledgment to SubmissionSink; SQL stores and claims it; Caller::accepted consumes it without live grant refresh. Scheduled/group writers resolve fresh authority before acceptance and retain separate SYSTEM attribution. Existing leases, idempotency and tenant transactions remain. |
| Exact binding occurrence | REQ-012,015,052; INV-004; TASK-002 scenario 7 and TASK-009 scenario 3 | State selection → request → cache → direct/submitted acceptance and worker. FAIL DATA-01. |
| Environment execution | REQ-053; TASK-009 scenario 3 | Binding/request → effective_environment → SQL/status → claim/engine/gateway/server-read consumers. FAIL DATA-02. |
| Scribe/replay | Bifrost accepted handoff | Cumulative execution_lanes/ingress/replay changes, ResultPayloadBuilder handoff and SubmissionSink inspected. Most lane/replay edits substitute flat signed Card scope/test principals; accepted principal authority remains in Submission. Warehouse schema/projection/readback is TASK-003 and was not required here. |

## Material findings

### DATA-01 — Exact bound occurrence is discarded before acceptance

Classification: MISSING. Obligations: REQ-012,015,052, INV-004; TASK-009 requires carrying the resolved occurrence through request and accepted queue owners.

Locations: `crates/shared/wyrd-client/src/state.rs:410,887–939`; `crates/wyrd-spec/src/verification/execution.rs:40–64`; `crates/wyrd/wyrd-server/src/verification/cache.rs:168–203`; `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:105–133`; direct/submitted `ResultRun` in `components/verification/service.rs:739,800`.

Evidence: state::bound_verifier traverses component occurrences but returns only Verifier UID, subject UID and spec. Observe::target adds Run/group/environment. VerificationRequest has no occurrence/binding field. The server collects selectors from every binding of the same Verifier/subject, including all component occurrences and the Agent's own bindings, into one set. Manual submission SQL writes neither binding_id nor owner_card_uid; worker binding_id remains None. Direct/submitted ResultRun already has a binding_id slot but sets None.

Consequence: a Service containing the same Agent under two aliases with the same Verifier cannot select or preserve which occurrence was invoked. Different environment selectors are rejected as globally ambiguous even for one chosen occurrence; identical selectors collapse distinct bindings. This gap precedes warehouse projection. The distinct-Service/Agent test uses different Verifiers and subjects and cannot expose the collision.

Smallest testable correction: retain the selected occurrence in the existing state/Run-to-request path and validate it at the existing binding resolver against exact root, subject and Verifier. Reuse existing binding identity/natural occurrence keys (`OWNER_OCCURRENCE_KEY`, component aliases and projected binding IDs), rather than another graph resolver. Freeze that occurrence through direct/submitted acceptance and queue claim/retry, selecting only its environment. Preserve authorized unbound/rootless verification. Prove two aliases of one Agent with the same Verifier and different selectors retain distinct accepted bindings through restart; another root's binding must be refused before staging. No warehouse-schema expansion is needed to prove this boundary.

### DATA-02 — Environment is persisted for status but absent from executing context

Classification: INCORRECT. Obligations: REQ-053 and TASK-009 scenario 3 require frozen environment through worker/gateway origin and explicit server-read filtering before judging; scheduled binding scope must survive without implicit fanout.

Locations: `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:73–85,105–133,409–428,1324–1370`; `crates/wyrd/wyrd-server/src/components/verification/service.rs:513–549`; `verification/runner.rs:425–474`; `verification/hosted/blocks.rs:62–78,149–158`; `verification/drift.rs:120–165,778`.

Evidence: direct execution computes effective_environment but uses it only in the returned VerificationResult; Execution has no environment. Enqueue freezes request.deployment_environment into SQL and STATUS_SQL reads it back. CLAIM_SQL does not return the column, ClaimedExecution/ExecutionInput omit it, and runner builds its engine without it. Scheduled SQL and resolved binding targets do not freeze the binding selector either. Hosted gateway origin contains Run and subject only. ObservationWindow accepts no environment and renders subject/series/time predicates only. These sibling consumers never receive the accepted value.

Consequence: a restarted submitted check can report prod while its actual gateway-backed execution has no originating environment; an environment-selected scheduled window cannot constrain its server-read population before judging. No worker misconfiguration is needed: the value is unavailable. The new configured_service_context_survives_acceptance_and_restart proof uses EngineScript's early return at runner.rs:419, then checks SQL/status. It cannot prove evaluator/gateway propagation.

Smallest testable correction: carry one frozen optional environment through existing accepted execution/claim/input context, binding-scheduled acceptance, direct engine and gateway-origin/evidence-read owners. Retries consume the accepted snapshot, never worker configuration, renewed tokens, account identity or all matching bindings. Apply effective selectors to supported server-read populations before judging while preserving unscoped and whole-trace rules. A focused real-engine proof must observe originating environment at the gateway invocation boundary after restart and show a binding-selected server-read population excludes another environment before evaluation. This requires invocation/evaluator context only; canonical warehouse columns, physical defaults and row readback remain TASK-003.

## Verification and limits

Read task evidence and review-time verification.md. Its focused client configuration/cache and Python/TypeScript adapter checks pass. No Postgres, runtime or broad suites were independently rerun by this reviewer. Conclusions use cumulative diff and producer-to-consumer source tracing, not other reviewers' conclusions. Existing SQL/status proofs do not exercise the missing real consumers. Cryptographic profile validation and SDK language-runtime correctness belong to the other coherent domain scopes.

Skipped TASK-003 warehouse schema/projection/readback and broad test reruns. Risk: exact occurrence and environment acceptance currently appear successful without surviving the complete execution boundary.
