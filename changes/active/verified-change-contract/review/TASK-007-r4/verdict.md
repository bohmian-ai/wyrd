# TASK-007 R4 Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `e2da27694e8a3d057f2ff863c5d17429adc0a495`
- Approved specification: `changes/active/verified-change-contract/spec.md`,
  revision 35
- Original task:
  `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Remediation inputs: the R1, R2, and R3 Operator delivery correction tasks
  and their preserved review artifacts

The candidate remained the stated commit throughout both review waves. The
review covered the complete cumulative base-to-candidate range, not only the
R3 correction.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| `REQ-097`-`REQ-099`, `REQ-138`: failed-only atomic dispatch fanout and immutable bounded context | Transactional run settlement and idempotent dispatch insertion; Postgres fanout and provider journeys | PASS |
| `REQ-139`, `REQ-147`, `INV-006`-`INV-007`: tenant-owned encrypted credentials, exact authority, redaction, RLS, audit, and key rotation | Forced-RLS connection storage, envelope encryption, per-attempt lookup, permission/audit journeys, focused security tests, and tenant-isolation check | **FAIL**: the concrete cryptographic path is secure, but `FIND-TASK-007-16` violates the approved external-resolver ownership boundary |
| `REQ-140`-`REQ-143`: Slack, PagerDuty, and HTTP delivery; bounded retries/redirects; Workflow remains non-executable | Private provider adapters, resolve-screen-pin ordering, stable dispatch identity, local-provider journeys, and focused delivery tests | PASS |
| `REQ-145`-`REQ-146`, `REQ-152`, `INV-015`: authorization, fair bounded claims, database time, fencing, restart, shutdown, health, and metrics | Canonical permissions/audit, permit-before-claim loop, database-clock SQL, fencing/fairness/restart coverage | PASS |
| `REQ-148`, `REQ-150`, `AC-030`-`AC-031`: typed HTTP, Rust, Python, TypeScript, CLI, and MCP connection management | Shared wire/client contracts, generated schemas/stubs, real-server journeys, codegen and boundary checks | **FAIL**: `FIND-TASK-007-17` leaves the MCP list description inconsistent with its typed response |
| `REQ-149`: registration and delivery enforce exact active authority before credential use | Shared compatibility predicate, registration refusal, rotation, redirect, and credential-ordering coverage | PASS |
| `AC-029`: real-server provider journey and gated live-provider smoke | Local Slack/PagerDuty/HTTP mock-provider journey is present; live credentialed smoke remains an explicit release gate | PASS with verification limit |
| Repository Rust declaration/import rule | Full cumulative source inspection and green format/lint checks | **FAIL**: `FIND-TASK-007-15` remains at candidate-added declaration sites not enforced by current gates |
| Scope and non-goals: no unapproved provider/ownership contract | Candidate adds a direct HashiCorp KV v2 configuration/client instead of the approved backend-agnostic `SecretRef::Vault` resolver; the named shared resolver is absent | **FAIL**: `FIND-TASK-007-16` |
| R1-R3 remediation obligations | Prior ledgers, cumulative source inspection, and fresh focused key/version/role tests | PASS for `FIND-TASK-007-1` through `-14`; `FIND-TASK-007-15` is reopened and revised for additional sites |

## Wave results

| Review | Result | Material outcome |
|---|---|---|
| Task implementation review | FAIL | Proposed the external-secret resolver boundary violation |
| Repository standards review | FAIL | Proposed remaining qualified declaration types and the inaccurate MCP list descriptor |
| Security domain review | PASS | No material security, RBAC, tenancy, secret, SSRF, or audit finding |
| Delivery domain review | PASS | No material durability, concurrency, retry, restart, provider, or persistence finding |
| Structured Ponytail validation | COMPLETE | Confirmed or revised three findings and required specification revision |

## Validated finding ledger

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-007-15` | REVISED | VIOLATION | Import the already-used declaration types at module scope and use bare names at the enumerated candidate-added sites; add no abstraction or dependency. |
| `FIND-TASK-007-16` | CONFIRMED | VIOLATION | Obtain human approval for either the missing shared external-secret resolver ownership/configuration contract or a revised HashiCorp-only contract. This is a product, security, architecture, and cross-service ownership decision. |
| `FIND-TASK-007-17` | CONFIRMED | INCORRECT | Remove the false `secret version` promise from the MCP list descriptor and pin the descriptor with the existing MCP catalog proof. |

The exact locations, caller traces, consequences, correction boundaries, and
closure proofs are preserved in `findings-validation.md`.

## Prior-finding closure

- `FIND-TASK-007-1` through `FIND-TASK-007-14`: **CLOSED**. The cumulative
  candidate retains the independently reviewed R1-R3 corrections.
- `FIND-TASK-007-15`: **OPEN / REVISED**. R3 fixed its enumerated sites, but
  the same mandatory import rule remains violated at additional
  candidate-added declarations.
- `FIND-TASK-007-16` and `FIND-TASK-007-17`: **OPEN / NEW**.

## Verification limits

- Reviewers did not rerun every broad Cargo, Postgres, Python, TypeScript, CLI,
  MCP, and journey lane already recorded green by the implementation and prior
  reviews. Fresh evidence included focused key/version/role, security, SSRF,
  delivery, tenant-isolation, format, codegen, client-tier, PyO3-scope,
  unwrap-audit, and cumulative diff checks.
- The credentialed Slack/PagerDuty smoke was not run. It remains intentionally
  gated release evidence; local provider journeys cover the repository-owned
  wire, retry, failure, and disclosure contracts.
- CodeGraph was unavailable in this checkout, so reviewers traced source and
  callers directly.
- No executable test can prove compliance with the absent shared external-
  secret resolver boundary. Current tests prove only the substituted concrete
  HashiCorp path.

## Verdict

**SPEC_REVISION_REQUIRED**

`FIND-TASK-007-16` cannot be packaged as an implementation remediation without
choosing or changing an expensive-to-reverse secret-provider and ownership
contract. No R4 remediation task is written. After the specification decision
is approved, a fresh task must include that decision plus the bounded
corrections for `FIND-TASK-007-15` and `FIND-TASK-007-17`, and the next review
must reassess the complete cumulative candidate.
