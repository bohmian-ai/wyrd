# Repository standards review — TASK-003 r4

## Subject and scope

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-verification-closeout-task3`
- Immutable base: `7f79fb341`; candidate: `f6c841d57` (220 changed files).
- Evidence supplied: `mise run -c gate` exit 0 on 2026-10-09; Scribe 28/28, Forge 22/22, Oracle 50/50; codegen, format, lints, and diff check pass. I inspected the source and task evidence; I did not repeat those lanes.
- `.codegraph/` is absent, so repository navigation used `git diff` and `rg`.

## Authority coverage

| Changed surface | Governing authority inspected | Standards result |
|---|---|---|
| Wyrd protocol, Service table declarations, gateway correlation, schemas, errors, examples and docs | `AGENTS.md` §§2, 3, 8, 9, 11, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; references `doctrine/positioning-and-vocabulary.md`, `doctrine/architecture-constraints.md`, `architecture/patterns.md`, `languages/errors.md`, `languages/agent-harness.md`, `languages/spec-driven-development.md` | **FAIL**: current design authority still names `card_ref` as the observation correlator in several current passages (RS-2). Typed contracts and generated schemas otherwise follow the changed UID surface; `codegen:check` was reported green. |
| Gateway, auth issuance, verification, outbox, server boot and SQL | `architecture/wyrd-security-posture.md`; `AGENTS.md` §§4–6, 9–10; `architecture/agent-rules.md`; references `languages/rust-core.md`, `architecture/patterns.md`, `domain/evaluation.md` | PASS on inspected tenancy and permission shape. Changed production SQL accepts `TenantConn` or `OperatorPool`, including `ForgeTasks` and `VerifierRuns`; raw `PgPool` hits were test fixtures or private transaction helpers. Gateway UID authorization uses a tenant connection before dispatch. |
| Bifrost Scribe, Gate, Oracle, Forge, telemetry, storage and lifecycle | `architecture/bifrost-design.md`; `AGENTS.md` §§4–6, 10–11; references `domain/vala-architecture.md`, `domain/telemetry-observations.md`, `domain/olap-serving.md`, `domain/iceberg.md`, `domain/datafusion.md`, `domain/arrow-analytical-interop.md`, `domain/analytical-operations-reliability.md` | PASS on the inspected ownership, async and tenant-capability rules; runtime correctness is for the implementation, system and domain reviews. |
| Rust client, SDK, examples, test helpers, Rust tests | `AGENTS.md` §§3–5, 11, 16; `architecture/agent-rules.md`; references `languages/rust-core.md`, `languages/maintainer-style.md`, `languages/testing-workflows.md` | **FAIL**: two newly added or materially modified functions lack mandatory rustdoc (RS-1). The support-desk story is wired into the Rust journey lane. |
| Python SDK, PyO3 wrappers, stubs, tests, examples | `AGENTS.md` §§7–8, 11, 16; references `languages/pyo3-boundaries.md`, `languages/python-api-and-stubs.md`, `languages/testing-workflows.md` | PASS on inspected public export, runtime ownership and generated-stub parity; reported `codegen:check` and gate cover the generated and typecheck lanes. |
| TypeScript SDK, N-API, declarations, tests, example | `AGENTS.md` §§3, 11; references `languages/typescript-guide.md`, `languages/testing-workflows.md` | PASS on inspected package/declaration and journey wiring; reported gate covers the SDK lane. |
| `mise.toml`, Postgres scripts, docs-site and test infrastructure | `AGENTS.md` §§11–12, 16; `architecture/agent-rules.md`; references `languages/testing-workflows.md`, `languages/implementation-execution.md` | PASS on inspected gates. `verify:rust-sdk` excludes `signed_in_development` while `test:identity:journey:inner` selects that module. Reported gate includes docs, examples, codegen, Rust, SDK and Bifrost lanes. Existing `#[ignore]` convention is used for gated server journeys. |

## Material findings

### RS-1 — Missing rustdoc on touched Rust functions

- **Rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` rustdoc rule require documentation for every new or materially modified Rust item, including test helpers. Fallible functions require `# Errors`; async operations document relevant side effects.
- **Location:** `crates/shared/wyrd-client/src/bifrost/mod.rs:152` (`card`) and `examples/support-desk/rust/main.rs:13` (`main`). The former changed from `CardRef` to `CardUid` in this range; the latter is a new binary entry point. Neither has an item rustdoc comment. `main` also returns a fallible result and has no `# Errors` section.
- **Consequence:** A maintainer has no item-level description of the test UID's role or the binary's deployment/verification workflow and failure boundary. The repository explicitly calls missing rustdoc a hard blocker even when gates pass.
- **Testable correction:** Add concise rustdoc to those two functions; explain the UID fixture's role and `main`'s workflow and error conditions. No behavior change or new test is needed.

### RS-2 — Current architecture authority contradicts the UID correlation contract

- **Rule:** `AGENTS.md` §§1–2 and 9 place `architecture/wyrd-design.md` above code and generated contracts; public documentation must align with the typed wire contract. The changed Bifrost design and telemetry reference state that observation correlation uses `card_uid`.
- **Location:** `architecture/wyrd-design.md:132` still says an observation carries `card_ref` and the server maps it to `card_uid`; lines 762, 881–887 still describe the `(card_ref, run_id)` observation pair and `card_ref` as the row subject. `architecture/references/domain/evaluation.md:13` likewise says runtime observation `card_ref` supplies subject identity. In the same changed design, lines 811–870 now prescribe a client-supplied `card_uid` authorized directly against signed UID scope or the tenant registry.
- **Consequence:** The active design authority gives two incompatible instructions to SDK and ingest implementers. Following the remaining `card_ref` passages produces a wire value that the new correlation surface no longer accepts; the contradiction also obscures how verification routing obtains the subject.
- **Testable correction:** Reconcile those current observation/correlation passages with the approved UID contract, preserving `card_ref` only where it identifies a Card in registry or declaration semantics. A source search of the active design and evaluation reference should then find no assertion that the observation wire or stored row uses `card_ref` for correlation.

## Overall result

**FAIL** — RS-1 is a hard rustdoc gate; RS-2 leaves the changed public design authority internally inconsistent. No other material repository-standard violation was established by this pass.
