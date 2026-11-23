//! `WyrdKms` over real Google Cloud KMS, run by `test:kms:gcp:cloud`.
//!
//! Proves what the protocol unit tests cannot: Cloud KMS accepts application
//! default credentials, answers the adapter's request shapes, and binds the
//! envelope to its additional authenticated data.

use wyrd_kms::KmsBackendKind;

/// Seal, reopen with another facade, and refuse another owner through Cloud KMS.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires a provisioned Cloud KMS key and credentials; run by test:kms:gcp:cloud"]
async fn gcp_seals_reopens_and_refuses_another_owner() {
    super::seals_reopens_and_refuses_another_owner(KmsBackendKind::GcpKms).await;
}
