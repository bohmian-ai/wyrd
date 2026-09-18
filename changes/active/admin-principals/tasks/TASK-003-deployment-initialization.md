---
task: TASK-003
title: Deployment initialization
spec: SPEC-admin-principals
spec_revision: 6
obligations: [REQ-020, REQ-021, REQ-022, REQ-023, REQ-024, INV-005, INV-002, AC-001]
depends_on: [TASK-001, TASK-002]
---

## Objective

An operator establishes the deployment's root of trust with one command. A
`wyrd-server` subcommand, authorized by possession of the deployment's database
credentials, creates the global administrative principal, generates its initial
credential, persists only the verifier, grants platform authority, marks the
installation initialized, and prints the plaintext exactly once to the invoking
operator's terminal. Server start creates no administrative state and emits no
credential material, so the root credential never enters the log pipeline.

## Constraints

- Server start must not initialize, generate, or print credential material under
  any deployment profile.
- Initialization is one transaction. Concurrent and repeated invocation converge
  on exactly one global administrative principal and one initial credential; a
  second invocation refuses rather than minting another root or re-exposing the
  existing one.
- A failed initialization leaves the deployment uninitialized and retryable —
  never a principal with no usable credential, and never a credential whose
  plaintext was not exposed.
- The plaintext is written to the invoking command's stdout only. It is never
  logged, traced, or recorded in an audit payload.
- An uninitialized server starts and serves normally, refusing
  platform-control-plane operations with a stable error.
- Local development stays a single command with no manual secret capture.
- Non-goal: platform OIDC, human platform principals, tenant creation.
- Non-goal: an unauthenticated initialization HTTP endpoint.

## Relevant Surface

- `crates/wyrd/wyrd-server/src/main.rs` — subcommand dispatch.
- `crates/wyrd/wyrd-server/src/boot/` — boot-time operations that open their own
  pools outside the serving path; `bootstrap.rs` is the shape precedent this
  replaces in `TASK-004`.
- `crates/wyrd/wyrd-sql/src/postgres.rs` — `connect_from_dsns`, migration and
  pool construction ordering.
- `crates/wyrd/wyrd-auth/` — credential issuance from `TASK-001`.
- `mise.toml` — the local development task that currently runs
  `cli:bootstrap-key`.
- `docs/src/content/docs/self-hosting/` — running the server, local development.

## Approach

1. Add the initialization operation as server-owned behavior that runs on its
   own pool outside the serving path, following the existing boot-command shape.
2. Make it transactional and safe under concurrency, using a durable
   single-initialization guard rather than a read-then-write check.
3. Expose it as a `wyrd-server` subcommand returning the plaintext for one
   print, and refuse cleanly when already initialized.
4. Make the serving path refuse platform-plane operations while uninitialized,
   with a stable error and no enumeration.
5. Point the local development task at the new command so one command still
   yields a working local deployment.

## Acceptance Criteria

- On a fresh deployment the command creates exactly one global administrative
  principal holding platform authority, exactly one credential, and prints the
  plaintext once with explicit non-retrievable guidance.
- A second invocation refuses, creates nothing, and re-exposes nothing.
- Concurrent invocations converge on one principal and one credential; the
  losing invocation refuses rather than creating a duplicate.
- An injected failure at each stage leaves the deployment uninitialized and a
  later invocation succeeds cleanly.
- Starting the server — before and after initialization, under every deployment
  profile — emits no credential material to stdout, logs, or traces.
- An uninitialized server serves ordinary routes and refuses platform routes
  with a stable error.
- The resulting credential authenticates and yields a platform-scope
  authenticated context.

## Verification

Scope is `VER-001` through `VER-006`.

```bash
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-server --all-targets
```

Initialization is durable behavior, so its proof is a Postgres-backed
integration test run through `scripts/postgres/with-test-postgres.sh`, covering
fresh initialization, refusal on re-run, concurrent invocation, staged failure,
and absence of credential material in captured server output. Run it through a
focused `mise` lane for this capability and through the exact nextest
expressions for the tests you add.
