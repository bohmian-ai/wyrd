# TASK-001 round 4 — Wave 2 findings validation

## Immutable subject and reviewed inputs

- Repository: `/Users/stevenforrester/Documents/GitHub/verified-change-contract`
- Original base: `5293546f33b3a5fd9de529098e23ea70d472c412`
- Cumulative candidate: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-001-verifier-contract-and-registration.md`
- Prior authority: every verdict, validated ledger, and remediation task under
  `review/TASK-001-r1/`, `review/TASK-001-r2/`, and
  `review/TASK-001-r3-retry1/`; the interrupted round-3 verdict was treated
  only as review-process history
- Wave 1 inputs: `task-review.md`, `standards-review.md`,
  `domain-review-contracts.md`, `domain-review-data-durability.md`, and
  `domain-review-security-tenancy.md` in this directory

`HEAD` was exactly `c8bb490ad814c0c7770cac33ed7779897ff776e4`
at the start and end of validation. The only worktree entries were the new
round-4 review artifacts. The repository has no `.codegraph/` directory, so
source discovery used `rg`, Git, and direct reads.

I independently inspected the complete `5293546f3..c8bb490ad` range, the
applicable Wyrd, doctrine, Bifrost, security, testing, and execution
authorities, the source bodies and callers implicated by every prior finding,
and the final evidence correction. I did not treat implementation summaries or
green aggregate lanes as substitutes for source behavior.

## Wave 1 finding-union validation

Every Wave 1 report proposes an explicitly empty finding set and returns
`PASS`. That empty union is independently supported:

| Wave 1 report | Validation | Result |
|---|---|---|
| Task implementation | The acceptance matrix maps every TASK-001 obligation and non-goal to current source and credible proof. Direct inspection found no missing registration, retirement, schema, or public-contract behavior. | **CONFIRMED PASS** |
| Repository standards | The cumulative code retains one contract owner, one binding-site inventory, server-owned durable behavior, append-only migrations, generated-artifact ownership, exact command evidence, and the required Rust documentation. | **CONFIRMED PASS** |
| Contracts and schemas | The 15-kind catalog and closed Drift/Eval implementation union remain exact; strict decoding, retired-kind refusal, and recursive OpenAPI component closure are present. | **CONFIRMED PASS** |
| Security and tenancy | The authenticated route checks permission before registration, tenant resolution stays on `TenantConn`, nested bindings fail before writes, canonical duplicate identity ignores presented UID, and stable refusal codes/no-write behavior are proved. | **CONFIRMED PASS** |
| Data, durability, and concurrency | The retired tables have no live owner, the canonical registry contains six built-ins, restart proof exercises replacement pools instead of allocator addresses, and Oracle cleanup preserves the exact six-field zero invariant under a finite poll. | **CONFIRMED PASS** |

No contradiction between Wave 1 reports requires resolution, and no report
relies on a missing authority or unavailable source.

## Independent caller and correction tracing

### Registration and binding validation

`register_card_http` reaches the cards service, whose `validate_request`
decodes the submitted `Spec` and calls `spec_binding_errors` before resolution
or durable writes. `Spec::binding_sites` is now the single inventory for legal
Service, component, and standalone-Agent bindings and structurally reachable
inline-Agent bindings. Nested nonempty sites are refused; legal sites flow to
`binding_validation_errors` and then to `EffectiveSpecs::validate_bindings` for
tenant-scoped effective Trigger and Operator bodies. Resolution, UID pinning,
relationship derivation, loader diagnostics, and server validation therefore
reuse the same owner rather than parallel location walkers.

The combined `referenced_binding_refusals_leave_no_writes` test is the smallest
credible cross-boundary proof. Its fifth case uses the existing authenticated
server and writer fixture to submit a Workflow containing an inline Agent with
`verified_by`, asserts HTTP 400 and
`WYRD_REGISTRY_400_INVALID_CARD_SPEC`, and reuses the same durable no-write
assertion. Splitting it into another server-starting test would duplicate the
harness without exercising a different seam.

Referenced Verifiers and Operators use `CardRef::identity_key()`, the same
identity resolution uses and one that deliberately excludes optional UID.
Inline Operators use existing typed equality and a bounded linear scan over
one binding's small `on_failure` list. The inline path has no canonical ordered
identity to reuse; a digest, trait, or new key type would add machinery without
improving this contract.

### Strict authored contract and OpenAPI closure

`VerifierImplementation` is an adjacently tagged, `deny_unknown_fields`
Drift/Eval enum. `TriggerActivation::ObservationsReady {}` is a fieldless
struct variant so enum-level `deny_unknown_fields` rejects sibling typos and
secret-shaped input while preserving the authored
`{kind: observations_ready}` form.

The existing `WyrdApiDoc` component owner registers `VerificationBinding`,
`VerifierImplementation`, and `TriggerActivation`. The Verifier schema owner
forwards `DriftSpec`'s existing transitive dependencies. The focused closure
test walks `$ref`s from the five task-owned roots and fails if any reachable
component is undefined. This is a necessary check that artifact regeneration
alone cannot supply; extending it to every historical OpenAPI root or deleting
the harmless defined-but-unreferenced `DriftSpec` component would be unrelated
cleanup.

### Restart and Oracle proof

`PostgresPoolIdentity` and its only two address-comparison callers are gone.
Both restart tests now query the replacement application and Vala pools while
retaining stopped-node absence, survivor availability, WAL, resource-plan,
reset-state, role, and credential assertions. This directly proves usable
replacement ownership and cannot flake through allocator reuse.

The Oracle journey repeatedly reads the existing inspection owner under the
300 × 100 ms bound already justified by terminal-before-settlement ordering.
Success still requires all six ownership counters to equal zero; timeout
reports the final snapshot. No production wait primitive, retry waiver, or
weakened invariant was introduced.

### Public vocabulary, table count, and later-task boundary

The final source corrections change descriptions only. The validated CLI,
Eval-engine, permission-resource, and `SourceSpec` owners now describe
eval-backed or drift-backed Verifier Cards, and the canonical six-entry table
registry says six in both previously stale rustdocs. No API, stable error,
resource variant, behavior, schema, helper, or dependency was added.

Residual `eval_ref`/`drift_ref` observation-record fields and their adjacent
documentation are explicitly removed and reprojected by TASK-002. Private test
expectation strings, non-authoritative design renders, and UI presentation are
not alternate registrable contracts and are not required by TASK-001's
contract/loader/registry/schema/CLI/SDK/HTTP/MCP acceptance surface. Sweeping
them now would duplicate later work or broaden this remediation without a
reachable TASK-001 defect.

### Evidence chronology and user-approved history

The three corrected R2 command cells show the currently applicable pinned
commands. The immediately adjacent addendum explicitly states that the
original executions used raw Cargo and that later executions reran the same
selectors through `mise exec -- cargo nextest run --locked`, retaining their
environment wrappers, features, profiles, ignored flags, and `--retries 0`.
Git preserves the pre-correction cells. The record therefore corrects the
proof form without rewriting or concealing chronology.

The registration test records one pass; the restart pair records three
consecutive two-test passes; and the Oracle journey records three consecutive
passes, all without retry. The named selectors correspond to current tests and
the environment-owning wrappers match their Postgres/migration needs.

`FIND-TASK-001-16` remains **RESOLVED / NOT NEEDED** by the user's explicit
approval to retain commit `5f14f3c32`, its unrelated `AGENTS.md` process edit,
and its existing attribution metadata. That current user decision outranks the
earlier proposed cleanup and authorizes no history rewrite. It is not an open
implementation finding and is omitted from the retained ledger.

## Prior-finding closure

| Finding | Independently validated closure |
|---|---|
| `FIND-TASK-001-1` | Deleted Eval test target is absent from Bifrost routing and build/change-detection references; the capability aggregate is recorded green. |
| `FIND-TASK-001-2` | Shared nested-site validation fails closed, and the combined authenticated route case proves the stable 400 and no registration writes. |
| `FIND-TASK-001-3` | Trigger and Verifier discriminated unions reject unknown and secret-shaped sibling fields without changing their approved wire shapes. |
| `FIND-TASK-001-4` | All required forms and refusals are covered; duplicate identity uses existing canonical owners; exact RBAC denial and no-write proof remain. |
| `FIND-TASK-001-5` | Every retained live TASK-001 description named by the validated ledgers uses the Verifier model; TASK-002-owned record migration stays deferred. |
| `FIND-TASK-001-6` | Eval pull-route request/response, lease identity, CLI/client protocol, fixtures, generated exports, and dead errors are absent while reusable Eval engine types remain. |
| `FIND-TASK-001-7` | Production source contains no task-plan justification or agent/process reference. |
| `FIND-TASK-001-8` | Materially changed fallible Rust items carry the required intent and error contracts. |
| `FIND-TASK-001-9` | `EffectiveSpecs` is the dependency-owning registration-validation owner; no free orchestration function rethreads its connection and resolution cache. |
| `FIND-TASK-001-10` | New imports are module-scoped and signatures use imported bare names; test-module imports use the explicit repository exception. |
| `FIND-TASK-001-11` | Pointer identity and its only callers are deleted; replacement pool usability and lifecycle invariants have deterministic no-retry proof. |
| `FIND-TASK-001-12` | Oracle settlement is bounded and preserves the exact six-field zero invariant with deterministic no-retry proof. |
| `FIND-TASK-001-13` | The bounded remediation audit records complete intent and panic documentation without a waiver or redundant permanent checker. |
| `FIND-TASK-001-14` | Every component reachable from the five Verifier/binding/Trigger OpenAPI roots resolves through the existing schema owner. |
| `FIND-TASK-001-15` | Both canonical built-in-table descriptions match the six-entry registry. |
| `FIND-TASK-001-16` | **RESOLVED / NOT NEEDED** by explicit user approval; no history operation is required or authorized. |
| `FIND-TASK-001-17` | All three named proof groups were rerun through exact pinned commands, and the addendum preserves original-versus-rerun provenance. |

## Ponytail assessment

The retained implementation stops at existing owners and native mechanisms:
the existing Card envelope and typed enums, `Spec::binding_sites`,
`CardRef::identity_key`, typed equality, `EffectiveSpecs`, the existing
OpenAPI component hook, SQL health queries, and Tokio's bounded sleep. The
remediations mostly delete dead paths or replace unreliable proof. No new
dependency, trait, generic layer, factory, configuration knob, test harness,
compatibility path, runtime registry, or speculative implementation variant is
present. Optional cleanup identified in prior rounds remains correctly
rejected.

## Final validated finding ledger

Explicitly empty. All Wave 1 findings unions are empty, every previously
retained finding is closed or explicitly resolved/not-needed, and independent
source inspection found no new reachable TASK-001 defect.

## Verification limits

- This validation did not rerun the expensive Postgres, restart, Oracle, or
  broad workspace lanes. Their exact committed commands, selectors, results,
  and source assertions were inspected; the security reviewer also
  independently reran the authenticated registration proof.
- `git diff --check 5293546f3..c8bb490ad` passed during this validation.
- Untracked round-4 reports are orchestration artifacts and do not alter the
  immutable candidate.

## Recommendation

**PASS**

The cumulative candidate satisfies TASK-001 exactly, preserves its non-goals
and later-task boundaries, and has an explicitly validated empty finding
ledger.
