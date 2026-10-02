# TASK-004 round-2 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `362878494ed80ca5c5533a4364f744bf92dd1e06`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, with
  revisions 8 and 9 superseding conflicting TASK-004 text
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-004-laptop-clients.md`
- Binding human direction:
  `changes/active/oidc-production-readiness/review/TASK-004-r1/human-direction-FIND-TASK-004-4.md`,
  including every addendum

Revision 10's REQ-021 is owned by TASK-008 and was excluded. The candidate
remained `HEAD` through discovery, follow-up, validation, and verdict
preparation. `.codegraph/` is absent, so reviewers used the cumulative diff,
`rg`, and direct source/caller inspection.

## Reconciled acceptance matrix

| Obligation | Reconciled implementation and proof | Result |
|---|---|---|
| REQ-011 RFC 8628 device login | The custom handoff is removed. Device authorization, browser approval, polling cadence, denial, expiry, one-use redemption, and token-free CLI output use the shared server and CLI owners. | PASS on ordinary paths; FAIL on terminal-state issuance race (`FIND-TASK-004-12`) and Windows browser launch (`FIND-TASK-004-14`) |
| REQ-012 saved credential store | One `credentials.toml` preserves other content, uses a blocking lock and atomic replacement, selects the newest login by default, and refreshes under the lock. | FAIL — unsafe group-writable directory and non-origin key (`FIND-TASK-004-9`, `FIND-TASK-004-10`) |
| Credential precedence and tenant selection | Explicit and environment tiers remain ordered; tenant is the approved route-key selector, is refused beside self-naming bearer/API-key credentials, and all SDKs delegate to shared Rust behavior. | PASS |
| Secret-bearing client transport | `TokenExchange` reuses the shared transport policy and rejects lowercase remote HTTP. | FAIL — mixed-case HTTP bypass and redirect replay remain (`FIND-TASK-004-5`, `FIND-TASK-004-13`) |
| Concurrent renewal and crash behavior | Revision 8's conventional lock/reread/refresh/save flow is implemented; the deleted generation, pending, tombstone, and custom deadline mechanisms remain absent. | PASS |
| Logout and revocation | Local deletion precedes best-effort server revocation; only the selected refresh chain is revoked; canonical audit shares the transaction. | PASS |
| First-class Rust, Python, and TypeScript clients | Runtime constructors and declarations carry the tenant selector and the real journeys pass through the shared owner. | PASS, with stale public/test documentation (`FIND-TASK-004-8`) |
| Persistent schema evolution | Fresh databases reach the candidate schema. | FAIL — two registered migrations were rewritten, so a base deployment cannot upgrade (`FIND-TASK-004-11`) |
| Non-goals and standard practice | No second IdP app, provider-token authority, language-specific credential store, custom renewal state machine, live-provider qualification matrix, or REQ-021 work is required. | PASS |

## Independent review results

| Report | Result |
|---|---|
| `task-review-behavior.md` | PASS — no proposed findings |
| `task-review-invariants.md` | FAIL — origin normalization and directory trust |
| `standards-review.md` | FAIL — stale revision-8 documentation |
| `maintainer-review.md` | FAIL — stale device-login wrapper documentation |
| `system-review.md` | PASS — no proposed findings |
| `domain-review-security.md` | FAIL — TLS spelling bypass, redirect replay, Windows command injection |
| `domain-review-concurrency-durability.md` | PASS — no proposed findings |
| `domain-review-data-tenancy.md` | FAIL — migration mutation and device lifecycle race |
| `followup-review.md` | RESOLVED — confirmed the device race, origin leak, and writable-directory race |
| `findings-validation.md` | FIX_REQUIRED — eight retained findings |

All required reviewers and reports were available. Follow-up was required
because the data/tenancy lifecycle result conflicted with the system,
concurrency, and invariant reviews, while the origin and directory results
conflicted with other domain assessments. The focused reviewer resolved all
three conflicts from source. The structured Ponytail reviewer then validated
every proposed finding independently, deduplicated documentation claims, and
rejected any correction that would restore superseded custom mechanisms.

## Validated finding ledger

- `FIND-TASK-004-5` — REVISED: remote cleartext refusal is bypassable by
  standards-valid mixed-case HTTP spelling.
- `FIND-TASK-004-8` — CONFIRMED: changed SDK documentation describes the
  removed ambiguity error and custom CLI handoff.
- `FIND-TASK-004-9` — CONFIRMED: the saved-login key retains path and userinfo
  instead of using a canonical origin, and CLI output can expose userinfo.
- `FIND-TASK-004-10` — CONFIRMED: a group-writable configuration directory
  defeats the user-only credential boundary.
- `FIND-TASK-004-11` — CONFIRMED: registered migrations were rewritten instead
  of evolved through a later migration.
- `FIND-TASK-004-12` — REVISED: a terminal device authorization can still
  leave renewable Wyrd authority issued before token-endpoint redemption.
- `FIND-TASK-004-13` — CONFIRMED: secret-bearing OAuth POSTs follow 307/308
  redirects across origins.
- `FIND-TASK-004-14` — CONFIRMED: Windows browser opening interprets the
  server-returned verification URL through `cmd.exe`.

Decision-complete traces, corrections, rejected alternatives, and focused
proofs are in `findings-validation.md` and are packaged in
`TASK-004-R2-production-readiness-fixes.md`.

## Verification limits

- The orchestrator independently ran `git diff --check` and
  `mise run test:identity:journey`; both exited 0. The identity aggregate
  included the migration check, server identity suite, CLI device journey,
  shared-client concurrent renewal, and Rust, Python, and TypeScript saved-user
  journeys.
- The task packet records the remaining format, lint, codegen, boundary,
  client, server, Python, TypeScript, docs, and typecheck lanes as passing.
- Green lanes do not exercise mixed-case HTTP, cross-origin 307/308 replay,
  Windows shell interpretation, base-ledger migration upgrade, the approval
  race, URL userinfo/path normalization, or a group-writable directory. They
  also reproduce stale generated prose when its source is stale.
- REQ-021 request encoding and OAuth response envelopes remain TASK-008 scope,
  not a TASK-004 verification limit.

## Prior-finding closure

| Prior finding | Round-2 disposition |
|---|---|
| `FIND-TASK-004-1` | CLOSED under the tenant-key-only human direction |
| `FIND-TASK-004-2` | CLOSED by revision 8, which removed generation/per-use revalidation |
| `FIND-TASK-004-3` | CLOSED by revision 8, which removed pending/lock-timeout semantics and accepted replay containment |
| `FIND-TASK-004-4` | CLOSED by the one-file/no-encryption human direction and revision 8 |
| `FIND-TASK-004-5` | OPEN, REVISED — the root TLS defect remains through mixed-case spelling |
| `FIND-TASK-004-6` | CLOSED — Python runtime and declaration projection is complete |
| `FIND-TASK-004-7` | CLOSED — logout revocation and audit are transactional |

## Verdict

**FIX_REQUIRED**

Eight bounded implementation findings remain. Each correction uses an existing
owner and a standard or repository-native mechanism. No approved behavior or
expensive-to-reverse decision needs revision.
