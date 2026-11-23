//! `WyrdKms` over real AWS KMS, run by `test:kms:aws:cloud`.
//!
//! Proves what the protocol unit tests cannot: AWS accepts the standard
//! credential chain and the adapter's SigV4 signing, answers the adapter's
//! request shapes, and binds the envelope to its encryption context.

use wyrd_kms::KmsBackendKind;

/// Seal, reopen with another facade, and refuse another owner through AWS KMS.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires a provisioned AWS KMS key and credentials; run by test:kms:aws:cloud"]
async fn aws_seals_reopens_and_refuses_another_owner() {
    super::seals_reopens_and_refuses_another_owner(KmsBackendKind::AwsKms).await;
}
