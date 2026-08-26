# Testing Workflows

Use this reference with
`architecture/references/languages/implementation-execution.md`. A bounded
task runs task-defined focused verification with default or exact features. A
milestone or phase may add the default-feature workspace gate. Treat
all-feature workspace checks and `mise run pre-pr` as whole-plan closeout
gates, not per-task defaults.

## Three Tiers (Priority Order)

Wyrd has three test tiers, ranked. Higher tiers prove the product works;
lower tiers prove a part works. **A lower tier never substitutes for a
missing higher one.**

1. **User-journey tests — the primary contract.** Drive the real SDK
   against a real server (`WyrdTestServer` + embedded Postgres) along a
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

Run verification for the code you changed. Use the narrowest `mise` task
that covers the touched surface:

### Format and lint

```bash
# Bounded task: run only commands required by its affected surface.
mise run fmt            # when Rust changed
mise run py:format      # when Python changed
mise run py:lints       # when Python changed
mise run py:typecheck   # when Python public typing changed

# Milestone or phase integration:
mise run check:default  # fmt check + workspace Clippy, default features

# Whole-plan closeout:
mise run check          # fmt check + workspace Clippy, --all-features
```

Use the focused lint command named by the active task. Do not substitute
`check:default` for missing task-level lint scope; return an under-specified
task for correction when choosing a broader gate would materially expand
verification.

### Rust crate changes

Prefer crate-family `mise` tasks (they set up env, migrations, and
fixtures). Use raw `cargo test` only for narrow pure unit tests with no
repo setup.

```bash
mise run test:wyrd            # wyrd/* family (no DB)
mise run test:skald           # skald/* family (no DB)
mise run test:vala            # vala/* family (no DB)
mise run test:shared          # shared/* family (no DB)
mise run test:sql             # live Postgres SQL integration tests
mise run test:bifrost         # vala-bifrost integration tests
mise run test:bifrost:journey # Rust bifrost user-journey tests/multi-pod distributed tests
                              # (capability binaries + registration rule:
                              #  crates/wyrd/wyrd-testing/tests/README.md)
mise run test:e2e             # server-level e2e (wyrd-auth, wyrd-server, wyrd-testing, wyrd-client, vala-sdk)
mise run test:storage:matrix  # storage emulator matrix (S3/GCS/Azure)

# Narrow single-test iteration:
mise exec -- cargo test --locked -p <crate> <test_name> -- --nocapture --test-threads=1
```

`--all-features` in bounded-task, milestone, or phase commands forces the heavy
feature union to recompile and defeats artifact reuse. Prefer default features
or the exact feature set the changed behavior needs. The integrated whole-plan
`check` verifies the workspace feature union once at closeout.

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

### Contract / schema / MCP / stub changes

```bash
mise run codegen:check   # fail if openapi / schemas / MCP / pyi drift
mise run codegen:regen   # regenerate everything from source
```

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
| `check:from-pools-allowlist` | `from_pools` called only at 16 allowlisted sites |
| `check:fixtures-no-server` | `wyrd-dev-fixtures` does not import `wyrd-server` |
| `check:no-legacy-server-vocab` | Reject legacy vocabulary + orphan-rule violations |
| `check:no-tonic-outside-wyrd-tonic` | Reject tonic-family deps outside `wyrd-tonic` + workspace pins |
| `check:test-coverage` | Every crate assigned to exactly one family test lane |
| `check:py-wheel-no-testing` | Production `py-wyrd` wheel does not expose `wyrd.testing` |
| `check:error-coverage` | Skald error codes mapped; SQL errors have coverage |
| `check:design-sync` | `ValaQueryService` + payload-read permissions match `wyrd-design.md` |
| `check:single-into-response-impl` | HTTP errors flow through one server `IntoResponse` mapper |
| `check:proto-drift` | `wyrd.v1` FileDescriptorSet matches `.proto` |
| `check:tokens` | Generated theme CSS matches `palette.json` |

## Aggregate CI Gate

`mise run pre-pr` runs the full battery: the all-feature `check`, `test:unit`,
`test:bifrost`, `codegen:check`, `cardkind:check`, `check:design-sync`,
every boundary gate above, `py:setup`, `py:format:check`, `py:lints`,
`py:typecheck`, `py:test:unit`, plus example / docs / vocab gates.

Run `pre-pr` after a complete integrated plan when the plan requires it, or
when the change modifies shared CI/build/test infrastructure, prepares a
release, or the user explicitly asks for it. It is a final confidence sweep,
**not** the normal bar for a task, milestone, or phase.

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
