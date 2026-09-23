# TASK-001 round 2 — Review Verdict

## Verdict: `FIX_REQUIRED`

Retained findings: `FIND-TASK-001-2`, `FIND-TASK-001-4`,
`FIND-TASK-001-5`, `FIND-TASK-001-11`, `FIND-TASK-001-12`,
`FIND-TASK-001-13`, and `FIND-TASK-001-14`.

Remediation task: `TASK-001-R2-verifier-contract-closure.md` in this directory.

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/verified-change-contract`
- Base: `5293546f33b3a5fd9de529098e23ea70d472c412`
- Cumulative candidate: `dd0503e7149017d760a987346e2838001e5c3429`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-001-verifier-contract-and-registration.md`
- Prior review and remediation: `review/TASK-001-r1/`
- Current worktree HEAD: `5f14f3c325cc8c45081fe9ad0543e7794b4a4e0f`; its only tracked difference from the candidate is the current `AGENTS.md` process authority.

The complete `5293546f3..dd0503e71` range was reviewed. The candidate remained
resolvable and the reviewed source remained unchanged through both waves.

## Review topology and results

| Wave | Role | Report | Result |
|---|---|---|---|
| 1 | task implementation | `task-review.md` | FAIL (`TR-1`…`TR-4`) |
| 1 | repository standards | `standards-review.md` | FAIL (`SR2-1`, `SR2-2`) |
| 1 | security and tenancy | `domain-review-security-tenancy.md` | FAIL (`DS-R2-1`) |
| 1 | data, durability, and concurrency | `domain-review-data-durability.md` | FAIL (`DDR2-1`, `DDR2-2`) |
| 1 | public contracts and projections | `domain-review-contracts.md` | FAIL (`DC-1`, `DC-2`) |
| 2 | structured Ponytail validation | `findings-validation.md` | FIX_REQUIRED (seven retained findings) |

All reviewers were fresh and independent. Wave 1 reviewers did not receive
each other's conclusions or an intended verdict. Wave 2 independently checked
the source, callers, reachability, task relevance, corrections, and finding-ID
continuity before deduplicating the ledger.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-045/046/047/056/109, INV-014: one closed, registrable Verifier Card with Drift/Eval implementations only | `wyrd-spec` envelope and Verifier contract; 15-kind catalog | contract tests and generated schemas | PASS, except unresolved HTTP schema closure (`FIND-TASK-001-14`) |
| REQ-110/111, INV-012: approved Drift/Eval payloads and engines retained | Drift validation and Eval payload owners | focused validation and shared-crate evidence | PASS |
| REQ-090/091/092/094/143, AC-018: supported binding sites resolve and fail closed before writes | shared binding-site owner and server validation path | pure nested-refusal tests and registration journeys | FAIL: required authenticated server-boundary proof is absent (`FIND-TASK-001-2`) |
| REQ-093: strict Trigger and Verifier authored shapes | closed enums with unknown-field rejection | strict-decoding tests | PASS |
| REQ-102: duplicate Verifier and Operator identities are rejected | graph binding validation | UID-less referenced duplicates only | FAIL: UID-sensitive rendered keys and unchecked equal inline Operators (`FIND-TASK-001-4`) |
| Unauthorized registration exposes the stable denial contract and writes nothing | server refusal path | status/no-write assertion | FAIL: stable code is not asserted (`FIND-TASK-001-4`) |
| REQ-103/109/114, AC-021: live public surfaces describe Verifier only | registrable kinds and primary generated artifacts use Verifier | codegen/docs evidence | FAIL: CLI and Rust docs still advertise Eval Card/`Spec::Eval` (`FIND-TASK-001-5`) |
| REQ-116/120/144, AC-022: retired tables, pull protocol, and alert-router owners are unreachable | deleted routes, owners, schemas, crate, and migration paths | SQL/Bifrost/codegen evidence | PASS |
| REQ-113 and non-goals: reuse existing owners; no compatibility alias, second walker, DAG, or future implementation | existing visitor, composite registration, clients, and engines | source inspection | PASS |
| Language-agnostic HTTP contract resolves the new Verifier graph | generated OpenAPI contains new references | no reference-closure proof | FAIL: three reachable component types are undefined (`FIND-TASK-001-14`) |
| Repository test integrity and credible required gates | required Bifrost lanes eventually reported green | two recorded failures were accepted after rerun | FAIL: allocator-address and asynchronous-settlement assertions remain nondeterministic (`FIND-TASK-001-11`, `FIND-TASK-001-12`) |
| Rust documentation hard blocker for new/materially changed items | production remediation docs improved | bounded test/helper audit | FAIL: panicking tests/helpers omit required panic contracts (`FIND-TASK-001-13`) |
| Tenant credential-admission repair | test tenant is seeded before mint/use | all related call sites inspected | PASS; no latent sibling finding retained |

## Prior-finding closure

| Prior finding | Result |
|---|---|
| `FIND-TASK-001-1` | CLOSED — retired Bifrost target removed from lane and change detection. |
| `FIND-TASK-001-2` | OPEN — production validation is fail closed, but its required authenticated server proof is missing. |
| `FIND-TASK-001-3` | CLOSED — strict Trigger/Verifier decoding is covered. |
| `FIND-TASK-001-4` | OPEN — duplicate identity enforcement and the denial-code proof remain incomplete. |
| `FIND-TASK-001-5` | OPEN — live adjacent CLI and Rust documentation still states the retired model. |
| `FIND-TASK-001-6` | CLOSED — pull-protocol wire types, lease token, fixtures, exports, and CLI variants are gone. |
| `FIND-TASK-001-7` | CLOSED — task-plan prose was removed from production source. |
| `FIND-TASK-001-8` | CLOSED — the five named fallible production items are documented. |
| `FIND-TASK-001-9` | CLOSED — effective-spec orchestration is struct-owned. |
| `FIND-TASK-001-10` | CLOSED — imports and signature types follow repository rules. |

## Validated finding ledger

The decision-complete evidence and corrections are in
`findings-validation.md`.

| ID | Status | Classification | Summary |
|---|---|---|---|
| `FIND-TASK-001-2` | REVISED | MISSING | Nested inline-Agent refusal lacks the required authenticated HTTP stable-code/no-write proof. |
| `FIND-TASK-001-4` | REVISED | INCORRECT | Duplicate identity checks use UID-sensitive display values, omit equal inline Operators, and one denial proof omits its stable code. |
| `FIND-TASK-001-5` | REVISED | DRIFT | Live CLI remediation and public Rust docs still advertise the retired Eval Card model. |
| `FIND-TASK-001-11` | CONFIRMED | VIOLATION | Allocator addresses make two required cluster restart tests nondeterministic. |
| `FIND-TASK-001-12` | CONFIRMED | VIOLATION | The Oracle journey samples cleanup before its asynchronous settlement boundary. |
| `FIND-TASK-001-13` | REVISED | VIOLATION | New and materially changed panicking Rust tests/helpers omit required panic documentation. |
| `FIND-TASK-001-14` | CONFIRMED | INCORRECT | Generated OpenAPI references three new Verifier components that it does not define. |

No retained correction changes approved behavior or requires a new product,
public API, architecture, security, compatibility, concurrency, ownership, or
persistent-data decision. `SPEC_REVISION_REQUIRED` does not apply.

## Verification limits

- Reviewers inspected the immutable cumulative diff and committed remediation evidence; the broad multi-hour lanes were not rerun during review.
- The candidate records the required lanes as eventually green, but also records the two retained Bifrost failures before rerun. A passing retry is not credible closure for those assertions.
- `codegen:check` establishes reproducibility, not that all OpenAPI component references resolve.
- No second reachable unseeded-tenant test was found; hypothetical unrelated cases were not promoted into findings.

## Verdict

`FIX_REQUIRED`. Route `TASK-001-R2-verifier-contract-closure.md` directly to
`$wyrd-implement`, then reassess the complete base-to-new-candidate range in a
third task-review round. The skill limit is three fix rounds.
