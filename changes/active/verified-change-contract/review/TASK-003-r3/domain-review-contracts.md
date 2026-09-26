# TASK-003 R3 Domain Review: Public Contracts and SDK Surfaces

## Immutable Subject and Boundary

- Original base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Cumulative candidate: `a5a5b60f446981760ac831f64aba871582ec45e4`
- Approved specification: revision 33 of `changes/active/verified-change-contract/spec.md`
- Original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- Prior review/remediation: complete `TASK-003-r1` and `TASK-003-r2` verdicts, finding ledgers, domain reports, and remediation tasks
- Reviewed boundary: the authoritative Rust Card/status serde contract; generated Card and GetCardResponse JSON Schema; served OpenAPI projection; all server Card hydration paths; shared Rust client and state consumers; TypeScript `Card`/`CardStatus`/`VerificationStatus`, `Cards.get`, and `WyrdState.card`; and Rust, Python, and TypeScript binding-status journey proof.
- CodeGraph was unavailable because `.codegraph/` is absent.

The candidate was pinned to `a5a5b60f446981760ac831f64aba871582ec45e4` before inspection and remained at that commit after review.

## Authority and Source Coverage

| Area | Authority | Source and consumer coverage | Result |
|---|---|---|---|
| Card verification status | Spec `REQ-134`, `AC-028`; TASK-003 scenario 4; Wyrd design status/public-surface doctrine | `crates/wyrd-spec/src/card/verifier.rs:196-210`; `crates/wyrd-spec/src/envelope.rs:127-146,459-477,729-760` | PASS |
| Generated and runtime schemas | `AGENTS.md` contract/codegen rules; testing-workflows OpenAPI rule | `crates/wyrd-spec/schemas/{card,get_card_response}.json`; matching test goldens; `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:483-544` | PASS |
| Server hydration and read producers | TASK-003 scenario 4; server-owned status rule | `get_card_by_uid`, `get_card_by_ref`, and `get_latest_card` in `crates/wyrd/wyrd-server/src/components/cards/service.rs:85-173`; their shared `hydrate_card` at `268-338` | PASS |
| Shared/public Card consumers | Wyrd client model; first-class SDK projection rules | `crates/shared/wyrd-client/src/cards/{handle.rs:274-293,reads/get.rs:12-60}`; `crates/shared/wyrd-client/src/state.rs:575-589`; TypeScript native Card bridge and public wrappers | PASS |
| TypeScript projection | TypeScript guide placement, declaration, and public-shape rules; R2 `FIND-TASK-003-9` correction boundary | `sdks/wyrd-sdk-ts/wyrd/src/index.ts:892-920,1000-1029,1382-1390`; package public export path | PASS |
| First-class SDK proof | Wyrd design doctrine 20; `AGENTS.md` section 11; testing-workflows three-tier rule; R1/R2 remediation | Rust `sdks/wyrd-sdk-rust/tests/cards_state.rs:82-115`; Python `sdks/wyrd-sdk-python/tests/integration/state/test_state_journey.py:352-376`; TypeScript `sdks/wyrd-sdk-ts/wyrd/tests/integration/cards-state.test.ts:193-201` | PASS |

Applicable repository authorities reviewed for this boundary were `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, the reference router, TypeScript guide, testing-workflows and error references, approved specification revision 33, TASK-003, and both prior review/remediation rounds. The complete original-base-to-candidate diff and the surrounding Card producers and consumers named above were inspected.

## Contract and Nullability Resolution

`binding_ids` **may be omitted when a `verification` object exists**. It is not nullable when present.

| Layer | Authoritative behavior | Candidate projection |
|---|---|---|
| Rust serde | `VerificationStatus.binding_ids` is `Vec<BindingId>` with `#[serde(default, skip_serializing_if = "Vec::is_empty")]` (`verifier.rs:205-209`). Missing deserializes to an empty vector; an empty vector serializes with the property omitted; JSON `null` is invalid for the vector. | TypeScript uses `readonly binding_ids?: readonly string[]` and does not admit `null`. Exact. |
| Card status | `Card.status` is optional and omitted for `None`; `Status.verification` is optional and omitted for `None` (`envelope.rs:143-145,471-476`). Both serde shapes also accept explicit `null`. | `Card.status?: CardStatus | null` and `CardStatus.verification?: VerificationStatus | null`. Exact. |
| Generated JSON Schema | `Status` requires only `phase`; `verification` is optional and nullable. `VerificationStatus` has no `required` array; `binding_ids`, when present, is an array of UUID strings (`card.json:10400-10437,10856-10869`; equivalent GetCardResponse definitions). | TypeScript optionality and value type match. Exact. |
| Current Card GET runtime | `hydrate_card` emits `verification` only when `binding_ids` is non-empty (`service.rs:328-333`). Therefore every verification object emitted by the current TASK-003 read path contains a present, non-empty `binding_ids`, while the broader serde/schema contract still permits omission (including future verification substate that has no binding IDs). | The journey observes the current non-empty runtime case without narrowing the broader public contract incorrectly. Exact. |

The R2 validation ledger proposed a required TypeScript `binding_ids`, but the subsequent R2 remediation evidence correctly records the implemented property as optional because the server omits an empty list. The approved Rust serde owner and both generated schemas settle that apparent conflict. Making the TypeScript property required would be stricter than the public wire contract, so the candidate's optional property is the correct closure.

## Prior-Finding Closure

| Prior finding | R3 disposition | Contract-boundary evidence |
|---|---|---|
| `FIND-TASK-003-4` | VERIFIED CLOSED | Pre-write schedule validation and its public refusal remain intact; R2/R3 changes do not alter it. |
| `FIND-TASK-003-5` | VERIFIED CLOSED | The non-null `$owner` occurrence and reserved-alias boundary remain the total binding identity domain. |
| `FIND-TASK-003-6` | VERIFIED CLOSED | Public verification query results continue to use `BindingId`/`CardUid`/`PrincipalId`; Card status exposes serialized typed `BindingId` values. |
| `FIND-TASK-003-7` | VERIFIED CLOSED | `BindingProjector` remains the transaction-scoped projection owner; no R2/R3 contract change displaced it. |
| `FIND-TASK-003-9` | **VERIFIED CLOSED** | `Card` now declares `status`; exported `CardStatus` mirrors the complete live status shape; exported `VerificationStatus` matches serde/schema optionality; `Cards.get` and `WyrdState.card` both return that public `Card`; the integration journey removed the local cast and reads `status?.verification?.binding_ids` directly. `ts:typecheck` now checks the public projection, and the real TypeScript SDK/server journey proves one stable UUIDv7 after reapply. Rust and Python journey evidence remains present and coherent. |
| `FIND-TASK-003-10` | VERIFIED CLOSED | The served-document test still walks `Card -> Status -> verification -> VerificationStatus -> binding_ids` and asserts array/UUID item shape. |
| `FIND-TASK-003-11` | VERIFIED CLOSED IN THIS BOUNDARY | Contract and Card-consumer items inspected here retain substantive documentation without new suppression. |
| `FIND-TASK-003-12` | VERIFIED CLOSED AS A REGRESSION SEAM | The R2 SQL correction removes the manual tenant predicate/bind while preserving exact-one lookup. It does not change Card/status wire behavior. |
| `FIND-TASK-003-13` | VERIFIED CLOSED | The cumulative `git diff --check` command exits zero; the R1 report changed only by removal of trailing whitespace. |

R1 findings `FIND-TASK-003-1` through `-3` and `-8` concern authorization, principal selection, monotonic activity, and runtime-activity proof rather than this domain's Card/status contract. The cumulative source inspected here introduces no public-contract regression to those prior closures; their full reassessment belongs to the corresponding R3 domain reviews.

## Verification Limits

- Ran `mise run ts:typecheck`: PASS. This rebuilt the public/native TypeScript package and checked direct typed access to the status projection.
- Ran `mise run ts:test:integration`: PASS, 5 files and 18 tests, including `cards-state.test.ts` and its stable binding-ID read/reapply assertion.
- Ran `mise run codegen:check`: PASS; generated Card/GetCardResponse schemas and language artifacts are drift-free.
- Ran `git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..a5a5b60f446981760ac831f64aba871582ec45e4`: PASS.
- I did not rerun the Rust SDK, Python SDK, or Postgres-backed served-OpenAPI lanes. Their candidate success is recorded in the task/remediation evidence, and their relevant source assertions and public paths were inspected. This is a residual execution-evidence limit, not a source finding.
- The OpenAPI test proves field reference and UUID-array shape but does not independently assert the `required` set of `VerificationStatus`. The generated schema and owning serde annotations directly establish optionality; no contradictory runtime schema evidence was found.

## Material Findings

None. No correctness, contract, schema, TypeScript projection, or first-class SDK journey defect remains in this reviewed boundary.

## Overall Result

**PASS**

The candidate closes `FIND-TASK-003-9` with the smallest existing public TypeScript projection and direct journey proof. Its optional `binding_ids` property is not a weakened correction: it exactly matches the authoritative Rust serde and generated schema contract, while the current server runtime continues to emit the property whenever it emits TASK-003 binding verification status.
