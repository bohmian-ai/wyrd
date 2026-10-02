# Lead direction: FIND-TASK-009-5 withdrawn

Standing human direction: Wyrd does what everyone else does, and reviewer
findings may not require mechanisms that the standard practice does not have.

FIND-TASK-009-5 asks Wyrd to discard the provider's RFC 6749 §5.2
`error_description` and `error_uri` before logging a token-endpoint refusal.
Comparable relying parties (oauth2-proxy, MLflow's OIDC plugin, Dex as a
client, Keycloak brokering) log those standard fields, because they are the
operator's main tool for diagnosing a misconfigured provider. The threat it
cites, a configured provider echoing back a secret it already holds, is not
one those projects defend against.

Correction: none. Keep the `oauth2` error display as shipped. Wyrd must still
never log its own client secret, PKCE verifier, codes or tokens, and the
shipped code already meets that. The remediation task's FIND-TASK-009-5
section and its closure proof are void. Every other finding in
`TASK-009-R1-relying-party-corrections.md` stands.
