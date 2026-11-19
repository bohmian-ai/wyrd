---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-stored-secret-encryption
spec_revision: 1
requirements: [REQ-002, REQ-006, REQ-007, REQ-008, REQ-009, REQ-012]
depends_on: [TASK-002]
---

# Deliver resource Rotate and Revoke

## Outcome and flow

An administrator selects an existing resource and renews encryption, supplies
a replacement credential, or revokes its use. Existing resource identity →
owner-authorized action → common facade and atomic owner update → existing
runtime refresh → unchanged client reference. The operator need not handle
DEKs or database rows. Actual provider-token replacement/revocation remains
an issuer responsibility unless an existing supported integration performs it.

Use the [approved spec](../spec.md) and
[administrator journey](../design/admin-journey.md), particularly selected-secret
rotation, resource revocation, and retained propagation/in-flight behavior.
Task 2 supplies common encrypted rows, injected facade, and existing owner
consuming paths. This task adds/extends their administration, not a new registry.

## Existing owners and scope

Extend `OperatorConnectionControl` update/disable, `GatewayAdministration`
credential replacement/revoke, auth `HumanConnections` candidate/test/activate,
issuer configuration owners, and platform identity administration. Existing
permissions and resource lifecycle semantics remain authoritative. Reuse current
actions where they already deliver replacement/revocation; add encryption renewal
through the same resource owners. Keep auth credential replacement distinct from
resealing unchanged plaintext. Do not make auth replacement bypass validation or
promise session preservation that its lifecycle does not provide.

Project the owner actions through typed `wyrd-spec` contracts/errors, server
HTTP/OpenAPI, `wyrd-client`, CLI, Rust/Python/TypeScript SDKs and scoped MCP
writes. Carry each changed projection's exports, Python authored stubs, native
bindings and generated TypeScript declarations with it. No UI or public raw
encrypt/decrypt endpoint. No wrapping-key administration in tenant actions.

Use the common facade outside DB locks, commit against current authority/secret
state, and trigger existing owner refresh. Preserve Gateway propagation and
admitted-call snapshots, Operator next-attempt checks, and staged nonblocking
audit. Document the actual action/mode and effects at its public owner surface.

## Ordered acceptance scenarios and evidence

Add actual assertion locations, exact focused commands, RED/GREEN, and limits
in these entries; test variant owners/surfaces under the same scenarios.

1. **Renew a selected secret's encryption** — REQ-002, REQ-006, REQ-008–009.
   Given a working resource created through Task 2's public path, an authorized
   admin chooses renewal. Assert unchanged plaintext and identity, fresh DEK/
   payload, redacted response, and successful actual downstream use without
   client reconfiguration. Auth renewal preserves semantic revision and session
   behavior. Begin with the smallest owner action in real-server/Postgres
   integration; RED is missing renewal through that owner and facade.
2. **Replace a leaked/expired credential** — REQ-002, REQ-006, REQ-008–009.
   Given a resource whose old credential expired or whose old envelope cannot
   decrypt, supply a valid replacement through its typed owner operation.
   Actual downstream use receives the replacement under the same identity with
   a fresh DEK. Auth replacement follows candidate/test/activate and its existing
   session effects. Provider failure/invalid input leaves committed state intact.
   Server/SDK journeys plus OIDC/provider fixtures assert the consuming effect;
   RED is absent action/mode or replacement incorrectly depending on old decrypt.
3. **Revoke without resurrection** — REQ-007–009.
   Given an in-use resource, revoke it and exercise subsequent consuming use
   after existing owner refresh, including a second replica and restart. It is
   refused; a different resource still works. A concurrent renewal/replacement
   cannot undo revocation; an explicit existing authorized re-enable remains
   separate where supported. Deterministic server/Postgres integration observes
   races and admitted-work boundaries without asserting instantaneous global
   cancellation. Existing correct disable/revoke behavior needs regression proof;
   RED for new behavior is a missing projection or unsafe encryption action.
4. **Preserve permissions, commit safety, and redaction** — REQ-008–009.
   Attempt the actions as a wrong tenant, unprivileged principal, and MCP caller
   lacking write scope. Inject provider refusal/timeout and a competing resource
   update between read and commit. Observe permission refusal or conflict, no
   partial/losing write, and no keys/plaintext in responses, logs, errors or audit.
   Exercise actual owner services and existing snapshot invalidation; do not
   replace permission or persistence behavior with facade mocks. RED is a newly
   missing boundary/atomicity case; reuse existing proven cases.
5. **Operate through every public projection** — REQ-006–008, REQ-012.
   Drive the previous operations through public Rust, Python, TypeScript, CLI,
   HTTP and scoped MCP callers against the real server/Postgres. Verify typed
   requests/modes, stable resource references, errors, and actual downstream
   use/refusal. Document upstream credential revocation separately from Wyrd
   revoke and encryption renewal. RED is missing consumer exposure or divergent
   projection semantics; binding-only construction tests cannot replace journeys.

## Verification and handoff

Reuse `wyrd-server` integration and `wyrd-auth` PG tests for owner state/races;
Rust SDK integration, Python integration, TypeScript integration, CLI and MCP
journey targets for projections. Existing module/file owners include Operator
connections, Gateway administration, identity, and platform journeys. Discover
the actual new test selectors and record focused setup-aware commands before
RED. Available environment-owning runners are `mise run test:operators:integration`,
`mise run test:wyrdstate:journey`, `mise run test:gateway:journey`,
`mise run test:identity:journey`, and `mise run test:platform:journey`.
Full language gates (`verify:rust-sdk`, `verify:python-sdk`,
`verify:typescript-sdk`) and combined journey runs belong to integrated review;
use their actual package/feature/setup recipes for focused task execution.

Run affected owner/projection tests, `mise run codegen:check`,
`mise run check:deps`, `mise run check:tenant-isolation`, Rust format/lints,
Python format/lints/typecheck, TypeScript format/lints/typecheck and native
declaration checks for the changed surfaces, and `mise run docs:check`.
If runtime hot-path code changes, enumerate external waits and run the applicable
clean full L1 benchmark/comparison as required by the spec and benchmark rules.

Output: tested, documented resource actions on every required surface. Task 4
is independent after Task 2; selected-secret actions do not depend on a new
deployment maintenance framework or provider key-admin privileges.
