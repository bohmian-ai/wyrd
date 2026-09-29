# TASK-003 R3 Task Implementation Review

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Original base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Cumulative candidate: `a5a5b60f446981760ac831f64aba871582ec45e4`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- Prior reviews and validated ledgers: `changes/active/verified-change-contract/review/TASK-003-r1/` and `changes/active/verified-change-contract/review/TASK-003-r2/`
- Prior remediation tasks: `TASK-003-R1-close-binding-activity-contract.md` and `TASK-003-R2-close-typescript-tenancy-and-gate.md`
- Review scope: complete original-base-to-cumulative-candidate range; implementation completion summaries were not treated as proof

The repository was at the pinned candidate before inspection. `.codegraph/` is absent, so source and caller tracing used the immutable Git range and direct repository reads.

## Full Acceptance Matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `REQ-078`: TASK-003 binding control state is owned by `wyrd-sql`, stored in the `wyrd` schema, forced-RLS protected, and reached through caller-owned `TenantConn` transactions | `20260601000027_verification_bindings.sql:53-101`; `queries/verification.rs:251-301`; registration transaction in `cards/service.rs:1080-1172` | Projection rollback/tenant tests inspected; `mise run check:tenant-isolation` passed independently | PASS |
| `REQ-095`, `REQ-102`, `REQ-104`: one effective binding is one exact subject occurrence plus Verifier version, with a stable typed UUIDv7 identity | `BindingId`; `OWNER_OCCURRENCE_KEY`; non-null occurrence column; natural-key constraint; `BindingProjector::freeze`; `project_bindings` | UUIDv7, owner/component, reapply, reorder, and alias-collision tests inspected | PASS |
| `REQ-104`: effective referenced Trigger/Operator identities and inline bodies are frozen by UID or canonical digest | `cards/service.rs:1220-1296`; migration trigger/operator columns and integrity checks | Registration-route frozen-target assertions inspected | PASS |
| Invalid or impossible effective schedules are refused before durable registration writes | `cards/resolve.rs:187-224` parses the resolved Trigger and requires a future occurrence | Impossible-date and rollback assertions inspected | PASS |
| `REQ-105`, `REQ-106`: registration does not activate; only successful API-key or workload `jwt-bearer` issuance for the exact Card-bound Service/Agent records activity in the issuance transaction | `TenantGrant::records_owner_activity`; `TenantTokenIssuer::issue`; `record_machine_authentication` | API-key and workload real-boundary journeys plus issuance Postgres cases inspected | PASS |
| `REQ-105`, `REQ-106`: delegation, OIDC login, human refresh, Card-free automation, cached bearer use, component scope, ordinary observations, and another Card version do not renew activity | Closed grant gate and Card-bound active-machine SQL predicate; no request/observation writer in the cumulative diff | Exclusion, stale-token, observation, A/B, and replica evidence inspected; nonexistent TASK-004 SYSTEM mint correctly remains excluded | PASS |
| `REQ-106`: out-of-order exchanges cannot move activity backward | `GREATEST(last_authenticated_at, $2)` in `RECORD_AUTHENTICATION_SQL` | Reverse-order Postgres test inspected | PASS |
| `REQ-107`, `REQ-108`: owner/Card lifecycle, the 86,400-second default inactivity window, exact version, component inheritance, and shared-principal replicas gate future work | `BINDING_ACTIVITY_SQL`; `InactivityTimeout`; typed `BindingActivity` conversion | Lifecycle/window/component/A-B/replica tests and assembled activity journey inspected | PASS |
| `REQ-108`, `REQ-112`: first qualifying authentication arms only null schedule cursors to the next boundary; renewal does not move an armed cursor or backfill | `UNARMED_SCHEDULES_SQL`; `ARM_SCHEDULE_SQL`; `BindingSchedule::next_after`; issuance transaction composition | First-exchange/renewal cursor test and assembled journey inspected | PASS |
| `REQ-112`: Card, Card-bound principal, resolved binding projection, registration operation, and allowed authorization audit commit or roll back together | One registration `TenantConn`; `persist_node`; `BindingProjector`; transaction-carried audit events | Projection rollback and registration audit-failure cases inspected | PASS |
| `REQ-145`, `INV-007`, auth portion of `AC-030`: Operator-bearing registration additionally evaluates and audits `operators:invoke`; Operator-free registration uses only `cards:write`; denial writes no Card state | `register_card_http`; `dispatches_operators`; `record_allowed`; transaction-carried allowed events | Inline/reference/no-Operator, denial, rollback, and audit-cardinality route cases inspected | PASS |
| Ambiguous partial CardRef principal resolution fails closed while exact references retain A/B and space identity | Shared `SERVICE_ACCOUNT_BY_CARD_REF_SQL` fetches at most two and `service_account_by_card_ref` accepts exactly one | SQL contract test and issuance/workload/delegation caller coverage inspected | PASS |
| TASK-003 tenancy constraint: the shared `TenantConn` lookup relies solely on forced RLS and carries no manual tenant predicate or bind | `service_accounts.rs:26-33,185-199` now binds only principal kind and CardRef | Focused SQL-text test and `mise run check:tenant-isolation` passed independently | PASS; closes `FIND-TASK-003-12` |
| Durable binding, owner Card, and principal boundaries use existing domain identity types and reject invalid stored UUID versions | `record_machine_authentication(PrincipalId)`; `BindingActivity` fields; private raw-row conversion | Invalid stored-identity and typed-boundary tests inspected | PASS |
| Dependency-backed freeze/projection orchestration has one cohesive concrete owner without speculative public layering | Private transaction-scoped `BindingProjector { conn }`; narrow SQL operations and pure digest helper | Complete caller and body inspection | PASS |
| New and materially modified Rust items satisfy required rustdoc/error/panic documentation without suppression | Cumulative Rust source and test diff | Source audit plus recorded format/lint evidence | PASS |
| `REQ-134`, TASK-003 scenario 4, and binding portion of `AC-028`: Card GET overlays stable binding IDs without mutating authored spec or creating a binding resource | `hydrate_card` loads `owner_binding_ids` and sets `Status.verification` only when the returned list is non-empty | Raw HTTP, Rust SDK, Python SDK, generated schema, and served OpenAPI assertions inspected | PASS for server, Rust, Python, and schemas |
| R2 `FIND-TASK-003-9`: exported TypeScript Card/status types project the complete live status contract, with required `readonly binding_ids: readonly string[]` once `verification` exists, and the existing journey reads it without a cast | `Card.status`, `CardStatus`, and `VerificationStatus` were added and the cast helper was deleted, but `index.ts:917-919` declares `binding_ids?` rather than the required property | `mise run ts:typecheck` and the exact Card integration journey pass, but they accept the weakened optional property | **FAIL (`TREV-003-R3-001`, retained `FIND-TASK-003-9`)** |
| `FIND-TASK-003-10`: served OpenAPI exposes `Card -> Status -> verification -> VerificationStatus -> binding_ids` with UUID items | Shared contract derivation and `pg_openapi_contract.rs:486-544` | Served-document assertion source inspected; `mise run codegen:check` passed independently | PASS |
| `FIND-TASK-003-13`: cumulative whitespace gate is reproducibly green and prior review wording is unchanged apart from whitespace | R1 standards report has only whitespace removal (`git diff -w` is empty for that file) | `git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..a5a5b60f446981760ac831f64aba871582ec45e4` exits zero | PASS; closes `FIND-TASK-003-13` |
| Prohibited scope remains absent: no heartbeat, activity table, activation endpoint, idle refresh, per-request/per-observation activity touch, Vala control store, scheduler, fabricated SYSTEM mint path, second binding identity, compatibility alias, new SDK client, endpoint, generator, SQL lookup, tenant argument, or test harness | Complete cumulative and R2-remediation diffs | Diff and caller inspection | PASS |
| Required cumulative completion evidence is credible | R2 remediation records the focused and broader lanes | Independently reproduced TypeScript typecheck/journey, SQL contract test, tenant isolation, codegen, and cumulative whitespace checks; retained typed-contract mismatch remains | FAIL |

## Prior-Finding Closure

| Prior finding | R3 reassessment against the cumulative candidate | Result |
|---|---|---|
| `FIND-TASK-003-1` | Operator-bearing registration evaluates `operators:invoke`, records denial canonically, and carries allowed events into the registration transaction; no-Operator, inline, referenced, rollback, and cardinality paths remain covered. | CLOSED |
| `FIND-TASK-003-2` | The shared lookup accepts exactly one of at most two active CardRef matches, preserving exact references and refusing ambiguity across API-key, workload, and delegation callers. | CLOSED |
| `FIND-TASK-003-3` | Postgres `GREATEST` keeps activity monotonic and null-only cursor arming prevents renewal from moving an armed cursor. | CLOSED |
| `FIND-TASK-003-4` | Effective schedule validation requires a future occurrence before registration writes. | CLOSED |
| `FIND-TASK-003-5` | `$owner` is non-null and reserved; component aliases cannot collide with it; Agent rows are constrained to the owner occurrence. | CLOSED |
| `FIND-TASK-003-6` | Public verification SQL boundaries use `PrincipalId`, `BindingId`, and `CardUid`; invalid stored binding/Card UUID versions are refused. | CLOSED |
| `FIND-TASK-003-7` | Private transaction-scoped `BindingProjector` owns the dependency-backed freeze/project workflow without a new public abstraction. | CLOSED |
| `FIND-TASK-003-8` | Existing real boundaries cover qualifying API-key/workload exchanges, reachable exclusions, lifecycle/inactivity gates, A/B versions, shared replicas, request-driven stale-token renewal, and observation no-touch. | CLOSED |
| `FIND-TASK-003-9` | The local cast is gone and TypeScript now exports Card status types, but `VerificationStatus.binding_ids` is optional. R2's validated correction required `readonly binding_ids: readonly string[]`; the live Card GET constructs `verification` only from a non-empty list. | **OPEN; `TREV-003-R3-001`** |
| `FIND-TASK-003-10` | Served OpenAPI resolves the nested status schema and UUID array items. | CLOSED |
| `FIND-TASK-003-11` | Added/materially modified Rust items remain substantively documented. | CLOSED |
| `FIND-TASK-003-12` | The CardRef lookup removed `data_tenant_id = $1` and the tenant bind, renumbered the remaining parameters, and retained active filtering, `LIMIT 2`, and exact-one refusal under forced RLS. | CLOSED |
| `FIND-TASK-003-13` | Only trailing whitespace was removed from the preserved R1 standards report, and the exact cumulative `git diff --check` command is green. | CLOSED |

## Proposed Findings

### Important

#### TREV-003-R3-001 — `VerificationStatus.binding_ids` remains optional in the TypeScript public contract

- **Source-local ID:** `TREV-003-R3-001`
- **Stable prior ID:** `FIND-TASK-003-9`
- **Classification:** INCORRECT / CONTRACT PROJECTION
- **Violated obligation:** TASK-003 scenario 4 and `REQ-134` require the binding owner Card read to expose its stable binding IDs. The R2 validated correction is decision-complete: `VerificationStatus` must contain `readonly binding_ids: readonly string[]`; the R2 remediation requires the complete already-live status contract and a readonly string list, not an optional list.
- **Exact location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:916-920`; governing correction at `changes/active/verified-change-contract/review/TASK-003-r2/findings-validation.md:50-62` and `TASK-003-R2-close-typescript-tenancy-and-gate.md:38-59,104-110`.
- **Evidence:** the candidate declares `readonly binding_ids?: readonly string[]`. Rust does use `#[serde(default, skip_serializing_if = "Vec::is_empty")]` and the generated generic schema therefore does not list the property as universally required. That generic omission rule does not change this endpoint's live invariant: `hydrate_card` sets `verification` only when `binding_ids` is non-empty (`crates/wyrd/wyrd-server/src/components/cards/service.rs:326-333`). Consequently every owner Card GET with `status.verification` contains `binding_ids`, and the R2 correction deliberately required the TypeScript projection to encode that narrowed public behavior. The candidate instead widened the type to admit `{ verification: {} }` and documents that widening as ordinary behavior.
- **Observable consequence:** TypeScript callers cannot rely on the R2-guaranteed invariant that the presence of `status.verification` supplies the binding locator; they must handle an impossible missing inner property. The exported SDK contract therefore still differs from the exact accepted remediation contract even though the runtime journey receives the array.
- **Required testable correction:** remove the optional marker so the existing interface declares `readonly binding_ids: readonly string[]`, and update its comment to describe the stable ordered identities without claiming omission in the live verification state. Preserve `Card.status` and `CardStatus.verification` as optional/nullable exactly as the remediation specifies. Keep the existing direct journey; rerun `mise run ts:typecheck`, its exact Card integration selector, `mise run ts:test:integration`, and `mise run codegen:check`. No runtime conversion, new client, endpoint, generator, or test harness is required.

No new finding ID is warranted; this is incomplete closure of the existing `FIND-TASK-003-9` contract obligation.

## Open Questions

None. The remaining correction is fixed by the approved specification and R2 remediation contract; it requires no product, protocol, architecture, security, tenancy, compatibility, concurrency, or persistence decision.

## Verification Notes

- Independently passed `mise run ts:typecheck`.
- Independently passed the exact TypeScript Card journey selector through the repository-managed Postgres wrapper: `cards-state.test.ts -t "registers, reads, hydrates, and loads offline state"`.
- Independently passed the exact `wyrd-sql` unit selector `queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding`.
- Independently passed `mise run check:tenant-isolation` and `mise run codegen:check`.
- Independently passed `git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..a5a5b60f446981760ac831f64aba871582ec45e4`.
- Confirmed `git diff -w` is empty for the R1 standards-report cleanup.
- The broader recorded principal, SQL, Rust/Python/TypeScript journey, format, and lint lanes were inspected through their command evidence and source assertions but were not all rerun in this Wave-1 review.

## Overall Result

**FAIL**

`FIND-TASK-003-12` and `FIND-TASK-003-13` are closed, and all earlier TASK-003 findings except `FIND-TASK-003-9` remain closed. The TypeScript remediation stops one optional marker short of the exact validated public contract, so the cumulative candidate has not yet earned task acceptance.
