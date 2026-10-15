# Gateway deployment sealing keyring

- Change ID: `SPEC-sealing-keyring`, approved revision 1.
- Completed: 2026-10-14.
- Reviewed target: `202e5bab768cb998a01f04555c39f43b53c09929`.
- Completion target: `5d244908cf82037c6b620ae6257c8b2c1363aa14`, including documentation corrections in `f8c478129dfc02ed4d6244b0efced00c491905a4`.
- Delivery reference: branch `split/sealing-keyring`; release reference not supplied. Nothing has shipped.
- Completion authority: explicit human override on 2026-10-14 authorizing closeout, commit and push without another review or completion of outstanding verification. No final-review PASS is claimed.

## Intent and implemented behavior

Gateway managed credentials use the deployment sealing keyring, and a fresh
`wyrd server dev --tenant <slug>` provisions a persistent sealing key before
boot so the quickstart credential step requires no additional key setup or
restart.

Gateway administration and invocation share the deployment `SealingKeyring`,
configured through `WYRD_SEALING_KEY_FILE` or `WYRD_SEALING_KEY_BASE64`.
Managed credentials use its versioned envelope, retained-key reads and existing
boot rewrap. Authenticated payloads bind each value to its tenant, credential
name and provider. The separate Gateway keyring and its per-tenant plumbing
are removed without a compatibility path.

When no key is supplied, the dev CLI creates an owner-only, random 32-byte
base64 key at the release-independent installation root. Publication does not
overwrite an existing key; subsequent starts reuse it. Explicit key settings
retain precedence. Credential reads remain redacted, and keys and provider
secrets must never appear in logs or output.

## Lasting decisions and constraints

The existing deployment keyring and boot rewrap own encryption and rotation;
Gateway adds only its authenticated row identity. Tenant operations retain
RLS through `TenantConn`; cross-tenant rewrap uses the existing operator
capability and exact-ciphertext compare-and-swap to preserve concurrent writes.
Retained keys must remain available until all writers use the new key and a
subsequent pass proves no old-key envelopes remain. Safe rolling rotation
also requires every reader to hold the next key before it is used for writes.

The local key outlives installed releases. Losing it makes stored ciphertext
unreadable unless the original key is restored or credentials are resubmitted.
Nothing has shipped, so no deployed-format conversion or compatibility support
is required. The approved specification's behavior was not changed by closeout;
the human override applies to the completion gate and remaining proof.

## Acceptance evidence and limits

| Obligation | Evidence and status |
|---|---|
| Shared envelope, retained keys, boot rewrap and identity binding | Independently inspected source and negative/rotation test bodies support the required behavior. Runtime execution was not established in the review. |
| First dev boot, quickstart credential submission and restart persistence | Installed-CLI journey covers credential creation, upstream use, restart and reuse; its execution and historical failing-base proof remain unverified. |
| Tenant isolation and secret redaction | Source review found no material defect; tenant-isolation and dependency checks passed on the reviewed candidate. Leak-test execution remains unverified. |
| Review documentation corrections | Required error contracts and boot IO description were corrected, and the incomplete optional rotation recipe was removed. Remediation evidence records passing formatting, docs and whitespace checks. |
| Full acceptance gates | Full lints, codegen and owning runtime journeys were not established. Human-authorized completion accepts these verification limits; no green final gate is asserted. |

## Current authority and owners

- [Security architecture](../../../architecture/wyrd-security-posture.md) and [Wyrd design](../../../architecture/wyrd-design.md).
- [Shared sealing keyring](../../../crates/shared/wyrd-crypt/src/lib.rs), [Gateway binding](../../../crates/wyrd/wyrd-gateway/src/managed.rs), and [credential resolver](../../../crates/wyrd/wyrd-gateway/src/credential.rs).
- [Gateway administration](../../../crates/wyrd/wyrd-server/src/components/gateway/service.rs), [boot](../../../crates/wyrd/wyrd-server/src/boot/mod.rs), and [rewrap owner](../../../crates/wyrd/wyrd-auth/src/sealing.rs).
- [Dev CLI](../../../crates/wyrd/wyrd-cli/src/server/dev.rs), [installed-CLI journey](../../../crates/wyrd/wyrd-cli/tests/pg_tests.rs), and [Gateway restart/rotation tests](../../../crates/wyrd/wyrd-server/src/components/gateway/pg_invocation_tests.rs).
- [Quickstart](../../../docs/src/content/docs/get-started/quickstart.md) and [deployment configuration](../../../docs/src/content/docs/operate/deploy-wyrd-for-a-team.md).
