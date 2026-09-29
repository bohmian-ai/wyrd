# TASK-003 R2 Wave-2 Findings Validation

## Immutable Subject

- Original base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Cumulative candidate: `449aceb346f5f5fbc27958d260bd9c0c50225466`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- Prior review: `changes/active/verified-change-contract/review/TASK-003-r1/`
- Candidate `HEAD` before validation: `449aceb346f5f5fbc27958d260bd9c0c50225466`
- CodeGraph: unavailable because `.codegraph/` is absent

The complete original-base-to-cumulative-candidate diff, applicable repository
authorities, prior R1 verdict/ledger/remediation, and all five R2 Wave-1 reports
were inspected. Every proposed correction was checked against the full changed
body, every caller or consumer, actual reachability, and the existing owner or
mechanism. Optional cleanup and speculative hardening were rejected.

## Wave-1 Finding Dispositions

| Wave-1 source ID | Disposition | Validated result |
|---|---|---|
| `TREV-003-R2-001` | REVISED | Deduplicated with `STD-003-R2-002` into new `FIND-TASK-003-13`. The correction is only removal of the committed trailing spaces; the existing evidence becomes reproducible against the corrected cumulative candidate without another mechanism. |
| `STD-003-R2-001` | CONFIRMED | Retained as new `FIND-TASK-003-12`. The task and repository rules expressly prohibit this manual tenant predicate, and the existing `TenantConn` RLS boundary already supplies tenancy. |
| `STD-003-R2-002` | REVISED | Deduplicated with `TREV-003-R2-001` into `FIND-TASK-003-13`; both reports identify the same 28 whitespace diagnostics and the same failed command. |
| `CONTRACT-R2-001` | REVISED | Retained under prior `FIND-TASK-003-9`, not assigned a new ID. The R1 closure requirement included the typed TypeScript public Card envelope; the local test-only cast bypasses it. The correction must project the existing complete `Status` shape rather than introduce a partial status abstraction. |

The security/tenancy and data/durability reports proposed no source-local
findings. Their PASS results do not contradict the retained standards,
verification, or TypeScript contract findings.

## Prior-Finding Closure

| Prior finding | R2 reassessment | Result |
|---|---|---|
| `FIND-TASK-003-1` | `register_card_http` detects effective Operator-bearing bindings, evaluates `operators:invoke` once in addition to `cards:write`, records denials through the canonical audit owner, and carries allowed events into the registration transaction. The focused route proof covers no-Operator, inline, referenced, allowed, denied, rollback, and audit-cardinality cases. | CLOSED |
| `FIND-TASK-003-2` | The one shared CardRef lookup fetches at most two active matches and accepts exactly one. API-key issuance, workload `jwt-bearer`, CardRef delegation, and the test credential helper all retain this shared owner; explicit references resolve and ambiguous partial references follow existing refusal paths. | CLOSED; the retained manual-tenant-predicate violation is separate and does not reopen ambiguity handling. |
| `FIND-TASK-003-3` | The activity update uses Postgres `GREATEST(last_authenticated_at, $2)`, while cursor arming remains null-only. Reverse-time proof covers the stored timestamp, activity window, and cursor stability. | CLOSED |
| `FIND-TASK-003-4` | Pre-write effective-binding validation parses the schedule and requires `next_after(Utc::now())`; the impossible-date route case proves refusal before registration writes. | CLOSED |
| `FIND-TASK-003-5` | `$owner` is the non-null reserved owner occurrence, component aliases reject it, Agent rows are constrained to it, and projection tests cover stable identity across the total owner/component domain. | CLOSED |
| `FIND-TASK-003-6` | Public verification query boundaries use `PrincipalId`, `BindingId`, and `CardUid`; private raw rows validate UUIDv7 binding/Card identities before return. | CLOSED |
| `FIND-TASK-003-7` | Private transaction-scoped `BindingProjector` owns freeze/projection; narrow SQL operations and pure digest/UID helpers remain direct, avoiding the rejected repository/factory/trait layers. | CLOSED |
| `FIND-TASK-003-8` | Existing real boundaries now cover API-key and workload JWT qualifying exchanges, reachable exclusions, stale-token re-exchange, lifecycle and inactivity gates, A/B versions, shared replicas, and observation no-touch. The nonexistent SYSTEM path remains correctly excluded. | CLOSED |
| `FIND-TASK-003-9` | Rust uses the typed Card status contract and Python uses its established public mapping projection. TypeScript receives the correct runtime JSON, but its exported `Card` omits `status`; the journey locally casts an `unknown` field, so TypeScript type-checking cannot prove `status.verification.binding_ids`. | **REOPENED** as retained prior finding; see the ledger below. |
| `FIND-TASK-003-10` | The served OpenAPI test resolves `Card -> Status -> verification -> VerificationStatus -> binding_ids` and asserts the UUID-array item shape. | CLOSED |
| `FIND-TASK-003-11` | Added and materially modified Rust items in the cumulative candidate carry substantive rustdoc and applicable `# Errors`/`# Panics` documentation without suppression. | CLOSED |

## Validated Finding Ledger

### FIND-TASK-003-9

- **Wave-1 IDs:** prior `FIND-TASK-003-9`, `CONTRACT-R2-001`
- **Status:** REVISED / REOPENED
- **Classification:** MISSING VERIFICATION / CONTRACT PROJECTION
- **Obligation:** TASK-003 scenario 4, the R1 remediation acceptance criterion for `FIND-TASK-003-9`, Wyrd design client doctrine, and the TypeScript guide require the first-class TypeScript SDK to project and prove the same typed Card contract, including `card.status.verification.binding_ids`.
- **Exact location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:892-905,1011-1014`; `sdks/wyrd-sdk-ts/wyrd/tests/integration/cards-state.test.ts:9,138-142,199-205`.
- **Caller/consumer and reachability evidence:** `Cards.get` is the public network read and returns `Promise<Card>` after `lifecycleValue<Card>` parses the native JSON. `WyrdState.card` is the other public `Card` producer. Repository search finds one exported `Card` declaration and no public `CardStatus` or `VerificationStatus` projection. The real TypeScript journey reaches `Cards.get`, but `servedBindingIds` casts `card.status` from the interface's `[key: string]: unknown` to a locally invented shape. Thus the runtime path is real while the public type and `ts:typecheck` proof are absent.
- **Observable consequence:** TypeScript users cannot discover or safely access the only public locator for inline bindings. The journey can continue compiling after the SDK omits or mis-types the field because its test-local cast asserts the desired shape instead of checking the exported contract.
- **Decision-complete minimal correction:** project the existing Rust/schema contract on the exported TypeScript Card type: one public `CardStatus` matching the already-live `phase`, nullable/optional `message`, nullable/optional `updated_at`, and nullable/optional `verification`; one public `VerificationStatus` with readonly `binding_ids: readonly string[]`; and `Card.status?: CardStatus | null`. Delete `servedBindingIds` and access `card.status?.verification?.binding_ids` directly in the existing journey. Do not add runtime conversion, validation, a client, endpoint, or status engine.
- **Existing mechanism to reuse:** the current hand-authored public `Card` projection, `Cards.get`, `lifecycleValue`, and the generated Rust/OpenAPI/JSON-schema field names. No new dependency or generator is warranted for this bounded correction.
- **Adjacent behavior preserved:** generic `spec`/metadata openness, native JSON transport, `WyrdState.card`, authored-spec immutability, stable UUIDv7 values, registration/reapply behavior, and existing error projection remain unchanged.
- **Focused closure proof:** the existing TypeScript Card journey reads the values directly through the exported type and proves UUIDv7 shape and reapply stability; `mise run ts:typecheck`, `mise run ts:test:integration`, and `mise run codegen:check` pass. A separate test or harness is unnecessary.

### FIND-TASK-003-12

- **Wave-1 IDs:** `STD-003-R2-001`
- **Status:** CONFIRMED
- **Classification:** VIOLATION / TENANCY BOUNDARY
- **Obligation:** TASK-003 lines 29-31 and 121-132, `architecture/agent-rules.md`, and `rust-core.md` prohibit manual tenant predicates on `TenantConn` queries and require Postgres forced RLS through `wyrd.current_tenant()` to remain the sole tenant authority.
- **Exact location:** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:26-34,170-201`, especially SQL line 29 and the first bind at line 191.
- **Caller/consumer and reachability evidence:** the complete lookup body accepts `&mut TenantConn`, executes on that transaction, and is reached by `IssueApiKey::execute` (`issue_api_key.rs:86-137`), workload `bound_service_account` (`jwt_bearer.rs:149-159`), CardRef delegation resolution (`exchange_api_key.rs:350-366`), and `WyrdTestServer::credential_registered_service` (`wyrd-testing/src/server.rs:2518-2574`). The migration-enforced table RLS is active on these production paths. The remediation materially changed this same SQL and function but retained `data_tenant_id = $1` plus `.bind(conn.data_tenant_id().as_uuid())`, exactly the duplicated authority the task requires the refactor to delete.
- **Observable consequence:** this reachable credential-selection path carries two tenant authorities. That is repository-contract drift and leaves a redundant bind/predicate that can diverge from the transaction-bound RLS policy.
- **Decision-complete minimal correction:** in the existing `SERVICE_ACCOUNT_BY_CARD_REF_SQL`, delete only `data_tenant_id = $1`, renumber `principal_kind` and `card_ref` to `$1`/`$2`, and delete the tenant bind in `service_account_by_card_ref`. Preserve `status = 'active'`, containment lookup, `LIMIT 2`, and the exact-one fail-closed result. No caller changes or new query API are needed.
- **Existing mechanism to reuse:** the existing `TenantConn` transaction and forced RLS policy on `wyrd.auth_service_accounts` already enforce tenant isolation.
- **Adjacent behavior preserved:** explicit/UID CardRef resolution, ambiguous partial-ref refusal, active-principal filtering, API-key issuance, workload assertion verification, delegation attenuation, caller-owned transaction lifecycle, and all audit behavior remain unchanged.
- **Focused closure proof:** strengthen the existing `service_account_by_card_ref_uses_jsonb_card_ref_binding` SQL-text test to assert `$1` principal kind, `$2` CardRef, and absence of `data_tenant_id`; run its exact selector plus `mise run test:principals:unit`, `mise run test:principals:integration`, `mise run test:sql`, and `mise run check:tenant-isolation`. No new test file or fixture is warranted.

### FIND-TASK-003-13

- **Wave-1 IDs:** `TREV-003-R2-001`, `STD-003-R2-002`
- **Status:** REVISED
- **Classification:** VIOLATION / COMPLETION EVIDENCE
- **Obligation:** TASK-003's verification list, the R1 remediation proof list, `AGENTS.md` sections 11-12, and the spec-driven workflow require the cumulative `git diff --check` gate to pass and recorded completion evidence to be reproducible.
- **Exact location:** `changes/active/verified-change-contract/review/TASK-003-r1/standards-review.md:3-5,43-47,50-54,57-61,64-68,71-75`; recorded pass claim at `changes/active/verified-change-contract/review/TASK-003-r1/TASK-003-R1-close-binding-activity-contract.md:276-286`.
- **Caller/consumer and reachability evidence:** exact execution of `git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..449aceb346f5f5fbc27958d260bd9c0c50225466` exits 2 with 28 trailing-whitespace diagnostics, all in the committed R1 standards report. The remediation-only range fails on the same bytes. This is the exact command required by the task and claimed green in the durable remediation record, so the failure is reachable and not a dormant source-format preference.
- **Observable consequence:** the immutable cumulative candidate fails an explicit completion gate, and one recorded verification claim cannot be reproduced.
- **Decision-complete minimal correction:** remove only the reported trailing spaces from the preserved R1 standards report without changing its words or conclusions, then rerun the exact original-base-to-new-candidate command. No production, test, or review-logic change is required.
- **Existing mechanism to reuse:** Git's existing `diff --check` gate. Ordinary Markdown paragraphs and blank-line separation preserve the report without trailing-space hard breaks.
- **Adjacent behavior preserved:** every R1 finding, verdict, remediation decision, source line, and implementation behavior remains unchanged.
- **Focused closure proof:** `git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..<new-candidate>` exits zero. No additional test is credible or necessary for a whitespace-only correction.

## Specification Revision Decision

`SPEC_REVISION_REQUIRED` is not indicated. All three corrections are fixed by
approved revision 33 and existing repository authority. They add no product,
public protocol, architecture, security model, compatibility policy,
concurrency semantic, resource owner, or persistent-data decision.
