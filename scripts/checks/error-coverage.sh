#!/usr/bin/env bash
# WHY THIS FILE EXISTS: stable error codes are a public contract — operators
# and clients match on them in alerts, runbooks, and retry logic. An emitted
# code with no map entry surfaces as an opaque unknown to every consumer. An
# untested code mapping can regress silently: the code ships, the mapping
# breaks, and nobody knows until a production alert fires with an unrecognized
# code. This script keeps emitted codes and their observable mappings in sync.
#
# WHAT IT CHECKS: every error code emitted in source has a corresponding entry
# in the observable error map; every mapped code has test coverage.
set -eu

emitted=$(mktemp)
mapped=$(mktemp)
sql_emitted=$(mktemp)
sql_tested=$(mktemp)
trap 'rm -f "$emitted" "$mapped" "$sql_emitted" "$sql_tested"' EXIT

{
  rg --no-filename -oN 'SKALD_AGENT_[0-9]{3}_[A-Z_]+' crates/skald/skald-agent/src || true
  rg --no-filename -oN 'SKALD_SESSION_[0-9]{3}_[A-Z_]+' crates/skald/skald-agent/src || true
  rg --no-filename -oN 'SKALD_WORKFLOW_[0-9]{3}_[A-Z_]+' crates/skald/skald-workflow/src || true
} | sort -u > "$emitted"

rg --no-filename -oN 'SKALD_(AGENT|SESSION|WORKFLOW)_[0-9]{3}_[A-Z_]+' \
  crates/skald/skald-observer/src/error_map.rs \
  | sort -u > "$mapped"

if comm -23 "$emitted" "$mapped" | rg .; then
  echo 'Observed Skald error map is missing emitted source codes'
  exit 1
fi

rg --no-filename -oN 'WYRD_SQL_[0-9]{3}_[A-Z_]+' \
  crates/wyrd/wyrd-sql/src/error.rs \
  | sort -u > "$sql_emitted"

sed -n '/#\[cfg(test)\]/,$p' crates/wyrd/wyrd-sql/src/error.rs \
  | rg --no-filename -oN 'WYRD_SQL_[0-9]{3}_[A-Z_]+' \
  | sort -u > "$sql_tested"

if comm -23 "$sql_emitted" "$sql_tested" | rg .; then
  echo 'SQL error coverage is missing stable WYRD_SQL_* codes'
  exit 1
fi
