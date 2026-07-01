//! Tenant-admin CRUD client over the assembled [`WyrdClient`].
//!
//! [`AdminClient`] is a thin typed projection of the server's tenant-admin
//! routes for trusted OIDC issuers and workload bindings
//! (`POST/GET/DELETE /v1/admin/trusted-issuers` and `/v1/admin/workload-bindings`).
//! Every method delegates to [`WyrdClient::request_json`], so it rides the one
//! shared auth/token/retry path the façade already owns — there is no second
//! request or credential path here. The tenant is implicit in the authenticated
//! admin credential; the client passes none and Postgres RLS scopes the write.
//!
//! Durable behavior (discovery, secret sealing, audit) lives server-side. This
//! client only sends the contract: `create` carries the plaintext client secret
//! in the request body, and the redacted [`TrustedIssuerView`] it gets back
//! never echoes a secret.

#![cfg(feature = "transport-http")]

use reqwest::Method;
use wyrd_spec::auth::{
    CreateTrustedIssuerRequest, CreateWorkloadBindingRequest, TrustedIssuerView,
    WorkloadBindingView,
};
use wyrd_spec::error::WyrdError;

use crate::client::WyrdClient;

const TRUSTED_ISSUERS_PATH: &str = "/v1/admin/trusted-issuers";
const WORKLOAD_BINDINGS_PATH: &str = "/v1/admin/workload-bindings";

/// Tenant-admin CRUD client riding the shared [`WyrdClient`] auth path.
#[derive(Debug, Clone)]
pub struct AdminClient {
    client: WyrdClient,
}

impl AdminClient {
    /// Wrap an assembled [`WyrdClient`] with the admin CRUD surface.
    #[must_use]
    pub fn new(client: WyrdClient) -> Self {
        Self { client }
    }

    /// Register a trusted OIDC issuer (`POST /v1/admin/trusted-issuers`).
    ///
    /// The plaintext client secret travels in the request body; the redacted
    /// [`TrustedIssuerView`] returned never carries it back.
    ///
    /// # Errors
    /// A duplicate issuer surfaces as [`WyrdError::AdminConflict`]; other non-2xx
    /// responses map through the server's structured error catalog.
    pub async fn create_trusted_issuer(
        &self,
        request: &CreateTrustedIssuerRequest,
    ) -> Result<TrustedIssuerView, WyrdError> {
        self.client
            .request_json(Method::POST, TRUSTED_ISSUERS_PATH, Some(request))
            .await
    }

    /// List every trusted OIDC issuer for the caller's tenant
    /// (`GET /v1/admin/trusted-issuers`).
    ///
    /// # Errors
    /// Non-2xx responses map through the server's structured error catalog.
    pub async fn list_trusted_issuers(&self) -> Result<Vec<TrustedIssuerView>, WyrdError> {
        self.client
            .request_json::<serde_json::Value, _>(Method::GET, TRUSTED_ISSUERS_PATH, None)
            .await
    }

    /// Remove a trusted OIDC issuer addressed by query string
    /// (`DELETE /v1/admin/trusted-issuers?issuer=&cascade=`).
    ///
    /// `cascade` removes referencing workload bindings first so the delete is not
    /// blocked by the FK restriction.
    ///
    /// # Errors
    /// A missing issuer surfaces as [`WyrdError::AdminNotFound`]; a delete blocked
    /// by a live binding surfaces as [`WyrdError::AdminConflict`].
    pub async fn delete_trusted_issuer(
        &self,
        issuer: &str,
        cascade: bool,
    ) -> Result<(), WyrdError> {
        let mut path = format!("{TRUSTED_ISSUERS_PATH}?issuer=");
        path.push_str(&encode(issuer));
        path.push_str("&cascade=");
        path.push_str(if cascade { "true" } else { "false" });
        self.client
            .request_json::<serde_json::Value, serde_json::Value>(Method::DELETE, &path, None)
            .await?;
        Ok(())
    }

    /// Register a workload binding (`POST /v1/admin/workload-bindings`).
    ///
    /// # Errors
    /// A duplicate `(issuer, subject)` surfaces as [`WyrdError::AdminConflict`].
    pub async fn create_workload_binding(
        &self,
        request: &CreateWorkloadBindingRequest,
    ) -> Result<WorkloadBindingView, WyrdError> {
        self.client
            .request_json(Method::POST, WORKLOAD_BINDINGS_PATH, Some(request))
            .await
    }

    /// List workload bindings for the caller's tenant, optionally filtered by
    /// exact issuer and/or subject (`GET /v1/admin/workload-bindings?issuer=&subject=`).
    ///
    /// # Errors
    /// Non-2xx responses map through the server's structured error catalog.
    pub async fn list_workload_bindings(
        &self,
        issuer: Option<&str>,
        subject: Option<&str>,
    ) -> Result<Vec<WorkloadBindingView>, WyrdError> {
        let mut path = WORKLOAD_BINDINGS_PATH.to_owned();
        let mut sep = '?';
        if let Some(issuer) = issuer {
            path.push(sep);
            path.push_str("issuer=");
            path.push_str(&encode(issuer));
            sep = '&';
        }
        if let Some(subject) = subject {
            path.push(sep);
            path.push_str("subject=");
            path.push_str(&encode(subject));
        }
        self.client
            .request_json::<serde_json::Value, _>(Method::GET, &path, None)
            .await
    }

    /// Remove a workload binding addressed by query string
    /// (`DELETE /v1/admin/workload-bindings?issuer=&subject=`).
    ///
    /// # Errors
    /// A missing binding surfaces as [`WyrdError::AdminNotFound`].
    pub async fn delete_workload_binding(
        &self,
        issuer: &str,
        subject: &str,
    ) -> Result<(), WyrdError> {
        let mut path = format!("{WORKLOAD_BINDINGS_PATH}?issuer=");
        path.push_str(&encode(issuer));
        path.push_str("&subject=");
        path.push_str(&encode(subject));
        self.client
            .request_json::<serde_json::Value, serde_json::Value>(Method::DELETE, &path, None)
            .await?;
        Ok(())
    }
}

/// Percent-encode a query value (issuer URLs carry `:` and `/`).
fn encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::auth::{
        ClaimMappingPayload, ClientAuthKind, CreateTrustedIssuerRequest,
        CreateWorkloadBindingRequest, IssuerUrl, PrincipalKindPayload,
    };
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;

    use super::AdminClient;
    use crate::client::WyrdClient;
    use crate::config::ClientConfig;

    const SECRET: &str = "super-secret";

    /// Build an admin client pointed at `base_url`, authenticating with a static
    /// bearer so no `/auth/token` exchange happens (the credential is passed
    /// through as-is). The env lock serializes the `WYRD_ACCESS_TOKEN` write.
    fn admin_client(base_url: &str) -> AdminClient {
        let _env = crate::ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: ENV_MUTEX (held here) serializes env mutation in this binary.
        // Hold it across `with_config` so the bearer credential resolves before
        // the env is cleared.
        unsafe {
            std::env::set_var("WYRD_ACCESS_TOKEN", "admin-bearer");
        }
        let mut config = ClientConfig::from_env();
        config.http.base_url = base_url.to_owned();
        let client = WyrdClient::with_config(config).expect("client assembles");
        // SAFETY: ENV_MUTEX (held here) serializes env mutation in this binary.
        unsafe {
            std::env::remove_var("WYRD_ACCESS_TOKEN");
        }
        AdminClient::new(client)
    }

    fn create_issuer_request() -> CreateTrustedIssuerRequest {
        CreateTrustedIssuerRequest {
            issuer: IssuerUrl::new("https://idp.example.com").expect("issuer"),
            expected_audience: "wyrd-api".to_owned(),
            client_id: "wyrd-client".to_owned(),
            client_auth: ClientAuthKind::SecretPost,
            client_secret: Some(SECRET.to_owned()),
            claim_mapping: ClaimMappingPayload {
                subject: "sub".to_owned(),
                email: None,
                groups: None,
            },
            group_role_map: HashMap::new(),
            default_roles: Vec::new(),
            principal_kind: PrincipalKindPayload::Workload,
            jwks_ttl_secs: None,
        }
    }

    fn issuer_view_json() -> serde_json::Value {
        serde_json::json!({
            "issuer": "https://idp.example.com",
            "jwks_uri": "https://idp.example.com/jwks",
            "expected_audience": "wyrd-api",
            "client_id": "wyrd-client",
            "client_auth": "SecretPost",
            "principal_kind": "Workload",
            "jwks_ttl_secs": 3600,
            "claim_mapping": { "subject": "sub" },
            "group_role_map": {},
            "default_roles": [],
        })
    }

    #[tokio::test]
    async fn create_sends_secret_in_body_and_redacted_view_has_none() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/admin/trusted-issuers"))
            .respond_with(ResponseTemplate::new(200).set_body_json(issuer_view_json()))
            .mount(&server)
            .await;

        let client = admin_client(&server.uri());
        let request = create_issuer_request();
        let view = client
            .create_trusted_issuer(&request)
            .await
            .expect("create succeeds");

        // The body the SDK sent carries the plaintext secret.
        let received = &server.received_requests().await.expect("requests")[0];
        let sent: serde_json::Value = serde_json::from_slice(&received.body).expect("json body");
        assert_eq!(sent["client_secret"], SECRET);

        // The redacted view round-tripped from the server never echoes a secret.
        let view_json = serde_json::to_value(&view).expect("view serializes");
        assert!(
            !view_json.to_string().contains(SECRET),
            "redacted view must not carry the plaintext secret"
        );
    }

    #[tokio::test]
    async fn list_deserializes_a_vec() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/admin/trusted-issuers"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([issuer_view_json()])),
            )
            .mount(&server)
            .await;

        let client = admin_client(&server.uri());
        let views = client.list_trusted_issuers().await.expect("list succeeds");
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].issuer, "https://idp.example.com");
    }

    #[tokio::test]
    async fn delete_sends_query_params_and_tolerates_204() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/v1/admin/trusted-issuers"))
            .and(query_param("issuer", "https://idp.example.com"))
            .and(query_param("cascade", "true"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = admin_client(&server.uri());
        client
            .delete_trusted_issuer("https://idp.example.com", true)
            .await
            .expect("delete succeeds against a 204 no-content route");
    }

    #[tokio::test]
    async fn conflict_surfaces_admin_conflict_code() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/admin/trusted-issuers"))
            .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
                "code": "WYRD_AUTH_409_ADMIN_CONFLICT",
                "title": "admin conflict",
                "status": 409,
                "detail": "admin mutation conflicted",
                "details": { "constraint": "auth_trusted_issuers_pkey" },
            })))
            .mount(&server)
            .await;

        let client = admin_client(&server.uri());
        let error = client
            .create_trusted_issuer(&create_issuer_request())
            .await
            .expect_err("a 409 must surface");
        assert!(
            matches!(error, WyrdError::AdminConflict { .. }),
            "expected AdminConflict, got {error:?}"
        );
    }

    #[tokio::test]
    async fn not_found_surfaces_admin_not_found_code() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/v1/admin/trusted-issuers"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "code": "WYRD_AUTH_404_ADMIN_NOT_FOUND",
                "title": "admin not found",
                "status": 404,
                "detail": "trusted issuer not found",
                "details": { "issuer": "https://idp.example.com" },
            })))
            .mount(&server)
            .await;

        let client = admin_client(&server.uri());
        let error = client
            .delete_trusted_issuer("https://idp.example.com", false)
            .await
            .expect_err("a 404 must surface");
        assert!(
            matches!(error, WyrdError::AdminNotFound { .. }),
            "expected AdminNotFound, got {error:?}"
        );
    }

    #[tokio::test]
    async fn workload_binding_create_and_delete_round_trip() {
        let server = MockServer::start().await;
        let card_ref = CardRef {
            kind: CardKind::Service,
            name: CardName::new("my-model").expect("name"),
            version: VersionBlock::parse("1.0.0").expect("version"),
            space: SpaceName::new("prod").expect("space"),
            uid: None,
        };
        let binding_view = serde_json::json!({
            "issuer": "https://idp.example.com",
            "subject": "system:serviceaccount:default/sa",
            "audience": serde_json::Value::Null,
            "card_ref": serde_json::to_value(&card_ref).expect("card_ref"),
        });
        Mock::given(method("POST"))
            .and(path("/v1/admin/workload-bindings"))
            .respond_with(ResponseTemplate::new(200).set_body_json(binding_view))
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/v1/admin/workload-bindings"))
            .and(query_param("issuer", "https://idp.example.com"))
            .and(query_param("subject", "system:serviceaccount:default/sa"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = admin_client(&server.uri());
        let request = CreateWorkloadBindingRequest {
            issuer: IssuerUrl::new("https://idp.example.com").expect("issuer"),
            subject: "system:serviceaccount:default/sa".to_owned(),
            audience: None,
            card_ref,
        };
        let created = client
            .create_workload_binding(&request)
            .await
            .expect("binding create succeeds");
        assert_eq!(created.subject, "system:serviceaccount:default/sa");

        client
            .delete_workload_binding(
                "https://idp.example.com",
                "system:serviceaccount:default/sa",
            )
            .await
            .expect("binding delete succeeds");
    }
}
