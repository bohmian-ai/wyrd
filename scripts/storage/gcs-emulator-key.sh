#!/usr/bin/env bash
# Print a throwaway GCS service-account key for the fake-gcs-server lanes.
#
# The emulator ignores authentication, but `object_store` signs GCS URLs only
# with a service-account private key. Each run generates a fresh RSA key that
# never leaves the process environment and is never committed.
set -euo pipefail

openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 2>/dev/null |
  python3 -c '
import json, sys
print(json.dumps({
    "type": "service_account",
    "project_id": "wyrd-emulator",
    "private_key_id": "wyrd-emulator",
    "private_key": sys.stdin.read(),
    "client_email": "wyrd-emulator@wyrd-emulator.iam.gserviceaccount.com",
    "client_id": "0",
    "token_uri": "https://oauth2.googleapis.com/token",
}))
'
