# TASK-002 verdict: FIX_REQUIRED

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-002`.
- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Candidate: `5c421663b43d7deeca135134d6b07cf49a11b83f` on `change/long-horizon-verification-task-002`.
- Reviewed complete cumulative range: `git diff ef3a57b0776dd7bc451273b9176498fcca700550...5c421663b43d7deeca135134d6b07cf49a11b83f` (54 files).
- Original task: `changes/active/long-horizon-verification/tasks/TASK-002-scheduled-binding-simplification.md`.
- Approved scope supplied explicitly by the user: specification revision 13, REQ-012, AC-007, INV-001–005. Both pinned commits contain revision 14 draft; no approval or new obligation is inferred from its managed-table upgrade note. The scoped task and user approval suffice for both bounded corrections.
- HEAD remained the candidate; no tracked production, test or task source was changed. Only reports in this new review directory were written. No commit, merge, push or deploy occurred.

## Reconciled acceptance matrix

PASS entries establish source conformance and inspected proof, with executed versus historical verification distinguished below. They do not imply independently rerunning every journey.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-012: reject binding inputs, empty/exact maps, no alias or ignored field | VerificationBinding removes inputs and retains deny_unknown_fields; shared path-aware envelope decoder | Inline removed-field test independently passed; Rust/Python/TS negative journey source and Rust raw-HTTP 400/404 proof inspected | PASS |
| REQ-012: samples required:false refused; normal optional JSON/media/trace preserved | Samples variant loses required; other variants/required() arms unchanged; existing mixed-evidence validation retained | Same focused contract test; all three SDK negative scenarios inspected | PASS |
| Field-specific diagnosis and no persistence | Decoder identifies parent path and explicit unknown field; registration decode precedes projection | SDK and raw-HTTP negatives assert error family/location and Card absence | PASS |
| AC-007: both drift requirements judged without binding selectors | DriftEngine::checks unconditionally iterates exact spec.drift_checks(); no slots/schedule_inputs claim map | Three SDK two-requirement sample-write/flush/activate/read paths inspected | PASS |
| 120/0.01 yields failed/passed, overall failed; 80/missing-error_rate yields passed/inconclusive, overall inconclusive | Existing scoring, unscored report and decide(all) owners retained | Matching deterministic branches in Rust/Python/TS; task records green focused story runs, not independently rerun here | PASS |
| Exact pinned Verifier, subject, window and requirement set survive retry/reclaim | Execution-owned UID/version/window, exact tenant-keyed runner resolution, unchanged lease fences; requirements derived from same pinned spec | Existing freeze/retry/reclaim supporting tests inspected; no new forced two-check reclaim run claimed | PASS |
| Baselines, empty windows, scoring and failed-only Operators remain | Only obsolete omission gate removed; baseline/scoring/settlement owners unchanged | Existing story and supporting runtime/operator tests retained and inspected; task claims green runs | PASS |
| Persistent/tenant behavior | NewBinding/projection/claim remove same column/map; TenantConn/RLS and caller-owned transaction remain | Independently executed migrations and 10 binding SQL tests pass; domain audit passes | PASS |
| INV-001–005 and task non-goals | No new runtime, group/outbox/admission/benchmark owner, platform-specific mechanism or alternate audit/write path | Complete discovery source trace and domain/system reports | PASS |
| Public guidance and generated consumers accept only selector-free authoring | Normal guides/types/fixtures agree, but reachable activation-mismatch metadata still prescribes forbidden inputs | Independent source-to-HTTP and generated-doc trace in validation | FAIL — FIND-TASK-002-2 |
| Required changed-Rust documentation | New decode lacks Errors; materially changed spec_from_kind_value lacks rustdoc | Independent body/caller inspection under explicit AGENTS §16 gate | FAIL — FIND-TASK-002-1 |
| Shared machinery uses existing owners | Existing parser, registration, SQL queue, DriftEngine and journey helpers extended; selectors deleted | Maintainer and implementation owner/caller comparisons | PASS |

## Independent review results

| Role | Report | Discovery result |
|---|---|---|
| behavior-rev | task-review-behavior.md | FAIL: BEH-001 |
| invariant-rev | task-review-invariants.md | FAIL: INV-DOC-1, INV-ERR-1 |
| repo-rev | standards-review.md | FAIL: RSTD-001 |
| maintainer-rev | maintainer-review.md | FAIL: MAINT-001, MAINT-002 |
| system-rev | system-review.md | PASS, no material findings |
| domain-rev persistent data/tenancy | domain-review-persistent-data.md | PASS, no material findings |
| domain-rev evaluation | domain-review-evaluation.md | PASS, no material findings |
| fresh ponytail-rev | findings-validation.md | Two independently CONFIRMED findings |

All required reports are complete. No reviewer filled multiple roles. Discovery reviewers did not receive other reviewers' conclusions or an intended verdict. The fresh validator received every report and inspected source independently. No follow-up was necessary: claims were consistent and no unresolved reachable path or repeated-remediation cause emerged. See claim-comparison.md. The orchestrator reconciles the independently validated ledger; it does not originate or self-validate findings.

## Validated ledger

| Stable ID | Status/classification | Exact location and consequence | Minimal correction |
|---|---|---|---|
| FIND-TASK-002-1 | CONFIRMED / VIOLATION | envelope.rs:752–759: new decode has no Errors; materially rewritten spec_from_kind_value has no rustdoc. Explicit repository acceptance gate fails at shared parsing boundary. | Document both existing functions accurately, including failure conditions and retained normalization; preserve bodies. |
| FIND-TASK-002-2 | CONFIRMED / REGRESSION | wyrd-spec/src/error.rs:2076, emitted by server check_activation; generated api/errors.md:351 repeats it. Recovery guidance instructs callers to add binding inputs that the new contract rejects. | Remove obsolete mapping instruction from authoritative error metadata, preserve error/refusal identity, regenerate via docs:generate, assert corrected remediation in existing HTTP mismatch branch. |

Full source-local discovery IDs, producer/caller/sibling traces, governing obligations, consequences and focused closure proof are preserved in findings-validation.md. There are no rejected findings or human-directed additions. No prior FIND IDs or remediation rounds were supplied, so prior-finding closure is not applicable.

## Verification and limits

Independently executed by the orchestrator, all passing:

- `git diff --check ef3a57b07..5c421663b`.
- Exact wyrd-spec removed-field contract test: 1 passed, 757 skipped.
- Python formatting check and Ruff/authored-stub lint checks (temporary writable uv cache).
- Repository-managed Wyrd and Vala migration/idempotence tests: 1 + 1 passed per setup lifecycle.
- Focused `pg_verification_bindings::` SQL tests: 10 passed, 93 skipped.
- Exact served OpenAPI Card lifecycle/problem and verification operations/upload tests: 2 passed, 268 skipped.

See verification.md for exact commands, nextest IDs and initial uv-cache failure/retry. Both Postgres lifecycles recovered the host endpoint through the existing wrapper's sanctioned rebuild and completed successfully; Postgres was runnable in this sandbox. No database failure was misreported as a pass.

The task's broader Rust/Python/TS, server, SQL, CLI/MCP and static counts remain historical implementation claims. No full journey suite or gate was run. Two focused served-document tests do not independently assert every removed field or substitute for the full principals lane. No crash/recovery, performance, cross-platform runtime or mixed-version rollout qualification is claimed. The implementation explicitly omitted separate RED; source inspection cannot retroactively establish it. This remains a historical process/evidence limit, not an invented third code defect or instruction to manufacture history.

## Remediation and decision

**FIX_REQUIRED**. Both findings have bounded corrections within the approved task; no specification revision is needed. Route the single self-contained `TASK-002-R1-decoder-docs-and-activation-guidance.md` in this directory directly to `$wyrd-implement`. It batches the mandatory documentation correction with the public guidance defect and prescribes only focused closure proof and narrow static lanes. Do not implement during review. Re-review the resulting immutable cumulative candidate against the original task before granting PASS.
