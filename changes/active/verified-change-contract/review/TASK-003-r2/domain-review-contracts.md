# TASK-003 R2 Domain Review: Contracts and SDK Surfaces

## Immutable Subject and Review Boundary

- Original base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Candidate: `449aceb346f5f5fbc27958d260bd9c0c50225466`
- Approved specification: revision 33 of `changes/active/verified-change-contract/spec.md`
- Original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- Reviewed boundary: cumulative public `BindingId` and Card-status contracts; binding occurrence composition and projection ownership; all Card read hydration paths; generated Card/GetCard schemas; served runtime OpenAPI; and Rust, Python, and TypeScript Card journey surfaces. Authentication, SQL durability, and authorization were inspected only where they directly support those public contracts.
- CodeGraph was unavailable because `.codegraph/` is absent.

## Authority and Source Coverage

| Area | Authority | Source and consumer coverage |
|---|---|---|
| Binding and occurrence identity | `REQ-095`, `REQ-102`, `REQ-104`, `REQ-112`, `INV-001`; TASK-003 scenarios 1 and 5 | `wyrd-spec/src/{ids.rs,card/verifier.rs,graph/composition.rs}`; migration 27; `wyrd-sql/src/queries/verification.rs`; `cards/service.rs`; projection and composition tests |
| Card status and reads | `REQ-134`, binding portion of `AC-028`; TASK-003 scenario 4; Wyrd doctrine public-surface rules | `wyrd-spec/src/envelope.rs`; all three Card read paths and `hydrate_card`; shared Rust `Cards` consumer; generated Card/GetCard schemas |
| Runtime OpenAPI | `AGENTS.md` contract rules; testing-workflows runtime-OpenAPI rule | `pg_openapi_contract.rs`; served `Card -> Status -> VerificationStatus -> binding_ids` component chain |
| First-class SDK journeys | Wyrd design client model; doctrine public surfaces; `AGENTS.md` test taxonomy; `AC-020`; R1 `FIND-TASK-003-9` | Rust `cards_state.rs`; Python `test_state_journey.py`; TypeScript `cards-state.test.ts`; public SDK Card types and projections |
| Prior remediation | R1 verdict, validation ledger, and remediation task | Relevant closure evidence for `FIND-TASK-003-4`, `-5`, `-6`, `-7`, `-9`, `-10`, and `-11` was checked against the cumulative candidate rather than accepted from the task report |

Repository authorities reviewed for this boundary were `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, the spec-driven-development, testing, and error references, specification revision 33, TASK-003, and the three required R1 review/remediation artifacts. The complete base-to-candidate file inventory and the cumulative diffs plus surrounding callers/consumers in this boundary were inspected.

## Verification Limits

- This was primarily a static, review-only audit. I ran `mise run codegen:check`; it passed and left the worktree clean.
- I did not rerun the Postgres-backed OpenAPI or Rust/Python/TypeScript journey lanes. Their final-candidate success is reported in the immutable task evidence, and their source assertions were inspected.
- `git diff --check base..candidate` does not reproduce the task report's claimed success: it reports trailing spaces in the R1 standards-review Markdown. That pre-existing review-artifact issue is outside this domain boundary and is not counted as the contract finding below, but the aggregate verification claim should not be treated as independently reproduced.

## Prior-Finding Closure

| Prior finding | R2 disposition | Evidence |
|---|---|---|
| `FIND-TASK-003-4` | VERIFIED CLOSED | `EffectiveSpecs::validate_binding` parses the effective schedule and calls `next_after(Utc::now())` before the registration transaction; the route test now uses the parseable, impossible `0 0 30 2 *` case and asserts no registration writes. |
| `FIND-TASK-003-5` | VERIFIED CLOSED | `OWNER_OCCURRENCE_KEY` supplies the total owner occurrence; migration 27 makes the column non-null and constrains Agent rows; composition rejects the reserved alias and duplicate component aliases; projection tests pin the persisted value and stable identity. |
| `FIND-TASK-003-6` | VERIFIED CLOSED | Public verification query boundaries now use `PrincipalId`, `BindingId`, and `CardUid`; raw SQL rows are private and invalid stored binding UUID versions are rejected through `stored_binding_id`. |
| `FIND-TASK-003-7` | VERIFIED CLOSED | `BindingProjector` is the single private transaction-scoped owner for freeze-and-project behavior, while the narrow SQL operations and pure digest/UID helpers remain direct. |
| `FIND-TASK-003-9` | PARTIALLY CLOSED / REOPENED | All three added journeys observe stable UUIDv7 values through public runtime objects. Rust uses the typed `Status`/`VerificationStatus` contract and Python uses its established mapping projection. The TypeScript journey, however, must locally cast an untyped unknown field because the exported `Card` contract still does not declare `status`; see `CONTRACT-R2-001`. |
| `FIND-TASK-003-10` | VERIFIED CLOSED | The assembled-server OpenAPI test now resolves and asserts `Card -> Status -> verification -> VerificationStatus -> binding_ids`, including array and UUID item shape. |
| `FIND-TASK-003-11` | VERIFIED CLOSED IN THIS BOUNDARY | Added or materially modified public-contract items inspected here carry substantive rustdoc, including error and panic behavior where applicable; no suppression was introduced in these files. This disposition does not substitute for the standards review's repository-wide documentation audit. |

## Material Findings

### Important

#### CONTRACT-R2-001 — TypeScript's public `Card` type omits the new verification status contract

- **Source-local ID:** `CONTRACT-R2-001`
- **Obligation:** `architecture/wyrd-doctrine.mdx` requires Rust, Python, and TypeScript to be first-class SDKs with generated/typed client projections of the same Card contract. R1 `FIND-TASK-003-9` specifically requires the TypeScript Card journey to observe `card.status.verification.binding_ids` through its public Card envelope, not through a test-only substitute.
- **Location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:892-905`; `sdks/wyrd-sdk-ts/wyrd/tests/integration/cards-state.test.ts:138-142,199-205`.
- **Evidence:** the exported `Card` interface declares only `apiVersion`, `kind`, `metadata`, and `spec`, then hides every other field behind `[key: string]: unknown`. The new journey cannot access `card.status`; its `servedBindingIds` helper casts `card.status` to a locally invented `{ verification?: { binding_ids?: unknown } }` shape. Consequently `ts:typecheck` verifies the cast, not the SDK's public status contract, and neither the field name nor its `string[]` value type is owned by the SDK API.
- **Consequence:** TypeScript users cannot discover or safely consume the only public locator for inline bindings without recreating the wire shape themselves. A rename, omission, or incompatible value change in the SDK projection can compile and leave this journey green because the test has opted out of the exported type.
- **Testable correction:** add the minimal public TypeScript status shape (`CardStatus` with optional `verification`, and `VerificationStatus` with `readonly binding_ids: readonly string[]`) and declare `Card.status?: CardStatus | null`. Remove the local cast and assert the same stable UUIDv7 values through `card.status?.verification?.binding_ids`; run `mise run ts:typecheck`, `mise run ts:test:integration`, and `mise run codegen:check`. No new client, endpoint, runtime conversion, or abstraction is needed.

### Suggestions

None.

## Overall Result

**FAIL**

The server-side Card composition/status contract, generated schemas, runtime OpenAPI proof, and Rust/Python journey observations are coherent, and the other relevant R1 findings are closed. The TypeScript journey currently bypasses rather than verifies the first-class SDK type for the new public status field, so `FIND-TASK-003-9` is not fully closed.
