# TASK-001-008-R8 cumulative re-review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Review base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Cumulative candidate: `84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6`
- Code candidate: `20e5becad783d48121010b673da75e211d4dc497`
- Approved specification:
  `changes/active/admin-principals/spec.md`, revision 13, SHA-256
  `57f91317e68b06e7b4d34ea94b964e4a1dd99678275a2ee67d1d51f9b4b46332`
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`
- Prior cumulative review and remediation:
  `changes/active/admin-principals/review/whole-branch-08/`

The candidate remained unchanged through both review waves.

## Verdict

**FIX_REQUIRED**

The revision-13 tenant-auth architecture, delegation attenuation, R8 SQL,
OpenAPI, Bifrost, rustdoc, and evidence corrections pass their reviewed
boundaries. Three independently validated defects remain.

Remediation task:
`changes/active/admin-principals/review/whole-branch-09/TASK-001-008-R9-close-validated-findings.md`.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| `REQ-001`–`REQ-011`: durable principals, generic credentials, secrecy, rotation | Principal and credential stores remain separate; issuance and rotation evidence is recorded | PASS |
| `REQ-012`–`REQ-012b`, `AC-018`: five grants share one five-minute issuer; requests verify JWTs locally | `TenantTokenIssuer` and concrete DB-free `TokenVerifier`; auth/identity journeys | PASS |
| `REQ-012c`, `INV-013a`, `AC-020`: delegation attenuates authority and audits each decision once | Permission intersection and allow/deny/no-effect paths pass | **FAIL** — successful delegation loses the authenticating caller credential from its audit row (`R9-1`) |
| `REQ-013`–`REQ-019`: closed auth planes and typed permission checks | Tenant and platform contexts remain distinct; platform revalidates current state | PASS |
| `REQ-020`–`REQ-028`: initialization, provisioning, lifecycle | Platform journeys, including consecutive runs | PASS |
| `REQ-029`–`REQ-035`: tenant administration, credentials, recovery, OIDC | Principal, SQL, identity, recovery, and rotation evidence | PASS |
| `REQ-036`, `REQ-047`–`REQ-049`, `AC-013`, `AC-014`, `AC-019`: shared clients, CLI/MCP/OpenAPI contracts | Shared transport, served OpenAPI and MCP evidence | **FAIL** — credential IDs lose their UUID type across new DTO/client/CLI seams (`R9-2`) |
| `REQ-037`, `AC-009`: every authorization decision names principal, credential, permission, resource, tenant, outcome | Refused and no-effect delegation rows retain attribution | **FAIL** — successful delegation row has `credential_id = NULL` (`R9-1`) |
| `REQ-038`–`REQ-046`: removed bootstrap model and platform administration | Final schema, routes, grants, OIDC and platform journeys | PASS |
| `INV-002`: raw credentials never reach recoverable or diagnostic surfaces | New platform/principal credential commands were corrected | **FAIL** — other shipped commands still expose secret options and debug-printable strings (`R8-2`) |
| Remaining invariants, constraints, and non-goals | Plane separation, RLS, five-minute snapshots, NOWAIT retention, no compatibility migration, no replacement auth abstraction | PASS |
| `FIND-admin-principals-R6-1`, `R7-5`, `R8-3`, `R8-5`, `R8-6`, `R8-7` | Final source and exact evidence table | PASS |
| Stable `FIND-admin-principals-R8-2` | Two new endpoint families reject secret arguments | **FAIL** — correction did not cover the complete shipped CLI command tree |
| Verification obligations | Required lanes, 16 exact selectors, strict rustdoc, codegen/docs, and whitespace are recorded passing | PASS for covered behavior; the three gaps lack closure proof |

## Wave results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | FAIL | `TREV-WB09-1`, `TREV-WB09-2` |
| Repository standards | FAIL | `STD-R9-1`, `STD-R9-2` |
| Auth/security domain | FAIL | `AUTH-SEC-01`, `AUTH-SEC-02` |
| Data/audit domain | FAIL | `DATA-R9-01` |
| Contract/CLI domain | FAIL | `CONTRACT-CLI-1`, `CONTRACT-CLI-2` |
| Bifrost domain | FAIL | `BIFROST-R9-1` |
| Ponytail validation | COMPLETE | Three retained roots |

## Validated finding ledger

| Finding | Status and class | Consequence | Minimum correction and proof |
|---|---|---|---|
| `FIND-admin-principals-R8-2` | REVISED · VIOLATION | Bearer, refresh, and OIDC secrets can enter argv, shell history, process listings, and debug output | Delete every shipped secret-valued Clap option; reuse ambient `ClientConfig`, refresh environment input, and issuer environment/file input; delete the unused explicit-token builder; prove parser rejection, redaction, ambient success and missing-input failure |
| `FIND-admin-principals-R9-1` | REVISED · INCORRECT | A successful delegation audit cannot identify which caller credential authorized it | Carry caller credential ID as audit-only context and attach it to the one successful event; keep delegated JWT `cid` absent; prove exactly one attributed row and an unattributed delegated JWT |
| `FIND-admin-principals-R9-2` | CONFIRMED · VIOLATION | New issue/list schemas and shared clients accept arbitrary credential strings although the server/MCP contract is UUID | Use installed `Uuid` end-to-end and parse CLI text once at the edge; prove schema/OpenAPI/MCP alignment and that malformed IDs cannot reach request construction |

Full caller traces, locations, evidence, rejected alternatives, and closure
boundaries are preserved in `findings-validation.md`.

## Prior-finding closure

- `FIND-admin-principals-R8-3`, `R8-5`, `R8-6`, and `R8-7` are closed.
- Stable `FIND-admin-principals-R8-2` remains open in revised form because its
  original two-command correction left sibling shipped CLI commands exposed.
- Human-rejected `R8-1` remains rejected: retain `67b4d0ba` and its replay
  proof.
- Human-rejected `R8-4` remains rejected: Wyrd is unshipped and gains no legacy
  audit compatibility path.
- Revision-13 delegation attenuation and decision-count/fail-closed behavior
  pass; only successful audit credential attribution remains open.

## Verification limits

This was a read-only static review. Reviewers inspected the recorded R8 lane
and exact-selector evidence but did not rerun the long Docker/Postgres suites.
The existing tests never assert successful delegation credential attribution,
the CLI parser proof covers only two endpoint families, and the UUID mismatch
is visible in source/schema types. `git diff --check` is clean, and reviewed
HEAD remained `84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6`.
