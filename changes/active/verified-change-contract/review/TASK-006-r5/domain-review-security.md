# TASK-006 R5 — security domain review

## Subject and coverage

- Immutable base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`; candidate: `6ce9f9bb7e2dea288a1b78081346551d896cb7a8` (HEAD checked at candidate).
- Approved authority: `changes/active/verified-change-contract/spec.md` revision 43, original `tasks/TASK-006-continuous-eval-verifier.md`, R1–R4 remediation tasks, `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/wyrd-security-posture.md`, `architecture/operations/deployment-and-release.md`, and the spec-driven development reference.
- Reviewed boundary: public ingress and gRPC TLS, peer mTLS identity and pre-payload admission, receiver-owned tenant context and refusal audit, database roles/DSNs, production Kubernetes secrets and network policy, and first-use credential handling. Inspected the cumulative diff and current source and guide. No source files were edited.

## Security Audit

### Critical

None confirmed.

### High

- **SEC-R5-01 — MISSING** `[docs/src/content/docs/self-hosting/kubernetes-production.svx:341-371]`: The production path terminates TLS at ingress, forwards to nginx's plaintext `8080`/`50051` listeners, and permits those ports from every pod or network source. The approved operations authority at `architecture/operations/deployment-and-release.md:28-32` requires authenticated, encrypted gateway-to-server transport and confines plaintext to loopback development or a mutually authenticated, policy-enforced local boundary. A compromised workload in the cluster can connect directly to the Wyrd service and send or observe API and gRPC traffic on the pod network, bypassing edge rate and connection controls; passive access to this hop can expose bearer tokens. **Correction:** Keep public TLS termination at the edge as spec revision 43 requires, but make the production guide select and show an authenticated, encrypted edge-to-pod transport (for example, strict service-mesh mTLS on that hop) and restrict `8080`/`50051` ingress to the authenticated gateway path. Do not route public clients to the peer port or reuse peer certificates for public ingress. **Focused proof:** deploy the guide's production routing on a real or locally testable mTLS-capable cluster, show the gateway can reach HTTP and gRPC, an ordinary pod cannot reach the plaintext listeners, and peer mTLS remains separate.

### Medium

- **SEC-R5-02 — MISSING** `[docs/src/content/docs/self-hosting/kubernetes-production.svx:70-72,94-97]`: The production migration and serving database URL examples omit certificate-verifying TLS settings even though Postgres is external; `wyrd-sql/src/dsn.rs:113-116` only checks that a DSN parses and does not require TLS. This leaves the documented production path able to send owner, app, and platform-admin passwords over unverified or plaintext Postgres connections, contrary to `architecture/operations/deployment-and-release.md:42-43`. **Correction:** Give the guide certificate-verifying DSN examples for all three logins, including the provider trust root, and a connection check that refuses a wrong server certificate. Do not change the local development DSNs or conflate this with peer TLS.
- **SEC-R5-03 — VIOLATION** `[docs/src/content/docs/self-hosting/kubernetes-development.svx:216-218]`: The second-tenant command places the live `WYRD_PLATFORM_CREDENTIAL` in the `kubectl exec` command arguments. Kubernetes exec sends command arguments through the API request, where audit/request logs can retain them; the local shell history also retains the literal credential. This conflicts with the task's credentialing journey and `REQ-155`'s terminal-confined credential disclosure. **Correction:** Remove the credential-bearing `kubectl exec ... env KEY=value` example; use the existing authenticated platform CLI/API from the operator machine, or another documented path that does not put the secret in Kubernetes API arguments. Prove a second tenant can be created without the credential appearing in the exec request or pod logs.

### Low / Defense In Depth

No additional task-bound finding.

### Positive Controls

- The private peer listener uses a dedicated CA, client certificate validation, fixed `wyrd-peer` SAN admission before body polling, and separate public and peer transport planes (`crates/wyrd/wyrd-server/src/grpc/mod.rs:148-177`).
- Peer refusal audit uses the system-owner chain until receiver state binds a tenant (`crates/wyrd/wyrd-server/src/oracle/peer_authority.rs:167-185,189-277`), avoiding foreign-tenant audit attribution from untrusted claims.
- Peer TLS private key debug output is redacted (`crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs:2306-2319`), and the production guide keeps the peer CA private key outside pods (`kubernetes-production.svx:103-126`).
- Migration owner credentials are isolated to the Job in the guide; serving pods receive the distinct `wyrd_app` and `wyrd_platform_admin` URLs (`kubernetes-production.svx:70-72,216-219`).

## Verification limits

The supplied R4 evidence reports passing SQL, startup, peer, kind, format, lint, and documentation lanes, and a kind production-guide walkthrough. Those lanes prove boot, peer authentication, and Oracle scaling; they do not exercise encrypted ingress-to-pod transport, Postgres certificate verification, or Kubernetes API audit handling of a second-tenant credential. No live production cluster or external Postgres TLS endpoint was available in this review.

## Result

**FAIL.** The production guide leaves a reachable transport boundary below the required security posture, and its database and follow-on credential instructions omit necessary protections.
