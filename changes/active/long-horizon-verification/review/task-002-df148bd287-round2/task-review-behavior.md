# TASK-002 behavior acceptance review — remediation round 2

Overall: **PASS**. Proposed classified findings: **none**.

## Immutable subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-002`.
Base `ef3a57b0776dd7bc451273b9176498fcca700550`; candidate `df148bd287d3e9fcae861470fe9c27bd576f2eeb`, branch `change/long-horizon-verification-task-002`. HEAD matched candidate during inspection. Reviewed the cumulative three-dot diff, and separately compared prior candidate `5c421663b43d7deeca135134d6b07cf49a11b83f` for remediation closure.

Acceptance authority is owner-approved revision-13 REQ-012, AC-007 and INV-001–005, the original TASK-002 and its R1 remediation. The checked-in revision-14 draft does not grant additional scope. Read AGENTS.md, agent rules, the reference router, spec-driven development, maintainer style, testing workflows, evaluation and drift-monitoring references, and applicable Verifier/binding design and doctrine. The lower-level drift-monitoring reference retains old selector prose; approved REQ-012 and the active design's selector-free contract override that prose. This does not authorize restoring selectors.

Used CodeGraph before code exploration, then read current owners, cumulative diffs, callers and test assertions. No current discovery report was read. No production or test file was changed.

## Caller-to-result and owner comparison

1. Local Card loading and raw HTTP registration reach the existing typed Card/Spec boundary. `envelope.rs::spec_from_kind_value` dispatches all native kinds through one `serde_path_to_error` decoder. `VerificationBinding` remains `deny_unknown_fields` and now has no `inputs`; `InputSlot::Samples` has no `required`. Forbidden fields are rejected rather than normalized or retained. The existing typed errors remain the projection boundary. `required: true` is also refused on Samples, consistent with removing the optionality field rather than preserving an alias.
2. Registration still uses `EffectiveSpecs::validate_binding` to load exact references, check schedule compatibility, validate the cron and Operators, then the existing registration transaction and `BindingProjector` project accepted bindings. `check_activation` retains the samples-only activation check. Removing `validate_schedule` does not accidentally allow mixed required JSON/media/trace inputs: unchanged `validate_evidence_mode` rejects combining samples and other slot types before registration. No second resolver or schedule validator was introduced.
3. `NewBinding`/`PROJECT_BINDING_SQL` and the additive migration remove only obsolete binding selector storage. Existing tenant transactions, natural-key conflict handling, pinned Verifier/subject/Trigger/Operator identities and schedule cursors remain owners of durable state.
4. `CLAIM_SQL` still returns execution-owned exact Verifier UID, subject UID, accepted authority and frozen window; it no longer reads a mutable binding selector map. `VerifierRunner::resolve` loads/caches by tenant and exact Verifier UID; `dispatch` supplies that spec and fixed window to `DriftEngine::checks`. Its complete `spec.drift_checks()` iterator replaces the slots membership filter. Existing `score`, observation-window query construction, fitted baseline decoding, failure propagation and unscored reports remain the scoring owner. Missing measurements become unscored/inconclusive, rather than removing a requirement.
5. Existing first-class SDK journey owners register, write samples, flush, activate and read results. They add two-slot deterministic Custom checks without changing SDK transport or lifecycle logic. Rust directly exercises raw HTTP refusal and post-refusal absence, preventing local SDK validation from masking a server gap. Python and TypeScript independently prove local refusal and absent Cards. Existing fitted, passing, failing, empty-window and Operator cases remain.

The new decoder is earned field-path diagnostic reuse of an already-installed dependency, not a second parser or removed-field blacklist. Harness changes extend existing owners; the Custom-only fixture avoids an inapplicable baseline wait. The removed omission journey asserted the behavior explicitly forbidden by AC-007, and replacement three-SDK tests prove the required complete requirement set. No unnecessary replacement owner was found.

## Acceptance matrix

Verification below distinguishes inspected executable assertions from historical implementer results. This reviewer executed no runtime checks; the orchestrator owns independent checks.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-012: one Verifier declaration, no binding `inputs` or selector alias | `card/verifier/mod.rs::VerificationBinding`; deleted `ScheduledInput`/`validate_schedule`; binding projection and claim SQL | Rust/Python/TS removed-field journey assertions; unit `removed_scheduled_samples_fields_fail_at_their_path` | PASS |
| Refuse `{}` and exact binding maps at authored field | Shared `envelope.rs::decode`, binding deny-unknown-fields | Unit checks `verified_by[0].inputs`; Rust raw HTTP asserts 400 and existing public code/path | PASS |
| Refuse samples `required:false` at the offending declaration | Samples variant has only `features`; same decoder | Unit asserts slot path plus unknown `required`; SDK and raw HTTP negative cases identify `inputs.latency` and forbidden field | PASS |
| Refusals persist no invalid Card/binding or execution | Decode occurs before registration projection; transaction remains unchanged | Rust raw HTTP cases assert absent Cards; existing registration refusal test retains no-write helper | PASS |
| All samples are mandatory; ordinary JSON/media/trace optionality unchanged | `InputSlot::required` returns true for Samples and retains existing bool for other variants | Unchanged optional-slot validation tests plus typed source; historical wyrd-spec 760/760 | PASS |
| Exact bound Verifier, subject and window remain authority on claim/retry/recovery | Execution-owned UID/window columns, exact tenant-keyed resolver; no selector subquery | SQL frozen scheduled/submitted-input assertions and retry test preserve window; claim/reclaim implementation preserves identity fields | PASS |
| Every declared drift requirement is evaluated | `DriftEngine::checks:697–704` iterates all drift checks; no map filter | Two-slot scenarios require exact outcome map in all three SDKs | PASS |
| AC-007 sufficient 120/0.01: failed latency, passed error rate, failed overall | New `latency-error-budget.yaml` declares deterministic Custom checks | Rust, Python and TS two-requirement scenario first branch | PASS |
| AC-007 latency 80/no error rate: passed/inconclusive, inconclusive overall | Existing unscored-report path retained per check | Same three SDK scenario second branch, exact requirement maps | PASS |
| Exact subject/Verifier/window result provenance | Existing result builder untouched; exact resolver and window dispatch retained | Three SDK scenarios assert subject, Verifier name and per-requirement window; SQL frozen UID/version/window assertions | PASS |
| Existing scoring, Window submit, baseline and failure-Operator behavior | Only omission filter/signatures removed; scoring bodies, submitted-input branch and delivery owner untouched | Existing scheduled passing/failing/empty/baseline cases retained; historical Rust 7, Python 10, TS 10 passing cases | PASS |
| Generated schemas/types, authoring fixtures and public docs use new contract | Samples/binding derives; generated TS deletion; fixtures and build/verify docs updated; architecture aligned | Historical codegen:check/docs:check and loader/MCP assertions; source inspection found no new alias | PASS |
| INV-001 Wyrd-owned machinery | No added runtime/queue/provider dependency | Cumulative diff and owner tracing | PASS |
| INV-002 load bounds preserved | Existing per-execution query/runtime limits unchanged; removed selector storage/read | Cumulative runtime diff; no new staging or fan-out mechanism | PASS |
| INV-003 Rust server ownership and tenancy | Existing TenantConn, exact tenant cache and SYSTEM-read boundary retained; spec stays IO/async/PyO3-free | Source signatures and diff; historical check:deps/check:tenant-isolation | PASS |
| INV-004 immutable judgment/identity boundary unaffected | No result mutation, source grouping or historical revision changes; exact spec lookup retained | Cumulative source inspection and existing SQL identity assertions | PASS |
| INV-005 portable execution/audit/outbox boundary unaffected | No new platform mechanism or changed audit/outbox path | Cumulative diff | PASS |
| Non-goals: no grouping, admission, benchmarks, scoring redesign or compatibility | Cumulative diff limited to selector removal and consumer/proof closure | Full changed-file audit, including manifest/lock additions and documentation | PASS |

## Prior-finding closure

| Finding | Closure evidence | Result |
|---|---|---|
| FIND-TASK-002-1 — VIOLATION | `envelope.rs:752–781` now documents typed decoding, field-path failures, kind dispatch, existing `type` preprocessing and External refusal. Both functions have meaningful `# Errors`. Prior-to-current diff changes only their doc blocks, leaving complete bodies unchanged. Direct callers remain `Spec::from_kind_and_value` and `Card::deserialize`. | CLOSED |
| FIND-TASK-002-2 — REGRESSION | `error.rs:2076` now directs authors to samples declarations, with no binding-input instruction; existing error identity/title/status remain. `errors.md:351` matches. Existing HTTP mismatch branch at `pg_card_registration_route.rs:1079–1095` asserts samples guidance and excludes `binding inputs` while retaining code and no-write assertions. The source producer closes HTTP/MCP/docs projections together. | CLOSED |

Historical R1 evidence records the exact focused HTTP test RED on stale guidance and GREEN after correction, plus docs generation and static checks. Source inspection corroborates the changed assertion and authoritative producer; this report does not independently claim those historical runs were executed by this reviewer.

## Verification limits and findings

No proposed MISSING, INCORRECT, DRIFT, VIOLATION or REGRESSION finding remains. Original implementation did not record a runtime RED before the initial removal; it explicitly discloses that fact. There is no new timeout/skip/assertion weakening used to conceal a failing requirement. Original task evidence is present at the candidate and includes focused commands, counts and non-goal audit.

Skipped runtime checks, including Postgres, full journeys and gate, because the orchestrator owns verification and full suites are prohibited for this review. Behavioral acceptance here combines independently inspected source/test coverage with clearly attributed historical proof; it does not certify an independently rerun integration environment.

Orchestrator verification supplement (read after independent source review): current `verification.md` records independent PASS for the exact decoder test (1 selected), exact activation-mismatch HTTP test (1 selected), repository-managed migrations and `fmt:check`. Postgres is runnable in this sandbox. `docs:check` stopped at dependency installation with `unable to open database file`, so current documentation-gate execution is unproven; this is an environment limitation, not a source defect established by this behavior audit. Historical broader SDK/check evidence remains historical. The independent cumulative whitespace check flags only a blank final line in a prior review report, with no task behavior consequence.
