# TASK-006 R3 security domain review

**Subject:** `f500ea38bc749f36b3ee8d88893dcf7c0161435c..2fbe90cd880936d8e4f25173839e4cc4fa18f0b4` on `vcc/task-006`; cumulative TASK-006 base `f8811ac5035c3aa165d34c38992f9889b3c9081f`. Candidate HEAD matched the supplied identity during review. Source was not edited.

**Boundary reviewed:** public authorization and delegated exchange removal; private Oracle, analytical worker, and Scribe tail gRPC transport; receiver authority and audit; peer credentials and negative peer journey. Authority: `AGENTS.md` §§2, 9, 11; `architecture/agent-rules.md` audit and tenancy rules; `architecture/wyrd-design.md` Auth/Policy and tenant-principal sections; `architecture/wyrd-security-posture.md` security principles, trust boundaries, peer identity, tenancy, and audit; `architecture/bifrost-design.md` tenant, peer, and audit contracts; approved `changes/active/verified-change-contract/spec.md` revision 40, especially REQ-159–166, INV-016, AC-036, and AC-038; original TASK-006 and TASK-006-R2 remediation packet. Inspected the cumulative remediation diff and the surrounding transport, peer authority, Scribe tail, test CA, and peer listener journey sources.

## Findings

### SEC-01 — INCORRECT: client peer identity is not checked

- **Obligation:** REQ-160 and AC-036 require a fixed `wyrd-peer` peer identity and rejection of wrong-identity credentials, as well as CA chain, validity, and client/server usage checks.
- **Location/evidence:** `crates/wyrd/wyrd-tonic/src/server/mod.rs:75-86` builds `ServerTlsConfig` with `identity` and `client_ca_root` only. It requires a CA-signed client certificate but supplies no client-name constraint. `crates/wyrd/wyrd-server/src/app/server.rs:894-909` passes the bundle directly to that builder. Outbound dials validate `wyrd-peer` as the *server* name through `BifrostPeerTls`; this does not validate the inbound client's identity. The negative journey in `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/listener.rs:237-256` tests anonymous, foreign-CA, expired-client and wrong-*server*-name cases, but no same-CA, wrong-client-name case.
- **Consequence:** A valid client-auth certificate issued by the dedicated CA for another identity reaches the private RPC application layer. The required fixed workload identity is therefore not enforced on inbound peers.
- **Testable correction:** Enforce the `wyrd-peer` client identity at the existing private listener's TLS admission boundary while keeping mandatory CA, validity, and EKU verification. Add one same-CA client leaf with a different DNS SAN to the existing peer listener journey and assert it cannot reach application body polling; retain the current positive and negative cases.

### SEC-02 — VIOLATION: untrusted peer context selects a tenant audit chain

- **Obligation:** `architecture/wyrd-security-posture.md:15-22` says peer-supplied bytes never select the effective tenant and tenant binding failures deny the operation; REQ-161 grants the peer certificate cluster identity only and requires receiver-side tenant validation before tenant storage is touched. INV-016 retains canonical audit.
- **Location/evidence:** `crates/wyrd/wyrd-server/src/oracle/peer_authority.rs:117-165` deserializes unsigned forwarding claims, extracts `claims.context.data_tenant_id`, then calls `forwarding_rejection(Some(tenant), ...)` on an audience, fence, or expiry mismatch. That method appends a *verified-tenant* audit event at lines 171-185. The fragment and stage rejection paths similarly extract a tenant from unsigned claims and append via `reject_verified` or `reject_stage_verified` at lines 206-258, 286-296, and 497-510. These branches execute before the receiver has established the claim's tenant against query/reservation state. The comments at the file top explicitly acknowledge the contexts are unsigned.
- **Consequence:** A connected cluster principal can choose an arbitrary tenant for a rejected private request's audit entry, even when the request fails the receiver's audience or fence check. This pollutes another tenant's canonical audit chain with attacker-selected attribution. It does not require access to that tenant's data.
- **Testable correction:** Audit pre-binding failures on the system chain through the existing unverified-rejection path. Use a tenant audit chain only after the receiver has matched that tenant to its own query or reservation state. Add a focused negative test presenting a foreign tenant in a stale-fence or wrong-audience context and prove no row enters the foreign tenant's audit chain.

### SEC-03 — VIOLATION: normative security architecture still requires removed tickets

- **Obligation:** REQ-160/164 retire peer tickets; REQ-165 requires the owning security architecture document to be synchronized. The approved revision 40 spec and `architecture/wyrd-design.md` govern the changed model.
- **Location/evidence:** `architecture/wyrd-security-posture.md:35` still says replica traffic requires mTLS **plus** a signed peer ticket. Its `Peer identity and distributed Oracle` section at lines 247-265 still requires ticket keys, signatures, replay checks, rotation, and ticket rejection audit. The code deleted the ticket protocol and now sends unsigned typed contexts over mTLS, as documented in `crates/wyrd/wyrd-server/src/oracle/peer_authority.rs:1-12`.
- **Consequence:** The normative security document gives an incompatible trust boundary and operational requirement. A deployment following it cannot implement the approved peer protocol, and future changes could restore retired ticket behavior.
- **Testable correction:** Update the existing security posture trust-boundary row and peer section to the approved mTLS-only, fixed-identity, receiver-state validation model; remove retired ticket-key, replay, and rotation instructions. A document search for normative ticket requirements should be empty.

## Coverage and verification limits

- The route and `PolicyHook` deletions, token-exchange edits, and authorization journey show no additional material finding in this domain. The source still uses the typed permission path for ordinary Wyrd API requests.
- The candidate reports passing `test:server:peer` (9/9), `test:server:startup`, server journey (22/22), and `codegen:check`. I did not rerun them. The peer listener journey does not exercise a same-CA wrong-client-name leaf, and the authority unit tests do not show foreign-tenant audit-chain isolation for a rejected unsigned context; those green lanes do not close SEC-01 or SEC-02.
- This review examined the security paths needed for the remediation, not all unrelated server behavior.

**Overall: FAIL.**
