//! GCP KMS, AWS KMS, and Vault Transit adapters at their HTTP and
//! authentication boundaries, against local protocol fixtures.
//!
//! The fixture behaves like the provider: it wraps by sealing the plaintext
//! together with the authenticated context it was given, and refuses a
//! decrypt whose context differs, so a passing open proves the adapter sent
//! the exact canonical context both times. These are local protocol checks,
//! not live-cloud proof.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use object_store::CredentialProvider;
use object_store::aws::AwsCredential;
use object_store::gcp::GcpCredential;
use serde_json::{Value, json};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};
use wyrd_spec::DataTenantId;

use crate::aws::AwsKms;
use crate::config::{VaultSettings, VaultToken};
use crate::gcp::GcpKms;
use crate::transit::TransitKms;
use crate::{
    Backend, KmsBackendKind, KmsError, ProviderFailure, SecretContext, SecretPurpose, SecretScope,
    WRAP_DOMAIN, WyrdKms,
};

/// GCP CryptoKeys the tests configure.
const GCP_OLD: &str = "projects/p/locations/global/keyRings/r/cryptoKeys/wyrd-secrets";
const GCP_NEW: &str = "projects/p/locations/global/keyRings/r/cryptoKeys/wyrd-replacement";

/// AWS key ARNs the tests configure.
const AWS_OLD: &str = "arn:aws:kms:us-east-1:111122223333:key/1111aaaa-1111-1111-1111-111111111111";
const AWS_NEW: &str = "arn:aws:kms:us-east-1:111122223333:key/2222bbbb-2222-2222-2222-222222222222";

/// Short call bound the timeout case exceeds.
const TIMEOUT: Duration = Duration::from_millis(300);

/// A Gateway managed-credential context.
fn context(tenant: DataTenantId) -> SecretContext<'static> {
    SecretContext {
        scope: SecretScope::Tenant(tenant),
        purpose: SecretPurpose::GatewayProviderCredential,
        owner: "openai-prod",
        authority: &["openai"],
        version: None,
    }
}

/// Credentials that change on every request, standing in for a provider
/// chain that refreshes an expiring token.
#[derive(Debug)]
struct Rotating<T> {
    /// Requests served so far.
    served: AtomicUsize,
    /// Credential for the n-th request.
    make: fn(usize) -> T,
}

#[async_trait::async_trait]
impl<T: std::fmt::Debug + Send + Sync> CredentialProvider for Rotating<T> {
    type Credential = T;

    async fn get_credential(&self) -> object_store::Result<Arc<T>> {
        Ok(Arc::new((self.make)(
            self.served.fetch_add(1, Ordering::SeqCst),
        )))
    }
}

/// A credential chain that cannot produce a credential.
#[derive(Debug)]
struct Failing;

#[async_trait::async_trait]
impl CredentialProvider for Failing {
    type Credential = GcpCredential;

    async fn get_credential(&self) -> object_store::Result<Arc<GcpCredential>> {
        Err(object_store::Error::Generic {
            store: "test",
            source: "metadata server unreachable".into(),
        })
    }
}

/// GCP bearer tokens `token-0`, `token-1`, ….
fn gcp_tokens() -> Arc<Rotating<GcpCredential>> {
    Arc::new(Rotating {
        served: AtomicUsize::new(0),
        make: |n| GcpCredential {
            bearer: format!("token-{n}"),
        },
    })
}

/// AWS access keys `AKID0`, `AKID1`, ….
fn aws_keys() -> Arc<Rotating<AwsCredential>> {
    Arc::new(Rotating {
        served: AtomicUsize::new(0),
        make: |n| AwsCredential {
            key_id: format!("AKID{n}"),
            secret_key: "secret-canary".to_owned(),
            token: None,
        },
    })
}

/// Field names of one provider's wrap protocol.
#[derive(Clone, Copy)]
struct Protocol {
    /// Plaintext field of the encrypt request and decrypt answer.
    plaintext: &'static str,
    /// Ciphertext field of the encrypt answer and decrypt request.
    ciphertext: &'static str,
    /// Where the request carries its authenticated context.
    context: fn(&Value) -> Value,
    /// Whether this request encrypts.
    encrypting: fn(&Request) -> bool,
    /// Status and body a context mismatch answers.
    mismatch: (u16, fn() -> Value),
}

/// Cloud KMS: `:encrypt` / `:decrypt` with `additionalAuthenticatedData`.
const GCP: Protocol = Protocol {
    plaintext: "plaintext",
    ciphertext: "ciphertext",
    context: |body| body["additionalAuthenticatedData"].clone(),
    encrypting: |request| request.url.path().ends_with(":encrypt"),
    mismatch: (400, || json!({"error": {"status": "INVALID_ARGUMENT"}})),
};

/// AWS KMS JSON actions with the `wyrd-context` encryption context.
const AWS: Protocol = Protocol {
    plaintext: "Plaintext",
    ciphertext: "CiphertextBlob",
    context: |body| body["EncryptionContext"].clone(),
    encrypting: |request| {
        request
            .headers
            .get("x-amz-target")
            .is_some_and(|target| target == "TrentService.Encrypt")
    },
    mismatch: (400, || json!({"__type": "InvalidCiphertextException"})),
};

/// A fixture provider: encrypt seals `{context, plaintext}` into an opaque
/// blob; decrypt opens it only under the same context.
struct FakeKms(Protocol);

impl Respond for FakeKms {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let protocol = self.0;
        let body: Value = serde_json::from_slice(&request.body).expect("json request");
        let context = (protocol.context)(&body);
        if (protocol.encrypting)(request) {
            let blob = json!({"context": context, "plaintext": body[protocol.plaintext]});
            return ResponseTemplate::new(200)
                .set_body_json(json!({protocol.ciphertext: STANDARD.encode(blob.to_string())}));
        }
        let blob: Value = body[protocol.ciphertext]
            .as_str()
            .and_then(|encoded| STANDARD.decode(encoded).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        if blob["context"] != context {
            return ResponseTemplate::new(protocol.mismatch.0)
                .set_body_json((protocol.mismatch.1)());
        }
        ResponseTemplate::new(200).set_body_json(json!({protocol.plaintext: blob["plaintext"]}))
    }
}

/// A fixture server answering every POST like `protocol`'s provider.
async fn fake(protocol: Protocol) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(FakeKms(protocol))
        .mount(&server)
        .await;
    server
}

/// A GCP facade over `server` with `active` and `retained` CryptoKeys.
fn gcp(server: &MockServer, active: &str, retained: &[&str]) -> WyrdKms {
    WyrdKms::with_backend(
        KmsBackendKind::GcpKms,
        active,
        retained,
        Backend::Gcp(
            GcpKms::with_credentials(&server.uri(), gcp_tokens(), TIMEOUT).expect("builds"),
        ),
    )
}

/// An AWS facade over `server` with `active` and `retained` key ARNs.
fn aws(server: &MockServer, active: &str, retained: &[&str]) -> WyrdKms {
    WyrdKms::with_backend(
        KmsBackendKind::AwsKms,
        active,
        retained,
        Backend::Aws(
            AwsKms::with_credentials(Some(&server.uri()), aws_keys(), TIMEOUT).expect("builds"),
        ),
    )
}

/// Transit fixture: `vault:v1:` ciphertext sealing `{context,
/// plaintext}`; a decrypt under another context answers 400.
struct Transit;
impl Respond for Transit {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).expect("json");
        if request.url.path().contains("/encrypt/") {
            let blob = json!({"context": body["associated_data"], "plaintext": body["plaintext"]});
            let ciphertext = format!("vault:v1:{}", STANDARD.encode(blob.to_string()));
            return ResponseTemplate::new(200)
                .set_body_json(json!({"data": {"ciphertext": ciphertext}}));
        }
        let blob: Value = body["ciphertext"]
            .as_str()
            .and_then(|text| text.strip_prefix("vault:v1:"))
            .and_then(|encoded| STANDARD.decode(encoded).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        if blob["context"] != body["associated_data"] {
            return ResponseTemplate::new(400)
                .set_body_json(json!({"errors": ["cipher: message authentication failed"]}));
        }
        ResponseTemplate::new(200).set_body_json(json!({"data": {"plaintext": blob["plaintext"]}}))
    }
}
/// A Transit fixture server.
async fn transit_fake() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(Transit)
        .mount(&server)
        .await;
    server
}

/// Requests the fixture received.
async fn requests(server: &MockServer) -> Vec<Request> {
    server.received_requests().await.expect("recording")
}

/// GCP: seal wraps the facade's DEK through the active CryptoKey with the
/// exact canonical context and a freshly obtained token; a facade that
/// rotated to a replacement key opens the old envelope through the retained
/// CryptoKey named in it; a substituted context is refused by KMS.
#[tokio::test]
async fn gcp_wraps_through_the_configured_crypto_key_with_exact_context() {
    let server = fake(GCP).await;
    let ctx = context(DataTenantId::new_v7());
    let sealed = gcp(&server, GCP_OLD, &[])
        .seal(&ctx, b"sk-live")
        .await
        .expect("seals");
    let rotated = gcp(&server, GCP_NEW, &[GCP_OLD]);
    assert_eq!(
        rotated.open(&ctx, &sealed).await.expect("opens").as_slice(),
        b"sk-live"
    );

    let seen = requests(&server).await;
    assert_eq!(seen[0].url.path(), format!("/v1/{GCP_OLD}:encrypt"));
    assert_eq!(
        seen[1].url.path(),
        format!("/v1/{GCP_OLD}:decrypt"),
        "retained key selected"
    );
    let expected = STANDARD.encode(ctx.canonical(WRAP_DOMAIN));
    for request in &seen {
        let body: Value = serde_json::from_slice(&request.body).expect("json");
        assert_eq!(body["additionalAuthenticatedData"], expected.as_str());
    }
    let bearers: Vec<_> = seen
        .iter()
        .map(|request| {
            request.headers["authorization"]
                .to_str()
                .expect("ascii")
                .to_owned()
        })
        .collect();
    assert_eq!(
        bearers,
        ["Bearer token-0", "Bearer token-0"],
        "each facade has its own chain"
    );

    let refreshed = gcp(&server, GCP_OLD, &[]);
    refreshed.seal(&ctx, b"a").await.expect("seals");
    refreshed.seal(&ctx, b"b").await.expect("seals");
    let seen = requests(&server).await;
    assert_eq!(seen[2].headers["authorization"], "Bearer token-0");
    assert_eq!(
        seen[3].headers["authorization"], "Bearer token-1",
        "credential re-read per call"
    );

    let other = SecretContext {
        owner: "openai-staging",
        ..ctx
    };
    assert_eq!(
        rotated.open(&other, &sealed).await.map(|_| ()),
        Err(KmsError::Authentication)
    );
}

/// AWS: the same envelope flow over SigV4-signed KMS JSON actions, with the
/// key ARN and encryption context in every request and a refreshed access
/// key on each call.
#[tokio::test]
async fn aws_wraps_through_the_configured_key_arn_with_exact_context() {
    let server = fake(AWS).await;
    let ctx = context(DataTenantId::new_v7());
    let sealed = aws(&server, AWS_OLD, &[])
        .seal(&ctx, b"sk-live")
        .await
        .expect("seals");
    let rotated = aws(&server, AWS_NEW, &[AWS_OLD]);
    assert_eq!(
        rotated.open(&ctx, &sealed).await.expect("opens").as_slice(),
        b"sk-live"
    );

    let seen = requests(&server).await;
    let expected = json!({"wyrd-context": STANDARD.encode(ctx.canonical(WRAP_DOMAIN))});
    for (request, target) in seen
        .iter()
        .zip(["TrentService.Encrypt", "TrentService.Decrypt"])
    {
        let body: Value = serde_json::from_slice(&request.body).expect("json");
        assert_eq!(request.headers["x-amz-target"], target);
        assert_eq!(body["KeyId"], AWS_OLD, "the envelope's configured key");
        assert_eq!(body["EncryptionContext"], expected);
        let authorization = request.headers["authorization"].to_str().expect("ascii");
        assert!(
            authorization.starts_with("AWS4-HMAC-SHA256 Credential=AKID0/")
                && authorization.contains("/us-east-1/kms/aws4_request"),
            "{authorization}"
        );
    }

    let refreshed = aws(&server, AWS_OLD, &[]);
    refreshed.seal(&ctx, b"a").await.expect("seals");
    refreshed.seal(&ctx, b"b").await.expect("seals");
    let seen = requests(&server).await;
    assert!(
        seen[3].headers["authorization"]
            .to_str()
            .expect("ascii")
            .contains("Credential=AKID1/")
    );

    let other = SecretContext {
        version: Some(2),
        ..ctx
    };
    assert_eq!(
        rotated.open(&other, &sealed).await.map(|_| ()),
        Err(KmsError::Authentication)
    );
}

/// Denial, timeout, and credential failures refuse with a redacted class,
/// return no envelope, and an unconfigured key reaches no provider.
#[tokio::test]
async fn provider_failures_refuse_redacted_and_disallowed_keys_send_nothing() {
    let ctx = context(DataTenantId::new_v7());
    let server = fake(GCP).await;
    let sealed = gcp(&server, GCP_OLD, &[])
        .seal(&ctx, b"sk-live")
        .await
        .expect("seals");
    server.reset().await;
    assert_eq!(
        gcp(&server, GCP_NEW, &[])
            .open(&ctx, &sealed)
            .await
            .map(|_| ()),
        Err(KmsError::KeyNotAllowed)
    );
    assert!(
        requests(&server).await.is_empty(),
        "no backend request for a disallowed key"
    );

    let canaries = [
        server.uri(),
        GCP_OLD.to_owned(),
        "token-0".to_owned(),
        "secret-canary".to_owned(),
    ];
    let cases = [
        (ResponseTemplate::new(403), ProviderFailure::Refused),
        (ResponseTemplate::new(503), ProviderFailure::Transport),
        (
            ResponseTemplate::new(200).set_delay(TIMEOUT * 3),
            ProviderFailure::Timeout,
        ),
    ];
    for (answer, failure) in cases {
        server.reset().await;
        Mock::given(method("POST"))
            .respond_with(answer)
            .mount(&server)
            .await;
        for kms in [gcp(&server, GCP_OLD, &[]), aws(&server, AWS_OLD, &[])] {
            let backend = kms.kind;
            let sealed_error = kms.seal(&ctx, b"sk-live").await.expect_err("refused");
            assert_eq!(sealed_error, KmsError::Unavailable { backend, failure });
            let text = format!("{sealed_error} {sealed_error:?}");
            assert!(
                canaries
                    .iter()
                    .all(|canary| !text.contains(canary.as_str())),
                "{text}"
            );
        }
    }

    let no_credential = WyrdKms::with_backend(
        KmsBackendKind::GcpKms,
        GCP_OLD,
        &[],
        Backend::Gcp(
            GcpKms::with_credentials(&server.uri(), Arc::new(Failing), TIMEOUT).expect("builds"),
        ),
    );
    assert_eq!(
        no_credential.seal(&ctx, b"sk-live").await,
        Err(KmsError::Unavailable {
            backend: KmsBackendKind::GcpKms,
            failure: ProviderFailure::Credential,
        })
    );
}

/// Transit: the facade wraps through the configured Transit key with the
/// exact context as `associated_data`, re-reads a renewed token file, and a
/// substituted context is refused.
#[tokio::test]
async fn transit_wraps_through_the_configured_key_and_rereads_its_token() {
    let server = transit_fake().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let token_file = dir.path().join("token");
    let write_token = |token: &str| {
        std::fs::write(&token_file, token).expect("writes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&token_file, std::fs::Permissions::from_mode(0o600))
                .expect("owner-only");
        }
    };
    write_token("hvs.first\n");
    let kms = WyrdKms::with_backend(
        KmsBackendKind::VaultTransit,
        "wyrd-secrets",
        &[],
        Backend::Transit(
            TransitKms::new(
                url::Url::parse(&server.uri()).expect("url"),
                VaultSettings {
                    mount: "transit".to_owned(),
                    namespace: None,
                    token: VaultToken::File(token_file.clone()),
                },
            )
            .expect("builds"),
        ),
    );
    let ctx = context(DataTenantId::new_v7());
    let sealed = kms.seal(&ctx, b"sk-live").await.expect("seals");
    write_token("hvs.renewed\n");
    assert_eq!(
        kms.open(&ctx, &sealed).await.expect("opens").as_slice(),
        b"sk-live"
    );
    let seen = requests(&server).await;
    assert_eq!(seen[0].url.path(), "/v1/transit/encrypt/wyrd-secrets");
    assert_eq!(seen[1].url.path(), "/v1/transit/decrypt/wyrd-secrets");
    assert_eq!(seen[0].headers["x-vault-token"], "hvs.first");
    assert_eq!(
        seen[1].headers["x-vault-token"], "hvs.renewed",
        "token file re-read"
    );
    let expected = STANDARD.encode(ctx.canonical(WRAP_DOMAIN));
    for request in &seen {
        let body: Value = serde_json::from_slice(&request.body).expect("json");
        assert_eq!(body["associated_data"], expected.as_str());
    }
    let other = SecretContext {
        scope: SecretScope::Platform,
        ..ctx
    };
    assert_eq!(
        kms.open(&other, &sealed).await.map(|_| ()),
        Err(KmsError::Unavailable {
            backend: KmsBackendKind::VaultTransit,
            failure: ProviderFailure::Refused,
        })
    );
}

/// A provider failure during renewal or rewrap returns no replacement
/// envelope; the old one stays readable once the provider recovers.
#[tokio::test]
async fn renew_and_rewrap_return_nothing_when_the_provider_fails() {
    let ctx = context(DataTenantId::new_v7());
    let server = fake(GCP).await;
    let old = gcp(&server, GCP_OLD, &[])
        .seal(&ctx, b"sk-live")
        .await
        .expect("seals");
    let rotating = gcp(&server, GCP_NEW, &[GCP_OLD]);
    server.reset().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let unavailable = Err(KmsError::Unavailable {
        backend: KmsBackendKind::GcpKms,
        failure: ProviderFailure::Transport,
    });
    assert_eq!(rotating.renew(&ctx, &old).await, unavailable);
    assert_eq!(rotating.rewrap(&ctx, &old).await, unavailable);
    server.reset().await;
    Mock::given(method("POST"))
        .respond_with(FakeKms(GCP))
        .mount(&server)
        .await;
    assert_eq!(
        rotating.open(&ctx, &old).await.expect("opens").as_slice(),
        b"sk-live"
    );
    let rewrapped = rotating.rewrap(&ctx, &old).await.expect("rewraps");
    assert_eq!(rotating.is_current(&rewrapped), Ok(true));
    assert!(
        requests(&server)
            .await
            .iter()
            .any(|request| request.url.path() == format!("/v1/{GCP_NEW}:encrypt")),
        "rewrap wraps through the active CryptoKey"
    );
}

/// A Transit backend over `server` with an inline token.
fn transit(server: &MockServer) -> TransitKms {
    TransitKms::new(
        url::Url::parse(&server.uri()).expect("url"),
        VaultSettings {
            mount: "transit".to_owned(),
            namespace: None,
            token: VaultToken::Inline("hvs.inline".to_owned().into()),
        },
    )
    .expect("builds")
}

/// Every remote backend hands back an unwrapped DEK whose `Debug` reveals
/// none of its bytes.
#[tokio::test]
async fn remote_unwrapped_deks_have_redacted_debug() {
    let dek = [0xAB_u8; 32];
    let aad = b"context";
    let gcp_server = fake(GCP).await;
    let aws_server = fake(AWS).await;
    let transit_server = transit_fake().await;
    let gcp = GcpKms::with_credentials(&gcp_server.uri(), gcp_tokens(), TIMEOUT).expect("builds");
    let aws =
        AwsKms::with_credentials(Some(&aws_server.uri()), aws_keys(), TIMEOUT).expect("builds");
    let transit = transit(&transit_server);

    let unwrapped = [
        gcp.unwrap(
            GCP_OLD,
            &gcp.wrap(GCP_OLD, &dek, aad).await.expect("wraps"),
            aad,
        )
        .await
        .expect("unwraps"),
        aws.unwrap(
            AWS_OLD,
            &aws.wrap(AWS_OLD, &dek, aad).await.expect("wraps"),
            aad,
        )
        .await
        .expect("unwraps"),
        transit
            .unwrap(
                "k",
                &transit.wrap("k", &dek, aad).await.expect("wraps"),
                aad,
            )
            .await
            .expect("unwraps"),
    ];
    for bytes in &unwrapped {
        assert_eq!(&bytes[..], dek.as_slice());
        let debug = format!("{bytes:?}");
        assert!(!debug.contains("171"), "{debug}");
        assert!(debug.contains("REDACTED"), "{debug}");
    }
}

/// A decrypt answer is read in place from the protected response body: a
/// plaintext that would need unescaping into a fresh, unprotected string is
/// refused as a transport failure instead.
#[tokio::test]
async fn remote_plaintext_answers_needing_a_copy_are_refused() {
    let escaped = r"\/".repeat(42) + "8=";
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"{{"plaintext":"{escaped}","Plaintext":"{escaped}","data":{{"plaintext":"{escaped}"}}}}"#
            ),
            "application/json",
        ))
        .mount(&server)
        .await;
    let gcp = GcpKms::with_credentials(&server.uri(), gcp_tokens(), TIMEOUT).expect("builds");
    let aws = AwsKms::with_credentials(Some(&server.uri()), aws_keys(), TIMEOUT).expect("builds");
    let transport = |backend| KmsError::Unavailable {
        backend,
        failure: ProviderFailure::Transport,
    };
    assert_eq!(
        gcp.unwrap(GCP_OLD, b"blob", b"aad").await.map(|_| ()),
        Err(transport(KmsBackendKind::GcpKms))
    );
    assert_eq!(
        aws.unwrap(AWS_OLD, b"blob", b"aad").await.map(|_| ()),
        Err(transport(KmsBackendKind::AwsKms))
    );
    assert_eq!(
        transit(&server)
            .unwrap("k", b"vault:v1:blob", b"aad")
            .await
            .map(|_| ()),
        Err(transport(KmsBackendKind::VaultTransit))
    );
}

/// An STS fixture: the first exchange answers `fed-0` valid for one second,
/// later exchanges `fed-<n>` valid for an hour.
struct Sts(AtomicUsize);

impl Respond for Sts {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body = String::from_utf8_lossy(&request.body);
        if !body.contains("subject_token=subject-jwt") {
            return ResponseTemplate::new(400).set_body_json(json!({"error": "invalid_grant"}));
        }
        let n = self.0.fetch_add(1, Ordering::SeqCst);
        ResponseTemplate::new(200).set_body_json(json!({
            "access_token": format!("fed-{n}"),
            "issued_token_type": "urn:ietf:params:oauth:token-type:access_token",
            "token_type": "Bearer",
            "expires_in": if n == 0 { 1 } else { 3600 },
        }))
    }
}

/// Build `GcpKms` through its ambient constructor with a Workload Identity
/// Federation ADC file whose subject token file holds `subject`.
fn federated_gcp(server: &MockServer, dir: &std::path::Path, subject: &str) -> GcpKms {
    let token_file = dir.join("subject-token");
    std::fs::write(&token_file, subject).expect("writes subject token");
    let adc = dir.join("adc.json");
    std::fs::write(
        &adc,
        json!({
            "type": "external_account",
            "audience": "//iam.googleapis.com/projects/1/locations/global/workloadIdentityPools/pool/providers/onprem",
            "subject_token_type": "urn:ietf:params:oauth:token-type:jwt",
            "token_url": format!("{}/sts", server.uri()),
            "credential_source": {"file": token_file},
        })
        .to_string(),
    )
    .expect("writes adc");
    let endpoint = url::Url::parse(&server.uri()).expect("url");
    temp_env::with_var("GOOGLE_APPLICATION_CREDENTIALS", Some(&adc), || {
        GcpKms::new(Some(&endpoint)).expect("builds from federated ADC")
    })
}

/// A self-hosted deployment's federated workload ADC builds the GCP backend
/// through the ambient constructor: the facade seals and opens with the
/// exchanged token, and once that token expires the next request carries the
/// refreshed one. A refused exchange returns a redacted credential failure
/// and no replacement envelope.
#[tokio::test]
async fn gcp_federated_adc_builds_through_the_ambient_constructor_and_refreshes() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(wiremock::matchers::path("/sts"))
        .respond_with(Sts(AtomicUsize::new(0)))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(FakeKms(GCP))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let kms = WyrdKms::with_backend(
        KmsBackendKind::GcpKms,
        GCP_OLD,
        &[],
        Backend::Gcp(federated_gcp(&server, dir.path(), "subject-jwt")),
    );
    let ctx = context(DataTenantId::new_v7());
    let sealed = kms.seal(&ctx, b"sk-live").await.expect("seals");
    assert_eq!(
        kms.open(&ctx, &sealed).await.expect("opens").as_slice(),
        b"sk-live"
    );
    // Let the one-second token expire; this waits on the credential's
    // lifetime, not on another task.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    kms.seal(&ctx, b"sk-next")
        .await
        .expect("seals after refresh");
    let kms_bearers: Vec<String> = requests(&server)
        .await
        .iter()
        .filter(|request| request.url.path().starts_with("/v1/"))
        .map(|request| {
            request.headers["authorization"]
                .to_str()
                .expect("ascii")
                .to_owned()
        })
        .collect();
    assert_eq!(
        kms_bearers,
        ["Bearer fed-0", "Bearer fed-0", "Bearer fed-1"],
        "the expired exchanged token is replaced"
    );

    let refused_dir = tempfile::tempdir().expect("tempdir");
    let refused = WyrdKms::with_backend(
        KmsBackendKind::GcpKms,
        GCP_OLD,
        &[],
        Backend::Gcp(federated_gcp(&server, refused_dir.path(), "revoked-jwt")),
    );
    let credential = KmsError::Unavailable {
        backend: KmsBackendKind::GcpKms,
        failure: ProviderFailure::Credential,
    };
    let error = refused.renew(&ctx, &sealed).await.expect_err("refused");
    assert_eq!(error, credential);
    assert_eq!(refused.seal(&ctx, b"sk-live").await, Err(credential));
    let text = format!("{error} {error:?}");
    for canary in ["revoked-jwt", "invalid_grant", server.uri().as_str()] {
        assert!(!text.contains(canary), "{text}");
    }
}
