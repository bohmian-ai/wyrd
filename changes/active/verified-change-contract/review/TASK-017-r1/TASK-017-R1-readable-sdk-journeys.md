---
id: TASK-017-R1
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 66
requirements: [REQ-192, REQ-195, REQ-196, REQ-207, AC-048, AC-051, AC-057, AC-059]
depends_on: [TASK-017]
parent_task: TASK-017
remediates: [FIND-TASK-017-01, FIND-TASK-017-02, FIND-TASK-017-03, FIND-TASK-017-04, FIND-TASK-017-05, FIND-TASK-017-06]
---

# TASK-017 R1 — Make the SDK journeys readable examples

## Authority and subject

- Approved authority: [spec revision 66](../../spec.md).
- Original task: [TASK-017](../../tasks/TASK-017-sdk-test-standard-and-cleanup.md).
- Audit inventory: [SDK test audit](../sdk-test-audit/summary.md).
- Reviewed range: `04d1882dc~1..a97c14f87`. Remediation base: `16532e3a2`
  (lint and type gates already widened in every SDK; do not redo that work).

An independent readability review asked one question of every rewritten test:
would a new developer or data scientist understand the test, and learn how to
use the SDK from it? Most Python and TypeScript stories pass; the Rust stories
and several Python and TypeScript files do not. Revision 66 records the user's
decisions that make the fixes possible. The findings below are the complete
correction set.

## Diagnoses and selected corrections

### FIND-TASK-017-01 — Credentials only from the environment force child processes

**Violated obligation and consequence.** REQ-192 requires deployment-shaped,
in-process public journeys. The CLI functions and Workflow loading read their
credential only from the environment, and Rust cannot change its own
environment soundly in a multi-threaded test. So every Rust journey that acts
as a second principal re-runs its own binary through
`support::is_child()` / `Deployment::run_child`
(`sdks/wyrd-sdk-rust/tests/integration/support/mod.rs:182-230`). A reader sees
branching on process identity, not SDK usage.

**Correction outcome (REQ-196, REQ-207).**
- The networked CLI functions take an optional `client: WyrdClient` that
  replaces `server`. When it's omitted they keep the ambient chain, exactly like
  the executable. Owner: `crates/wyrd/wyrd-cli` (`card.rs` `apply`, `get` and
  the other networked commands).
- Workflow loading accepts an optional client:
  - Rust: `Workflow::from_path_with_client(path, client)`, alongside the
    unchanged `from_path`, in `crates/shared/wyrd-client/src/workflow`;
  - Python: `Workflow.from_path(path, client=None)`;
  - TypeScript: `Workflow.fromPath(path, { client })`.
  Registry refs and Gateway calls run as that client.
- Reuse the existing optional-client pattern (`Bifrost(client=...)`,
  `Cards::with_client`). Do not add a new client type or an optional `Cards`.

**Focused acceptance proof.**
- AC-059: in all three SDKs, a journey loads a Workflow with registry refs
  using an explicit client that has read access, while the ambient credential
  has none.
- In Rust, exactly one ambient-resolution test runs in a child process. No
  other journey calls `run_child`.

### FIND-TASK-017-02 — In-process CLI functions ship in production builds

**Violated obligation.** Revision 66 REQ-196 makes the in-process command
functions a test-only surface. They currently ship in production:
- Python: `wyrd.cli` (`python/wyrd/cli/__init__.py`), built into every wheel;
- TypeScript: in `@wyrd/sdk`;
- Rust: behind the `cli` feature (`sdks/wyrd-sdk-rust/src/lib.rs:39`).

**Correction outcome.** Move them behind each SDK's existing testing gate:
- Python: `wyrd.testing.cli.<command>`, built only with the `testing` feature.
  Production `wyrd.cli` keeps only the executable entry point.
- TypeScript: `cli.<command>` from `@wyrd/testing`. `@wyrd/sdk` exports none of
  them.
- Rust: rename the `cli` feature to `testing`. `wyrd_sdk::cli` exists only with
  that feature.

Don't add a new feature and don't keep an alias. Python `wyrd.testing.cli`
returns typed results, not `dict[str, Any]`. Update the stub sources and
regenerate them; never hand-edit generated stubs.

**Focused acceptance proof.**
- AC-051: in each SDK, a journey runs `apply` and `get` in-process with an
  explicit client.
- Inspection or a unit check shows the production wheel and `@wyrd/sdk`
  expose no in-process command.
- `mise run codegen:check` passes.

### FIND-TASK-017-03 — Rust journeys read as plumbing

**Consequence.** Even apart from FIND-01, the Rust journeys teach internals:
- they import types from `wyrd_client` or `wyrd_spec` instead of `wyrd_sdk`;
- they compare kinds as strings;
- they wrap futures in `Box::pin`;
- `workflow_loading.rs` is one long story.

**Correction outcome.**
- Re-export the needed public types through `wyrd_sdk`. Tests import only
  `wyrd_sdk` and test support.
- Compare `CardKind` values, not strings.
- Remove `Box::pin` wherever plain `.await` works.
- Rewrite `workflow_loading.rs` to match the split in FIND-TASK-017-04.

**Focused acceptance proof.** Rust journeys compile against `wyrd_sdk` alone,
plus `wyrd-testing` support. `mise run test:bifrost` and the Rust SDK journey
lane pass.

### FIND-TASK-017-04 — `test_workflow_loading_journey` is one huge test

**Consequence.** A single function proves thirteen behaviours, one of which is not a server journey at all. A failure
doesn't say which behaviour broke, and nobody can learn one behaviour from it
in isolation.

**Correction outcome.** In all three SDKs, use these same test names:

| Story | Tests |
|---|---|
| workflow_loading | `local_workflow_runs_without_credentials`, `gateway_workflow_without_credentials_is_refused_before_any_step`, `registry_refs_resolve_through_the_registry`, `registry_refs_without_read_access_are_refused`, `local_sibling_never_satisfies_a_registry_ref`, `deleted_registry_card_is_refused`, `applied_workflow_stays_pinned_to_its_registered_cards`, `loaded_workflow_runs_its_pinned_cards`, `loading_a_bad_selector_is_refused` |
| gateway_inference (moved here) | `example_workflow_runs_through_the_wyrd_gateway`, `applying_a_workflow_calls_no_model`, `registered_example_runs_through_the_gateway` |

Move the story's Card YAML to `fixtures/cards/workflow_loading/`.

Delete the external-gateway-binding journey (`test_cards_crud.py:654-678` and
its counterparts in the other SDKs) rather than moving it. An `ext_gateway`
route runs entirely on the client side and never reaches the Wyrd server, and
its fake gateway's random port forces the test to build YAML in code.
`selected_local_dependencies_use_shared_config` in
`crates/shared/wyrd-client/src/workflow/mod.rs` already owns that behaviour
against a mock gateway: the binding is selected from shared configuration, the
secret header is sent, and bad bindings are refused before dispatch.

**Focused acceptance proof.** Each named test runs by its exact selector in
each SDK's journey lane.

### FIND-TASK-017-05 — Cross-SDK stories diverge

**Violated obligation.** REQ-192 requires each story to use the same test names
and fixtures, and to assert the same catalog code, in all three SDKs. They
diverge in these places:
- `refused_cli_command`, `registering_again_is_idempotent` and
  `run_observations_read_back_by_run_id` differ in their bodies or codes;
- loading an Agent as a Workflow is refused with 400 in Rust and 404 elsewhere,
  because Rust passes an Agent-kind selector and the others pass the Agent's
  uid;
- `UID_NOT_RESOLVABLE_HERE` is asserted unevenly;
- TypeScript has six extra selector rows;
- the set of tests per story differs between SDKs.

**Correction outcome.**
- Give each test name one input and one expected catalog code in every SDK.
  For the Agent-as-Workflow test, choose one input.
- Remove or mirror the extra rows.
- List each story's test names in `fixtures/README.md` so drift is visible in
  review.
- Don't add a check script.

**Focused acceptance proof.** For every story, the test names in
`fixtures/README.md` match the test names in all three SDKs, and each test
asserts exactly one code.

### FIND-TASK-017-06 — Patterns REQ-192 prohibits remain in Python and TypeScript

**Python**
- `auth/test_saved_user_auth.py`:
  - remove the uuid suffixes, the message matching and the bare
    `status == 403` checks;
  - use the REQ-195 saved-login fixtures;
  - split the file into five tests: save, expire, refresh, revoke and stale.
- `test_observe_a_run.py`:
  - remove the `json.loads` of results, the OTel protobuf decoding and the
    `while True` retry loop;
  - split the nested and async scope test.
- `test_gateway_inference.py`: remove the `hashlib` digest and the assertion
  that accepts either of two header values.
- Everywhere: replace sentence matching on `details["reason"]` with catalog
  codes or typed fields.
- `unit/state/test_observe_surface.py`: remove the subprocess.
- `state/test_state_journey.py`: replace the ad-hoc fixtures and aliases with
  domain fixtures.

**TypeScript**
- `startTestServer` takes an options object.
- A `registered()` helper replaces the `as CardRef` casts and the `uid ?? ""`
  fallbacks.
- The Service key gets the `agent` role, not admin.
- The observe journey issues a real Card-scoped key.
- The `execFile` test leaves the story set.
- The `fromNative` unit tests are either proven against a real server or
  labelled internal.

**Focused acceptance proof.** A grep of the SDK test trees for `uuid`,
`json.loads`, `while True`, `hashlib`, `subprocess`, `execFile`, `as CardRef`
and `?? ""` finds nothing in story journeys, or only a justified use recorded
in the evidence table.

## Constraints and non-goals

- Keep the server's durable behaviour, the error catalog codes, the Bifrost
  storage and query paths, and the gates as they are. Do not weaken, disable or
  `ignore` any test.
- Keep what TASK-017 delivered: the KEEP and MOVE rows of the audit, and the
  `TESTING.md` checklist.
- Do not add compatibility aliases (`wyrd.cli.apply`, the `server=` argument),
  new features, subprocess CLI use, generated YAML, sleeps or polling loops.
- Rustdoc every changed Rust item, with `# Errors` sections.
- Python tests remain top-level `def test_*` functions.

## Verification and evidence

Record the exact command for every test that is named or added:
- Rust: `mise exec -- cargo nextest run --locked -p <crate> --test <target> -E 'test(=...)'`,
  through the repository's Postgres wrapper where needed.
- Python and TypeScript: the exact pytest or vitest selector.

Then run all of:
- `mise run verify:rust-sdk`, `mise run verify:python-sdk` and
  `mise run verify:typescript-sdk`;
- `mise run test:bifrost`;
- `mise run codegen:check` and `mise run check:deps`;
- `mise run fmt` and `mise run lints`;
- `git diff --check`.

Stop for a new spec revision if a fix needs another public API, security or
persistent-data decision.

## Implementation evidence

Diff: uncommitted on `wyrd/verified-change-contract/TASK-017` over `b2d118450`.
Status: implemented; the cumulative gate is carried into
[TASK-017-R2](../TASK-017-r2/TASK-017-R2-one-client-one-surface.md), which
revision 67 opened from the persona review of this candidate.

| Finding | Implementation | Verification | Result |
|---|---|---|---|
| FIND-TASK-017-01 | `wyrd_cli::commands` networked functions take `client: Option<WyrdClient>`; `Workflow::from_path_with_client` (`crates/shared/wyrd-client/src/workflow/mod.rs`); Python `Workflow.from_path(path, client=None)`; TS `Workflow.fromPath(path, { client })`; Rust journeys use explicit clients | `workflow_loading::registry_refs_resolve_through_the_registry` in all three SDKs; only `otel_export.rs:102` spawns a child (grep of `Command::new\|current_exe` over `sdks/wyrd-sdk-rust/tests`) | PASS |
| FIND-TASK-017-02 | Python `wyrd.testing.cli` (`sdks/wyrd-sdk-python/src/testing_cli.rs`, stubs regenerated); TS `@wyrd/testing` `cli` (`sdks/wyrd-sdk-ts/native-testing/src/cli.rs`, `testing/index.js`); Rust `testing` feature replaces `cli`; `@wyrd/sdk` `index.d.ts` regenerated without `cli*` | `test_production_cli_exposes_only_the_console_script`; TS unit "the published sdk exports no in-process cli"; `mise run codegen:check` exit 0; `mise run verify:python-sdk` (includes `check:py-wheel-no-testing`) exit 0 | PASS |
| FIND-TASK-017-03 | `wyrd_sdk` re-exports `Card`, `Spec`, `VerifierImplementation`, `WorkflowRun`, `WorkflowRunStatus`, `VerifierCounts`, `Judgment`, `VerificationVerdict`, `VerifierKind`, `SessionId`, `QueryTerminalOutcome`; journeys import only `wyrd_sdk` and support; `CardKind` compared; no `Box::pin` | grep: no `wyrd_client::`/`wyrd_spec::` and 0 `Box::pin` under `sdks/wyrd-sdk-rust/tests`; `mise run verify:rust-sdk` exit 0 (61 journeys + 6 identity) | PASS |
| FIND-TASK-017-04 | 9 `workflow_loading` tests and 3 moved `gateway_inference` tests per SDK; Card YAML moved to `fixtures/cards/workflow_loading/`; external-gateway journey deleted | name-parity script over `fixtures/README.md` vs all three SDKs: 0 mismatches; `mise run test:workflow:journey` via `verify:*` lanes | PASS |
| FIND-TASK-017-05 | `fixtures/README.md` lists every story's test names; bodies, inputs, and codes aligned (observe_a_run, saved_user_auth, refused CLI, Agent-as-Workflow uid → 404) | name-parity script: 11 stories × 3 SDKs match exactly | PASS |
| FIND-TASK-017-06 | Python: uuid/message/`status == 403` removed, `details["reason"]` asserts removed, observe split, subprocess removed, state journey fixtures; TS: `startTestServer(options)`, `registered()`, `agent` key, real Card-scoped key, `execFile` out of story set, `fromNative` tests labelled internal | grep of the 33 story files for `uuid\|json\.loads\|while True\|hashlib\|subprocess\|execFile\|as CardRef\|?? ""`: 0 hits | PASS |

Diagnosis — CLI journey lane (`mise run test:cli:journey`):
- **Symptom:** `card_lifecycle::pg_tests::apply_completes_when_completion_decision_audit_fails` panicked `metrics recorder installs` when run after another test in one `cargo test` process.
- **Cause:** `WyrdTestServerBuilder::start_in_process` installs the process-global recorder via `shared_process_telemetry_for_test`; `AuditCommitFailures::install` tried to install a second one.
- **Fix site:** `crates/wyrd/wyrd-testing/src/server.rs` `AuditCommitFailures` borrows the shared capture and counts from a baseline. Its other callers (`audit_publication.rs:542,1146`) are covered by `test:bifrost`. The diagnostician's report concurred.
- **Result:** the lane passed, 36 passed.

Other verification:
- **Passed:** `mise run fmt`, `mise run lints`, `mise run check:deps` and `git diff --check`.
- **`verify:typescript-sdk`:** passed with the regenerated `index.d.ts` staged (17 files, 95 tests). `ts:napi:check` compares against the git index.
- **`mise run test:bifrost`:** FAILED. `production_closeout::compaction_geometry_exact_rows_and_non_destructive_second_pass` saw one hot file. The cause is a production Scribe flush race, not this diff, and the fix is carried to TASK-017-R2 (W3).

Out of scope, recorded: `wyrd_sdk`'s `pub use wyrd_client::*` re-exports `gateway_credential` despite the crate doc.
