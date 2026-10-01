---
id: TASK-001-HUMAN
kind: remediation
directed_by: human (Steven Forrester), 2026-09-25
---

# Human-directed additions to TASK-001

Both items were decided by the human; they are in scope for TASK-001 review.

## 1. Platform login refuses a cleartext discovered authorization endpoint

`crates/wyrd/wyrd-auth/src/platform_login.rs` sends the browser to the
provider-discovered `authorization_endpoint` without the scheme screening that
tenant login applies (R3, FIND-TASK-001-20). Apply the same existing shared
scheme rule before any login state is persisted or a redirect is returned, so
production (`BlockInternal`) refuses `http://` and the local/test
`AllowInternal` policy keeps working. Reuse the tenant-login mechanism; add no
second scheme rule.

Acceptance: a production-policy platform login whose discovered authorization
endpoint is `http://` is refused with the same public error as tenant login and
persists no state; `https://` succeeds; existing platform login tests pass.
Cover it with the narrowest test that proves platform login calls the check.

## 2. Delete `scripts/checks/no-legacy-server-vocab.sh`

Decision: retire the check (AGENTS.md §12). Its non-final `! rg` lines could
never fail under `set -e`, so it has protected nothing, and its `\bScope\b`
rule conflicts with legitimate OTLP `Scope` names. Delete the script and every
reference to it (mise.toml tasks/aggregates, CI workflows, docs, TESTING.md,
any other check lists). Record in the commit message which check was removed,
the property it claimed, and why it is retired.

Acceptance: `rg no-legacy-server-vocab` finds nothing outside git history;
the aggregates that referenced it still run.

## Verification

- `mise run fmt`, `mise run lints`
- The exact focused nextest command for each new or changed platform-login test
- `mise run test:principals:integration`
- Any `mise` aggregate that previously referenced the deleted check
