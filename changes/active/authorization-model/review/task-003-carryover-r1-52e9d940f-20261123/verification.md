# Fresh review verification

Candidate: `52e9d940fd57c49a3ab5084fb319171e21eafb37`. Root owns all execution; reviewers did not run concurrent Cargo commands. Existing R1 evidence is separate from these fresh results.

## Focused production journeys and migration setup: PASS

Executed through the existing Postgres/Mailpit lifecycle:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration --no-fail-fast -E "test(=platform_admin_e2e::signup::email_signup_proves_a_mailbox_in_the_browser_that_asked) | test(=platform_admin_e2e::signup::completion_consumes_either_proof_and_recovers_its_own_claim) | test(=platform_admin_e2e::signup::the_owned_workspace_cap_holds_atomically_across_purge) | test(=platform_admin_e2e::signup::signup_refuses_without_its_derivation_key) | test(=platform_admin_e2e::signup::sign_in_starts_are_admitted_per_source) | test(=platform_admin_e2e::signup::a_configured_challenge_gates_browser_creation)"'
```

- Wyrd migration/idempotence: 1/1 PASS, run `46f0e570-951c-4ddf-a297-d358a092ec35`.
- Vala migration/idempotence: 1/1 PASS, run `c7dccce2-f30e-45e2-a150-73af37a641f6`.
- Exact six journey selectors: 6/6 PASS, 533 skipped, 6.134 seconds; run `97730818-58ed-4553-9df4-521ef8ccd83a`.
- Parent cycle-11 intentionally RED setup test was not selected or waived.

## Derivation-key unit checks: PASS

```sh
mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=derivation::tests::pseudonyms_separate_scope_and_key) | test(=derivation::tests::sealed_values_open_only_for_their_label)'
```

2/2 PASS, 140 skipped, 0.004 seconds; run `adaa9209-dc61-4fc9-918d-684b956bc4d5`.

## UI gate: PASS

`mise run ui:check` completed successfully: package and host Svelte checks each reported 0 errors/0 warnings; Vitest passed 11/11 files and 101/101 tests. Total task duration 25.77 seconds. Required TypeScript addon build completed first. Tracked source remained unchanged afterward.

## Other limits

No full parent-task suite, broad gate, live-provider credentials or benchmark was requested/run. Additional execution only if a specific unresolved issue warrants it. Cumulative diff whitespace output is disclosed in `subject.md`; do not describe it as globally clean. The two on-disk authority files remain pre-existing uncommitted inputs.
