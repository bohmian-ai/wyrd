Recorded command claims, not independently rerun:


```bash
mise exec -- cargo nextest run --locked -p vala-eval --lib            # 120 passed
mise exec -- cargo nextest run --locked -p vala-drift --lib           # 75 passed
mise exec -- cargo clippy --locked -p vala-eval -p vala-drift --all-features --lib -- -D warnings  # clean
mise exec -- cargo clippy --locked -p vala-eval --all-features --lib --tests -- -D warnings        # clean
mise run fmt                                                          # clean
mise run check:deps                                                   # exit 0
git diff --check                                                      # clean
# Focused, each 1 passed:
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::scope::tests::select_distinguishes_absent_from_null)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::engine::tests::no_step_verifier_judges_inputs_and_traces)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::engine::tests::context_capture_hashes_or_redacts_actual_values)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::engine::tests::independent_actions_overlap_within_the_concurrency_bound)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::engine::tests::dependents_wait_for_upstream_step_requirements)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::engine::tests::each_assertion_compares_every_wildcard_match)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::engine::tests::skip_causes_propagate_and_failure_wins)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::engine::tests::missing_and_optional_evidence_resolve_honestly)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::engine::tests::empty_and_all_skipped_verifiers_are_inconclusive)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::engine::tests::decide_rules_count_inconclusive_requirements)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::engine::tests::retries_spend_the_budget_and_keep_attempts_visible)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::engine::tests::step_timeouts_fail_the_step_and_let_unrelated_work_finish)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::engine::tests::execution_deadline_cancels_running_work)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::engine::tests::hosted_check_passes_need_reachable_citations)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::engine::tests::drift_requirements_score_through_vala_drift)'
```

Material limit: the Vala integration target and journey suites were not run
(per task); scenario consumers still compile against the preserved
`execute_plan`.

Status: IMPLEMENTED.
