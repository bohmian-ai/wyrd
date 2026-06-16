//! Boot-time storage preflight checks.

use crate::s3::S3Signer;
use crate::signer::BackendSigner;

/// Run non-fatal boot preflight checks for the selected backend.
pub async fn run(signer: &BackendSigner) {
    if let BackendSigner::S3(s3) = signer {
        check_s3_lifecycle(s3).await;
    }
}

/// Warn if an S3 bucket lacks an incomplete multipart upload lifecycle rule.
pub async fn check_s3_lifecycle(s3: &S3Signer) {
    let response = s3
        .client()
        .get_bucket_lifecycle_configuration()
        .bucket(s3.bucket())
        .send()
        .await;

    match response {
        Ok(output) => {
            let has_abort_rule = output.rules().iter().any(|rule| {
                rule.abort_incomplete_multipart_upload()
                    .and_then(
                        aws_sdk_s3::types::AbortIncompleteMultipartUpload::days_after_initiation,
                    )
                    .is_some()
            });
            if !has_abort_rule {
                tracing::warn!(
                    bucket = %s3.bucket(),
                    rule_id = "wyrd-abort-incomplete-multipart",
                    lifecycle_rule_missing = true,
                    remediation = "Add an AbortIncompleteMultipartUpload lifecycle rule with DaysAfterInitiation: 7 or shorter.",
                    "S3 bucket lifecycle missing AbortIncompleteMultipartUpload rule"
                );
            }
        }
        Err(source) => {
            tracing::warn!(
                bucket = %s3.bucket(),
                error = ?source,
                lifecycle_rule_missing = true,
                "S3 lifecycle preflight could not fetch bucket lifecycle configuration"
            );
        }
    }
}
