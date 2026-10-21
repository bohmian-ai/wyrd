---
id: TASK-007
kind: implementation
status: superseded
spec: SPEC-developer-docsite-rebuild
spec_revision: 2
requirements: [REQ-004, REQ-005, REQ-006, REQ-007, REQ-010, REQ-013, REQ-014, REQ-016, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-003, AC-004, AC-005, AC-006]
depends_on: [TASK-001]
parent_task:
remediates: []
---

## Supersession — 2026-10-20

Remaining documentation work is owned by
[SPEC-docsite-incremental-reset revision 2](../../docsite-incremental-reset/spec.md)
and its four tasks. Preserve this task's historical implementation evidence;
this status does not assert completion or revoke the retained brand, platform,
navigation, or accessibility commitments.

## Outcome and Value

An operator can deploy the supported self-hosted Wyrd shape, confirm
readiness, connect a team, and diagnose common deployment failures. The
procedure links to concise system architecture and exact configuration and
environment-variable reference.

## Owners, Scope, Consumers, and Prohibited Changes

Own Operate deployment/troubleshooting content and related Understand and
Reference pages. Describe only shipped server/container artifacts and actual
Postgres, object storage, Bifrost, networking, and readiness requirements.
Keep the existing single logical serving surface and approved brand. Do not
write new deployment code, imply an unpublished image/chart exists, document
hosted SaaS operation, or make engine internals into separate client APIs.
Use `$human-tech-docs` for every reader-facing guide, explanation, and
reference page, including its Diátaxis, humanizer, and voice passes.

## Approach

1. Confirm the supported self-hosted deployment shape and the actual
   configuration, persistence, secrets, and health boundaries.
2. Author the shortest team deployment and readiness procedure with a
   visible check and cleanup or recovery where needed.
3. Explain only the architecture an operator needs and publish source-backed
   configuration, environment, and documented-limit lookup.
4. Cover material startup, storage, and connection failures; integrate
   published pages into Operate, Understand, Reference, and search.

## Proof Strategy

These are documentation and configuration-reference changes. Execute the
documented deployment using the stated repository-managed prerequisites or
published artifacts, check health/readiness and a client connection, and
reproduce at least one material misconfiguration. Compare options with
current server/container sources and deployment authority. Run
`mise run docs:check` and inspect links and responsive tables. No
manufactured RED applies.

## Acceptance Criteria

- A reader can deploy the supported self-hosted shape and verify a healthy,
  connected service without guessing missing resources or credentials.
- System architecture explains server/Bifrost roles and one user-facing
  endpoint only to the depth needed for operation.
- Configuration and environment-variable reference names actual effective
  settings and trust boundaries; troubleshooting produces an observable
  diagnosis and recovery action.
- Unpublished images, charts, and SaaS paths are not presented as runnable.

## Expected Write Set and Consumer Closure

New Operate, Understand, and Reference content under `docs/src/content/docs/`
and generated published-page indexes. No runtime configuration or deployment
artifact implementation change is in scope.

## Verification and Evidence

Deployment and failure transcript, source/config comparison,
`mise run docs:check`, and built-site navigation/search/link inspection.

## Material Stop Conditions

Stop for a spec revision if the promised self-hosted outcome requires an
unpublished artifact or new product/deployment capability.

## Authority Links

- [Approved specification](../spec.md)
- [Repository instructions](../../../../AGENTS.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Bifrost design](../../../../architecture/bifrost-design.md)
- [Architecture constraints](../../../../architecture/wyrd-design.md)
