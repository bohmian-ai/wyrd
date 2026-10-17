# TASK-005-R3 — Enforce individual-file admission and complete getter rustdoc

Status: READY. Route directly to `$wyrd-implement`. This packages only independently validated FIND-TASK-005-14 and residual FIND-TASK-005-7.

## Subject and authority

Approved spec: `changes/active/verifier-building-blocks/spec.md`, revision 9.
Original task: `changes/active/verifier-building-blocks/tasks/TASK-005-server-integration.md`.
Reviewed base exclusive: `f2a726c90edbf8956c7b91c843379175ee20eac0`.
Reviewed candidate: `02848d737a621938ec0b5f817e1aa84d5f3df1fd`.
Prior candidate: `de69cb69b68eced72f7d760681d27253e013f8a1`.

Diagnosis and source validation: this directory's `findings-validation.md`, `followup-review.md` and `verdict.md`. Nothing shipped: no migration/history/catalog work. Both corrections fit existing approved behavior.

## FIND-TASK-005-14: diagnosis and selected correction

Approved Deployment limits (`spec.md:928–950`) independently cap one input file (`FILE_MAX`) and all original files (`FILES_MAX`); an authored media slot can only lower deployment bounds. REQ-002 keeps validation server-authoritative. Current runtime reference promises an oversized input is refused before blocks.

`crates/wyrd/wyrd-server/src/verification/hosted/evidence.rs:333–379::TenantEvidence::admit` resolves each original File declared size or selected authorized Artifact inventory size, checks Artifact slot constraints, and checked-adds into the combined limit. It never compares that size with `limits.file_max_bytes`. Engine conformity checks authored slot bounds, not deployment limits. The only individual deployment check is later in `stored:261–263`, reached if bytes are read.

Raw HTTP/MCP declarations need no preceding upload-plan call. Artifact registration independently supplies real inventory. Direct and queued engines both install this same TenantEvidence. Consequently a File above 20 MiB but below the default 100 MiB total, in an unconstrained media slot, can accompany passing assertions on another JSON input. The existing direct test uses 200 MiB and only proves combined refusal. A real 11-byte registered notes Artifact similarly passes metadata admission with valid configured individual/combined limits of 4/20 bytes and no authored lower cap. A consuming graph instead discovers a block-local failure, potentially after sibling work. Neither outcome is the required execution-level refusal.

**Outcome:** reject every original File or selected Artifact whose resolved size exceeds the current individual deployment ceiling before any block runs, including unused supplied media. Direct uses the existing 413; queued execution terminates `errored`/`input_invalid` without a result, verdict or failure reaction. Enqueue remains asynchronous and performs no new registry/body read or PostgreSQL write.

**Selected minimal boundary:** extend the existing TenantEvidence admission operation to apply the existing per-file bound to each resolved original size before combined accounting/drive, returning existing `EngineError::InputTooLarge`. This owner already has the authoritative size and deployment settings and is shared by all entry points. Do not patch SDKs, HTTP/MCP handlers or individual executors. Do not require upload planning for Artifacts or prefetch bodies. Retain `stored`'s guard: it owns byte-read safety and also protects Artifact code sources outside original input slots.

Preserve authorized/audited exact Artifact selection; optional lower slot bounds; overflow-safe whole-input totals counted once; digest/length/sniffing; archive/workspace/scratch restrictions; missing-required/inconclusive and optional-null semantics; retries/deadlines/cancellation; Busy propagation and queue attempt refund. No new endpoint, error, dependency, validator service, upload session, reservation, durability promise or contract change.

This is one new finding across File/Artifact and direct/queued/MCP. Prior FIND-10 slot conformity and FIND-11 combined accounting remain closed; do not reopen or duplicate them.

## FIND-TASK-005-7: diagnosis and selected correction

The materially changed `PyGoogleFunctionDeclaration::parameters` getter at `crates/skald/skald-prompt/src/python/google.rs:577–588` calls the retained `f`/`google_request` expect/index invariant chain, but has no item-level Panics contract. AGENTS §16 expressly requires that section whenever a panic remains possible. Constructor-derived indices and the immutable retained request preserve the invariant for supported callers; no reachable supported-input panic is alleged.

Add the short accurate Panics explanation for a broken internal request-variant/declaration-index invariant. Preserve the method body, schema precedence, null/None projection, errors, wrappers, registration and signatures. Do not add guards or runtime tests solely for prose, change untouched accessors or start a broad documentation sweep. The earlier enumerated documentation repairs remain closed. Batch this correction with FIND-14.

## Acceptance and focused proof

| Finding | Required evidence |
|---|---|
| FIND-14 | Existing direct fixture: conforming File above individual but below combined limit, no slot cap, otherwise passing no-read assertions -> existing 413 before work. Existing hosted fixture: real selected Artifact in that interval with consistently configured limits -> metadata-only refusal. Existing queued fixture: individual-only excess -> terminal input_invalid on first attempt, no result/verdict/Operator; within-limit control completes. |
| FIND-14 preservation | Keep existing schema/kind/format/slot/combined, duplicate-declaration and repeated-read controls. No large allocation or live provider needed. Shared MCP owner needs no separate implementation. |
| FIND-7 | Read changed item and invariant chain; accurate local Panics contract present and body unchanged. |

Use the existing fixtures and record RED/GREEN for the added branches. Run named tests with their exact selections and managed environment (tracing on when diagnosing failures):

```bash
mise run build:monty-worker
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 --run-ignored=all -E 'test(=pg_verification_routes::direct_execution_refuses_nonconforming_evidence)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 --run-ignored=all -E 'test(=pg_hosted_artifacts::artifact_evidence_is_admitted_against_its_slot_and_the_combined_limit)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 --run-ignored=all -E 'test(=pg_hosted_artifacts::nonconforming_queued_evidence_errors_without_a_verdict)'"
```

If a new test name is chosen, record and run its actual exact selection too. After focused GREENs, run only the affected existing regression slice and required Rust hygiene:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 --run-ignored=all -E 'test(/^pg_verification_routes::/) or test(/^pg_hosted_artifacts::/)'"
mise run fmt
mise run lints
```

The Postgres setup applies the existing test schema; it is not product upgrade work. This minimal write set changes no SQL, dependency, generated/public contract or docsite content. No new tenant/dependency/codegen/docs lane, full gate, whole-language journey suite, live sweep or external-example rebuild is required for this remediation. Final change review owns integrated qualification. Leave user edits and unrelated untracked reports untouched; independent task review alone can accept completion.

## Implementation evidence

Owner reused: `TenantEvidence::admit` already resolves every original File/Artifact size with the Deployment limits; one `file_max_bytes` comparison per resolved size runs after slot checks and before combined accounting, returning the existing `EngineError::InputTooLarge` (direct 413, queued `input_invalid`). `stored`'s guard is kept.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-14 direct: unused 21 MiB File, uncapped slot, below 100 MiB combined -> 413 before work | `evidence.rs` `TenantEvidence::admit`; new case in `pg_verification_routes.rs::direct_execution_refuses_nonconforming_evidence` | RED `(200, None)`; GREEN exact selector | PASS |
| FIND-14 hosted Artifact: 11-byte notes on a 4/20-byte engine -> metadata-only `InputTooLarge` | `pg_hosted_artifacts.rs::artifact_evidence_is_admitted_against_its_slot_and_the_combined_limit` (cramped pool) | RED `Ok(Inconclusive …)` block-local failure; GREEN exact selector | PASS |
| FIND-14 queued: one 15-byte File past a 12-byte individual, within 20-byte combined -> `errored`/`input_invalid`, attempt 1, no result; combined case (11+11) and valid control kept | `pg_hosted_artifacts.rs::nonconforming_queued_evidence_errors_without_a_verdict`; `tight_files` individual bound 12 | RED `completed`; GREEN exact selector | PASS |
| FIND-14 preservation: slot/format/combined/duplicate/repeated-read controls | unchanged cases in the same tests | `test(/^pg_verification_routes::/) or test(/^pg_hosted_artifacts::/)`: 18/18 | PASS |
| FIND-7 `parameters` Panics contract | `skald-prompt/src/python/google.rs` rustdoc only; body unchanged | read of `f`/`google_request` invariant chain | PASS |
| Hygiene | — | `mise run fmt`, `mise run lints`, `git diff --check` | PASS |

Commands: `mise run build:monty-worker`; the three task commands (with `WYRD_LOG=info,wyrd_server=debug`) and the regression slice under `scripts/postgres/with-test-postgres.sh`. No SQL, dependency, contract, generated, docsite, SDK, handler or executor change. The unrelated working-tree edit to `changes/active/verifier-building-blocks/spec.md` was left untouched and uncommitted.
