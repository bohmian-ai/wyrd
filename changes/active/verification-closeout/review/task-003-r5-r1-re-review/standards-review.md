# TASK-003 R1 repository standards review

**Result: FAIL**

## Subject and authority

- Immutable base `7f79fb3417db651adedac194ada8908f0a0372d7`; candidate `9a8f9f7eef95f70d356c037a192b7d7b90a37f31`.
- Reviewed the cumulative diff and used `f6c841d57..9a8f9f7ee` to locate R1 changes. Read `AGENTS.md`, `architecture/agent-rules.md`, the reference router, applicable language, testing, Bifrost, observation and evaluation guidance, approved spec, original task, prior verdict and R1 remediation task. No `.codegraph/` index exists. Source remained unchanged.
- The R1 evidence reports focused journeys and checks passing; this standards review did not rerun those lanes. `git diff --check 7f79fb341..9a8f9f7ee` passed here.

## Authority coverage

| Changed surface | Governing rules | Standards result |
|---|---|---|
| Gateway, Card, Run, verification, shared outbox and their Rust/Python/TypeScript contracts in the cumulative diff | `AGENTS.md` §§2–4, 7–11, 16; agent rules for audit, generated files, tenant SQL and rustdoc; Wyrd design/doctrine; language/error/testing references | PASS on inspected boundaries; generated parity supported by recorded `codegen:check`, not rerun here |
| Bifrost Scribe/Oracle/Forge, SQL, lifecycle and tests | `AGENTS.md` §§4–6, 11, 15–16; agent rules for SQL types, in-module tests, imports, rustdoc; Bifrost design; Rust, OLAP and analytical reliability references | FAIL: new Forge field has no rustdoc; import placement note below |
| Three support-desk examples and SDK journeys | `AGENTS.md` §§3, 8–11, 16; Python API/stubs, TypeScript, Rust, and testing references | PASS for R1 deployment-pair check and three language journey coverage from source and recorded proof |
| Postgres roles script | `AGENTS.md` §§11–12; agent rules on gates; existing `test:postgres:roles` task | PASS: second container `psql` invocation has `ON_ERROR_STOP=1` |
| Active architecture and reference prose | `AGENTS.md` §§1–2, 9; Wyrd design, doctrine, telemetry and evaluation references | PASS: observation correlation now uses `card_uid`; legitimate registry/principal `card_ref` remains |
| Changed Rust fixture and example entry point | `AGENTS.md` §16; agent rules on rustdoc; maintainer-style reference | PASS: `bifrost::tests::card` and fallible support-desk `main` have item docs and `# Errors` on `main` |

## Applicable rule checks

| Rule | Source evidence | Result |
|---|---|---|
| Every new or materially modified Rust item, including fields, has rustdoc | `crates/vala/vala-bifrost-redux/src/forge/worker.rs:715` declares a new tuple field with no field-level rustdoc; surrounding type and methods are documented | **FAIL** |
| Imports live at module top, barring the specified narrow exceptions | `crates/vala/vala-bifrost-redux/src/oracle/admission.rs:2529` adds a function-local `use datafusion::execution::memory_pool::MemoryConsumer;` inside a test; the test module already has a top-level import block | FAIL, non-blocking placement note |
| New Rust SQL capability signatures use tenant/operator connection types, with no raw pool | Changed `vala-sql::ForgeTasks` reclaim method uses its existing `OperatorPool` owner; cumulative SQL diff adds no raw `PgPool`, caller transaction, or `TenantConn::commit`/`rollback` signature | PASS |
| User-facing behavior is proved through the owning runtime | R1 adds provider collision/refusal checks to Rust, Python, and TypeScript real-server support-desk journeys; late-Scribe journey starts with no Scribe node | PASS by source; recorded focused runs passed |
| Public declarations and generated contracts track source | Cumulative diff updates wire source and generated schema/stub/declaration files; task records passing `codegen:check`, language typechecks and OpenAPI journey | PASS by recorded evidence, not independently regenerated |
| Rustdoc closure from prior round | `wyrd-client/src/bifrost/mod.rs:152–158` and `examples/support-desk/rust/main.rs:12–21` now document the touched functions, with `# Errors` for `main` | PASS |
| Shell role idempotency rerun propagates SQL error | `scripts/postgres/test-roles.sh:29` invokes container `psql ... -v ON_ERROR_STOP=1`; task records successful roles lane and exit 3 on deliberate SQL error | PASS |

## Material finding

**RS-R1-1 — Mandatory rustdoc missing on the new Forge quiescence field.** `AGENTS.md` §16 and `architecture/agent-rules.md` explicitly require rustdoc on every new Rust field, including private fields, and make missing rustdoc a hard acceptance blocker. `crates/vala/vala-bifrost-redux/src/forge/worker.rs:715` adds `struct ForgeLoopQuiescence(Arc<AtomicBool>);` without documentation on its tuple field. The type-level prose explains the mechanism but does not document the field item itself. Add field-level rustdoc naming that the atomic flag is shared across supervised worker incarnations and means the prior loop joined when true. This is a static correction; no test is needed.

## Non-blocking notes

- Move the test-local `MemoryConsumer` import at `oracle/admission.rs:2529` to the test module's top import block to follow `agent-rules.md`. It has no behavioral consequence and does not independently require a `FIX_REQUIRED` verdict.
- In `examples/support-desk/python/support_desk.py:33–35`, adding `PROVIDER` between `REQUESTS` and its string documentation makes the following string describe the wrong constant. This is a documentation placement error in an example, with no runtime or public contract consequence.

No additional SQL capability, generated artifact, test-tier, or tenant-isolation violation was established from the inspected cumulative owners and R1 changes.
