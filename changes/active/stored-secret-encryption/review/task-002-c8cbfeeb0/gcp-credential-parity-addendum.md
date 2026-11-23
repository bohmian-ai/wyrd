# Pending input for the next TASK-002 review

The maintainer requests that the next `wyrd-task-review` assess Google credential
parity and include the gap in its remediation task where applicable. Only
`wyrd-task-review` creates remediation tasks. This note is neither a task nor a
validated finding, verdict amendment, or implementation authorization.

Requested outcome: an administrator supplying storage credentials through
`GOOGLE_ACCOUNT_JSON_BASE64` or `GOOGLE_APPLICATION_CREDENTIALS_JSON` can use
the same credential source for Cloud KMS, with the required KMS permissions.

Source observation at `d725e6e11c3e3c7d07f5a6f807f6a5e3336ae051`:
`wyrd-storage/src/factory/mod.rs::service_account_json` handles these inputs and
`cloud.rs::CloudStore::gcs` applies them; `wyrd-kms/src/gcp.rs::GcpKms::new`
uses the provider environment builder without that resolution. The next review
must assess the then-current candidate and existing credential-selection behavior.
No production change or runtime verification is recorded by this note.
