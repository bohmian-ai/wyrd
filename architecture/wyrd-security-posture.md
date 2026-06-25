# Wyrd Security Posture

Companion to `wyrd-design.md`. States the trust model, what is guaranteed
today, and the known gaps with their roadmap. This is the artifact an
enterprise security review, SOC 2 audit, or EU AI Act assessment reads first.

## Trust model

- **Server owns durable behavior.** Identity, tenancy, authz, audit, registry,
  and storage decisions are made server-side. Clients project wire contracts;
  a client never becomes the source of truth (`wyrd-design.md` platform
  posture).
- **No client self-assertion of identity.** `/auth/token` derives `tenant_id`
  and `principal_id` from the verified API-key record, never from a
  client-supplied header. `Wyrd-Caller-Identity` is rejected.
- **Two planes, not three** (doctrine #18): **Auth** (every route, incl.
  data-plane ingest, via JWT + `Permission { resource, action }`) and **Policy**
  (CEL on card states and cross-service invokes). The legacy per-card
  governance token is removed — the JWT's `principal.card_ref` proves the
  emitter; `run_id` correlates the action.

## Identity & credential lifecycle

1. `wyrd apply -f service.yaml` upserts a card-bound `Principal`
   (`(tenant, card_kind, card_uid)`); no secret returned.
2. `wyrd auth issue-key <card_ref>` (admin) mints a `wyrd_sk_…` API key →
   deploy secret store → pod `WYRD_API_KEY`. Re-issuable for rotation. Policy
   `gate` may allow/deny issuance.
3. SDK exchanges the key **once at startup** at `POST /auth/token` for a
   short-lived (~15m) JWT; auto-refreshes. The key never travels on the wire.
4. Cross-service calls carry the JWT in `X-Wyrd-Access-Token` with an RFC 8693
   `act` delegation chain; both caller and callee are server-verified from one
   signature. Federation (RFC 7523 `jwt-bearer`) exchanges an external IdP
   token for a Wyrd JWT with the user as `sub`.

## Audit guarantees (today)

- **Same-transaction, fail-closed.** `append_audit` runs in the operation's tx;
  if it cannot record, the operation rolls back
  (`WYRD_VALA_500_AUDIT_UNAVAILABLE`).
- **Tamper-evident.** Per-tenant gapless `seq` + hash chain
  (`entry_hash = SHA256(prev ‖ canonical(event))`) + append-only trigger + RLS.
- **Durable + queryable.** Background relay ships events to
  `vala.system.audit_log`, idempotent by `wyrd_batch_id`; replayable by
  `Wyrd-Request-Id`.
- **Decisions audited automatically.** Every `/v1/authz/check` allow/deny emits
  a `PolicyInvokeDecision` observation; Agent→Agent edges are derived from
  observed hops.

## Standards alignment

| Area | Standard |
|---|---|
| Identity / no implicit trust | NIST SP 800-207 (Zero Trust) |
| Delegation / federation | RFC 8693, RFC 7523, OAuth 2.0 |
| Access control | NIST 800-53 AC-family; OWASP ASVS V4 |
| Audit logging | NIST 800-53 AU-9/AU-10; ISO 27001 A.8.15; RFC 9162 (tamper-evident log pattern) |
| Policy decision/enforcement | XACML PDP/PEP; OPA / Envoy ext_authz |
| Provenance / lineage | SLSA, in-toto; EU AI Act Art. 12; NIST AI RMF |

## Known gaps & roadmap

Ranked by how hard an auditor presses. None are on a shipped-but-hidden path —
each is explicitly scoped.

1. **Audit chain is tamper-*evident*, not externally anchored.** A DB superuser
   could rewrite the chain including hashes. **Fix:** Stage-5 sealed checkpoints
   — sign `(tenant, seq_range, head_hash)` with a key held outside the DB, and
   anchor heads to WORM/object-lock or an external transparency log. Pull
   forward for high-assurance tenants.
2. **Internal JWT signing-key lifecycle unspecified.** JWKS is planned for
   *external* IdP verification only. **Fix:** asymmetric-only, explicit `alg`
   allowlist (block `alg=none` / RS256↔HS256 confusion), `kid`-based rotation
   with overlap, published JWKS.
3. **Authz PDP fail-mode not stated.** **Fix:** `/v1/authz/check` must fail
   **closed** (deny) when the PDP is unavailable; SDK verdict cache (short TTL)
   bounds the blast radius. State it explicitly.
4. **Bearer tokens are replayable within their ~15m window; no mid-life
   revocation.** Mesh mTLS mitigates in-cluster. **Fix:** consider
   sender-constrained tokens (RFC 9449 DPoP / RFC 8705 mTLS-bound) and a shorter
   TTL for privileged actions; document the assumption of transport mTLS.
5. **Immutable audit vs data-minimization/erasure.** Append-only + multi-year
   retention conflicts with GDPR/CCPA erasure and concentrates risk. **Fix:**
   PII redaction at ingest (the CEL redaction hook exists), field-level
   classification + retention, and a crypto-shredding erasure story.

## Scope notes

- Correlation context (`CorrelationContext` — repo/commit/branch/dev_session)
  is a **dev-time** code-axis bridge for work that predates a registered card.
  It is currently unwired and deferred; it is **not** the runtime card-anchoring
  mechanism (that is the JWT `principal.card_ref`).
- ABAC/CEL attribute policy, query admission control, and audit sealing are
  enterprise / Stage-5 features, not in the Stage-3 data-plane floor.
