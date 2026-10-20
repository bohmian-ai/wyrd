# TASK-001 R2 re-review

**Verdict: FIX_REQUIRED.** One validated finding remains: **FIND-TASK-001-8 (REVISED)**. Logging out a matching suspended tenant clears the cookie without revoking its durable session; ordinary reactivation makes the saved value usable again. The correction needs the existing tenant-directory lookup, not a spec revision or new infrastructure.

## Immutable subject and acceptance scope

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
- Complete cumulative base: `8651b96fd0c049ab5e63b0606c98326af1fc8848`.
- Candidate: `71909c64d888caac85bcb184bbf2aba7c25e9e0b`.
- R2 navigation base: `2741395a33d5e1e5ee5d7022916627917254a663`.
- Approved authority: [spec revision 10](../../spec.md), [original TASK-001](../../tasks/TASK-001-distribution-login.md), retained [TASK-002](../../tasks/TASK-002-remove-old-ui.md), prior original/R1 reviews and R1/R2 remediation. Full inputs and navigation: [subject.md](subject.md).

Current caller direction explicitly defers nginx execution (FIND-2); the latest request also reserves image matrix, signed native runtime and human visual checks for later stages. Those are **DEFERRED / UNVERIFIED**, not blockers to this R2 source-correction review and not executed passes. This review does not grant final TASK-001 deployment/visual acceptance. The failure above is an implementation regression within the current scope, unrelated to deferred nginx work.

## Independent review and reconciliation

| Fresh role | Report | Discovery result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | PASS within current scope |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | PASS within current scope |
| Repository standards | [standards-review.md](standards-review.md) | PASS |
| Maintainer | [maintainer-review.md](maintainer-review.md) | PASS |
| System resilience | [system-review.md](system-review.md) | PASS within current scope |
| Security/RBAC | [domain-review-security.md](domain-review-security.md) | PASS within domain |
| Data/durability | [domain-review-data.md](domain-review-data.md) | FAIL: DATA-R2-001 |
| Focused follow-up | [followup-review.md](followup-review.md) | RESOLVED: FOLLOW-001 |
| Independent Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED: revised FIND-8 |

[Claim comparison](claim-comparison.md) required a fresh follow-up because the data reviewer traced a suspended-tenant transition missing from the other reviewers' logout closure. Follow-up established public/internal reachability, the status writer, active-only resolver and persistent recovery consequence. Independent validation confirmed the defect from actual source; six closure opinions do not outweigh that trace. DATA-R2-001 and FOLLOW-001 consolidate into the existing FIND-8 mutation boundary, with no new finding number.

## Reconciled acceptance and prior closure

| Obligation | Source and inspected verification evidence | Result |
|---|---|---|
| FIND-1: installation admission on fresh/cache/current paths | Shared `check_payload` retained before activation/lookup; incomplete-payload and preservation fixtures retained | Source closure retained; signed runtime separately unverified |
| FIND-3: public native session forwarding | `forwardAuth` retains Wyrd browser-session cookies only for browser auth paths; focused unit plus real native driver's public enumeration/logout and saved-value checks | Closed |
| FIND-4: current metadata only | Accepted enumeration required; no stale selected fallback; focused current/503/401/sibling-only assertions and layout consumer traced | Closed |
| FIND-5: callback outage classification | Carried catalog 5xx returned before directory read, with no-store/cleared spent binding; focused unavailable/ordinary-refusal assertions | Closed |
| FIND-6: cleanup after failure | Original status passed to existing cleanup after best-effort forwarder removal; recorded four-case probe and actual callers inspected | Closed |
| FIND-7: remove extra fixture setting | Producer/consumer reuse `WYRD_UI_JOURNEY` | Closed |
| FIND-8: tenant-bound, durable session-only logout | Cross-tenant relabel defect fixed, but active-only lookup skips revoke for a matching suspended tenant; supplied test never logs out suspended tenant then resumes | **FAIL — REVISED** |
| FIND-9: finite rustdoc/delivery account | Three formatter docs, specified modules and committed-but-undelivered semantics present; recorded fmt/lints assessed | Closed |
| REQ-033 cumulative durable session/security constraints | Rust owner, digest-only Postgres persistence, current credential/connection authority, exact Origin, bearer precedence and protocol confinement independently inspected | Retained except logout regression above |
| Packaging/dev lifecycle, supported image source, old UI deletion and non-goals | Existing build/installer/process owners, route topology and retained UI consumers inspected through cumulative diff | Source regression scope retained; platform execution below unverified |
| Changed activation assertion | Activation retires exact connection; post-action current metadata refuses actor's session; following replacement/owner-key assertions retain outcome proof | Source diagnosis accepted; historical separate diagnostician artifact unavailable |
| FIND-2 and final platform/visual evidence | No Linux/nginx execution, full image matrix, source-free signed native runtime/digests or human visual proof inferred | DEFERRED / UNVERIFIED |

## Validated ledger and correction

**FIND-TASK-001-8 — REVISED — REGRESSION.** At `crates/wyrd/wyrd-auth/src/browser_sessions.rs:329–335`, logout uses `resolve_tenant_slug`, whose SQL admits active tenants only. For a matching suspended tenant it returns success before revocation. The HTTP adapter returns 204 and clears the cookie; suspension/resume changes only tenant status, so the saved unrevoked session works again after resume before expiry. This violates REQ-033/AC-012 selected-session revocation. It does not bypass suspension or require blanket session revocation on suspension.

Use existing `tenant_directory_entry` for the parsed tenant's authoritative slug comparison at the same logout owner, then retain the existing exact-digest TenantConn revocation. Preserve active admission for authentication/recovery, mismatch refusal, Origin, RLS, idempotency, unavailable propagation and sibling sessions/credentials. The prior recommendation to reuse recovery's active-only resolver was insufficient for revocation and is replaced by this identity-only lookup. Full independently validated trace and minimum correction are in [findings-validation.md](findings-validation.md).

Ready remediation: [TASK-001-R3-suspended-tenant-logout.md](TASK-001-R3-suspended-tenant-logout.md), routed to `$wyrd-implement`. No implementation was performed during review.

## Verification limits and preservation

This was an independent static cumulative review. No tests, builds, services or images were run; supplied RED/GREEN records were checked against test source and exact selectors. The missing suspended-logout transition is source-proven, not presented as an executed reproduction. Historical standalone diagnostician/trace and scratch shell-probe artifacts were unavailable; the task record and current independent source diagnoses are distinguished from fresh runtime proof.

HEAD remained `71909c64d888caac85bcb184bbf2aba7c25e9e0b`. Uncommitted trace edits remained byte-identical (diff SHA-256 `c40d5eb244036dd64dc81c3031eb3bb33ef8af2876bd2b819dcbef3850a09ea0`). Only this new report directory was written. No source, spec, older task/report, commit, merge or deployment was changed.
