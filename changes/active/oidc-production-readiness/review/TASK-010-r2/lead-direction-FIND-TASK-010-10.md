# Lead direction: FIND-TASK-010-10 correction narrowed

The finding is right that the image-local NGINX `limit_req` is wrong.
Behind the deployment's public edge, `$binary_remote_addr` is the edge peer.
One shared budget per replica then throttles every user's device-code entry,
and the limit still does not bound an attacker across replicas.

The prescribed correction, in which Wyrd owns rate-limit configuration on the
deployment's public edge (Gateway API or Istio), goes beyond what Wyrd ships.
The edge is operator-owned. `architecture/operations/deployment-and-release.md`
says the gateway owns rate-limit integration, and Wyrd ships no edge
manifests. Comparable self-hosted authorization servers (Keycloak, Dex,
GitLab) leave edge rate limits to the operator's ingress and document the
endpoint to protect.

Correction:
1. Delete the `map`, `limit_req_zone`, `limit_req_status` and `limit_req`
   lines from `docker/official/extras/nginx/nginx.conf.template`. Delete the
   device-limit assertions from `scripts/server/test-startup.sh`. Keep the
   lane's owner-only key and single-tenant fixes, which r2 accepted.
2. Add one short operator note to the existing self-hosting or deployment
   docs page. It should say to rate-limit `POST /auth/device` (RFC 8628 §5.1
   user-code entry) per client address at the public ingress.
3. Add nothing else: no application limiter, header parsing, edge
   manifests, setting or state. Device-code entropy, short expiry and one-use
   redemption remain the in-server protections.

`TASK-010-R2-public-edge-device-admission.md` is superseded by this direction.
