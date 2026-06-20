# Principal Extraction

Wyrd HTTP handlers use a small extractor lattice: `AuthenticatedPrincipal`
verifies the signed Wyrd access token from `X-Wyrd-Access-Token`, then `Caller`
projects that verified principal together with the request ID for handler code.
`Caller.data_tenant_id` is copied from `principal.tenant_id`; request headers
are not a tenant authority.
