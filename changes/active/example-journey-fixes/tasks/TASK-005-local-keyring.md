# TASK-005 Local server handles the gateway keyring

Spec: `../spec.md` REQ-007, INV-002, AC-001..AC-003.

`wyrd server dev --tenant <slug>` must leave a deployment where
`wyrd gateway credential put` with a managed_secret source succeeds with no
manual step. Today the server needs
`[gateway.managed_secret_keys."<tenant-uuid>"]` in its config, the tenant UUID
exists only after setup, and the config is read only at boot, so the docs
quickstart's credential step fails (`WyrdTestServer` masks this by creating
keyrings itself). Create an owner-only per-tenant key for the provisioned
tenant, configure the server with it, and restart the server when the key did
not exist at start; reuse it on later runs so stored credentials keep
decrypting. Never print the key.

Evidence: a CLI journey that runs `wyrd server dev` against a locally
installed server build (the existing installed-release layout), then stores a
managed_secret credential and resolves it through a gateway call; a second
run that reuses the key; docs quickstart and troubleshooting updated.
