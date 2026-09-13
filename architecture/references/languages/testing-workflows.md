# Testing Workflows

Use this reference with
`architecture/references/languages/implementation-execution.md`. Run the
narrowest complete verification set for the changed surface. Capability-scoped
`verify:<scope>` tasks are the local and pull-request default; `mise run gate`
is the nightly, release, and conservative fallback aggregate.

## Three Tiers (Priority Order)

Wyrd has three test tiers, ranked. Higher tiers prove the product works;
lower tiers prove a part works. **A lower tier never substitutes for a
missing higher one.**

1. **User-journey tests — the primary contract.** Drive the real SDK
   against a real server (`WyrdTestServer` + repository-managed Postgres) along a
   complete user/agent path, client → server → client, no in-process
   engine fixtures. Cover the happy path **and** the edge and negative
   flows a real caller hits (lazy vs eager instantiation, schema
   fingerprint conflict, under-privileged token → rejection, oversized
   query → floor rejection, replayed batch → no double-write). Cover every
   user-facing surface the capability ships — Rust, Python, and TypeScript
   SDKs, and the MCP/HTTP path when the capability is agent-facing.
   Journeys run in a gated lane (`integration` pytest marker; Rust
   `e2e`/`#[ignore]`; the repo TypeScript integration task) so the fast
   lane stays credential- and server-free.
2. **Integration tests — supporting.** Exercise one subsystem against its
   real dependency (a handler against Postgres, the ingest service against
   the writer) without standing up the full journey. Use them to pin a
   seam contract precisely where a full journey would be noisy.
3. **Unit tests — supporting.** A single function or type in isolation,
   IO-free, credential-free, in the fast lane. Use for pure logic,
   error/`WyrdError` mapping, and negative branches that are cleaner to
   force in-process than end-to-end.

Rule: every new user/agent-facing capability ships a user-journey test.
Pushing a user-observable behavior — especially a negative flow — down to
a unit test *only* is a coverage gap to flag in review.

## Verification Scope

Verification has three levels:

1. **Bounded task:** run only focused commands for the affected behavior and
   exact optional features.
2. **Milestone integration:** run the combined lanes for the integrated
   dependency closure.
3. **Whole-plan closeout:** run the repository aggregate required by the
   approved plan.

Pull requests select only the lanes associated with affected code and its
dependency closure. The complete non-credentialed correctness suite runs
nightly on `main`, including Rust, SQL, Python, TypeScript, integration,
user-journey, Bifrost cluster, identity, and storage-emulator lanes. Live-cloud
and performance/qualification suites run on separate schedules.

Use the narrowest `mise` task that covers the touched surface:

## Repository-managed Postgres and production-shaped harnesses

`wyrd-dev-fixtures::PgFixture` owns one isolated ephemeral database per test.
The least-privilege `wyrd_test_admin` role alone creates and drops databases;
`wyrd_migrator` owns and migrates schemas but cannot create or drop databases.
`PgFixture::attach` lets child processes reuse the parent-created database
without remigration, reseeding, or cleanup authority. Attached fixtures are
non-owning and cannot destroy the parent database.

`wyrd-testing` is the authority for `WyrdTestCluster`, production-shaped server
and multi-process fixtures, Forge fixtures, Bifrost telemetry capture, and the
capability-target Bifrost journey suite. Do not replace those paths with
globally serialized shared-container fixtures or in-process engine substitutes.
The retired Bifrost benchmark/qualification harness layer and its removed
public testing APIs, fixtures, scripts, docs, and `mise` tasks are not part of
the current testing architecture and must not be restored from an older branch.

### Format and lint

```bash
mise run fmt            # when Rust changed
mise run lints          # workspace Clippy with repository feature policy
mise run py:format      # when Python changed
mise run py:lints       # when Python changed
mise run py:typecheck   # when Python public typing changed
```

`mise run check` is the format-check plus workspace Clippy aggregate. Use it
when the affected scope calls for that combined check. There is no separate
aggregate for a default-feature-only workspace pass.

### Rust crate changes

Use crate-family `mise run` tasks for crate-, module-, family-, environment-,
and aggregate-level coverage; they set up required env, migrations, and
fixtures. Every specifically named Rust test uses an exact task-recorded
`mise exec -- cargo nextest run` command.

```bash
mise run test:wyrd            # complete wyrd/* default-feature family
mise run test:skald           # skald/* family (no DB)
mise run test:vala            # complete vala/* default-feature family
mise run test:shared          # complete shared/* default-feature family
mise run test:sql             # live Postgres SQL integration tests
mise run verify:bifrost       # complete Bifrost checks and all test tiers
mise run test:bifrost         # all Bifrost tests and language surfaces
mise run test:bifrost:journey # Rust bifrost user-journey tests/multi-pod distributed tests
                              # (capability binaries + registration rule:
                              #  crates/wyrd/wyrd-testing/tests/README.md)
mise run test:e2e             # server-level e2e (wyrd-auth, wyrd-server, wyrd-testing, wyrd-client)
mise run test:storage:matrix  # storage emulator matrix (S3/GCS/Azure)

# Narrow named lib test:
mise exec -- cargo nextest run --locked -p <crate> --lib \
  -E 'test(=module::tests::test_name)'

# Narrow named integration-test target:
mise exec -- cargo nextest run --locked -p <crate> --test <target> \
  -E 'test(=test_name)'
```

Confirm exact target and test names from source and, when needed,
`mise exec -- cargo nextest list`. Include repository-managed setup in the
command for tests that require Postgres, storage emulators, or a live server.
Do not use a positional filter that can pass after selecting no test.

`--all-features` in test commands forces the heavy feature union to recompile
and defeats artifact reuse. Prefer default features or the exact feature set
the changed behavior needs. Repository lint/type-check tasks own their declared
feature policy.

### Python changes

```bash
mise run py:setup             # sync deps + maturin develop
mise run py:test:unit         # Python unit tests (no TF marker)
mise run py:test:integration  # wyrd.testing journeys against live Postgres
mise run py:test:testing      # wyrd.testing harness suite
mise run py:typecheck         # after stubs/exports/typing changes
```

### TypeScript changes

```bash
mise run ts:build             # napi addon build
mise run ts:typecheck
mise run ts:test:unit         # @wyrd/sdk auth-parity + error envelope
mise run ts:test:integration  # in-process WyrdTestServer client→server journey
mise run ts:napi:check        # napi-generated index.d.ts committed
```

### Contract, schema, OpenAPI, or stub changes

```bash
mise run codegen:check   # fail if OpenAPI / JSON schema / public pyi drift
mise run codegen:regen   # regenerate everything from source
```

Runtime MCP catalogs are verified through their owning MCP tests, not through
the code-generation snapshot.

## Boundary Checks

Run the check that matches the edit. One line each so you know which gate
a specific change will trip:

| Gate | What it enforces |
|---|---|
| `check:client-tier` | Foundation invariants (wyrd-spec server-tier-free; shared shells pyo3+sqlx-free; Skald locked Wyrd edges) |
| `check:pyo3-scope` | PyO3 stays out of pure contracts and Python-free shared crates |
| `check:mocks-scope` | Mock helpers stay out of production source |
| `check:unwrap-audit` | Audits `unwrap()`/`expect()` outside tests |
| `check:tenant-isolation` | SQL foundation tenant isolation + server-tier boundaries |
| `check:registry-no-server-routes` | `wyrd-sql` does not import `axum`/`hyper`/`tower` |
| `check:registry-tx-coupling` | Card registry writes stay inside caller's `TenantConn` tx |
| `check:registry-immutable-spec-hash` | `wyrd.cards` trigger raises `P0001` |
| `check:registry-single-table` | Single `wyrd.cards` table; no per-kind shadow tables |
| `check:object-store-pin` | Single versions of `object_store`/`datafusion`/`arrow`/`parquet` |
| `check:from-pools-allowlist` | Pool construction remains limited to sanctioned production and fixture boundaries |
| `check:fixtures-no-server` | `wyrd-dev-fixtures` does not import `wyrd-server` |
| `check:no-legacy-server-vocab` | Reject legacy vocabulary + orphan-rule violations |
| `check:no-tonic-outside-wyrd-tonic` | Reject tonic-family deps outside `wyrd-tonic` + workspace pins |
| `check:test-coverage` | Every crate assigned to exactly one family test lane |
| `check:py-wheel-no-testing` | Production `wyrd-sdk-python` wheel does not expose `wyrd.testing` |
| `check:error-coverage` | Skald error codes mapped; SQL errors have coverage |
| `check:design-sync` | `ValaQueryService` + payload-read permissions match `wyrd-design.md` |
| `check:single-into-response-impl` | HTTP errors flow through one server `IntoResponse` mapper |
| `check:proto-drift` | `wyrd.v1` FileDescriptorSet matches `.proto` |
| `check:tokens` | Generated theme CSS matches `palette.json` |

## Aggregate CI Gate

`mise run gate` runs the broad repository battery: workspace checks, unit and
Bifrost tests, code generation, boundary invariants, Python and TypeScript
checks, examples, and documentation checks. Nightly, release, mixed, global,
and unclassified changes own that cost. Capability-scoped changes run their
complete `verify:<scope>` task; unknown paths conservatively fall back to
`gate`.

Run all Cargo-backed commands sequentially across agents sharing a checkout or
target directory. Parallel source work must not create overlapping Cargo
builds, tests, lints, docs, or codegen processes.

## Test Design

- Exercise public or crate-visible behavior.
- Cover success, stable failures, and edge cases; assert Wyrd error codes.
- Use local fixtures and mock services (`RustFS`/`fake-gcs`/`Azurite` for
  storage; local Postgres via `PgFixture`; mocked providers).
- Do not require credentials for unit tests.
- Keep generated output drift-free.
- Do not broaden tests into slow integration gates unless the touched
  behavior requires it.

## Never Circumvent A Gate

Do not weaken or disable a check, add `#[allow]`, delete or `#[ignore]` a
failing test, or broaden a boundary glob to hide a real violation. Fix the
underlying cause. Only use a check's own sanctioned mechanism (e.g. the
documented per-file allowlist) when the usage is legitimately test-only
and matches existing in-pattern precedent.
