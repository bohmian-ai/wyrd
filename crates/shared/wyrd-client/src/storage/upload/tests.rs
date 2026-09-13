use std::sync::Arc;
use std::sync::Mutex;

use crate::WyrdClient;
use crate::auth::AuthMiddleware;
use crate::config::ClientConfig;
use crate::transport::HttpTransport;
use crate::transport::config::HttpConfig;
use crate::transport::credential::ResolvedCredential;

use super::{ProgressCallback, UploadHooks, UploadOutcome, report, validate_plan};
use crate::storage::WyrdStorageClient;
use crate::storage::error::StorageClientError;
use wyrd_spec::storage::{S3MultipartComplete, UploadCompleteRequest};

fn test_auth_middleware() -> Result<Arc<AuthMiddleware>, crate::error::WyrdClientError> {
    AuthMiddleware::new(
        &ClientConfig::default(),
        ResolvedCredential::BearerToken("test-bearer".to_owned().into()),
    )
}

fn client() -> WyrdClient {
    let config = ClientConfig::default();
    let auth = test_auth_middleware().expect("test_setup: auth builds");
    let http = HttpTransport::new(&HttpConfig::default(), Arc::clone(&auth)).expect("transport");
    WyrdClient::from_parts(auth, http, config.grpc)
}

#[test]
fn server_completion_is_exposed_without_protocol_matching_at_the_call_site() {
    let outcome = UploadOutcome::NeedsServerComplete(UploadCompleteRequest::S3Multipart(
        S3MultipartComplete { parts: Vec::new() },
    ));
    assert!(outcome.into_server_complete().is_some());
}

#[test]
fn malformed_dimensions_fail_before_dispatch() {
    let plan = wyrd_spec::storage::UploadPlan::GcsResumable {
        session_uri: "https://backend.invalid/session".to_owned(),
        chunk_size_bytes: 0,
    };
    assert!(matches!(
        validate_plan(&plan),
        Err(StorageClientError::PlanInvalid(_))
    ));

    let plan = wyrd_spec::storage::UploadPlan::AzureBlockBlob {
        sas_url: "https://backend.invalid/blob".to_owned(),
        block_size_bytes: 0,
        block_count_planned: 1,
    };
    assert!(matches!(
        validate_plan(&plan),
        Err(StorageClientError::PlanInvalid(_))
    ));

    let plan = wyrd_spec::storage::UploadPlan::S3Multipart {
        part_count: 0,
        part_size_bytes: 1,
        part_url_ttl_secs: 60,
        required_headers: Vec::new(),
    };
    assert!(matches!(
        validate_plan(&plan),
        Err(StorageClientError::PlanInvalid(_))
    ));
}

#[test]
fn progress_preserves_known_and_unknown_totals() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&seen);
    let callback: ProgressCallback = Arc::new(move |uploaded, total| {
        captured
            .lock()
            .expect("test mutex is not poisoned")
            .push((uploaded, total));
    });

    report(Some(&callback), 4, Some(10));
    report(Some(&callback), 7, None);

    assert_eq!(
        *seen.lock().expect("test mutex is not poisoned"),
        vec![(4, Some(10)), (7, None)]
    );
}

#[tokio::test]
async fn over_range_dimensions_fail_before_request_dispatch() {
    let plan = wyrd_spec::storage::UploadPlan::GcsResumable {
        session_uri: "https://backend.invalid/session".to_owned(),
        chunk_size_bytes: u64::MAX,
    };
    let wyrd = client();
    let error = WyrdStorageClient::new(&wyrd)
        .upload(
            &plan,
            Vec::<u8>::new(),
            UploadHooks {
                idempotency_key: "key",
                progress: None,
                part_url_minter: None,
            },
        )
        .await
        .expect_err("over-range dimensions must be rejected");
    assert!(matches!(error, StorageClientError::PlanInvalid(_)));
}
