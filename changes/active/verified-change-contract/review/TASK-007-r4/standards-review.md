# TASK-007-r4 Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `e2da27694e8a3d057f2ff863c5d17429adc0a495`
- Candidate observed at review start and finish: `e2da27694e8a3d057f2ff863c5d17429adc0a495`
- Scope: repository-rule compliance of the complete cumulative diff. This
  review does not decide task acceptance.

## Authority coverage

| Changed surface | Governing authority inspected | Coverage |
|---|---|---|
| Architecture and active-change records | `AGENTS.md` §§1-2, 14-16; `architecture/wyrd-design.md` Operator connection/delivery contract; `architecture/wyrd-doctrine.mdx`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md` | Complete for the changed design and review records. The candidate keeps Wyrd's Operator vocabulary and records the public contract in the active design authority. |
| `wyrd-spec` Operator, connection, ID, error, and generated-schema contracts | `AGENTS.md` §§2-4, 9, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/positioning-and-vocabulary.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md` | Complete. Pure, synchronous, PyO3-free typed contracts remain in `wyrd-spec`; closed provider/action shapes and stable errors are generated into schemas. |
| Cryptography, OIDC screening, and runtime permissions | `AGENTS.md` §§3-7; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/languages/rust-core.md` | Complete. Envelope cryptography remains in `wyrd-crypt`; screening remains in the existing screened HTTP owner; permission additions remain typed in `wyrd-runtime`. |
| Postgres migration and connection/dispatch queries | `AGENTS.md` §§3-6, 9, 15-16; `architecture/agent-rules.md` SQL/RLS/transaction rules; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md` | Complete. Tenant paths use `TenantConn`, cross-tenant discovery uses `OperatorPool`, RLS is forced, PostgreSQL owns coordination timestamps, and callees do not commit caller-owned transactions. |
| Server CRUD, key resolution, audit, registration validation, worker, provider delivery, configuration, boot, metrics, and health | `AGENTS.md` §§2-6, 9-12, 15-16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md` | Complete. Durable behavior stays server-owned; connection writes compose their allowed audit event in the write transaction; denied decisions fail closed; delivery mechanics do not create audit events; URLs are screened and pinned before credentials are attached. One Rust declaration-style violation remains (`STD-001`). |
| HTTP, shared Rust client, CLI, and MCP | `AGENTS.md` §§2-3, 8-9, 11, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/agent-harness.md`; `architecture/references/languages/errors.md` | Complete. All surfaces project the shared typed contract and permissions. One MCP descriptor contradicts its response contract (`STD-002`). |
| Rust, Python, and TypeScript SDKs, PyO3/napi boundaries, exports, declarations, and stubs | `AGENTS.md` §§2-3, 7-9, 11, 16; `architecture/agent-rules.md`; `architecture/references/languages/pyo3-boundaries.md`; `architecture/references/languages/python-api-and-stubs.md`; `architecture/references/languages/typescript-guide.md`; `architecture/references/languages/errors.md` | Complete. Each binding delegates transport and durable semantics to `wyrd-client`; PyO3 remains in the Python SDK; public exports and generated typing artifacts are present. |
| Rust, SQL, HTTP, MCP, CLI, Python, and TypeScript tests; code generation and `mise` tasks | `AGENTS.md` §§8, 11-12, 16; `architecture/agent-rules.md`; `architecture/references/languages/testing-workflows.md` | Complete. Real-server journeys exist for the public SDK/CLI/MCP surfaces, Postgres tests are in `pg_*` targets, and codegen owns schemas/stubs. The credentialed live-provider smoke remains an explicitly gated release check. |

## Applicable rule results

| Rule | Evidence | Result |
|---|---|---|
| Server owns durable connection, encryption orchestration, authorization, tenancy, audit, and delivery behavior. Clients remain projections over `wyrd-client`. | `components/operators/service.rs`, `verification/operators.rs`, and `wyrd-sql` own the behavior; `wyrd-client::OperatorConnections` is the single transport handle used by all three SDKs and CLI. | PASS |
| `wyrd-spec` is IO-free, async-free, SQL-free, and PyO3-free. | New connection/Card/error/ID contracts contain only validation, serialization, schema, and pure rendering logic. Boundary checks passed. | PASS |
| Tenant SQL uses `TenantConn` without manual tenant predicates; cross-tenant work uses `OperatorPool`; callees do not end caller transactions. | `operator_connections.rs` tenant queries rely on `wyrd.current_tenant()` RLS; only key-version discovery and due-tenant discovery use `OperatorPool`; transaction commits remain in server owners. `check:tenant-isolation` passed. | PASS |
| RLS and privilege boundaries are explicit for durable tenant state. | Migration enables and forces RLS on `wyrd.operator_connections`, grants tenant CRUD to `wyrd_app`, and grants the admin role only the two columns required for rotation discovery. | PASS |
| Authorization decisions are audited once at their commit boundary and fail closed. Engine mechanics are not audit. | Writes use `authorize_recording_denial`, append the allowed event on the same `TenantConn`, and use `record_unless_committed` for pre-commit failure. Reads use the canonical `audit::authorize`. Dispatch claims, settlements, and rewrap work emit no canonical audit. | PASS |
| Secrets never enter public reads, diagnostics, CLI argv, or unredacted `Debug`; key/provider failures expose stable safe detail. | `OperatorConnectionView` contains only nonsecret authority; secret-bearing request types use `SecretBearer`; CLI reads bodies from file/stdin; sealed/key/config debug implementations redact; decode failures name only position/category. | PASS |
| Every external URL is resolved, screened, and pinned before credential attachment, with redirect re-screening. | `OperatorDelivery::client` delegates to `ScreenedHttp::client_for`; `http` builds each hop from that client before `Credential::attach`; same-origin redirects loop through screening again; provider posts use the same screened client path. | PASS |
| Async is limited to IO/composition, with bounded external calls and concurrency. | Pure validation/rendering stays synchronous; key reads and provider attempts have timeouts; claim work is globally and per-tenant permitted; response bodies and redirects are bounded. | PASS |
| New/materially changed Rust declarations use top-level imports and bare type names. | Candidate-added declarations still contain qualified types in fields, parameters, and returns; see `STD-001`. | **FAIL** |
| HTTP/MCP/CLI/SDK surfaces describe and project the same typed contract. | Operations and wire shapes align except the MCP list descriptor promises a `secret_version` absent from `OperatorConnectionView`; see `STD-002`. | **FAIL** |
| Public errors use stable catalog variants and map consistently across surfaces. | Operator connection/key errors are derive-backed `WyrdError` variants; SDK error-code projections include them; handlers return the common response mapper. | PASS |
| Python/PyO3 and TypeScript/napi boundaries stay thin and generated artifacts remain synchronized. | Wrappers decode at the edge and call `wyrd-client`; `check:pyo3-scope`, `check:client-tier`, and `codegen:check` passed. | PASS |
| User/agent-facing capability has real client-to-server journeys and supporting seam tests. | Rust, Python, TypeScript, CLI, and MCP journey files are present; route/SQL/delivery tests exercise authorization, tenant isolation, encryption, retries, fencing, rotation, and SSRF. Available implementation records report the broader lanes green. | PASS, subject to the verification limits below |

## Material repository-rule findings

### `STD-001` — Candidate-added Rust declarations still bypass module import manifests

- Classification: **VIOLATION**
- Violated authority: `architecture/agent-rules.md` requires types to be
  imported in the top-of-module `use` block and used as bare names in fields,
  parameters, return types, trait bounds, and `where` clauses. `AGENTS.md` §16
  makes repository Rust structure and style a hard completion requirement.
- Locations and evidence:
  - `crates/wyrd/wyrd-server/src/components/operators/keys.rs:235` stores
    `Option<reqwest::Client>`, and line 550 returns `sqlx::Error`, despite the
    module already having an explicit dependency manifest.
  - `crates/wyrd/wyrd-server/src/verification/operators.rs:628`, lines
    732, 762, 777-778, 975, and 992 use qualified `serde_json`, `reqwest`, or
    `std::collections` types in method signatures or fields.
  - `crates/wyrd/wyrd-server/src/verification/operators/slack.rs:29` and
    `pager_duty.rs:28` return `serde_json::Value` without importing `Value`.
  - `crates/wyrd-spec/src/card/operator.rs:251`, line 531, and its new test
    helpers at lines 550 and 555 use qualified `serde_json::Value` in fields or
    signatures.
  - `crates/wyrd/wyrd-sql/src/queries/operator_dispatches.rs:374` names
    `wyrd_spec::ids::IdError` in a closure parameter instead of importing the
    domain error beside the other ID types.
  - `crates/wyrd/wyrd-testing/src/server.rs:576`, lines 664-668, 3479, and
    3808 add qualified `wyrd_server`, `tempfile`, and `std::path` types to
    fields and signatures. These are cumulative-candidate additions, not
    merely older drift.
- Observable consequence: the candidate does not satisfy the repository's
  explicit declaration/import acceptance rule. Dependencies are hidden at use
  sites instead of being readable from the module manifest, and the R3 import
  cleanup closes only a subset of the same violation.
- Testable correction: add the existing types to each owning module's top
  import block (using aliases only where names collide), replace every
  candidate-added qualified field/signature/declaration occurrence with the
  bare name, and keep expression-only macro/function paths unchanged when no
  import improves the declaration. Verify by source inspection over the full
  base-to-candidate additions, then run `mise run fmt` and `mise run lints`.

### `STD-002` — MCP advertises a response field the typed response never returns

- Classification: **INCORRECT**
- Violated authority: `AGENTS.md` §§2 and 9 and
  `architecture/references/languages/agent-harness.md` require agent-facing
  descriptions and schemas to project the same typed contract precisely.
- Location: `crates/wyrd/wyrd-server/src/mcp/operators.rs:80-82` says the list
  result contains “secret version.” The actual output is
  `ListAnswer { connections: Vec<OperatorConnectionView> }`, and
  `OperatorConnectionView` contains connection ID, name, provider/nonsecret
  config, status, and timestamps, but no secret-version field.
- Observable consequence: an MCP client following the tool description can
  plan around a field that can never exist, while the generated output schema
  correctly omits it. This makes the machine-facing catalog internally
  contradictory.
- Testable correction: delete “and secret version” from the list descriptor;
  do not expose the internal secret version. Extend the existing MCP discovery
  assertion to pin that the descriptor names only fields present in the typed
  redacted response, then run the focused MCP journey.

## Verification limits

- Independently run during this review, all exit 0: `mise exec -- cargo fmt
  --all -- --check`, `mise run check:client-tier`, `mise run
  check:pyo3-scope`, `mise run check:unwrap-audit`, `mise run
  check:tenant-isolation`, `mise run codegen:check`, and `git diff --check
  f8811ac5..e2da2769`.
- The cumulative task and remediation implementation records report the SQL,
  shared, server, journey, CLI, MCP, Python, TypeScript, typecheck, lint, and
  focused Postgres lanes green. This reviewer did not rerun those longer lanes
  within the 20-minute sub-review budget.
- The live Slack/PagerDuty smoke was not run because credentials are
  intentionally absent from repository lanes; it remains gated release
  evidence.
- Passing automated gates do not detect either retained finding: the qualified
  declaration rule is broader than rustfmt/current lint enforcement, and
  codegen correctly preserves the typed MCP response while the prose
  descriptor remains wrong.

## Overall result

**FAIL**

The security, tenancy, durability, audit, ownership, SDK-boundary, and testing
rules inspected are satisfied, but two bounded repository-rule violations
remain. Both corrections reuse existing imports, typed response contracts, and
journey coverage; neither requires a new abstraction, dependency, or product
decision.
