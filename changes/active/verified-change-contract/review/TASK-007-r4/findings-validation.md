# TASK-007 R4 Wave 2 Finding Validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `e2da27694e8a3d057f2ff863c5d17429adc0a495`
- Approved specification: `changes/active/verified-change-contract/spec.md`,
  revision 35
- Inputs: the original TASK-007, R1/R2/R3 remediation tasks and review
  ledgers, the complete cumulative diff, and all four R4 Wave 1 reports

The candidate remained the stated commit at the beginning and end of this
validation. The checkout has no `.codegraph/` directory, so source and caller
tracing used the immutable candidate and `rg`.

## Proposed-finding validation

| Wave 1 proposal | Result | Independent validation |
|---|---|---|
| `TREV-R4-001` | **CONFIRMED** | `REQ-147` and TASK-007 require the existing backend-agnostic `SecretRef::Vault` resolver and make a different key-provider contract a stop condition. The base and candidate contain the `SecretRef` contract but no `wyrd_auth::resolve` or external-secret resolver implementation. The candidate instead adds `VaultKeysConfig` and a direct HashiCorp KV v2 `reqwest` client, then changes `wyrd-design.md` and appends task evidence to describe that substitution. Those later artifacts cannot amend approved revision 35. The path is live through boot attachment/readiness, connection create/update sealing, delivery open, and rewrap. Closing it requires a human-approved resolver/ownership decision or an approved Vault-only specification revision, not another local patch. Retained as new `FIND-TASK-007-16`; outcome is `SPEC_REVISION_REQUIRED`. |
| `STD-001` | **REVISED** | The cited declaration sites are candidate additions and violate the explicit top-level-import/bare-name rule. This is the same obligation as prior `FIND-TASK-007-15`, not a new defect class. R3 corrected only its enumerated sites; it did not close the remaining candidate declarations in `operator.rs`, `keys.rs`, `operators.rs`, the private provider modules, `operator_dispatches.rs`, and `wyrd-testing::server`. Reopen and expand stable `FIND-TASK-007-15`. Expression and macro paths are excluded; only declaration/type positions need mechanical import reuse. |
| `STD-002` | **CONFIRMED** | `operators::descriptors_unscoped` is reached by every MCP catalog through `WyrdMcpHandler::catalog`. It promises a `secret version`, while its output schema and execution both use `ListAnswer { connections: Vec<OperatorConnectionView> }`; that view has no secret-version field by design. Deleting the two words preserves the approved redaction contract and is the minimum correction. Retained as new `FIND-TASK-007-17`. |

The security report's PASS establishes that the concrete HashiCorp path is
secure as implemented; it does not override the approved resolver ownership
contract. The delivery report proposed no findings and does not contradict the
retained ledger.

## Final deduplicated finding ledger

### FIND-TASK-007-15 — REVISED — VIOLATION: candidate declarations still bypass module import manifests

- **Wave 1 source:** `STD-001`
- **Prior identity:** R3 corrected the previously enumerated `std::fmt`,
  `NonZeroU32`, and `Path` sites. The same cumulative candidate still contains
  other candidate-added qualified declaration types, so the stable rule
  finding is reopened rather than duplicated.
- **Violated obligation:** `architecture/agent-rules.md` requires types in
  fields, parameters, return types, bounds, and `where` clauses to be imported
  at module top and used by bare name.
- **Exact locations:**
  - `crates/wyrd/wyrd-server/src/components/operators/keys.rs:235,550`;
  - `crates/wyrd/wyrd-server/src/verification/operators.rs:608,628,712,732,762,777-778,903-905`;
  - `crates/wyrd/wyrd-server/src/verification/operators/slack.rs:29` and
    `pager_duty.rs:28`;
  - `crates/wyrd-spec/src/card/operator.rs:251,531,550,555`;
  - `crates/wyrd/wyrd-sql/src/queries/operator_dispatches.rs:374`;
  - candidate-added Operator-key declarations in
    `crates/wyrd/wyrd-testing/src/server.rs:273,576,664-668,3479,3808`.
- **Evidence and reachability:** full owning bodies were inspected. These are
  live `OperatorKeys`, provider-delivery, template-validation, SQL claim decode,
  and test-server construction declarations. Their callers need no behavioral
  change: the defect is only the declaration shape. Qualified expression paths
  such as `serde_json::json!`, enum variants, builders, and associated-function
  calls are not part of the correction.
- **Observable consequence:** the cumulative candidate still violates the
  repository's mandatory Rust dependency-manifest style after R3 claimed this
  obligation closed.
- **Decision-complete correction:** extend each existing module import block
  with the already-used types (`Client`, `Value`, `BTreeMap`, `IdError`,
  `OperatorKeysConfig`, `TempDir`, and `Path`, with an alias only for a real
  collision), then use the bare names at every location above. Add no helper,
  suppression, dependency, or general source rewrite.
- **Focused closure proof:** inspect the enumerated declarations, then run
  `mise run fmt` and `mise run lints`. No runtime test is warranted because no
  behavior changes.

### FIND-TASK-007-16 — CONFIRMED — VIOLATION: the candidate substitutes a concrete HashiCorp client for the approved external-secret resolver

- **Wave 1 source:** `TREV-R4-001`
- **Violated obligation:** `REQ-147` fixes the deployment key owner as the
  existing external-secret resolver through `SecretRef::Vault`, with a
  configured Vault, AWS Secrets Manager, or Google Secret Manager backend.
  TASK-007 repeats that owner and requires stopping for a different key-provider
  contract.
- **Exact location:** approved contract at
  `changes/active/verified-change-contract/spec.md:1046-1054` and
  `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md:21-24,98-105,233-239`;
  substituted config/client at
  `crates/wyrd/wyrd-server/src/config.rs:1776-2017` and
  `crates/wyrd/wyrd-server/src/components/operators/keys.rs:218-421`;
  conflicting later descriptions at `architecture/wyrd-design.md:1383-1394`
  and the task's appended implementation evidence at lines 249-255.
- **Caller and reachability evidence:** `boot::attach_config_fields` constructs
  `OperatorKeys`; production readiness calls `verify_active`; connection
  create/update call `seal`; delivery calls `open` on every credential attempt;
  and rotation calls `rewrap_pass`. Each reaches `OperatorKeys::key`, whose
  Vault branch calls the private `vault_key` and directly builds the KV v2 URL
  and `X-Vault-Token` request. Neither configuration nor execution constructs
  `SecretRef::Vault`. A base/candidate source search found only the documented
  `SecretRef` type and no shared resolver implementation.
- **Observable consequence:** the candidate silently changes a material
  security/architecture boundary, locks multi-tenant production to HashiCorp
  KV v2, and duplicates deployment-secret configuration instead of using the
  approved backend-agnostic owner.
- **Required decision:** do not prescribe implementation remediation under the
  current spec. Human approval must either (a) define and approve the missing
  shared external-secret resolver ownership/configuration contract and retain
  the `SecretRef::Vault` requirement, or (b) revise `REQ-147`, TASK-007, and the
  owning architecture authority to approve the concrete HashiCorp-only
  boundary and AWS/GCP deferral. This is a product/security/architecture and
  cross-service ownership decision, so the review result is
  `SPEC_REVISION_REQUIRED`.
- **Closure proof after approval:** prove the selected boundary reads the exact
  `<prefix>/<tenant>/<version>` 32-byte key, refuses missing/malformed active
  keys before multi-tenant production readiness, and exposes no second
  unapproved selector/token configuration path. Re-run the existing startup,
  sealing/opening, rotation, and redaction coverage.

### FIND-TASK-007-17 — CONFIRMED — INCORRECT: MCP list description promises an absent secret-version field

- **Wave 1 source:** `STD-002`
- **Violated obligation:** `AGENTS.md` §§2 and 9 and
  `architecture/references/languages/agent-harness.md` require MCP descriptions
  and typed schemas to describe the same deterministic contract.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/mcp/operators.rs:75-92`; typed response at
  `:66-71` and
  `crates/wyrd-spec/src/operator_connection.rs:493-510`.
- **Caller and reachability evidence:** `WyrdMcpHandler::catalog` always extends
  the authenticated tool catalog with `operators::descriptors_unscoped`; direct
  calls route `operator_connections.list` through
  `mcp_operator_connections`, which serializes the same typed redacted views.
  The generated schema and runtime response both omit secret version.
- **Observable consequence:** an agent can plan around a field the tool can
  never return, contradicting the machine-readable output schema.
- **Decision-complete correction:** delete `and secret version` from the list
  descriptor. Do not expose the internal key/secret version or change the
  response type.
- **Focused closure proof:** extend the existing MCP discovery/catalog
  assertion to reject `secret version` in this descriptor and keep the focused
  MCP journey green.

## Prior-finding closure

| Prior finding | R4 validation |
|---|---|
| `FIND-TASK-007-1`–`FIND-TASK-007-14` | **CLOSED**. The cumulative source retains the independently reviewed R1-R3 corrections. |
| `FIND-TASK-007-15` | **OPEN / REVISED**. R3 fixed its enumerated sites, but the same mandatory import rule remains violated at the additional candidate-added declarations above. |

## Recommendation

The validated ledger contains `FIND-TASK-007-15`,
`FIND-TASK-007-16`, and `FIND-TASK-007-17`. Findings 15 and 17 are bounded,
mechanical corrections. Finding 16 cannot be routed to an implementation
remediation task without choosing or changing a material secret-provider and
ownership contract. The appropriate task-review outcome is therefore
`SPEC_REVISION_REQUIRED`.

## Verification limits

- Validation was static and time-bounded. It inspected the approved contract,
  all Wave 1 reports, prior ledgers/remediation tasks, cumulative diff, and the
  full live owners/callers relevant to every retained finding. It did not rerun
  the broad Cargo/Postgres/Python/TypeScript lanes already recorded by Wave 1.
- No executable check can prove compliance with the absent shared-resolver
  boundary; current tests prove only the substituted concrete Vault path.
- The MCP descriptor and import-shape findings are not enforced by current
  codegen/lint gates; their focused closure proof is therefore source/catalog
  inspection plus the existing narrow lanes.
- Credentialed Slack/PagerDuty smoke remains gated release evidence and is not
  relevant to any retained finding.
