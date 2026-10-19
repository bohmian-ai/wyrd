# TASK-002 behavior review

Overall: **FAIL**. The scheduled-samples behavior satisfies the scoped task by source inspection; one explicit changed-Rust documentation gate remains unmet (BEH-001). No behavioral, tenancy, scoring, or durability defect was established.

## Subject and authority

Reviewed the complete `ef3a57b0776dd7bc451273b9176498fcca700550...5c421663b43d7deeca135134d6b07cf49a11b83f` range, all 54 changed files, independently of other reports. HEAD remained `5c421663b43d7deeca135134d6b07cf49a11b83f`; only the review directory was untracked. Original task: `changes/active/long-horizon-verification/tasks/TASK-002-scheduled-binding-simplification.md`. Scope is owner-approved revision 13 REQ-012/AC-007 and INV-001–005 as identified in subject.md. The checked-in spec is revision 14 draft; its new managed-Bifrost-table upgrade text is not necessary authority for this task's binding-column deletion. No approval of that draft is inferred.

Read the task-review skill, AGENTS.md, agent rules, spec-driven-development and maintainer references, applicable design/doctrine/Bifrost boundaries and the original task/specification. CodeGraph was the first code-navigation operation. Implementation Evidence was treated as unverified command claims, never as implementation proof.

## Caller-to-result coverage

- Authoring: `VerificationBinding` and the internally tagged `InputSlot` retain `deny_unknown_fields`; removal of binding inputs and samples required makes both obsolete authored fields unrepresentable. Other slot variants retain their prior required flags and `InputSlot::required` behavior.
- Shared decode: all Card kinds still dispatch to the same Rust spec types. `envelope.rs::decode` substitutes serde_path_to_error for serde_json's error formatting, preserving Serde validation. Both loader registration and raw HTTP server registration reach `Spec::from_kind_and_value`; no duplicate removed-field scanner or alternate registration transport was introduced.
- Registration: `components/cards/resolve.rs::EffectiveSpecs` resolves the effective Verifier/Trigger before schedule activation. `check_activation` retains the no-samples rejection. Deleting validate_schedule does not accidentally permit mixed samples/JSON/media/trace Verifiers: `Validator::validate_evidence_mode` already refuses those and mixed Run/Window checks. BindingProjector still pins exact Verifier/subject/Trigger/operator identities.
- Persistence/claim: NewBinding and project_bindings stop writing the retired column, and the additive migration drops it. CLAIM_SQL no longer rereads a mutable binding selector; it returns execution-owned subject/window/exact Verifier UID/version. ClaimedRow constructs `Scheduled { window }`. Existing TenantConn, lease fencing, settlement and submitted-input branches remain.
- Runtime: runner resolve caches by tenant/exact Verifier UID, fetching that Card on a miss. Dispatch uses the scheduled window and `DriftEngine::checks` now iterates all drift checks. Each score continues reading its existing method/profile and subject window; unscorable reports remain explicit requirements. Existing PSI/SPC/Custom algorithms, baseline lookup and infrastructure-error outcomes are unchanged. Generic verdict/Operator settlement remains unchanged.
- Consumers/proof: inspected the Rust/Python/TS scheduled journey additions and existing failed/passing/empty/SPC cases, shared test helpers, SQL claim/retry/reclaim tests, server runtime/operator/registration tests, loader/CLI/MCP consumers, authored fixtures, generated TypeScript projection, docs and architecture edits. Deletion of the optional-slot omission journey removes a deliberately superseded behavior; two-requirement journeys replace its important positive/negative scoring coverage.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| Binding has no inputs, rejecting empty and exact maps at authored location | `card/verifier/mod.rs::VerificationBinding`; `envelope.rs::decode` | Three SDK rejection cases; Rust also POSTs raw HTTP, asserts 400/public catalog code/path, then 404 resolve | PASS |
| Samples required:false refused, no alias or silently ignored field | `InputSlot::Samples` has only features and enum deny_unknown_fields | Unit removed_scheduled_samples_fields_fail_at_their_path; all three SDK negatives and Rust raw HTTP negative | PASS |
| Error identifies samples required field | Shared decode reports parent samples path plus `unknown field required`; no loss of which field was refused | Assertions require inputs.latency and unit additionally requires unknown field required | PASS |
| Optional JSON/media/trace unaffected | Their fields and required() branches preserved; window mode validation still rejects mixtures | Existing contract validation suite claimed; source regression comparison | PASS |
| Two slots/two Custom requirements yield failed/passed and overall failed for 120/0.01 | All drift checks iterated; unchanged score/decide owners; latency-error-budget fixture | New Rust/Python/TS two-requirement journey branches assert exact requirement maps and failed verdict | PASS |
| Sufficient 80 latency plus absent/insufficient error samples yield passed/inconclusive and overall inconclusive | Unscorable report remains supplied to DriftChecks for every requirement | Matching missing-error-rate branch in all three SDKs | PASS |
| Exact subject, Verifier and scheduled window retained | Execution-owned UID/version/window; tenant-keyed exact Verifier cache; observation window uses run.card_uid | Journeys assert subject, named Verifier and drift provenance; SQL freeze test checks exact UIDs/version/window | PASS |
| Retry/reclaim keeps requirement authority and window | CLAIM_SQL updates lifecycle only and returns same execution identities; no mutable binding selectors remain | Existing retries_back_off_then_exhaust_to_errored asserts same frozen window; reclaimed_lease_fences_the_stale_token covers same execution/new lease; all-check loop follows exact cached spec | PASS |
| Existing baseline/empty/scoring/operator behavior preserved | Only omission filter removed from DriftEngine; scoring implementations and generic settlement unchanged | Existing Rust/Python/TS failed/passing/empty/SPC journeys retained; server Operator/runtime and SQL tests updated mechanically | PASS |
| Rejected Card/binding does not persist or activate | Decode precedes registration and binding projection | Rust raw HTTP negatives followed by resolve_latest 404; Python/TS also assert 404 after local refusal | PASS |
| Contracts, schemas, fixtures, examples and docs closed | Removed source types; generated TS no longer includes ScheduledInput/binding inputs/samples required; checked changed fixtures/docs and sibling owners | codegen:check, docs:check, CLI/MCP tests recorded as passed by implementation; not rerun here | PASS |
| INV-001/002/003/004/005; no group/outbox/admission/benchmark changes | Existing server/SQL/scoring owners extended; no new queue, buffer, runtime or SDK durable behavior; transaction/RLS and result settlement unchanged | Complete cumulative diff and relevant callers | PASS |
| New/materially changed Rust items carry substantive rustdoc including Errors | New decode has no Errors; materially rewritten spec_from_kind_value has no rustdoc | Direct source at envelope.rs:752–759 | FAIL |

## Proposed finding

### BEH-001 — VIOLATION — shared spec decoder misses mandatory Rust documentation

- Obligation: AGENTS.md §16 and architecture/agent-rules.md require rustdoc on every new/materially modified Rust item and `# Errors` for fallible functions. The task-review skill explicitly keeps missing rustdoc blocking even without a behavioral consequence.
- Location: `crates/wyrd-spec/src/envelope.rs:752–759` (`decode` and `spec_from_kind_value`).
- Evidence: new `decode<T>(...) -> Result<T, String>` has an intent paragraph but no Errors section; the rewritten `spec_from_kind_value(...) -> Result<Spec, String>` has no rustdoc at all. Every kind's decode now flows through these helpers, including loader and server callers above.
- Consequence: the mandatory changed-Rust documentation gate is incomplete; maintainers have no function contract documenting the decoder's refusal behavior and the dispatcher's existing normalization behavior. This is not a claim that malformed cards are accepted.
- Smallest testable correction: add substantive rustdoc to these existing helpers, including their actual error cases. Preserve existing parsing, validation, field formatting and compatibility behavior. Static inspection and the existing formatting lane are sufficient; no new test, abstraction or runtime change is needed.

## Non-blocking notes and limits

No extra behavioral change is recommended. The samples error combines a parent path with the named unknown field; together they identify the authored required field, so no bespoke decoder is warranted solely to concatenate a longer string. Retry/reclaim proof is composed from existing SQL lifecycle coverage and immutable exact-spec source flow, rather than a new two-check failure-injection journey. The original task records that RED was not separately executed; this report does not reinterpret source inspection as an executed RED run.

I did not execute suites, focused runtime tests, Postgres setup or checks; the orchestrator owns independent verification. Reported green command counts are implementation claims only. No production/test files were edited. The draft revision 14 schema decision remains outside this review's approval.
