---
id: SPEC-sealing-keyring
revision: 1
status: approved
---

# Gateway secrets use the deployment sealing keyring

## Objective and user value

Gateway managed secrets are sealed with the deployment sealing keyring, and `wyrd server dev` provisions that key, so the quickstart's credential step works as written.

## Approval

Approved by the human owner on 2026-10-14 as part of SPEC-example-journey-fixes
r6. Split into its own change by owner direction on 2026-10-14 so each problem
is reviewed and verified on its own. The requirement text is carried over
unchanged.

## Required behavior

### REQ-007 Deployment sealing keyring for managed secrets

Revised in r2 per owner directive 2026-10-14: reuse the deployment sealing
keyring.

- Gateway `managed_secret` credentials are sealed and opened with the
  deployment `SealingKeyring` (`WYRD_SEALING_KEY_FILE` /
  `WYRD_SEALING_KEY_BASE64`, retained keys for rotation), exactly as the other
  stored sealed secrets: same envelope, same boot rewrap. Each sealed value is
  bound to its tenant, name, and provider, so it opens only in its own row.
- The separate gateway keyring (`gateway.managed_secret_keys` and its
  per-tenant key plumbing) is removed with no compatibility path.
- `wyrd server dev --tenant <slug>` gives a fresh deployment a sealing key
  before first boot: unless the caller supplies one, it creates an owner-only
  key in the installed server directory on first run and reuses it on later
  runs, so stored credentials keep decrypting. No restart is needed. The key
  and stored secrets never appear in logs or output.
- The docs quickstart's credential step works as written.

## Invariants

- INV-001 Tenant isolation holds on every changed path.
- INV-002 Provider credentials and keys are never logged or returned.

## Acceptance obligations

- AC-001 Each REQ has a user-journey test (Rust, Python, and TypeScript where
  the surface ships in all three; MCP/HTTP where agent-facing) that fails on
  the base and passes on the candidate, booting the server the way users do
  where the defect was harness-masked (REQ-003 uses the official TypeScript
  MCP SDK; REQ-007 drives the CLI).
- AC-003 Format, lints, codegen check, and the owning `mise` lanes pass.

## Open material decisions

None.
