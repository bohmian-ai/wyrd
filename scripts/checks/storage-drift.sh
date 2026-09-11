#!/usr/bin/env bash
# WHY THIS FILE EXISTS: storage sits at a security and multi-tenancy boundary.
# Violations there typically compile cleanly and only surface as data leaks,
# credential exposure, or tenant isolation failures at runtime. The type system
# cannot catch: a bearer token written to a SQL column, a raw PgPool handed to
# a tenant-scoped query, cloud SDK symbols imported by a client that should
# work without cloud credentials, or in-memory state shared across requests.
# Static pattern matching catches these before they reach production.
#
# WHAT IT CHECKS: patterns that violate storage security or architectural
# invariants — credential persistence, tenant isolation bypass, language
# binding leakage, cloud-neutral client boundary, cross-request state,
# protocol dispatch correctness, identity surface uniqueness, and error
# propagation discipline.
set -eu

rg_optional() {
  pattern="$1"
  shift
  existing=""
  for path in "$@"; do
    if [ -e "$path" ]; then
      existing="$existing $path"
    else
      echo "skip missing path: $path"
    fi
  done
  if [ -n "$existing" ]; then
    # shellcheck disable=SC2086
    rg -n "$pattern" $existing
  else
    return 1
  fi
}

rg_fixed_optional() {
  pattern="$1"
  shift
  existing=""
  for path in "$@"; do
    if [ -e "$path" ]; then
      existing="$existing $path"
    else
      echo "skip missing path: $path"
    fi
  done
  if [ -n "$existing" ]; then
    # shellcheck disable=SC2086
    rg -nF "$pattern" $existing
  else
    return 1
  fi
}

if rg_optional 'pyo3|pymodule|pyclass|pymethods' \
  crates/wyrd/wyrd-storage \
  crates/wyrd/wyrd-server/src/storage \
  crates/wyrd-spec/src/storage; then
  echo 'storage foundation must stay PyO3-free'
  exit 1
fi

if rg_optional 'session_uri|session_url|sas|presigned_url' \
  crates/wyrd/wyrd-sql/migrations \
  crates/wyrd/wyrd-sql/src/queries/storage; then
  echo 'bearer-equivalent upload tokens must not be persisted in migrations or storage queries'
  exit 1
fi

if rg_fixed_optional 'partial.state' \
  crates/wyrd/wyrd-client/src/artifacts \
  crates/wyrd/wyrd-client/src/cards; then
  echo 'upload sidecar state is forbidden'
  exit 1
fi

if rg_fixed_optional 'storage_multipart_upload_parts' crates/wyrd/wyrd-sql/migrations; then
  echo 'per-part multipart upload tables are forbidden'
  exit 1
fi

if rg_optional 'presign_batch|next_parts|part_batch_size|batch_presign|presign.*batch|batch.*presign' \
  crates/wyrd/wyrd-storage/src \
  crates/wyrd-spec/src/storage \
  crates/wyrd/wyrd-client/src; then
  echo 'per-part presign batching is forbidden'
  exit 1
fi

if rg_optional 'aws_sdk_s3|gcloud_storage|azure_storage|azure_core|object_store|sqlx' \
  crates/wyrd/wyrd-client/src \
  crates/wyrd/wyrd-client/Cargo.toml; then
  echo 'client tier must stay free of cloud SDK, object_store, and sqlx dependencies'
  exit 1
fi

if rg_optional '&PgPool' \
  crates/wyrd/wyrd-sql/src/queries/storage/multipart_uploads.rs \
  crates/wyrd/wyrd-sql/src/queries/storage/artifact_metadata.rs \
  crates/wyrd/wyrd-sql/src/queries/storage/idempotency.rs; then
  echo 'tenant-scoped storage queries must use TenantConn, not PgPool'
  exit 1
fi

if rg_optional 'PendingMap|storage_pending' \
  crates/wyrd/wyrd-server/src \
  crates/wyrd-spec/src; then
  echo 'storage uploads must not rely on in-memory cross-request state'
  exit 1
fi

if rg -n --glob '*.rs' 'wire_protocol\s*==\s*"' crates; then
  echo 'WireProtocol dispatch must not use string equality'
  exit 1
fi

if rg -n --type rust 'ArtifactRef' crates/wyrd-spec/src/storage 2>/dev/null; then
  echo 'storage must not introduce a second artifact identity surface'
  exit 1
fi

if ! rg -q '#\[serde\(tag = "protocol", content = "data"' crates/wyrd-spec/src/storage/upload.rs; then
  echo 'UploadPlan must remain a closed tagged union'
  exit 1
fi

if rg_optional '\.unwrap\(\)' crates/wyrd/wyrd-storage/src; then
  echo 'unwrap is forbidden in non-test storage code'
  exit 1
fi
