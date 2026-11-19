//! `wyrd-kms` integration target.
//!
//! Every provider check links into this one binary. Each submodule owns one
//! wrapping provider and is selected by its own explicitly requested lane
//! with `-E 'test(/^<module>::/)'`; ordinary lanes skip them and need no
//! credentials. A cloud check reads the normal `WYRD_KMS_*` configuration and
//! the provider's standard credential chain, and fails naming the missing
//! setting rather than skipping.

mod integration_aws;
mod integration_gcp;
mod integration_vault;

use wyrd_kms::{
    KmsBackendKind, KmsConfig, KmsError, SecretContext, SecretPurpose, SecretScope, WyrdKms,
};
use wyrd_spec::DataTenantId;

/// Fixed value every cloud check seals.
const VALUE: &[u8] = b"wyrd-kms-integration-value";

/// A production facade from the `WYRD_KMS_*` environment, which must select
/// `backend`.
///
/// # Panics
/// Panics, naming the setting, when the environment is unreadable, selects
/// another backend, or the facade refuses its configuration or credentials.
fn facade(backend: KmsBackendKind) -> WyrdKms {
    let mut config = KmsConfig::default();
    config.apply_env().expect("WYRD_KMS_* is readable");
    assert_eq!(
        config.backend,
        Some(backend),
        "WYRD_KMS_BACKEND must select {}, with WYRD_KMS_ACTIVE_KEY naming a provisioned key",
        backend.as_str()
    );
    WyrdKms::new(&config, true).expect("the configured key and provider credentials load")
}

/// Seal a fixed value through `backend`, open it with a second facade built
/// from the same configuration, and refuse it under another owner.
///
/// # Panics
/// Panics when either facade cannot be built, sealing or opening fails, the
/// opened value differs, or the provider opens it for another owner.
async fn seals_reopens_and_refuses_another_owner(backend: KmsBackendKind) {
    let tenant = SecretScope::Tenant(DataTenantId::new_v7());
    let owner = SecretContext {
        scope: tenant,
        purpose: SecretPurpose::OperatorConnection,
        owner: "integration-connection",
        authority: &["http", "integration"],
        version: Some(1),
    };
    let sealed = facade(backend).seal(&owner, VALUE).await.expect("seals");

    let reader = facade(backend);
    assert_eq!(
        reader
            .open(&owner, &sealed)
            .await
            .expect("a second facade opens it")
            .as_slice(),
        VALUE
    );
    let other = SecretContext {
        owner: "another-connection",
        ..owner
    };
    assert_eq!(
        reader.open(&other, &sealed).await.map(|_| ()),
        Err(KmsError::Authentication),
        "the provider refuses the envelope under another owner"
    );
}
