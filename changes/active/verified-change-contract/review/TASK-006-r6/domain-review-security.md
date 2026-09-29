# TASK-006 R6 — security and trust domain review

## Subject and coverage

- Immutable base `f8811ac5035c3aa165d34c38992f9889b3c9081f`; candidate `3f93886489a1d95be1a3eb2382fe059bb9856988` (HEAD confirmed). Reviewed the cumulative diff, with the R5 delta inspected in detail.
- Authority: `AGENTS.md` §§9, 11–12; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md` (production composition, peer identity, secret handling); `architecture/operations/deployment-and-release.md` (authenticated gateway hop, verified external TLS, secret handling); approved `changes/active/verified-change-contract/spec.md` revision 44, especially REQ-153–165 and AC-034–037; original TASK-006 and R5 remediation task.
- Boundaries traced: owner-only migration in `wyrd-server/src/main.rs` and `OperatorPool`; serving app/admin DSNs; SQLx 0.9 and Iceberg SQLx 0.8.1 TLS configuration; production Kubernetes Gateway, Istio policies, NetworkPolicy, secret creation and credential commands; public SDK gRPC TLS; readiness logging. No source code was edited.

## Security Audit

### Critical

None confirmed.

### High

- **SEC-R6-01 — INCORRECT** `[docs/src/content/docs/self-hosting/kubernetes-production.svx:422-426]`: The guide starts the anchor and Oracle pods and waits for both to be ready before applying `wyrd-netpol.yaml` and `wyrd-mesh.yaml`. Until line 426, nginx listens on pod ports 8080/50051 with no guide-supplied ingress restriction or STRICT mesh policy; the prerequisites require a NetworkPolicy-capable CNI but do not establish a pre-existing deny policy. A compromised workload in the namespace can connect directly to those pod IPs during initial rollout or a fresh installation, bypassing the intended edge-only authenticated transport; if it can observe that traffic, bearer credentials and request bodies are exposed. This fails R5 FIND-27's requirement that the production route *enforce* an authenticated, encrypted gateway-to-pod boundary. **Correction:** Apply `wyrd-netpol.yaml` and `wyrd-mesh.yaml` before starting either serving workload, then apply the edge after they are ready. **Focused proof:** Follow the printed order from a fresh namespace; while a Wyrd pod starts, an ordinary pod cannot reach 8080 or 50051, and afterward HTTP and gRPC through the gateway succeed.

### Medium

- **SEC-R6-02 — VIOLATION** `[docs/src/content/docs/self-hosting/kubernetes-production.svx:118-120,133-136]`: The three `psql "$url"` checks and the `kubectl create secret --from-literal=...="$URL"` commands place the owner, app and platform-admin passwords in process arguments. A same-host observer with process-list access on the operator machine can capture those credentials while the commands run; this is the same exposure mode R5 removed from the development `kubectl exec` example. `architecture/wyrd-security-posture.md:374` says credentials are not accepted through command arguments. **Correction:** Keep the existing verified DSNs and three-login check, but provide the password through a non-argument channel for `psql`, and use the already documented secret-provider or a restrictive file-based Kubernetes Secret creation path rather than `--from-literal` for secret DSNs. **Focused proof:** Follow the guide and inspect spawned `psql`/`kubectl` argv; no database password appears, while all three logins still verify TLS and the Job/pods receive the intended Secrets.

### Low / Defense In Depth

None task-bound.

### Positive Controls

- The serving pods mount only `wyrd_app` and `wyrd_platform_admin` DSNs; the one-off Job alone mounts the owner DSN (`kubernetes-production.svx:186-205,297-303,378-384`; `wyrd-server/src/main.rs:202-221`).
- All three documented Postgres DSNs require `sslmode=verify-full` with a mounted CA; both SQLx 0.9 and the Iceberg catalog's SQLx 0.8.1 read `PGSSLROOTCERT` (`kubernetes-production.svx:97-108,198-204,300-303`; SQLx `options/mod.rs`).
- After application, the NetworkPolicy admits public ports only from the edge's pod label, while Istio STRICT mTLS plus exact gateway principal prevents another pod with a forged label from gaining access (`kubernetes-production.svx:433-486`). The peer port remains on dedicated certificate-based mTLS.
- HTTPS gRPC uses native trust roots (`wyrd-client/src/transport/grpc.rs:165-173`); the obsolete accepted-but-ignored custom TLS fields have been removed.

## Verification limits

The R5 evidence reports a kind guide walk that tested gateway HTTP/gRPC, mutual-TLS mesh statistics, a blocked ordinary pod, and wrong-CA/wrong-host Postgres refusals **after** the policies and secrets were installed. It did not test the interval before policy application or inspect operator-machine process arguments. I did not run a live cluster or handle real credentials in this review. The new full-text readiness logs could include backend context; I found no concrete secret-bearing error path, so that remains an open question rather than a finding.

## Result

**FAIL.** The documented initial rollout has an unprotected serving interval, and database credentials enter command arguments contrary to the approved secret-handling rule.
