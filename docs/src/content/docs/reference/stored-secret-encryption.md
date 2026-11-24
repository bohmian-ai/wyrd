---
title: Stored-secret encryption
description: The [kms] server settings that select the wrapping backend and keys for stored secrets, with provider setup, startup refusals, and runtime errors.
pillar: wyrd
section: Reference
order: 6
---

`wyrd-server` encrypts every recoverable secret it stores with a fresh 256-bit data key, and wraps that key with the deployment wrapping key that `[kms]` selects. Settings are read once at startup from the `[kms]` table of the [server TOML file](/reference/verification-runtime-limits/#server-toml-file). Environment variables override them, and a change takes effect after a restart.

## Settings

| Setting | Environment variable | Applies to | Value |
| --- | --- | --- | --- |
| `backend` | `WYRD_KMS_BACKEND` | all | `file`, `memory`, `gcp_kms`, `aws_kms`, or `vault_transit`. Required |
| `active_key` | `WYRD_KMS_ACTIVE_KEY` | all | The key that wraps new data keys. Required |
| `retained_keys` | `WYRD_KMS_RETAINED_KEYS` | all except `memory` | Keys still allowed to unwrap existing values. A comma-separated list in the variable |
| `key_dir` | `WYRD_KMS_KEY_DIR` | `file` | Directory holding one key file per key name. Required for `file` |
| `endpoint` | `WYRD_KMS_ENDPOINT` | `gcp_kms`, `aws_kms`, `vault_transit` | API base URL. Required for `vault_transit`; GCP defaults to `https://cloudkms.googleapis.com`, AWS to the key ARN's regional endpoint |
| `vault_mount` | `WYRD_KMS_VAULT_MOUNT` | `vault_transit` | Transit mount path. Default `transit` |
| `vault_namespace` | `WYRD_KMS_VAULT_NAMESPACE` | `vault_transit` | Vault Enterprise namespace |
| `vault_token_file` | `WYRD_KMS_VAULT_TOKEN_FILE` | `vault_transit` | Owner-only file holding the Vault token, re-read on every call |
| — | `WYRD_KMS_VAULT_TOKEN` | `vault_transit` | The Vault token itself; environment only |

An environment variable that is set but empty, or not UTF-8, refuses startup.

## Key references

| Backend | `active_key` and `retained_keys` |
| --- | --- |
| `file`, `memory`, `vault_transit` | A key name: 1 to 128 ASCII letters, digits, `_`, `-`, or `.`, not starting with `.` |
| `gcp_kms` | A CryptoKey resource name without a version: `projects/<p>/locations/<l>/keyRings/<r>/cryptoKeys/<k>` |
| `aws_kms` | A key ARN in `aws`, `aws-cn`, or `aws-us-gov`: `arn:<partition>:kms:<region>:<account>:key/<id>`. Aliases are refused |

Each stored value records the key that wrapped it. Opening a value whose key is neither active nor retained refuses without calling the backend.

## Backends

| Backend | Wrapping key | Authentication | Wrap context |
| --- | --- | --- | --- |
| `file` | A file in `key_dir` named after the key, mode `0600`, holding 32 random bytes as base64 | File ownership | AES-256-GCM associated data |
| `memory` | 32 random bytes generated at startup and lost on exit; values stored under it cannot be opened after a restart | None | AES-256-GCM associated data |
| `gcp_kms` | A symmetric `ENCRYPT_DECRYPT` CryptoKey; Cloud KMS selects the primary version to encrypt and the needed version to decrypt | The Google identity GCS storage uses: `GOOGLE_ACCOUNT_JSON_BASE64` (base64 service-account JSON), else `GOOGLE_APPLICATION_CREDENTIALS_JSON` (service-account JSON), else application default credentials: `GOOGLE_APPLICATION_CREDENTIALS`, the gcloud default, or the metadata server | `additionalAuthenticatedData` |
| `aws_kms` | A symmetric key; AWS selects its key material on decrypt | The standard AWS credential chain: environment, web identity, container, or instance metadata | Encryption context `wyrd-context` |
| `vault_transit` | A Transit key of an AEAD type such as `aes256-gcm96`; Transit keeps old versions for decrypt | The Vault token | `associated_data` |

Provider credentials are refreshed before they expire. One wrap or unwrap call to a remote backend is bounded to 10 seconds and follows no redirects.

The workload needs these provider permissions on every active and retained key:

| Backend | Permission |
| --- | --- |
| `gcp_kms` | `cloudkms.cryptoKeyVersions.useToEncrypt` and `cloudkms.cryptoKeyVersions.useToDecrypt` (role `roles/cloudkms.cryptoKeyEncrypterDecrypter`) |
| `aws_kms` | `kms:Encrypt` and `kms:Decrypt` |
| `vault_transit` | `update` on `<mount>/encrypt/<key>` and `<mount>/decrypt/<key>` |

## Provider setup

Wyrd neither creates nor deletes wrapping keys. Provision each key before the server starts, and grant the server's workload only the permissions above.

### `file`

```bash
mkdir kms
openssl rand -base64 32 | tr -d '\n' > kms/v1
chmod 0600 kms/v1
```

Set `key_dir` to the `kms` directory and `active_key` to `v1`. The user running the server must own the file.

### `vault_transit`

```bash
vault secrets enable transit
vault write -f transit/keys/wyrd-secrets type=aes256-gcm96
vault policy write wyrd-kms - <<'HCL'
path "transit/encrypt/wyrd-secrets" { capabilities = ["update"] }
path "transit/decrypt/wyrd-secrets" { capabilities = ["update"] }
HCL
vault token create -policy=wyrd-kms -field=token > vault-token
chmod 0600 vault-token
```

Set `endpoint` to the Vault address, `active_key` to `wyrd-secrets`, and `vault_token_file` to the token file. A Vault Agent may renew or replace the token in that file. Rotate with `vault write -f transit/keys/wyrd-secrets/rotate`; existing values stay readable and `active_key` does not change.

### `gcp_kms`

```bash
gcloud kms keyrings create wyrd --location=us-central1
gcloud kms keys create wyrd-secrets --keyring=wyrd --location=us-central1 --purpose=encryption
gcloud kms keys add-iam-policy-binding wyrd-secrets --keyring=wyrd --location=us-central1 \
  --member=serviceAccount:<server-service-account> --role=roles/cloudkms.cryptoKeyEncrypterDecrypter
```

Set `active_key` to `projects/<project>/locations/us-central1/keyRings/wyrd/cryptoKeys/wyrd-secrets`. Run the server as that service account, point `GOOGLE_APPLICATION_CREDENTIALS` at its key file, or set its key JSON in `GOOGLE_ACCOUNT_JSON_BASE64` (base64-encoded) or `GOOGLE_APPLICATION_CREDENTIALS_JSON`.

GCS storage and Cloud KMS read the same variables, so a server that already reaches its bucket through one of them uses that service account for Cloud KMS too. Grant it the key role above; storage permissions do not include Cloud KMS. When both inline variables are set, `GOOGLE_ACCOUNT_JSON_BASE64` wins, and a value in it that is not valid base64 of UTF-8 selects no inline key rather than falling back to `GOOGLE_APPLICATION_CREDENTIALS_JSON`.

### `aws_kms`

```bash
aws kms create-key --key-spec SYMMETRIC_DEFAULT --key-usage ENCRYPT_DECRYPT --query KeyMetadata.Arn --output text
```

Allow `kms:Encrypt` and `kms:Decrypt` on the printed ARN to the server's IAM role, through the key policy or an IAM policy, and set `active_key` to that ARN.

## Example

```toml
[kms]
backend = "aws_kms"
active_key = "arn:aws:kms:us-east-1:111122223333:key/2222bbbb-2222-2222-2222-222222222222"
retained_keys = ["arn:aws:kms:us-east-1:111122223333:key/1111aaaa-1111-1111-1111-111111111111"]
```

## Renew, replace, or revoke a stored secret

Each resource that stores a secret offers three separate actions under its existing name or id and permission:

- **Renew encryption** reseals the unchanged secret under a fresh data key wrapped by the active key. The resource's identity, settings, and state, and the secret the provider accepts, do not change, so clients need no reconfiguration. A human connection keeps its revision and its sessions. Only modification metadata changes: `updated_at`, and an Operator connection's `updated_by`.
- **Replace** stores a new secret supplied by the administrator, without opening the old one. Wyrd never mints one.
- **Revoke** stops Wyrd using the secret. It does not revoke the credential at its provider; do that at the provider, then replace or revoke it in Wyrd.

| Resource | Renew encryption | Replace | Revoke |
| --- | --- | --- | --- |
| Operator connection | `POST /v1/operator-connections/{connection_id}/renew-encryption`, `wyrd operator-connection renew-encryption` | Update with a new credential | Disable |
| Gateway managed secret | `POST /v1/admin/gateway/provider-credentials/{name}/renew-encryption`, `wyrd gateway credential renew-encryption` | Put the credential again | Revoke |
| Human OIDC connection | `POST /v1/identity/oidc/connections/{id}/renew-encryption` | Stage, test, and activate a candidate | Delete the connection |
| Trusted issuer | `POST /v1/admin/trusted-issuers/renew-encryption?issuer=`, `wyrd auth trusted-issuer renew-encryption` | `PUT /v1/admin/trusted-issuers/client-secret?issuer=` with `{"client_secret": "..."}`, `wyrd auth trusted-issuer replace-secret`; workload bindings are kept | Delete the issuer |
| Platform OIDC connection | `POST /platform/oidc/connection/renew-encryption` | `PUT /platform/oidc/connection` | `DELETE /platform/oidc/connection` |

`wyrd auth trusted-issuer replace-secret` reads the new secret from `--client-secret-file` or `WYRD_ISSUER_CLIENT_SECRET`, never from an argument.

A renewal or trusted-issuer replacement reads the secret, seals outside any database transaction, and stores the result only if the resource still holds the secret it read. One that races a replacement, revocation, or deletion loses with a `409` and the other change stands; neither restores a revoked or deleted resource.

| Resource | `404` | `409` |
| --- | --- | --- |
| Operator connection | No such connection in the caller's tenant | The connection changed between the read and the store. A disabled connection renews and stays disabled |
| Gateway managed secret | No such credential | The credential is revoked, holds no managed secret, or changed |
| Human OIDC connection | No Active or Candidate connection with the id; an inactive connection is not found | The connection holds no client secret, or was deactivated, replaced, or renewed after the read |
| Trusted issuer | No such issuer in the caller's tenant | The issuer holds no client secret, or was deleted or changed |
| Platform OIDC connection | No connection is configured | The connection holds no client secret, or was removed or replaced |

Every action answers `503` with the stored secret unchanged when the key service or the store is unavailable. A trusted-issuer replacement with an empty secret answers `400`.

## Rotate or recover the wrapping key

Changing `active_key` affects only new writes. Existing values keep the key that wrapped them, and stay readable while that key is listed in `retained_keys`. A platform administrator moves them with one maintenance pass at a time:

```bash
WYRD_PLATFORM_CREDENTIAL=... wyrd platform stored-secrets maintain --mode routine --server https://wyrd.example.com
```

```text
mode: routine
current: 0
changed: 9
remaining: 0
bounded: false
```

| Mode | Effect on each value not under `active_key` | Use for |
| --- | --- | --- |
| `routine` | Rewraps its data key under `active_key`. The data key and payload ciphertext are kept | Planned rotation |
| `recovery` | Reseals the value under a fresh data key wrapped by `active_key` | A wrapping key or data key may be exposed |

Native provider rotation (`gcloud kms keys versions create`, `vault write -f transit/keys/<key>/rotate`, AWS automatic rotation) keeps the same reference: values stay readable and need no pass.

**Planned rotation**

1. Provision the new key and grant the server its permissions.
2. Restart every server with the new key as `active_key` and the old key in `retained_keys`.
3. Run `--mode routine` until `remaining` is `0` and `bounded` is `false`.
4. Remove the old key from `retained_keys` only when no backup you may restore holds values under it, then restart.

**Compromise recovery**

1. Contain the incident: revoke the exposed access and stop any server that should no longer run.
2. Provision a fresh key never used as `active_key` before, including by an earlier routine pass.
3. Restart every server with the fresh key active and the old keys retained. Stop every process still writing under an old key.
4. Run `--mode recovery` until `remaining` is `0` and `bounded` is `false`. Never run `--mode routine` into this key; nothing stops an administrator from choosing the wrong mode.
5. Replace each exposed credential at its provider and in Wyrd. Resealing does not revoke a credential an attacker already copied.

If containment removed access to an old key, recovery cannot open the values under it. Restore controlled read access to that key, or replace each credential through its owner without opening it. Removing a key from `[kms]` and restarting refuses every value under it until the key is listed again; Wyrd has no action that disables a key or cancels work already admitted.

### Maintenance pass

`POST /platform/stored-secrets/maintenance` with a platform session; the CLI command above calls it. It needs the platform `platform_credentials:write` permission; a tenant token is refused. The destination is always `active_key`; the request cannot name a key, tenant, or resource.

| Request field | Value |
| --- | --- |
| `mode` | `routine` or `recovery`. Required |
| `limit` | Most values the pass attempts to move, at least 1. Omitted, the pass attempts all within half the server's request timeout. CLI `--limit` |

| Report field | Meaning |
| --- | --- |
| `current` | Values the pass saw already under `active_key` |
| `changed` | Values the pass moved |
| `remaining` | Values the pass read that are still not under `active_key`: failed, changed concurrently, or left by the bound |
| `bounded` | The pass stopped at `limit` or its time budget; stores it did not read are not counted |
| `failures` | Up to 100 values the pass could not move, each with `store`, `data_tenant_id` (absent for the platform connection), `owner`, and `reason` |

Counts describe only what one pass saw. `remaining: 0` does not prove that no backup, cache, or still-running old server depends on a key. Repeat with the same mode while `remaining` is above `0` or `bounded` is `true`; a repeated pass leaves values it already moved unchanged.

| `reason` | Cause |
| --- | --- |
| `key_not_allowed` | The value's key is neither active nor retained |
| `key_unavailable` | The backend refused or could not serve the call. Vault Transit also answers a value copied from another owner this way |
| `unopenable` | The value is not a Wyrd envelope or did not authenticate under its owner |
| `invalid_owner` | The row's owner metadata cannot bind a value |
| `concurrent_change` | The owner replaced or renewed the value first; that change stands |

The pass answers `503` with `reason: kms_not_configured` when the server has no `[kms]`, and `401` without a platform session.

## Startup refusals

Startup fails and names the first offending setting; the message never contains a key, token, path, or URL.

| Condition | Setting named |
| --- | --- |
| `backend` missing or not a listed value | `backend` |
| `memory` on a production server | `backend` |
| `active_key` missing or malformed for the backend | `active_key` |
| A retained key malformed, equal to `active_key`, or listed twice | `retained_keys` |
| `retained_keys` with `memory` | `retained_keys` |
| A setting that does not apply to the selected backend | that setting |
| `key_dir` missing for `file`, or a key file missing, readable by others, or not 32 bytes of base64 | `key_dir` |
| `endpoint` not an `http` or `https` URL, `http` on a production server, or missing for `vault_transit` | `endpoint` |
| Not exactly one of `vault_token_file` and `WYRD_KMS_VAULT_TOKEN` for `vault_transit` | `vault_token` |
| GCP or AWS credentials cannot be loaded | `backend` |

Wyrd never generates a substitute key when a configured one is missing.

## Errors

Errors at runtime, when a stored secret is written or opened:

| Condition | Result |
| --- | --- |
| The stored bytes are not a Wyrd envelope | Unknown format |
| Encrypting the value failed | Encryption failed |
| The value's key is neither active nor retained | Key not allowed; the backend is not called |
| The value, its wrapped data key, or its expected owner was changed | Authentication failure; Vault Transit answers a changed wrap context as `refused` |
| The unwrapped data key is not 32 bytes | Invalid data key |
| The backend refuses, times out, cannot be reached, or has no usable credential | Backend unavailable, with the backend and one of `refused`, `timeout`, `transport`, or `credential` |
