# TASK-003 R2 review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `05ff68fb47572e7d8e5fa34037042559bcfeac83`
- Approved specification: `SPEC-oidc-production-readiness` revision 5 at
  `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20:changes/active/oidc-production-readiness/spec.md`
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Prior review and remediation:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/`
- Human amendment:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`
  replaces prior `FIND-TASK-003-1` and `R1-AC-01`.

The complete cumulative base-to-candidate range was reviewed. The remediation
range from prior candidate `4e0ca8d2ecc2940724861cf6884b68f9adf65464`
was used only to locate changed owners and assess prior-finding closure. The
candidate remained the checked-out `HEAD` throughout discovery, follow-up, and
structured validation.

## Independent review results

| Required report | Result | Proposed findings |
|---|---|---|
| `task-review-behavior.md` | FAIL | `BEH-R2-001` through `BEH-R2-003` |
| `task-review-invariants.md` | FAIL | `INV-R2-01`, `INV-R2-02` |
| `standards-review.md` | FAIL | `STD-R2-001` through `STD-R2-003` |
| `maintainer-review.md` | FAIL | `MAINT-R2-001` |
| `system-review.md` | FAIL | `SYSTEM-R2-001` |
| `domain-review-security-identity.md` | PASS | None |
| `domain-review-persistence-concurrency.md` | PASS | None |
| `followup-review.md` | RESOLVED | Confirmed the three assigned disputed paths |
| `findings-validation.md` | FIX_REQUIRED | `FIND-TASK-003-4`, `-5`, `-6`, `-10` through `-13` |

Every required reviewer and report was available. No missing report was
converted into a verification limit or a passing result.

## Follow-up decision

A focused follow-up was required because discovery materially disagreed about
expired stored ciphertext and the multi-provider journey, and one reviewer
identified an otherwise unreviewed proactive-renewal path. It resolved all
three questions from approved authority and source:

- expired, non-null browser-session envelopes remain stored outside the
  canonical inventory, so prior `FIND-TASK-003-4` is still open;
- the browser journey uses two Keycloak realms instead of the remediation's
  Keycloak/Dex topology and discards genuine callback `iss`, so prior
  `FIND-TASK-003-6` is still open; and
- a refused proactive renewal commits revocation while the current access
  token is still valid, creating new `FIND-TASK-003-10`.

The follow-up returned `RESOLVED`; no discovery uncertainty remains.

## Reconciled acceptance matrix

| Obligation | Reconciled implementation and proof | Result |
|---|---|---|
| REQ-003 / AC-006: authorized settings projection | The UI calls the existing tenant management API with session authority; the replacement journey exercises stage, test, activate, denial, replacement login, and removal. | PASS |
| REQ-005: sealed browser credentials and canonical key lifecycle | Live-session rewrap, exact-byte CAS, and K2-only recovery are implemented, but expired non-null session envelopes are excluded from inventory and keyless boot. | FAIL — `FIND-TASK-003-4` |
| REQ-006: canonical tenant entry and server-bound callback | Tenant and connection resolve from server-owned state; the BFF completion route remains fixed and token-free. | PASS |
| REQ-007 / amended R1-AC-01: conditional RFC 9207 binding | Present `iss` is exact-matched; absence is refused only for advertising providers; non-advertising providers remain usable as the explicit human direction requires. | PASS |
| REQ-009: replica-safe production BFF session | Postgres authority, one-use completion, cookie protections, CSRF, tenant binding, server-only tokens, and replica crossing exist. The real TLS channel proof, bounded chooser verification, and fail-closed upstream configuration remain incomplete. | FAIL — `FIND-TASK-003-5`, `FIND-TASK-003-12`, `FIND-TASK-003-13` |
| REQ-010: OIDC-off entry through existing credential authority | Browser API-key entry uses the shared exchange or exactly one dummy verification; invalid classes remain indistinguishable. | PASS |
| REQ-015 / AC-003: independent provider sessions and safe switching | Server-verified switching works, but the required different-provider-service and standards-shaped mixed-callback browser proof is incomplete. | FAIL — `FIND-TASK-003-6` |
| REQ-016: old-connection renewal cutoff at access expiry | Renewal is refused after lifecycle change, but proactive renewal refusal revokes a still-valid session before its issued token expires. | FAIL — `FIND-TASK-003-10` |
| REQ-018 / AC-009: documentation and generated contracts agree | Issuer residual risk, sealing procedure, UI production behavior, and generated callback schemas are updated consistently. | PASS |
| INV-001: browser/provider data cannot select effective tenant identity | Session and callback authority remain server-bound; the remaining callback issue is a required journey-proof gap, not a demonstrated authority bypass. | PASS, subject to `FIND-TASK-003-6` proof closure |
| INV-003: principal planes remain distinct | No provider group, BFF service key, or browser credential creates platform or workload authority. | PASS |
| INV-005: UI projects server-owned identity and permissions | The authoritative tenant UUID reaches server-only `TenantContext`; browser metadata remains limited to safe key/name data. | PASS |
| AC-001 / AC-002: OIDC-off and real-provider UI journeys | Existing real journeys cover operator credential entry, provider login, role mapping, allowed/denied calls, flow binding, CSRF, logout, and replicas. | PASS |
| AC-007: security, rotation, lifecycle, and replica evidence | Most negative flows are covered, but expired-envelope inventory, actual TLS transport, and exact-expiry renewal behavior are not. | FAIL — `FIND-TASK-003-4`, `FIND-TASK-003-5`, `FIND-TASK-003-10` |
| Repository structural and documentation rules | Owners, SQL capabilities, transactions, generated artifacts, and prior rustdoc repairs conform. New OIDC discovery items lack mandatory rustdoc, chooser reads are unbounded, and explicit empty upstream configuration bypasses validation. | FAIL — `FIND-TASK-003-11`, `FIND-TASK-003-12`, `FIND-TASK-003-13` |
| Explicit non-goals and prohibited changes | No browser bearer storage, UI role mapper, password authority, new dependency/feature/harness, callback route, compatibility alias, or commercial stub was added. | PASS |

## Validated finding ledger

The source evidence, rejected proposal, and decision-complete corrections are
in `findings-validation.md`. The retained ledger is:

| Finding | Classification | Validated cause |
|---|---|---|
| `FIND-TASK-003-4` | INCORRECT | Canonical rewrap and keyless inventory omit stored expired-session ciphertext. |
| `FIND-TASK-003-5` | MISSING | The secret-bearing BFF path has URL-policy tests but no real trusted TLS operation. |
| `FIND-TASK-003-6` | MISSING | The browser proof uses two Keycloak realms and strips genuine callback `iss`, missing the required topology and refusal path. |
| `FIND-TASK-003-10` | INCORRECT | Refused proactive renewal revokes still-valid browser authority before expiry. |
| `FIND-TASK-003-11` | VIOLATION | New private OIDC discovery items lack mandatory substantive rustdoc and `# Errors`. |
| `FIND-TASK-003-12` | VIOLATION | Cookie hints fan out through unbounded concurrent authenticated reads. |
| `FIND-TASK-003-13` | VIOLATION | Falsey fallback converts explicit empty upstream configuration into loopback instead of refusing it. |

`MAINT-R2-001` was independently rejected as optional naming cleanup and is
not part of the ledger. The TLS proposals were deduplicated into prior stable
ID `FIND-TASK-003-5`.

## Prior-finding closure

| Prior finding | Result |
|---|---|
| `FIND-TASK-003-1` | CLOSED under the explicit human replacement; no provider-support activation gate is required. |
| `FIND-TASK-003-2` | CLOSED — every browser API-key class pays exactly one shared verification. |
| `FIND-TASK-003-3` | CLOSED — chooser hints are server-resolved before rendering. |
| `FIND-TASK-003-4` | OPEN — expired stored envelopes remain outside inventory. |
| `FIND-TASK-003-5` | OPEN — real TLS transport proof remains absent. |
| `FIND-TASK-003-6` | OPEN — provider topology and mixed-callback proof remain incomplete. |
| `FIND-TASK-003-7` | CLOSED — the originally cited Rust items are documented correctly. |
| `FIND-TASK-003-8` | CLOSED — authoritative tenant id is projected server-side without browser exposure. |
| `FIND-TASK-003-9` | CLOSED — `SessionLifetime` and the dual-lifetime SQL branch were removed. |

## Verification limits

- Discovery, follow-up, and structured validation were static acceptance
  audits. They inspected recorded green evidence but did not rerun Cargo,
  mise, pnpm, provider, browser, or Postgres lanes.
- The candidate records green UI unit/typecheck, filtered and unfiltered
  identity journeys, Wyrd and SQL families, codegen, tenant-isolation, docs,
  format, lints, and `git diff --check`. Those results do not exercise the
  seven validated gaps; missing required proof is retained as a finding rather
  than softened into a verification limit.
- CodeGraph was unavailable because the repository has no `.codegraph/`
  directory; reviewers used immutable Git diffs and repository-native source
  navigation.

## Verdict

**FIX_REQUIRED**

The bounded remediation is specified in
`TASK-003-R2-production-ui-remediation.md`. None of the validated corrections
requires a specification revision or a new product, public API, architecture,
security, compatibility, cross-service, concurrency-semantics,
resource-ownership, or persistent-data decision.
