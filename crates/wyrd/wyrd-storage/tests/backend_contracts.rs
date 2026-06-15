#[test]
fn azure_abort_dispatch_is_not_silent_success() {
    let signer_source = include_str!("../src/signer.rs");

    assert!(
        !signer_source.contains("Self::Local(_) | Self::Azure(_) => Ok(())"),
        "Azure abort must not share the local no-op path"
    );
    assert!(
        signer_source.contains("Self::Azure(signer) => signer.abort_multipart(path).await"),
        "Azure abort must dispatch to AzureSigner"
    );
}
