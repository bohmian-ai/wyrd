# Data-domain review — TASK-006 R7

**Immutable subject:** `f8811ac5035c3aa165d34c38992f9889b3c9081f..f3c65147e0fd828fe5c657d2871ff80f9b3d5543`. **Result: PASS.**

## Boundary and authority coverage

| Boundary | Source and verification inspected | Authority |
|---|---|---|
| Role bootstrap and database credentials | `crates/wyrd/wyrd-sql/bootstrap/roles.sql`; `scripts/postgres/test-roles.sh`; development and production Kubernetes guides; R6 task and recorded walkthrough | Approved spec rev 44 REQ-156–158, AC-035; R6 FIND-31; `architecture/wyrd-security-posture.md` credential and SQL boundaries |
| Owner-only migration and lease | `wyrd-server/src/main.rs:185-224`; `wyrd-sql/src/lib.rs:47-234`; `wyrd-sql/src/operator_pool.rs`; `vala-sql/src/lib.rs`; prior data verdicts and migration proof | REQ-157/158, AC-035; R5 FIND-20; `architecture/agent-rules.md` pool rules |
| Serving RLS, schema readiness and Eval persistence | `wyrd-sql/src/postgres.rs`, `schema_check.rs`, `queries/verifier_runs.rs`; Wyrd and Vala migration SQL; cumulative diff and R6 data review | REQ-077/083–085/156–158, AC-014/016/035; `architecture/wyrd-design.md`, `architecture/bifrost-design.md`, `AGENTS.md` §§5, 9, 11 |

## Assessment

No material data-domain finding. The R6 change to `roles.sql` reads the two serving-role passwords with psql `\getenv` unless throwaway-test `--set` values were supplied. The documented production path passes the owner password through `PGPASSWORD`, passes role-bootstrap passwords through the environment, and sends the Secret values to `kubectl` by file descriptor. The role SQL still repairs role attributes and membership and grants only database `CONNECT` to the two serving logins. The updated role test exercises the environment channel; the recorded walkthrough checks all three logins, certificate and hostname rejection, migration, and password-free `psql`/`kubectl` argument lists.

The one-off migrate command still constructs an owner-backed `OperatorPool`, acquires its detached session advisory lease, runs Wyrd then Vala migrations and schema validation, and closes that pool. Serving constructs separate app and platform-admin pools, verifies exact logins, migration checksums, grants and forced tenant RLS, and never receives the owner URL. The R6 delta does not alter those SQL or Eval persistence paths. Prior focused migration, `test:sql`, and startup evidence applies to those unchanged boundaries.

## Verification limits

This is a source and recorded-evidence audit; I did not rerun Postgres tests or the Kubernetes walkthrough. The R6 task records `test:postgres:roles` and `test:postgres:contract` passing for the changed bootstrap path. No material proposed finding.
