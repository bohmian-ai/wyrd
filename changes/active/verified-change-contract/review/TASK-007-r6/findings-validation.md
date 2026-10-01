# TASK-007 R6 Structured Ponytail Validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `cd002ab1394cdc1679d95f957313a7f697d7e0a5`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 36
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Remediation inputs: TASK-007-R1 through TASK-007-R5 and their prior verdicts and validated ledgers
- Wave 1 inputs: `task-review.md`, `standards-review.md`,
  `domain-review-security.md`, and `domain-review-delivery.md` in this directory

The candidate was the stated repository `HEAD` before and after validation.
The uncommitted R6 reports are outside the immutable subject. The checkout has
no `.codegraph/` directory, so caller tracing used the candidate source and
`rg`.

## Wave 1 proposal decisions

All four Wave 1 reviewers proposed no material findings. I independently
validated that explicitly empty union rather than treating agreement as proof.

| Wave 1 report | Proposed material findings | Validation decision |
|---|---:|---|
| Task implementation | 0 | **VALIDATED EMPTY** — every TASK-007 obligation and non-goal is mapped to reachable source and credible proof; no missing, incorrect, drifting, violating, or regressed behavior was found. |
| Repository standards | 0 | **VALIDATED EMPTY** — the changed surfaces retain their required ownership, tenancy, audit, async, import, documentation, generated-artifact, and test shape. |
| Security domain | 0 | **VALIDATED EMPTY** — the live key, connection, permission, audit, redaction, SSRF, and credential-attachment paths retain the approved controls. |
| Delivery durability domain | 0 | **VALIDATED EMPTY** — the live settlement, claim, lease, retry, deadline, drain, restart, and rewrap paths retain their database-owned bounds and fencing. |

No proposal was rejected, revised, or retained because there was no proposed
finding union. No `FIND-TASK-007-20` is assigned.

## Independent source and caller validation

### Production key-source policy

The complete live path is
`WyrdServerConfig::load` → `WyrdServerConfig::validate` →
`OperatorKeysConfig::validate` → boot state attachment through
`OperatorKeys::for_role`/`OperatorKeys::new` → connection seal/open and
multi-tenant readiness through `OperatorKeys::verify_active`.

- `crates/wyrd/wyrd-server/src/config.rs:1905-1959` rejects
  `OperatorKeySource::Env` for every production deployment, retains Vault-only
  multi-tenant production, and retains file or HTTPS Vault for single-tenant
  production. The validator reads no key, so it does not turn the approved
  deferred single-tenant provider check into a boot-time availability check.
- `crates/wyrd/wyrd-server/src/config.rs:2878-2890` calls that validator only
  for API-serving roles, preserving the deliberate non-API role boundary.
- `crates/wyrd/wyrd-server/src/components/operators/keys.rs:251-306` keeps the
  same concrete owner and returns the already established typed construction
  failures; no new source, trait, factory, feature, or configuration knob was
  added.
- `crates/wyrd/wyrd-server/src/boot/mod.rs:1458-1470` retains the separate
  active-tenant readiness proof for multi-tenant production.
- `config::tests::operator_key_source_follows_deployment` exercises the six
  material deployment/source combinations at the validator itself.

Ponytail result: the correction stops at the first shared owner that holds —
one policy guard in the existing validator. Moving it into every key read or
adding another policy abstraction would duplicate a resolved decision.

### Oversized provider retry

The complete live path is provider response classification →
`Attempt::Retry` → `OperatorWorker::settle` →
`OperatorDispatchQueue::retry` → `RETRY_SQL`. Tests and queue callers use the
same public queue method; there is no sibling settlement path that bypasses the
SQL owner.

- `crates/wyrd/wyrd-server/src/verification/operators.rs:386-420` takes the
  greater of the fixed server backoff and accepted provider delay, then passes
  the duration unchanged to the queue owner.
- `crates/wyrd/wyrd-sql/src/queries/operator_dispatches.rs:99-118` applies
  `LEAST($5, $6)` before interval multiplication and retains the independent
  absolute creation-time deadline, attempt predicate, fencing predicate,
  error payload, and lease clearing.
- `crates/wyrd/wyrd-sql/src/queries/operator_dispatches.rs:279-299` remains the
  sole production retry method and continues to bind the existing queue
  deadline. `millis` saturates an unrepresentable Rust duration to `i64::MAX`,
  after which SQL bounds it to the five-minute deadline before constructing
  the interval.
- `pg_verifier_runs::maximum_retry_after_settles_at_the_deadline` enters at the
  queue boundary with the exact `Duration` produced by the accepted maximum
  decimal `Retry-After` and proves a durable `retrying` row at the
  database-owned deadline with its lease cleared. Existing provider and
  fencing tests retain parser-to-settlement and stale-lease coverage.

Ponytail result: the correction reuses the native PostgreSQL deadline operand
already owning coordination time. A new Rust retry type, parser ceiling,
application clock, helper, or migration would widen the change without closing
another reachable task gap.

### Cumulative changed boundary

I inspected the complete base-to-candidate inventory and the owners and callers
behind the prior ledgers, not only the R5 diff. The implementation's larger
types are justified by current distinct consumers: the shared typed connection
contract projects through HTTP, MCP, CLI, and three SDKs; the concrete
`OperatorKeys` owner serves create/update, per-attempt open, readiness, and
rewrap; the concrete dispatch queue serves claim and fenced lifecycle
settlement; and private Slack/PagerDuty modules isolate two real wire formats.
No single-implementation trait, speculative provider, compatibility path,
optional policy knob, duplicate clock, broker, Alert resource, or executable
Workflow path entered the candidate.

The apparent opportunities to delete more are required behavior, not
complexity debt: per-attempt credential reload closes rotation/revocation;
screen-and-pin must precede each credential attachment and redirect hop;
separate dispatch state supplies independent fan-out and recovery; and bounded
rewrap must remain independent of delivery claims. Collapsing those seams
would weaken explicit security or durability obligations.

## Prior-finding closure

| Stable finding | Independently validated cumulative closure |
|---|---|
| `FIND-TASK-007-1` | **CLOSED** — key failures expose constant public detail and bounded failure classes, not source selectors, paths, Vault addresses, or raw responses. |
| `FIND-TASK-007-2` | **CLOSED** — multi-tenant production verifies every active tenant's active key before readiness. |
| `FIND-TASK-007-3` | **CLOSED** — initial and redirected effective URLs are resolved, screened, and pinned before credential attachment. |
| `FIND-TASK-007-4` | **CLOSED** — the cumulative diff contains no unrelated skill-policy change. |
| `FIND-TASK-007-5` | **CLOSED** — production Vault transport requires HTTPS and token-file reads use the owner-only reader. |
| `FIND-TASK-007-6` | **CLOSED** — Python and TypeScript expose closed provider-discriminated connection shapes. |
| `FIND-TASK-007-7` | **CLOSED** — mounted KEK and Vault token reads use the blocking boundary instead of blocking async workers. |
| `FIND-TASK-007-8` | **CLOSED** — the previously identified production/library imports remain module-scoped. |
| `FIND-TASK-007-9` | **CLOSED** — discovery and tenant work share one finite rewrap-pass deadline, run beside claims, and cancel safely. |
| `FIND-TASK-007-10` | **CLOSED** — exact persisted key-version conversion is fallible, direct construction returns `VersionOutOfRange`, and non-API roles retain their unused default owner. |
| `FIND-TASK-007-11` | **CLOSED** — key configuration and owners use selector-redacting diagnostics. |
| `FIND-TASK-007-12` | **CLOSED** — Slack and PagerDuty wire construction remains in focused private modules while common screened transport stays single-owned. |
| `FIND-TASK-007-13` | **CLOSED** — PagerDuty documentation states only possible retry grouping and disclaims exactly-once and incident-grouping guarantees. |
| `FIND-TASK-007-14` | **CLOSED** — the four previously identified trait/default methods retain intent and invariant rustdoc. |
| `FIND-TASK-007-15` | **CLOSED** — the cumulative declaration sweep remains module-import/bare-name compliant at the previously identified sites. |
| `FIND-TASK-007-16` | **CLOSED** — approved revision 36 authoritatively selects the concrete server-owned env/file/HashiCorp Vault contract and defers the generic resolver and cloud sources. |
| `FIND-TASK-007-17` | **CLOSED** — the MCP list descriptor promises exactly the redacted view fields it returns, and its catalog assertion remains present. |
| `FIND-TASK-007-18` | **CLOSED** — the existing configuration validator refuses environment KEKs in every production shape while preserving the approved development, single-tenant, and multi-tenant boundaries. |
| `FIND-TASK-007-19` | **CLOSED** — the existing SQL queue owner bounds every accepted provider delay before interval construction and settles it within the database-owned deadline. |

Later remediation does not reopen an earlier correction: R5 changes only the
existing source-policy validator, retry SQL, and their focused proofs; the
cumulative candidate retains all R1-R4 corrected paths and the revision-36
authority that resolved `FIND-TASK-007-16`.

## Final validated finding ledger

**Empty.** No independently confirmed or revised material finding remains.
There is no bounded correction to package and no approved behavior or
expensive-to-reverse decision requiring specification revision.

## Recommendation

**PASS.** The smallest safe result is to accept the candidate. Adding another
guard, abstraction, retry representation, test harness, provider layer, or
configuration surface would duplicate behavior already owned and proved.

## Verification limits

- This was a time-bounded static Wave 2 review of the complete cumulative diff,
  applicable authority, four Wave 1 reports, five prior validated ledgers, and
  the live owners/callers relevant to every proposed or prior finding.
- I did not rerun broad Cargo, Postgres, Python, TypeScript, generated-artifact,
  or journey lanes. Wave 1 and implementation records report the focused R5
  configuration and PostgreSQL regressions, `test:sql`, `test:wyrd`, formatting,
  lint, tenant-isolation, unwrap-audit, SDK/client/MCP/CLI journeys, typechecks,
  codegen, and boundary lanes green.
- Credentialed Slack and PagerDuty live smoke remains intentionally gated
  release evidence and was unavailable. Local real-server provider journeys
  cover request construction, credential ordering, classification, retry, and
  durable settlement without vendor credentials.
- No live production Vault, mounted production secret, hostile DNS service, or
  multi-replica deployment was exercised in this wave. Their concrete
  transport, file-mode, pinning, readiness, rotation, fencing, and RLS paths
  were inspected in source and existing focused proof.

## Overall result

**PASS** — the Wave 1 empty union is independently validated, stable findings
`FIND-TASK-007-1` through `FIND-TASK-007-19` are closed in the cumulative
candidate, and the final ledger is empty.
