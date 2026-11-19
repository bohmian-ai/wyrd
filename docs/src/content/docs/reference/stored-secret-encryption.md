---
title: Stored-secret encryption
description: The [kms] server settings that select the wrapping backend and keys for stored secrets, with provider setup, startup refusals, and runtime errors.
pillar: wyrd
section: Reference
order: 6
draft: true
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
| `gcp_kms` | A symmetric `ENCRYPT_DECRYPT` CryptoKey; Cloud KMS selects the primary version to encrypt and the needed version to decrypt | Application default credentials: `GOOGLE_APPLICATION_CREDENTIALS`, the gcloud default, or the metadata server | `additionalAuthenticatedData` |
| `aws_kms` | A symmetric key; AWS selects its key material on decrypt | The standard AWS credential chain: environment, web identity, container, or instance metadata | Encryption context `wyrd-context` |
| `vault_transit` | A Transit key of an AEAD type such as `aes256-gcm96`; Transit keeps old versions for decrypt | The Vault token | `associated_data` |

Provider credentials are refreshed before they expire. One wrap or unwrap call to a remote backend is bounded to 10 seconds and follows no redirects.

The workload needs these provider permissions on every active and retained key:

| Backend | Permission |
| --- | --- |
| `gcp_kms` | `cloudkms.cryptoKeyVersions.useToEncrypt` and `cloudkms.cryptoKeyVersions.useToDecrypt` (role `roles/cloudkms.cryptoKeyEncrypterDecrypter`) |
| `aws_kms` | `kms:Encrypt` and `kms:Decrypt` |
| `vault_transit` | `update` on `<mount>/encrypt/<key>` and `<mount>/decrypt/<key>` |

## Example

```toml
[kms]
backend = "aws_kms"
active_key = "arn:aws:kms:us-east-1:111122223333:key/2222bbbb-2222-2222-2222-222222222222"
retained_keys = ["arn:aws:kms:us-east-1:111122223333:key/1111aaaa-1111-1111-1111-111111111111"]
```

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
