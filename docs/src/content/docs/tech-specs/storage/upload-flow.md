---
title: Storage — upload flow
description: End-to-end multipart upload across S3, GCS, Azure, and Local. Two-phase init, server-verified SHA-256, and the abort path.
---

The upload surface is four routes plus one Local-only blob writer. Each
service function is a pure `async fn(&AppState, Caller, Request) -> Result<Response, WyrdError>`
that's testable without axum.

| Method | Path | Service fn |
|---|---|---|
| POST | `/v1/cards/upload/init` | `service::upload_init` |
| POST | `/v1/cards/upload/{id}/part-url?part_number=N` | `service::upload_part_url` |
| POST | `/v1/cards/upload/{id}/complete` | `service::upload_complete` |
| POST | `/v1/cards/upload/{id}/abort` | `service::upload_abort` |
| PUT  | `/v1/cards/upload/local/{*path}` | `service::upload_local_blob` |

## End-to-end sequence (S3 multipart, the canonical case)

```mermaid
sequenceDiagram
    autonumber
    participant C as wyrd-client
    participant S as wyrd-server<br>storage::service
    participant DB as Postgres<br>storage_multipart_uploads
    participant B as S3

    Note over C,B: 1. Plan the upload
    C->>S: POST /v1/cards/upload/init<br>{card_uid, relative_path, sha256, size}
    S->>S: tenant_path::validate(path, caller.tenant)
    S->>DB: SELECT for prior pending (dedup)
    alt prior pending found
        S->>B: AbortMultipartUpload(prior_backend_upload_id)
        S->>DB: UPDATE prior row status='aborted'
    end
    S->>DB: INSERT row status='initiating'<br>(L13: insert precedes SDK)
    Note over S: drop(conn) before SDK call (M3)
    S->>B: CreateMultipartUpload(checksum_algorithm=SHA256)
    B-->>S: backend_upload_id
    S->>DB: UPDATE row status='pending'<br>backend_upload_id, expected_size, sha256
    S-->>C: UploadInitResponse{<br>  upload_id, backend=S3,<br>  plan: S3Multipart{part_urls: []},<br>  storage_path<br>}

    Note over C,B: 2. Upload parts (one presign per part, L5)
    loop part_number ∈ 1..N
        C->>S: POST /v1/cards/upload/{id}/part-url?part_number=N
        S->>B: PresignPart(upload_id, part_number)
        B-->>S: presigned PUT URL
        S-->>C: { put_url, ttl_secs }
        C->>B: PUT presigned URL (streamed)
        B-->>C: 200 OK + ETag
    end

    Note over C,B: 3. Complete (server-driven, L15)
    C->>S: POST /v1/cards/upload/{id}/complete<br>{ S3: { parts: [{number, etag}, ...] } }
    S->>DB: SELECT row by id<br>(backend_upload_id, sha256, size)
    S->>B: CompleteMultipartUpload(<br>  backend_upload_id, parts<br>)
    B-->>S: 200 OK
    S->>B: HEAD object (read size + checksum header)
    S->>S: BackendSigner::verify_sha256(...)<br>(L17: backend checksum is primary)
    alt SHA mismatch
        S->>DB: UPDATE row status='failed' error='sha_mismatch'
        S->>DB: INSERT audit_log entry
        S-->>C: 400 WYRD_STORAGE_400_SHA256_MISMATCH
    else verified
        S->>DB: INSERT artifact_metadata
        S->>DB: UPDATE row status='completed'
        S->>DB: INSERT audit_log entry
        S-->>C: UploadCompleteResponse{ stored: StoredObjectRef }
    end
```

## Per-backend deltas

The init / part / complete shape is the same. The bytes-on-the-wire and
the server's commit step differ.

### Local (filesystem)

```mermaid
sequenceDiagram
    participant C as wyrd-client
    participant S as wyrd-server

    C->>S: POST /v1/cards/upload/init
    S-->>C: UploadInitResponse{ plan: LocalFsV1{ put_url: /v1/cards/upload/local/{*path} } }
    C->>S: PUT /v1/cards/upload/local/{path}<br>(streamed body)
    S->>S: write to temp file
    S->>S: fsync + rename(target)
    S->>S: BackendSigner::verify_sha256(...)
    S-->>C: { uploaded: true }
    C->>S: POST /v1/cards/upload/{id}/complete<br>{ Local: {} }
    S-->>C: UploadCompleteResponse
```

### GCS resumable

```mermaid
sequenceDiagram
    participant C as wyrd-client
    participant S as wyrd-server
    participant G as Google Cloud Storage

    C->>S: POST /v1/cards/upload/init
    S->>G: POST resumable upload init (server credentials)
    G-->>S: session_uri (bearer — L2, server returns once, never persists)
    S-->>C: UploadInitResponse{ plan: GcsResumable{ session_uri } }
    loop chunked PUT
        C->>G: PUT session_uri Content-Range: bytes X-Y/total<br>(client drives finalization)
    end
    G-->>C: 200 OK
    C->>S: POST /v1/cards/upload/{id}/complete<br>{ Gcs: {} }
    S->>G: HEAD object (server credentials)
    S->>S: verify_sha256 via x-goog-hash OR stream-recompute<br>(L15 server-verified, L17 SHA)
    S-->>C: UploadCompleteResponse
```

### Azure block blob

```mermaid
sequenceDiagram
    participant C as wyrd-client
    participant S as wyrd-server
    participant A as Azure Blob Storage

    C->>S: POST /v1/cards/upload/init
    S->>A: issue SAS (server credentials)
    A-->>S: SAS token (bearer — L2, server returns once, never persists)
    S-->>C: UploadInitResponse{ plan: AzureBlock{ sas, block_count: N } }
    loop block_id ∈ derived from 0..N
        C->>A: PUT block?blockid=BASE64(format!("{:06}", idx))<br>(deterministic IDs server can reconstruct)
    end
    C->>S: POST /v1/cards/upload/{id}/complete<br>{ Azure: {} }
    Note over S: Server reconstructs block IDs from persisted block_count_planned (L12)
    S->>A: PutBlockList(blocks=derived) via credentialed BlobClient
    S->>A: HEAD object
    S->>S: verify_sha256 (stream-recompute — Azure has no checksum trailer)
    S-->>C: UploadCompleteResponse
```

## Two-phase init (L13)

The `initiating → pending` split closes a race where the SDK
`CreateMultipartUpload` succeeds but the server crashes before persisting
the `backend_upload_id`. Without two-phase init, that crash would leak a
backend upload with no row to track it.

```mermaid
flowchart LR
    A[INSERT row<br>status=initiating] --> B[drop tenant conn]
    B --> C[SDK CreateMultipartUpload]
    C -->|success| D[acquire tenant conn]
    D --> E[UPDATE row<br>status=pending<br>backend_upload_id]
    C -->|failure| F[row stays 'initiating']
    F -.->|after init_grace_secs| G[Sweeper picks up<br>aborts backend upload<br>UPDATE status=aborted]
```

The connection drops between step (1) and step (3) because the SDK call
is the slow path; holding a Postgres connection across it would burn pool
slots under load (M3).

## Re-init dedup

If a client re-runs `register` after a crash, the second `upload_init`
with the same `(card_uid, expected_sha256)` finds the prior pending row.
The service:

1. Aborts the prior backend upload via `BackendSigner::abort_multipart`.
2. Marks the prior row `aborted`.
3. Issues a fresh `UploadId` and starts over.

A rare race — two concurrent `upload_init` calls — can both pass
`find_pending_for_dedupe` while both rows are still `initiating`. The
partial unique index on `(data_tenant_id, card_uid, expected_sha256) WHERE status = 'pending'` catches them at `mark_pending` time. The
second caller's UPDATE hits `UniqueViolation` and the service returns a
409 with documented remediation: "retry the upload; the first caller's
pending plan is already live."

This is a **409, not a 500**.

## SHA-256 verification matrix (L17)

`BackendSigner::verify_sha256(path, expected, head_hint)` runs after HEAD
and before `mark_completed`. The primary source is the backend-supplied
checksum where available; streaming recompute is the fallback.

| Backend | Primary | Fallback |
|---|---|---|
| S3 | `x-amz-checksum-sha256` (set via `ChecksumAlgorithm::SHA256` on `CreateMultipartUpload`) | `wyrd_storage::sha::stream_sha256` |
| GCS | `x-goog-hash` from HEAD response | `wyrd_storage::sha::stream_sha256` |
| Azure | — (no trailer support on the standard PUT path) | `wyrd_storage::sha::stream_sha256` |
| Local | — | `wyrd_storage::sha::stream_sha256` |

Mismatch fails closed:

```rust
mark_failed(&mut conn, upload_id, "sha_mismatch").await?;
return Err(WyrdStorageError::Sha256Mismatch { ... });
```

## Abort path

```mermaid
sequenceDiagram
    participant C as wyrd-client
    participant S as wyrd-server
    participant B as backend
    participant DB as Postgres

    C->>S: POST /v1/cards/upload/{id}/abort
    S->>DB: SELECT row by id
    alt row status terminal
        S-->>C: 409 WYRD_STORAGE_409_UPLOAD_NOT_PENDING
    else status ∈ {initiating, pending}
        S->>B: backend abort_multipart(backend_upload_id)
        Note over S,B: best-effort — backend failure logged + counted, not surfaced
        S->>DB: UPDATE row status='aborted'
        S->>DB: INSERT audit_log entry
        S-->>C: AbortResponse
    end
```

The abort is best-effort against the backend because the durable
contract is the row state. A backend abort failure is logged and bumps
`sweeper.row_failure_total{backend, error_class}`; the next sweeper tick
picks it up (or a future re-init dedup does).

## Idempotency (route-level)

`upload_init` accepts an idempotency key in the request body (not a
header — bearer-free, L2). The first call writes `(key, response_hash, expires_at)` to `wyrd.storage_idempotency_keys` and returns. Repeats
within the TTL replay the cached response without touching the backend.

The reaper that cleans expired keys is part of the storage sweeper —
see [Lifecycle](/tech-specs/storage/lifecycle/).

## Authz

Every upload route gates on `Scope::CardWrite`. Failures return
`WyrdError::InsufficientScope` directly (not through any storage-specific
mapper). The single `IntoResponse for WyrdError` impl in
`wyrd-server::error` (commit 00a) renders both `WyrdError` and lifted
`WyrdStorageError` into the same problem+json envelope.
