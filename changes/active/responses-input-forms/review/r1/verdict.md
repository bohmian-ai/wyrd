# FIX_REQUIRED

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/responses-input-forms`.
- Base: `807c4fe68ddcf43f325e970ed15f8a7c9918e0d4`.
- Candidate: `6294bf6bbc0acb9abc37e4e4d617e2638f9dc3f7`.
- Approved specification and original task: `changes/active/responses-input-forms/spec.md`, revision 1. The user explicitly made this spec the task; no separate task packet is required.
- Complete range: `candidate.diff`. Initial worktree was clean; implementation and HEAD remained unchanged. Only review artifacts were added.
- Prior reviews/remediations: none; no prior finding closures to reconcile.

## Reconciled acceptance

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 easy input decodes as typed equivalent | Existing Skald input owner plus `easy_items` | Exact captured-body test passes | PASS |
| REQ-001 function output accepts text or parts and retains selected form | `OpenAiFunctionCallOutput` untagged enum; existing text writers use `From<String>` | Captured parts assertion and existing Skald tool-loop coverage pass | PASS |
| REQ-001 both forms reach provider unchanged | Gateway `typed_body` returns original JSON; native adapter changes model only | Independently traced by implementation, system and domain reviewers; real SDK gateway journey missing | Implementation PASS; required proof FAIL, FIND-1 |
| INV-001 tenant isolation | Authenticated Caller and existing tenant-scoped admission remain owners; parser has no tenant authority | Independent security-tenancy and system traces | PASS |
| AC-001 captured Agents SDK unit | `easy_input_messages_decode_as_typed_messages` | One exact test selected and passed | PASS |
| AC-002 format, lints, codegen and owning lane | Existing verification owners | Format check, both lint stages, snapshot codegen check, 345 Skald tests pass | PASS |
| Stronger repository user-journey obligation | Only added proof stops at decoder | Approved spec records missing journey without explicitly waiving AGENTS §11 | FAIL, FIND-1 |
| Machine-readable accepted input contract parity | Serde hook expands accepted input; Schemars still publishes canonical item only | Source/schema/declaration trace; green regeneration does not prove accepted-input completeness | FAIL, FIND-3 |
| Rustdoc on materially changed Python method | Text projection now concatenates parts and omits media; accessor has no rustdoc | Static source and authored/public stub trace | FAIL, FIND-2 |
| Bounded scope and non-goals | Existing wire/parser/consumer owners extended; no new transport, durable state, identity or lifecycle owner | Full cumulative diff independently reviewed | PASS |

## Independent review results

| Role | Report | Discovery result |
|---|---|---|
| Behavior | `task-review-behavior.md` | FAIL |
| Invariants | `task-review-invariants.md` | FAIL |
| Repository standards | `standards-review.md` | FAIL |
| Maintainer | `maintainer-review.md` | FAIL |
| System resilience | `system-review.md` | PASS |
| Security and tenancy | `domain-review-security-tenancy.md` | PASS |
| Fresh Ponytail validation | `findings-validation.md` | FIX_REQUIRED; all claims independently adjudicated |

Follow-up was not required: source claims do not conflict, and the unique schema claim concerns the already mapped input/contract boundary. See `claim-comparison.md`. System/domain PASS results concern runtime resilience and tenancy, not missing proof or schema parity. Earlier reports' pending lint/codegen assessments are superseded by `verification.md`; no verification failure is retained.

## Validated finding ledger

The detailed source evidence, producer-to-consumer traces, sibling consumers, selected corrections and focused proof are in `findings-validation.md` and are incorporated here.

| Stable ID | Discovery sources | Status / class | Location and consequence | Required correction |
|---|---|---|---|---|
| FIND-responses-input-forms-1 | BEH-001, INV-REV-001, STD-001 | REVISED / MISSING | New test at `openai_responses.rs:692`; existing TS gateway journey is Chat-only. Real agent easy request and tool continuation through gateway remain unproved. | Extend existing real-server TypeScript gateway inference journey with stock Agents SDK Responses tool loop, original upstream shapes, final answer and same-boundary refused caller. |
| FIND-responses-input-forms-2 | INV-REV-002, STD-002, MNT-001 | CONFIRMED / VIOLATION | `skald-prompt/src/python/openai_responses.rs:571`; changed public text projection lacks required rustdoc/Errors and accurate stub prose. | Document existing projection and error; update authored prompt stub and regenerate. Preserve return type and behavior. |
| FIND-responses-input-forms-3 | MNT-002 | REVISED / INCORRECT | `skald-spec/src/wire/openai_responses.rs:291`; schema and generated input declarations reject easy forms the decoder accepts. | Describe accepted easy alternatives at input-schema boundary, retaining canonical item/output types; regenerate and prove served OpenAPI parity. |

No finding requires a new product or expensive-to-reverse architecture decision. Rejected excess proof scope (unrelated malformed-body/all-language journeys) is excluded. The typed Skald object may normalize easy messages; only gateway raw forwarding must preserve them.

## Verification limits and disposition

`verification.md` and command logs record passing format, lints, codegen, exact unit and Skald lane. Codegen ran on a disposable git-archive source snapshot so the reviewed worktree remained immutable. No full gateway journeys or additional Python/TypeScript runtime/typecheck lanes were run in this audit; the missing new journey cannot be executed yet.

Verdict: **FIX_REQUIRED**. Route `responses-input-forms-R1-contract-and-journey.md` directly to `$wyrd-implement`. Review does not implement, merge, push or deploy.
