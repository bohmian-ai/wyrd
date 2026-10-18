---
id: TASK-002-R1
kind: remediation
status: ready
spec: SPEC-long-horizon-verification
spec_revision: 13
requirements: [REQ-012, AC-007, INV-001, INV-002, INV-003, INV-004, INV-005]
parent_task: TASK-002
remediates: [FIND-TASK-002-1, FIND-TASK-002-2]
---

# Complete decoder documentation and selector-free activation guidance

Execute with `$wyrd-implement`. This is a bounded correction under approved behavior, not a new design plan. Do not implement it during review.

## Subject and authority

- Approved scope: `changes/active/long-horizon-verification/spec.md`, owner-approved revision 13, REQ-012, AC-007, INV-001–005.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-002-scheduled-binding-simplification.md`.
- Reviewed base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Reviewed candidate: `5c421663b43d7deeca135134d6b07cf49a11b83f`, branch `change/long-horizon-verification-task-002`.
- Prior verdict: `verdict.md` in this directory; independently validated ledger: `findings-validation.md`.
- Both commits contain a revision-14 draft spec. Apply only the explicitly approved revision-13 task scope; neither this review nor this remediation approves draft additions. No correction requires those additions.

## Diagnosis and intended outcome

### FIND-TASK-002-1 — VIOLATION: shared decoder rustdoc gate is incomplete

At `crates/wyrd-spec/src/envelope.rs:752–759`, new `decode` returns Result without a `# Errors` section and materially rewritten `spec_from_kind_value` has no rustdoc. AGENTS.md §16 expressly makes these omissions blocking, including private functions. These are live shared decoder functions: `Spec::from_kind_and_value` and `Card::deserialize` call the dispatcher, which calls the generic decoder for all fourteen native Card kinds. Loader parsing, inline references, composition, SQL Card decoding and server registration consume the same boundary.

The source correctly supplies field-aware errors, but its new contract is undocumented at the required owner. Passing compilation, codegen and the removed-field test cannot prove this documentation gate.

Document the two existing functions in place. Explain typed decoding and path-bearing failures; explain kind dispatch, the existing top-level `type` preprocessing and refusal of External. Both fallible functions need actual Errors conditions. Preserve every function body and current input/error behavior. Deleting the path-aware decoder would remove useful required diagnostics; another parser or abstraction is unnecessary. This documentation-only correction uses static proof rather than manufactured RED.

### FIND-TASK-002-2 — REGRESSION: activation refusal prescribes invalid authoring

At `crates/wyrd-spec/src/error.rs:2076`, the derive metadata for `SpecTriggerActivationMismatch` still instructs authors to map samples slots in binding inputs. The candidate removed `VerificationBinding.inputs` under deny_unknown_fields and proves refusal of empty and exact maps. `check_activation` at server `components/cards/resolve.rs:37` still emits the mismatch when a schedule is bound to a Verifier without samples. Its existing HTTP test, `pg_card_registration_route::referenced_binding_refusals_leave_no_writes`, exercises that path.

The error derive produces remediation(), `WyrdError::as_problem_json` includes it, and the server returns it on the public HTTP path. MCP/error projections share that metadata. The API reference at `docs/src/content/docs/api/errors.md:351` is another projection of the same producer. A caller following the emitted instruction adds a forbidden field and receives another registration rejection. Existing no-write/code assertions and green codegen cannot detect wrong source guidance.

Correct the existing authoritative remediation text to direct callers to a recorded-samples Verifier whose declarations supply measurements, removing the binding-input instruction. Preserve its code, title, status, activation refusal, field details and verify/submit advice for other evidence. Reuse the derive metadata and existing HTTP test; do not add downstream guards, aliases, replacement validators or a new harness. Regenerate API error documentation through `mise run docs:generate` (the existing generate_api_docs.py owner); `codegen:regen` alone does not regenerate this page. Include only generated changes attributable to the metadata correction.

## Ordered behavior and proof

For FIND-TASK-002-2, extend the existing activation-mismatch branch of `referenced_binding_refusals_leave_no_writes` to inspect the emitted remediation for guidance compatible with selector-free samples authoring. Retain its exact error-code and no-write assertions. Run that focused test before the metadata correction to establish the expected stale-guidance RED, then update metadata and rerun GREEN. No new parser, error family or journey is required. Follow AGENTS rustdoc requirements for any materially changed Rust test item.

For FIND-TASK-002-1, inspect both complete function bodies and direct callers, add substantive documentation and verify it statically. No new runtime test is needed for rustdoc alone. Regenerate documentation from the corrected source and run the bounded lanes below.

## Preserved behavior and non-goals

Keep the all-drift-requirement iterator, exact Verifier UID/subject/window authority, missing-data inconclusive behavior, ordinary JSON/media/trace optionality, baseline/scoring algorithms, submitted inputs, Operator delivery, transaction/RLS/lease fences and outbox semantics unchanged. No grouping, admission, benchmark, compatibility, persistent schema or rollout decision belongs here. Do not edit spec approval/revision metadata, resurrect selector maps, add optional-samples behavior, or clean up unrelated internal reference prose. Do not manufacture the original implementation's missing RED history.

Likely owning surfaces are the two envelope function doc blocks, existing WyrdError metadata, existing activation-mismatch HTTP test and generated error-reference projections. This is guidance about correction ownership, not a private helper or file allowlist.

## Acceptance criteria

| Finding | Required result | Closure proof |
|---|---|---|
| FIND-TASK-002-1 | Both shared decoder functions have substantive operation rustdoc and correct Errors sections; bodies unchanged | Compare docs with complete bodies and direct callers; formatting check |
| FIND-TASK-002-2 | The reachable activation-mismatch problem directs accepted samples authoring and never prescribes binding inputs; existing refusal/code/no-write semantics remain | Existing HTTP branch asserts remediation and passes focused RED/GREEN; generated API error row agrees with source |
| Both | No adjacent behavior, error identity, schema, optionality or lifecycle changes | Bounded diff inspection and scoped checks below |

## Exact focused commands and narrow lanes

```bash
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_card_registration_route::referenced_binding_refusals_leave_no_writes)"'
mise run docs:generate
mise run fmt:check
mise run lints
mise run codegen:check
mise run docs:check
```

Confirm the existing exact test selector before recording its result. The wrapper owns Postgres and migrations; log any environment failure honestly. These are the only prescribed runtime regression test and static lanes for this bounded write set. No full user-journey suite, full principals suite or `mise run gate` is required.

If implementation nevertheless changes decoder bodies, explain why and retain the existing contract proof:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::verifier_card_tests::removed_scheduled_samples_fields_fail_at_their_path)'
```

Append source locations, exact RED/GREEN results, selected counts, static-check results and a diff/non-goal audit. Return implementation evidence to a fresh immutable cumulative TASK-002 review; implementation does not grant PASS, commit or merge authorization.

## Implementation Evidence

Commit: `dfbcc946b` on top of reviewed candidate `5c421663b`.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-002-1 | Rustdoc with `# Errors` on `decode` and `spec_from_kind_value` (`crates/wyrd-spec/src/envelope.rs`); bodies unchanged | `mise run fmt:check`, `mise run lints`; `wyrd-spec` 760/760 | PASS |
| FIND-TASK-002-2 | `SpecTriggerActivationMismatch` remediation now reads "Attach a schedule Trigger only to a Verifier whose samples slots declare the recorded features it judges; supply other evidence through verify or submit." (`crates/wyrd-spec/src/error.rs`); code/title/status unchanged; `docs/src/content/docs/api/errors.md` regenerated via `mise run docs:generate` (one row changed) | `referenced_binding_refusals_leave_no_writes` asserts the HTTP problem's `remediation` mentions samples and not binding inputs, keeping its code and no-write assertions | PASS |
| Both | No behavior, error identity, schema or lifecycle change | `mise run codegen:check`, `mise run docs:check`, `git diff --check`; diff touches only the 4 files above | PASS |

RED (before the metadata change), 1 run, 1 failed with the stale text
`"...and map its samples slots in the binding inputs; ..."`; GREEN after, 1 passed:

```bash
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_card_registration_route::referenced_binding_refusals_leave_no_writes)"'
```

`docs:check` compares generated docs against git, so it passes only after the
regenerated page is committed; it was rerun green after `dfbcc946b`.

Non-goals: no decoder body, parser, error family, alias, optional-samples,
grouping, outbox or spec metadata changes.

Return: `IMPLEMENTED`.
