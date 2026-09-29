# TASK-003 R2 Task Implementation Review

## Immutable Subject

- Base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Cumulative candidate: `449aceb346f5f5fbc27958d260bd9c0c50225466`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- Prior review: `changes/active/verified-change-contract/review/TASK-003-r1/verdict.md`
- Remediation task: `changes/active/verified-change-contract/review/TASK-003-r1/TASK-003-R1-close-binding-activity-contract.md`
- Scope: complete original-base-to-cumulative-candidate range; prior implementation summaries were not treated as proof

## Proposed Findings

### Important

- **TREV-003-R2-001 — VIOLATION — the cumulative candidate fails its required whitespace gate while claiming that gate passed.**
  - **Violated obligation:** `AGENTS.md` section 12 requires format and targeted checks to pass; the original task and R1 remediation task both require `git diff --check`; a task-review `PASS` also requires credible verification of the complete immutable candidate.
  - **Location:** `changes/active/verified-change-contract/review/TASK-003-r1/standards-review.md:3-5,43-75`; the contradictory completion claim is at `changes/active/verified-change-contract/review/TASK-003-r1/TASK-003-R1-close-binding-activity-contract.md:283`.
  - **Evidence:** `git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..449aceb346f5f5fbc27958d260bd9c0c50225466` exits nonzero and reports trailing whitespace on the listed lines. The narrower remediation range `467ea07d94a5a6d665d24f56afc0bc0a12532304..449aceb346f5f5fbc27958d260bd9c0c50225466` fails identically. This directly contradicts the remediation artifact's statement that the original-base range exited zero.
  - **Observable consequence:** the cumulative task candidate does not satisfy its explicit completion gate, and the recorded verification evidence is not reproducible from the pinned commits.
  - **Testable correction:** remove only the trailing spaces from the preserved R1 standards report without changing its review content, then run the exact original-base-to-new-candidate `git diff --check` command and require exit zero.

No other material task-acceptance finding was identified. Ponytail rejects optional cleanup or refactoring beyond this required gate correction.

## Full Acceptance Matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `REQ-078`: the TASK-003 binding projection lives in the `wyrd` schema, is owned by `wyrd-sql`, uses forced RLS and `TenantConn`, and composes in the caller transaction | Migration `20260601000027_verification_bindings.sql:53-101`; `queries/verification.rs:251-301`; `cards/service.rs:1034-1107,1121-1172` | `projection_is_transactional_and_tenant_isolated`; reported `check:tenant-isolation`, `check:registry-tx-coupling`, and `check:from-pools-allowlist` | PASS |
| `REQ-095`, `REQ-102`: one effective binding is one subject occurrence plus exact Verifier, with duplicates refused | Natural key in migration lines 73-74; occurrence construction in `BindingProjector::freeze`; existing binding validation remains in the composition/spec path | Projection stability tests and registration-route duplicate-binding coverage inspected | PASS |
| `REQ-104`: `BindingId` is typed UUIDv7, server-minted, stable under reapply/reorder, and keyed by a total owner/component occurrence domain | `ids.rs::BindingId`; `OWNER_OCCURRENCE_KEY`; non-null migration column/check; `project_bindings`; reserved/duplicate alias validation | `binding_id_is_uuid7_backed` independently passed; `agent_owner_occurrence_is_the_reserved_key`, `projection_identity_is_stable_under_reapply_and_reorder`, and reserved-alias route coverage inspected | PASS |
| `REQ-104`: effective referenced Trigger/Operator identities and inline bodies are frozen by UID or canonical digest | `cards/service.rs:1220-1296`; migration trigger/operator columns and checks | Registration-route frozen-target assertions inspected | PASS |
| `REQ-105`, `REQ-106`: registration and credential creation do not activate; only successful exact-owner API-key and workload `jwt-bearer` exchanges stamp activity in the issuance transaction | `TenantGrant::records_owner_activity`; `TenantTokenIssuer::issue` lines 349-352; `record_machine_authentication` | API-key real-client journey, Keycloak workload journey, auth integration tests, and no-activity registration assertions inspected | PASS |
| `REQ-105`: delegation, OIDC login, human refresh, Card-free automation, cached bearer use, component scope, observation writes, and other Card versions do not activate | Closed grant gate in `issuance.rs`; Card-bound active-machine predicate in `RECORD_AUTHENTICATION_SQL`; no request/observation activity writer | `runtime_activity_follows_only_qualifying_exchanges`, `human_oidc_login_journey`, `non_qualifying_grants_never_touch_activity`, and `observe_run` coverage inspected; nonexistent TASK-004 SYSTEM issuance is correctly not fabricated | PASS |
| `REQ-106`: activity is monotonic under out-of-order exchanges | `GREATEST(last_authenticated_at, $2)` in `RECORD_AUTHENTICATION_SQL` | `out_of_order_exchanges_never_move_activity_backward` inspected | PASS |
| `REQ-107`, `REQ-108`: current principal/Card lifecycle and the default 86,400-second window gate new work; component bindings inherit owner activity; A/B owners are independent; replicas share one principal | `BINDING_ACTIVITY_SQL`; `InactivityTimeout`; exact owner join and live status predicates | SQL lifecycle/window tests and assembled activity journey inspected | PASS |
| `REQ-108`: first authentication arms only null schedule cursors to the next future boundary; renewal does not move the cursor or backfill missed work | `UNARMED_SCHEDULES_SQL`, `ARM_SCHEDULE_SQL`, and `BindingSchedule::next_after` | `first_exchange_arms_schedule_and_renewal_keeps_cursor` and assembled activity journey inspected | PASS |
| Invalid or impossible schedules fail before registration writes | `EffectiveSpecs::validate_binding` parses the effective schedule and requires `next_after(Utc::now())` | `unarmable_schedules_are_refused` independently passed; route zero-write assertions inspected | PASS |
| `REQ-112`: Card, Card-bound principal, resolved binding projection, and operation/audit state commit atomically | One `TenantConn` is opened in `write_registration`; `persist_node` writes Card, relationships, artifacts, principal, then projection before caller commit | Transaction rollback and audit-failure route tests inspected | PASS |
| `REQ-134`, binding portion of `AC-028`: Card GET exposes server-derived stable binding IDs without changing authored spec or creating a binding listing resource | `hydrate_card` overlays `Status.verification`; `owner_binding_ids` is used by all Card GET service paths | Raw HTTP, Rust SDK, Python SDK, TypeScript SDK, generated-schema, and served-OpenAPI assertions inspected | PASS |
| `REQ-145`, auth portion of `AC-030`: Operator-bearing registration additionally evaluates and audits `operators:invoke`; Operator-free registration requires only `cards:write`; denial leaves no registration writes | `register_card_http` lines 404-430; `dispatches_operators`; transaction-carried allowed audit events | `operator_bearing_registration_requires_operator_invoke` checks inline/reference allow, deny, audit cardinality, and rollback | PASS |
| Ambiguous partial CardRef principal resolution fails closed while exact refs preserve A/B and space identity | `SERVICE_ACCOUNT_BY_CARD_REF_SQL` fetches at most two; `service_account_by_card_ref` returns only one exact match | Same-name/two-space issuance and delegation journey, exact Keycloak workload path, and SQL query-contract test inspected | PASS |
| Durable identity APIs use existing domain newtypes and reject invalid stored binding/Card UUID versions | `record_machine_authentication(PrincipalId)`; typed `BindingActivity`; private raw row conversion through constructors | `non_v7_stored_identities_are_refused` inspected | PASS |
| New dependency-backed freeze/projection orchestration has one cohesive owner without speculative public abstraction | Private transaction-scoped `BindingProjector { conn }`; narrow SQL operations and pure digest helper remain separate | Source/caller inspection; reported lints | PASS |
| New and materially modified Rust items satisfy the repository rustdoc contract | Added SQL constants, structs, fields, helpers, and tests carry intent plus applicable `# Errors`/`# Panics` sections | Base-to-candidate documentation inspection; no new suppression found | PASS |
| First-class SDK journeys prove the public verification status contract | Existing Rust, Python, and TypeScript Card journey files now register bound owners, read status, validate UUIDv7 IDs, and assert reapply stability | Reported `test:wyrdstate:journey`, `py:test:wyrdstate:integration`, and `ts:test:integration`; source assertions inspected | PASS |
| Served OpenAPI proves `Card -> Status -> verification -> binding_ids` | Shared contract type plus `pg_openapi_contract.rs::resolve_schema` assertions | Exact served-document test source inspected | PASS |
| Prohibited scope remains excluded: no heartbeat, activation endpoint, idle timer, per-request/observation touch, activity table, Vala control store, scheduler, new SYSTEM mint path, second binding identity, or compatibility alias | Complete cumulative source diff contains none of these mechanisms | Diff inspection | PASS |
| Required verification is reproducible for the cumulative candidate | Remediation artifact claims all named lanes and original-base `git diff --check` passed | Two focused unit tests independently pass, but exact cumulative and remediation `git diff --check` commands fail | **FAIL (`TREV-003-R2-001`)** |

## Prior-Finding Closure

| Prior finding | Reassessment against cumulative candidate | Result |
|---|---|---|
| `FIND-TASK-003-1` | Route detects effective top-level Operator-bearing bindings, requires `operators:invoke`, records denial canonically, and carries allowed decisions into the registration transaction; focused route proof covers inline/reference/no-Operator cases | CLOSED |
| `FIND-TASK-003-2` | Shared CardRef lookup returns a principal only for one active match; same-name/version Cards in two spaces prove ambiguous refusal and exact resolution across issuance/delegation, while configured workload binding supplies exact space | CLOSED |
| `FIND-TASK-003-3` | Postgres uses `GREATEST` for monotonic activity and keeps cursor arming null-only; reverse-time regression proof exists | CLOSED |
| `FIND-TASK-003-4` | Effective schedule validation now requires a future occurrence before the write transaction; impossible-date registration proves zero writes | CLOSED |
| `FIND-TASK-003-5` | `$owner` is the non-null reserved owner occurrence, component aliases reject it, Agent rows are constrained to it, and projection identity tests cover the total domain | CLOSED |
| `FIND-TASK-003-6` | Public verification query boundaries use `BindingId`, `CardUid`, and `PrincipalId`; private raw rows validate UUIDv7 identities before return | CLOSED |
| `FIND-TASK-003-7` | The dependency-backed freeze/project workflow is owned by private transaction-scoped `BindingProjector`; no trait, factory, repository layer, or public abstraction was added | CLOSED |
| `FIND-TASK-003-8` | Existing real server/client evidence now covers API-key and workload JWT qualifying exchanges plus the reachable exclusions, lifecycle gates, A/B versions, shared replicas, stale-token renewal, and observation no-touch; nonexistent SYSTEM issuance remains outside this task | CLOSED |
| `FIND-TASK-003-9` | Rust, Python, and TypeScript Card journeys directly observe stable UUIDv7 binding IDs through their public Card envelopes | CLOSED |
| `FIND-TASK-003-10` | The served OpenAPI test walks the named schema-reference chain and asserts the binding-ID array's UUID shape | CLOSED |
| `FIND-TASK-003-11` | Added/materially modified Rust items in the cumulative implementation and tests now carry the required substantive documentation | CLOSED |

## Verification Notes

- `.codegraph/` is absent, so repository inspection used the immutable Git range and direct source reads.
- Independently executed and passed:
  - `mise exec -- cargo test --locked -p wyrd-spec --lib foundation_contracts_tests::binding_id_is_uuid7_backed -- --exact`
  - `mise exec -- cargo test --locked -p wyrd-sql --lib queries::verification::tests::unarmable_schedules_are_refused -- --exact`
- Independently executed and failed:
  - `git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..449aceb346f5f5fbc27958d260bd9c0c50225466`
  - `git diff --check 467ea07d94a5a6d665d24f56afc0bc0a12532304..449aceb346f5f5fbc27958d260bd9c0c50225466`
- The remaining broad Postgres, Keycloak, SDK, codegen, lint, and journey lanes were inspected through their source assertions and recorded command evidence but were not independently rerun in this Wave-1 review.

## Overall Result

**FAIL**

All eleven prior material findings are closed in implementation and focused proof, but the cumulative candidate fails an explicit required verification command and contains a contradictory pass claim. The correction is bounded to whitespace in the preserved review artifact and requires no specification or architecture decision.
