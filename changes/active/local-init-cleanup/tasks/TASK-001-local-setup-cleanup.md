---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-local-init-cleanup
spec_revision: 1
requirements: [REQ-008, REQ-009, REQ-010, REQ-011, INV-004, INV-005]
acceptance: [AC-006, AC-007, AC-008]
depends_on: []
---

# Local setup cleanup

Status: proposed. This existing task and [the documentation task](TASK-002-document-verification-scope.md)
share [spec revision 1 (draft)](../spec.md). No implementation or broader setup
redesign is included in this document.

## Outcome

Make `wyrd init` and `wyrd init guided` report setup accurately and quietly.
Agents must receive the current documentation URL, normal startup must not
produce repeated ERROR messages, and restarting an already configured gateway
must show its existing deployments.

## Required changes

1. **Correct the documentation URL.** Replace the retired
   `https://bohmian-ai.github.io/wyrd/` in the MCP onboarding guide with
   `https://docs.bohmian.ai/wyrd/`. Keep `/wyrd/`, `llms.txt`, and
   `llms-full.txt` paths intact. Check active onboarding sources for the same
   stale URL; do not rewrite historical change records or generated files.
2. **Remove expected startup error spam.** `/readyz` must still return 503
   while dependencies initialize. Expected startup polling must not emit an
   ERROR per request. Keep those routine diagnostics at DEBUG if useful.
   Preserve genuine dependency-failure diagnostics, unexpected request errors,
   readiness loss after startup, and the CLI's startup timeout/exit errors.
   Do not globally lower HTTP failure logging or report readiness early.
3. **Report actual configuration.** When setup is skipped on a restart, read
   existing deployments through the authenticated gateway administration
   client. Report their names as existing configuration. Report “not
   configured” only after a successful read finds no deployments. A failed
   read must surface as a failure or an explicit unknown state, never as an
   empty gateway. Applying an explicit gateway file must keep working.

Present the existing readiness summary as a concise, labelled checklist that
both people and agents can understand. Reuse the facts already collected:

```text
Local Wyrd ready for tenant support.
  [ready] HTTP: http://127.0.0.1:8080
  [ready] gRPC: http://127.0.0.1:50051
  [ready] UI: http://127.0.0.1:3000
  [configured] Storage: file:///.../storage
  [configured] Bifrost data: /.../bifrost
  [saved] Client settings: /.../config.toml
  [configured] Gateway: openai (existing)
```

Wording and markers may follow existing CLI conventions. Preserve the current
useful paths, credential-file location, MCP host outcomes, diagnostic log
location, and shutdown guidance. “Configured” confirms stored configuration,
not provider-key validity or a successful inference call. Keep JSON output a
single readiness document with its existing fields and types; its gateway
summary must agree with text output. No new schema or health-check framework.

## Implementation and proof

Work in order: URL correction, readiness logging, then configuration summary.
Reuse current owners and fixtures. No new dependencies, persisted settings,
provider calls, automatic credential replacement, or changes to first-run
provider selection. Card-registration logging is separate from this task.

### Documentation URL

Static proof is sufficient: inspect the MCP guide and search active onboarding
sources for the retired URL. No new test solely to pin a URL string.

### Startup readiness logging

**Behavior.** Repeated startup `/readyz` responses remain 503 without ERROR
spam. Normal API failures and real readiness failures retain diagnostics.

**RED.** Use the smallest existing HTTP/telemetry fixture to demonstrate that
an expected startup 503 currently emits ERROR; include an ordinary failing API
response as a control. Confirm the actual focused test selector before running.

**GREEN.** Adjust the existing readiness/HTTP logging owner so the expected
startup case is quiet without weakening readiness or ordinary failure logs.

**REFACTOR.** Keep the correction local; reuse existing readiness state and
logging facilities rather than adding another startup state machine.

### Restart configuration summary

**Behavior.** A restart with retained deployments names them as existing;
an empty gateway says “not configured”; a failed query does not claim absence.
The checklist retains the other setup facts and machine output stays valid.

**RED.** Extend the existing local-init restart journey to assert the summary
alongside its existing persistence assertions. Use the narrowest existing
fixture for empty and failed-read cases; no separate test suite or new harness.

**GREEN.** Read tenant-scoped gateway state when no document is applied and
render that result through the existing readiness output owner.

**REFACTOR.** Reuse the administration client and summary data already present;
avoid duplicate configuration discovery or provider validation.

## Implementation reference

- `crates/wyrd/wyrd-server/src/mcp/guide.rs`: onboarding guide contains the
  stale URL. The CLI launches the shared setup prompt, which directs agents
  into this guide; fix the guide rather than adding a CLI-specific prompt.
- `crates/wyrd/wyrd-server/src/http/router.rs`: `build_router` wraps health
  routes in Tower tracing; its default failure callback classifies 503 as ERROR.
- `crates/wyrd/wyrd-server/src/components/health/mod.rs`: `readyz` reads the
  cached snapshot; `readiness_loop` and dependency probes own readiness state
  and existing failure diagnostics.
- `crates/wyrd/wyrd-cli/src/init.rs`: `DevServer::ready` polls every 100 ms;
  `DevInputs::configure_gateway` returns “skipped” without reading existing
  deployments; `DevReady::print_text` and serialization own the summaries.
- `crates/wyrd/wyrd-cli/tests/pg_tests.rs`: the retained managed-secret journey
  already configures a gateway, restarts init, and proves inference still works.
  Existing local-init coverage also asserts the old “Gateway: skipped” wording;
  update affected assertions to match actual state.

## Verification

Keep verification focused. The URL needs static inspection. The two changed
branches need small regression checks, preferably by extending existing tests.
Do not add tests for decorative checklist formatting or run the broad gate.

For the existing retained-gateway journey, use the installed CLI/server/UI
prerequisites from `mise.toml`'s `test:local-dev:journey` task, then run:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli --run-ignored=all -E 'test(=pg_tests::init_keeps_the_gateway_managed_secret_key)'
```

Record exact focused commands for any additional tests selected or added.
Run `mise run fmt` and `mise run lints` for the implementation. Inspect one
normal startup and restart summary, including JSON mode, for the acceptance
outcomes above. Never log keys, submitted secrets, or credential contents.

Authority: root `AGENTS.md`, `architecture/operations/README.md` readiness rule,
`changes/implementation-execution.md`, and `TESTING.md`. Stop if a fix requires
a new public contract, persisted state, or changing when the server is ready.

## Implementation evidence

Commits: `d40786c98`, `66f30204d`, `16c2ff177`, `8b5cb0c2c`. Spec revision 1 is
still `draft`; implemented on the maintainer's direct instruction.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-006 current docs origin | `crates/wyrd/wyrd-server/src/mcp/guide.rs` | `git grep bohmian-ai.github.io` finds nothing outside completed records; generated `docs/public/llms-full.txt` already used `docs.bohmian.ai` | PASS |
| AC-007 quiet startup 503, failures retained | `http/router.rs` `RequestFailures` classifier treats only `/readyz` 503 as an expected answer (DEBUG response log); `health/mod.rs` `log_transition` logs INFO on ready and WARN with checks on readiness loss | `http::router::tests::readiness_503_is_not_logged_as_a_failure` (RED: `/readyz` 503 logged ERROR; control 503 on `/v1/cards` still ERROR); `components::health::tests::readiness_transitions_are_logged`; `pg_tests::init_first_run_is_client_ready` asserts no `/readyz` ERROR in the diagnostic log (fails against the pre-fix router) | PASS |
| AC-008 actual gateway configuration | `init.rs` `GatewaySummary` + `stored_deployments`: no-apply paths read tenant deployments through `wyrd_client::Gateway`; empty → `not configured`, failure → `unknown`; checklist text and JSON `gateway` string share status/detail | `init::tests::gateway_summary_distinguishes_empty_and_failed_reads`; `pg_tests::init_keeps_the_gateway_managed_secret_key` (rerun prints `[configured] Gateway: local-mock (existing)`, inference and secret-hiding still hold); `pg_tests::init_saved_settings_survive_working_directory_change` (`[not configured] Gateway:`); `init_first_run_is_client_ready` (JSON `gateway`); Python `test_installed_cli_guided_setup`, `test_installed_cli_gateway_setup` | PASS |

Commands:

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=http::router::tests::readiness_503_is_not_logged_as_a_failure) | test(=components::health::tests::readiness_transitions_are_logged)'
mise exec -- cargo nextest run --locked -p wyrd-cli --lib -E 'test(=init::tests::gateway_summary_distinguishes_empty_and_failed_reads)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli --run-ignored=all -E 'test(=pg_tests::init_keeps_the_gateway_managed_secret_key) | test(=pg_tests::init_saved_settings_survive_working_directory_change) | test(=pg_tests::init_first_run_is_client_ready) | test(=pg_tests::init_repeat_and_partial_failure_preserve_state)'
mise exec -- scripts/postgres/with-test-postgres.sh -- node scripts/check_ts_package.mjs mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_installed_cli_guided_setup tests/integration/test_local_development.py::test_installed_cli_gateway_setup
mise run fmt && mise run lints && git diff --check
```

Notes: the readiness header line `Local Wyrd ready at <endpoint> for tenant <t>.`
is kept because Rust/Python journeys and `get-started/setup.md` key on it;
the checklist follows it. JSON `gateway` keeps its string type
(`"<status>: <detail>"`). A failed deployment read reports `unknown` rather
than failing setup. Non-goals (card-registration logging, provider calls,
first-run selection, new schema or dependency) stayed excluded. The new
`test_support::CapturedLogs` duplicates the private sink in
`operators/keys.rs` tests, which was left untouched.

Status: `IMPLEMENTED`; routes to `$wyrd-task-review`.
