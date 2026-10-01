# TASK-006 R4 security domain review

**Result: PASS**

## Subject and boundary

Reviewed the immutable cumulative candidate `f8811ac5035c3aa165d34c38992f9889b3c9081f..58cabb529b93da366959db796ac3b596a6c6c1e6`, including the R3 remediation after `2fbe90cd880936d8e4f25173839e4cc4fa18f0b4`. The boundary is peer workload identity, receiver-owned private context checks, pre-binding audit ownership, and preservation of public authentication and tenant isolation. Authority: `AGENTS.md` §§2, 9–12; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md` (Bifrost peer trust); `architecture/bifrost-design.md`; approved `changes/active/verified-change-contract/spec.md` revision 41 (REQ-159–164, AC-036, AC-038); original `tasks/TASK-006-continuous-eval-verifier.md`; R3 verdict, findings validation, and `TASK-006-R3-startup-trust-closure.md`.

## Authority and source coverage

| Boundary | Authority | Source and result |
|---|---|---|
| Fixed peer identity and isolated listener | REQ-159–160, AC-036; security posture peer trust | `wyrd-server/src/grpc/mod.rs` `build_peer_grpc` mounts the closed private service set; `GrpcTransportAdmissionService::call` checks the client leaf before polling any body, while `wyrd-tls::certificate_has_dns_name` narrows the handshake-validated leaf to `wyrd-peer`. `BifrostPeerTls::endpoint` is the shared outbound mTLS path. PASS. |
| Peer identity grants no tenant authority | REQ-161; security posture peer trust | `OraclePeerAuthority` bounds and checks forwarding, fragment, reservation, and stage contexts; `StageTicketClaims::verify_binding` checks the receiver-derived tenant, query, fence, digest, and assignment. Scribe tail reads use receiver-owned fences and query ownership in `ScribeTailReader`. PASS within the approved shared-key trust model. |
| Pre-binding audit chain | Security posture platform sentinel; R3 FIND-16; AGENTS.md audit rules | `OraclePeerAuthority::{forwarding_rejection,reject_unverified,reject_stage_unverified}` use `append_unverified_ticket_rejection`; stage expiry after `verify_binding` uses the bound tenant. `PostgresPeerSecurityAudit` commits through canonical `audit::append_on` and routes unverified refusals to `DataTenantId::SYSTEM_OWNER`. Scribe tail access refusal also uses the system chain. PASS. |
| Public plane retained | REQ-160–161, AC-038; AGENTS.md server rules | `TransportPlane::Public` is separate from the private certificate gate; peer services are absent from the public router; peer identity does not replace public authz. The R3 diff adds no public peer bypass. PASS. |

## Verification and limits

The reported `test:server:peer` 9/9 includes a same-CA wrong-SAN client that is refused before the private body's poll counter advances, plus anonymous, unrelated-CA, expired, and wrong-server-name cases. `peer_context_refusals` exercises live private adapters and bound-context refusals. The five exact `OraclePeerAuthority` tests inspect system versus tenant audit selection, and existing Postgres `peer_audit`/`system_owner_security_rejections_retain_once` tests prove durable system-chain writes. A live negative journey does not assert the absence of a foreign-tenant audit row; that absence would depend on asynchronous publication timing. The exact chain-selection seam is directly tested and the durable writer is tested separately, which is credible for FIND-16. Verification results are recorded in the R3 remediation task; this review did not rerun lanes.

## Findings

None. FIND-TASK-006-15 and FIND-TASK-006-16 close at the relevant ingress and audit owners. No additional material security or tenancy finding was found in this boundary.
