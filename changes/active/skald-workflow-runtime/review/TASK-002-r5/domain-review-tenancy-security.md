# TASK-002-r5 tenancy and security domain review

**Result: PASS.** No material tenancy, authorization, audit, identity,
credential, input-boundary, or security finding is proposed.

## Immutable subject and reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `2d669917c03699876b3c8926f0de5ac88c578c01`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-002-cleanup.md`
- Remediation inputs: `TASK-002-R2-close-cleanup-review-gaps.md`,
  `TASK-002-R3-close-remaining-review-gaps.md`, and
  `TASK-002-R4-close-workflow-selector-validation.md`

The candidate remained `HEAD` before and after this review. I reviewed the
complete base-to-candidate range and the current source, using prior reports
only as hypotheses and closure obligations.

This domain pass traced:

- authored local, external, and registered Workflow graph reads;
- exact version and UID identity assertions, Active-state refusal, and
  wrong-kind or malformed selector refusal;
- tenant derivation, RLS, RBAC, and canonical audit on client-visible reads;
- server composite-registration preflight, exact-UID write recheck,
  relationship binding, transaction ownership, and audit coupling;
- Workflow/Agent principal semantics and credential non-transfer; and
- Rust, Python, and TypeScript selector conversion and public journey proof.

Provider egress, gateway credential resolution, and server Workflow-run
authority are later-task surfaces and were not treated as TASK-002 acceptance
criteria. No changed TASK-002 path introduces a URL fetch, cryptographic
primitive, redirect, webhook, or deployment-secret boundary.

## Authority and source coverage

| Boundary | Governing authority | Producer-to-consumer evidence | Assessment |
|---|---|---|---|
| Effective tenant and read authorization | `AGENTS.md` §§2, 9; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md` security principles, authorization, tenant isolation, and audit; Revision 12 REQ-055/056 and INV-006 | `Workflow::from_path` lazily constructs the ordinary `Cards` context only for external refs; `GraphTraversal` and `resolve_refs` call `reads::get_response`; `/v1/cards/by-ref` and `/v1/cards/by-uid` authenticate, authorize `cards:read`, append canonical audit, then open `registry_tenant_conn(caller.data_tenant_id)` | PASS: no payload, selector, relationship, or ambient header selects the effective tenant; every external Card read uses the established audited route and tenant RLS. |
| Exact root and transitive identity | Revision 12 resolution contract, REQ-056, INV-005, AC-029/030; Wyrd design CardRef rules | `reads::get_response` calls `assert_selector_identity`; exact selectors compare kind/space/name/version and optional UID; graph traversal compares each response with its UID-bearing relationship reference; R4 makes `VersionBlock` deserialize through `parse` | PASS: malformed/ranged exact versions cannot inhabit the exact-version type, mismatched UIDs refuse, and newer versions cannot float into a loaded graph. |
| Active lifecycle and cross-tenant invisibility | Security posture tenant/data isolation; REQ-056; AC-030 | `WorkflowBodies::extend_registered` refuses non-Active returned Cards; server reads exclude deleted/invisible rows under RLS; `fetches_and_executes_locked_workflow_graph` covers foreign tenant, pending/deleted dependency, missing identity, wrong kind, and UID mismatch | PASS: inactive or foreign dependencies publish no Workflow and dispatch no provider/tool call. |
| Authored provenance | Revision 12 resolution/ownership contract, REQ-055/056/059; prior FIND-TASK-002-1 | Loader retains `Sibling` versus external `Ref`; `WorkflowBodies` stores siblings separately from registered Cards and resolves each variant only from its own source; `EffectiveSpecs` likewise separates `siblings` and `externals` | PASS: a caller-controlled local sibling cannot satisfy an external ref or bypass its registry authorization, audit, lifecycle, or identity checks. |
| Registration tenant, transaction, and audit | `AGENTS.md` §§2, 9; security posture production/audit patterns; agent rules SQL tenancy and registry coupling; REQ-014/028/056/059 | `resolve_external` opens a verified-caller `TenantConn`; `EffectiveSpecs::resolve/load` reads Active external bodies under that connection; `RegistrationWriter::write` appends allowed verdicts on the write transaction, rechecks exact `(CardRef, CardUid)` rows `FOR SHARE`, then binds refs and persists nodes/relationships before one commit | PASS: preflight cannot cross tenants; stale replacement, lifecycle change, audit failure, binding failure, or cancellation commits no partial registration. |
| Workflow identity and credential non-transfer | Security posture principal lifecycle; Revision 12 REQ-057 and INV-003/008 | `persist_node` provisions Card-bound principals only for `Service` and `Agent`; Workflow loading retains bodies/runtime identity but not `Cards`, tokens, roles, or a referenced Agent principal; execution uses the caller's existing runtime/tool registry | PASS: Workflow creates no principal or credential, and reusing an Agent transfers no RBAC authority. |
| SDK input validation | `AGENTS.md` §§7–9; errors and TypeScript references; REQ-054/056, INV-007, AC-030; prior FIND-TASK-002-11 | Python uses typed constructors; Rust exact values now preserve `VersionBlock`'s constructor invariant under Serde; TypeScript native parsing keeps a closed `deny_unknown_fields` shape, validates raw strings with `CardUid`, `SpaceName`, `CardName`, and `VersionBlock`, and attributes the precise malformed field before IO | PASS: malformed values and mixed/incomplete shapes fail locally with `WYRD_WORKFLOW_400_INVALID_CARD_REF`; registry defense remains unchanged. |
| Dependency and mechanism drift | Human standing direction; AGENTS dependency ownership; R4 non-goals | R4 adds only the existing workspace `wyrd-semver` crate to the TypeScript native boundary and uses ordinary Serde validated-newtype and domain-constructor patterns; lockfile adds no third-party package | PASS: no bespoke validator framework, scanner, gate, setting, option, cache, compatibility path, or security mechanism was introduced. |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. No speculative hardening is required for acceptance.

### Positive Controls

- Tenant identity is derived from the verified caller and bound through
  `TenantConn`/Postgres RLS; Card names, UIDs, refs, and request bodies cannot
  select another tenant.
- Each remotely loaded Workflow, Agent, and Prompt is read through the existing
  `cards:read` authorization and canonical audit boundary.
- Exact response identity is checked at the client, including optional UID
  assertions, and registered transitive relationships remain UID-bound.
- Registration preflight preserves sibling/external provenance, while the
  write transaction rechecks the exact Active UID and holds its lifecycle lock
  through relationship persistence and commit.
- Registration audit and writes remain atomically coupled; failures use the
  existing fail-closed audit behavior and do not create a parallel audit sink.
- Loading and registration do not resolve provider secrets or execute tools or
  providers. Workflow objects retain no client credential or referenced-Agent
  authority.
- Public selector conversion rejects malformed values before registry IO and
  does not expose raw database, token, or secret material.

## Prior-finding closure

- **FIND-TASK-002-1 remains closed:** client and server body stores preserve
  `Sibling`/external provenance through the final Skald lookup.
- **FIND-TASK-002-4 remains closed:** preflight's exact UID survives into the
  write plan and is rechecked with identity, Active state, RLS, and `FOR SHARE`
  locking before any durable binding.
- **FIND-TASK-002-11 is closed:** `VersionBlock` deserialization now delegates
  to its established parser; TypeScript validates each raw selector field with
  the existing domain constructor and retains `selector` only for shape
  failures. This is the standard ecosystem/repository mechanism selected by
  R4, not new validation machinery.
- The remaining prior findings do not reopen a security boundary: exact
  relationship proof now exists in all three language journeys, generic Python
  selector errors retain their owning contract, and the blocking-pool/Hakari/
  hygiene corrections introduce no security or tenancy behavior.

## Verification evidence and limits

I independently ran:

```text
mise exec -- cargo nextest run --locked -p wyrd-semver --lib \
  -E 'test(=block::tests::serde_preserves_the_exact_version_invariant)'
```

Result: 1 selected, 1 passed. The cumulative
`git diff --check 0569b79702218600c4f9790f45cc03100d5c6f1c
2d669917c03699876b3c8926f0de5ac88c578c01` also passed.

Candidate implementation evidence records all five R4 focused commands and
the affected aggregate/type/codegen/boundary lanes passing. R4's prior
candidate-bound verification directly records the three SDK journeys, the
server registration/UID-fence tests, SQL lifecycle-race test, generated
contracts, typing, N-API, client-tier, PyO3, and registry-transaction checks on
the immediately preceding reviewed candidate. I inspected the final R4-only
diff and its tests; I did not independently rerun Postgres, Python, or Node
integration suites in this domain pass. That is a verification limit, not a
material security finding, because the relevant final changes are the
validated-newtype and TypeScript boundary conversions and the supplied final
evidence records those exact journeys passing.

No fresh audit-writer outage was injected. Fail-closed read and registration
audit behavior is established by the unchanged canonical route/audit owners
and transaction flow, not claimed as new outage-test evidence.

## Material findings

None.

## Overall result

**PASS** — the cumulative candidate satisfies TASK-002's tenancy, exact
identity, authorization, audit, lifecycle, credential non-transfer, and
selector-validation obligations. It uses established Wyrd, Rust/Serde, Tokio,
PostgreSQL, and SDK mechanisms; no unsupported mechanism or remediation is
required.
