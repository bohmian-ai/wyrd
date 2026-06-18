# Policy Hook

`PolicyHook` is the server-side seam between the authz-check mechanism in this
server and the CEL-backed policy plane. The hook evaluates an
`AuthzCheckContext` after delegated-token guards have passed. Development and
test state may mount explicit stub hooks, while production state rejects stub
policy and audit hooks before serving traffic.
