---
title: Storage — download flow
description: Presigned GET, range-parallel client driver, and resume-via-sidecar semantics.
---

Downloads are simpler than uploads. The server mints one presigned GET
URL; the client drives bytes with HTTP range parallelism and a
`.partial.state` sidecar for resume.

| Method | Path | Service fn |
|---|---|---|
| POST | `/v1/cards/download/init` | `service::download_init` |
| GET | `/v1/cards/download/local/{*path}` | `service::download_local_blob` (Local-only) |

## End-to-end sequence (cloud backend)

```mermaid
sequenceDiagram
    autonumber
    participant C as wyrd-client
    participant S as wyrd-server
    participant DB as Postgres<br>storage_artifact_metadata
    participant B as backend (S3 / GCS / Azure)

    Note over C,B: 1. Plan the download
    C->>S: POST /v1/cards/download/init<br>{ card_uid, relative_path, ttl_secs? }
    S->>S: Scope::CardRead check
    S->>S: tenant_path::build + validate
    S->>DB: artifact_metadata::get(validated_path)
    DB-->>S: { size_bytes, sha256, backend, ... }
    S->>B: BackendSigner::presign_get(path, ttl)
    B-->>S: presigned GET URL
    S->>DB: INSERT audit_log entry
    S-->>C: DownloadInitResponse{<br>  plan: { get_url, ttl_secs },<br>  size_bytes, sha256<br>}

    Note over C,B: 2. Range-parallel fetch
    C->>C: pre-allocate {dest}.partial (set_len(size_bytes))
    C->>C: load {dest}.partial.state if present (resume)
    C->>C: plan ranges (size / DEFAULT_RANGE_SIZE_BYTES)
    par bounded concurrency
        C->>B: GET get_url Range: bytes=0-N
    and
        C->>B: GET get_url Range: bytes=N+1-2N
    and
        C->>B: GET get_url Range: bytes=2N+1-3N
    end
    B-->>C: 206 Partial Content × N
    C->>C: write each at offset, update sidecar atomically

    Note over C,B: 3. Verify + finalize
    C->>C: stream-SHA256({dest}.partial)
    alt SHA matches expected
        C->>C: rename({dest}.partial, {dest})
        C->>C: unlink({dest}.partial.state)
    else mismatch
        C->>C: unlink partial + sidecar
        C-->>S: WyrdStorageError::Sha256Mismatch
    end
```

## Local backend (no presign)

For Local-mode deployments the presigned URL would round-trip back to
the wyrd-server itself, so the server short-circuits and streams from
disk directly:

```mermaid
sequenceDiagram
    participant C as wyrd-client
    participant S as wyrd-server
    participant FS as filesystem

    C->>S: GET /v1/cards/download/local/{*path}
    S->>S: Scope::CardRead check
    S->>S: tenant_path::validate(path, caller.tenant)
    S->>FS: tokio::fs::File::open(local.root() / validated.full)
    FS-->>S: file handle
    S-->>C: 200 OK<br>Content-Type: application/octet-stream<br>Content-Length: file.metadata().len()<br>Body: ReaderStream::new(file)
```

The route is only mounted when `backend = Local`; boot misconfiguration
fails fast with `WYRD_STORAGE_500_CONFIG_INVALID`.

## Range-parallel driver

The client driver in `wyrd-client::cards::loops::range_download` reads
the response, plans byte ranges based on
`DEFAULT_RANGE_SIZE_BYTES`, and issues bounded-concurrency GET requests
via `reqwest`. Each completed range writes at the correct offset of
`{dest}.partial` and atomically updates the sidecar.

```
{dest}.partial         ── pre-allocated to size_bytes via tokio File::set_len
{dest}.partial.state   ── serde_json with deny_unknown_fields:
                          { ranges_completed: [(start, end), ...],
                            sha256_expected: "...", size_bytes: N }
```

Atomic sidecar writes use `tempfile::NamedTempFile::persist` to avoid
torn writes. The sidecar is human-debuggable; downloads are not a
hot path (Q4).

## Resume

If the process crashes mid-download, the next `load` call sees
`{dest}.partial` and `{dest}.partial.state`, intersects
`ranges_completed` against the freshly-planned ranges, and only fetches
the missing pieces. Sidecar `sha256_expected` must match the new
`DownloadInitResponse`; mismatch aborts and starts fresh.

## SHA verification (client-side, this time)

Server-side verification happens at upload time (L17). Download
verification is client-side because the server already knows the bytes
are correct — the client wants to know the bytes-on-disk match what was
stored.

```rust
// wyrd-client::cards::loops::range_download (sketch)
let actual = stream_sha256(&partial_path).await?;
if actual != expected {
    fs::remove_file(&partial_path).await?;
    fs::remove_file(&sidecar_path).await?;
    return Err(WyrdStorageError::Sha256Mismatch { ... });
}
fs::rename(&partial_path, &dest_path).await?;
fs::remove_file(&sidecar_path).await?;
```

`stream_sha256` is the same `wyrd_storage::sha::stream_sha256` helper
the server-side fallback uses — one implementation, two callers.

## Errors

| Code | Status | Cause |
|---|---|---|
| `WYRD_STORAGE_400_TENANT_PATH_MISMATCH` | 400 | `relative_path` failed `tenant_path::validate` |
| `WYRD_STORAGE_400_SHA256_MISMATCH` | 400 | Client-side SHA differs from `DownloadInitResponse.sha256` |
| `WYRD_STORAGE_403_UPLOAD_FOREIGN_TENANT` | 403 | Local-blob path's first segment is another tenant |
| `WYRD_STORAGE_404_OBJECT_NOT_FOUND` | 404 | `artifact_metadata` returned None OR backend HEAD returned 404 |
| `WYRD_STORAGE_412_PRECONDITION` | 412 | If-Match failed during range fetch (source changed mid-download) |
| `WYRD_STORAGE_416_RANGE_NOT_SATISFIABLE` | 416 | Range past object length (source shrank) |
| `WYRD_STORAGE_503_PRESIGN_EXPIRED` | 503 | Presigned URL refused; client re-requests via `download_init` |
