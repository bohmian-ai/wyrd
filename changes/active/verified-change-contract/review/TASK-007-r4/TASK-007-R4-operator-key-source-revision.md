---
id: TASK-007-R4
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 36
requirements: [REQ-147, REQ-150, AC-031]
depends_on: []
parent_task: TASK-007
remediates: [FIND-TASK-007-15, FIND-TASK-007-16, FIND-TASK-007-17]
---

# TASK-007-R4 — Record the Approved Operator Key Source and Close Remaining Findings

## Authority and immutable subject

- Spec: `changes/active/verified-change-contract/spec.md`, revision 35 → 36 (this task)
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Review ledger: `changes/active/verified-change-contract/review/TASK-007-r4/findings-validation.md`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Reviewed candidate: `e2da2769`

## Human decision (approved by the user on 2026-09-24)

The R4 review returned `SPEC_REVISION_REQUIRED` for `FIND-TASK-007-16`: revision
35 and TASK-007 assumed an existing backend-agnostic `SecretRef::Vault` resolver
(`wyrd_auth::resolve`) supplies tenant KEKs, but no such resolver exists. The
user chose to approve the implemented key source rather than build a shared
resolver:

- Tenant key-encryption keys are read by the server-owned `OperatorKeys` owner
  from exactly one configured source: an environment variable, owner-only
  mounted files, or HashiCorp Vault KV v2 over the already-installed `reqwest`.
- Multi-tenant production requires the HashiCorp Vault source over HTTPS and
  fails start unless every active tenant key is readable before readiness.
- Without a readable key, only connection create/update (sealing) refuses;
  other surfaces keep working and delivery retries `credential_store_unavailable`.
- A shared `SecretRef` resolver and AWS Secrets Manager / Google Secret
  Manager KEK sources are deferred; `SecretRef` is not the KEK configuration
  contract for this change.

## Required corrections

### `FIND-TASK-007-16` — spec revision 36

Edit `changes/active/verified-change-contract/spec.md` only to record the
decision above:

1. Bump front matter `revision: 35` → `revision: 36` (status stays `approved`).
2. Amend REQ-147's KEK sentences so the "deployment secret/KMS boundary" is
   the three sources above with the multi-tenant production Vault/HTTPS and
   fail-start rules; remove any requirement to reuse a `SecretRef`/external
   secret resolver for KEKs, and state the deferrals.
3. Fix any other spec sentence that names the `SecretRef::Vault` resolver as
   the KEK source (grep `SecretRef`, `resolver`, `KMS`).
4. Add a `Revision 36 Operator key source (2026-09-24)` entry to the revision
   history in the existing style, ending "This revision was explicitly
   approved by the user on 2026-09-24."
5. Update "Open material decisions" to name revision 36 as approved on
   2026-09-24.
6. Update the original TASK-007 file's `spec_revision` and the lines that
   require reusing the `SecretRef::Vault` resolver (around lines 23, 45, 68,
   100, 102, 198) so the task matches revision 36. Keep
   `architecture/wyrd-design.md` consistent with the same wording.

Do not change product behavior; the implementation already matches this
decision. Do not add a resolver, trait, or new provider.

### `FIND-TASK-007-15` — module-scope imports

Import the already-used declaration types at module scope and use bare names
at every candidate-added declaration/type position enumerated in
`review/TASK-007-r4/findings-validation.md` (`operator.rs`, `keys.rs`,
`operators.rs`, the private provider modules, `operator_dispatches.rs`,
`wyrd-testing::server`). Expression and macro paths are excluded. Sweep the
full `f8811ac5..HEAD` diff for any remaining candidate-added fully qualified
declaration types so this does not reopen.

### `FIND-TASK-007-17` — MCP list descriptor

Remove the false "secret version" promise from the Operator connection list
MCP descriptor (`mcp/operators.rs` `descriptors_unscoped`) and pin the
descriptor text with the existing MCP catalog proof.

## Verification

- `mise run fmt`, `mise run lints`, `git diff --check`
- `mise run codegen:check` (MCP descriptor)
- `mise run test:bifrost:journey:mcp` and the focused MCP catalog test by exact
  nextest expression
- `mise run test:wyrd`
- Final `git status` clean

## Evidence

Append `| Acceptance criterion | Implementation evidence | Verification evidence | Result |`
rows for each finding here.
