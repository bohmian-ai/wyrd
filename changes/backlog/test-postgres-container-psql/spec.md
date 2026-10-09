---
id: SPEC-test-postgres-container-psql
revision: 1
status: draft
---

# Run test role bootstrap with the container's psql

## Intent

Make repository-managed Postgres tests independent of the host's `psql` version. This backlog draft does not authorize implementation.

## Problem

Commit `4ae6a1992` changed `crates/wyrd/wyrd-sql/bootstrap/roles.sql` to use `\getenv`, which requires psql 15 or later. `scripts/postgres/with-test-postgres.sh` currently runs that file with the first `psql` on the host `PATH`. On a host with Homebrew `postgresql@14`, role bootstrap fails before the requested test command starts. Psql 14 rejects `\getenv` even in the `\if` branch skipped by the wrapper's `--set` variables. Selecting psql 17 manually on `PATH` works around the failure but leaves the shared test wrapper host-dependent.

## Required behavior

- **REQ-001:** The test Postgres wrapper runs `roles.sql` with `psql` from its own Postgres Compose container, so the bootstrap client matches the test server version regardless of host `PATH`.
- **REQ-002:** Role bootstrap still applies the configured test role passwords, stops on SQL errors, and fails the wrapper before invoking the requested test command when bootstrap fails.
- **REQ-003:** Existing database URLs and the isolated Compose lifecycle continue to work for the invoked test command.

## Scope and constraints

- The change belongs to shared test infrastructure in `scripts/postgres/with-test-postgres.sh`, outside verification-closeout TASK-003's write set.
- Keep `roles.sql` compatible with its existing deployment contract. Do not require a host `psql` installation for the test wrapper.
- Do not expose role passwords in test output or weaken bootstrap error handling.

## Acceptance

- **AC-001:** With psql 14 first on host `PATH`, a repository-managed Postgres test lane completes role bootstrap and runs its test command.
- **AC-002:** A bootstrap error exits nonzero without running the test command, and the Compose project is cleaned up.

## Authority and revision

- `AGENTS.md` §11; `scripts/postgres/with-test-postgres.sh`; `crates/wyrd/wyrd-sql/bootstrap/roles.sql`; `docker-compose.yml`.
- Revision 1 — draft, 2026-10-09: record the host psql 14 bootstrap failure and container-client fix.
