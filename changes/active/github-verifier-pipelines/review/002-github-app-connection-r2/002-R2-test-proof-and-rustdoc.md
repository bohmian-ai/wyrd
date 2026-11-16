---
id: TASK-002-R2
kind: remediation
status: proposed
parent_task: TASK-002
approved_spec_revision: 5
findings: [FIND-002-10, FIND-002-17]
---

# Finish the GitHub connection test proof and helper documentation

## Authority and subject

- Approved [spec revision 5](../../spec.md) and [contracts](../../contracts.md).
- Original [task 002](../../tasks/002-github-app-connection.md) and [R1 remediation](../002-github-app-connection-r1/002-R1-conventional-github-app.md).
- Original base `395274d58bf76a2075f245ca3a5c1bd044925027`.
- Prior reviewed candidate `3dd1555a0f40117ee512ecaaff1f5b51845ef3b6`.
- This reviewed candidate `78052b910d8d8e3fbfd88d4e1a8c35a555c9c652`.
- Review [validation](findings-validation.md), [discovery diagnosis](remediation-review.md) and [independent execution](verification.md).

Implement this bounded packet through `$wyrd-implement`. The runtime corrections have no retained findings. Keep the accepted GitHub.com flow, SQL fences and provider handling intact.

## FIND-002-10 — Complete the new fallible helper's error contract

Root `AGENTS.md` and `agent_docs/rust.md` require `# Errors` for fallible changed items, including private test helpers. New `Connected::mint_while` in `crates/wyrd/wyrd-server/tests/integration/pg_github_connection.rs:499–516` returns the actual result of `GithubConnections::installation_token` but its documentation describes only the interleaving. It omits the error contract. This leaves the existing explicit rustdoc gate incomplete even though compilation and lint checks pass.

Add the missing error documentation beside this helper, describing the delegated token refusals/provider/database failures that it returns after the held operation. Preserve its signature and behavior. This is a few lines at the existing owner; no helper split, new checker or test is needed. Batch it with the assertion correction below. Review the actual helper's panic/cancellation behavior when documenting it without introducing new runtime behavior.

Acceptance: the helper has accurate `# Errors` documentation and retains the existing interleaving behavior. The other original rustdoc corrections carry forward as accepted.

## FIND-002-17 — Measure the five-minute expiry against the request interval

The required SDK acceptance proof must reliably test the configured lifetime. `sdks/wyrd-sdk-rust/tests/integration/github_connection.rs:185–190` samples `begun_at` before `admin.sdk.begin().await`, then asserts that `receipt.expires_at - begun_at` is at most five minutes. The SQL producer correctly derives expiry from its later `statement_timestamp()` plus the unchanged five-minute `LOGIN_STATE_TTL`. Ordinary request latency can therefore make the asserted interval exceed five minutes despite correct production behavior.

The review's exact SDK selector failed with `PT300.000649823S` at line 188; the same source passed on a fresh isolated lifecycle. The discovery report independently traces service `connect`/`begin` to `insert_github_login_state`, SQL `statement_timestamp() + ($13 * interval '1 second')` and the bound duration. This is a timing-dependent test defect, with no production lifetime defect alleged. A passing rerun leaves the unreliable assertion in place and can interrupt required acceptance coverage before the connection paths run.

Bracket the existing SDK begin request with client timestamps before and after it. Keep the existing meaningful lower lifetime bound against the before timestamp, and check the five-minute upper bound against the after timestamp. The server's SQL statement occurred within that request interval. Correct the assertion at its existing test owner; do not change the production duration, SQL time authority, SDK result or connection assertions. Do not add sleeps, automatic retries, arbitrary tolerance, a timing framework or artificial host load. The small assertion correction removes the false failure while retaining proof of the configured lifetime.

Acceptance: the exact SDK case completes, retains the lifetime lower bound and five-minute upper bound with the request interval accounted for, and still checks both ordinary connection entry paths and their existing durable outcomes. Preserve the observed failure and diagnosis in the evidence record; no retrospective failing-first claim is needed.

## Preserved behavior and focused proof

Do not change production source, public contracts, fixtures, provider scope, authorization, signing, token fences, webhook observations or the other accepted assertions. No specification decision remains. Removed provider scope supplies no work.

For FIND-002-10 inspect the changed helper's documentation directly; compiler success alone cannot prove that requirement. Run `mise run fmt:check` and `mise run lints` for the changed test items. For FIND-002-17 run only its exact existing selector with repository-managed Postgres and migrations:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=github_connection::pg_tests::connections_associate_only_after_github_confirms_them)"'
```

Record the exact result and `mise exec -- git diff --check`. No broad suite, live provider run, new diagnostic agent or mutation campaign is required. A later task review checks these two corrections and carries forward this round's accepted runtime and specialist evidence.

## Obligations

| ID | Source | Planned assertion |
|---|---|---|
| R2-10a | FIND-002-10 acceptance | `Connected::mint_while` documents `# Errors` as the delegated `installation_token` refusals, provider and registry failures |
| R2-10b | FIND-002-10 "review the actual helper's panic/cancellation behavior" | Docs state the panic propagated from `during` and the wait when the mint never reaches GitHub's token request |
| R2-10c | FIND-002-10 "preserve its signature and behavior" | Diff touches doc comments only |
| R2-17a | FIND-002-17 lower bound | Expiry is more than four minutes after the pre-request sample |
| R2-17b | FIND-002-17 upper bound | Expiry is at most five minutes after the post-request sample |
| R2-17c | FIND-002-17 "still checks both ordinary connection entry paths and their existing durable outcomes" | Wyrd-begun and GitHub-begun assertions are unchanged and run to completion |
| R2-17d | FIND-002-17 "preserve the observed failure and diagnosis" | Evidence records the review's failure and cause |
| R2-P | Preserved behavior | No production source, contract, fixture or other assertion changed; no sleep, retry, tolerance or harness |

## Evidence

Commits `8bcd1acbc` and the evidence commit that follows it; the second also prints both intervals in the expiry failure message, which the checker flagged. Abbreviations: SRV = `crates/wyrd/wyrd-server/tests/integration/pg_github_connection.rs`; SDK = `sdks/wyrd-sdk-rust/tests/integration/github_connection.rs`.

| Obligation | Implementation evidence | Assertion | Verification evidence | Result |
|---|---|---|---|---|
| R2-10a: delegated error contract | SRV:505–509 names `GithubConnections::installation_token`, conflict, not-connected, inaccessible, unavailable and registry errors, matching its `# Errors` at `service.rs:715–722` | Doc inspection against `service.rs:715–722` and SRV:518–528 returning `minted` unchanged | Direct inspection; `mise run lints` exit 0 | PASS |
| R2-10b: panic and wait behavior | SRV:502–503 (wait when the token request is never reached: `hold.arrived()` never fires); SRV:511–512 (`during` panic propagates through `tokio::join!`) | Doc inspection against SRV:520–527 | Direct inspection | PASS |
| R2-10c: behavior preserved | `git diff 78052b910 -- SRV` adds only `///` lines | Diff audit | `git diff 78052b910 --stat`: 12 insertions, 0 deletions | PASS |
| R2-17a: lower bound | SDK:185 `sent_at` before `begin()` | SDK:189 | Exact selector below: PASS 1/1 | PASS |
| R2-17b: upper bound | SDK:187 `answered_at` after `begin()`; SQL `statement_timestamp()` lies within `[sent_at, answered_at]` | SDK:190 | Exact selector below: PASS 1/1 | PASS |
| R2-17c: entry paths kept | Unchanged Wyrd-begun path (SDK:195–251) and GitHub-begun path (SDK:253–284) | SDK:219–226, 243–251, 263–284 | Same run reached completion | PASS |
| R2-17d: failure and diagnosis | Below | — | Recorded | PASS |
| R2-P: preserved behavior | Diff covers the two test files only; no production, fixture or contract path | `git diff 78052b910 --stat` (excluding the user's uncommitted skill/workflow edits) | `mise run fmt:check`, `mise run lints`, `git diff --check` all exit 0 | PASS |

**Observed failure (FIND-002-17).** The review's exact selector failed at SDK:188 with `PT300.000649823S`. Cause: `begun_at` was sampled before the request. Expiry is `statement_timestamp() + 300s`, taken later inside the request, so request latency alone pushed the measured interval past five minutes. The fix keeps the lower bound against the before sample and checks the upper bound against the after sample. No failing-first rerun was required.

**Commands.**

```bash
mise run fmt:check                                                     # exit 0
mise run lints                                                         # exit 0
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=github_connection::pg_tests::connections_associate_only_after_github_confirms_them)"'
# PASS [1.585s] after the message change (first run 1.556s) wyrd-ai::integration github_connection::pg_tests::connections_associate_only_after_github_confirms_them — 1 passed
mise exec -- git diff --check                                          # exit 0
```

**Checker.** A fresh read-only checker read the task file, this ledger and the diff. It reported no gaps and confirmed the line references, the rustdoc against `HeldAnswer` and `tokio::join!` behavior, and the soundness of the bounds. It noted that the upper-bound failure message printed the wrong interval; the message now prints both.
