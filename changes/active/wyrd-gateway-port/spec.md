---
id: SPEC-wyrd-gateway-port
revision: 1
status: approved
---

# Port completed Gateway V1 to current Wyrd

## Intent and authority

Restore the completed Gateway V1 capability from
`change/skald-workflow-runtime` on the new Wyrd history. The workflow work is
owned by the separate `changes/active/skald-workflow-runtime` packet imported
from that runtime worktree. Current `AGENTS.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-security-posture.md`, and `architecture/bifrost-design.md`
govern every adaptation. The approved Gateway V1 Revision 21 specification in
the private archive is the behavioral source for features that do not conflict
with those current authorities. The old integration branch is adaptation
evidence, not a merge base or independent contract authority.

Source pins (read through a local `wyrd-private-archive` checkout or its Git
remote):

- Gateway implementation: `change/skald-workflow-runtime` at `d890708400675e71645744cd709935790b58464d`.
- Approved contract: `changes/active/wyrd-gateway-v1/spec.md` at that commit,
  Revision 21. Latest cumulative TASK-007-R5 review is PASS at `0292320da`.
- Workflow source: the separate runtime worktree and its active change packet
  own intended workflow behavior. That packet's draft spec and task statuses
  require workflow-specific reconciliation; this port does not approve them.
- Historical admin adaptation: `integration/skald-workflow-on-admin` at
  `0f1d385bab95d579df89268b80d1184df7c5eb3b`. Its review was not final
  approval, so every adaptation needs current-repo proof.
- Port starting point: new Wyrd `origin/main` at
  `f7c61336ae824029b55e44c870a96997010d35fd`. Later main changes must be
  incorporated before integration.

## Required behavior

- **REQ-001 — Complete gateway.** Preserve Gateway V1 Revision 21's public
  administration, credential, invocation, routing, provider, operation,
  accounting, capture, error, and lifecycle behavior. Public gateway contracts
  remain typed and consistent across HTTP, Rust, Python, TypeScript, CLI, MCP,
  schemas, and documentation. A missing source feature is a port gap, not an
  implicit deferral.
- **REQ-002 — Serving boundary.** `wyrd-server` remains the only Wyrd listener.
  One in-process gateway engine uses `skald-providers` for provider calls. All
  public ingress dialects converge on the same governed invocation path.
  Expose the internal invocation capability for the separate server-hosted
  workflow work without adding a recursive public HTTP call.
- **REQ-003 — Current identity.** Authenticate existing tenant principals with
  current Wyrd token verification. Derive tenant and caller solely from the
  verified principal. Preserve original credential and delegation attribution
  where current routes require it. Authorize each provider/model/operation and
  admin action through current tenant-scoped RBAC. Denials remain audited.
  Reject ambiguous credential carriers, upstream key exposure, and caller
  control over tenant or deployment authority.
- **REQ-004 — Secrets and persistence.** Preserve Environment,
  ExternalSecret, and write-only ManagedSecret behavior, tenant isolation,
  redacted reads, rotation, and restart durability. Use current SQL tenancy,
  storage, cryptography, and migration conventions. Never reuse historical
  migration identifiers or rewrite applied migrations.
- **REQ-005 — Governed execution.** Preserve bounded endpoint screening,
  admission, fallback, retry, stream termination, quota/budget accounting,
  batch idempotency, and usage/cost attribution. Mandatory accounting remains
  durable independently of optional analytical capture.
- **REQ-006 — Audit and capture.** Authorization decisions use the current
  canonical audit append and publisher. Optional gateway call/attempt capture
  uses Bifrost without delaying inference. Any server-owned capture writer has
  only tenant- and destination-scoped record-write authority.
  Caller attribution survives delegation and capture. Capture failures do not
  change a completed inference response.
- **REQ-007 — Verification.** Restore the credential-free gateway journey
  through real SDK/client, server, Postgres, mock upstreams, and Bifrost,
  including success, authorization denial, cross-tenant refusal, native
  ingress, managed secret rotation, streaming failure/drain, and capture
  backpressure. Prove public contract and codegen consistency. Live-provider
  smoke remains opt-in.

## Invariants and non-goals

- **INV-001:** No second gateway process, listener, virtual key, unverified
  tenant header, raw pool in domain code, or compatibility route.
- **INV-002:** No caller-selected provider credential; no plaintext secret in
  a Card, response, log, trace, audit row, Bifrost row, or generated artifact.
- **INV-003:** Gateway capture authority cannot write outside its tenant and
  exact capture destinations.
- **INV-004:** No historical merge or blanket cherry-pick that imports
  archive-era admin, auth, audit, migrations, or unrelated Oracle/surface work.
- **INV-005:** Workflow behavior is owned by the separate imported workflow
  packet. This gateway port exposes the public and in-process invocation seams
  it consumes without implementing workflow execution.

## Acceptance

- **AC-001:** A tenant administrator can submit a managed provider secret,
  configure a deployment, grant a narrow caller, and that caller can invoke
  through an unmodified provider SDK; upstream receives only the tenant's
  provider key. Redacted reads reveal no key.
- **AC-002:** Under-privileged, cross-tenant, ambiguous-credential, unsafe
  endpoint, quota/budget, capability, and malformed requests fail with the
  correct stable error and cannot reach upstream or another tenant's data.
- **AC-003:** Gateway V1's supported operations and native ingress retain
  contract and stream semantics, including bounded media, batch lifecycle,
  retry/fallback safety, and terminal error behavior.
- **AC-004:** The accounting ledger, audit log, and optional Bifrost capture
  contain attributable tenant-scoped evidence. Restart, rotation, replay,
  drain, and capture outage preserve the source contract's guarantees.
- **AC-005:** Rust, Python, TypeScript, HTTP, CLI, and MCP journeys and current
  repository checks pass; generated contracts match the served API.

## Capture boundary

Port the old gateway's reserved capture identity as a tenant- and
destination-scoped service capability adapted to current Wyrd token, Bifrost,
and audit rules. The implementation agent chooses the smallest current-repo
mechanism that satisfies this boundary. Any need for broader privilege or a
new public identity contract requires a spec revision before code changes.

The imported workflow packet remains draft and is reviewed on its own terms.

## Open material decisions

None for the gateway port. An adaptation that changes a public contract,
capture authority, tenant isolation, audit behavior, or applied migration
requires a new specification revision before implementation.

## Revision history

- **Revision 1 — approved (2026-09-24):** Ports completed Gateway V1 Revision
  21 onto the new Wyrd history under current repository identity, tenancy,
  audit, Bifrost, and migration authority. Approval was explicitly requested
  by the user after the draft was prepared.
