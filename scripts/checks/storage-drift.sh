#!/usr/bin/env bash
# WHY THIS FILE EXISTS: an upload session URI or presigned URL is a bearer
# credential. Persisting one in a migration or storage query compiles cleanly
# and leaks write access to anyone who can read the row.
#
# WHAT IT CHECKS: no bearer-equivalent upload token column or field in the
# Wyrd SQL migrations or storage queries.
set -euo pipefail

if rg -n '\b(session_uri|session_url|sas|presigned_url)\b' \
  crates/wyrd/wyrd-sql/migrations \
  crates/wyrd/wyrd-sql/src/queries/storage; then
  echo 'bearer-equivalent upload tokens must not be persisted in migrations or storage queries'
  exit 1
fi
