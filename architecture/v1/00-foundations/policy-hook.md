# Policy Hook

`PolicyHook` is the server-side seam between the authz-check mechanism in this
followup and the future CEL-backed policy plane. The hook evaluates an
`AuthzCheckContext` after delegated-token guards have passed, allowing this PR
to ship safe test defaults while keeping the durable policy implementation
outside the request-id and application-state stage.
