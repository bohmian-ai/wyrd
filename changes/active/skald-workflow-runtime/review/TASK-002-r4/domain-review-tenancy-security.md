# TASK-002-r4 independent tenancy/security review

**Result: PASS.** No material tenancy, authorization, credential, input-validation, injection, secret-exposure, or supply-chain finding remains in the cumulative candidate.

## Reviewed boundary and authority

Immutable subject:

- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate and reviewed HEAD: `2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Task inputs: `TASK-002-cleanup`, R2 and R3 remediation tasks, including commit `375d97e67`
- Repository authority: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/wyrd-security-posture.md`, and applicable architecture-constraints, patterns, errors, PyO3, Python, TypeScript, testing, spec-driven-development, and maintainer guidance

The review traced authored-file loading, exact registered graph reads, selector parsing, tenant/RBAC/audit boundaries, registration preflight and write-time fencing, principal projection, SDK credential use, dependency changes, and negative test coverage.

## Boundary and source coverage

| Boundary | Source evidence | Result |
|---|---|---|
| Lazy credential use and local-file isolation | `wyrd-client/src/workflow.rs:57-73` creates `Cards` only when external refs exist. Loader filesystem work uses the established sandbox and blocking pool; loading publishes no partial Workflow or durable state. | PASS |
| Sibling versus registered provenance | `cards/hydrate/workflow.rs:29-200` keeps sibling and registered bodies in separate stores and selects by `Ref` provenance. A local same-identity body cannot satisfy an external reference. | PASS |
| Exact authorized graph reads | `cards/hydrate/graph.rs:213-243,339-387` routes every root and transitive Card through `reads::get_response`; `cards/reads/get.rs:27-106,158-205` checks kind, UID, space, name, version, and optional UID assertions. Runtime traversal accepts only UID-bearing relationships and does not fetch artifact inventories. | PASS |
| Read authentication, authorization, audit, and tenant isolation | `cards/routes.rs:83-113,147-157` authorizes and audits each UID or exact-ref read. `cards/service.rs:96-143` derives `TenantConn` from the authenticated caller’s tenant; payload paths, names, UIDs, and selectors do not select tenancy. | PASS |
| Selector validation | Rust rejects wrong-kind and versionless Workflow selectors before IO (`workflow.rs:167-186`). Python accepts only UID-alone or complete named identity and emits `WorkflowInvalidCardRef` locally (`state/mod.rs:2599-2637`). Shared Python registry parsers now emit generic request validation (`state/mod.rs:3023-3088`). TypeScript uses a closed, unknown-field-denying selector DTO and typed identifiers (`native/src/workflow.rs:53-99`). | PASS |
| Registration authorization and canonical audit | `cards/routes.rs:390-449` authenticates before the handler, evaluates `card:write` and conditional `operators:invoke`, and records allowed and denied decisions through existing audit owners. | PASS |
| Registration tenant reads and exact preflight | `resolve.rs:118-190,430-514` uses borrowed `TenantConn`, preserves Ref/Sibling provenance, reads external bodies by resolved tenant UID, and performs pure validation without tools, providers, or secrets. | PASS |
| Atomic write, stale-UID fence, and audit | `service.rs:1040-1147` appends allowed decisions, rechecks exact preflight UIDs, reserves idempotency, persists nodes and relationships, and commits once. `relationships.rs:36-68` locks the exact identity plus expected UID and Active status with `FOR SHARE`; replacement cannot be silently rebound. | PASS |
| Principal and privilege scope | `service.rs:1208-1212` provisions identities only for Service and Agent Cards. Workflow hydration contains declarative runtime bodies, not credentials, grants, or tenant authority. SDK Workflow views reuse the requesting Cards client and never assume a referenced Agent’s identity. | PASS |
| Secrets, injection, network, and dependencies | No production secret values, credential administration, tenant-controlled URL fetch, shell/SQL string interpolation, unsafe deserialization, cryptographic change, or new third-party package entered the cumulative diff. Added dependencies are existing workspace Skald crates. Query values remain bound parameters. | PASS |
| Security-negative proof | `pg_workflow_registration.rs:708-870` covers wrong UID, wrong kind, non-exact version, foreign tenant, pending/deleted dependencies, and non-floating versions. Lines `884-1001` cover stale-preflight replacement and zero partial writes. Python journey lines `571-653` covers missing credentials, denied reads, deleted refs, exact pins, mixed/versionless selectors, wrong kind, and unauthorized loading; Rust and TypeScript journeys cover equivalent paths. | PASS |
| Human DRIFT direction | No new security scanner, allowlist, compatibility switch, credential option, tenant filter, audit path, custom retry/security mechanism, or task-specific gate was added or is recommended. The implementation uses established HTTP authorization, `TenantConn`/RLS, typed selectors, parameterized SQL, standard SDK environment configuration, and existing test harnesses. | PASS |

## Prior-finding closure

- The original sibling/external substitution defect remains closed: local and registered bodies retain distinct provenance through both client hydration and server validation.
- The preflight/write replacement race remains closed: the write transaction requires the UID validated during preflight and holds its lifecycle lock until commit.
- R3’s Python selector-error defect is closed by the existing generic `WyrdError::Validation` producer for shared registry selectors. Workflow selectors retain the Workflow-specific error required by commit `375d97e67`.
- Exact Agent/Prompt references and server-derived relationships are now asserted through the public Rust, Python/HTTP, and TypeScript paths.
- No Workflow principal, Workflow-rooted state, credential inheritance, provider dispatch during registration, or execution-secret resolution was introduced.

## Security Audit

### Critical

- None.

### High

- None.

### Medium

- None.

### Low / Defense In Depth

- None. No optional hardening is required for task acceptance.

### Positive Controls

- Tenant identity derives from verified caller state and reaches SQL only through `TenantConn` and RLS.
- Every graph Card read reuses the established authorized and audited Cards route.
- Exact response identity and UID-bearing transitive relationships prevent dependency floating or substitution.
- Ref/Sibling provenance remains explicit across local and server composition.
- Registration authorization audit, UID recheck, idempotency, Card writes, and relationship writes share one tenant transaction.
- Workflow registration cannot provision a principal or transfer referenced Agent credentials or permissions.
- Malformed SDK selectors are rejected through typed boundary validation before network IO.
- Negative journeys cover missing credentials, denied access, cross-tenant invisibility, wrong UID/kind/version, inactive dependencies, stale replacement, and absence of partial writes.
- No new third-party dependency, secret-bearing example, URL-fetch boundary, or nonstandard security mechanism was added.

## Evidence limits

This was a static audit of the complete immutable base-to-candidate diff and current candidate source. Per assignment, no builds or tests were run. Existing R3 verification records were treated as evidence claims; their concrete assertions and production paths were inspected. No runtime credential, database, or timing behavior was independently re-executed.

No material finding is proposed.
