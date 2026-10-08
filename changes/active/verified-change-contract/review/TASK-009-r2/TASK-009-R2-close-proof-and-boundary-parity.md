---
id: TASK-009-R2
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 45
requirements: [REQ-123, REQ-151, INV-007, INV-012, AC-032]
depends_on: [TASK-009, TASK-009-R1]
parent_task: TASK-009
remediates: [FIND-TASK-009-1, FIND-TASK-009-5, FIND-TASK-009-6]
---

# Close Run failure proof and public boundary parity

## Authority and immutable inputs

- Approved spec: `changes/active/verified-change-contract/spec.md`, revision 45, preserving TASK-009's revision-35 obligations.
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`.
- Prior remediation: `changes/active/verified-change-contract/review/TASK-009-r1/TASK-009-R1-restore-otel-correlation.md`.
- Base: `7d96c30066425e0cde2290842d5801307843283d`.
- Reviewed candidate: `c47761decff8db95768d2c05f0b85b8ba62a021a`.
- Independent ledger: `changes/active/verified-change-contract/review/TASK-009-r2/findings-validation.md`.

## Diagnosis and intended outcomes

### FIND-TASK-009-1 — actual registration failure has no observation proof

AC-032 separately requires API-only/no-SDK and processor-registration failure proof. At `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:375–388`, the provider's `add_span_processor` raises, but the test only calls installation. The Run/Drift test at `:414–425` chooses `object()`, reaching the missing-method branch in `python/wyrd/otel.py:260–262`, not the exception branch at `:269–275`. R1 wording compresses these cases but cannot weaken approved AC-032.

Current runtime containment is supported by source. The defect is the absent regression check: registration and Run/emission could become coupled while the suite remains green. Close only this remaining part of prior ID 1.

Selected correction: use the existing Python focused failure-test home and representative `_drift_reaches_the_ordinary_boundary` mechanism. Run entry must select a global provider whose registration actually raises, and a Drift call within that scope must reach `WYRD_SDK_400_BIFROST_NOT_STARTED`. Establish that registration was attempted, preventing a no-method or already-registered bypass from passing. Preserve existing API-only and all other failure cases. No production change is needed; separate installation-only and API-only tests cannot substitute for the required combined path.

### FIND-TASK-009-5 — public exit declaration and runtime disagree

`sdks/wyrd-sdk-python/src/observe/mod.rs:225–236` exposes `_exc_type`, `_exc_value`, `_traceback`, all optional with None defaults. The owning stub at `python/wyrd/stubs/observe.pyi:57–62`, projected into `observe/__init__.pyi:59–64`, declares conventional names without defaults. A real public Run rejects `run.__exit__(exc_type=None, exc_value=None, traceback=None)` with TypeError, although the stub accepts it; runtime accepts `run.__exit__()` although typing rejects it. Positional with-protocol tests hide the defect. This violates approved public names and runtime/stub parity authority.

Selected correction: align the existing PyO3 signature and parameter names with `exc_type`, `exc_value`, `traceback`, and express the existing None defaults in the owning hand-authored stub. Regenerate through the existing assembler. Keep precise exception/traceback types and Literal[False]. The invalid call contract originates in these two boundary sources, so fix them rather than adding a consumer wrapper, keyword aliases or a second declaration mechanism. This aligns approved behavior; it introduces no new API choice. Preserve optional cleanup and exception propagation, including positional with calls and omitted arguments already accepted by runtime.

### FIND-TASK-009-6 — new Rust tests omit required panic docs

`crates/shared/wyrd-client/src/observe/tests.rs:648–682` adds selection/refusal tests using fixture loading, expect/expect_err and assertions. Their rustdoc lacks # Panics. AGENTS §16 explicitly applies panic documentation to new test functions. The executable assertions are correct; only documentation is incomplete.

Selected correction: document the actual fixture and selection/invocation/refusal assertion panic conditions on these two new tests. Reuse ordinary rustdoc and the nearby `state_fixture`/Rust SDK `assert_initial_card_selection` pattern. Preserve executable bodies and assertions. Older adjacent documentation drift does not justify widening this correction; no runtime test or lint suppression is needed.

## Constraints and non-goals

Preserve one invocation across immutable Card views, strict local alias lookup, optional fail-open OTel, exact attributes, nesting/async/task-copy behavior, serialized key creation and failed-detach restoration. Preserve authenticated ingest and server-derived identity, explicit observation errors, caller-owned provider/exporter lifecycle, and separate flush/shutdown/publication barriers. Retain every existing failure and user-exception check.

No server Run, second pipeline/queue/writer, wrapper span, mandatory dependency, provider lifecycle ownership, telemetry retry/warning, alias/compatibility layer, per-observation failure matrix, unrelated documentation sweep or production OTel refactor. Do not reopen closed findings 2, 3 or 4. No public behavior beyond aligning the already approved exit names/defaults.

## Acceptance criteria

1. FIND-TASK-009-1: focused proof establishes an actual registration attempt raises during Run entry and explicit Drift still reaches the ordinary offline writer error; normal exit and application exceptions retain their behavior.
2. FIND-TASK-009-5: public Run conventional-keyword and omitted-argument exit calls return False; runtime signature names/defaults agree with generated public stubs; positional context-manager use and exception propagation remain green.
3. FIND-TASK-009-6: both new Rust test rustdoc blocks accurately state their unchanged panic conditions.
4. Existing healthy active/child/nested/async/private/global/concurrent-first-use/detach-recovery tests and authenticated persisted joins remain green. Generation and relevant typing/boundary gates pass.

## Focused proof and broader verification

Inspect `mise.toml` first; `py:setup` already enables testing. For changed executable behavior follow repository Red-Green discipline. Use the existing test `test_registration_and_attach_failures_never_block_observations` for the registration outcome, and name the exact exit-parity check in implementation evidence.

```bash
mise run py:setup
cd sdks/wyrd-sdk-python
mise exec -- uv run python -m pytest -q tests/unit/state/test_observe_surface.py::test_registration_and_attach_failures_never_block_observations
mise exec -- uv run python -m pytest -q tests/unit/state/test_observe_surface.py
```

Run the exact focused command for the exit-parity test selected during implementation. Rust doc-only correction needs static inspection and formatting, not another runtime test. After rebuilding the SDK run the unchanged persisted journey from repository root:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/state/test_observe_journey.py::test_scoped_run_emits_drift_eval_and_generic_rows'
mise run test:shared
mise run py:test:unit
mise run py:test:integration
mise run py:typecheck
mise run codegen:check
mise run check:pyo3-scope
mise run fmt
mise run py:format
mise run lints
mise run py:lints
git diff --check
```

Route directly to `$wyrd-implement`. A later task review must assess the cumulative original base through the remediated candidate, not merely this fix diff.

## Implementation evidence

Commits: `c50aa1b07`, plus formatter-only follow-up.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1. FIND-TASK-009-1: an actual registration attempt raises during Run entry and Drift still reaches the offline error; normal exit and app exceptions keep their behavior | `tests/unit/state/test_observe_surface.py::test_registration_and_attach_failures_never_block_observations` selects a global provider whose `add_span_processor` records the attempt and raises, runs `_drift_reaches_the_ordinary_boundary` in scope, asserts one attempt, then asserts a `ValueError` from the block propagates (second attempt). No production change | `uv run python -m pytest -q tests/unit/state/test_observe_surface.py::test_registration_and_attach_failures_never_block_observations` (pass) | PASS |
| 2. FIND-TASK-009-5: conventional-keyword and omitted exit calls return False; runtime names/defaults match generated stubs | `src/observe/mod.rs` `__exit__` signature `(exc_type=None, exc_value=None, traceback=None)`; `python/wyrd/stubs/observe.pyi` defaults `= None`, regenerated `observe/__init__.pyi` via `mise run codegen:regen` | `uv run python -m pytest -q tests/unit/state/test_observe_surface.py::test_run_exit_accepts_conventional_keywords_and_omitted_arguments` (red before rebuild: params were `_exc_type…`; green after); `codegen:check`, `py:typecheck` pass | PASS |
| 3. FIND-TASK-009-6: both new Rust tests document panic conditions | `crates/shared/wyrd-client/src/observe/tests.rs` `# Panics` on `run_for_card_selects_the_initial_view_and_shares_its_invocation` and `run_for_card_refuses_an_unknown_alias_without_network_io`; bodies unchanged | static inspection, `mise run fmt`, `mise run lints` | PASS |
| 4. Existing behavior, generation, typing and boundary gates stay green | no other changes | full `test_observe_surface.py` (34 passed); persisted journey `test_scoped_run_emits_drift_eval_and_generic_rows` (1 passed); `test:shared` (704 passed); `py:test:unit` (513 passed); `py:test:integration` (72 passed); `py:typecheck`, `codegen:check`, `check:pyo3-scope`, `fmt`, `py:format`, `lints`, `py:lints`, `git diff --check` all exit 0 | PASS |

Non-goals stayed excluded: no production OTel change, no alias/compat layer, no `#[allow]` (the unused exception triple is bound explicitly), and no unrelated files changed.
