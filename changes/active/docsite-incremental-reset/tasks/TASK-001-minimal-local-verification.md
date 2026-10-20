---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-docsite-incremental-reset
spec_revision: 2
requirements: [REQ-002, REQ-003, REQ-004, INV-002, INV-003, INV-004, INV-005, AC-002, AC-003]
depends_on: []
parent_task:
remediates: []
---

## Outcome and Value

After starting a released local Wyrd server once, a developer runs a complete
Python script beside one YAML declaration file and sees attributable passing
and failing verification results. The same script can run again without
manual cleanup. This delivers the executable source and installation evidence
that TASK-002 teaches; it does not publish unfinished instructions.

## Owners, Scope, Consumers, and Prohibited Changes

Use `$wyrd-implement`. The example and its tests are Wyrd-owned. Keep the two
authored files small: YAML holds the minimum subject and assertion Verifier;
Python declares, performs required SDK loading, supplies JSON, and prints the
result. Generated bundles are permissible, but cannot become extra authored
prerequisites or require a separate deployment script.

The existing CLI owns installation, startup, saved credentials, and connection
settings. Existing Cards, WyrdState, and observation APIs own declaration,
hydration, and verification. No new SDK convenience API, server feature,
transport, credential bypass, custom analytical table, or application runtime
is authorized. PostgreSQL and the running server remain explicit prerequisites.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Start once and connect later | `crates/wyrd/wyrd-cli/src/server/dev.rs`: `DevArgs::run`, `DevServer::save_settings`; Python CLI entrypoint in `sdks/wyrd-sdk-python/pyproject.toml` | `tests/integration/test_local_development.py`: guided setup and standalone saved-setup journeys; current Setup | Tests of source-built tools do not prove public acquisition instructions | Reuse `wyrd server dev` and saved client configuration; separately exercise released bytes | None; retain existing lifecycle and configuration owners |
| Declare and hydrate exact Cards | `crates/shared/wyrd-client/src/cards/handle.rs`: `Cards::declare_from_path`, `Cards::pull`; `state.rs`: `WyrdState::from_path`; public Python stubs under `python/wyrd/cards` and `python/wyrd/state` | `tests/integration/test_readme.py`: file-backed declarations and exact source execution; `test_support_desk.py`: copied local application | Current examples require a larger application or fragmented source | Add a minimal independent YAML/Python example using these public calls | No alternate loader or bundle lifecycle |
| Judge supplied JSON and distinguish refusal | `crates/shared/wyrd-client/src/observe/mod.rs`: `Run`, `Observe::verify` | `tests/integration/test_verify_in_real_time.py`: pass, fail, exact identity, denied invocation | Existing tests do not execute the new published sample | Add a dedicated documentation journey under the existing Python integration runner | A separate integration test module earns its place by driving the real server without importing an application example |

Python test paths above are relative to `sdks/wyrd-sdk-python/`. Exact sample
location and private test support remain implementation-owned.

## Approach

1. Confirm a compatible published SDK/CLI/server set and record artifact URLs,
   versions, platform, Python version, and PostgreSQL prerequisites. Version
   0.2.0 is a candidate, not proof of runtime compatibility.
2. Exercise the proposed clean-install startup, connection, stop/restart,
   persistence, and reset instructions against released artifacts with an
   isolated local configuration. Do not touch a developer's existing database.
3. Build the two-file example through the ordered scenarios below, reusing
   existing public SDK behavior and the Python real-server harness.
4. Record exact invocation, expected output, meaningful errors and recovery,
   and the distinction between supplied evidence and measured application
   behavior for TASK-002. Preserve useful application regression coverage.

## Ordered Implementation Scenarios

The selectors below are **planned new tests**, not claims that they already
exist. Their runner, integration marker, and Postgres wrapper come from the
current Python integration owner and `mise.toml`. Implement them under
`sdks/wyrd-sdk-python/tests/integration/test_docsite_quickstart.py`; private
fixture shape is not prescribed.

### Scenario 1 — One local sample produces both judgments

**Behavior.** Execute the sample's actual YAML and Python against
`WyrdTestServer` and repository-managed Postgres, with no provider credentials
or application checkout. Assert passing and failing verdicts, exact subject
and Verifier identities, and requirement reasons (REQ-003/004, AC-002/003).

**RED.** Add `test_docsite_quickstart_verifies_and_repeats`; execute its first-run
assertions before writing the sample and capture the missing behavior. After
the setup commands in Verification, run:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_docsite_quickstart.py::test_docsite_quickstart_verifies_and_repeats
```

**GREEN.** Supply the smallest complete declarations and script through the
existing SDK. Inspect the returned results; do not wait for or assert immediate
analytical query completeness.

**REFACTOR.** Remove unnecessary sample setup and reuse existing client and
test owners while the focused journey remains green. Do not conceal necessary
auth or server prerequisites behind a tutorial-only runtime.

### Scenario 2 — Running the script again requires no cleanup

**Behavior.** Repeat the same sample in the same environment and directory;
it still succeeds with exact identities and leaves a usable local state
(REQ-003, AC-002/003).

**RED.** Extend `test_docsite_quickstart_verifies_and_repeats` with the second
execution. Run the exact Scenario 1 command and establish any declaration or
bundle-replacement gap. If existing behavior already satisfies it, record
that green evidence rather than deliberately breaking production code.

**GREEN.** Use existing declaration/idempotency and bundle replacement behavior
to make both executions succeed. Rerun Scenario 1's assertions as part of the
same test; do not delete persistent state to manufacture a clean rerun.

**REFACTOR.** Keep only the file and lifecycle handling required by the SDK;
avoid a separate deployment framework or new idempotency mechanism.

### Scenario 3 — Insufficient authority is a refused request

**Behavior.** With the subject already declared, an authenticated client
without the required invocation permission receives the stable refusal,
distinct from an ordinary failed verdict (INV-003, AC-002).

**RED.** Add `test_docsite_quickstart_refuses_insufficient_authority`, exercising
the sample's verification path with insufficient invocation authority. It
must reach the relevant server permission check, not fail earlier because a
fixture could not load or declare. After setup, run:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_docsite_quickstart.py::test_docsite_quickstart_refuses_insufficient_authority
```

**GREEN.** Preserve the SDK's structured error propagation and check the actual
permission refusal. Rerun the Scenario 1 command; do not convert refusals into
synthetic verdicts or grant broader rights to pass the test.

**REFACTOR.** Reuse the current harness's scoped credentials and simplify
sample/test code while both exact commands stay green.

## Acceptance Criteria

- The reader authors only one YAML file and one complete Python script; the
  script declares and performs required loading without manual deployment.
- Both returned judgments expose exact identities, verdict, and reason; the
  failing judgment is an ordinary successful response.
- A second execution works; insufficient invocation authority is refused.
- A fresh walkthrough proves acquisition and startup with a compatible
  released set, then executes these same files. Source-build or testing-wheel
  success alone does not satisfy this criterion.
- Server stop/restart, persistence, and destructive reset boundaries have
  accurate evidence; no provider, application, UI, or future CLI flag is needed.
- Existing application regression coverage remains intact.

## Expected Write Set and Consumer Closure

Minimal local sample source, the Python integration journey, and narrowly
needed test/task wiring. Likely sample home is under `examples/`, compatible
with `CodeFromFile`; exact location is not fixed. Tests belong to the Python
SDK runtime owner. Source consumers in TASK-002 and CI consumers in TASK-004
must use the same sample. Server/client production code and public stubs are
outside this task's material write scope.

## Verification and Evidence

For the repository regression journey, use the current owner setup:

```bash
mise run py:setup
mise run build:server
```

Run both exact selectors above, then the focused module:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_docsite_quickstart.py
mise run py:format
mise run py:lints
git diff --check
```

Apply the same pinned Ruff rules to the chosen sample location and record its
exact command. Record actual commands and results for a separate clean
released-artifact walkthrough; that environment must not import `wyrd.testing`
or use pytest's source-tree `pythonpath`. The regression harness can use its
testing wheel, but that is not public installation proof. Check artifact
availability for all three supported targets; report actual runtime coverage
by platform without claiming unexecuted coverage. Reproduce failures with
`WYRD_LOG` tracing before diagnosing them.

This task adds a Python documentation scenario, not a new product capability;
it does not require unrequested Rust/TypeScript tutorial expansion. No broad
journey aggregate belongs in this task.

## Material Stop Conditions

- No compatible released set supports the exact journey: finish unaffected
  proof and report a publication blocker; do not substitute source builds.
- The two-file experience needs SDK/server semantics, new contracts, relaxed
  permissions, or a new lifecycle owner: report the precise gap for authority.
- Avoiding an unsupported claim would require changing the agreed tutorial
  outcome rather than correcting its explanation.

## Authority Links

- [Approved spec revision 2](../spec.md)
- [Repository standards](../../../../AGENTS.md) and [agent rules](../../../../architecture/agent-rules.md)
- [Protocol](../../../../architecture/wyrd-design.md) and [doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Spec workflow](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)
- Use `$wyrd-implement`; consult the security and operations authorities before
  publishing their corresponding claims.
